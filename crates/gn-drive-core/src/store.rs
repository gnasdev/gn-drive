//! SQLite persistence and repositories (port of `Store.swift`).
//!
//! Same schema as the Swift/Go versions so existing `gn-drive.db` files load
//! unchanged.

use crate::database::{Database, DatabaseError, SqlValue, Tx};
use crate::logger::Logger;
use crate::models::{
    Board, BoardEdge, BoardNode, DIRECTION_PUSH, DeltaState, Flow, FlowOperation, HistoryEntry,
    HistoryStats, Profile, ProfileStats, Schedule, profile_direction,
};
use serde_json::Value;

#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum StoreError {
    #[error("record not found")]
    NotFound,
    #[error("{0}")]
    Invalid(String),
    #[error("{0}")]
    Db(String),
}

impl From<DatabaseError> for StoreError {
    fn from(e: DatabaseError) -> Self {
        match e {
            DatabaseError::NotFound => StoreError::NotFound,
            other => StoreError::Db(other.to_string()),
        }
    }
}

type Result<T, E = StoreError> = std::result::Result<T, E>;

pub struct Store {
    db: Database,
    log: Logger,
}

const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS settings (
    key   TEXT PRIMARY KEY,
    value TEXT NOT NULL DEFAULT ''
);
CREATE TABLE IF NOT EXISTS profiles (
    name                 TEXT PRIMARY KEY,
    from_path            TEXT NOT NULL DEFAULT '',
    to_path              TEXT NOT NULL DEFAULT '',
    direction            TEXT NOT NULL DEFAULT '',
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
    updated_at       TEXT NOT NULL DEFAULT (datetime('now')),
    canvas_json      TEXT NOT NULL DEFAULT '{}'
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
";

const PROFILE_ALTERS: &[&str] = &[
    "ALTER TABLE profiles ADD COLUMN max_age TEXT NOT NULL DEFAULT ''",
    "ALTER TABLE profiles ADD COLUMN min_age TEXT NOT NULL DEFAULT ''",
    "ALTER TABLE profiles ADD COLUMN max_depth INTEGER",
    "ALTER TABLE profiles ADD COLUMN delete_excluded INTEGER NOT NULL DEFAULT 0",
    "ALTER TABLE profiles ADD COLUMN dry_run INTEGER NOT NULL DEFAULT 0",
    "ALTER TABLE profiles ADD COLUMN max_transfer TEXT NOT NULL DEFAULT ''",
    "ALTER TABLE profiles ADD COLUMN max_delete_size TEXT NOT NULL DEFAULT ''",
    "ALTER TABLE profiles ADD COLUMN suffix TEXT NOT NULL DEFAULT ''",
    "ALTER TABLE profiles ADD COLUMN suffix_keep_extension INTEGER NOT NULL DEFAULT 0",
    "ALTER TABLE profiles ADD COLUMN check_first INTEGER NOT NULL DEFAULT 0",
    "ALTER TABLE profiles ADD COLUMN order_by TEXT NOT NULL DEFAULT ''",
    "ALTER TABLE profiles ADD COLUMN retries_sleep TEXT NOT NULL DEFAULT ''",
    "ALTER TABLE profiles ADD COLUMN tps_limit REAL",
    "ALTER TABLE profiles ADD COLUMN conn_timeout TEXT NOT NULL DEFAULT ''",
    "ALTER TABLE profiles ADD COLUMN io_timeout TEXT NOT NULL DEFAULT ''",
    "ALTER TABLE profiles ADD COLUMN size_only INTEGER NOT NULL DEFAULT 0",
    "ALTER TABLE profiles ADD COLUMN update_mode INTEGER NOT NULL DEFAULT 0",
    "ALTER TABLE profiles ADD COLUMN ignore_existing INTEGER NOT NULL DEFAULT 0",
    "ALTER TABLE profiles ADD COLUMN delete_timing TEXT NOT NULL DEFAULT ''",
    "ALTER TABLE profiles ADD COLUMN resilient INTEGER NOT NULL DEFAULT 0",
    "ALTER TABLE profiles ADD COLUMN max_lock TEXT NOT NULL DEFAULT ''",
    "ALTER TABLE profiles ADD COLUMN check_access INTEGER NOT NULL DEFAULT 0",
    "ALTER TABLE profiles ADD COLUMN conflict_loser TEXT NOT NULL DEFAULT ''",
    "ALTER TABLE profiles ADD COLUMN conflict_suffix TEXT NOT NULL DEFAULT ''",
    "ALTER TABLE profiles ADD COLUMN direction TEXT NOT NULL DEFAULT ''",
];

const PROFILE_COLUMNS: &str = "name, from_path, to_path, direction, included_paths, excluded_paths, bandwidth, parallel, backup_path, cache_path, min_size, max_size, filter_from_file, exclude_if_present, use_regex, max_delete, immutable, conflict_resolution, multi_thread_streams, buffer_size, fast_list, retries, low_level_retries, max_duration, max_age, min_age, max_depth, delete_excluded, dry_run, max_transfer, max_delete_size, suffix, suffix_keep_extension, check_first, order_by, retries_sleep, tps_limit, conn_timeout, io_timeout, size_only, update_mode, ignore_existing, delete_timing, resilient, max_lock, check_access, conflict_loser, conflict_suffix";

const SCHEMA_VERSION: i64 = 1;

const HISTORY_COLUMNS: &str = "id, profile_name, action, status, start_time, end_time, duration,
                   files_transferred, bytes_transferred, errors, error_message";

const FLOW_COLUMNS: &str = "id, name, is_collapsed, schedule_enabled, cron_expr, sort_order, created_at, updated_at, canvas_json";

fn t(s: &str) -> SqlValue {
    SqlValue::Text(s.to_string())
}
fn i(v: i64) -> SqlValue {
    SqlValue::Int(v)
}
fn b(v: bool) -> SqlValue {
    SqlValue::from_bool(v)
}
fn text_or_null(s: &str) -> SqlValue {
    if s.is_empty() { SqlValue::Null } else { t(s) }
}

impl Store {
    /// Open (creating parent dirs) and migrate the database at `path`.
    pub fn open(path: &str, log: &Logger) -> Result<Store> {
        let db = Database::open(path)?;
        let store = Store {
            db,
            log: log.clone(),
        };
        store.migrate()?;
        log.info("database opened", &[("path", path)]);
        Ok(store)
    }

    /// Raw database access (rare).
    pub fn db(&self) -> &Database {
        &self.db
    }

    pub fn close(&self) {
        self.db.close();
    }

    // --- Schema --------------------------------------------------------------

    fn migrate(&self) -> Result<()> {
        for stmt in SCHEMA.split(";\n") {
            let sql = stmt.trim();
            if !sql.is_empty() {
                self.db.exec(sql, &[])?;
            }
        }
        for ddl in PROFILE_ALTERS {
            let _ = self.db.exec(ddl, &[]); // duplicate column → ignore
        }
        let _ = self.db.exec(
            "ALTER TABLE flows ADD COLUMN canvas_json TEXT NOT NULL DEFAULT '{}'",
            &[],
        );
        self.apply_migrations(&[(1, "")])
    }

    fn apply_migrations(&self, migrations: &[(i64, &str)]) -> Result<()> {
        let row = self.db.query_row("PRAGMA user_version", &[])?;
        let mut current = row.and_then(|r| r.first().map(SqlValue::int)).unwrap_or(0);
        for (version, sql) in migrations {
            if *version > current {
                if !sql.is_empty() {
                    self.db.exec(sql, &[])?;
                }
                self.db
                    .exec(&format!("PRAGMA user_version = {version}"), &[])?;
                current = *version;
                self.log.info(
                    "store: applied migration",
                    &[("version", &version.to_string())],
                );
            }
        }
        debug_assert!(current >= SCHEMA_VERSION);
        Ok(())
    }

    // --- Profiles ------------------------------------------------------------

    pub fn list_profiles(&self) -> Result<Vec<Profile>> {
        let rows = self.db.query(
            &format!("SELECT {PROFILE_COLUMNS} FROM profiles ORDER BY name"),
            &[],
        )?;
        Ok(rows.iter().map(|r| scan_profile(r)).collect())
    }

    pub fn get_profile(&self, name: &str) -> Result<Profile> {
        let row = self
            .db
            .query_row(
                &format!("SELECT {PROFILE_COLUMNS} FROM profiles WHERE name = ?"),
                &[t(name)],
            )?
            .ok_or(StoreError::NotFound)?;
        Ok(scan_profile(&row))
    }

    pub fn save_profile(&self, p: &mut Profile) -> Result<()> {
        if p.name.is_empty() {
            return Err(StoreError::Invalid("profile: name is required".into()));
        }
        // Swift trims `.whitespaces` (not newlines); ProfileDirection.isValid trims both.
        let mut dir = p.direction.trim_matches([' ', '\t']).to_string();
        if dir.is_empty() {
            dir = DIRECTION_PUSH.to_string();
        }
        if !profile_direction::is_valid(&dir) {
            return Err(StoreError::Invalid(format!(
                "profile: invalid direction {}",
                p.direction
            )));
        }
        p.direction = dir;
        let placeholders = vec!["?"; 48].join(",");
        let sql = format!(
            "INSERT INTO profiles ({PROFILE_COLUMNS}) VALUES (
            {placeholders})
            ON CONFLICT(name) DO UPDATE SET
              from_path=excluded.from_path, to_path=excluded.to_path, direction=excluded.direction,
              included_paths=excluded.included_paths, excluded_paths=excluded.excluded_paths,
              bandwidth=excluded.bandwidth, parallel=excluded.parallel, backup_path=excluded.backup_path,
              cache_path=excluded.cache_path, min_size=excluded.min_size, max_size=excluded.max_size,
              filter_from_file=excluded.filter_from_file, exclude_if_present=excluded.exclude_if_present,
              use_regex=excluded.use_regex, max_delete=excluded.max_delete, immutable=excluded.immutable,
              conflict_resolution=excluded.conflict_resolution,
              multi_thread_streams=excluded.multi_thread_streams, buffer_size=excluded.buffer_size,
              fast_list=excluded.fast_list, retries=excluded.retries,
              low_level_retries=excluded.low_level_retries, max_duration=excluded.max_duration,
              max_age=excluded.max_age, min_age=excluded.min_age, max_depth=excluded.max_depth,
              delete_excluded=excluded.delete_excluded, dry_run=excluded.dry_run,
              max_transfer=excluded.max_transfer, max_delete_size=excluded.max_delete_size,
              suffix=excluded.suffix, suffix_keep_extension=excluded.suffix_keep_extension,
              check_first=excluded.check_first, order_by=excluded.order_by,
              retries_sleep=excluded.retries_sleep, tps_limit=excluded.tps_limit,
              conn_timeout=excluded.conn_timeout, io_timeout=excluded.io_timeout,
              size_only=excluded.size_only, update_mode=excluded.update_mode,
              ignore_existing=excluded.ignore_existing, delete_timing=excluded.delete_timing,
              resilient=excluded.resilient, max_lock=excluded.max_lock,
              check_access=excluded.check_access, conflict_loser=excluded.conflict_loser,
              conflict_suffix=excluded.conflict_suffix"
        );
        let args = [
            t(&p.name),
            t(&p.from),
            t(&p.to),
            t(&p.direction),
            t(&json_string(&p.included_paths)),
            t(&json_string(&p.excluded_paths)),
            i(p.bandwidth),
            i(p.parallel),
            t(&p.backup_path),
            t(&p.cache_path),
            t(&p.min_size),
            t(&p.max_size),
            t(&p.filter_from_file),
            t(&p.exclude_if_present),
            b(p.use_regex),
            SqlValue::from_opt_int(p.max_delete),
            b(p.immutable),
            t(&p.conflict_resolution),
            SqlValue::from_opt_int(p.multi_thread_streams),
            t(&p.buffer_size),
            b(p.fast_list),
            SqlValue::from_opt_int(p.retries),
            SqlValue::from_opt_int(p.low_level_retries),
            t(&p.max_duration),
            t(&p.max_age),
            t(&p.min_age),
            SqlValue::from_opt_int(p.max_depth),
            b(p.delete_excluded),
            b(p.dry_run),
            t(&p.max_transfer),
            t(&p.max_delete_size),
            t(&p.suffix),
            b(p.suffix_keep_extension),
            b(p.check_first),
            t(&p.order_by),
            t(&p.retries_sleep),
            SqlValue::from_opt_double(p.tps_limit),
            t(&p.conn_timeout),
            t(&p.io_timeout),
            b(p.size_only),
            b(p.update_mode),
            b(p.ignore_existing),
            t(&p.delete_timing),
            b(p.resilient),
            t(&p.max_lock),
            b(p.check_access),
            t(&p.conflict_loser),
            t(&p.conflict_suffix),
        ];
        self.db.exec(&sql, &args)?;
        Ok(())
    }

    pub fn delete_profile(&self, name: &str) -> Result<()> {
        self.delete_one("DELETE FROM profiles WHERE name = ?", name)
    }

    fn delete_one(&self, sql: &str, key: &str) -> Result<()> {
        if self.db.exec(sql, &[t(key)])? == 0 {
            return Err(StoreError::NotFound);
        }
        Ok(())
    }

    // --- Schedules -----------------------------------------------------------

    pub fn list_schedules(&self) -> Result<Vec<Schedule>> {
        let rows = self.db.query(
            "SELECT id, profile_name, action, cron_expr, enabled, last_run, next_run, last_result, created_at
            FROM schedules ORDER BY created_at DESC",
            &[],
        )?;
        Ok(rows
            .iter()
            .map(|r| Schedule {
                id: r[0].text(),
                profile_name: r[1].text(),
                action: r[2].text(),
                cron: r[3].text(),
                enabled: r[4].int() != 0,
                last_run: r[5].text(),
                next_run: r[6].text(),
                last_result: r[7].text(),
                created_at: r[8].text(),
            })
            .collect())
    }

    pub fn save_schedule(&self, s: &Schedule) -> Result<()> {
        if s.id.is_empty() {
            return Err(StoreError::Invalid("schedule: id is required".into()));
        }
        self.db.exec(
            "INSERT INTO schedules (id, profile_name, action, cron_expr, enabled, last_run, next_run, last_result)
            VALUES (?, ?, ?, ?, ?, ?, ?, ?)
            ON CONFLICT(id) DO UPDATE SET
              profile_name=excluded.profile_name, action=excluded.action,
              cron_expr=excluded.cron_expr, enabled=excluded.enabled,
              last_run=excluded.last_run, next_run=excluded.next_run,
              last_result=excluded.last_result",
            &[
                t(&s.id),
                t(&s.profile_name),
                t(&s.action),
                t(&s.cron),
                b(s.enabled),
                text_or_null(&s.last_run),
                text_or_null(&s.next_run),
                t(&s.last_result),
            ],
        )?;
        Ok(())
    }

    pub fn delete_schedule(&self, id: &str) -> Result<()> {
        self.delete_one("DELETE FROM schedules WHERE id = ?", id)
    }

    // --- History -------------------------------------------------------------

    pub fn list_history(&self, limit: i64, offset: i64) -> Result<Vec<HistoryEntry>> {
        let rows = self.db.query(
            &format!(
                "SELECT {HISTORY_COLUMNS} FROM history ORDER BY start_time DESC LIMIT ? OFFSET ?"
            ),
            &[i(limit), i(offset)],
        )?;
        Ok(rows.iter().map(|r| scan_history(r)).collect())
    }

    pub fn list_history_for_profile(
        &self,
        profile: &str,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<HistoryEntry>> {
        let rows = self.db.query(
            &format!(
                "SELECT {HISTORY_COLUMNS} FROM history WHERE profile_name = ? ORDER BY start_time DESC LIMIT ? OFFSET ?"
            ),
            &[t(profile), i(limit), i(offset)],
        )?;
        Ok(rows.iter().map(|r| scan_history(r)).collect())
    }

    pub fn save_history(&self, e: &HistoryEntry) -> Result<()> {
        if e.id.is_empty() {
            return Err(StoreError::Invalid("history: id is required".into()));
        }
        self.db.exec(
            "INSERT INTO history (id, profile_name, action, status, start_time, end_time, duration,
                                 files_transferred, bytes_transferred, errors, error_message)
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            ON CONFLICT(id) DO UPDATE SET
              status=excluded.status, end_time=excluded.end_time, duration=excluded.duration,
              files_transferred=excluded.files_transferred, bytes_transferred=excluded.bytes_transferred,
              errors=excluded.errors, error_message=excluded.error_message",
            &[
                t(&e.id),
                t(&e.profile_name),
                t(&e.action),
                t(&e.state),
                t(&e.started_at),
                t(&e.finished_at),
                t(&e.duration.to_string()),
                i(e.files),
                i(e.bytes),
                i(e.errors),
                t(&e.error_message),
            ],
        )?;
        Ok(())
    }

    pub fn clear_history(&self) -> Result<()> {
        self.db.exec("DELETE FROM history", &[])?;
        Ok(())
    }

    pub fn history_stats(&self) -> Result<HistoryStats> {
        let mut stats = HistoryStats::default();
        if let Some(row) = self.db.query_row(
            "SELECT COUNT(*), COALESCE(SUM(bytes_transferred),0), COALESCE(SUM(CAST(duration AS INTEGER)),0), COALESCE(SUM(errors),0)
            FROM history",
            &[],
        )? {
            stats.total_syncs = row[0].int();
            stats.total_bytes = row[1].int();
            stats.total_duration = row[2].int();
            stats.total_errors = row[3].int();
        }
        let rows = self.db.query(
            "SELECT profile_name, COUNT(*), COALESCE(SUM(bytes_transferred),0),
                   COALESCE(SUM(CAST(duration AS INTEGER)),0), COALESCE(SUM(errors),0)
            FROM history GROUP BY profile_name",
            &[],
        )?;
        for row in rows {
            stats.by_profile.insert(
                row[0].text(),
                ProfileStats {
                    syncs: row[1].int(),
                    bytes: row[2].int(),
                    duration: row[3].int(),
                    errors: row[4].int(),
                },
            );
        }
        Ok(stats)
    }

    // --- Settings ------------------------------------------------------------

    pub fn get_setting(&self, key: &str) -> Result<String> {
        let row = self
            .db
            .query_row("SELECT value FROM settings WHERE key = ?", &[t(key)])?
            .ok_or(StoreError::NotFound)?;
        Ok(row[0].text())
    }

    pub fn set_setting(&self, key: &str, value: &str) -> Result<()> {
        self.db.exec(
            "INSERT INTO settings (key, value) VALUES (?, ?)
            ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            &[t(key), t(value)],
        )?;
        Ok(())
    }

    pub fn get_setting_bool(&self, key: &str, default: bool) -> bool {
        match self.get_setting(key) {
            Ok(v) => v == "true" || v == "1",
            Err(_) => default,
        }
    }

    // --- Boards --------------------------------------------------------------

    pub fn list_boards(&self) -> Result<Vec<Board>> {
        let rows = self.db.query(
            "SELECT id, name, created_at, updated_at, schedule_enabled, cron_expr
            FROM boards ORDER BY updated_at DESC",
            &[],
        )?;
        Ok(rows.iter().map(|r| scan_board(r)).collect())
    }

    pub fn get_board(&self, id: &str) -> Result<Board> {
        let row = self
            .db
            .query_row(
                "SELECT id, name, created_at, updated_at, schedule_enabled, cron_expr
            FROM boards WHERE id = ?",
                &[t(id)],
            )?
            .ok_or(StoreError::NotFound)?;
        Ok(scan_board(&row))
    }

    pub fn load_board_graph(&self, id: &str) -> Result<Board> {
        let mut board = self.get_board(id)?;
        let nrows = self.db.query(
            "SELECT id, remote_name, path, label, x, y FROM board_nodes WHERE board_id = ? ORDER BY id",
            &[t(id)],
        )?;
        board.nodes = nrows
            .iter()
            .map(|r| BoardNode {
                id: r[0].text(),
                remote_name: r[1].text(),
                path: r[2].text(),
                label: r[3].text(),
                x: r[4].double(),
                y: r[5].double(),
            })
            .collect();
        let erows = self.db.query(
            "SELECT id, source_id, target_id, action, sync_config FROM board_edges WHERE board_id = ? ORDER BY id",
            &[t(id)],
        )?;
        board.edges = erows
            .iter()
            .map(|r| {
                let cfg = r[4].text();
                BoardEdge {
                    id: r[0].text(),
                    source_id: r[1].text(),
                    target_id: r[2].text(),
                    action: r[3].text(),
                    sync_config: if cfg.is_empty() { "{}".into() } else { cfg },
                }
            })
            .collect();
        Ok(board)
    }

    pub fn save_board_graph(&self, board: &Board) -> Result<()> {
        self.db.transaction::<_, StoreError>(|tx: &Tx<'_>| {
            tx.exec(
                "INSERT INTO boards (id, name, schedule_enabled, cron_expr, updated_at)
                VALUES (?, ?, ?, ?, datetime('now'))
                ON CONFLICT(id) DO UPDATE SET
                  name=excluded.name, schedule_enabled=excluded.schedule_enabled,
                  cron_expr=excluded.cron_expr, updated_at=datetime('now')",
                &[t(&board.id), t(&board.name), i(0), t("")],
            )?;
            tx.exec("DELETE FROM board_nodes WHERE board_id = ?", &[t(&board.id)])?;
            for n in &board.nodes {
                tx.exec(
                    "INSERT INTO board_nodes (id, board_id, remote_name, path, label, x, y)
                    VALUES (?, ?, ?, ?, ?, ?, ?)",
                    &[
                        t(&n.id),
                        t(&board.id),
                        t(&n.remote_name),
                        t(&n.path),
                        t(&n.label),
                        SqlValue::Double(n.x),
                        SqlValue::Double(n.y),
                    ],
                )?;
            }
            tx.exec("DELETE FROM board_edges WHERE board_id = ?", &[t(&board.id)])?;
            for e in &board.edges {
                let cfg = if e.sync_config.is_empty() { "{}" } else { e.sync_config.as_str() };
                tx.exec(
                    "INSERT INTO board_edges (id, board_id, source_id, target_id, action, sync_config)
                    VALUES (?, ?, ?, ?, ?, ?)",
                    &[t(&e.id), t(&board.id), t(&e.source_id), t(&e.target_id), t(&e.action), t(cfg)],
                )?;
            }
            Ok(())
        })
    }

    pub fn delete_board(&self, id: &str) -> Result<()> {
        self.delete_one("DELETE FROM boards WHERE id = ?", id)
    }

    // --- Flows ---------------------------------------------------------------

    pub fn list_flows(&self) -> Result<Vec<Flow>> {
        let rows = self.db.query(
            &format!("SELECT {FLOW_COLUMNS} FROM flows ORDER BY sort_order, name"),
            &[],
        )?;
        let mut flows: Vec<Flow> = rows.iter().map(|r| scan_flow(r)).collect();
        for f in &mut flows {
            f.operations = self.list_operations(&f.id)?;
        }
        Ok(flows)
    }

    pub fn get_flow(&self, id: &str) -> Result<Flow> {
        let row = self
            .db
            .query_row(
                &format!("SELECT {FLOW_COLUMNS} FROM flows WHERE id = ?"),
                &[t(id)],
            )?
            .ok_or(StoreError::NotFound)?;
        let mut f = scan_flow(&row);
        f.operations = self.list_operations(&f.id)?;
        Ok(f)
    }

    pub fn list_operations(&self, flow_id: &str) -> Result<Vec<FlowOperation>> {
        let rows = self.db.query(
            "SELECT id, flow_id, source_remote, source_path, target_remote, target_path,
                   action, sync_config, is_expanded, sort_order
            FROM operations WHERE flow_id = ? ORDER BY sort_order, id",
            &[t(flow_id)],
        )?;
        Ok(rows
            .iter()
            .map(|r| {
                let mut op = FlowOperation {
                    id: r[0].text(),
                    flow_id: r[1].text(),
                    source_remote: r[2].text(),
                    source_path: r[3].text(),
                    target_remote: r[4].text(),
                    target_path: r[5].text(),
                    action: r[6].text(),
                    is_expanded: r[8].int() != 0,
                    sort_order: r[9].int(),
                    ..FlowOperation::default()
                };
                let cfg = r[7].text();
                if !cfg.is_empty()
                    && let Ok(Value::Object(obj)) = serde_json::from_str::<Value>(&cfg)
                {
                    op.sync_config = obj;
                }
                op.normalize_action();
                op
            })
            .collect())
    }

    /// Upsert a flow and replace its operations (Wails SaveFlows semantics).
    pub fn save_flow(&self, f: &mut Flow) -> Result<()> {
        if f.id.is_empty() {
            return Err(StoreError::Invalid("flow: id is required".into()));
        }
        let canvas = if f.canvas_json.is_empty() {
            "{}"
        } else {
            f.canvas_json.as_str()
        };
        self.db.transaction::<_, StoreError>(|tx| {
            tx.exec(
                "INSERT INTO flows (id, name, is_collapsed, schedule_enabled, cron_expr, sort_order, created_at, updated_at, canvas_json)
                VALUES (?, ?, ?, ?, ?, ?, COALESCE(NULLIF(?, ''), datetime('now')), datetime('now'), ?)
                ON CONFLICT(id) DO UPDATE SET
                  name=excluded.name, is_collapsed=excluded.is_collapsed,
                  schedule_enabled=excluded.schedule_enabled, cron_expr=excluded.cron_expr,
                  sort_order=excluded.sort_order, updated_at=datetime('now'),
                  canvas_json=excluded.canvas_json",
                &[
                    t(&f.id),
                    t(&f.name),
                    b(f.is_collapsed),
                    b(f.schedule_enabled),
                    t(&f.schedule_cron),
                    i(f.sort_order),
                    t(&f.created_at),
                    t(canvas),
                ],
            )?;
            tx.exec("DELETE FROM operations WHERE flow_id = ?", &[t(&f.id)])?;
            for (idx, src) in f.operations.iter().enumerate() {
                if src.id.is_empty() {
                    return Err(StoreError::Invalid("flow: operation id is required".into()));
                }
                let mut op = src.clone();
                op.flow_id = f.id.clone();
                if op.sort_order == 0 {
                    op.sort_order = idx as i64;
                }
                op.normalize_action();
                let cfg = serde_json::to_string(&op.sync_config).unwrap_or_else(|_| "{}".into());
                tx.exec(
                    "INSERT INTO operations
                    (id, flow_id, source_remote, source_path, target_remote, target_path, action, sync_config, is_expanded, sort_order)
                    VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
                    &[
                        t(&op.id),
                        t(&f.id),
                        t(&op.source_remote),
                        t(if op.source_path.is_empty() { "/" } else { &op.source_path }),
                        t(&op.target_remote),
                        t(if op.target_path.is_empty() { "/" } else { &op.target_path }),
                        t(&op.action),
                        t(&cfg),
                        b(op.is_expanded),
                        i(op.sort_order),
                    ],
                )?;
            }
            Ok(())
        })
    }

    pub fn delete_flow(&self, id: &str) -> Result<()> {
        self.delete_one("DELETE FROM flows WHERE id = ?", id)
    }

    // --- Delta state ---------------------------------------------------------

    pub fn get_delta_state(&self, remote_key: &str) -> Result<DeltaState> {
        let row = self
            .db
            .query_row(
                "SELECT remote_key, provider, last_full_sync, delta_count, is_watching
            FROM delta_state WHERE remote_key = ?",
                &[t(remote_key)],
            )?
            .ok_or(StoreError::NotFound)?;
        Ok(DeltaState {
            remote_key: row[0].text(),
            provider: row[1].text(),
            last_full_sync: row[2].text(),
            delta_count: row[3].int(),
            is_watching: row[4].int() != 0,
        })
    }

    pub fn record_full_sync(&self, remote_key: &str, provider: &str) -> Result<()> {
        let now = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
        self.db.exec(
            "INSERT INTO delta_state (remote_key, provider, last_full_sync, delta_count, is_watching)
            VALUES (?, ?, ?, 0, 0)
            ON CONFLICT(remote_key) DO UPDATE SET
              last_full_sync=excluded.last_full_sync, delta_count=0, is_watching=0",
            &[t(remote_key), t(provider), t(&now)],
        )?;
        Ok(())
    }
}

