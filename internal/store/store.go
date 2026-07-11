// Package store provides SQLite persistence and repositories.
//
// Phase 2: full implementation ported from desktop/backend/services/db.go.
// Uses modernc.org/sqlite (pure-Go, no CGo) to keep go.mod minimal.
package store

import (
	"context"
	"database/sql"
	"encoding/json"
	"errors"
	"fmt"
	"log/slog"
	"os"
	"path/filepath"
	"strings"
	"sync"
	"time"

	_ "modernc.org/sqlite"
)

// ErrNotFound is returned when a record is not found.
var ErrNotFound = errors.New("store: record not found")

// Store manages the SQLite database connection and provides repositories.
type Store struct {
	db     *sql.DB
	logger *slog.Logger
	mu     sync.Mutex // serialize schema changes
}

// sqlOpenFn is overridable for tests; defaults to sql.Open.
var sqlOpenFn = func(driver, path string) (*sql.DB, error) {
	return sql.Open(driver, path)
}

// execContextFn is overridable for tests; defaults to (*sql.DB).ExecContext.
var execContextFn = func(db *sql.DB, ctx context.Context, q string) (sql.Result, error) {
	return db.ExecContext(ctx, q)
}

// New opens the SQLite database at the given path, applies migrations,
// and returns a Store with all repositories ready.
func New(ctx context.Context, dbPath string, logger *slog.Logger) (*Store, error) {
	if logger == nil {
		logger = slog.Default()
	}

	if err := os.MkdirAll(filepath.Dir(dbPath), 0o755); err != nil {
		return nil, fmt.Errorf("create db directory: %w", err)
	}

	db, err := sqlOpenFn("sqlite", dbPath)
	if err != nil {
		return nil, fmt.Errorf("open database: %w", err)
	}

	if _, err := execContextFn(db, ctx, "PRAGMA journal_mode=WAL"); err != nil {
		_ = db.Close()
		return nil, fmt.Errorf("enable WAL: %w", err)
	}
	if _, err := execContextFn(db, ctx, "PRAGMA foreign_keys=ON"); err != nil {
		_ = db.Close()
		return nil, fmt.Errorf("enable foreign keys: %w", err)
	}

	db.SetMaxOpenConns(1) // SQLite single-writer

	s := &Store{db: db, logger: logger}
	if err := migrateFn(s, ctx); err != nil {
		_ = db.Close()
		return nil, fmt.Errorf("migrate: %w", err)
	}

	logger.Info("database opened", "path", dbPath)
	return s, nil
}

// Close closes the database connection.
func (s *Store) Close() error {
	if s.db == nil {
		return nil
	}
	return s.db.Close()
}

// DB returns the underlying *sql.DB. Used for transactions in repositories.
func (s *Store) DB() *sql.DB { return s.db }

// migrateFn is overridable for tests; defaults to (*Store).migrate.
var migrateFn = func(s *Store, ctx context.Context) error { return s.migrate(ctx) }

// migrate creates all tables and applies schema migrations.
func (s *Store) migrate(ctx context.Context) error {
	if err := s.createAllTables(ctx); err != nil {
		return fmt.Errorf("create tables: %w", err)
	}
	s.migrateProfilesNewColumns(ctx)
	if err := s.applyMigrations(ctx); err != nil {
		return fmt.Errorf("apply versioned migrations: %w", err)
	}
	return nil
}

func (s *Store) createAllTables(ctx context.Context) error {
	_, err := s.db.ExecContext(ctx, schema)
	return err
}

// schemaVersion is the current schema version stamped into the database's
// PRAGMA user_version. Bump it whenever you append a migration below.
const schemaVersion = 1

// migration is a single forward schema change, applied when the database's
// user_version is below its version.
type migration struct {
	version int
	sql     string
}

// migrations are applied in ascending version order. Version 1 is the baseline
// schema already materialised by createAllTables, so it carries no extra SQL.
// Add future changes as {version: 2, sql: "ALTER TABLE ..."} and bump
// schemaVersion to match.
var migrations = []migration{
	{version: 1, sql: ""},
}

