CREATE TABLE IF NOT EXISTS docs (
    id TEXT PRIMARY KEY,
    doc_type TEXT NOT NULL,  -- 'festival' or 'group'
    data BLOB NOT NULL,
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE IF NOT EXISTS groups (
    id TEXT PRIMARY KEY,
    festival_id TEXT NOT NULL,
    name TEXT NOT NULL,
    key BLOB NOT NULL,  -- AES-256 group key
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE IF NOT EXISTS chat_messages (
    id TEXT PRIMARY KEY,
    topic TEXT NOT NULL,
    user_id TEXT NOT NULL,
    display_name TEXT NOT NULL,
    text TEXT NOT NULL,
    stage_id TEXT,
    timestamp TEXT NOT NULL,
    received_at TEXT NOT NULL DEFAULT (datetime('now'))
);
CREATE INDEX IF NOT EXISTS idx_chat_topic_ts ON chat_messages(topic, timestamp);

CREATE TABLE IF NOT EXISTS credentials (
    key TEXT PRIMARY KEY,
    value BLOB NOT NULL
);

CREATE TABLE IF NOT EXISTS starred_sets (
    festival_id TEXT NOT NULL,
    set_id TEXT NOT NULL,
    starred_at TEXT NOT NULL DEFAULT (datetime('now')),
    PRIMARY KEY (festival_id, set_id)
);

CREATE TABLE IF NOT EXISTS gossip_log (
    seq INTEGER PRIMARY KEY AUTOINCREMENT,
    topic TEXT NOT NULL,
    data BLOB NOT NULL,
    received_at TEXT NOT NULL DEFAULT (datetime('now'))
);
CREATE INDEX IF NOT EXISTS idx_gossip_topic_seq ON gossip_log(topic, seq);
