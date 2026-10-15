CREATE TABLE app_settings (
    id INTEGER PRIMARY KEY CHECK (id = 1),
    revision INTEGER NOT NULL CHECK (revision > 0),
    document TEXT NOT NULL
);

CREATE TABLE tool_stats (
    client TEXT NOT NULL,
    tool TEXT NOT NULL,
    outcome TEXT NOT NULL,
    calls INTEGER NOT NULL,
    duration_ms INTEGER NOT NULL,
    last_duration_ms INTEGER NOT NULL,
    last_used TEXT NOT NULL,
    PRIMARY KEY (client, tool, outcome)
);

CREATE TABLE write_audit (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    client TEXT NOT NULL,
    actor_id INTEGER,
    tool TEXT NOT NULL,
    target TEXT NOT NULL,
    outcome TEXT NOT NULL,
    created_at TEXT NOT NULL
);

CREATE INDEX audit_client_id ON write_audit (client, id);

CREATE TABLE feed_subjects (
    subject TEXT PRIMARY KEY,
    generation INTEGER NOT NULL CHECK (generation > 0),
    digest BLOB UNIQUE NOT NULL,
    sources TEXT NOT NULL,
    enabled INTEGER NOT NULL CHECK (enabled IN (0, 1))
);

CREATE TABLE feed_metadata (
    id INTEGER PRIMARY KEY CHECK (id = 1),
    namespace BLOB NOT NULL
);