// applyMigrations runs any forward migrations whose version exceeds the
// database's current user_version, stamping user_version as it goes. It is
// idempotent: a fully-migrated database is a no-op.
func (s *Store) applyMigrations(ctx context.Context) error {
	var current int
	if err := s.db.QueryRowContext(ctx, "PRAGMA user_version").Scan(&current); err != nil {
		return fmt.Errorf("read user_version: %w", err)
	}
	for _, m := range migrations {
		if m.version <= current {
			continue
		}
		if m.sql != "" {
			if _, err := s.db.ExecContext(ctx, m.sql); err != nil {
				return fmt.Errorf("migration v%d: %w", m.version, err)
			}
		}
		// PRAGMA user_version does not accept bound parameters; m.version is an
		// internal constant (never user input), so the formatted value is safe.
		if _, err := s.db.ExecContext(ctx, fmt.Sprintf("PRAGMA user_version = %d", m.version)); err != nil {
			return fmt.Errorf("stamp user_version=%d: %w", m.version, err)
		}
		current = m.version
		s.logger.Info("store: applied migration", "version", m.version)
	}
	return nil
}

// --- Settings repository ----------------------------------------------------

type SettingsRepo struct{ s *Store }

func (s *Store) Settings() SettingsRepo { return SettingsRepo{s: s} }

func (r SettingsRepo) Get(ctx context.Context, key string) (string, error) {
	var value string
	err := r.s.db.QueryRowContext(ctx, "SELECT value FROM settings WHERE key = ?", key).Scan(&value)
	if errors.Is(err, sql.ErrNoRows) {
		return "", ErrNotFound
	}
	return value, err
}

func (r SettingsRepo) Set(ctx context.Context, key, value string) error {
	_, err := r.s.db.ExecContext(ctx,
		`INSERT INTO settings (key, value) VALUES (?, ?)
		 ON CONFLICT(key) DO UPDATE SET value = excluded.value`,
		key, value)
	return err
}

func (r SettingsRepo) GetBool(ctx context.Context, key string, def bool) bool {
	v, err := r.Get(ctx, key)
	if err != nil {
		return def
	}
	return v == "true" || v == "1"
}

// --- Profile repository ----------------------------------------------------

type ProfileRepo struct{ s *Store }

func (s *Store) Profiles() ProfileRepo { return ProfileRepo{s: s} }

func (r ProfileRepo) List(ctx context.Context) ([]Profile, error) {
	rows, err := r.s.db.QueryContext(ctx, "SELECT "+profileSelectColumns+" FROM profiles ORDER BY name")
	if err != nil {
		return nil, err
	}
	defer rows.Close()
	return scanProfiles(rows)
}

func (r ProfileRepo) Get(ctx context.Context, name string) (*Profile, error) {
	row := r.s.db.QueryRowContext(ctx, "SELECT "+profileSelectColumns+" FROM profiles WHERE name = ?", name)
	p, err := scanProfile(row)
	if errors.Is(err, sql.ErrNoRows) {
		return nil, ErrNotFound
	}
	return p, err
}

