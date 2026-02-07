package services

import (
	"database/sql"
	"encoding/json"
	"fmt"
	"log"
	"os"
	"path/filepath"
	"sync"
	"time"

	"desktop/backend/models"

	_ "modernc.org/sqlite"
)

var (
	sharedDB     *sql.DB
	sharedDBOnce sync.Once
	sharedDBErr  error
)

// GetSharedDB returns the singleton database connection to ns-drive.db
func GetSharedDB() (*sql.DB, error) {
	sharedDBOnce.Do(func() {
		cfg := GetSharedConfig()
		if cfg == nil {
			sharedDBErr = fmt.Errorf("shared config not set")
			return
		}

		dbPath := filepath.Join(cfg.ConfigDir, "ns-drive.db")
		if err := os.MkdirAll(filepath.Dir(dbPath), 0755); err != nil {
			sharedDBErr = fmt.Errorf("failed to create db directory: %w", err)
			return
		}

		db, err := sql.Open("sqlite", dbPath)
		if err != nil {
			sharedDBErr = fmt.Errorf("failed to open database: %w", err)
			return
		}

		// Enable WAL mode and foreign keys
		if _, err := db.Exec("PRAGMA journal_mode=WAL"); err != nil {
			db.Close()
			sharedDBErr = fmt.Errorf("failed to set WAL mode: %w", err)
			return
		}
		if _, err := db.Exec("PRAGMA foreign_keys=ON"); err != nil {
			db.Close()
			sharedDBErr = fmt.Errorf("failed to enable foreign keys: %w", err)
			return
		}

		db.SetMaxOpenConns(1) // SQLite single-writer

		sharedDB = db
		log.Printf("Database opened: %s", dbPath)
	})
	return sharedDB, sharedDBErr
}

// InitDatabase initializes the shared database: creates tables and runs migrations.
// Should be called once from main.go after SetSharedConfig.
func InitDatabase() error {
	db, err := GetSharedDB()
	if err != nil {
		return err
	}

	if err := createAllTables(db); err != nil {
		return fmt.Errorf("failed to create tables: %w", err)
	}

	migrateFromJSON(db)
	return nil
}

// CloseDatabase closes the shared database connection.
func CloseDatabase() {
	if sharedDB != nil {
		sharedDB.Close()
	}
}

func createAllTables(db *sql.DB) error {
	_, err := db.Exec(`
		-- App settings (key-value store)
		CREATE TABLE IF NOT EXISTS settings (
			key   TEXT PRIMARY KEY,
			value TEXT NOT NULL DEFAULT ''
		);

		-- Sync profiles
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

		-- Scheduled tasks
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

		-- Operation history (capped at 1000 rows)
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

		-- Boards
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

		-- Flows
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
			id                  TEXT PRIMARY KEY,
			flow_id             TEXT NOT NULL,
			source_remote       TEXT NOT NULL DEFAULT '',
			source_path         TEXT NOT NULL DEFAULT '/',
			target_remote       TEXT NOT NULL DEFAULT '',
			target_path         TEXT NOT NULL DEFAULT '/',
			action              TEXT NOT NULL DEFAULT 'push',
			parallel            INTEGER NOT NULL DEFAULT 0,
			bandwidth           TEXT NOT NULL DEFAULT '',
			included_paths      TEXT NOT NULL DEFAULT '[]',
			excluded_paths      TEXT NOT NULL DEFAULT '[]',
			conflict_resolution TEXT NOT NULL DEFAULT '',
			dry_run             INTEGER NOT NULL DEFAULT 0,
			is_expanded         INTEGER NOT NULL DEFAULT 0,
			sort_order          INTEGER NOT NULL DEFAULT 0,
			FOREIGN KEY (flow_id) REFERENCES flows(id) ON DELETE CASCADE
		);
		CREATE INDEX IF NOT EXISTS idx_operations_flow_id ON operations(flow_id);
	`)
	return err
}

// ============ JSON Migrations ============

func migrateFromJSON(db *sql.DB) {
	cfg := GetSharedConfig()
	if cfg == nil {
		return
	}

	migrateSettings(db, cfg)
	migrateProfiles(db, cfg)
	migrateSchedules(db, cfg)
	migrateHistory(db, cfg)
	migrateBoards(db, cfg)
	migrateFlowsDB(db, cfg)
}