// --- Row scanners ---------------------------------------------------------------

fn opt_int(v: &SqlValue) -> Option<i64> {
    if v.is_null() { None } else { Some(v.int()) }
}

fn scan_profile(r: &[SqlValue]) -> Profile {
    Profile {
        name: r[0].text(),
        from: r[1].text(),
        to: r[2].text(),
        direction: r[3].text(),
        included_paths: string_slice(&r[4].text()),
        excluded_paths: string_slice(&r[5].text()),
        bandwidth: r[6].int(),
        parallel: r[7].int(),
        backup_path: r[8].text(),
        cache_path: r[9].text(),
        min_size: r[10].text(),
        max_size: r[11].text(),
        filter_from_file: r[12].text(),
        exclude_if_present: r[13].text(),
        use_regex: r[14].int() != 0,
        max_delete: opt_int(&r[15]),
        immutable: r[16].int() != 0,
        conflict_resolution: r[17].text(),
        multi_thread_streams: opt_int(&r[18]),
        buffer_size: r[19].text(),
        fast_list: r[20].int() != 0,
        retries: opt_int(&r[21]),
        low_level_retries: opt_int(&r[22]),
        max_duration: r[23].text(),
        max_age: r[24].text(),
        min_age: r[25].text(),
        max_depth: opt_int(&r[26]),
        delete_excluded: r[27].int() != 0,
        dry_run: r[28].int() != 0,
        max_transfer: r[29].text(),
        max_delete_size: r[30].text(),
        suffix: r[31].text(),
        suffix_keep_extension: r[32].int() != 0,
        check_first: r[33].int() != 0,
        order_by: r[34].text(),
        retries_sleep: r[35].text(),
        tps_limit: if r[36].is_null() {
            None
        } else {
            Some(r[36].double())
        },
        conn_timeout: r[37].text(),
        io_timeout: r[38].text(),
        size_only: r[39].int() != 0,
        update_mode: r[40].int() != 0,
        ignore_existing: r[41].int() != 0,
        delete_timing: r[42].text(),
        resilient: r[43].int() != 0,
        max_lock: r[44].text(),
        check_access: r[45].int() != 0,
        conflict_loser: r[46].text(),
        conflict_suffix: r[47].text(),
    }
}