func (r ProfileRepo) Save(ctx context.Context, p *Profile) error {
	if p.Name == "" {
		return errors.New("profile: name is required")
	}
	// Profiles only accept push | bi | bi-resync (not pull / copy / …).
	// Empty is normalized to push so older clients without a direction still save.
	dir := strings.TrimSpace(p.Direction)
	if dir == "" {
		dir = ProfileDirectionPush
	}
	if !IsValidProfileDirection(dir) {
		return fmt.Errorf("profile: invalid direction %q (allowed: push, bi, bi-resync)", p.Direction)
	}
	p.Direction = dir
	_, err := r.s.db.ExecContext(ctx, profileUpsertSQL,
		p.Name, p.From, p.To, p.Direction,
		marshalStringSlice(p.IncludedPaths), marshalStringSlice(p.ExcludedPaths),
		p.Bandwidth, p.Parallel, p.BackupPath, p.CachePath,
		p.MinSize, p.MaxSize, p.FilterFromFile, p.ExcludeIfPresent,
		boolToInt(p.UseRegex), intPtrToNullable(p.MaxDelete), boolToInt(p.Immutable),
		p.ConflictResolution, intPtrToNullable(p.MultiThreadStreams),
		p.BufferSize, boolToInt(p.FastList),
		intPtrToNullable(p.Retries), intPtrToNullable(p.LowLevelRetries), p.MaxDuration,
		p.MaxAge, p.MinAge, intPtrToNullable(p.MaxDepth), boolToInt(p.DeleteExcluded),
		boolToInt(p.DryRun), p.MaxTransfer, p.MaxDeleteSize, p.Suffix, boolToInt(p.SuffixKeepExtension),
		boolToInt(p.CheckFirst), p.OrderBy, p.RetriesSleep, floatPtrToNullable(p.TpsLimit),
		p.ConnTimeout, p.IoTimeout, boolToInt(p.SizeOnly), boolToInt(p.UpdateMode),
		boolToInt(p.IgnoreExisting), p.DeleteTiming, boolToInt(p.Resilient),
		p.MaxLock, boolToInt(p.CheckAccess), p.ConflictLoser, p.ConflictSuffix,
	)
	return err
}

func (r ProfileRepo) Delete(ctx context.Context, name string) error {
	res, err := r.s.db.ExecContext(ctx, "DELETE FROM profiles WHERE name = ?", name)
	if err != nil {
		return err
	}
	n, _ := res.RowsAffected()
	if n == 0 {
		return ErrNotFound
	}
	return nil
}

// --- Schedule repository ---------------------------------------------------

type ScheduleRepo struct{ s *Store }

func (s *Store) Schedules() ScheduleRepo { return ScheduleRepo{s: s} }

func (r ScheduleRepo) List(ctx context.Context) ([]Schedule, error) {
	rows, err := r.s.db.QueryContext(ctx,
		`SELECT id, profile_name, action, cron_expr, enabled, last_run, next_run, last_result, created_at
		 FROM schedules ORDER BY created_at DESC`)
	if err != nil {
		return nil, err
	}
	defer rows.Close()

	var out []Schedule
	for rows.Next() {
		var s Schedule
		var enabled int
		var lastRun, nextRun, createdAt sql.NullString
		if err := rows.Scan(&s.ID, &s.ProfileName, &s.Action, &s.Cron, &enabled,
			&lastRun, &nextRun, &s.LastResult, &createdAt); err != nil {
			return nil, err
		}
		s.Enabled = enabled != 0
		if lastRun.Valid {
			s.LastRun = lastRun.String
		}
		if nextRun.Valid {
			s.NextRun = nextRun.String
		}
		if createdAt.Valid {
			s.CreatedAt = createdAt.String
		}
		out = append(out, s)
	}
	return out, rows.Err()
}

func (r ScheduleRepo) Get(ctx context.Context, id string) (*Schedule, error) {
	row := r.s.db.QueryRowContext(ctx,
		`SELECT id, profile_name, action, cron_expr, enabled, last_run, next_run, last_result, created_at
		 FROM schedules WHERE id = ?`, id)
	var s Schedule
	var enabled int
	var lastRun, nextRun, lastResult, createdAt sql.NullString
	if err := row.Scan(&s.ID, &s.ProfileName, &s.Action, &s.Cron, &enabled,
		&lastRun, &nextRun, &lastResult, &createdAt); err != nil {
		if errors.Is(err, sql.ErrNoRows) {
			return nil, ErrNotFound
		}
		return nil, err
	}
	s.Enabled = enabled != 0
	if lastRun.Valid {
		s.LastRun = lastRun.String
	}
	if nextRun.Valid {
		s.NextRun = nextRun.String
	}
	if lastResult.Valid {
		s.LastResult = lastResult.String
	}
	if createdAt.Valid {
		s.CreatedAt = createdAt.String
	}
	return &s, nil
}