func migrateSettings(db *sql.DB, cfg *SharedConfig) {
	var count int
	if err := db.QueryRow("SELECT COUNT(*) FROM settings").Scan(&count); err != nil || count > 0 {
		return
	}

	filePath := filepath.Join(cfg.ConfigDir, "settings.json")
	data, err := os.ReadFile(filePath)
	if err != nil {
		return
	}

	var settings struct {
		NotificationsEnabled bool `json:"notifications_enabled"`
		DebugMode            bool `json:"debug_mode"`
		MinimizeToTray       bool `json:"minimize_to_tray"`
		StartAtLogin         bool `json:"start_at_login"`
	}
	if err := json.Unmarshal(data, &settings); err != nil {
		log.Printf("Warning: failed to parse settings.json for migration: %v", err)
		return
	}

	tx, err := db.Begin()
	if err != nil {
		return
	}
	defer tx.Rollback()

	stmt, err := tx.Prepare("INSERT INTO settings (key, value) VALUES (?, ?)")
	if err != nil {
		return
	}
	defer stmt.Close()

	stmt.Exec("notifications_enabled", boolToStr(settings.NotificationsEnabled))
	stmt.Exec("debug_mode", boolToStr(settings.DebugMode))
	stmt.Exec("minimize_to_tray", boolToStr(settings.MinimizeToTray))
	stmt.Exec("start_at_login", boolToStr(settings.StartAtLogin))

	if err := tx.Commit(); err == nil {
		log.Printf("Migrated settings from settings.json")
	}
}

func migrateProfiles(db *sql.DB, cfg *SharedConfig) {
	var count int
	if err := db.QueryRow("SELECT COUNT(*) FROM profiles").Scan(&count); err != nil || count > 0 {
		return
	}

	filePath := filepath.Join(cfg.ConfigDir, "profiles.json")
	data, err := os.ReadFile(filePath)
	if err != nil {
		return
	}

	var profiles []models.Profile
	if err := json.Unmarshal(data, &profiles); err != nil {
		log.Printf("Warning: failed to parse profiles.json for migration: %v", err)
		return
	}

	if len(profiles) == 0 {
		return
	}

	tx, err := db.Begin()
	if err != nil {
		return
	}
	defer tx.Rollback()

	stmt, err := tx.Prepare(`INSERT INTO profiles (name, from_path, to_path, included_paths, excluded_paths,
		bandwidth, parallel, backup_path, cache_path, min_size, max_size, filter_from_file,
		exclude_if_present, use_regex, max_delete, immutable, conflict_resolution,
		multi_thread_streams, buffer_size, fast_list, retries, low_level_retries, max_duration)
		VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)`)
	if err != nil {
		return
	}
	defer stmt.Close()

	for _, p := range profiles {
		stmt.Exec(p.Name, p.From, p.To,
			marshalStringSlice(p.IncludedPaths), marshalStringSlice(p.ExcludedPaths),
			p.Bandwidth, p.Parallel, p.BackupPath, p.CachePath,
			p.MinSize, p.MaxSize, p.FilterFromFile, p.ExcludeIfPresent,
			boolToInt(p.UseRegex), intPtrToNullable(p.MaxDelete), boolToInt(p.Immutable),
			p.ConflictResolution, intPtrToNullable(p.MultiThreadStreams),
			p.BufferSize, boolToInt(p.FastList),
			intPtrToNullable(p.Retries), intPtrToNullable(p.LowLevelRetries), p.MaxDuration)
	}

	if err := tx.Commit(); err == nil {
		log.Printf("Migrated %d profiles from profiles.json", len(profiles))
	}
}

func migrateSchedules(db *sql.DB, cfg *SharedConfig) {
	var count int
	if err := db.QueryRow("SELECT COUNT(*) FROM schedules").Scan(&count); err != nil || count > 0 {
		return
	}

	filePath := filepath.Join(cfg.ConfigDir, "schedules.json")
	data, err := os.ReadFile(filePath)
	if err != nil {
		return
	}

	var schedules []models.ScheduleEntry
	if err := json.Unmarshal(data, &schedules); err != nil {
		log.Printf("Warning: failed to parse schedules.json for migration: %v", err)
		return
	}

	if len(schedules) == 0 {
		return
	}

	tx, err := db.Begin()
	if err != nil {
		return
	}
	defer tx.Rollback()

	stmt, err := tx.Prepare(`INSERT INTO schedules (id, profile_name, action, cron_expr, enabled, last_run, next_run, last_result, created_at)
		VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)`)
	if err != nil {
		return
	}
	defer stmt.Close()

	for _, s := range schedules {
		stmt.Exec(s.Id, s.ProfileName, s.Action, s.CronExpr, boolToInt(s.Enabled),
			timePtrToNullable(s.LastRun), timePtrToNullable(s.NextRun),
			s.LastResult, s.CreatedAt.UTC().Format(time.RFC3339))
	}

	if err := tx.Commit(); err == nil {
		log.Printf("Migrated %d schedules from schedules.json", len(schedules))
	}
}