fn scan_history(r: &[SqlValue]) -> HistoryEntry {
    HistoryEntry {
        id: r[0].text(),
        profile_name: r[1].text(),
        action: r[2].text(),
        state: r[3].text(),
        started_at: r[4].text(),
        finished_at: r[5].text(),
        duration: r[6].text().parse::<i64>().unwrap_or_else(|_| r[6].int()),
        files: r[7].int(),
        bytes: r[8].int(),
        errors: r[9].int(),
        error_message: r[10].text(),
    }
}

fn scan_board(r: &[SqlValue]) -> Board {
    Board {
        id: r[0].text(),
        name: r[1].text(),
        created_at: r[2].text(),
        updated_at: r[3].text(),
        ..Board::default()
    }
}

fn scan_flow(r: &[SqlValue]) -> Flow {
    let canvas = r[8].text();
    Flow {
        id: r[0].text(),
        name: r[1].text(),
        is_collapsed: r[2].int() != 0,
        schedule_enabled: r[3].int() != 0,
        schedule_cron: r[4].text(),
        sort_order: r[5].int(),
        created_at: r[6].text(),
        updated_at: r[7].text(),
        canvas_json: if canvas.is_empty() {
            "{}".into()
        } else {
            canvas
        },
        operations: Vec::new(),
    }
}

