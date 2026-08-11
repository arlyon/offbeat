mod migrations;

use anyhow::{Result, bail};
use rusqlite::{Connection, OptionalExtension, params};
use std::path::Path;
use std::sync::Mutex;

use crate::types::{
    ChatMessage, ChatTrust, Festival, FestivalRegistryCache, FestivalStatus, SignedUpdate, Stage,
    VerifiedFestivalUpdate,
};

const MAX_REMOTE_LAMPORT_ADVANCE: i64 = 1_000_000;
const MAX_CACHED_FESTIVALS: usize = 2_000;
const MAX_CACHED_STAGES: usize = 100_000;
const MAX_CACHED_STAGES_PER_FESTIVAL: usize = 500;
const MAX_CACHED_REGISTRY_STORAGE_BYTES: i64 = 4 * 1024 * 1024;
const MAX_CACHE_TEXT_BYTES: usize = 1_024;
const REQUEST_TOKEN_ROLLOVER_FLOOR: &str = "90000000000000000000";
pub const EQUIVOCATED_HEAD_ID: &str = "__offbeat/equivocated__";
const ATTESTATION_GRACE_SECONDS: i64 = 7 * 24 * 60 * 60;
const MAX_UNVERIFIED_MESSAGES_PER_WRITER_TOPIC: i64 = 200;
const MAX_UNVERIFIED_MESSAGES_PER_TOPIC: i64 = 2_000;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredChatAuthorProof {
    pub writer_key: Vec<u8>,
    pub attestation_message: String,
    pub attestation_signature: Vec<u8>,
    pub issuer: Vec<u8>,
}

#[derive(Debug, Clone)]
pub struct PendingPublicChat {
    pub message_id: String,
    pub message: ChatMessage,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FestivalCheckIn {
    pub festival_id: String,
    pub kind: String,
    pub value: Option<String>,
    pub checked_at: i64,
    pub expires_at: i64,
    pub revision: i64,
}

struct ExistingSignedChat {
    user_id: String,
    display_name: String,
    text: String,
    topic: String,
    stage_id: Option<String>,
    timestamp: String,
    writer_seq: i64,
    logical_time: i64,
    writer_key: Vec<u8>,
    signature: Vec<u8>,
}

impl ExistingSignedChat {
    fn matches(&self, message: &ChatMessage, writer_seq: i64, logical_time: i64) -> bool {
        self.user_id == message.user_id
            && self.display_name == message.display_name
            && self.text == message.text
            && self.topic == message.topic
            && self.stage_id == message.stage_id
            && self.timestamp == message.timestamp
            && self.writer_seq == writer_seq
            && self.logical_time == logical_time
            && self.writer_key == message.writer_key
            && self.signature == message.signature
    }
}

fn chat_trust_value(trust: ChatTrust) -> i64 {
    match trust {
        ChatTrust::Unverified => 0,
        ChatTrust::Verified => 1,
        ChatTrust::VerifiedGrace => 2,
    }
}

fn chat_trust_from_value(value: i64) -> rusqlite::Result<ChatTrust> {
    match value {
        0 => Ok(ChatTrust::Unverified),
        1 => Ok(ChatTrust::Verified),
        2 => Ok(ChatTrust::VerifiedGrace),
        _ => Err(rusqlite::Error::IntegralValueOutOfRange(11, value)),
    }
}

fn encode_hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn current_unix_seconds() -> Result<i64> {
    Ok(i64::try_from(
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_secs(),
    )?)
}

fn parse_chat_attestation(message: &str) -> Result<(&str, i64, i64)> {
    let parts: Vec<&str> = message.split(':').collect();
    if parts.len() != 5 || parts[0] != "attestation" || parts[1] != "v1" {
        anyhow::bail!("invalid chat attestation format");
    }
    let issued_at = parts[3].parse::<i64>()?;
    let expires_at = parts[4].parse::<i64>()?;
    if issued_at < 0 || expires_at <= issued_at {
        anyhow::bail!("invalid chat attestation lifetime");
    }
    Ok((parts[2], issued_at, expires_at))
}

pub fn chat_head_commitment(message_id: &str, logical_time: u64) -> String {
    format!("{message_id}@{logical_time}")
}

fn committed_logical_time(commitment: &str) -> Option<i64> {
    commitment
        .rsplit_once('@')?
        .1
        .parse::<u64>()
        .ok()
        .and_then(|value| i64::try_from(value).ok())
}

/// Thread-safe SQLite database wrapper.
pub struct Database {
    conn: Mutex<Connection>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PendingGroupUpdate {
    pub id: i64,
    pub festival_id: String,
    pub group_id: String,
    pub envelope: Vec<u8>,
}

struct CachedFestivalRow {
    id: String,
    name: String,
    year: u32,
    location: String,
    city: String,
    country: String,
    start_date: String,
    end_date: String,
    genres_json: String,
    status: String,
    clashfinder_id: Option<String>,
    public_key: String,
    updated_at: String,
    lat: Option<f64>,
    lon: Option<f64>,
}

// SAFETY: `Connection` is `Send` (rusqlite documents this), and we guard
// all access with a `Mutex`, so `Database` can be shared across threads.
unsafe impl Sync for Database {}

fn festival_status_text(status: &FestivalStatus) -> &'static str {
    match status {
        FestivalStatus::Upcoming => "upcoming",
        FestivalStatus::Live => "live",
        FestivalStatus::Past => "past",
    }
}

fn parse_festival_status(status: &str) -> Result<FestivalStatus> {
    match status {
        "upcoming" => Ok(FestivalStatus::Upcoming),
        "live" => Ok(FestivalStatus::Live),
        "past" => Ok(FestivalStatus::Past),
        value => bail!("invalid cached festival status: {value}"),
    }
}

fn validate_cache_text(value: &str, field: &str) -> Result<()> {
    if value.len() > MAX_CACHE_TEXT_BYTES {
        bail!("cached festival {field} is too long");
    }
    Ok(())
}

fn is_valid_request_token(request_token: &str) -> bool {
    request_token.len() == 20
        && request_token.bytes().all(|byte| byte.is_ascii_digit())
        && request_token != "99999999999999999999"
}

fn validate_festival_registry(
    festivals: &[Festival],
    fetched_at: &str,
    request_token: &str,
) -> Result<()> {
    if fetched_at.is_empty() {
        bail!("cached festival fetch timestamp is empty");
    }
    validate_cache_text(fetched_at, "fetch timestamp")?;
    if !is_valid_request_token(request_token) {
        bail!("cached festival request token is invalid");
    }
    if festivals.len() > MAX_CACHED_FESTIVALS {
        bail!("festival registry exceeds {MAX_CACHED_FESTIVALS} entries");
    }
    let mut stage_count = 0usize;
    let mut storage_bytes = fetched_at.len().saturating_add(request_token.len());
    for festival in festivals {
        if festival.id.is_empty() || festival.name.is_empty() {
            bail!("cached festival id and name are required");
        }
        for (field, value) in [
            ("id", festival.id.as_str()),
            ("name", festival.name.as_str()),
            ("location", festival.location.as_str()),
            ("city", festival.city.as_str()),
            ("country", festival.country.as_str()),
            ("start date", festival.start_date.as_str()),
            ("end date", festival.end_date.as_str()),
            ("public key", festival.public_key.as_str()),
            ("updated timestamp", festival.updated_at.as_str()),
        ] {
            validate_cache_text(value, field)?;
            storage_bytes = storage_bytes.saturating_add(value.len());
        }
        if let Some(clashfinder_id) = &festival.clashfinder_id {
            validate_cache_text(clashfinder_id, "Clashfinder id")?;
            storage_bytes = storage_bytes.saturating_add(clashfinder_id.len());
        }
        if festival.genres.len() > 100 {
            bail!("festival {} has too many genres", festival.id);
        }
        for genre in &festival.genres {
            validate_cache_text(genre, "genre")?;
            storage_bytes = storage_bytes.saturating_add(genre.len().saturating_add(3));
        }
        storage_bytes = storage_bytes.saturating_add(32);
        if festival.stages.len() > MAX_CACHED_STAGES_PER_FESTIVAL {
            bail!(
                "festival {} exceeds {MAX_CACHED_STAGES_PER_FESTIVAL} cached stages",
                festival.id
            );
        }
        stage_count = stage_count.saturating_add(festival.stages.len());
        if stage_count > MAX_CACHED_STAGES {
            bail!("festival registry exceeds {MAX_CACHED_STAGES} total stages");
        }
        if festival.lat.is_some_and(|value| !value.is_finite())
            || festival.lon.is_some_and(|value| !value.is_finite())
        {
            bail!("festival {} has non-finite coordinates", festival.id);
        }
        for stage in &festival.stages {
            if stage.id.is_empty() || stage.name.is_empty() {
                bail!("cached stage id and name are required");
            }
            storage_bytes = storage_bytes.saturating_add(festival.id.len().saturating_add(8));
            for (field, value) in [
                ("stage id", stage.id.as_str()),
                ("stage name", stage.name.as_str()),
                ("stage short name", stage.short.as_str()),
                ("stage color", stage.color.as_str()),
            ] {
                validate_cache_text(value, field)?;
                storage_bytes = storage_bytes.saturating_add(value.len());
            }
        }
        if storage_bytes > MAX_CACHED_REGISTRY_STORAGE_BYTES as usize {
            bail!("festival registry exceeds local storage safety bounds");
        }
    }
    Ok(())
}

fn sqlite_counter(value: u64, name: &str) -> Result<i64> {
    i64::try_from(value).map_err(|_| anyhow::anyhow!("chat {name} exceeds SQLite range"))
}

fn effective_logical_time(message: &ChatMessage) -> Result<i64> {
    let value = if message.logical_time == 0 {
        message.writer_seq
    } else {
        message.logical_time
    };
    sqlite_counter(value, "Lamport time")
}

fn chat_trust_for_writer(tx: &rusqlite::Transaction<'_>, writer_id: &str) -> Result<ChatTrust> {
    let expires_at: Option<i64> = tx
        .query_row(
            "SELECT expires_at FROM chat_author_proofs WHERE writer_id = ?1",
            [writer_id],
            |row| row.get(0),
        )
        .optional()?;
    let Some(expires_at) = expires_at else {
        return Ok(ChatTrust::Unverified);
    };
    let now = current_unix_seconds()?;
    if now <= expires_at {
        Ok(ChatTrust::Verified)
    } else if now <= expires_at.saturating_add(ATTESTATION_GRACE_SECONDS) {
        Ok(ChatTrust::VerifiedGrace)
    } else {
        Ok(ChatTrust::Unverified)
    }
}

fn insert_chat_message(
    tx: &rusqlite::Transaction<'_>,
    message: &ChatMessage,
    trust: ChatTrust,
) -> Result<usize> {
    let writer_seq = sqlite_counter(message.writer_seq, "writer sequence")?;
    let logical_time = effective_logical_time(message)?;
    Ok(tx.execute(
        "INSERT OR IGNORE INTO chat_messages
         (id, topic, user_id, display_name, text, stage_id, timestamp,
          writer_seq, logical_time, writer_key, signature, writer_id, trust_state, received_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, datetime('now'))",
        params![
            message.id,
            message.topic,
            message.user_id,
            message.display_name,
            message.text,
            message.stage_id,
            message.timestamp,
            writer_seq,
            logical_time,
            message.writer_key,
            message.signature,
            message.writer_id(),
            chat_trust_value(trust),
        ],
    )?)
}

fn reconcile_legacy_chat_order(
    tx: &rusqlite::Transaction<'_>,
    message: &ChatMessage,
    logical_time: i64,
) -> Result<usize> {
    if message.logical_time == 0 {
        return Ok(0);
    }
    Ok(tx.execute(
        "UPDATE chat_messages SET logical_time = ?2
         WHERE id = ?1 AND logical_time = writer_seq AND logical_time <> ?2
           AND topic = ?3 AND user_id = ?4 AND display_name = ?5
           AND stage_id IS ?6 AND timestamp = ?7 AND writer_seq = ?8 AND text = ?9",
        params![
            message.id,
            logical_time,
            message.topic,
            message.user_id,
            message.display_name,
            message.stage_id,
            message.timestamp,
            sqlite_counter(message.writer_seq, "writer sequence")?,
            message.text,
        ],
    )?)
}

fn row_counter(row: &rusqlite::Row<'_>, index: usize, name: &str) -> rusqlite::Result<u64> {
    let value: i64 = row.get(index)?;
    u64::try_from(value).map_err(|_| {
        rusqlite::Error::FromSqlConversionFailure(
            index,
            rusqlite::types::Type::Integer,
            Box::new(std::io::Error::other(format!("negative chat {name}"))),
        )
    })
}

fn chat_message_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<ChatMessage> {
    Ok(ChatMessage {
        id: row.get(0)?,
        user_id: row.get(1)?,
        display_name: row.get(2)?,
        text: row.get(3)?,
        topic: row.get(4)?,
        stage_id: row.get(5)?,
        timestamp: row.get(6)?,
        writer_seq: row_counter(row, 7, "writer sequence")?,
        logical_time: row_counter(row, 8, "Lamport time")?,
        writer_key: row.get(9)?,
        signature: row.get(10)?,
        trust: chat_trust_from_value(row.get(11)?)?,
    })
}

impl Database {
    /// Open (or create) a database at the given path and run migrations.
    pub fn new(path: &Path) -> Result<Self> {
        let conn = Connection::open(path)?;
        Self::apply_pragmas(&conn)?;
        migrations::apply_migrations(&conn)?;
        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    /// Create an in-memory database (for tests).
    pub fn new_in_memory() -> Result<Self> {
        let conn = Connection::open_in_memory()?;
        Self::apply_pragmas(&conn)?;
        migrations::apply_migrations(&conn)?;
        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    fn apply_pragmas(conn: &Connection) -> Result<()> {
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.pragma_update(None, "synchronous", "NORMAL")?;
        conn.pragma_update(None, "busy_timeout", "5000")?;
        Ok(())
    }

    // --- docs ---

    pub fn save_doc(&self, id: &str, doc_type: &str, data: &[u8]) -> Result<()> {
        self.conn.lock().unwrap().execute(
            "INSERT OR REPLACE INTO docs (id, doc_type, data, updated_at)
             VALUES (?1, ?2, ?3, datetime('now'))",
            params![id, doc_type, data],
        )?;
        Ok(())
    }

    pub fn load_doc(&self, id: &str) -> Result<Option<Vec<u8>>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare("SELECT data FROM docs WHERE id = ?1")?;
        let mut rows = stmt.query(params![id])?;
        if let Some(row) = rows.next()? {
            Ok(Some(row.get(0)?))
        } else {
            Ok(None)
        }
    }

    /// Delete a document snapshot and all of its incremental updates.
    pub fn delete_doc(&self, doc_id: &str) -> Result<()> {
        let mut conn = self.conn.lock().unwrap();
        let tx = conn.transaction()?;
        tx.execute("DELETE FROM doc_updates WHERE doc_id = ?1", params![doc_id])?;
        tx.execute("DELETE FROM docs WHERE id = ?1", params![doc_id])?;
        tx.commit()?;
        Ok(())
    }

    /// Returns the IDs of all docs of the given type.
    pub fn list_docs(&self, doc_type: &str) -> Result<Vec<String>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt =
            conn.prepare("SELECT id FROM docs WHERE doc_type = ?1 ORDER BY updated_at DESC")?;
        let ids = stmt
            .query_map(params![doc_type], |row| row.get(0))?
            .collect::<rusqlite::Result<Vec<String>>>()?;
        Ok(ids)
    }

    // --- doc updates (append-only CRDT persistence) ---

    /// Append a single CRDT update for a doc.
    pub fn append_doc_update(&self, doc_id: &str, update_data: &[u8]) -> Result<()> {
        self.conn.lock().unwrap().execute(
            "INSERT INTO doc_updates (doc_id, update_data) VALUES (?1, ?2)",
            params![doc_id, update_data],
        )?;
        Ok(())
    }

    /// Load all update blobs for a doc, ordered by insertion.
    pub fn load_doc_updates(&self, doc_id: &str) -> Result<Vec<Vec<u8>>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt =
            conn.prepare("SELECT update_data FROM doc_updates WHERE doc_id = ?1 ORDER BY id")?;
        let updates = stmt
            .query_map(params![doc_id], |row| row.get(0))?
            .collect::<rusqlite::Result<Vec<Vec<u8>>>>()?;
        Ok(updates)
    }

