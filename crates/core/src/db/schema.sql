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
    writer_seq INTEGER NOT NULL DEFAULT 0,
    received_at TEXT NOT NULL DEFAULT (datetime('now'))
);
CREATE INDEX IF NOT EXISTS idx_chat_topic_ts ON chat_messages(topic, timestamp);
CREATE INDEX IF NOT EXISTS idx_chat_writer_seq ON chat_messages(topic, user_id, writer_seq);

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

CREATE TABLE IF NOT EXISTS cached_festivals (
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
);
