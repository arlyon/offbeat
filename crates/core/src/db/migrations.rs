use anyhow::Result;
use rusqlite::Connection;

/// Each migration: (version, SQL to apply).
const MIGRATIONS: &[(u32, &str)] = &[
    (1, include_str!("schema.sql")),
    (
        2,
        "CREATE TABLE IF NOT EXISTS doc_updates (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        doc_id TEXT NOT NULL,
        update_data BLOB NOT NULL,
        created_at TEXT NOT NULL DEFAULT (datetime('now'))
    );
    CREATE INDEX IF NOT EXISTS idx_doc_updates_doc_id ON doc_updates(doc_id);",
    ),
    // Durable peer directory: persists peers learned online so the mesh can
    // cold-start offline. Scoped per festival because gossip bootstrap is
    // per-topic — only peers seen on a festival's topics seed that topic.
    (
        3,
        "CREATE TABLE IF NOT EXISTS festival_peers (
        festival_id TEXT NOT NULL,
        endpoint_id TEXT NOT NULL,
        relay_url TEXT,
        last_seen INTEGER NOT NULL,
        source TEXT NOT NULL,
        PRIMARY KEY (festival_id, endpoint_id)
    );
    CREATE INDEX IF NOT EXISTS idx_festival_peers_recency
        ON festival_peers(festival_id, last_seen DESC);",
    ),
    (
        4,
        "CREATE TABLE IF NOT EXISTS verified_festival_updates (
        doc_id TEXT NOT NULL,
        authority_seq INTEGER NOT NULL,
        kind INTEGER NOT NULL,
        update_data BLOB NOT NULL,
        author TEXT NOT NULL,
        signature BLOB NOT NULL,
        received_at TEXT NOT NULL DEFAULT (datetime('now')),
        PRIMARY KEY (doc_id, authority_seq, kind)
    );
    CREATE INDEX IF NOT EXISTS idx_verified_festival_checkpoint
        ON verified_festival_updates(doc_id, kind, authority_seq DESC);",
    ),
    (
        5,
        "CREATE TABLE IF NOT EXISTS pending_group_updates (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        festival_id TEXT NOT NULL,
        group_id TEXT NOT NULL,
        envelope BLOB NOT NULL,
        created_at TEXT NOT NULL DEFAULT (datetime('now'))
    );
    CREATE INDEX IF NOT EXISTS idx_pending_group_updates_festival
        ON pending_group_updates(festival_id, id);",
    ),
    (
        6,
        "ALTER TABLE chat_messages
            ADD COLUMN logical_time INTEGER NOT NULL DEFAULT 0;
        UPDATE chat_messages
            SET writer_seq = 0
            WHERE writer_seq < 0 OR writer_seq > 1000000;
        WITH writer_max AS (
            SELECT topic, user_id, COALESCE(MAX(writer_seq), 0) AS max_seq
            FROM chat_messages
            GROUP BY topic, user_id
        ), legacy_rank AS (
            SELECT c.id,
                   w.max_seq + ROW_NUMBER() OVER (
                       PARTITION BY c.topic, c.user_id
                       ORDER BY c.received_at, c.id
                   ) AS assigned_seq
            FROM chat_messages c
            JOIN writer_max w ON w.topic = c.topic AND w.user_id = c.user_id
            WHERE c.writer_seq = 0
        )
        UPDATE chat_messages
        SET writer_seq = (
            SELECT assigned_seq FROM legacy_rank WHERE legacy_rank.id = chat_messages.id
        )
        WHERE writer_seq = 0;
        UPDATE chat_messages SET logical_time = writer_seq;
        CREATE TABLE chat_topic_clocks (
            topic TEXT PRIMARY KEY,
            logical_time INTEGER NOT NULL
        );
        INSERT INTO chat_topic_clocks(topic, logical_time)
            SELECT topic, MAX(logical_time) FROM chat_messages GROUP BY topic;
        CREATE TABLE chat_writer_sequences (
            topic TEXT NOT NULL,
            user_id TEXT NOT NULL,
            writer_seq INTEGER NOT NULL,
            PRIMARY KEY(topic, user_id)
        );
        CREATE TABLE chat_sequence_conflicts (
            topic TEXT NOT NULL,
            user_id TEXT NOT NULL,
            writer_seq INTEGER NOT NULL,
            PRIMARY KEY(topic, user_id, writer_seq)
        );
        INSERT INTO chat_writer_sequences(topic, user_id, writer_seq)
            SELECT topic, user_id, MAX(writer_seq)
            FROM chat_messages GROUP BY topic, user_id;
        DROP INDEX IF EXISTS idx_chat_topic_ts;
        CREATE INDEX idx_chat_topic_order
            ON chat_messages(topic, logical_time, user_id, writer_seq, id);",
    ),
    (
        7,
        "CREATE TABLE IF NOT EXISTS cached_festivals (
            id TEXT PRIMARY KEY,
            name TEXT NOT NULL,
            year INTEGER NOT NULL,
            location TEXT NOT NULL,
            city TEXT NOT NULL,
            country TEXT NOT NULL,
            start_date TEXT NOT NULL,
            end_date TEXT NOT NULL,
            genres_json TEXT NOT NULL,
            status TEXT NOT NULL,
            clashfinder_id TEXT,
            public_key TEXT NOT NULL,
            updated_at TEXT NOT NULL,
            lat REAL,
            lon REAL
        );
        CREATE TABLE IF NOT EXISTS cached_festival_stages (
            festival_id TEXT NOT NULL,
            id TEXT NOT NULL,
            name TEXT NOT NULL,
            short TEXT NOT NULL,
            color TEXT NOT NULL,
            sort_order INTEGER NOT NULL,
            PRIMARY KEY (festival_id, id)
        );
        CREATE INDEX IF NOT EXISTS idx_cached_festival_stages_order
            ON cached_festival_stages(festival_id, sort_order, id);
        CREATE TABLE IF NOT EXISTS festival_registry_meta (
            singleton INTEGER PRIMARY KEY CHECK (singleton = 1),
            fetched_at TEXT NOT NULL
        );",
    ),
    (
        8,
        "ALTER TABLE festival_registry_meta
             ADD COLUMN request_token TEXT NOT NULL DEFAULT '00000000000000000000';",
    ),
];