// --- JSON helpers ---------------------------------------------------------------

/// Encode a string array as compact JSON (`[]` on failure).
pub fn json_string(arr: &[String]) -> String {
    serde_json::to_string(arr).unwrap_or_else(|_| "[]".into())
}

/// Decode a JSON string array (accepts `\/` escapes); anything else → empty.
pub fn string_slice(s: &str) -> Vec<String> {
    if s.is_empty() {
        return Vec::new();
    }
    serde_json::from_str::<Vec<String>>(s).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;
    use serde_json::json;

    fn open_store() -> (tempfile::TempDir, Store) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("sub").join("gn-drive.db");
        let s = Store::open(path.to_str().unwrap(), &Logger::quiet()).unwrap();
        (dir, s)
    }

    fn sample_profile(name: &str) -> Profile {
        Profile {
            name: name.into(),
            from: "local:/a".into(),
            to: "remote:/b".into(),
            direction: "bi".into(),
            included_paths: vec!["docs/**".into(), "a/b".into()],
            excluded_paths: vec!["*.tmp".into(), "he said \"x\"".into()],
            bandwidth: 100,
            parallel: 4,
            backup_path: "bk".into(),
            cache_path: "cache".into(),
            min_size: "1k".into(),
            max_size: "2G".into(),
            filter_from_file: "f.txt".into(),
            exclude_if_present: ".nosync".into(),
            use_regex: true,
            max_age: "7d".into(),
            min_age: "1h".into(),
            max_depth: Some(3),
            delete_excluded: true,
            max_delete: Some(0),
            immutable: true,
            conflict_resolution: "newer".into(),
            dry_run: true,
            max_transfer: "10G".into(),
            max_delete_size: "5M".into(),
            suffix: ".bak".into(),
            suffix_keep_extension: true,
            multi_thread_streams: Some(0),
            buffer_size: "16M".into(),
            retries: Some(5),
            low_level_retries: Some(0),
            max_duration: "1h".into(),
            check_first: true,
            order_by: "size".into(),
            retries_sleep: "2s".into(),
            tps_limit: Some(0.0),
            conn_timeout: "30s".into(),
            io_timeout: "60s".into(),
            size_only: true,
            update_mode: true,
            ignore_existing: true,
            delete_timing: "after".into(),
            resilient: true,
            max_lock: "15m".into(),
            check_access: true,
            conflict_loser: "delete".into(),
            conflict_suffix: ".conflict".into(),
            fast_list: true,
        }
    }

    // --- open / migrate ---

    #[test]
    fn open_creates_parent_dir_and_stamps_version() {
        let (dir, s) = open_store();
        assert!(dir.path().join("sub/gn-drive.db").exists());
        let v = s
            .db()
            .query_row("PRAGMA user_version", &[])
            .unwrap()
            .unwrap();
        assert_eq!(v[0].int(), SCHEMA_VERSION);
        // Re-migrating is idempotent and keeps the version.
        s.migrate().unwrap();
        let v = s
            .db()
            .query_row("PRAGMA user_version", &[])
            .unwrap()
            .unwrap();
        assert_eq!(v[0].int(), SCHEMA_VERSION);
    }

    #[test]
    fn open_logs_database_opened() {
        let dir = tempfile::tempdir().unwrap();
        let log = Logger::quiet();
        let seen = std::sync::Arc::new(parking_lot::Mutex::new(Vec::<String>::new()));
        let seen2 = seen.clone();
        log.set_sink(Some(std::sync::Arc::new(
            move |r: &crate::logger::LogRecord| {
                seen2.lock().push(r.msg.clone());
            },
        )));
        Store::open(dir.path().join("x.db").to_str().unwrap(), &log).unwrap();
        assert!(seen.lock().iter().any(|m| m == "database opened"));
    }

    #[test]
    fn open_fails_when_parent_is_file() {
        let dir = tempfile::tempdir().unwrap();
        let blocker = dir.path().join("blocker");
        std::fs::write(&blocker, "x").unwrap();
        let r = Store::open(
            blocker.join("gn-drive.db").to_str().unwrap(),
            &Logger::quiet(),
        );
        assert!(matches!(r, Err(StoreError::Db(_))));
    }

    #[test]
    fn apply_migrations_runs_pending_once() {
        let (_d, s) = open_store();
        s.apply_migrations(&[(1, ""), (99, "CREATE TABLE migration_probe (id TEXT)")])
            .unwrap();
        let v = s
            .db()
            .query_row("PRAGMA user_version", &[])
            .unwrap()
            .unwrap();
        assert_eq!(v[0].int(), 99);
        s.db()
            .exec("INSERT INTO migration_probe (id) VALUES ('x')", &[])
            .unwrap();
        // Idempotent: no duplicate-table error.
        s.apply_migrations(&[(1, ""), (99, "CREATE TABLE migration_probe (id TEXT)")])
            .unwrap();
    }

    #[test]
    fn store_is_send_sync() {
        fn assert<T: Send + Sync>() {}
        assert::<Store>();
    }

    #[test]
    fn close_makes_operations_fail() {
        let (_d, s) = open_store();
        s.close();
        assert!(matches!(s.list_profiles(), Err(StoreError::Db(_))));
    }

    // --- profiles ---

    #[test]
    fn profile_round_trips_all_columns() {
        let (_d, s) = open_store();
        let mut p = sample_profile("full");
        s.save_profile(&mut p).unwrap();
        let got = s.get_profile("full").unwrap();
        assert_eq!(got, p);
        assert_eq!(got.max_delete, Some(0));
        assert_eq!(got.multi_thread_streams, Some(0));
        assert_eq!(got.low_level_retries, Some(0));
        assert_eq!(got.tps_limit, Some(0.0));
    }

    #[test]
    fn profile_none_vs_some_zero() {
        let (_d, s) = open_store();
        let mut p = Profile {
            name: "n".into(),
            ..Profile::default()
        };
        s.save_profile(&mut p).unwrap();
        let got = s.get_profile("n").unwrap();
        assert_eq!(got.max_delete, None);
        assert_eq!(got.multi_thread_streams, None);
        assert_eq!(got.retries, None);
        assert_eq!(got.low_level_retries, None);
        assert_eq!(got.max_depth, None);
        assert_eq!(got.tps_limit, None);
        // Saving Some(0) over None updates the NULL.
        let mut z = Profile {
            name: "n".into(),
            max_delete: Some(0),
            max_depth: Some(0),
            tps_limit: Some(0.0),
            ..Profile::default()
        };
        s.save_profile(&mut z).unwrap();
        let got = s.get_profile("n").unwrap();
        assert_eq!(got.max_delete, Some(0));
        assert_eq!(got.max_depth, Some(0));
        assert_eq!(got.tps_limit, Some(0.0));
        // And back to None.
        let mut none = Profile {
            name: "n".into(),
            ..Profile::default()
        };
        s.save_profile(&mut none).unwrap();
        assert_eq!(s.get_profile("n").unwrap().max_delete, None);
    }

    #[test]
    fn profile_direction_normalization_and_validation() {
        let (_d, s) = open_store();
        let mut p = Profile {
            name: "d".into(),
            direction: String::new(),
            ..Profile::default()
        };
        s.save_profile(&mut p).unwrap();
        assert_eq!(p.direction, "push");
        assert_eq!(s.get_profile("d").unwrap().direction, "push");

        let mut p = Profile {
            name: "d".into(),
            direction: "  bi-resync ".into(),
            ..Profile::default()
        };
        s.save_profile(&mut p).unwrap();
        assert_eq!(p.direction, "bi-resync");

        let mut p = Profile {
            name: "d".into(),
            direction: "pull".into(),
            ..Profile::default()
        };
        let err = s.save_profile(&mut p).unwrap_err();
        assert_eq!(
            err,
            StoreError::Invalid("profile: invalid direction pull".into())
        );
        assert_eq!(err.to_string(), "profile: invalid direction pull");
        // Failed save left the previous row alone.
        assert_eq!(s.get_profile("d").unwrap().direction, "bi-resync");
    }

    #[test]
    fn profile_name_required() {
        let (_d, s) = open_store();
        let mut p = Profile::default();
        let err = s.save_profile(&mut p).unwrap_err();
        assert_eq!(err.to_string(), "profile: name is required");
        assert!(matches!(err, StoreError::Invalid(_)));
    }

    #[test]
    fn profile_upsert_list_delete() {
        let (_d, s) = open_store();
        for n in ["zeta", "alpha", "mid"] {
            let mut p = sample_profile(n);
            s.save_profile(&mut p).unwrap();
        }
        let names: Vec<_> = s
            .list_profiles()
            .unwrap()
            .into_iter()
            .map(|p| p.name)
            .collect();
        assert_eq!(names, ["alpha", "mid", "zeta"]);

        let mut p = sample_profile("mid");
        p.from = "changed".into();
        p.max_delete = None;
        s.save_profile(&mut p).unwrap();
        assert_eq!(s.list_profiles().unwrap().len(), 3);
        let got = s.get_profile("mid").unwrap();
        assert_eq!(got.from, "changed");
        assert_eq!(got.max_delete, None);

        s.delete_profile("mid").unwrap();
        assert_eq!(s.get_profile("mid"), Err(StoreError::NotFound));
        assert_eq!(s.delete_profile("mid"), Err(StoreError::NotFound));
        assert_eq!(StoreError::NotFound.to_string(), "record not found");
    }

    #[test]
    fn profile_paths_json_with_slashes_and_unicode() {
        let (_d, s) = open_store();
        let mut p = Profile {
            name: "j".into(),
            included_paths: vec!["a/b/**".into(), "ünï/日本".into()],
            ..Profile::default()
        };
        s.save_profile(&mut p).unwrap();
        assert_eq!(s.get_profile("j").unwrap().included_paths, p.included_paths);
        assert_eq!(
            s.get_profile("j").unwrap().excluded_paths,
            Vec::<String>::new()
        );
    }

    // --- json helpers ---

    #[test]
    fn json_helpers() {
        assert_eq!(json_string(&[]), "[]");
        assert_eq!(json_string(&["a".into(), "b\"c".into()]), r#"["a","b\"c"]"#);
        assert_eq!(string_slice(""), Vec::<String>::new());
        assert_eq!(string_slice("not json"), Vec::<String>::new());
        assert_eq!(string_slice("{\"a\":1}"), Vec::<String>::new());
        assert_eq!(string_slice("[1,2]"), Vec::<String>::new());
        // Swift JSONSerialization writes `/` as `\/`; both forms must parse.
        assert_eq!(
            string_slice(r#"["a\/b","c/d"]"#),
            vec!["a/b".to_string(), "c/d".to_string()]
        );
    }

    // --- schedules ---

    #[test]
    fn schedules_crud() {
        let (_d, s) = open_store();
        assert_eq!(
            s.save_schedule(&Schedule::default())
                .unwrap_err()
                .to_string(),
            "schedule: id is required"
        );

        let sch = Schedule {
            id: "s1".into(),
            profile_name: "p".into(),
            action: "push".into(),
            cron: "0 * * * *".into(),
            enabled: true,
            ..Schedule::default()
        };
        s.save_schedule(&sch).unwrap();
        let list = s.list_schedules().unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].id, "s1");
        assert!(list[0].enabled);
        assert_eq!(list[0].last_run, "");
        assert_eq!(list[0].next_run, "");
        assert!(!list[0].created_at.is_empty());
        // Empty last_run/next_run are stored as NULL.
        let row = s
            .db()
            .query_row(
                "SELECT last_run, next_run FROM schedules WHERE id='s1'",
                &[],
            )
            .unwrap()
            .unwrap();
        assert!(row[0].is_null() && row[1].is_null());

        let created = list[0].created_at.clone();
        let upd = Schedule {
            enabled: false,
            last_run: "2026-01-01T00:00:00Z".into(),
            next_run: "2026-01-02T00:00:00Z".into(),
            last_result: "ok".into(),
            cron: "5 * * * *".into(),
            ..sch.clone()
        };
        s.save_schedule(&upd).unwrap();
        let got = &s.list_schedules().unwrap()[0];
        assert!(!got.enabled);
        assert_eq!(got.last_run, "2026-01-01T00:00:00Z");
        assert_eq!(got.next_run, "2026-01-02T00:00:00Z");
        assert_eq!(got.last_result, "ok");
        assert_eq!(got.cron, "5 * * * *");
        assert_eq!(got.created_at, created);

        s.delete_schedule("s1").unwrap();
        assert_eq!(s.delete_schedule("s1"), Err(StoreError::NotFound));
        assert!(s.list_schedules().unwrap().is_empty());
    }

    // --- history ---

    fn hist(id: &str, profile: &str, start: &str) -> HistoryEntry {
        HistoryEntry {
            id: id.into(),
            profile_name: profile.into(),
            action: "push".into(),
            state: "running".into(),
            started_at: start.into(),
            ..HistoryEntry::default()
        }
    }

    #[test]
    fn history_id_required() {
        let (_d, s) = open_store();
        assert_eq!(
            s.save_history(&HistoryEntry::default())
                .unwrap_err()
                .to_string(),
            "history: id is required"
        );
    }

    #[test]
    fn history_paging_filter_and_order() {
        let (_d, s) = open_store();
        for n in 0..5 {
            let p = if n % 2 == 0 { "even" } else { "odd" };
            s.save_history(&hist(
                &format!("h{n}"),
                p,
                &format!("2026-01-0{}T00:00:00Z", n + 1),
            ))
            .unwrap();
        }
        let ids = |v: Vec<HistoryEntry>| v.into_iter().map(|e| e.id).collect::<Vec<_>>();
        assert_eq!(
            ids(s.list_history(10, 0).unwrap()),
            ["h4", "h3", "h2", "h1", "h0"]
        );
        assert_eq!(ids(s.list_history(2, 0).unwrap()), ["h4", "h3"]);
        assert_eq!(ids(s.list_history(2, 2).unwrap()), ["h2", "h1"]);
        assert_eq!(ids(s.list_history(2, 4).unwrap()), ["h0"]);
        assert!(s.list_history(2, 10).unwrap().is_empty());
        assert_eq!(
            ids(s.list_history_for_profile("even", 10, 0).unwrap()),
            ["h4", "h2", "h0"]
        );
        assert_eq!(
            ids(s.list_history_for_profile("even", 1, 1).unwrap()),
            ["h2"]
        );
        assert!(
            s.list_history_for_profile("none", 10, 0)
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn history_upsert_updates_result_columns_only() {
        let (_d, s) = open_store();
        s.save_history(&hist("h", "p", "2026-01-01T00:00:00Z"))
            .unwrap();
        let done = HistoryEntry {
            state: "success".into(),
            finished_at: "2026-01-01T00:01:00Z".into(),
            duration: 60,
            bytes: 1 << 40,
            errors: 2,
            files: 7,
            error_message: "boom".into(),
            // These must NOT change on conflict.
            profile_name: "other".into(),
            action: "bi".into(),
            started_at: "1999".into(),
            ..hist("h", "p", "")
        };
        s.save_history(&done).unwrap();
        let all = s.list_history(10, 0).unwrap();
        assert_eq!(all.len(), 1);
        let e = &all[0];
        assert_eq!(e.profile_name, "p");
        assert_eq!(e.action, "push");
        assert_eq!(e.started_at, "2026-01-01T00:00:00Z");
        assert_eq!(e.state, "success");
        assert_eq!(e.finished_at, "2026-01-01T00:01:00Z");
        assert_eq!(
            (e.duration, e.bytes, e.errors, e.files),
            (60, 1 << 40, 2, 7)
        );
        assert_eq!(e.error_message, "boom");
        // Duration is stored as TEXT.
        let row = s
            .db()
            .query_row("SELECT typeof(duration), duration FROM history", &[])
            .unwrap()
            .unwrap();
        assert_eq!(row[0].text(), "text");
        assert_eq!(row[1].text(), "60");
    }

    #[test]
    fn history_stats_and_clear() {
        let (_d, s) = open_store();
        assert_eq!(s.history_stats().unwrap(), HistoryStats::default());
        let mk = |id: &str, p: &str, bytes, dur, errs| HistoryEntry {
            bytes,
            duration: dur,
            errors: errs,
            ..hist(id, p, id)
        };
        s.save_history(&mk("1", "a", 100, 10, 0)).unwrap();
        s.save_history(&mk("2", "a", 50, 5, 1)).unwrap();
        s.save_history(&mk("3", "b", 7, 3, 2)).unwrap();
        let st = s.history_stats().unwrap();
        assert_eq!(
            (
                st.total_syncs,
                st.total_bytes,
                st.total_duration,
                st.total_errors
            ),
            (3, 157, 18, 3)
        );
        assert_eq!(st.by_profile.len(), 2);
        assert_eq!(
            st.by_profile["a"],
            ProfileStats {
                syncs: 2,
                bytes: 150,
                duration: 15,
                errors: 1
            }
        );
        assert_eq!(
            st.by_profile["b"],
            ProfileStats {
                syncs: 1,
                bytes: 7,
                duration: 3,
                errors: 2
            }
        );
        s.clear_history().unwrap();
        assert!(s.list_history(10, 0).unwrap().is_empty());
        assert_eq!(s.history_stats().unwrap().total_syncs, 0);
    }

    #[test]
    fn history_legacy_non_numeric_duration_reads_as_zero() {
        let (_d, s) = open_store();
        s.db()
            .exec(
                "INSERT INTO history (id, duration) VALUES ('x', '1m30s')",
                &[],
            )
            .unwrap();
        s.db()
            .exec("INSERT INTO history (id, duration) VALUES ('y', '42')", &[])
            .unwrap();
        let all = s.list_history(10, 0).unwrap();
        let by = |id: &str| all.iter().find(|e| e.id == id).unwrap().duration;
        assert_eq!(by("x"), 0);
        assert_eq!(by("y"), 42);
        // CAST('1m30s' AS INTEGER) = 1 in SQLite
        assert_eq!(s.history_stats().unwrap().total_duration, 43);
    }

    // --- settings ---

    #[test]
    fn settings() {
        let (_d, s) = open_store();
        assert_eq!(s.get_setting("k"), Err(StoreError::NotFound));
        s.set_setting("k", "v1").unwrap();
        s.set_setting("k", "v2").unwrap();
        assert_eq!(s.get_setting("k").unwrap(), "v2");
        s.set_setting("empty", "").unwrap();
        assert_eq!(s.get_setting("empty").unwrap(), "");
    }

    #[test]
    fn setting_bool() {
        let (_d, s) = open_store();
        assert!(s.get_setting_bool("missing", true));
        assert!(!s.get_setting_bool("missing", false));
        for (v, want) in [
            ("true", true),
            ("1", true),
            ("false", false),
            ("0", false),
            ("TRUE", false),
            ("", false),
        ] {
            s.set_setting("b", v).unwrap();
            assert_eq!(s.get_setting_bool("b", !want), want, "value {v:?}");
        }
    }

    // --- boards ---

    fn sample_board(id: &str) -> Board {
        Board {
            id: id.into(),
            name: "B".into(),
            nodes: vec![
                BoardNode {
                    id: "n2".into(),
                    remote_name: "r".into(),
                    path: "/p".into(),
                    label: "L2".into(),
                    x: 10.5,
                    y: -3.25,
                },
                BoardNode {
                    id: "n1".into(),
                    remote_name: "".into(),
                    path: "/".into(),
                    label: "L1".into(),
                    x: 0.0,
                    y: 1.0,
                },
            ],
            edges: vec![
                BoardEdge {
                    id: "e2".into(),
                    source_id: "n1".into(),
                    target_id: "n2".into(),
                    action: "bi".into(),
                    sync_config: r#"{"a":1}"#.into(),
                },
                BoardEdge {
                    id: "e1".into(),
                    source_id: "n2".into(),
                    target_id: "n1".into(),
                    action: "push".into(),
                    sync_config: String::new(),
                },
            ],
            ..Board::default()
        }
    }

    #[test]
    fn board_graph_round_trip() {
        let (_d, s) = open_store();
        assert_eq!(s.get_board("b1"), Err(StoreError::NotFound));
        assert_eq!(s.load_board_graph("b1"), Err(StoreError::NotFound));
        s.save_board_graph(&sample_board("b1")).unwrap();
        let g = s.load_board_graph("b1").unwrap();
        assert_eq!(g.id, "b1");
        assert_eq!(g.name, "B");
        assert!(!g.created_at.is_empty() && !g.updated_at.is_empty());
        assert_eq!(
            g.nodes.iter().map(|n| n.id.as_str()).collect::<Vec<_>>(),
            ["n1", "n2"]
        );
        assert_eq!(g.nodes[1].x, 10.5);
        assert_eq!(g.nodes[1].y, -3.25);
        assert_eq!(g.nodes[1].label, "L2");
        assert_eq!(
            g.edges.iter().map(|e| e.id.as_str()).collect::<Vec<_>>(),
            ["e1", "e2"]
        );
        assert_eq!(g.edges[0].sync_config, "{}"); // empty → {}
        assert_eq!(g.edges[1].sync_config, r#"{"a":1}"#);
        assert_eq!(g.edges[1].action, "bi");
        // get_board / list_boards do not load the graph.
        assert!(s.get_board("b1").unwrap().nodes.is_empty());
        assert_eq!(s.list_boards().unwrap().len(), 1);
    }

    #[test]
    fn board_save_replaces_graph() {
        let (_d, s) = open_store();
        s.save_board_graph(&sample_board("b1")).unwrap();
        let mut b = sample_board("b1");
        b.name = "Renamed".into();
        b.nodes.truncate(1);
        b.edges.clear();
        s.save_board_graph(&b).unwrap();
        let g = s.load_board_graph("b1").unwrap();
        assert_eq!(g.name, "Renamed");
        assert_eq!(g.nodes.len(), 1);
        assert!(g.edges.is_empty());
        assert_eq!(s.list_boards().unwrap().len(), 1);
    }

    #[test]
    fn board_save_is_atomic() {
        let (_d, s) = open_store();
        s.save_board_graph(&sample_board("b1")).unwrap();
        let mut bad = sample_board("b1");
        bad.name = "Bad".into();
        bad.nodes.push(bad.nodes[0].clone()); // duplicate PK (board_id, id)
        assert!(matches!(s.save_board_graph(&bad), Err(StoreError::Db(_))));
        let g = s.load_board_graph("b1").unwrap();
        assert_eq!(g.name, "B");
        assert_eq!(g.nodes.len(), 2);
        assert_eq!(g.edges.len(), 2);
        // Connection still usable after rollback.
        s.save_board_graph(&sample_board("b2")).unwrap();
    }

    #[test]
    fn board_delete_cascades() {
        let (_d, s) = open_store();
        s.save_board_graph(&sample_board("b1")).unwrap();
        s.save_board_graph(&sample_board("b2")).unwrap();
        s.delete_board("b1").unwrap();
        assert_eq!(s.delete_board("b1"), Err(StoreError::NotFound));
        let count = |tbl: &str, id: &str| {
            s.db()
                .query_row(
                    &format!("SELECT COUNT(*) FROM {tbl} WHERE board_id = ?"),
                    &[t(id)],
                )
                .unwrap()
                .unwrap()[0]
                .int()
        };
        assert_eq!(count("board_nodes", "b1"), 0);
        assert_eq!(count("board_edges", "b1"), 0);
        assert_eq!(count("board_nodes", "b2"), 2);
        assert_eq!(count("board_edges", "b2"), 2);
    }

    // --- flows ---

    fn op(id: &str, action: &str) -> FlowOperation {
        FlowOperation {
            id: id.into(),
            action: action.into(),
            source_remote: "s".into(),
            target_remote: "t".into(),
            ..FlowOperation::default()
        }
    }

    #[test]
    fn flow_validation_errors() {
        let (_d, s) = open_store();
        let mut f = Flow::default();
        assert_eq!(
            s.save_flow(&mut f).unwrap_err().to_string(),
            "flow: id is required"
        );
        let mut f = Flow {
            id: "f".into(),
            operations: vec![op("", "push")],
            ..Flow::default()
        };
        assert_eq!(
            s.save_flow(&mut f).unwrap_err().to_string(),
            "flow: operation id is required"
        );
        // Rolled back: no flow row left behind.
        assert_eq!(s.get_flow("f"), Err(StoreError::NotFound));
    }

    #[test]
    fn flow_round_trip_and_operation_rules() {
        let (_d, s) = open_store();
        let mut cfg = op("o2", "bi");
        cfg.sync_config.insert("bandwidth".into(), json!(5));
        cfg.sync_config
            .insert("includedPaths".into(), json!(["a/b", "c"]));
        cfg.source_path = String::new();
        cfg.target_path = "/dst".into();
        cfg.is_expanded = true;
        let mut f = Flow {
            id: "f1".into(),
            name: "Flow".into(),
            is_collapsed: true,
            schedule_enabled: true,
            schedule_cron: "0 0 * * *".into(),
            sort_order: 3,
            canvas_json: String::new(),
            operations: vec![op("o1", "weird-legacy"), cfg, op("o3", "")],
            ..Flow::default()
        };
        s.save_flow(&mut f).unwrap();
        let got = s.get_flow("f1").unwrap();
        assert_eq!(got.name, "Flow");
        assert!(got.is_collapsed && got.schedule_enabled);
        assert_eq!(got.schedule_cron, "0 0 * * *");
        assert_eq!(got.sort_order, 3);
        assert_eq!(got.canvas_json, "{}");
        assert!(!got.created_at.is_empty() && !got.updated_at.is_empty());
        // sort_order 0 → index; ops come back ordered by sort_order, id.
        let ids: Vec<_> = got
            .operations
            .iter()
            .map(|o| (o.id.as_str(), o.sort_order))
            .collect();
        assert_eq!(ids, [("o1", 0), ("o2", 1), ("o3", 2)]);
        assert!(got.operations.iter().all(|o| o.flow_id == "f1"));
        // Legacy action normalized to push; sync_config.action aligned.
        assert_eq!(got.operations[0].action, "push");
        assert_eq!(got.operations[0].sync_config["action"], json!("push"));
        assert_eq!(got.operations[2].action, "push");
        let o2 = &got.operations[1];
        assert_eq!(o2.action, "bi");
        assert_eq!(o2.source_path, "/"); // empty → "/"
        assert_eq!(o2.target_path, "/dst");
        assert!(o2.is_expanded);
        assert_eq!(o2.sync_config["bandwidth"], json!(5));
        assert_eq!(o2.sync_config["includedPaths"], json!(["a/b", "c"]));
        assert_eq!(o2.sync_config["action"], json!("bi"));
    }

    #[test]
    fn flow_action_resolved_from_sync_config() {
        let (_d, s) = open_store();
        let mut o = op("o1", "");
        o.sync_config.insert("action".into(), json!("bi-resync"));
        let mut f = Flow {
            id: "f".into(),
            operations: vec![o],
            ..Flow::default()
        };
        s.save_flow(&mut f).unwrap();
        assert_eq!(s.get_flow("f").unwrap().operations[0].action, "bi-resync");
    }

    #[test]
    fn flow_explicit_sort_order_and_replace_operations() {
        let (_d, s) = open_store();
        let mut a = op("a", "push");
        a.sort_order = 5;
        let mut b = op("b", "push");
        b.sort_order = 2;
        let mut f = Flow {
            id: "f".into(),
            name: "F".into(),
            operations: vec![a, b, op("c", "push")],
            ..Flow::default()
        };
        s.save_flow(&mut f).unwrap();
        let ids: Vec<_> = s
            .list_operations("f")
            .unwrap()
            .into_iter()
            .map(|o| o.id)
            .collect();
        assert_eq!(ids, ["b", "c", "a"]); // b=2, c=index 2 (tie → by id), a=5
    }

    #[test]
    fn flow_resave_replaces_operations_keeps_created_at() {
        let (_d, s) = open_store();
        let mut f = Flow {
            id: "f".into(),
            created_at: "2020-01-01 00:00:00".into(),
            operations: vec![op("o1", "push"), op("o2", "push")],
            ..Flow::default()
        };
        s.save_flow(&mut f).unwrap();
        let first = s.get_flow("f").unwrap();
        assert_eq!(first.created_at, "2020-01-01 00:00:00");

        let mut f2 = Flow {
            id: "f".into(),
            name: "New".into(),
            operations: vec![op("o3", "bi")],
            ..Flow::default()
        };
        s.save_flow(&mut f2).unwrap();
        let got = s.get_flow("f").unwrap();
        assert_eq!(got.name, "New");
        assert_eq!(got.created_at, "2020-01-01 00:00:00");
        assert_eq!(got.operations.len(), 1);
        assert_eq!(got.operations[0].id, "o3");
        assert_eq!(s.list_operations("f").unwrap().len(), 1);
    }

    #[test]
    fn flow_list_order_and_delete_cascade() {
        let (_d, s) = open_store();
        for (id, name, so) in [("f1", "b", 1), ("f2", "a", 1), ("f3", "z", 0)] {
            let mut f = Flow {
                id: id.into(),
                name: name.into(),
                sort_order: so,
                operations: vec![op(&format!("{id}-o"), "push")],
                ..Flow::default()
            };
            s.save_flow(&mut f).unwrap();
        }
        let flows = s.list_flows().unwrap();
        assert_eq!(
            flows.iter().map(|f| f.id.as_str()).collect::<Vec<_>>(),
            ["f3", "f2", "f1"]
        );
        assert!(flows.iter().all(|f| f.operations.len() == 1));

        s.delete_flow("f1").unwrap();
        assert_eq!(s.delete_flow("f1"), Err(StoreError::NotFound));
        assert_eq!(s.get_flow("f1"), Err(StoreError::NotFound));
        let n = s
            .db()
            .query_row("SELECT COUNT(*) FROM operations WHERE flow_id='f1'", &[])
            .unwrap()
            .unwrap()[0]
            .int();
        assert_eq!(n, 0);
        assert_eq!(s.list_flows().unwrap().len(), 2);
    }

    #[test]
    fn flow_canvas_json_preserved() {
        let (_d, s) = open_store();
        let mut f = Flow {
            id: "f".into(),
            canvas_json: r#"{"zoom":2}"#.into(),
            ..Flow::default()
        };
        s.save_flow(&mut f).unwrap();
        assert_eq!(s.get_flow("f").unwrap().canvas_json, r#"{"zoom":2}"#);
    }

    // --- delta state ---

    #[test]
    fn delta_state() {
        let (_d, s) = open_store();
        assert_eq!(s.get_delta_state("k"), Err(StoreError::NotFound));
        s.record_full_sync("k", "drive").unwrap();
        let d = s.get_delta_state("k").unwrap();
        assert_eq!(d.remote_key, "k");
        assert_eq!(d.provider, "drive");
        assert_eq!(d.delta_count, 0);
        assert!(!d.is_watching);
        // RFC3339, second precision, UTC `Z`.
        let ts = &d.last_full_sync;
        assert_eq!(ts.len(), 20, "{ts}");
        assert!(ts.ends_with('Z') && ts.as_bytes()[10] == b'T');
        assert!(chrono::DateTime::parse_from_rfc3339(ts).is_ok());

        // Re-recording resets counters but keeps the provider.
        s.db()
            .exec(
                "UPDATE delta_state SET delta_count = 9, is_watching = 1, last_full_sync = NULL",
                &[],
            )
            .unwrap();
        s.record_full_sync("k", "other").unwrap();
        let d = s.get_delta_state("k").unwrap();
        assert_eq!(
            (d.delta_count, d.is_watching, d.provider.as_str()),
            (0, false, "drive")
        );
        assert!(!d.last_full_sync.is_empty());
    }

    #[test]
    fn delta_state_null_last_full_sync_reads_empty() {
        let (_d, s) = open_store();
        s.db()
            .exec("INSERT INTO delta_state (remote_key) VALUES ('n')", &[])
            .unwrap();
        assert_eq!(s.get_delta_state("n").unwrap().last_full_sync, "");
    }

    // --- Swift-written / legacy databases ---

    /// Pre-ALTER `profiles` table (24 columns, as in the first Go release),
    /// plus a flows table without `canvas_json`, with NULLs in nullable columns.
    fn build_legacy_db(path: &str) {
        let c = Connection::open(path).unwrap();
        c.execute_batch(
            "CREATE TABLE profiles (
                name TEXT PRIMARY KEY,
                from_path TEXT NOT NULL DEFAULT '',
                to_path TEXT NOT NULL DEFAULT '',
                direction TEXT NOT NULL DEFAULT '',
                included_paths TEXT NOT NULL DEFAULT '[]',
                excluded_paths TEXT NOT NULL DEFAULT '[]',
                bandwidth INTEGER NOT NULL DEFAULT 0,
                parallel INTEGER NOT NULL DEFAULT 0,
                backup_path TEXT NOT NULL DEFAULT '',
                cache_path TEXT NOT NULL DEFAULT '',
                min_size TEXT NOT NULL DEFAULT '',
                max_size TEXT NOT NULL DEFAULT '',
                filter_from_file TEXT NOT NULL DEFAULT '',
                exclude_if_present TEXT NOT NULL DEFAULT '',
                use_regex INTEGER NOT NULL DEFAULT 0,
                max_delete INTEGER,
                immutable INTEGER NOT NULL DEFAULT 0,
                conflict_resolution TEXT NOT NULL DEFAULT '',
                multi_thread_streams INTEGER,
                buffer_size TEXT NOT NULL DEFAULT '',
                fast_list INTEGER NOT NULL DEFAULT 0,
                retries INTEGER,
                low_level_retries INTEGER,
                max_duration TEXT NOT NULL DEFAULT ''
            );
            INSERT INTO profiles (name, from_path, to_path, direction, included_paths, bandwidth, max_delete, retries, fast_list)
                VALUES ('legacy', 'a', 'b', 'pull', '[\"x\\/y\",\"z\"]', 12, NULL, 0, 1);
            CREATE TABLE flows (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL DEFAULT '',
                is_collapsed INTEGER NOT NULL DEFAULT 0,
                schedule_enabled INTEGER NOT NULL DEFAULT 0,
                cron_expr TEXT NOT NULL DEFAULT '',
                sort_order INTEGER NOT NULL DEFAULT 0,
                created_at TEXT NOT NULL DEFAULT (datetime('now')),
                updated_at TEXT NOT NULL DEFAULT (datetime('now'))
            );
            INSERT INTO flows (id, name) VALUES ('lf', 'Legacy flow');
            CREATE TABLE operations (
                id TEXT PRIMARY KEY,
                flow_id TEXT NOT NULL,
                source_remote TEXT NOT NULL DEFAULT '',
                source_path TEXT NOT NULL DEFAULT '/',
                target_remote TEXT NOT NULL DEFAULT '',
                target_path TEXT NOT NULL DEFAULT '/',
                action TEXT NOT NULL DEFAULT 'push',
                sync_config TEXT NOT NULL DEFAULT '{}',
                is_expanded INTEGER NOT NULL DEFAULT 0,
                sort_order INTEGER NOT NULL DEFAULT 0,
                FOREIGN KEY (flow_id) REFERENCES flows(id) ON DELETE CASCADE
            );
            INSERT INTO operations (id, flow_id, action, sync_config, sort_order)
                VALUES ('lo', 'lf', '', '{\"action\":\"bi\",\"path\":\"a\\/b\"}', 0);
            CREATE TABLE schedules (
                id TEXT PRIMARY KEY, profile_name TEXT NOT NULL, action TEXT NOT NULL DEFAULT 'push',
                cron_expr TEXT NOT NULL DEFAULT '', enabled INTEGER NOT NULL DEFAULT 1,
                last_run TEXT, next_run TEXT, last_result TEXT NOT NULL DEFAULT '',
                created_at TEXT NOT NULL DEFAULT (datetime('now'))
            );
            INSERT INTO schedules (id, profile_name) VALUES ('ls', 'legacy');
            PRAGMA user_version = 0;",
        )
        .unwrap();
    }

    #[test]
    fn legacy_database_migrates_and_reads() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("gn-drive.db");
        build_legacy_db(path.to_str().unwrap());

        let s = Store::open(path.to_str().unwrap(), &Logger::quiet()).unwrap();
        let v = s
            .db()
            .query_row("PRAGMA user_version", &[])
            .unwrap()
            .unwrap();
        assert_eq!(v[0].int(), 1);

        // All 48 columns exist after ALTERs.
        let cols = s.db().query("PRAGMA table_info(profiles)", &[]).unwrap();
        assert_eq!(cols.len(), 48);

        let p = s.get_profile("legacy").unwrap();
        assert_eq!(p.from, "a");
        assert_eq!(p.to, "b");
        assert_eq!(p.direction, "pull"); // read as-is
        assert_eq!(p.included_paths, vec!["x/y".to_string(), "z".to_string()]);
        assert_eq!(p.bandwidth, 12);
        assert_eq!(p.max_delete, None);
        assert_eq!(p.retries, Some(0));
        assert!(p.fast_list);
        // Later-added columns come back as defaults / None.
        assert_eq!(p.max_depth, None);
        assert_eq!(p.tps_limit, None);
        assert_eq!(p.max_age, "");
        assert!(!p.dry_run && !p.resilient && !p.check_access);

        // Re-saving a legacy profile with an invalid direction is rejected; fixing it works.
        let mut p = p;
        assert!(s.save_profile(&mut p).is_err());
        p.direction = String::new();
        s.save_profile(&mut p).unwrap();
        assert_eq!(s.get_profile("legacy").unwrap().direction, "push");

        // Flows gained canvas_json.
        let f = s.get_flow("lf").unwrap();
        assert_eq!(f.canvas_json, "{}");
        assert_eq!(f.name, "Legacy flow");
        assert_eq!(f.schedule_cron, "");
        assert_eq!(f.operations.len(), 1);
        let o = &f.operations[0];
        assert_eq!(o.action, "bi"); // resolved from sync_config
        assert_eq!(o.sync_config["path"], json!("a/b"));

        let sch = s.list_schedules().unwrap();
        assert_eq!(sch[0].last_run, "");
        assert!(sch[0].enabled);

        // Tables that did not exist before are created and usable.
        s.save_board_graph(&sample_board("b")).unwrap();
        s.record_full_sync("k", "p").unwrap();
        // Opening a second time is a no-op migration.
        drop(s);
        let s = Store::open(path.to_str().unwrap(), &Logger::quiet()).unwrap();
        assert_eq!(s.list_profiles().unwrap().len(), 1);
    }

    /// A database as the Swift app would write it: full schema, NULLs in
    /// nullable columns, `\/`-escaped JSON, REAL stored for tps_limit, and
    /// numeric history duration stored as TEXT.
    #[test]
    fn swift_written_database_reads_unchanged() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("gn-drive.db");
        // Create via our own schema, then overwrite rows with raw rusqlite as Swift would.
        drop(Store::open(path.to_str().unwrap(), &Logger::quiet()).unwrap());
        {
            let c = Connection::open(&path).unwrap();
            c.execute_batch(
                "INSERT INTO profiles (name, from_path, to_path, direction, included_paths, excluded_paths,
                    max_delete, multi_thread_streams, retries, low_level_retries, max_depth, tps_limit, dry_run)
                 VALUES ('sw', 'l:/a', 'r:/b', 'bi', '[\"a\\/b\\/**\"]', '[]', 0, NULL, 3, NULL, NULL, 2.5, 1);
                 INSERT INTO history VALUES ('h1','sw','push','success','2026-02-01T00:00:00Z','2026-02-01T00:00:09Z','9',3,1024,0,'');
                 INSERT INTO settings VALUES ('autostart','1');
                 INSERT INTO boards (id, name) VALUES ('b','Board');
                 INSERT INTO board_nodes VALUES ('n','b','r','/p','L',1,2.5);
                 INSERT INTO board_edges (id, board_id, source_id, target_id, sync_config) VALUES ('e','b','n','n','');
                 INSERT INTO delta_state (remote_key, provider, last_full_sync, delta_count, is_watching)
                    VALUES ('rk','drive','2026-02-01T00:00:00Z',4,1);",
            )
            .unwrap();
        }
        let s = Store::open(path.to_str().unwrap(), &Logger::quiet()).unwrap();
        let p = s.get_profile("sw").unwrap();
        assert_eq!(p.included_paths, vec!["a/b/**".to_string()]);
        assert_eq!(
            (
                p.max_delete,
                p.multi_thread_streams,
                p.retries,
                p.low_level_retries
            ),
            (Some(0), None, Some(3), None)
        );
        assert_eq!(
            (p.max_depth, p.tps_limit, p.dry_run),
            (None, Some(2.5), true)
        );
        let h = &s.list_history(10, 0).unwrap()[0];
        assert_eq!((h.duration, h.files, h.bytes, h.errors), (9, 3, 1024, 0));
        assert!(s.get_setting_bool("autostart", false));
        let g = s.load_board_graph("b").unwrap();
        assert_eq!(g.nodes[0].y, 2.5);
        assert_eq!(g.edges[0].sync_config, "{}");
        assert_eq!(g.edges[0].action, "push");
        let d = s.get_delta_state("rk").unwrap();
        assert_eq!(
            (d.delta_count, d.is_watching, d.last_full_sync.as_str()),
            (4, true, "2026-02-01T00:00:00Z")
        );
    }

    #[test]
    fn concurrent_use_is_serialized() {
        let (_d, s) = open_store();
        let s = std::sync::Arc::new(s);
        let handles: Vec<_> = (0..4)
            .map(|n| {
                let s = s.clone();
                std::thread::spawn(move || {
                    for k in 0..5 {
                        s.set_setting(&format!("k{n}-{k}"), "v").unwrap();
                    }
                })
            })
            .collect();
        for h in handles {
            h.join().unwrap();
        }
        let n = s
            .db()
            .query_row("SELECT COUNT(*) FROM settings", &[])
            .unwrap()
            .unwrap()[0]
            .int();
        assert_eq!(n, 20);
    }
}