func (r ScheduleRepo) Save(ctx context.Context, sch *Schedule) error {
	if sch.ID == "" {
		return errors.New("schedule: id is required")
	}
	_, err := r.s.db.ExecContext(ctx,
		`INSERT INTO schedules (id, profile_name, action, cron_expr, enabled, last_run, next_run, last_result)
		 VALUES (?, ?, ?, ?, ?, ?, ?, ?)
		 ON CONFLICT(id) DO UPDATE SET
		   profile_name=excluded.profile_name, action=excluded.action,
		   cron_expr=excluded.cron_expr, enabled=excluded.enabled,
		   last_run=excluded.last_run, next_run=excluded.next_run,
		   last_result=excluded.last_result`,
		sch.ID, sch.ProfileName, sch.Action, sch.Cron, boolToInt(sch.Enabled),
		nullableString(sch.LastRun), nullableString(sch.NextRun), sch.LastResult)
	return err
}

func (r ScheduleRepo) Delete(ctx context.Context, id string) error {
	res, err := r.s.db.ExecContext(ctx, "DELETE FROM schedules WHERE id = ?", id)
	if err != nil {
		return err
	}
	n, _ := res.RowsAffected()
	if n == 0 {
		return ErrNotFound
	}
	return nil
}

// --- History repository ----------------------------------------------------

type HistoryRepo struct{ s *Store }

func (s *Store) History() HistoryRepo { return HistoryRepo{s: s} }

func (r HistoryRepo) List(ctx context.Context, limit, offset int) ([]HistoryEntry, error) {
	rows, err := r.s.db.QueryContext(ctx,
		`SELECT id, profile_name, action, status, start_time, end_time, duration,
		        files_transferred, bytes_transferred, errors, error_message
		 FROM history ORDER BY start_time DESC LIMIT ? OFFSET ?`,
		limit, offset)
	if err != nil {
		return nil, err
	}
	defer rows.Close()
	return scanHistory(rows)
}

func (r HistoryRepo) ListByProfile(ctx context.Context, profileName string, limit, offset int) ([]HistoryEntry, error) {
	rows, err := r.s.db.QueryContext(ctx,
		`SELECT id, profile_name, action, status, start_time, end_time, duration,
		        files_transferred, bytes_transferred, errors, error_message
		 FROM history WHERE profile_name = ? ORDER BY start_time DESC LIMIT ? OFFSET ?`,
		profileName, limit, offset)
	if err != nil {
		return nil, err
	}
	defer rows.Close()
	return scanHistory(rows)
}

func (r HistoryRepo) Save(ctx context.Context, e *HistoryEntry) error {
	if e.ID == "" {
		return errors.New("history: id is required")
	}
	_, err := r.s.db.ExecContext(ctx,
		`INSERT INTO history (id, profile_name, action, status, start_time, end_time, duration,
		                     files_transferred, bytes_transferred, errors, error_message)
		 VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
		 ON CONFLICT(id) DO UPDATE SET
		   status=excluded.status, end_time=excluded.end_time, duration=excluded.duration,
		   files_transferred=excluded.files_transferred, bytes_transferred=excluded.bytes_transferred,
		   errors=excluded.errors, error_message=excluded.error_message`,
		e.ID, e.ProfileName, e.Action, e.State, e.StartedAt, e.FinishedAt, e.Duration,
		e.Files, e.Bytes, e.Errors, e.ErrorMessage)
	return err
}

func (r HistoryRepo) Clear(ctx context.Context) error {
	_, err := r.s.db.ExecContext(ctx, "DELETE FROM history")
	return err
}

func (r HistoryRepo) Stats(ctx context.Context) (HistoryStats, error) {
	var stats HistoryStats
	stats.ByProfile = map[string]ProfileStats{}

	row := r.s.db.QueryRowContext(ctx,
		`SELECT COUNT(*), COALESCE(SUM(bytes_transferred), 0), COALESCE(SUM(duration), 0), COALESCE(SUM(errors), 0)
		 FROM history`)
	if err := row.Scan(&stats.TotalSyncs, &stats.TotalBytes, &stats.TotalDuration, &stats.TotalErrors); err != nil {
		return stats, err
	}

	rows, err := r.s.db.QueryContext(ctx,
		`SELECT profile_name, COUNT(*), COALESCE(SUM(bytes_transferred), 0),
		        COALESCE(SUM(duration), 0), COALESCE(SUM(errors), 0)
		 FROM history GROUP BY profile_name`)
	if err != nil {
		return stats, err
	}
	defer rows.Close()
	for rows.Next() {
		var name string
		var ps ProfileStats
		if err := rows.Scan(&name, &ps.Syncs, &ps.Bytes, &ps.Duration, &ps.Errors); err != nil {
			return stats, err
		}
		stats.ByProfile[name] = ps
	}
	return stats, rows.Err()
}