func migrateHistory(db *sql.DB, cfg *SharedConfig) {
	var count int
	if err := db.QueryRow("SELECT COUNT(*) FROM history").Scan(&count); err != nil || count > 0 {
		return
	}

	filePath := filepath.Join(cfg.ConfigDir, "history.json")
	data, err := os.ReadFile(filePath)
	if err != nil {
		return
	}

	var entries []models.HistoryEntry
	if err := json.Unmarshal(data, &entries); err != nil {
		log.Printf("Warning: failed to parse history.json for migration: %v", err)
		return
	}

	if len(entries) == 0 {
		return
	}

	tx, err := db.Begin()
	if err != nil {
		return
	}
	defer tx.Rollback()

	stmt, err := tx.Prepare(`INSERT INTO history (id, profile_name, action, status, start_time, end_time, duration, files_transferred, bytes_transferred, errors, error_message)
		VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)`)
	if err != nil {
		return
	}
	defer stmt.Close()

	for _, e := range entries {
		stmt.Exec(e.Id, e.ProfileName, e.Action, e.Status,
			e.StartTime.UTC().Format(time.RFC3339), e.EndTime.UTC().Format(time.RFC3339),
			e.Duration, e.FilesTransferred, e.BytesTransferred, e.Errors, e.ErrorMessage)
	}

	if err := tx.Commit(); err == nil {
		log.Printf("Migrated %d history entries from history.json", len(entries))
	}
}

func migrateBoards(db *sql.DB, cfg *SharedConfig) {
	var count int
	if err := db.QueryRow("SELECT COUNT(*) FROM boards").Scan(&count); err != nil || count > 0 {
		return
	}

	filePath := filepath.Join(cfg.ConfigDir, "boards.json")
	data, err := os.ReadFile(filePath)
	if err != nil {
		return
	}

	var boards []models.Board
	if err := json.Unmarshal(data, &boards); err != nil {
		log.Printf("Warning: failed to parse boards.json for migration: %v", err)
		return
	}

	if len(boards) == 0 {
		return
	}

	tx, err := db.Begin()
	if err != nil {
		return
	}
	defer tx.Rollback()

	boardStmt, err := tx.Prepare(`INSERT INTO boards (id, name, description, created_at, updated_at, schedule_enabled, cron_expr, last_run, next_run, last_result)
		VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)`)
	if err != nil {
		return
	}
	defer boardStmt.Close()

	nodeStmt, err := tx.Prepare(`INSERT INTO board_nodes (id, board_id, remote_name, path, label, x, y)
		VALUES (?, ?, ?, ?, ?, ?, ?)`)
	if err != nil {
		return
	}
	defer nodeStmt.Close()

	edgeStmt, err := tx.Prepare(`INSERT INTO board_edges (id, board_id, source_id, target_id, action, sync_config)
		VALUES (?, ?, ?, ?, ?, ?)`)
	if err != nil {
		return
	}
	defer edgeStmt.Close()

	for _, b := range boards {
		// Skip the __flows__ board (already migrated via FlowService)
		if b.Name == "__flows__" {
			continue
		}

		boardStmt.Exec(b.Id, b.Name, b.Description,
			b.CreatedAt.UTC().Format(time.RFC3339), b.UpdatedAt.UTC().Format(time.RFC3339),
			boolToInt(b.ScheduleEnabled), b.CronExpr,
			timePtrToNullable(b.LastRun), timePtrToNullable(b.NextRun), b.LastResult)

		for _, n := range b.Nodes {
			nodeStmt.Exec(n.Id, b.Id, n.RemoteName, n.Path, n.Label, n.X, n.Y)
		}

		for _, e := range b.Edges {
			syncConfigJSON, _ := json.Marshal(e.SyncConfig)
			edgeStmt.Exec(e.Id, b.Id, e.SourceId, e.TargetId, e.Action, string(syncConfigJSON))
		}
	}

	if err := tx.Commit(); err == nil {
		log.Printf("Migrated %d boards from boards.json", len(boards))
	}
}

