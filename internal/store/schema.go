// Schema and model types for the gn-drive SQLite store.
//
// Ported from desktop/backend/services/db.go (createAllTables) and
// desktop/backend/models/* (Profile, Board, Flow, ...).
package store

// schema is the full SQL DDL applied at first open.
// It is identical to the Wails schema so users can migrate transparently.
const schema = `
CREATE TABLE IF NOT EXISTS settings (
    key   TEXT PRIMARY KEY,
    value TEXT NOT NULL DEFAULT ''
);

CREATE TABLE IF NOT EXISTS profiles (
    name                 TEXT PRIMARY KEY,
    from_path            TEXT NOT NULL DEFAULT '',
    to_path              TEXT NOT NULL DEFAULT '',
    included_paths       TEXT NOT NULL DEFAULT '[]',
    excluded_paths       TEXT NOT NULL DEFAULT '[]',
    bandwidth            INTEGER NOT NULL DEFAULT 0,
    parallel             INTEGER NOT NULL DEFAULT 0,
    backup_path          TEXT NOT NULL DEFAULT '',
    cache_path           TEXT NOT NULL DEFAULT '',
    min_size             TEXT NOT NULL DEFAULT '',
    max_size             TEXT NOT NULL DEFAULT '',
    filter_from_file     TEXT NOT NULL DEFAULT '',
    exclude_if_present   TEXT NOT NULL DEFAULT '',
    use_regex            INTEGER NOT NULL DEFAULT 0,
    max_delete           INTEGER,
    immutable            INTEGER NOT NULL DEFAULT 0,
    conflict_resolution  TEXT NOT NULL DEFAULT '',
    multi_thread_streams INTEGER,
    buffer_size          TEXT NOT NULL DEFAULT '',
    fast_list            INTEGER NOT NULL DEFAULT 0,
    retries              INTEGER,
    low_level_retries    INTEGER,
    max_duration         TEXT NOT NULL DEFAULT ''
);

CREATE TABLE IF NOT EXISTS schedules (
    id           TEXT PRIMARY KEY,
    profile_name TEXT NOT NULL,
    action       TEXT NOT NULL DEFAULT 'push',
    cron_expr    TEXT NOT NULL DEFAULT '',
    enabled      INTEGER NOT NULL DEFAULT 1,
    last_run     TEXT,
    next_run     TEXT,
    last_result  TEXT NOT NULL DEFAULT '',
    created_at   TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE IF NOT EXISTS history (
    id                TEXT PRIMARY KEY,
    profile_name      TEXT NOT NULL DEFAULT '',
    action            TEXT NOT NULL DEFAULT '',
    status            TEXT NOT NULL DEFAULT '',
    start_time        TEXT NOT NULL DEFAULT '',
    end_time          TEXT NOT NULL DEFAULT '',
    duration          TEXT NOT NULL DEFAULT '',
    files_transferred INTEGER NOT NULL DEFAULT 0,
    bytes_transferred INTEGER NOT NULL DEFAULT 0,
    errors            INTEGER NOT NULL DEFAULT 0,
    error_message     TEXT NOT NULL DEFAULT ''
);
CREATE INDEX IF NOT EXISTS idx_history_start_time ON history(start_time DESC);

CREATE TABLE IF NOT EXISTS boards (
    id               TEXT PRIMARY KEY,
    name             TEXT NOT NULL DEFAULT '',
    description      TEXT NOT NULL DEFAULT '',
    created_at       TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at       TEXT NOT NULL DEFAULT (datetime('now')),
    schedule_enabled INTEGER NOT NULL DEFAULT 0,
    cron_expr        TEXT NOT NULL DEFAULT '',
    last_run         TEXT,
    next_run         TEXT,
    last_result      TEXT NOT NULL DEFAULT ''
);

CREATE TABLE IF NOT EXISTS board_nodes (
    id          TEXT NOT NULL,
    board_id    TEXT NOT NULL,
    remote_name TEXT NOT NULL DEFAULT '',
    path        TEXT NOT NULL DEFAULT '',
    label       TEXT NOT NULL DEFAULT '',
    x           REAL NOT NULL DEFAULT 0,
    y           REAL NOT NULL DEFAULT 0,
    PRIMARY KEY (board_id, id),
    FOREIGN KEY (board_id) REFERENCES boards(id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS board_edges (
    id          TEXT NOT NULL,
    board_id    TEXT NOT NULL,
    source_id   TEXT NOT NULL,
    target_id   TEXT NOT NULL,
    action      TEXT NOT NULL DEFAULT 'push',
    sync_config TEXT NOT NULL DEFAULT '{}',
    PRIMARY KEY (board_id, id),
    FOREIGN KEY (board_id) REFERENCES boards(id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS flows (
    id               TEXT PRIMARY KEY,
    name             TEXT NOT NULL DEFAULT '',
    is_collapsed     INTEGER NOT NULL DEFAULT 0,
    schedule_enabled INTEGER NOT NULL DEFAULT 0,
    cron_expr        TEXT NOT NULL DEFAULT '',
    sort_order       INTEGER NOT NULL DEFAULT 0,
    created_at       TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at       TEXT NOT NULL DEFAULT (datetime('now'))
);
CREATE INDEX IF NOT EXISTS idx_flows_sort_order ON flows(sort_order);

CREATE TABLE IF NOT EXISTS operations (
    id            TEXT PRIMARY KEY,
    flow_id       TEXT NOT NULL,
    source_remote TEXT NOT NULL DEFAULT '',
    source_path   TEXT NOT NULL DEFAULT '/',
    target_remote TEXT NOT NULL DEFAULT '',
    target_path   TEXT NOT NULL DEFAULT '/',
    action        TEXT NOT NULL DEFAULT 'push',
    sync_config   TEXT NOT NULL DEFAULT '{}',
    is_expanded   INTEGER NOT NULL DEFAULT 0,
    sort_order    INTEGER NOT NULL DEFAULT 0,
    FOREIGN KEY (flow_id) REFERENCES flows(id) ON DELETE CASCADE
);
CREATE INDEX IF NOT EXISTS idx_operations_flow_id ON operations(flow_id);

CREATE TABLE IF NOT EXISTS delta_state (
    remote_key     TEXT PRIMARY KEY,
    provider       TEXT NOT NULL DEFAULT '',
    is_watching    INTEGER NOT NULL DEFAULT 0,
    last_full_sync TEXT,
    delta_count    INTEGER NOT NULL DEFAULT 0,
    created_at     TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at     TEXT NOT NULL DEFAULT (datetime('now'))
);
`