// --- Board / Flow / Delta repositories (Phase 3 wires full CRUD) ---------

type BoardRepo struct{ s *Store }

func (s *Store) Boards() BoardRepo { return BoardRepo{s: s} }

func (r BoardRepo) List(ctx context.Context) ([]Board, error) {
	rows, err := r.s.db.QueryContext(ctx,
		`SELECT id, name, created_at, updated_at, schedule_enabled, cron_expr
		 FROM boards ORDER BY updated_at DESC`)
	if err != nil {
		return nil, err
	}
	defer rows.Close()

	var boards []Board
	for rows.Next() {
		var b Board
		var schedEnabled int
		var cron sql.NullString
		if err := rows.Scan(&b.ID, &b.Name, &b.CreatedAt, &b.UpdatedAt,
			&schedEnabled, &cron); err != nil {
			return nil, err
		}
		// Nodes/edges loaded via LoadGraph.
		boards = append(boards, b)
	}
	return boards, rows.Err()
}

func (r BoardRepo) Get(ctx context.Context, id string) (*Board, error) {
	row := r.s.db.QueryRowContext(ctx,
		`SELECT id, name, created_at, updated_at, schedule_enabled, cron_expr
		 FROM boards WHERE id = ?`, id)
	var b Board
	var schedEnabled int
	var cron sql.NullString
	if err := row.Scan(&b.ID, &b.Name, &b.CreatedAt, &b.UpdatedAt,
		&schedEnabled, &cron); err != nil {
		if errors.Is(err, sql.ErrNoRows) {
			return nil, ErrNotFound
		}
		return nil, err
	}
	return &b, nil
}

// LoadGraph returns the board with its nodes and edges populated. Used for
// DAG execution and the web UI canvas.
func (r BoardRepo) LoadGraph(ctx context.Context, id string) (*Board, error) {
	b, err := r.Get(ctx, id)
	if err != nil {
		return nil, err
	}
	b.Nodes = []BoardNode{}
	b.Edges = []BoardEdge{}

	nrows, err := r.s.db.QueryContext(ctx,
		`SELECT id, remote_name, path, label, x, y
		 FROM board_nodes WHERE board_id = ? ORDER BY id`, id)
	if err != nil {
		return nil, err
	}
	defer nrows.Close()
	for nrows.Next() {
		var n BoardNode
		if err := nrows.Scan(&n.ID, &n.RemoteName, &n.Path, &n.Label, &n.X, &n.Y); err != nil {
			return nil, err
		}
		b.Nodes = append(b.Nodes, n)
	}
	if err := nrows.Err(); err != nil {
		return nil, err
	}

	erows, err := r.s.db.QueryContext(ctx,
		`SELECT id, source_id, target_id, action, sync_config
		 FROM board_edges WHERE board_id = ? ORDER BY id`, id)
	if err != nil {
		return nil, err
	}
	defer erows.Close()
	for erows.Next() {
		var e BoardEdge
		var syncCfg string
		if err := erows.Scan(&e.ID, &e.SourceID, &e.TargetID, &e.Action, &syncCfg); err != nil {
			return nil, err
		}
		e.SyncConfig = json.RawMessage(syncCfg)
		b.Edges = append(b.Edges, e)
	}
	return b, erows.Err()
}

func (r BoardRepo) Save(ctx context.Context, b *Board) error {
	_, err := r.s.db.ExecContext(ctx,
		`INSERT INTO boards (id, name, schedule_enabled, cron_expr, updated_at)
		 VALUES (?, ?, ?, ?, datetime('now'))
		 ON CONFLICT(id) DO UPDATE SET
		   name=excluded.name,
		   schedule_enabled=excluded.schedule_enabled, cron_expr=excluded.cron_expr,
		   updated_at=datetime('now')`,
		b.ID, b.Name, 0, "")
	return err
}