func migrateFlowsDB(db *sql.DB, cfg *SharedConfig) {
	var count int
	if err := db.QueryRow("SELECT COUNT(*) FROM flows").Scan(&count); err != nil || count > 0 {
		return
	}

	oldDBPath := filepath.Join(cfg.ConfigDir, "flows.db")
	if _, err := os.Stat(oldDBPath); os.IsNotExist(err) {
		return
	}

	oldDB, err := sql.Open("sqlite", oldDBPath)
	if err != nil {
		log.Printf("Warning: failed to open old flows.db for migration: %v", err)
		return
	}
	defer oldDB.Close()

	// Read flows from old DB
	rows, err := oldDB.Query("SELECT id, name, is_collapsed, schedule_enabled, cron_expr, sort_order, created_at, updated_at FROM flows ORDER BY sort_order")
	if err != nil {
		log.Printf("Warning: failed to read flows from old DB: %v", err)
		return
	}

	tx, err := db.Begin()
	if err != nil {
		rows.Close()
		return
	}
	defer tx.Rollback()

	flowStmt, err := tx.Prepare(`INSERT INTO flows (id, name, is_collapsed, schedule_enabled, cron_expr, sort_order, created_at, updated_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?)`)
	if err != nil {
		rows.Close()
		return
	}
	defer flowStmt.Close()

	var flowCount int
	for rows.Next() {
		var id, name, cronExpr, createdAt, updatedAt string
		var isCollapsed, scheduleEnabled, sortOrder int
		if err := rows.Scan(&id, &name, &isCollapsed, &scheduleEnabled, &cronExpr, &sortOrder, &createdAt, &updatedAt); err != nil {
			continue
		}
		flowStmt.Exec(id, name, isCollapsed, scheduleEnabled, cronExpr, sortOrder, createdAt, updatedAt)
		flowCount++
	}
	rows.Close()

	// Read operations from old DB
	opRows, err := oldDB.Query("SELECT id, flow_id, source_remote, source_path, target_remote, target_path, action, parallel, bandwidth, included_paths, excluded_paths, conflict_resolution, dry_run, is_expanded, sort_order FROM operations ORDER BY sort_order")
	if err != nil {
		log.Printf("Warning: failed to read operations from old DB: %v", err)
		return
	}

	opStmt, err := tx.Prepare(`INSERT INTO operations (id, flow_id, source_remote, source_path, target_remote, target_path, action, parallel, bandwidth, included_paths, excluded_paths, conflict_resolution, dry_run, is_expanded, sort_order) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)`)
	if err != nil {
		opRows.Close()
		return
	}
	defer opStmt.Close()

	for opRows.Next() {
		var id, flowId, sourceRemote, sourcePath, targetRemote, targetPath, action, bandwidth, includedPaths, excludedPaths, conflictResolution string
		var parallel, dryRun, isExpanded, sortOrder int
		if err := opRows.Scan(&id, &flowId, &sourceRemote, &sourcePath, &targetRemote, &targetPath, &action, &parallel, &bandwidth, &includedPaths, &excludedPaths, &conflictResolution, &dryRun, &isExpanded, &sortOrder); err != nil {
			continue
		}
		opStmt.Exec(id, flowId, sourceRemote, sourcePath, targetRemote, targetPath, action, parallel, bandwidth, includedPaths, excludedPaths, conflictResolution, dryRun, isExpanded, sortOrder)
	}
	opRows.Close()

	if err := tx.Commit(); err == nil {
		log.Printf("Migrated %d flows from flows.db", flowCount)
		// Remove old flows.db
		os.Remove(oldDBPath)
		os.Remove(oldDBPath + "-wal")
		os.Remove(oldDBPath + "-shm")
	}
}

// ============ Helpers ============

func boolToStr(b bool) string {
	if b {
		return "true"
	}
	return "false"
}

func boolToInt(b bool) int {
	if b {
		return 1
	}
	return 0
}

func intPtrToNullable(p *int) interface{} {
	if p == nil {
		return nil
	}
	return *p
}

func timePtrToNullable(t *time.Time) interface{} {
	if t == nil {
		return nil
	}
	return t.UTC().Format(time.RFC3339)
}