    /// Count updates for a doc.
    pub fn count_doc_updates(&self, doc_id: &str) -> Result<u32> {
        let conn = self.conn.lock().unwrap();
        let count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM doc_updates WHERE doc_id = ?1",
            params![doc_id],
            |row| row.get(0),
        )?;
        Ok(count as u32)
    }

    /// Replace all updates for a doc with a single compacted blob.
    pub fn compact_doc_updates(&self, doc_id: &str, compacted: &[u8]) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute("DELETE FROM doc_updates WHERE doc_id = ?1", params![doc_id])?;
        conn.execute(
            "INSERT INTO doc_updates (doc_id, update_data) VALUES (?1, ?2)",
            params![doc_id, compacted],
        )?;
        // Also update the docs table for fast boot
        conn.execute(
            "INSERT OR REPLACE INTO docs (id, doc_type, data, updated_at)
             VALUES (?1, 'yrs', ?2, datetime('now'))",
            params![doc_id, compacted],
        )?;
        Ok(())
    }

    pub fn save_verified_festival_update(&self, update: &VerifiedFestivalUpdate) -> Result<()> {
        self.conn.lock().unwrap().execute(
            "INSERT OR IGNORE INTO verified_festival_updates
             (doc_id, authority_seq, kind, update_data, author, signature)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                update.doc_id,
                i64::try_from(update.authority_seq)?,
                update.kind,
                update.signed_update.update,
                update.signed_update.author,
                update.signed_update.signature,
            ],
        )?;
        Ok(())
    }

    pub fn highest_verified_festival_seq(&self, doc_id: &str) -> Result<u64> {
        let seq: i64 = self.conn.lock().unwrap().query_row(
            "SELECT COALESCE(MAX(authority_seq), 0)
             FROM verified_festival_updates WHERE doc_id = ?1",
            [doc_id],
            |row| row.get(0),
        )?;
        Ok(u64::try_from(seq)?)
    }

    pub fn load_latest_festival_checkpoint(
        &self,
        doc_id: &str,
        checkpoint_kind: i32,
    ) -> Result<Option<VerifiedFestivalUpdate>> {
        self.conn
            .lock()
            .unwrap()
            .query_row(
                "SELECT authority_seq, kind, update_data, author, signature
                 FROM verified_festival_updates
                 WHERE doc_id = ?1 AND kind = ?2
                 ORDER BY authority_seq DESC LIMIT 1",
                params![doc_id, checkpoint_kind],
                |row| {
                    let authority_seq: i64 = row.get(0)?;
                    Ok(VerifiedFestivalUpdate {
                        doc_id: doc_id.to_string(),
                        authority_seq: authority_seq as u64,
                        kind: row.get(1)?,
                        signed_update: SignedUpdate {
                            update: row.get(2)?,
                            author: row.get(3)?,
                            signature: row.get(4)?,
                        },
                    })
                },
            )
            .optional()
            .map_err(Into::into)
    }

    // --- groups ---

    pub fn save_group(&self, id: &str, festival_id: &str, name: &str, key: &[u8]) -> Result<()> {
        self.conn.lock().unwrap().execute(
            "INSERT INTO groups (id, festival_id, name, key, created_at)
             VALUES (?1, ?2, ?3, ?4, datetime('now'))
             ON CONFLICT(id) DO UPDATE SET
                 festival_id = CASE
                     WHEN excluded.festival_id = '' THEN groups.festival_id
                     ELSE excluded.festival_id
                 END,
                 name = CASE WHEN excluded.name = '' THEN groups.name ELSE excluded.name END,
                 key = excluded.key",
            params![id, festival_id, name, key],
        )?;
        Ok(())
    }

    /// Returns (id, name, key) tuples for all groups of the given festival.
    pub fn load_groups(&self, festival_id: &str) -> Result<Vec<(String, String, Vec<u8>)>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare("SELECT id, name, key FROM groups WHERE festival_id = ?1")?;
        let groups = stmt
            .query_map(params![festival_id], |row| {
                Ok((row.get(0)?, row.get(1)?, row.get(2)?))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(groups)
    }

    pub fn save_festival_checkin(&self, checkin: &FestivalCheckIn) -> Result<()> {
        self.conn.lock().unwrap().execute(
            "INSERT INTO festival_checkins
             (festival_id, kind, value, checked_at, expires_at, revision)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)
             ON CONFLICT(festival_id) DO UPDATE SET
                 kind = excluded.kind,
                 value = excluded.value,
                 checked_at = excluded.checked_at,
                 expires_at = excluded.expires_at,
                 revision = excluded.revision",
            params![
                checkin.festival_id,
                checkin.kind,
                checkin.value,
                checkin.checked_at,
                checkin.expires_at,
                checkin.revision,
            ],
        )?;
        Ok(())
    }

    pub fn load_festival_checkin(&self, festival_id: &str) -> Result<Option<FestivalCheckIn>> {
        self.conn
            .lock()
            .unwrap()
            .query_row(
                "SELECT kind, value, checked_at, expires_at, revision
                 FROM festival_checkins WHERE festival_id = ?1",
                params![festival_id],
                |row| {
                    Ok(FestivalCheckIn {
                        festival_id: festival_id.to_string(),
                        kind: row.get(0)?,
                        value: row.get(1)?,
                        checked_at: row.get(2)?,
                        expires_at: row.get(3)?,
                        revision: row.get(4)?,
                    })
                },
            )
            .optional()
            .map_err(Into::into)
    }

    pub fn clear_festival_checkin(&self, festival_id: &str) -> Result<()> {
        self.conn.lock().unwrap().execute(
            "DELETE FROM festival_checkins WHERE festival_id = ?1",
            params![festival_id],
        )?;
        Ok(())
    }

    pub fn load_group_festival_id(&self, group_id: &str) -> Result<Option<String>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare("SELECT festival_id FROM groups WHERE id = ?1")?;
        let mut rows = stmt.query(params![group_id])?;
        Ok(rows.next()?.map(|row| row.get(0)).transpose()?)
    }

    pub fn load_all_group_keys(&self) -> Result<Vec<(String, [u8; 32])>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare("SELECT id, key FROM groups")?;
        let rows = stmt
            .query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, Vec<u8>>(1)?))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        rows.into_iter()
            .map(|(id, key)| {
                let key = key
                    .try_into()
                    .map_err(|_| anyhow::anyhow!("group {id} key must be 32 bytes"))?;
                Ok((id, key))
            })
            .collect()
    }

    pub fn enqueue_group_update(
        &self,
        festival_id: &str,
        group_id: &str,
        envelope: &[u8],
    ) -> Result<i64> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO pending_group_updates (festival_id, group_id, envelope)
             VALUES (?1, ?2, ?3)",
            params![festival_id, group_id, envelope],
        )?;
        Ok(conn.last_insert_rowid())
    }

    pub fn load_pending_group_updates(&self, festival_id: &str) -> Result<Vec<PendingGroupUpdate>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, festival_id, group_id, envelope
             FROM pending_group_updates
             WHERE festival_id = ?1
             ORDER BY id",
        )?;
        Ok(stmt
            .query_map(params![festival_id], |row| {
                Ok(PendingGroupUpdate {
                    id: row.get(0)?,
                    festival_id: row.get(1)?,
                    group_id: row.get(2)?,
                    envelope: row.get(3)?,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?)
    }

    pub fn pending_group_update_exists(&self, id: i64) -> Result<bool> {
        let count: i64 = self.conn.lock().unwrap().query_row(
            "SELECT COUNT(*) FROM pending_group_updates WHERE id = ?1",
            params![id],
            |row| row.get(0),
        )?;
        Ok(count > 0)
    }

    pub fn delete_pending_group_update(&self, id: i64) -> Result<()> {
        self.conn.lock().unwrap().execute(
            "DELETE FROM pending_group_updates WHERE id = ?1",
            params![id],
        )?;
        Ok(())
    }

    /// Replace all older outbound group deltas with one leave tombstone while
    /// atomically purging local private state and key material.
    pub fn finalize_group_leave(
        &self,
        festival_id: &str,
        group_id: &str,
        leave_envelope: &[u8],
    ) -> Result<i64> {
        let mut conn = self.conn.lock().unwrap();
        let tx = conn.transaction()?;
        tx.execute(
            "DELETE FROM pending_group_updates WHERE group_id = ?1",
            params![group_id],
        )?;
        tx.execute(
            "INSERT INTO pending_group_updates (festival_id, group_id, envelope)
             VALUES (?1, ?2, ?3)",
            params![festival_id, group_id, leave_envelope],
        )?;
        let pending_id = tx.last_insert_rowid();
        let doc_id = format!("group/{group_id}/state");
        let chat_topic = format!("group/{group_id}/chat");
        tx.execute("DELETE FROM doc_updates WHERE doc_id = ?1", params![doc_id])?;
        tx.execute("DELETE FROM docs WHERE id = ?1", params![doc_id])?;
        tx.execute(
            "DELETE FROM chat_messages WHERE topic = ?1",
            params![chat_topic],
        )?;
        tx.execute(
            "DELETE FROM chat_topic_clocks WHERE topic = ?1",
            params![chat_topic],
        )?;
        tx.execute(
            "DELETE FROM chat_writer_sequences WHERE topic = ?1",
            params![chat_topic],
        )?;
        tx.execute(
            "DELETE FROM chat_sequence_conflicts WHERE topic = ?1",
            params![chat_topic],
        )?;
        tx.execute("DELETE FROM groups WHERE id = ?1", params![group_id])?;
        tx.commit()?;
        Ok(pending_id)
    }

    pub fn update_group_name(&self, id: &str, name: &str) -> Result<()> {
        self.conn.lock().unwrap().execute(
            "UPDATE groups SET name = ?2 WHERE id = ?1",
            params![id, name],
        )?;
        Ok(())
    }

    pub fn delete_group(&self, id: &str) -> Result<()> {
        self.conn
            .lock()
            .unwrap()
            .execute("DELETE FROM groups WHERE id = ?1", params![id])?;
        Ok(())
    }

    // --- starred sets ---

    /// Toggle a star on a set. Returns the new starred state (true = now starred).
    pub fn toggle_star(&self, festival_id: &str, set_id: &str) -> Result<bool> {
        let conn = self.conn.lock().unwrap();
        let exists: bool = conn.query_row(
            "SELECT COUNT(*) FROM starred_sets WHERE festival_id = ?1 AND set_id = ?2",
            params![festival_id, set_id],
            |row| row.get::<_, i64>(0),
        )? > 0;

        if exists {
            conn.execute(
                "DELETE FROM starred_sets WHERE festival_id = ?1 AND set_id = ?2",
                params![festival_id, set_id],
            )?;
            Ok(false)
        } else {
            conn.execute(
                "INSERT INTO starred_sets (festival_id, set_id, starred_at)
                 VALUES (?1, ?2, datetime('now'))",
                params![festival_id, set_id],
            )?;
            Ok(true)
        }
    }

    /// Returns the set IDs that are starred for a festival.
    pub fn get_stars(&self, festival_id: &str) -> Result<Vec<String>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare("SELECT set_id FROM starred_sets WHERE festival_id = ?1")?;
        let ids = stmt
            .query_map(params![festival_id], |row| row.get(0))?
            .collect::<rusqlite::Result<Vec<String>>>()?;
        Ok(ids)
    }

    // --- server-authoritative festival registry cache ---

    /// Atomically replace the complete server-authoritative festival registry cache.
    pub fn replace_festival_registry_cache(
        &self,
        festivals: &[Festival],
        fetched_at: &str,
        request_token: &str,
    ) -> Result<bool> {
        validate_festival_registry(festivals, fetched_at, request_token)?;
        let mut conn = self.conn.lock().unwrap();
        let tx = conn.transaction()?;
        let current_request_token = tx
            .query_row(
                "SELECT request_token FROM festival_registry_meta WHERE singleton = 1",
                [],
                |row| row.get::<_, String>(0),
            )
            .optional()?;
        if current_request_token.as_deref().is_some_and(|current| {
            is_valid_request_token(current)
                && current < REQUEST_TOKEN_ROLLOVER_FLOOR
                && current >= request_token
        }) {
            return Ok(false);
        }
        tx.execute("DELETE FROM cached_festival_stages", [])?;
        tx.execute("DELETE FROM cached_festivals", [])?;

        for festival in festivals {
            let genres_json = serde_json::to_string(&festival.genres)?;
            tx.execute(
                "INSERT INTO cached_festivals
                 (id, name, year, location, city, country, start_date, end_date,
                  genres_json, status, clashfinder_id, public_key, updated_at, lat, lon)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15)",
                params![
                    festival.id,
                    festival.name,
                    festival.year,
                    festival.location,
                    festival.city,
                    festival.country,
                    festival.start_date,
                    festival.end_date,
                    genres_json,
                    festival_status_text(&festival.status),
                    festival.clashfinder_id,
                    festival.public_key,
                    festival.updated_at,
                    festival.lat,
                    festival.lon,
                ],
            )?;
            for stage in &festival.stages {
                tx.execute(
                    "INSERT INTO cached_festival_stages
                     (festival_id, id, name, short, color, sort_order)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                    params![
                        festival.id,
                        stage.id,
                        stage.name,
                        stage.short,
                        stage.color,
                        stage.order,
                    ],
                )?;
            }
        }

        tx.execute(
            "INSERT INTO festival_registry_meta(singleton, fetched_at, request_token)
             VALUES (1, ?1, ?2)
             ON CONFLICT(singleton) DO UPDATE SET
                 fetched_at = excluded.fetched_at,
                 request_token = excluded.request_token",
            params![fetched_at, request_token],
        )?;
        tx.commit()?;
        Ok(true)
    }

    /// Load the complete cached registry, or `None` before the first successful fetch.
    pub fn load_festival_registry_cache(&self) -> Result<Option<FestivalRegistryCache>> {
        let conn = self.conn.lock().unwrap();
        let Some((meta_bytes, token_bytes)) = conn
            .query_row(
                "SELECT length(CAST(fetched_at AS BLOB)),
                        length(CAST(request_token AS BLOB))
                 FROM festival_registry_meta WHERE singleton = 1",
                [],
                |row| Ok((row.get::<_, i64>(0)?, row.get::<_, i64>(1)?)),
            )
            .optional()?
        else {
            return Ok(None);
        };
        if meta_bytes > MAX_CACHE_TEXT_BYTES as i64 || token_bytes != 20 {
            bail!("cached festival registry metadata exceeds local safety bounds");
        }

        let (festival_count, festival_bytes) = conn.query_row(
            "SELECT COUNT(*), COALESCE(SUM(
                length(CAST(id AS BLOB)) + length(CAST(name AS BLOB)) +
                length(CAST(location AS BLOB)) + length(CAST(city AS BLOB)) +
                length(CAST(country AS BLOB)) + length(CAST(start_date AS BLOB)) +
                length(CAST(end_date AS BLOB)) + length(CAST(genres_json AS BLOB)) +
                length(CAST(status AS BLOB)) +
                length(CAST(COALESCE(clashfinder_id, '') AS BLOB)) +
                length(CAST(public_key AS BLOB)) + length(CAST(updated_at AS BLOB)) + 32
             ), 0)
             FROM cached_festivals",
            [],
            |row| Ok((row.get::<_, i64>(0)?, row.get::<_, i64>(1)?)),
        )?;
        let (stage_count, stage_bytes) = conn.query_row(
            "SELECT COUNT(*), COALESCE(SUM(
                length(CAST(festival_id AS BLOB)) + length(CAST(id AS BLOB)) +
                length(CAST(name AS BLOB)) + length(CAST(short AS BLOB)) +
                length(CAST(color AS BLOB)) + 8
             ), 0)
             FROM cached_festival_stages",
            [],
            |row| Ok((row.get::<_, i64>(0)?, row.get::<_, i64>(1)?)),
        )?;
        let has_oversized_festival: bool = conn.query_row(
            "SELECT EXISTS(
                SELECT 1 FROM cached_festivals WHERE
                    length(CAST(id AS BLOB)) > ?1 OR length(CAST(name AS BLOB)) > ?1 OR
                    length(CAST(location AS BLOB)) > ?1 OR length(CAST(city AS BLOB)) > ?1 OR
                    length(CAST(country AS BLOB)) > ?1 OR
                    length(CAST(start_date AS BLOB)) > ?1 OR
                    length(CAST(end_date AS BLOB)) > ?1 OR
                    length(CAST(status AS BLOB)) > ?1 OR
                    length(CAST(COALESCE(clashfinder_id, '') AS BLOB)) > ?1 OR
                    length(CAST(public_key AS BLOB)) > ?1 OR
                    length(CAST(updated_at AS BLOB)) > ?1 OR
                    length(CAST(genres_json AS BLOB)) > 131072
            )",
            params![MAX_CACHE_TEXT_BYTES as i64],
            |row| row.get(0),
        )?;
        let has_oversized_stage: bool = conn.query_row(
            "SELECT EXISTS(
                SELECT 1 FROM cached_festival_stages WHERE
                    length(CAST(festival_id AS BLOB)) > ?1 OR
                    length(CAST(id AS BLOB)) > ?1 OR length(CAST(name AS BLOB)) > ?1 OR
                    length(CAST(short AS BLOB)) > ?1 OR length(CAST(color AS BLOB)) > ?1
            )",
            params![MAX_CACHE_TEXT_BYTES as i64],
            |row| row.get(0),
        )?;
        let has_too_many_stages_for_festival: bool = conn.query_row(
            "SELECT EXISTS(
                SELECT festival_id FROM cached_festival_stages
                GROUP BY festival_id HAVING COUNT(*) > ?1 LIMIT 1
            )",
            params![MAX_CACHED_STAGES_PER_FESTIVAL as i64],
            |row| row.get(0),
        )?;
        if festival_count > MAX_CACHED_FESTIVALS as i64
            || stage_count > MAX_CACHED_STAGES as i64
            || meta_bytes
                .saturating_add(token_bytes)
                .saturating_add(festival_bytes)
                .saturating_add(stage_bytes)
                > MAX_CACHED_REGISTRY_STORAGE_BYTES
            || has_oversized_festival
            || has_oversized_stage
            || has_too_many_stages_for_festival
        {
            bail!("cached festival registry exceeds local safety bounds");
        }
        let (fetched_at, request_token) = conn.query_row(
            "SELECT fetched_at, request_token
             FROM festival_registry_meta WHERE singleton = 1",
            [],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
        )?;

        let mut festival_stmt = conn.prepare(
            "SELECT id, name, year, location, city, country, start_date, end_date,
                    genres_json, status, clashfinder_id, public_key, updated_at, lat, lon
             FROM cached_festivals
             ORDER BY start_date, id",
        )?;
        let rows = festival_stmt
            .query_map([], |row| {
                Ok(CachedFestivalRow {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    year: row.get(2)?,
                    location: row.get(3)?,
                    city: row.get(4)?,
                    country: row.get(5)?,
                    start_date: row.get(6)?,
                    end_date: row.get(7)?,
                    genres_json: row.get(8)?,
                    status: row.get(9)?,
                    clashfinder_id: row.get(10)?,
                    public_key: row.get(11)?,
                    updated_at: row.get(12)?,
                    lat: row.get(13)?,
                    lon: row.get(14)?,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        drop(festival_stmt);

        let mut festivals = Vec::with_capacity(rows.len());
        for row in rows {
            let genres = serde_json::from_str(&row.genres_json)
                .map_err(|error| anyhow::anyhow!("invalid cached festival genres: {error}"))?;
            let mut stage_stmt = conn.prepare(
                "SELECT id, name, short, color, sort_order
                 FROM cached_festival_stages
                 WHERE festival_id = ?1
                 ORDER BY sort_order, id",
            )?;
            let stages = stage_stmt
                .query_map(params![row.id], |stage_row| {
                    Ok(Stage {
                        id: stage_row.get(0)?,
                        name: stage_row.get(1)?,
                        short: stage_row.get(2)?,
                        color: stage_row.get(3)?,
                        order: stage_row.get(4)?,
                    })
                })?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            festivals.push(Festival {
                id: row.id,
                name: row.name,
                year: row.year,
                location: row.location,
                city: row.city,
                country: row.country,
                start_date: row.start_date,
                end_date: row.end_date,
                stages,
                genres,
                status: parse_festival_status(&row.status)?,
                clashfinder_id: row.clashfinder_id,
                public_key: row.public_key,
                updated_at: row.updated_at,
                lat: row.lat,
                lon: row.lon,
            });
        }
        validate_festival_registry(&festivals, &fetched_at, &request_token)?;
        Ok(Some(FestivalRegistryCache {
            festivals,
            fetched_at,
            request_token,
        }))
    }

    // --- chat messages ---

    /// Allocate a writer sequence and per-topic Lamport time, then persist the
    /// local message in the same transaction.
    pub fn save_local_chat_message(&self, msg: ChatMessage) -> Result<ChatMessage> {
        self.save_local_chat_message_with(msg, |_| Ok(()))
    }

    pub fn save_local_signed_chat_message(
        &self,
        mut msg: ChatMessage,
        signing_key: &ed25519_dalek::SigningKey,
    ) -> Result<ChatMessage> {
        msg.writer_key = signing_key.verifying_key().to_bytes().to_vec();
        self.save_local_chat_message_with(msg, |message| {
            crate::signing::sign_public_chat_message(signing_key, message)
        })
    }

    fn save_local_chat_message_with(
        &self,
        mut msg: ChatMessage,
        finalize: impl FnOnce(&mut ChatMessage) -> Result<()>,
    ) -> Result<ChatMessage> {
        if msg.writer_seq != 0 || msg.logical_time != 0 {
            anyhow::bail!("local chat position must be allocated by the database");
        }

        let writer_id = msg.writer_id();
        let conn = self.conn.lock().unwrap();
        let tx = conn.unchecked_transaction()?;
        tx.execute(
            "INSERT OR IGNORE INTO chat_writer_sequences(topic, user_id, writer_seq)
             VALUES (?1, ?2, 0)",
            params![msg.topic, writer_id],
        )?;
        let current_writer_seq: i64 = tx.query_row(
            "SELECT writer_seq FROM chat_writer_sequences
             WHERE topic = ?1 AND user_id = ?2",
            params![msg.topic, writer_id],
            |row| row.get(0),
        )?;
        let next_writer_seq = current_writer_seq
            .checked_add(1)
            .ok_or_else(|| anyhow::anyhow!("chat writer sequence exhausted"))?;

        tx.execute(
            "INSERT OR IGNORE INTO chat_topic_clocks(topic, logical_time) VALUES (?1, 0)",
            params![msg.topic],
        )?;
        let current_logical_time: i64 = tx.query_row(
            "SELECT logical_time FROM chat_topic_clocks WHERE topic = ?1",
            params![msg.topic],
            |row| row.get(0),
        )?;
        let next_logical_time = current_logical_time
            .checked_add(1)
            .ok_or_else(|| anyhow::anyhow!("chat Lamport clock exhausted"))?;

        tx.execute(
            "UPDATE chat_writer_sequences SET writer_seq = ?3
             WHERE topic = ?1 AND user_id = ?2",
            params![msg.topic, writer_id, next_writer_seq],
        )?;
        tx.execute(
            "UPDATE chat_topic_clocks SET logical_time = ?2 WHERE topic = ?1",
            params![msg.topic, next_logical_time],
        )?;

        msg.writer_seq = next_writer_seq as u64;
        msg.logical_time = next_logical_time as u64;
        finalize(&mut msg)?;
        let trust = chat_trust_for_writer(&tx, &writer_id)?;
        msg.trust = trust;
        if insert_chat_message(&tx, &msg, trust)? != 1 {
            anyhow::bail!("chat message ID already exists");
        }
        if !msg.signature.is_empty() {
            tx.execute(
                "INSERT INTO pending_public_chat(message_id, topic, message_json)
                 VALUES (?1, ?2, ?3)",
                params![msg.id, msg.topic, serde_json::to_vec(&msg)?],
            )?;
        }
        tx.commit()?;
        Ok(msg)
    }

    /// Persist an incoming message and advance local sequence/Lamport floors.
    pub fn save_chat_message(&self, msg: &ChatMessage) -> Result<()> {
        self.save_chat_messages_batch(std::slice::from_ref(msg))
    }

    pub fn save_chat_messages_batch(&self, msgs: &[ChatMessage]) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        let tx = conn.unchecked_transaction()?;
        let mut initial_topic_clocks = std::collections::HashMap::new();
        let mut initial_writer_sequences = std::collections::HashMap::new();
        for msg in msgs {
            let logical_time = effective_logical_time(msg)?;
            let current_logical_time: i64 = tx
                .query_row(
                    "SELECT logical_time FROM chat_topic_clocks WHERE topic = ?1",
                    params![msg.topic],
                    |row| row.get(0),
                )
                .optional()?
                .unwrap_or(0);
            let initial_topic_clock = *initial_topic_clocks
                .entry(msg.topic.clone())
                .or_insert(current_logical_time);
            if logical_time > initial_topic_clock.saturating_add(MAX_REMOTE_LAMPORT_ADVANCE) {
                anyhow::bail!("remote chat Lamport clock exceeds accepted advance");
            }
            let writer_seq = sqlite_counter(msg.writer_seq, "writer sequence")?;
            let writer_id = msg.writer_id();
            let writer_key = (msg.topic.clone(), writer_id.clone());
            let initial_writer_sequence: i64 =
                *initial_writer_sequences.entry(writer_key).or_insert(
                    tx.query_row(
                        "SELECT writer_seq FROM chat_writer_sequences
                     WHERE topic = ?1 AND user_id = ?2",
                        params![msg.topic, writer_id],
                        |row| row.get(0),
                    )
                    .optional()?
                    .unwrap_or(0),
                );
            if writer_seq > initial_writer_sequence.saturating_add(MAX_REMOTE_LAMPORT_ADVANCE) {
                anyhow::bail!("remote chat writer sequence exceeds accepted advance");
            }
            if !msg.signature.is_empty() {
                let existing: Option<ExistingSignedChat> = tx
                    .query_row(
                        "SELECT user_id, display_name, text, topic, stage_id, timestamp,
                                writer_seq, logical_time, writer_key, signature
                         FROM chat_messages WHERE id = ?1",
                        [&msg.id],
                        |row| {
                            Ok(ExistingSignedChat {
                                user_id: row.get(0)?,
                                display_name: row.get(1)?,
                                text: row.get(2)?,
                                topic: row.get(3)?,
                                stage_id: row.get(4)?,
                                timestamp: row.get(5)?,
                                writer_seq: row.get(6)?,
                                logical_time: row.get(7)?,
                                writer_key: row.get(8)?,
                                signature: row.get(9)?,
                            })
                        },
                    )
                    .optional()?;
                if let Some(existing) = existing
                    && !existing.matches(msg, writer_seq, logical_time)
                {
                    anyhow::bail!("signed chat message ID collision");
                }
            }

            let has_conflict: bool = writer_seq > 0
                && tx.query_row(
                    "SELECT EXISTS(
                        SELECT 1 FROM chat_messages
                        WHERE topic = ?1 AND writer_id = ?2 AND writer_seq = ?3 AND id <> ?4
                    )",
                    params![msg.topic, writer_id, writer_seq, msg.id],
                    |row| row.get(0),
                )?;
            if has_conflict {
                tx.execute(
                    "INSERT OR IGNORE INTO chat_sequence_conflicts(topic, user_id, writer_seq)
                     VALUES (?1, ?2, ?3)",
                    params![msg.topic, writer_id, writer_seq],
                )?;
            }
            let trust = chat_trust_for_writer(&tx, &writer_id)?;
            if trust == ChatTrust::Unverified && msg.writer_key.len() == 32 {
                let already_stored: bool = tx.query_row(
                    "SELECT EXISTS(SELECT 1 FROM chat_messages WHERE id = ?1)",
                    [&msg.id],
                    |row| row.get(0),
                )?;
                if !already_stored {
                    let writer_count: i64 = tx.query_row(
                        "SELECT COUNT(*) FROM chat_messages
                         WHERE topic = ?1 AND writer_id = ?2 AND trust_state = 0",
                        params![msg.topic, writer_id],
                        |row| row.get(0),
                    )?;
                    let topic_count: i64 = tx.query_row(
                        "SELECT COUNT(*) FROM chat_messages WHERE topic = ?1 AND trust_state = 0",
                        [&msg.topic],
                        |row| row.get(0),
                    )?;
                    if writer_count >= MAX_UNVERIFIED_MESSAGES_PER_WRITER_TOPIC
                        || topic_count >= MAX_UNVERIFIED_MESSAGES_PER_TOPIC
                    {
                        anyhow::bail!("unverified public chat admission quota exceeded");
                    }
                }
            }
            let inserted = insert_chat_message(&tx, msg, trust)?;
            let reconciled = if inserted == 0 {
                reconcile_legacy_chat_order(&tx, msg, logical_time)?
            } else {
                0
            };
            if inserted == 0 && reconciled == 0 {
                continue;
            }
            tx.execute(
                "INSERT INTO chat_topic_clocks(topic, logical_time) VALUES (?1, ?2)
                 ON CONFLICT(topic) DO UPDATE SET
                    logical_time = MAX(logical_time, excluded.logical_time)",
                params![msg.topic, logical_time],
            )?;
            tx.execute(
                "INSERT INTO chat_writer_sequences(topic, user_id, writer_seq)
                 VALUES (?1, ?2, ?3)
                 ON CONFLICT(topic, user_id) DO UPDATE SET
                    writer_seq = MAX(writer_seq, excluded.writer_seq)",
                params![msg.topic, writer_id, writer_seq],
            )?;
        }
        tx.commit()?;
        Ok(())
    }

    pub fn delete_chat_messages(&self, topic: &str) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        let tx = conn.unchecked_transaction()?;
        tx.execute("DELETE FROM chat_messages WHERE topic = ?1", params![topic])?;
        tx.execute(
            "DELETE FROM pending_public_chat WHERE topic = ?1",
            params![topic],
        )?;
        tx.execute(
            "DELETE FROM chat_topic_clocks WHERE topic = ?1",
            params![topic],
        )?;
        tx.execute(
            "DELETE FROM chat_writer_sequences WHERE topic = ?1",
            params![topic],
        )?;
        tx.execute(
            "DELETE FROM chat_sequence_conflicts WHERE topic = ?1",
            params![topic],
        )?;
        tx.commit()?;
        Ok(())
    }

    pub fn get_chat_messages(
        &self,
        topic: &str,
        limit: u32,
        offset: u32,
    ) -> Result<Vec<ChatMessage>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT c.id, c.user_id, c.display_name, c.text, c.topic, c.stage_id, c.timestamp,
                    c.writer_seq, c.logical_time, c.writer_key, c.signature,
                    CASE
                      WHEN p.writer_id IS NULL THEN 0
                      WHEN p.expires_at >= CAST(strftime('%s','now') AS INTEGER) THEN 1
                      WHEN p.expires_at + 604800 >= CAST(strftime('%s','now') AS INTEGER) THEN 2
                      ELSE 0
                    END
             FROM chat_messages c
             LEFT JOIN chat_author_proofs p ON p.writer_id = c.writer_id
             WHERE c.topic = ?1
               AND NOT EXISTS (
                 SELECT 1 FROM chat_sequence_conflicts x
                 WHERE x.topic = c.topic AND x.user_id = c.writer_id
                   AND x.writer_seq = c.writer_seq
               )
             ORDER BY c.logical_time ASC, c.writer_id ASC, c.writer_seq ASC, c.id ASC
             LIMIT ?2 OFFSET ?3",
        )?;
        let msgs = stmt
            .query_map(params![topic, limit, offset], chat_message_from_row)?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(msgs)
    }

    /// Return a newest-first page while preserving chronological order inside
    /// that page. Increasing offset walks backward through older history.
    pub fn get_recent_chat_messages(
        &self,
        topic: &str,
        limit: u32,
        offset: u32,
    ) -> Result<Vec<ChatMessage>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, user_id, display_name, text, topic, stage_id, timestamp,
                    writer_seq, logical_time, writer_key, signature, trust_state
             FROM (
               SELECT c.id, c.user_id, c.display_name, c.text, c.topic, c.stage_id,
                      c.timestamp, c.writer_seq, c.logical_time, c.writer_key, c.signature,
                      CASE
                        WHEN p.writer_id IS NULL THEN 0
                        WHEN p.expires_at >= CAST(strftime('%s','now') AS INTEGER) THEN 1
                        WHEN p.expires_at + 604800 >= CAST(strftime('%s','now') AS INTEGER) THEN 2
                        ELSE 0
                      END AS trust_state,
                      c.writer_id
               FROM chat_messages c
               LEFT JOIN chat_author_proofs p ON p.writer_id = c.writer_id
               WHERE c.topic = ?1
                 AND NOT EXISTS (
                   SELECT 1 FROM chat_sequence_conflicts x
                   WHERE x.topic = c.topic AND x.user_id = c.writer_id
                     AND x.writer_seq = c.writer_seq
                 )
               ORDER BY c.logical_time DESC, c.writer_id DESC, c.writer_seq DESC, c.id DESC
               LIMIT ?2 OFFSET ?3
             ) recent
             ORDER BY logical_time ASC, writer_id ASC, writer_seq ASC, id ASC",
        )?;
        stmt.query_map(params![topic, limit, offset], chat_message_from_row)?
            .collect::<rusqlite::Result<Vec<_>>>()
            .map_err(Into::into)
    }

    /// Get the next writer sequence without reserving it. Local sends must use
    /// `save_local_chat_message` so allocation and persistence stay atomic.
    pub fn get_next_writer_seq(&self, topic: &str, user_id: &str) -> Result<u64> {
        let conn = self.conn.lock().unwrap();
        let current: i64 = conn
            .query_row(
                "SELECT writer_seq FROM chat_writer_sequences
                 WHERE topic = ?1 AND user_id = ?2",
                params![topic, user_id],
                |row| row.get(0),
            )
            .optional()?
            .unwrap_or(0);
        let next = current
            .checked_add(1)
            .ok_or_else(|| anyhow::anyhow!("chat writer sequence exhausted"))?;
        Ok(next as u64)
    }

    /// Compute complete highest-contiguous writer heads without loading message
    /// payloads or truncating to a history page.
    pub fn get_chat_writer_heads(&self, topic: &str) -> Result<Vec<(String, u64, String)>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT c.writer_id, c.writer_seq,
                    CASE WHEN EXISTS(
                        SELECT 1 FROM chat_sequence_conflicts x
                        WHERE x.topic = c.topic AND x.user_id = c.writer_id
                          AND x.writer_seq = c.writer_seq
                    ) THEN '__offbeat/equivocated__' ELSE MIN(c.id) END,
                    MAX(c.logical_time)
             FROM chat_messages c WHERE c.topic = ?1
             GROUP BY c.writer_id, c.writer_seq
             ORDER BY c.writer_id, c.writer_seq",
        )?;
        let mut rows = stmt.query(params![topic])?;
        let mut heads = Vec::new();
        let mut current_writer = String::new();
        let mut current_sequence = 0u64;
        let mut current_id = String::new();
        while let Some(row) = rows.next()? {
            let writer: String = row.get(0)?;
            let sequence = u64::try_from(row.get::<_, i64>(1)?)
                .map_err(|_| anyhow::anyhow!("negative chat writer sequence"))?;
            let message_id: String = row.get(2)?;
            let logical_time = u64::try_from(row.get::<_, i64>(3)?)
                .map_err(|_| anyhow::anyhow!("negative chat logical time"))?;
            let head_id = if message_id == EQUIVOCATED_HEAD_ID {
                message_id
            } else {
                chat_head_commitment(&message_id, logical_time)
            };
            if writer != current_writer {
                if !current_writer.is_empty() {
                    heads.push((
                        std::mem::take(&mut current_writer),
                        current_sequence,
                        std::mem::take(&mut current_id),
                    ));
                }
                current_writer = writer;
                current_sequence = 0;
                current_id.clear();
                if sequence == 0 {
                    current_id = head_id;
                } else if sequence == 1 {
                    current_sequence = 1;
                    current_id = head_id;
                }
            } else if sequence == current_sequence + 1 {
                current_sequence = sequence;
                current_id = head_id;
            }
        }
        if !current_writer.is_empty() {
            heads.push((current_writer, current_sequence, current_id));
        }
        Ok(heads)
    }

    /// Compute `{writer_id: highest_contiguous_writer_seq}` for compatibility
    /// with sequence-only catch-up peers.
    pub fn compute_chat_sv(&self, topic: &str) -> Result<std::collections::HashMap<String, u64>> {
        Ok(self
            .get_chat_writer_heads(topic)?
            .into_iter()
            .map(|(writer, sequence, _)| (writer, sequence))
            .collect())
    }

    /// Return missing messages in authoritative Lamport order.
    pub fn get_messages_since_sv(
        &self,
        topic: &str,
        sv: &std::collections::HashMap<String, u64>,
        limit: u32,
    ) -> Result<Vec<ChatMessage>> {
        self.get_messages_since_heads(topic, sv, &std::collections::HashMap::new(), limit)
    }

    /// Return missing messages plus an equal-sequence variant when the remote
    /// head commitment differs.
    pub fn get_messages_since_heads(
        &self,
        topic: &str,
        sv: &std::collections::HashMap<String, u64>,
        head_ids: &std::collections::HashMap<String, String>,
        limit: u32,
    ) -> Result<Vec<ChatMessage>> {
        let logical_floor = head_ids
            .values()
            .filter_map(|commitment| committed_logical_time(commitment))
            .max()
            .unwrap_or(0);
        let logical_ceiling = logical_floor.saturating_add(MAX_REMOTE_LAMPORT_ADVANCE);
        let conn = self.conn.lock().unwrap();
        let tx = conn.unchecked_transaction()?;
        tx.execute_batch(
            "CREATE TEMP TABLE IF NOT EXISTS requested_chat_heads (
                writer_id TEXT PRIMARY KEY,
                writer_seq INTEGER NOT NULL,
                head_id TEXT NOT NULL
            );
            DELETE FROM requested_chat_heads;",
        )?;
        {
            let mut insert = tx.prepare(
                "INSERT INTO requested_chat_heads(writer_id, writer_seq, head_id)
                 VALUES (?1, ?2, ?3)",
            )?;
            for (writer, sequence) in sv {
                insert.execute(params![
                    writer,
                    sqlite_counter(*sequence, "requested writer sequence")?,
                    head_ids.get(writer).map(String::as_str).unwrap_or_default(),
                ])?;
            }
        }
        let messages = {
            let mut stmt = tx.prepare(
                "SELECT c.id, c.user_id, c.display_name, c.text, c.topic, c.stage_id,
                        c.timestamp, c.writer_seq, c.logical_time, c.writer_key, c.signature,
                        CASE
                          WHEN p.writer_id IS NULL THEN 0
                          WHEN p.expires_at >= CAST(strftime('%s','now') AS INTEGER) THEN 1
                          WHEN p.expires_at + 604800 >= CAST(strftime('%s','now') AS INTEGER) THEN 2
                          ELSE 0
                        END
                 FROM chat_messages c
                 LEFT JOIN chat_author_proofs p ON p.writer_id = c.writer_id
                 LEFT JOIN requested_chat_heads h ON h.writer_id = c.writer_id
                 WHERE c.topic = ?1
                   AND (
                     length(c.writer_key) <> 32 OR
                     (p.writer_id IS NOT NULL AND
                      p.expires_at + 604800 >= CAST(strftime('%s','now') AS INTEGER))
                   )
                   AND c.logical_time <= ?4
                   AND c.writer_seq <= COALESCE(h.writer_seq, 0) + ?5
                   AND (
                    h.writer_id IS NULL OR c.writer_seq > h.writer_seq OR
                    (c.writer_seq = h.writer_seq AND h.head_id <> '' AND h.head_id <> ?3
                     AND (c.id || '@' || c.logical_time) <> h.head_id)
                 )
                 ORDER BY c.logical_time ASC, c.user_id ASC, c.writer_seq ASC, c.id ASC
                 LIMIT ?2",
            )?;
            stmt.query_map(
                params![
                    topic,
                    limit.clamp(1, 1000),
                    EQUIVOCATED_HEAD_ID,
                    logical_ceiling,
                    MAX_REMOTE_LAMPORT_ADVANCE,
                ],
                chat_message_from_row,
            )?
            .collect::<rusqlite::Result<Vec<_>>>()?
        };
        tx.commit()?;
        Ok(messages)
    }

    // --- public chat trust proofs ---

    pub fn pin_main_do_public_key(&self, public_key: &[u8; 32]) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        let existing: Option<Vec<u8>> = conn
            .query_row(
                "SELECT value FROM credentials WHERE key = 'main_do_public_key'",
                [],
                |row| row.get(0),
            )
            .optional()?;
        if let Some(existing) = existing {
            if existing.as_slice() != public_key {
                anyhow::bail!("MainDO public key does not match the pinned trust root");
            }
            return Ok(());
        }
        conn.execute(
            "INSERT INTO credentials(key, value) VALUES ('main_do_public_key', ?1)",
            [public_key.as_slice()],
        )?;
        Ok(())
    }

    pub fn load_main_do_public_key(&self) -> Result<Option<[u8; 32]>> {
        self.get_credential("main_do_public_key")?
            .map(|bytes| {
                bytes
                    .try_into()
                    .map_err(|_| anyhow::anyhow!("stored MainDO public key has wrong length"))
            })
            .transpose()
    }

    pub fn save_chat_author_proof(
        &self,
        writer_key: &[u8],
        attestation_message: &str,
        attestation_signature: &[u8],
        issuer: &[u8],
    ) -> Result<Vec<String>> {
        let writer_key: [u8; 32] = writer_key
            .try_into()
            .map_err(|_| anyhow::anyhow!("chat proof writer key must be 32 bytes"))?;
        let issuer: [u8; 32] = issuer
            .try_into()
            .map_err(|_| anyhow::anyhow!("chat proof issuer must be 32 bytes"))?;
        let pinned = self
            .load_main_do_public_key()?
            .ok_or_else(|| anyhow::anyhow!("MainDO trust root is not pinned"))?;
        if issuer != pinned {
            anyhow::bail!("chat proof issuer does not match the pinned MainDO root");
        }
        let (attested_key, issued_at, expires_at) = parse_chat_attestation(attestation_message)?;
        let writer_id = encode_hex(&writer_key);
        if attested_key != writer_id {
            anyhow::bail!("chat proof does not bind the writer key");
        }
        if issued_at > current_unix_seconds()?.saturating_add(300) {
            anyhow::bail!("chat proof issue time is in the future");
        }
        if current_unix_seconds()? > expires_at.saturating_add(ATTESTATION_GRACE_SECONDS) {
            anyhow::bail!("chat proof is outside its grace period");
        }
        if !crate::signing::verify(
            &pinned,
            attestation_message.as_bytes(),
            attestation_signature,
        ) {
            anyhow::bail!("invalid chat proof signature");
        }

        let conn = self.conn.lock().unwrap();
        let tx = conn.unchecked_transaction()?;
        tx.execute(
            "INSERT INTO chat_author_proofs
             (writer_id, writer_key, attestation_message, attestation_signature, issuer,
              issued_at, expires_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
             ON CONFLICT(writer_id) DO UPDATE SET
               writer_key = excluded.writer_key,
               attestation_message = excluded.attestation_message,
               attestation_signature = excluded.attestation_signature,
               issuer = excluded.issuer,
               issued_at = excluded.issued_at,
               expires_at = excluded.expires_at,
               updated_at = datetime('now')
             WHERE excluded.issued_at >= chat_author_proofs.issued_at",
            params![
                writer_id,
                writer_key,
                attestation_message,
                attestation_signature,
                issuer,
                issued_at,
                expires_at,
            ],
        )?;
        let trust = chat_trust_for_writer(&tx, &writer_id)?;
        tx.execute(
            "UPDATE chat_messages SET trust_state = ?2 WHERE writer_id = ?1",
            params![writer_id, chat_trust_value(trust)],
        )?;
        let topics = {
            let mut stmt = tx.prepare(
                "SELECT DISTINCT topic FROM chat_messages WHERE writer_id = ?1 ORDER BY topic",
            )?;
            stmt.query_map([writer_id], |row| row.get(0))?
                .collect::<rusqlite::Result<Vec<String>>>()?
        };
        tx.commit()?;
        Ok(topics)
    }

    pub fn get_chat_author_proof(
        &self,
        writer_key: &[u8; 32],
    ) -> Result<Option<StoredChatAuthorProof>> {
        self.query_chat_author_proof(writer_key, Some(current_unix_seconds()?))
    }

    /// Load historical public identity evidence for offline passkey unlock.
    ///
    /// Unlike the publishing path, this deliberately ignores proof expiry. The
    /// caller must verify the MainDO signature and may only use an expired proof
    /// to recover local identity, never to authorize a server operation.
    pub fn get_historical_chat_author_proof(
        &self,
        writer_key: &[u8; 32],
    ) -> Result<Option<StoredChatAuthorProof>> {
        self.query_chat_author_proof(writer_key, None)
    }

    fn query_chat_author_proof(
        &self,
        writer_key: &[u8; 32],
        valid_at: Option<i64>,
    ) -> Result<Option<StoredChatAuthorProof>> {
        let writer_id = encode_hex(writer_key);
        self.conn
            .lock()
            .unwrap()
            .query_row(
                "SELECT writer_key, attestation_message, attestation_signature, issuer
                 FROM chat_author_proofs
                 WHERE writer_id = ?1
                   AND (?2 IS NULL OR expires_at + ?3 >= ?2)",
                params![writer_id, valid_at, ATTESTATION_GRACE_SECONDS],
                |row| {
                    Ok(StoredChatAuthorProof {
                        writer_key: row.get(0)?,
                        attestation_message: row.get(1)?,
                        attestation_signature: row.get(2)?,
                        issuer: row.get(3)?,
                    })
                },
            )
            .optional()
            .map_err(Into::into)
    }

    pub fn load_pending_public_chats(
        &self,
        festival_id: &str,
        limit: u32,
    ) -> Result<Vec<PendingPublicChat>> {
        let prefix = format!("festival/{festival_id}/chat/");
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT message_id, message_json FROM pending_public_chat
             WHERE substr(topic, 1, length(?1)) = ?1
             ORDER BY created_at, message_id LIMIT ?2",
        )?;
        stmt.query_map(params![prefix, limit.clamp(1, 1000)], |row| {
            let message_id: String = row.get(0)?;
            let bytes: Vec<u8> = row.get(1)?;
            let message = serde_json::from_slice(&bytes).map_err(|error| {
                rusqlite::Error::FromSqlConversionFailure(
                    1,
                    rusqlite::types::Type::Blob,
                    Box::new(error),
                )
            })?;
            Ok(PendingPublicChat {
                message_id,
                message,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(Into::into)
    }

    pub fn delete_pending_public_chat(&self, message_id: &str) -> Result<()> {
        self.conn.lock().unwrap().execute(
            "DELETE FROM pending_public_chat WHERE message_id = ?1",
            [message_id],
        )?;
        Ok(())
    }

    // --- credentials ---

    /// Read a named credential value (arbitrary bytes).
    pub fn get_credential(&self, key: &str) -> Result<Option<Vec<u8>>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare("SELECT value FROM credentials WHERE key = ?1")?;
        let mut rows = stmt.query(params![key])?;
        if let Some(row) = rows.next()? {
            Ok(Some(row.get(0)?))
        } else {
            Ok(None)
        }
    }

    /// Upsert a named credential value.
    pub fn set_credential(&self, key: &str, value: &[u8]) -> Result<()> {
        self.conn.lock().unwrap().execute(
            "INSERT OR REPLACE INTO credentials (key, value) VALUES (?1, ?2)",
            params![key, value],
        )?;
        Ok(())
    }

    /// Atomically activate an identity recovered from a local passkey PRF.
    pub fn activate_offline_identity(
        &self,
        identity_seed: &[u8; 32],
        attestation_message: &str,
        attestation_signature: &str,
        attestation_issuer: &str,
    ) -> Result<()> {
        let mut conn = self.conn.lock().unwrap();
        let tx = conn.transaction()?;
        let private_state_exists: i64 = tx.query_row(
            "SELECT
                 EXISTS(
                     SELECT 1 FROM credentials
                     WHERE key NOT IN ('main_do_public_key', 'iroh_secret_key')
                 )
              OR EXISTS(SELECT 1 FROM groups)
              OR EXISTS(SELECT 1 FROM starred_sets)
              OR EXISTS(SELECT 1 FROM festival_checkins)
              OR EXISTS(SELECT 1 FROM pending_group_updates)
              OR EXISTS(SELECT 1 FROM pending_public_chat)
              OR EXISTS(SELECT 1 FROM docs WHERE doc_type = 'group' OR id LIKE 'group/%')
              OR EXISTS(SELECT 1 FROM chat_messages WHERE topic LIKE 'group/%')",
            [],
            |row| row.get(0),
        )?;
        if private_state_exists != 0 {
            bail!("offline identity activation requires a logged-out private state");
        }
        for (key, value) in [
            ("identity_secret_key", identity_seed.as_slice()),
            ("attestation_message", attestation_message.as_bytes()),
            ("attestation_signature", attestation_signature.as_bytes()),
            ("attestation_issuer", attestation_issuer.as_bytes()),
        ] {
            tx.execute(
                "INSERT OR REPLACE INTO credentials (key, value) VALUES (?1, ?2)",
                params![key, value],
            )?;
        }
        tx.commit()?;
        Ok(())
    }

    /// Remove account and private state while retaining the offline public cache.
    ///
    /// Public festival documents, verified checkpoints, registry metadata,
    /// public chat (including attribution proofs), and festival peers survive.
    pub fn purge_private_state_for_logout(&self) -> Result<()> {
        let mut conn = self.conn.lock().unwrap();
        // Checkpoint any historical WAL frames before deletion, then use a
        // rollback journal so committed zeroed pages land in the main file.
        // secure_delete overwrites deleted payloads rather than leaving them in
        // SQLite freelist pages recoverable from a copied post-logout database.
        conn.pragma_update(None, "journal_mode", "DELETE")?;
        conn.pragma_update(None, "secure_delete", "ON")?;
        let tx = conn.transaction()?;
        tx.execute_batch(
            "DELETE FROM doc_updates
                 WHERE doc_id LIKE 'group/%'
                    OR doc_id IN (SELECT id FROM docs WHERE doc_type = 'group');
             DELETE FROM docs WHERE id LIKE 'group/%' OR doc_type = 'group';
             DELETE FROM chat_messages WHERE topic LIKE 'group/%';
             DELETE FROM chat_topic_clocks WHERE topic LIKE 'group/%';
             DELETE FROM chat_writer_sequences WHERE topic LIKE 'group/%';
             DELETE FROM chat_sequence_conflicts WHERE topic LIKE 'group/%';
             DELETE FROM pending_group_updates;
             DELETE FROM groups;
             DELETE FROM starred_sets;
             DELETE FROM festival_checkins;
             DELETE FROM pending_public_chat;
             DELETE FROM festival_peers;
             DELETE FROM credentials WHERE key != 'main_do_public_key';",
        )?;
        tx.commit()?;

        // The security boundary is committed at this point. Compaction and
        // restoring WAL are best-effort maintenance and must not turn a
        // completed logout into a false rollback report.
        if let Err(error) = conn.execute_batch("VACUUM; PRAGMA journal_mode = WAL;") {
            tracing::warn!(%error, "post-logout SQLite compaction failed");
        }
        Ok(())
    }

    // --- iroh secret key ---

    /// Credential key used to persist the iroh node secret key.
    const IROH_SECRET_KEY: &str = "iroh_secret_key";

    /// Load a previously persisted iroh `SecretKey`, if any.
    pub fn load_iroh_secret_key(&self) -> Result<Option<iroh::SecretKey>> {
        let blob = self.get_credential(Self::IROH_SECRET_KEY)?;
        match blob {
            Some(bytes) => {
                let arr: [u8; 32] = bytes.try_into().map_err(|v: Vec<u8>| {
                    anyhow::anyhow!(
                        "iroh secret key has wrong length: expected 32 bytes, got {}",
                        v.len()
                    )
                })?;
                Ok(Some(iroh::SecretKey::from_bytes(&arr)))
            }
            None => Ok(None),
        }
    }

    /// Persist an iroh `SecretKey` so it survives restarts.
    pub fn save_iroh_secret_key(&self, key: &iroh::SecretKey) -> Result<()> {
        self.set_credential(Self::IROH_SECRET_KEY, &key.to_bytes())
    }

    // --- group key lookup ---

    /// Load the AES key for a group, returning None if not found.
    pub fn load_group_key(&self, group_id: &str) -> Result<Option<[u8; 32]>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare("SELECT key FROM groups WHERE id = ?1")?;
        let mut rows = stmt.query(params![group_id])?;
        if let Some(row) = rows.next()? {
            let bytes: Vec<u8> = row.get(0)?;
            let arr: [u8; 32] = bytes
                .try_into()
                .map_err(|_| anyhow::anyhow!("group key has wrong length"))?;
            Ok(Some(arr))
        } else {
            Ok(None)
        }
    }

    // --- festival peer directory ---

    /// Upsert a peer learned for a festival. On conflict the row's `last_seen`
    /// only advances (never regresses on a stale sighting), `relay_url` is
    /// overwritten when the new value is present, and `source` reflects the
    /// most recent sighting.
    pub fn upsert_festival_peer(
        &self,
        festival_id: &str,
        endpoint_id: &str,
        relay_url: Option<&str>,
        last_seen: u64,
        source: &str,
    ) -> Result<()> {
        self.conn.lock().unwrap().execute(
            "INSERT INTO festival_peers (festival_id, endpoint_id, relay_url, last_seen, source)
             VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT(festival_id, endpoint_id) DO UPDATE SET
                 last_seen = MAX(last_seen, excluded.last_seen),
                 relay_url = COALESCE(excluded.relay_url, relay_url),
                 source = excluded.source",
            params![
                festival_id,
                endpoint_id,
                relay_url,
                last_seen as i64,
                source
            ],
        )?;
        Ok(())
    }

    /// Load the freshest known peers for a festival, newest first, capped at
    /// `limit`. This is the cold-start bootstrap set fed to gossip `subscribe`.
    pub fn load_festival_peers(
        &self,
        festival_id: &str,
        limit: usize,
    ) -> Result<Vec<BootstrapPeer>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT endpoint_id, relay_url, last_seen, source FROM festival_peers
             WHERE festival_id = ?1 ORDER BY last_seen DESC LIMIT ?2",
        )?;
        let peers = stmt
            .query_map(params![festival_id, limit as i64], |row| {
                Ok(BootstrapPeer {
                    endpoint_id: row.get(0)?,
                    relay_url: row.get(1)?,
                    last_seen: row.get::<_, i64>(2)? as u64,
                    source: row.get(3)?,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(peers)
    }

    /// Bound the directory: keep only the `keep` freshest peers per festival,
    /// deleting older entries. Prevents unbounded growth across many sessions.
    pub fn prune_festival_peers(&self, festival_id: &str, keep: usize) -> Result<()> {
        self.conn.lock().unwrap().execute(
            "DELETE FROM festival_peers
             WHERE festival_id = ?1 AND endpoint_id NOT IN (
                 SELECT endpoint_id FROM festival_peers
                 WHERE festival_id = ?1 ORDER BY last_seen DESC LIMIT ?2
             )",
            params![festival_id, keep as i64],
        )?;
        Ok(())
    }
}

/// A peer row from the durable directory, used to seed gossip bootstrap.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BootstrapPeer {
    pub endpoint_id: String,
    pub relay_url: Option<String>,
    pub last_seen: u64,
    pub source: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::ChatMessage;

    fn test_db() -> Database {
        Database::new_in_memory().expect("in-memory db")
    }

    fn cached_festival(id: &str, start_date: &str) -> Festival {
        Festival {
            id: id.to_string(),
            name: format!("Festival {id}"),
            year: 2027,
            location: "Test Park".to_string(),
            city: "Bristol".to_string(),
            country: "GB".to_string(),
            start_date: start_date.to_string(),
            end_date: "2027-06-13".to_string(),
            stages: vec![Stage {
                id: "main".to_string(),
                name: "Main Stage".to_string(),
                short: "MAIN".to_string(),
                color: "#ff2d8f".to_string(),
                order: 0,
            }],
            genres: vec!["electronic".to_string()],
            status: FestivalStatus::Upcoming,
            clashfinder_id: Some(format!("cf-{id}")),
            public_key: String::new(),
            updated_at: "2027-01-01T00:00:00Z".to_string(),
            lat: Some(51.45),
            lon: Some(-2.58),
        }
    }

    #[test]
    fn festival_registry_cache_roundtrips_and_replaces_authoritatively() {
        let db = test_db();
        assert!(db.load_festival_registry_cache().unwrap().is_none());

        let first = cached_festival("first", "2027-06-12");
        db.replace_festival_registry_cache(
            std::slice::from_ref(&first),
            "2027-01-01T00:00:00Z",
            "00000000000000000001",
        )
        .unwrap();
        let loaded = db.load_festival_registry_cache().unwrap().unwrap();
        assert_eq!(loaded.festivals, vec![first]);
        assert_eq!(loaded.fetched_at, "2027-01-01T00:00:00Z");
        assert_eq!(loaded.request_token, "00000000000000000001");

        let second = cached_festival("second", "2027-07-12");
        db.replace_festival_registry_cache(
            std::slice::from_ref(&second),
            "2027-02-01T00:00:00Z",
            "00000000000000000002",
        )
        .unwrap();
        let replaced = db.load_festival_registry_cache().unwrap().unwrap();
        assert_eq!(replaced.festivals, vec![second.clone()]);
        assert_eq!(replaced.fetched_at, "2027-02-01T00:00:00Z");

        let stale = cached_festival("stale", "2027-05-12");
        assert!(
            !db.replace_festival_registry_cache(
                std::slice::from_ref(&stale),
                "2027-03-01T00:00:00Z",
                "00000000000000000001",
            )
            .unwrap()
        );
        assert!(
            !db.replace_festival_registry_cache(
                std::slice::from_ref(&stale),
                "2027-03-01T00:00:00Z",
                "00000000000000000002",
            )
            .unwrap()
        );
        let preserved = db.load_festival_registry_cache().unwrap().unwrap();
        assert_eq!(preserved.festivals, vec![second]);
        assert_eq!(preserved.fetched_at, "2027-02-01T00:00:00Z");
    }

    #[test]
    fn festival_registry_replacement_rolls_back_on_invalid_snapshot() {
        let db = test_db();
        let original = cached_festival("original", "2027-06-12");
        db.replace_festival_registry_cache(
            std::slice::from_ref(&original),
            "2027-01-01T00:00:00Z",
            "00000000000000000001",
        )
        .unwrap();

        let duplicate = cached_festival("duplicate", "2027-07-12");
        assert!(
            db.replace_festival_registry_cache(
                &[duplicate.clone(), duplicate],
                "2027-02-01T00:00:00Z",
                "00000000000000000002",
            )
            .is_err()
        );
        let loaded = db.load_festival_registry_cache().unwrap().unwrap();
        assert_eq!(loaded.festivals, vec![original]);
        assert_eq!(loaded.fetched_at, "2027-01-01T00:00:00Z");
    }

    #[test]
    fn festival_registry_cache_survives_database_restart() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("registry.db");
        let festival = cached_festival("restart", "2027-06-12");
        {
            let db = Database::new(&path).unwrap();
            db.replace_festival_registry_cache(
                std::slice::from_ref(&festival),
                "2027-01-01T00:00:00Z",
                "00000000000000000001",
            )
            .unwrap();
        }
        let reopened = Database::new(&path).unwrap();
        let loaded = reopened.load_festival_registry_cache().unwrap().unwrap();
        assert_eq!(loaded.festivals, vec![festival]);
    }

    #[test]
    fn festival_registry_rejects_oversized_normalized_snapshot() {
        let db = test_db();
        let mut festivals = Vec::new();
        for festival_index in 0..9 {
            let mut festival = cached_festival(
                &format!("{festival_index}{}", "x".repeat(1023)),
                "2027-06-12",
            );
            festival.name = "Festival".to_string();
            festival.clashfinder_id = None;
            festival.stages = (0..500)
                .map(|stage_index| Stage {
                    id: format!("stage-{stage_index}"),
                    name: "Main Stage".to_string(),
                    short: "MAIN".to_string(),
                    color: "#ff2d8f".to_string(),
                    order: stage_index,
                })
                .collect();
            festivals.push(festival);
        }

        assert!(
            db.replace_festival_registry_cache(
                &festivals,
                "2027-01-01T00:00:00Z",
                "00000000000000000001",
            )
            .is_err()
        );
        assert!(db.load_festival_registry_cache().unwrap().is_none());
    }

    #[test]
    fn festival_registry_repairs_terminal_request_token() {
        let db = test_db();
        db.conn
            .lock()
            .unwrap()
            .execute(
                "INSERT INTO festival_registry_meta(singleton, fetched_at, request_token)
                 VALUES (1, ?1, ?2)",
                params!["2027-01-01T00:00:00Z", "99999999999999999999"],
            )
            .unwrap();
        assert!(db.load_festival_registry_cache().is_err());

        let festival = cached_festival("recovered", "2027-06-12");
        assert!(
            db.replace_festival_registry_cache(
                std::slice::from_ref(&festival),
                "2027-01-02T00:00:00Z",
                "00000000000000000001",
            )
            .unwrap()
        );
        assert_eq!(
            db.load_festival_registry_cache()
                .unwrap()
                .unwrap()
                .festivals,
            vec![festival]
        );

        db.conn
            .lock()
            .unwrap()
            .execute(
                "UPDATE festival_registry_meta SET request_token = ?1 WHERE singleton = 1",
                ["99999999999999999998"],
            )
            .unwrap();
        let after_rollover = cached_festival("after-rollover", "2027-07-12");
        assert!(
            db.replace_festival_registry_cache(
                std::slice::from_ref(&after_rollover),
                "2027-01-03T00:00:00Z",
                "00000000000000000002",
            )
            .unwrap()
        );
        assert_eq!(
            db.load_festival_registry_cache()
                .unwrap()
                .unwrap()
                .festivals,
            vec![after_rollover]
        );
    }

    #[test]
    fn festival_registry_rejects_oversized_metadata_before_loading_rows() {
        let db = test_db();
        db.conn
            .lock()
            .unwrap()
            .execute(
                "INSERT INTO festival_registry_meta(singleton, fetched_at, request_token)
                 VALUES (1, ?1, ?2)",
                params!["x".repeat(MAX_CACHE_TEXT_BYTES + 1), "00000000000000000001"],
            )
            .unwrap();

        assert!(db.load_festival_registry_cache().is_err());
    }

    #[test]
    fn test_save_and_load_doc() {
        let db = test_db();
        let data = b"hello world";
        db.save_doc("doc1", "festival", data).unwrap();
        let loaded = db.load_doc("doc1").unwrap();
        assert_eq!(loaded, Some(data.to_vec()));
    }

    #[test]
    fn test_load_doc_missing() {
        let db = test_db();
        let loaded = db.load_doc("nonexistent").unwrap();
        assert_eq!(loaded, None);
    }

    #[test]
    fn test_festival_peer_upsert_and_load() {
        let db = test_db();
        db.upsert_festival_peer("fest-a", "peer-1", Some("https://relay"), 100, "crdt")
            .unwrap();
        db.upsert_festival_peer("fest-a", "peer-2", None, 200, "gossip")
            .unwrap();

        let peers = db.load_festival_peers("fest-a", 10).unwrap();
        assert_eq!(peers.len(), 2);
        // Newest first.
        assert_eq!(peers[0].endpoint_id, "peer-2");
        assert_eq!(peers[0].relay_url, None);
        assert_eq!(peers[0].source, "gossip");
        assert_eq!(peers[1].endpoint_id, "peer-1");
        assert_eq!(peers[1].relay_url.as_deref(), Some("https://relay"));
    }

    #[test]
    fn test_festival_peer_scoped_by_festival() {
        let db = test_db();
        db.upsert_festival_peer("fest-a", "peer-1", None, 100, "crdt")
            .unwrap();
        db.upsert_festival_peer("fest-b", "peer-2", None, 100, "crdt")
            .unwrap();
        assert_eq!(db.load_festival_peers("fest-a", 10).unwrap().len(), 1);
        assert_eq!(db.load_festival_peers("fest-b", 10).unwrap().len(), 1);
        assert_eq!(db.load_festival_peers("fest-c", 10).unwrap().len(), 0);
    }

    #[test]
    fn test_festival_peer_last_seen_never_regresses() {
        let db = test_db();
        db.upsert_festival_peer("fest-a", "peer-1", Some("https://r1"), 500, "crdt")
            .unwrap();
        // Stale sighting with an older timestamp and no relay must not regress.
        db.upsert_festival_peer("fest-a", "peer-1", None, 100, "ble")
            .unwrap();
        let peers = db.load_festival_peers("fest-a", 10).unwrap();
        assert_eq!(peers.len(), 1);
        assert_eq!(peers[0].last_seen, 500);
        // relay_url preserved (COALESCE keeps existing when new is NULL).
        assert_eq!(peers[0].relay_url.as_deref(), Some("https://r1"));
        // source reflects most recent write.
        assert_eq!(peers[0].source, "ble");
    }

    #[test]
    fn test_festival_peer_load_respects_limit() {
        let db = test_db();
        for i in 0..5 {
            db.upsert_festival_peer("fest-a", &format!("peer-{i}"), None, 100 + i, "crdt")
                .unwrap();
        }
        let peers = db.load_festival_peers("fest-a", 3).unwrap();
        assert_eq!(peers.len(), 3);
        // Highest last_seen first.
        assert_eq!(peers[0].endpoint_id, "peer-4");
        assert_eq!(peers[2].endpoint_id, "peer-2");
    }

    #[test]
    fn test_festival_peer_prune_keeps_freshest() {
        let db = test_db();
        for i in 0..5 {
            db.upsert_festival_peer("fest-a", &format!("peer-{i}"), None, 100 + i, "crdt")
                .unwrap();
        }
        db.prune_festival_peers("fest-a", 2).unwrap();
        let peers = db.load_festival_peers("fest-a", 10).unwrap();
        assert_eq!(peers.len(), 2);
        assert_eq!(peers[0].endpoint_id, "peer-4");
        assert_eq!(peers[1].endpoint_id, "peer-3");
    }

    #[test]
    fn test_delete_chat_messages_removes_only_requested_topic() {
        let db = test_db();
        for (id, topic) in [("one", "group/g1/chat"), ("two", "group/g2/chat")] {
            db.save_chat_message(&crate::types::ChatMessage {
                id: id.to_string(),
                user_id: "user".to_string(),
                display_name: "User".to_string(),
                text: "hello".to_string(),
                topic: topic.to_string(),
                stage_id: None,
                timestamp: "2026-01-01T00:00:00Z".to_string(),
                writer_seq: 1,
                logical_time: 1,
                writer_key: Vec::new(),
                signature: Vec::new(),
                trust: crate::types::ChatTrust::Unverified,
            })
            .unwrap();
        }

        db.delete_chat_messages("group/g1/chat").unwrap();
        assert!(
            db.get_chat_messages("group/g1/chat", 10, 0)
                .unwrap()
                .is_empty()
        );
        assert_eq!(
            db.get_chat_messages("group/g2/chat", 10, 0).unwrap().len(),
            1
        );
    }

    #[test]
    fn test_delete_doc_removes_snapshot_and_updates() {
        let db = test_db();
        db.save_doc("group/g1/state", "group", &[1, 2]).unwrap();
        db.append_doc_update("group/g1/state", &[3, 4]).unwrap();
        db.delete_doc("group/g1/state").unwrap();
        assert!(db.load_doc("group/g1/state").unwrap().is_none());
        assert!(db.load_doc_updates("group/g1/state").unwrap().is_empty());
    }

    #[test]
    fn test_list_docs() {
        let db = test_db();
        db.save_doc("a", "festival", b"data_a").unwrap();
        db.save_doc("b", "festival", b"data_b").unwrap();
        db.save_doc("c", "group", b"data_c").unwrap();
        let ids = db.list_docs("festival").unwrap();
        assert_eq!(ids.len(), 2);
        assert!(ids.contains(&"a".to_string()));
        assert!(ids.contains(&"b".to_string()));
    }

    #[test]
    fn test_save_and_load_group() {
        let db = test_db();
        let key = vec![0u8; 32];
        db.save_group("g1", "f1", "My Group", &key).unwrap();
        let groups = db.load_groups("f1").unwrap();
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].0, "g1");
        assert_eq!(groups[0].1, "My Group");
        assert_eq!(groups[0].2, key);

        db.save_group("g1", "", "", &[1u8; 32]).unwrap();
        let groups = db.load_groups("f1").unwrap();
        assert_eq!(groups[0].1, "My Group", "repeat joins preserve synced name");
        assert_eq!(groups[0].2, vec![1u8; 32]);
        assert_eq!(
            db.load_group_festival_id("g1").unwrap().as_deref(),
            Some("f1")
        );
        assert!(db.load_group_festival_id("missing").unwrap().is_none());
    }

    #[test]
    fn test_load_all_group_keys() {
        let db = test_db();
        db.save_group("g1", "f1", "One", &[1u8; 32]).unwrap();
        db.save_group("g2", "f2", "Two", &[2u8; 32]).unwrap();
        let mut groups = db.load_all_group_keys().unwrap();
        groups.sort_by(|a, b| a.0.cmp(&b.0));
        assert_eq!(
            groups,
            vec![("g1".to_string(), [1u8; 32]), ("g2".to_string(), [2u8; 32])]
        );
    }

    #[test]
    fn finalize_group_leave_compacts_queue_and_purges_private_state() {
        let db = test_db();
        db.save_group("g1", "festival-a", "Group", &[7; 32])
            .unwrap();
        db.save_doc("group/g1/state", "group", &[1]).unwrap();
        db.append_doc_update("group/g1/state", &[2]).unwrap();
        db.save_chat_message(&crate::types::ChatMessage {
            id: "message".to_string(),
            user_id: "user".to_string(),
            display_name: "User".to_string(),
            text: "private".to_string(),
            topic: "group/g1/chat".to_string(),
            stage_id: None,
            timestamp: "2026-01-01T00:00:00Z".to_string(),
            writer_seq: 1,
            logical_time: 1,
            writer_key: Vec::new(),
            signature: Vec::new(),
            trust: crate::types::ChatTrust::Unverified,
        })
        .unwrap();
        db.enqueue_group_update("festival-a", "g1", &[3]).unwrap();
        db.enqueue_group_update("festival-a", "g1", &[4]).unwrap();

        let leave_id = db.finalize_group_leave("festival-a", "g1", &[9]).unwrap();
        let pending = db.load_pending_group_updates("festival-a").unwrap();
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].id, leave_id);
        assert_eq!(pending[0].envelope, vec![9]);
        assert!(db.load_group_key("g1").unwrap().is_none());
        assert!(db.load_doc("group/g1/state").unwrap().is_none());
        assert!(db.load_doc_updates("group/g1/state").unwrap().is_empty());
        assert!(
            db.get_chat_messages("group/g1/chat", 10, 0)
                .unwrap()
                .is_empty()
        );
        let after_leave = db
            .save_local_chat_message(ChatMessage {
                id: "after-leave".to_string(),
                user_id: "user".to_string(),
                display_name: "User".to_string(),
                text: "new lifecycle".to_string(),
                topic: "group/g1/chat".to_string(),
                stage_id: None,
                timestamp: "not-authoritative".to_string(),
                writer_seq: 0,
                logical_time: 0,
                writer_key: Vec::new(),
                signature: Vec::new(),
                trust: crate::types::ChatTrust::Unverified,
            })
            .unwrap();
        assert_eq!((after_leave.writer_seq, after_leave.logical_time), (1, 1));
    }

    #[test]
    fn pending_group_updates_survive_restart_and_are_festival_scoped() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("pending-groups.db");
        let first_id;
        {
            let db = Database::new(&path).unwrap();
            first_id = db
                .enqueue_group_update("festival-a", "group-a", &[2, 3])
                .unwrap();
            db.enqueue_group_update("festival-b", "group-b", &[5])
                .unwrap();
        }

        let db = Database::new(&path).unwrap();
        let pending = db.load_pending_group_updates("festival-a").unwrap();
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].id, first_id);
        assert_eq!(pending[0].group_id, "group-a");
        assert_eq!(pending[0].envelope, vec![2, 3]);
        db.delete_pending_group_update(first_id).unwrap();
        assert!(
            db.load_pending_group_updates("festival-a")
                .unwrap()
                .is_empty()
        );
        assert_eq!(
            db.load_pending_group_updates("festival-b").unwrap().len(),
            1
        );
    }

    #[test]
    fn test_update_group_name() {
        let db = test_db();
        db.save_group("g1", "f1", "", &[0u8; 32]).unwrap();
        db.update_group_name("g1", "My Group").unwrap();
        let groups = db.load_groups("f1").unwrap();
        assert_eq!(groups[0].1, "My Group");
    }

    #[test]
    fn test_delete_group() {
        let db = test_db();
        db.save_group("g1", "f1", "My Group", &[0u8; 32]).unwrap();
        db.delete_group("g1").unwrap();
        let groups = db.load_groups("f1").unwrap();
        assert!(groups.is_empty());
    }

    #[test]
    fn test_toggle_star() {
        let db = test_db();
        // First toggle → starred
        let starred = db.toggle_star("f1", "s1").unwrap();
        assert!(starred);
        // Second toggle → unstarred
        let starred = db.toggle_star("f1", "s1").unwrap();
        assert!(!starred);
    }

    #[test]
    fn test_get_stars() {
        let db = test_db();
        db.toggle_star("f1", "s1").unwrap();
        db.toggle_star("f1", "s2").unwrap();
        let stars = db.get_stars("f1").unwrap();
        assert_eq!(stars.len(), 2);
    }

    #[test]
    fn test_stars_survive_database_restart() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("stars.db");
        {
            let db = Database::new(&path).unwrap();
            assert!(db.toggle_star("f1", "s1").unwrap());
        }
        let reopened = Database::new(&path).unwrap();
        assert_eq!(reopened.get_stars("f1").unwrap(), vec!["s1"]);
    }

    #[test]
    fn logout_purge_survives_restart_with_only_public_state() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("logout.db");
        let public_topic = "festival/f1/chat/campsite";
        let private_topic = "group/g1/chat";
        let private_text = "OFFBEAT_LOGOUT_PRIVATE_MESSAGE_CANARY_7F4A19";
        let private_peer = "OFFBEAT_LOGOUT_PRIVATE_PEER_CANARY_91C2E8";
        let private_key = [0xA5u8; 32];
        let identity_seed = [0xB6u8; 32];
        {
            let db = Database::new(&path).unwrap();
            db.replace_festival_registry_cache(
                &[cached_festival("f1", "2026-06-01")],
                "2026-06-01T00:00:00Z",
                "00000000000000000001",
            )
            .unwrap();
            db.save_doc("festival/f1/state", "festival", b"public-doc")
                .unwrap();
            db.append_doc_update("festival/f1/state", b"public-update")
                .unwrap();
            db.save_verified_festival_update(&VerifiedFestivalUpdate {
                doc_id: "festival/f1/state".to_string(),
                authority_seq: 7,
                kind: 1,
                signed_update: SignedUpdate {
                    update: b"checkpoint".to_vec(),
                    author: "authority".to_string(),
                    signature: vec![2; 64],
                },
            })
            .unwrap();
            db.upsert_festival_peer("f1", private_peer, None, 1, "private-group")
                .unwrap();

            db.save_doc("group/g1/state", "group", b"private-doc")
                .unwrap();
            db.append_doc_update("group/g1/state", b"private-update")
                .unwrap();
            db.save_group("g1", "f1", "Friends", &private_key).unwrap();
            db.enqueue_group_update("f1", "g1", b"private-envelope")
                .unwrap();
            db.toggle_star("f1", "set-1").unwrap();
            db.save_festival_checkin(&FestivalCheckIn {
                festival_id: "f1".to_string(),
                kind: "stage".to_string(),
                value: Some("main".to_string()),
                checked_at: 1,
                expires_at: 2,
                revision: 1,
            })
            .unwrap();

            for (id, topic, text) in [
                ("public-message", public_topic, "public"),
                ("private-message", private_topic, private_text),
            ] {
                db.save_chat_message(&ChatMessage {
                    id: id.to_string(),
                    user_id: "user".to_string(),
                    display_name: "User".to_string(),
                    text: text.to_string(),
                    topic: topic.to_string(),
                    stage_id: None,
                    timestamp: "2026-06-01T00:00:00Z".to_string(),
                    writer_seq: 1,
                    logical_time: 1,
                    writer_key: Vec::new(),
                    signature: Vec::new(),
                    trust: ChatTrust::Unverified,
                })
                .unwrap();
            }

            db.set_credential("main_do_public_key", &[4; 32]).unwrap();
            db.set_credential("identity_secret_key", &identity_seed).unwrap();
            db.set_credential("device_id", b"device").unwrap();
            db.set_credential("display_name", b"User").unwrap();
            db.set_credential("attestation_message", b"message").unwrap();
            db.set_credential("attestation_signature", b"signature")
                .unwrap();
            db.set_credential("attestation_issuer", b"issuer").unwrap();
            db.set_credential("iroh_secret_key", &[6; 32]).unwrap();
            let conn = db.conn.lock().unwrap();
            conn.execute(
                "INSERT INTO chat_author_proofs
                 (writer_id, writer_key, attestation_message, attestation_signature,
                  issuer, issued_at, expires_at)
                 VALUES (?1, ?2, 'proof', ?3, ?4, 1, 2)",
                params![
                    encode_hex(&[7u8; 32]),
                    vec![7u8; 32],
                    vec![8u8; 64],
                    vec![4u8; 32]
                ],
            )
            .unwrap();
            conn.execute(
                "INSERT INTO pending_public_chat(message_id, topic, message_json)
                 VALUES ('draft', ?1, X'00')",
                [public_topic],
            )
            .unwrap();
            for topic in [public_topic, private_topic] {
                conn.execute(
                    "INSERT INTO chat_sequence_conflicts(topic, user_id, writer_seq)
                     VALUES (?1, 'user', 99)",
                    [topic],
                )
                .unwrap();
            }
            drop(conn);

            db.purge_private_state_for_logout().unwrap();
        }

        let db = Database::new(&path).unwrap();
        assert!(db.load_festival_registry_cache().unwrap().is_some());
        assert_eq!(
            db.load_doc("festival/f1/state").unwrap(),
            Some(b"public-doc".to_vec())
        );
        assert_eq!(db.load_doc_updates("festival/f1/state").unwrap().len(), 1);
        assert_eq!(db.highest_verified_festival_seq("festival/f1/state").unwrap(), 7);
        assert!(db.load_festival_peers("f1", 10).unwrap().is_empty());
        assert_eq!(db.get_chat_messages(public_topic, 10, 0).unwrap().len(), 1);
        assert_eq!(
            db.get_historical_chat_author_proof(&[7; 32])
                .unwrap()
                .unwrap()
                .attestation_message,
            "proof"
        );
        assert_eq!(
            db.get_credential("main_do_public_key").unwrap(),
            Some(vec![4; 32])
        );

        assert!(db.load_doc("group/g1/state").unwrap().is_none());
        assert!(db.load_doc_updates("group/g1/state").unwrap().is_empty());
        assert!(db.load_groups("f1").unwrap().is_empty());
        assert!(db.load_pending_group_updates("f1").unwrap().is_empty());
        assert!(db.get_stars("f1").unwrap().is_empty());
        assert!(db.load_festival_checkin("f1").unwrap().is_none());
        assert!(db.get_chat_messages(private_topic, 10, 0).unwrap().is_empty());
        for key in [
            "identity_secret_key",
            "device_id",
            "display_name",
            "attestation_message",
            "attestation_signature",
            "attestation_issuer",
            "iroh_secret_key",
        ] {
            assert!(db.get_credential(key).unwrap().is_none(), "retained {key}");
        }
        let conn = db.conn.lock().unwrap();
        let pending_public: i64 = conn
            .query_row("SELECT COUNT(*) FROM pending_public_chat", [], |row| row.get(0))
            .unwrap();
        let private_clocks: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM chat_topic_clocks WHERE topic LIKE 'group/%'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        let public_clocks: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM chat_topic_clocks WHERE topic = ?1",
                [public_topic],
                |row| row.get(0),
            )
            .unwrap();
        let private_sequences: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM chat_writer_sequences WHERE topic LIKE 'group/%'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        let public_sequences: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM chat_writer_sequences WHERE topic = ?1",
                [public_topic],
                |row| row.get(0),
            )
            .unwrap();
        let private_conflicts: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM chat_sequence_conflicts WHERE topic LIKE 'group/%'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        let public_conflicts: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM chat_sequence_conflicts WHERE topic = ?1",
                [public_topic],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(pending_public, 0);
        assert_eq!(private_clocks, 0);
        assert_eq!(public_clocks, 1);
        assert_eq!(private_sequences, 0);
        assert_eq!(public_sequences, 1);
        assert_eq!(private_conflicts, 0);
        assert_eq!(public_conflicts, 1);
        drop(conn);
        drop(db);

        for candidate in [
            path.clone(),
            path.with_extension("db-wal"),
            path.with_extension("db-journal"),
        ] {
            if !candidate.exists() {
                continue;
            }
            let bytes = std::fs::read(&candidate).unwrap();
            assert!(!bytes.windows(private_text.len()).any(|window| {
                window == private_text.as_bytes()
            }));
            assert!(!bytes
                .windows(private_peer.len())
                .any(|window| window == private_peer.as_bytes()));
            assert!(!bytes
                .windows(private_key.len())
                .any(|window| window == private_key));
            assert!(!bytes
                .windows(identity_seed.len())
                .any(|window| window == identity_seed));
        }
    }

    #[test]
    fn test_save_and_get_chat_messages() {
        let db = test_db();
        let msg = ChatMessage {
            id: "m1".to_string(),
            user_id: "u1".to_string(),
            display_name: "Alice".to_string(),
            text: "hello".to_string(),
            topic: "festival/f1".to_string(),
            stage_id: None,
            timestamp: "2026-06-13T20:00:00Z".to_string(),
            writer_seq: 0,
            logical_time: 0,
            writer_key: Vec::new(),
            signature: Vec::new(),
            trust: crate::types::ChatTrust::Unverified,
        };
        db.save_chat_message(&msg).unwrap();
        let msgs = db.get_chat_messages("festival/f1", 10, 0).unwrap();
        assert_eq!(msgs.len(), 1);
        assert_eq!(msgs[0].id, "m1");
        assert_eq!(msgs[0].text, "hello");
    }

    #[test]
    fn recent_chat_pages_walk_backward_and_stay_chronological() {
        let db = test_db();
        for i in 0..5_u64 {
            db.save_chat_message(&ChatMessage {
                id: format!("recent-{i}"),
                user_id: "u1".to_string(),
                display_name: "Alice".to_string(),
                text: format!("message {i}"),
                topic: "festival/f1/chat/campsite".to_string(),
                stage_id: None,
                timestamp: "2026-06-13T20:00:00Z".to_string(),
                writer_seq: i,
                logical_time: i,
                writer_key: Vec::new(),
                signature: Vec::new(),
                trust: crate::types::ChatTrust::Unverified,
            })
            .unwrap();
        }

        let newest = db
            .get_recent_chat_messages("festival/f1/chat/campsite", 2, 0)
            .unwrap();
        assert_eq!(
            newest
                .iter()
                .map(|message| message.id.as_str())
                .collect::<Vec<_>>(),
            vec!["recent-3", "recent-4"]
        );

        let previous = db
            .get_recent_chat_messages("festival/f1/chat/campsite", 2, 2)
            .unwrap();
        assert_eq!(
            previous
                .iter()
                .map(|message| message.id.as_str())
                .collect::<Vec<_>>(),
            vec!["recent-1", "recent-2"]
        );
    }

    #[test]
    fn test_save_chat_messages_batch() {
        let db = test_db();
        let msgs: Vec<ChatMessage> = (0..100)
            .map(|i| ChatMessage {
                id: format!("m{i}"),
                user_id: "u1".to_string(),
                display_name: "Alice".to_string(),
                text: format!("msg {i}"),
                topic: "topic/batch".to_string(),
                stage_id: None,
                timestamp: format!("2026-06-13T20:{:02}:00Z", i % 60),
                writer_seq: i as u64,
                logical_time: i as u64,
                writer_key: Vec::new(),
                signature: Vec::new(),
                trust: crate::types::ChatTrust::Unverified,
            })
            .collect();
        db.save_chat_messages_batch(&msgs).unwrap();
        let loaded = db.get_chat_messages("topic/batch", 200, 0).unwrap();
        assert_eq!(loaded.len(), 100);
    }

    #[test]
    fn test_chat_messages_filtered_by_topic() {
        let db = test_db();
        for i in 0..3 {
            db.save_chat_message(&ChatMessage {
                id: format!("m{i}"),
                user_id: "u1".to_string(),
                display_name: "Alice".to_string(),
                text: format!("msg {i}"),
                topic: "topic/a".to_string(),
                stage_id: None,
                timestamp: format!("2026-06-13T2{i}:00:00Z"),
                writer_seq: i as u64,
                logical_time: i as u64,
                writer_key: Vec::new(),
                signature: Vec::new(),
                trust: crate::types::ChatTrust::Unverified,
            })
            .unwrap();
        }
        db.save_chat_message(&ChatMessage {
            id: "mx".to_string(),
            user_id: "u2".to_string(),
            display_name: "Bob".to_string(),
            text: "other".to_string(),
            topic: "topic/b".to_string(),
            stage_id: None,
            timestamp: "2026-06-13T20:00:00Z".to_string(),
            writer_seq: 0,
            logical_time: 0,
            writer_key: Vec::new(),
            signature: Vec::new(),
            trust: crate::types::ChatTrust::Unverified,
        })
        .unwrap();
        let msgs = db.get_chat_messages("topic/a", 10, 0).unwrap();
        assert_eq!(msgs.len(), 3);
    }

    #[test]
    fn test_chat_message_insert_or_ignore_preserves_received_at() {
        let db = test_db();
        let msg = ChatMessage {
            id: "dedup1".to_string(),
            user_id: "u1".to_string(),
            display_name: "Alice".to_string(),
            text: "first".to_string(),
            topic: "topic/a".to_string(),
            stage_id: None,
            timestamp: "2026-06-13T20:00:00Z".to_string(),
            writer_seq: 0,
            logical_time: 0,
            writer_key: Vec::new(),
            signature: Vec::new(),
            trust: crate::types::ChatTrust::Unverified,
        };
        db.save_chat_message(&msg).unwrap();

        // Read original received_at
        let conn = db.conn.lock().unwrap();
        let original_received_at: String = conn
            .query_row(
                "SELECT received_at FROM chat_messages WHERE id = ?1",
                params!["dedup1"],
                |row| row.get(0),
            )
            .unwrap();
        drop(conn);

        // Insert same ID again with different text — should be ignored
        let msg2 = ChatMessage {
            id: "dedup1".to_string(),
            user_id: "u1".to_string(),
            display_name: "Alice".to_string(),
            text: "second".to_string(),
            topic: "topic/a".to_string(),
            stage_id: None,
            timestamp: "2026-06-13T21:00:00Z".to_string(),
            writer_seq: 1,
            logical_time: 1,
            writer_key: Vec::new(),
            signature: Vec::new(),
            trust: crate::types::ChatTrust::Unverified,
        };
        db.save_chat_message(&msg2).unwrap();

        // received_at and text should be unchanged (original preserved)
        let conn = db.conn.lock().unwrap();
        let (stored_text, stored_received_at): (String, String) = conn
            .query_row(
                "SELECT text, received_at FROM chat_messages WHERE id = ?1",
                params!["dedup1"],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert_eq!(
            stored_text, "first",
            "INSERT OR IGNORE should preserve original text"
        );
        assert_eq!(
            stored_received_at, original_received_at,
            "INSERT OR IGNORE should preserve original received_at"
        );
    }

    #[test]
    fn test_chat_message_batch_insert_or_ignore() {
        let db = test_db();
        let msg = ChatMessage {
            id: "batch_dedup".to_string(),
            user_id: "u1".to_string(),
            display_name: "Alice".to_string(),
            text: "original".to_string(),
            topic: "topic/b".to_string(),
            stage_id: None,
            timestamp: "2026-06-13T20:00:00Z".to_string(),
            writer_seq: 0,
            logical_time: 0,
            writer_key: Vec::new(),
            signature: Vec::new(),
            trust: crate::types::ChatTrust::Unverified,
        };
        db.save_chat_message(&msg).unwrap();

        // Batch insert includes the same ID
        let msgs = vec![
            ChatMessage {
                id: "batch_dedup".to_string(),
                user_id: "u1".to_string(),
                display_name: "Alice".to_string(),
                text: "replaced".to_string(),
                topic: "topic/b".to_string(),
                stage_id: None,
                timestamp: "2026-06-13T21:00:00Z".to_string(),
                writer_seq: 1,
                logical_time: 1,
                writer_key: Vec::new(),
                signature: Vec::new(),
                trust: ChatTrust::Unverified,
            },
            ChatMessage {
                id: "batch_new".to_string(),
                user_id: "u1".to_string(),
                display_name: "Alice".to_string(),
                text: "new msg".to_string(),
                topic: "topic/b".to_string(),
                stage_id: None,
                timestamp: "2026-06-13T22:00:00Z".to_string(),
                writer_seq: 2,
                logical_time: 2,
                writer_key: Vec::new(),
                signature: Vec::new(),
                trust: ChatTrust::Unverified,
            },
        ];
        db.save_chat_messages_batch(&msgs).unwrap();

        let stored = db.get_chat_messages("topic/b", 10, 0).unwrap();
        assert_eq!(stored.len(), 2);
        let original = stored.iter().find(|m| m.id == "batch_dedup").unwrap();
        assert_eq!(
            original.text, "original",
            "batch INSERT OR IGNORE should preserve original"
        );
    }

    #[test]
    fn test_get_next_writer_seq() {
        let db = test_db();
        let topic = "festival/f1/chat/general";
        let user_id = "u1";

        // No messages yet → seq should be 1
        let seq = db.get_next_writer_seq(topic, user_id).unwrap();
        assert_eq!(seq, 1);

        // Insert a message with writer_seq=1
        db.save_chat_message(&ChatMessage {
            id: "m1".to_string(),
            user_id: user_id.to_string(),
            display_name: "Alice".to_string(),
            text: "hello".to_string(),
            topic: topic.to_string(),
            stage_id: None,
            timestamp: "2026-06-13T20:00:00Z".to_string(),
            writer_seq: 1,
            logical_time: 1,
            writer_key: Vec::new(),
            signature: Vec::new(),
            trust: crate::types::ChatTrust::Unverified,
        })
        .unwrap();

        // Next should be 2
        let seq2 = db.get_next_writer_seq(topic, user_id).unwrap();
        assert_eq!(seq2, 2);
    }

    #[test]
    fn test_compute_chat_sv_uses_highest_contiguous_sequence() {
        let db = test_db();
        let topic = "festival/f1/chat/general";
        for (writer, sequence) in [("alice", 1), ("alice", 2), ("alice", 4), ("bob", 2)] {
            db.save_chat_message(&ChatMessage {
                id: format!("{writer}-{sequence}"),
                user_id: writer.to_string(),
                display_name: writer.to_string(),
                text: "message".to_string(),
                topic: topic.to_string(),
                stage_id: None,
                timestamp: "not-authoritative".to_string(),
                writer_seq: sequence,
                logical_time: sequence,
                writer_key: Vec::new(),
                signature: Vec::new(),
                trust: crate::types::ChatTrust::Unverified,
            })
            .unwrap();
        }

        let sv = db.compute_chat_sv(topic).unwrap();
        assert_eq!(sv.get("alice").copied(), Some(2));
        assert_eq!(sv.get("bob").copied(), Some(0));
    }

    #[test]
    fn test_get_messages_since_sv() {
        let db = test_db();
        let topic = "festival/f1/chat/general";

        // Alice has seqs 1, 2, 3; Bob has seqs 1, 2
        for seq in 1u64..=3 {
            db.save_chat_message(&ChatMessage {
                id: format!("alice-{seq}"),
                user_id: "alice".to_string(),
                display_name: "Alice".to_string(),
                text: format!("alice msg {seq}"),
                topic: topic.to_string(),
                stage_id: None,
                timestamp: format!("2026-06-13T20:0{seq}:00Z"),
                writer_seq: seq,
                logical_time: seq,
                writer_key: Vec::new(),
                signature: Vec::new(),
                trust: crate::types::ChatTrust::Unverified,
            })
            .unwrap();
        }
        for seq in 1u64..=2 {
            db.save_chat_message(&ChatMessage {
                id: format!("bob-{seq}"),
                user_id: "bob".to_string(),
                display_name: "Bob".to_string(),
                text: format!("bob msg {seq}"),
                topic: topic.to_string(),
                stage_id: None,
                timestamp: format!("2026-06-13T21:0{seq}:00Z"),
                writer_seq: seq,
                logical_time: seq,
                writer_key: Vec::new(),
                signature: Vec::new(),
                trust: crate::types::ChatTrust::Unverified,
            })
            .unwrap();
        }

        // sv: alice=2, bob=1 → should return alice-3 and bob-2
        let sv = std::collections::HashMap::from([
            ("alice".to_string(), 2u64),
            ("bob".to_string(), 1u64),
        ]);
        let msgs = db.get_messages_since_sv(topic, &sv, 50).unwrap();
        let ids: Vec<&str> = msgs.iter().map(|m| m.id.as_str()).collect();
        assert!(ids.contains(&"alice-3"), "expected alice-3 in {ids:?}");
        assert!(ids.contains(&"bob-2"), "expected bob-2 in {ids:?}");
        assert!(!ids.contains(&"alice-1"), "alice-1 should be filtered");
        assert!(!ids.contains(&"alice-2"), "alice-2 should be filtered");
        assert!(!ids.contains(&"bob-1"), "bob-1 should be filtered");

        // sv empty → all messages returned
        let all = db
            .get_messages_since_sv(topic, &std::collections::HashMap::new(), 50)
            .unwrap();
        assert_eq!(all.len(), 5);
    }

    #[test]
    fn lamport_order_ignores_wall_clock_and_tracks_causality() {
        let db = test_db();
        let topic = "festival/f1/chat/general";
        db.save_chat_message(&ChatMessage {
            id: "remote".to_string(),
            user_id: "bob".to_string(),
            display_name: "Bob".to_string(),
            text: "future clock".to_string(),
            topic: topic.to_string(),
            stage_id: None,
            timestamp: "2099-01-01T00:00:00Z".to_string(),
            writer_seq: 4,
            logical_time: 50,
            writer_key: Vec::new(),
            signature: Vec::new(),
            trust: crate::types::ChatTrust::Unverified,
        })
        .unwrap();

        let local = db
            .save_local_chat_message(ChatMessage {
                id: "reply".to_string(),
                user_id: "alice".to_string(),
                display_name: "Alice".to_string(),
                text: "reply".to_string(),
                topic: topic.to_string(),
                stage_id: None,
                timestamp: "1970-01-01T00:00:00Z".to_string(),
                writer_seq: 0,
                logical_time: 0,
                writer_key: Vec::new(),
                signature: Vec::new(),
                trust: crate::types::ChatTrust::Unverified,
            })
            .unwrap();

        assert_eq!(local.logical_time, 51);
        assert_eq!(local.writer_seq, 1);
        let ids: Vec<String> = db
            .get_chat_messages(topic, 10, 0)
            .unwrap()
            .into_iter()
            .map(|message| message.id)
            .collect();
        assert_eq!(ids, vec!["remote", "reply"]);
    }

    #[test]
    fn rejects_terminal_remote_clock_without_poisoning_topic() {
        let db = test_db();
        let rejected = db.save_chat_message(&ChatMessage {
            id: "poison".to_string(),
            user_id: "mallory".to_string(),
            display_name: "Mallory".to_string(),
            text: "poison".to_string(),
            topic: "topic".to_string(),
            stage_id: None,
            timestamp: "not-authoritative".to_string(),
            writer_seq: 1,
            logical_time: 1_000_001,
            writer_key: Vec::new(),
            signature: Vec::new(),
            trust: crate::types::ChatTrust::Unverified,
        });
        assert!(rejected.is_err());
        let local = db
            .save_local_chat_message(ChatMessage {
                id: "local".to_string(),
                user_id: "alice".to_string(),
                display_name: "Alice".to_string(),
                text: "safe".to_string(),
                topic: "topic".to_string(),
                stage_id: None,
                timestamp: "not-authoritative".to_string(),
                writer_seq: 0,
                logical_time: 0,
                writer_key: Vec::new(),
                signature: Vec::new(),
                trust: crate::types::ChatTrust::Unverified,
            })
            .unwrap();
        assert_eq!(local.logical_time, 1);
    }

    #[test]
    fn batch_cannot_ratchet_remote_clock_limit_per_message() {
        let db = test_db();
        let messages = [
            ChatMessage {
                id: "first".to_string(),
                user_id: "mallory".to_string(),
                display_name: "Mallory".to_string(),
                text: "first".to_string(),
                topic: "topic".to_string(),
                stage_id: None,
                timestamp: "display-only".to_string(),
                writer_seq: 1,
                logical_time: 1_000_000,
                writer_key: Vec::new(),
                signature: Vec::new(),
                trust: crate::types::ChatTrust::Unverified,
            },
            ChatMessage {
                id: "second".to_string(),
                user_id: "mallory".to_string(),
                display_name: "Mallory".to_string(),
                text: "second".to_string(),
                topic: "topic".to_string(),
                stage_id: None,
                timestamp: "display-only".to_string(),
                writer_seq: 2,
                logical_time: 2_000_000,
                writer_key: Vec::new(),
                signature: Vec::new(),
                trust: crate::types::ChatTrust::Unverified,
            },
        ];
        assert!(db.save_chat_messages_batch(&messages).is_err());
        assert!(db.get_chat_messages("topic", 10, 0).unwrap().is_empty());
        let local = db
            .save_local_chat_message(ChatMessage {
                id: "local".to_string(),
                user_id: "alice".to_string(),
                display_name: "Alice".to_string(),
                text: "safe".to_string(),
                topic: "topic".to_string(),
                stage_id: None,
                timestamp: "display-only".to_string(),
                writer_seq: 0,
                logical_time: 0,
                writer_key: Vec::new(),
                signature: Vec::new(),
                trust: crate::types::ChatTrust::Unverified,
            })
            .unwrap();
        assert_eq!(local.logical_time, 1);
    }

    #[test]
    fn catchup_pages_respect_requester_lamport_floor() {
        let db = test_db();
        for (sequence, logical_time) in [(1, 1_000_000), (2, 2_000_000)] {
            db.save_chat_message(&ChatMessage {
                id: format!("m{sequence}"),
                user_id: "alice".to_string(),
                display_name: "Alice".to_string(),
                text: "message".to_string(),
                topic: "topic".to_string(),
                stage_id: None,
                timestamp: "display-only".to_string(),
                writer_seq: sequence,
                logical_time,
                writer_key: Vec::new(),
                signature: Vec::new(),
                trust: crate::types::ChatTrust::Unverified,
            })
            .unwrap();
        }

        let first = db
            .get_messages_since_heads(
                "topic",
                &std::collections::HashMap::new(),
                &std::collections::HashMap::new(),
                50,
            )
            .unwrap();
        assert_eq!(
            first
                .iter()
                .map(|message| message.id.as_str())
                .collect::<Vec<_>>(),
            vec!["m1"]
        );
        let second = db
            .get_messages_since_heads(
                "topic",
                &std::collections::HashMap::from([("alice".to_string(), 1)]),
                &std::collections::HashMap::from([("alice".to_string(), "m1@1000000".to_string())]),
                50,
            )
            .unwrap();
        assert_eq!(
            second
                .iter()
                .map(|message| message.id.as_str())
                .collect::<Vec<_>>(),
            vec!["m2"]
        );
    }

    #[test]
    fn duplicate_legacy_message_recovers_authoritative_lamport_time() {
        let db = test_db();
        let mut message = ChatMessage {
            id: "same-id".to_string(),
            user_id: "alice".to_string(),
            display_name: "Alice".to_string(),
            text: "same payload".to_string(),
            topic: "topic".to_string(),
            stage_id: None,
            timestamp: "display-only".to_string(),
            writer_seq: 1,
            logical_time: 0,
            writer_key: Vec::new(),
            signature: Vec::new(),
            trust: crate::types::ChatTrust::Unverified,
        };
        db.save_chat_message(&message).unwrap();
        let local_heads = db.get_chat_writer_heads("topic").unwrap();
        assert_eq!(local_heads[0].2, "same-id@1");

        message.logical_time = 50;
        let remote = test_db();
        remote.save_chat_message(&message).unwrap();
        let repaired = remote
            .get_messages_since_heads(
                "topic",
                &std::collections::HashMap::from([("alice".to_string(), 1)]),
                &std::collections::HashMap::from([("alice".to_string(), local_heads[0].2.clone())]),
                1,
            )
            .unwrap();
        assert_eq!(repaired.len(), 1);
        db.save_chat_messages_batch(&repaired).unwrap();
        let stored = db.get_chat_messages("topic", 10, 0).unwrap();
        assert_eq!(stored[0].logical_time, 50);
        let local = db
            .save_local_chat_message(ChatMessage {
                id: "next".to_string(),
                user_id: "bob".to_string(),
                display_name: "Bob".to_string(),
                text: "reply".to_string(),
                topic: "topic".to_string(),
                stage_id: None,
                timestamp: "display-only".to_string(),
                writer_seq: 0,
                logical_time: 0,
                writer_key: Vec::new(),
                signature: Vec::new(),
                trust: crate::types::ChatTrust::Unverified,
            })
            .unwrap();
        assert_eq!(local.logical_time, 51);
    }

    #[test]
    fn concurrent_lamport_ties_use_stable_writer_order() {
        let db = test_db();
        for (writer, id, timestamp) in [
            ("bob", "b", "1970-01-01T00:00:00Z"),
            ("alice", "a", "2099-01-01T00:00:00Z"),
        ] {
            db.save_chat_message(&ChatMessage {
                id: id.to_string(),
                user_id: writer.to_string(),
                display_name: writer.to_string(),
                text: "concurrent".to_string(),
                topic: "topic".to_string(),
                stage_id: None,
                timestamp: timestamp.to_string(),
                writer_seq: 1,
                logical_time: 10,
                writer_key: Vec::new(),
                signature: Vec::new(),
                trust: crate::types::ChatTrust::Unverified,
            })
            .unwrap();
        }
        let ids: Vec<String> = db
            .get_chat_messages("topic", 10, 0)
            .unwrap()
            .into_iter()
            .map(|message| message.id)
            .collect();
        assert_eq!(ids, vec!["a", "b"]);
    }

    #[test]
    fn concurrent_local_sends_allocate_unique_positions() {
        let db = std::sync::Arc::new(test_db());
        let topic = "festival/f1/chat/general";
        let handles: Vec<_> = (0..8)
            .map(|index| {
                let db = db.clone();
                std::thread::spawn(move || {
                    db.save_local_chat_message(ChatMessage {
                        id: format!("m{index}"),
                        user_id: "alice".to_string(),
                        display_name: "Alice".to_string(),
                        text: format!("message {index}"),
                        topic: topic.to_string(),
                        stage_id: None,
                        timestamp: "not-authoritative".to_string(),
                        writer_seq: 0,
                        logical_time: 0,
                        writer_key: Vec::new(),
                        signature: Vec::new(),
                        trust: crate::types::ChatTrust::Unverified,
                    })
                    .unwrap()
                })
            })
            .collect();
        let mut positions: Vec<(u64, u64)> = handles
            .into_iter()
            .map(|handle| {
                let message = handle.join().unwrap();
                (message.writer_seq, message.logical_time)
            })
            .collect();
        positions.sort_unstable();
        assert_eq!(
            positions,
            (1..=8).map(|value| (value, value)).collect::<Vec<_>>()
        );
    }

    #[test]
    fn complete_heads_and_bounded_catchup_scale_past_history_page() {
        let db = test_db();
        let messages: Vec<_> = (1..=1_200u64)
            .map(|sequence| ChatMessage {
                id: format!("alice-{sequence}"),
                user_id: "alice".to_string(),
                display_name: "Alice".to_string(),
                text: "message".to_string(),
                topic: "topic".to_string(),
                stage_id: None,
                timestamp: "not-authoritative".to_string(),
                writer_seq: sequence,
                logical_time: sequence,
                writer_key: Vec::new(),
                signature: Vec::new(),
                trust: crate::types::ChatTrust::Unverified,
            })
            .collect();
        db.save_chat_messages_batch(&messages).unwrap();

        assert_eq!(
            db.get_chat_writer_heads("topic").unwrap(),
            vec![("alice".to_string(), 1_200, "alice-1200@1200".to_string())]
        );
        let page = db
            .get_messages_since_heads(
                "topic",
                &std::collections::HashMap::from([("alice".to_string(), 0)]),
                &std::collections::HashMap::new(),
                50,
            )
            .unwrap();
        assert_eq!(page.len(), 50);
        assert_eq!(page.first().unwrap().writer_seq, 1);
        assert_eq!(page.last().unwrap().writer_seq, 50);

        let mut conflict = messages.last().unwrap().clone();
        conflict.id = "alice-1200-conflict".to_string();
        db.save_chat_message(&conflict).unwrap();
        assert_eq!(
            db.get_chat_writer_heads("topic").unwrap(),
            vec![("alice".to_string(), 1_200, EQUIVOCATED_HEAD_ID.to_string(),)]
        );
        let after_conflict = db
            .get_messages_since_heads(
                "topic",
                &std::collections::HashMap::from([("alice".to_string(), 1_200)]),
                &std::collections::HashMap::from([(
                    "alice".to_string(),
                    EQUIVOCATED_HEAD_ID.to_string(),
                )]),
                50,
            )
            .unwrap();
        assert!(after_conflict.is_empty());
    }

    #[test]
    fn lamport_clock_survives_database_restart() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("lamport.db");
        {
            let db = Database::new(&path).unwrap();
            db.save_chat_message(&ChatMessage {
                id: "remote".to_string(),
                user_id: "alice".to_string(),
                display_name: "Alice".to_string(),
                text: "before restart".to_string(),
                topic: "topic".to_string(),
                stage_id: None,
                timestamp: "2099-01-01".to_string(),
                writer_seq: 7,
                logical_time: 42,
                writer_key: Vec::new(),
                signature: Vec::new(),
                trust: crate::types::ChatTrust::Unverified,
            })
            .unwrap();
        }
        let db = Database::new(&path).unwrap();
        let next = db
            .save_local_chat_message(ChatMessage {
                id: "local".to_string(),
                user_id: "alice".to_string(),
                display_name: "Alice".to_string(),
                text: "after restart".to_string(),
                topic: "topic".to_string(),
                stage_id: None,
                timestamp: "1970-01-01".to_string(),
                writer_seq: 0,
                logical_time: 0,
                writer_key: Vec::new(),
                signature: Vec::new(),
                trust: crate::types::ChatTrust::Unverified,
            })
            .unwrap();
        assert_eq!((next.writer_seq, next.logical_time), (8, 43));
    }

    #[test]
    fn public_chat_proof_promotes_signed_history_and_pins_root() {
        let db = test_db();
        let writer = crate::signing::generate_signing_key();
        let root = crate::signing::generate_signing_key();
        let root_key = root.verifying_key().to_bytes();
        db.pin_main_do_public_key(&root_key).unwrap();
        assert!(db.pin_main_do_public_key(&[9; 32]).is_err());

        let now = current_unix_seconds().unwrap();
        let writer_key = writer.verifying_key().to_bytes();
        let message_text = format!(
            "attestation:v1:{}:{}:{}",
            encode_hex(&writer_key),
            now - 60,
            now + 3600,
        );
        let proof_signature = crate::signing::sign(&root, message_text.as_bytes());
        let topic = "festival/f/chat/campsite";
        let mut message = ChatMessage {
            id: "proof-message".to_string(),
            user_id: crate::auth::get_user_id(&writer),
            display_name: "Alice".to_string(),
            text: "hello".to_string(),
            topic: topic.to_string(),
            stage_id: None,
            timestamp: "display-only".to_string(),
            writer_seq: 1,
            logical_time: 1,
            writer_key: Vec::new(),
            signature: Vec::new(),
            trust: ChatTrust::Unverified,
        };
        crate::signing::sign_public_chat_message(&writer, &mut message).unwrap();
        db.save_chat_message(&message).unwrap();
        assert_eq!(
            db.get_chat_messages(topic, 10, 0).unwrap()[0].trust,
            ChatTrust::Unverified
        );
        assert!(
            db.get_messages_since_heads(
                topic,
                &std::collections::HashMap::new(),
                &std::collections::HashMap::new(),
                10,
            )
            .unwrap()
            .is_empty(),
            "signed messages without a usable registration proof are live-only",
        );

        let topics = db
            .save_chat_author_proof(&writer_key, &message_text, &proof_signature, &root_key)
            .unwrap();
        assert_eq!(topics, vec![topic]);
        assert_eq!(
            db.get_chat_messages(topic, 10, 0).unwrap()[0].trust,
            ChatTrust::Verified
        );
        assert_eq!(
            db.get_messages_since_heads(
                topic,
                &std::collections::HashMap::new(),
                &std::collections::HashMap::new(),
                10,
            )
            .unwrap()
            .len(),
            1,
        );
        assert!(
            db.save_chat_author_proof(&writer_key, &message_text, &[0; 64], &root_key)
                .is_err()
        );
    }

    #[test]
    fn festival_checkin_round_trip_and_clear() {
        let db = test_db();
        let checkin = FestivalCheckIn {
            festival_id: "fest".to_string(),
            kind: "campsite".to_string(),
            value: None,
            checked_at: 100,
            expires_at: 7300,
            revision: 1,
        };
        db.save_festival_checkin(&checkin).unwrap();
        assert_eq!(db.load_festival_checkin("fest").unwrap(), Some(checkin));
        db.clear_festival_checkin("fest").unwrap();
        assert!(db.load_festival_checkin("fest").unwrap().is_none());
    }

    #[test]
    fn test_wal_mode_enabled() {
        let db = test_db();
        let conn = db.conn.lock().unwrap();
        let mode: String = conn
            .pragma_query_value(None, "journal_mode", |row| row.get(0))
            .unwrap();
        // In-memory databases use "memory" journal mode, but the pragmas should
        // still be set without error. For on-disk databases this would be "wal".
        // The key assertion is that we can open the DB and the pragma calls succeed.
        assert!(
            mode == "memory" || mode == "wal",
            "expected 'memory' or 'wal', got '{mode}'"
        );
    }

    #[test]
    fn test_wal_mode_on_disk() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.db");
        let db = Database::new(&path).unwrap();
        let conn = db.conn.lock().unwrap();
        let mode: String = conn
            .pragma_query_value(None, "journal_mode", |row| row.get(0))
            .unwrap();
        assert_eq!(mode, "wal", "on-disk database should use WAL journal mode");
    }

    #[test]
    fn test_busy_timeout_set() {
        let db = test_db();
        let conn = db.conn.lock().unwrap();
        let timeout: i64 = conn
            .pragma_query_value(None, "busy_timeout", |row| row.get(0))
            .unwrap();
        assert!(
            timeout >= 5000,
            "busy_timeout should be at least 5000ms, got {timeout}"
        );
    }

    #[test]
    fn test_iroh_secret_key_roundtrip_in_memory() {
        let db = test_db();

        // Initially no key stored.
        assert!(db.load_iroh_secret_key().unwrap().is_none());

        // Save a key and reload it.
        let key = iroh::SecretKey::generate();
        db.save_iroh_secret_key(&key).unwrap();

        let loaded = db
            .load_iroh_secret_key()
            .unwrap()
            .expect("key should exist");
        assert_eq!(
            key.public(),
            loaded.public(),
            "loaded key must produce the same public key"
        );
    }

    #[test]
    fn test_secret_key_persistence() {
        // Verify that closing and re-opening the same on-disk database
        // produces the same iroh EndpointId (i.e. the secret key survives).
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("persist.db");

        // First open: generate & store a key.
        let public_key_1 = {
            let db = Database::new(&path).unwrap();
            assert!(db.load_iroh_secret_key().unwrap().is_none());
            let key = iroh::SecretKey::generate();
            db.save_iroh_secret_key(&key).unwrap();
            key.public()
        }; // db dropped, connection closed

        // Second open: key should be loaded from the database.
        let public_key_2 = {
            let db = Database::new(&path).unwrap();
            let loaded = db
                .load_iroh_secret_key()
                .unwrap()
                .expect("key should survive across reopens");
            loaded.public()
        };

        assert_eq!(
            public_key_1, public_key_2,
            "EndpointId must be identical after database reopen"
        );
    }
}