// SaveGraph persists the board along with its nodes and edges in a single
// transaction. It deletes any prior nodes/edges for the board.
func (r BoardRepo) SaveGraph(ctx context.Context, b *Board) error {
	tx, err := r.s.db.BeginTx(ctx, nil)
	if err != nil {
		return err
	}
	defer func() { _ = tx.Rollback() }()

	if _, err := tx.ExecContext(ctx,
		`INSERT INTO boards (id, name, schedule_enabled, cron_expr, updated_at)
		 VALUES (?, ?, ?, ?, datetime('now'))
		 ON CONFLICT(id) DO UPDATE SET
		   name=excluded.name,
		   schedule_enabled=excluded.schedule_enabled, cron_expr=excluded.cron_expr,
		   updated_at=datetime('now')`,
		b.ID, b.Name, 0, ""); err != nil {
		return err
	}

	if _, err := tx.ExecContext(ctx, "DELETE FROM board_nodes WHERE board_id = ?", b.ID); err != nil {
		return err
	}
	for _, n := range b.Nodes {
		if _, err := tx.ExecContext(ctx,
			`INSERT INTO board_nodes (id, board_id, remote_name, path, label, x, y)
			 VALUES (?, ?, ?, ?, ?, ?, ?)`,
			n.ID, b.ID, n.RemoteName, n.Path, n.Label, n.X, n.Y); err != nil {
			return err
		}
	}

	if _, err := tx.ExecContext(ctx, "DELETE FROM board_edges WHERE board_id = ?", b.ID); err != nil {
		return err
	}
	for _, e := range b.Edges {
		cfg := string(e.SyncConfig)
		if cfg == "" {
			cfg = "{}"
		}
		if _, err := tx.ExecContext(ctx,
			`INSERT INTO board_edges (id, board_id, source_id, target_id, action, sync_config)
			 VALUES (?, ?, ?, ?, ?, ?)`,
			e.ID, b.ID, e.SourceID, e.TargetID, e.Action, cfg); err != nil {
			return err
		}
	}

	return tx.Commit()
}

func (r BoardRepo) Delete(ctx context.Context, id string) error {
	res, err := r.s.db.ExecContext(ctx, "DELETE FROM boards WHERE id = ?", id)
	if err != nil {
		return err
	}
	n, _ := res.RowsAffected()
	if n == 0 {
		return ErrNotFound
	}
	return nil
}

type FlowRepo struct{ s *Store }

func (s *Store) Flows() FlowRepo { return FlowRepo{s: s} }

func (r FlowRepo) List(ctx context.Context) ([]Flow, error) {
	rows, err := r.s.db.QueryContext(ctx,
		`SELECT id, name, is_collapsed, schedule_enabled, cron_expr, sort_order, created_at, updated_at
		 FROM flows ORDER BY sort_order, name`)
	if err != nil {
		return nil, err
	}
	defer rows.Close()

	var flows []Flow
	for rows.Next() {
		f, err := scanFlow(rows)
		if err != nil {
			return nil, err
		}
		flows = append(flows, *f)
	}
	if err := rows.Err(); err != nil {
		return nil, err
	}
	// Load operations after closing flow rows — SQLite is single-conn.
	for i := range flows {
		ops, err := r.listOperations(ctx, flows[i].ID)
		if err != nil {
			return nil, err
		}
		flows[i].Operations = ops
	}
	if flows == nil {
		flows = []Flow{}
	}
	return flows, nil
}

func (r FlowRepo) Get(ctx context.Context, id string) (*Flow, error) {
	row := r.s.db.QueryRowContext(ctx,
		`SELECT id, name, is_collapsed, schedule_enabled, cron_expr, sort_order, created_at, updated_at
		 FROM flows WHERE id = ?`, id)
	f, err := scanFlow(row)
	if err != nil {
		if errors.Is(err, sql.ErrNoRows) {
			return nil, ErrNotFound
		}
		return nil, err
	}
	ops, err := r.listOperations(ctx, f.ID)
	if err != nil {
		return nil, err
	}
	f.Operations = ops
	return f, nil
}