/// Ensure the `_migrations` table exists and apply any pending migrations.
///
/// Idempotent: re-running against an already-migrated database is a no-op.
pub fn apply_migrations(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS _migrations (
            version INTEGER PRIMARY KEY,
            applied_at TEXT NOT NULL DEFAULT (datetime('now'))
        )",
    )?;

    for &(version, sql) in MIGRATIONS {
        let applied: bool = conn.query_row(
            "SELECT COUNT(*) FROM _migrations WHERE version = ?1",
            [version],
            |row| row.get::<_, i64>(0),
        )? > 0;

        if !applied {
            let tx = conn.unchecked_transaction()?;
            tx.execute_batch(sql)?;
            tx.execute("INSERT INTO _migrations (version) VALUES (?1)", [version])?;
            tx.commit()?;
            tracing::info!("applied migration v{version}");
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_migrations_idempotent() {
        let conn = Connection::open_in_memory().unwrap();
        apply_migrations(&conn).unwrap();
        // Apply again — should be a no-op
        apply_migrations(&conn).unwrap();

        // Verify migration was recorded
        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM _migrations", [], |row| row.get(0))
            .unwrap();
        assert_eq!(count, MIGRATIONS.len() as i64);
    }

    #[test]
    fn legacy_chat_rows_seed_lamport_state() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE _migrations (
                version INTEGER PRIMARY KEY,
                applied_at TEXT NOT NULL DEFAULT (datetime('now'))
            );",
        )
        .unwrap();
        for &(version, sql) in MIGRATIONS.iter().take(5) {
            conn.execute_batch(sql).unwrap();
            conn.execute("INSERT INTO _migrations(version) VALUES (?1)", [version])
                .unwrap();
        }
        conn.execute(
            "INSERT INTO chat_messages
             (id, topic, user_id, display_name, text, timestamp, writer_seq)
             VALUES ('legacy', 'festival/f/chat/general', 'alice', 'Alice', 'hi',
                     '2099-01-01T00:00:00Z', 7)",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO chat_messages
             (id, topic, user_id, display_name, text, timestamp, writer_seq)
             VALUES ('corrupt', 'festival/f/chat/other', 'mallory', 'Mallory', 'bad',
                     '1970-01-01T00:00:00Z', -9)",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO chat_messages
             (id, topic, user_id, display_name, text, timestamp, writer_seq)
             VALUES ('terminal', 'festival/f/chat/terminal', 'mallory', 'Mallory', 'bad',
                     '1970-01-01T00:00:00Z', 9223372036854775807)",
            [],
        )
        .unwrap();
        for index in 0..120 {
            conn.execute(
                "INSERT INTO chat_messages
                 (id, topic, user_id, display_name, text, timestamp, writer_seq)
                 VALUES (?1, 'festival/f/chat/page', 'legacy-writer', 'Legacy', 'old',
                         '1970-01-01T00:00:00Z', 0)",
                [format!("page-{index:03}")],
            )
            .unwrap();
        }

        apply_migrations(&conn).unwrap();

        let logical_time: i64 = conn
            .query_row(
                "SELECT logical_time FROM chat_messages WHERE id = 'legacy'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        let topic_clock: i64 = conn
            .query_row(
                "SELECT logical_time FROM chat_topic_clocks
                 WHERE topic = 'festival/f/chat/general'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        let writer_seq: i64 = conn
            .query_row(
                "SELECT writer_seq FROM chat_writer_sequences
                 WHERE topic = 'festival/f/chat/general' AND user_id = 'alice'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!((logical_time, topic_clock, writer_seq), (7, 7, 7));
        let normalized: (i64, i64) = conn
            .query_row(
                "SELECT writer_seq, logical_time FROM chat_messages WHERE id = 'corrupt'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert_eq!(normalized, (1, 1));
        let terminal: (i64, i64) = conn
            .query_row(
                "SELECT writer_seq, logical_time FROM chat_messages WHERE id = 'terminal'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert_eq!(terminal, (1, 1));
        let page_sequences: (i64, i64, i64) = conn
            .query_row(
                "SELECT COUNT(*), MIN(writer_seq), MAX(writer_seq)
                 FROM chat_messages WHERE topic = 'festival/f/chat/page'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .unwrap();
        assert_eq!(page_sequences, (120, 1, 120));
    }

    #[test]
    fn migration_seven_adds_registry_cache_to_existing_database() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE _migrations (
                version INTEGER PRIMARY KEY,
                applied_at TEXT NOT NULL DEFAULT (datetime('now'))
            );",
        )
        .unwrap();
        for &(version, sql) in MIGRATIONS.iter().take(6) {
            conn.execute_batch(sql).unwrap();
            conn.execute("INSERT INTO _migrations(version) VALUES (?1)", [version])
                .unwrap();
        }
        conn.execute_batch(
            "DROP TABLE cached_festival_stages;
             DROP TABLE cached_festivals;
             DROP TABLE festival_registry_meta;",
        )
        .unwrap();

        apply_migrations(&conn).unwrap();
        for table in [
            "cached_festivals",
            "cached_festival_stages",
            "festival_registry_meta",
        ] {
            let exists: i64 = conn
                .query_row(
                    "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = ?1",
                    [table],
                    |row| row.get(0),
                )
                .unwrap();
            assert_eq!(exists, 1, "migration should create {table}");
        }
        let request_token_column: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM pragma_table_info('festival_registry_meta')
                 WHERE name = 'request_token'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(request_token_column, 1);
    }

    #[test]
    fn test_tables_created() {
        let conn = Connection::open_in_memory().unwrap();
        apply_migrations(&conn).unwrap();

        // Check core tables exist
        let tables: Vec<String> = {
            let mut stmt = conn
                .prepare("SELECT name FROM sqlite_master WHERE type='table' ORDER BY name")
                .unwrap();
            stmt.query_map([], |row| row.get(0))
                .unwrap()
                .collect::<rusqlite::Result<Vec<_>>>()
                .unwrap()
        };

        assert!(tables.contains(&"docs".to_string()));
        assert!(tables.contains(&"groups".to_string()));
        assert!(tables.contains(&"chat_messages".to_string()));
        assert!(tables.contains(&"credentials".to_string()));
        assert!(tables.contains(&"starred_sets".to_string()));
        assert!(tables.contains(&"festival_peers".to_string()));
        assert!(tables.contains(&"verified_festival_updates".to_string()));
        assert!(tables.contains(&"pending_group_updates".to_string()));
        assert!(tables.contains(&"chat_topic_clocks".to_string()));
        assert!(tables.contains(&"chat_writer_sequences".to_string()));
        assert!(tables.contains(&"chat_sequence_conflicts".to_string()));
        assert!(tables.contains(&"cached_festivals".to_string()));
        assert!(tables.contains(&"cached_festival_stages".to_string()));
        assert!(tables.contains(&"festival_registry_meta".to_string()));
        assert!(tables.contains(&"_migrations".to_string()));
    }
}
