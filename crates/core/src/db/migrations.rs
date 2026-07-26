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
            conn.execute_batch(sql)?;
            conn.execute("INSERT INTO _migrations (version) VALUES (?1)", [version])?;
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
        assert!(tables.contains(&"_migrations".to_string()));
    }
}