func scanFlow(r rowScanner) (*Flow, error) {
	var f Flow
	var collapsed, schedEnabled, sortOrder int
	var cron, createdAt, updatedAt sql.NullString
	if err := r.Scan(&f.ID, &f.Name, &collapsed, &schedEnabled, &cron, &sortOrder, &createdAt, &updatedAt); err != nil {
		return nil, err
	}
	f.IsCollapsed = collapsed != 0
	f.ScheduleEnabled = schedEnabled != 0
	f.Enabled = f.ScheduleEnabled
	f.SortOrder = sortOrder
	if cron.Valid {
		f.ScheduleCron = cron.String
		f.CronExpr = cron.String
	}
	if createdAt.Valid {
		f.CreatedAt = createdAt.String
	}
	if updatedAt.Valid {
		f.UpdatedAt = updatedAt.String
	}
	if f.Operations == nil {
		f.Operations = []Operation{}
	}
	return &f, nil
}

func (r FlowRepo) listOperations(ctx context.Context, flowID string) ([]Operation, error) {
	rows, err := r.s.db.QueryContext(ctx,
		`SELECT id, flow_id, source_remote, source_path, target_remote, target_path,
		        action, sync_config, is_expanded, sort_order
		 FROM operations WHERE flow_id = ? ORDER BY sort_order, id`, flowID)
	if err != nil {
		return nil, err
	}
	defer rows.Close()
	var ops []Operation
	for rows.Next() {
		var op Operation
		var expanded int
		var cfg sql.NullString
		if err := rows.Scan(
			&op.ID, &op.FlowID, &op.SourceRemote, &op.SourcePath,
			&op.TargetRemote, &op.TargetPath, &op.Action, &cfg,
			&expanded, &op.SortOrder,
		); err != nil {
			return nil, err
		}
		op.IsExpanded = expanded != 0
		if cfg.Valid && cfg.String != "" {
			op.SyncConfig = json.RawMessage(cfg.String)
		}
		// Align action column with sync_config.action (Wails load mapping).
		op.NormalizeAction()
		ops = append(ops, op)
	}
	if ops == nil {
		ops = []Operation{}
	}
	return ops, rows.Err()
}

// Save upserts a flow and replaces its operations (Wails SaveFlows semantics per flow).
func (r FlowRepo) Save(ctx context.Context, f *Flow) error {
	if f == nil || f.ID == "" {
		return errors.New("flow: id is required")
	}
	// Normalize schedule flags from either field name clients may send.
	schedEnabled := f.ScheduleEnabled || f.Enabled
	cron := f.ScheduleCron
	if cron == "" {
		cron = f.CronExpr
	}
	f.ScheduleEnabled = schedEnabled
	f.Enabled = schedEnabled
	f.ScheduleCron = cron
	f.CronExpr = cron

	tx, err := r.s.db.BeginTx(ctx, nil)
	if err != nil {
		return err
	}
	defer func() { _ = tx.Rollback() }()

	_, err = tx.ExecContext(ctx,
		`INSERT INTO flows (id, name, is_collapsed, schedule_enabled, cron_expr, sort_order, created_at, updated_at)
		 VALUES (?, ?, ?, ?, ?, ?, COALESCE(NULLIF(?, ''), datetime('now')), datetime('now'))
		 ON CONFLICT(id) DO UPDATE SET
		   name=excluded.name, is_collapsed=excluded.is_collapsed,
		   schedule_enabled=excluded.schedule_enabled, cron_expr=excluded.cron_expr,
		   sort_order=excluded.sort_order, updated_at=datetime('now')`,
		// cron_expr is NOT NULL DEFAULT ''; never pass SQL NULL via nullableString.
		f.ID, f.Name, boolToInt(f.IsCollapsed), boolToInt(schedEnabled),
		cron, f.SortOrder, f.CreatedAt)
	if err != nil {
		return err
	}

	if _, err := tx.ExecContext(ctx, `DELETE FROM operations WHERE flow_id = ?`, f.ID); err != nil {
		return err
	}

	for i := range f.Operations {
		op := &f.Operations[i]
		if op.ID == "" {
			return errors.New("flow: operation id is required")
		}
		op.FlowID = f.ID
		if op.SortOrder == 0 {
			op.SortOrder = i
		}
		// Wails: bo.action = op.syncConfig.action; keep column + JSON in sync.
		op.NormalizeAction()
		cfg := "{}"
		if len(op.SyncConfig) > 0 {
			cfg = string(op.SyncConfig)
		}
		_, err := tx.ExecContext(ctx,
			`INSERT INTO operations
			 (id, flow_id, source_remote, source_path, target_remote, target_path, action, sync_config, is_expanded, sort_order)
			 VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)`,
			op.ID, f.ID, op.SourceRemote, nullIfEmpty(op.SourcePath, "/"),
			op.TargetRemote, nullIfEmpty(op.TargetPath, "/"),
			op.Action, cfg, boolToInt(op.IsExpanded), op.SortOrder,
		)
		if err != nil {
			return err
		}
	}

	return tx.Commit()
}

func nullIfEmpty(s, def string) string {
	if s == "" {
		return def
	}
	return s
}

func (r FlowRepo) Delete(ctx context.Context, id string) error {
	// operations cascade via FK
	res, err := r.s.db.ExecContext(ctx, "DELETE FROM flows WHERE id = ?", id)
	if err != nil {
		return err
	}
	n, _ := res.RowsAffected()
	if n == 0 {
		return ErrNotFound
	}
	return nil
}

type DeltaRepo struct{ s *Store }

func (s *Store) Deltas() DeltaRepo { return DeltaRepo{s: s} }

func (r DeltaRepo) GetState(ctx context.Context, remoteKey string) (*DeltaState, error) {
	row := r.s.db.QueryRowContext(ctx,
		`SELECT remote_key, provider, last_full_sync, delta_count, is_watching
		 FROM delta_state WHERE remote_key = ?`, remoteKey)
	var d DeltaState
	var isWatching int
	var lastFull sql.NullString
	if err := row.Scan(&d.RemoteKey, &d.Provider, &lastFull, &d.DeltaCount, &isWatching); err != nil {
		if errors.Is(err, sql.ErrNoRows) {
			return nil, ErrNotFound
		}
		return nil, err
	}
	if lastFull.Valid {
		d.LastFullSync = lastFull.String
	}
	d.IsWatching = isWatching != 0
	return &d, nil
}

func (r DeltaRepo) RecordFullSync(ctx context.Context, remoteKey, provider string) error {
	now := time.Now().UTC().Format(time.RFC3339)
	_, err := r.s.db.ExecContext(ctx,
		`INSERT INTO delta_state (remote_key, provider, last_full_sync, delta_count, is_watching)
		 VALUES (?, ?, ?, 0, 0)
		 ON CONFLICT(remote_key) DO UPDATE SET
		   last_full_sync=excluded.last_full_sync, delta_count=0, is_watching=0`,
		remoteKey, provider, now)
	return err
}

// --- Migrations ------------------------------------------------------------

func (s *Store) migrateProfilesNewColumns(ctx context.Context) {
	cols := []string{
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
	}
	for _, ddl := range cols {
		// Silently ignore "duplicate column" errors — ALTER TABLE ADD COLUMN is idempotent in spirit.
		_, _ = s.db.ExecContext(ctx, ddl)
	}
}

// --- Helpers ---------------------------------------------------------------

func marshalStringSlice(s []string) string {
	if s == nil {
		return "[]"
	}
	b, _ := json.Marshal(s)
	return string(b)
}

func unmarshalStringSlice(s string) []string {
	if s == "" {
		return nil
	}
	var out []string
	_ = json.Unmarshal([]byte(s), &out)
	return out
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

func floatPtrToNullable(p *float64) interface{} {
	if p == nil {
		return nil
	}
	return *p
}

func nullableString(s string) interface{} {
	if s == "" {
		return nil
	}
	return s
}
