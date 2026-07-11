// Model types for gn-drive persistence.
// Mirrors desktop/backend/models/* so existing data is loadable.
package store

import (
	"database/sql"
	"encoding/json"
	"strings"
)

// Profile directions allowed in the product surface (UI + API + CLI).
// One-way push, bidirectional, and bidirectional resync only — not pull.
const (
	ProfileDirectionPush     = "push"
	ProfileDirectionBi       = "bi"
	ProfileDirectionBiResync = "bi-resync"
)

// ValidProfileDirections is the closed set accepted for Profile.Direction.
var ValidProfileDirections = []string{
	ProfileDirectionPush,
	ProfileDirectionBi,
	ProfileDirectionBiResync,
}

// IsValidProfileDirection reports whether d is push|bi|bi-resync.
func IsValidProfileDirection(d string) bool {
	switch strings.TrimSpace(d) {
	case ProfileDirectionPush, ProfileDirectionBi, ProfileDirectionBiResync:
		return true
	default:
		return false
	}
}

// NormalizeProfileDirection returns a valid profile direction.
// Empty → push; unknown/legacy values (e.g. pull) → push.
func NormalizeProfileDirection(d string) string {
	d = strings.TrimSpace(d)
	if IsValidProfileDirection(d) {
		return d
	}
	return ProfileDirectionPush
}

// Profile represents a sync profile with all rclone flags.
// Mirrors models.Profile exactly; JSON keys match the Wails format.
type Profile struct {
	Name          string   `json:"name"`
	From          string   `json:"from"`
	To            string   `json:"to"`
	// Direction: push | bi | bi-resync (see ValidProfileDirections).
	Direction     string   `json:"direction,omitempty"`
	IncludedPaths []string `json:"included_paths"`
	ExcludedPaths []string `json:"excluded_paths"`
	Bandwidth     int      `json:"bandwidth"`
	Parallel      int      `json:"parallel"`
	BackupPath    string   `json:"backup_path"`
	CachePath     string   `json:"cache_path"`

	// Filtering
	MinSize          string `json:"min_size,omitempty"`
	MaxSize          string `json:"max_size,omitempty"`
	FilterFromFile   string `json:"filter_from_file,omitempty"`
	ExcludeIfPresent string `json:"exclude_if_present,omitempty"`
	UseRegex         bool   `json:"use_regex,omitempty"`
	MaxAge           string `json:"max_age,omitempty"`
	MinAge           string `json:"min_age,omitempty"`
	MaxDepth         *int   `json:"max_depth,omitempty"`
	DeleteExcluded   bool   `json:"delete_excluded,omitempty"`

	// Safety
	MaxDelete           *int  `json:"max_delete,omitempty"`
	Immutable           bool  `json:"immutable,omitempty"`
	ConflictResolution  string `json:"conflict_resolution,omitempty"`
	DryRun              bool  `json:"dry_run,omitempty"`
	MaxTransfer         string `json:"max_transfer,omitempty"`
	MaxDeleteSize       string `json:"max_delete_size,omitempty"`
	Suffix              string `json:"suffix,omitempty"`
	SuffixKeepExtension bool  `json:"suffix_keep_extension,omitempty"`

	// Performance
	MultiThreadStreams *int   `json:"multi_thread_streams,omitempty"`
	BufferSize         string `json:"buffer_size,omitempty"`
	Retries            *int   `json:"retries,omitempty"`
	LowLevelRetries    *int   `json:"low_level_retries,omitempty"`
	MaxDuration        string `json:"max_duration,omitempty"`
	CheckFirst         bool   `json:"check_first,omitempty"`
	OrderBy            string `json:"order_by,omitempty"`
	RetriesSleep       string `json:"retries_sleep,omitempty"`
	TpsLimit           *float64 `json:"tps_limit,omitempty"`
	ConnTimeout        string `json:"conn_timeout,omitempty"`
	IoTimeout          string `json:"io_timeout,omitempty"`

	// Comparison
	SizeOnly       bool `json:"size_only,omitempty"`
	UpdateMode     bool `json:"update_mode,omitempty"`
	IgnoreExisting bool `json:"ignore_existing,omitempty"`

	// Sync-specific
	DeleteTiming string `json:"delete_timing,omitempty"`

	// Bisync-specific
	Resilient      bool   `json:"resilient,omitempty"`
	MaxLock        string `json:"max_lock,omitempty"`
	CheckAccess    bool   `json:"check_access,omitempty"`
	ConflictLoser  string `json:"conflict_loser,omitempty"`
	ConflictSuffix string `json:"conflict_suffix,omitempty"`

	// Convenience for frontend
	FastList bool `json:"fast_list,omitempty"`
}

// StripEncryptPasswords clears encryption passwords so they are not persisted.
func (p *Profile) StripEncryptPasswords() {
	// Encryption credentials live in a separate struct (SyncConfig overlay) in
	// Phase 2; full crypt wrapping is implemented in Phase 3.
	_ = p
}

// Schedule represents a cron-scheduled sync.
type Schedule struct {
	ID          string `json:"id"`
	ProfileName string `json:"profile_name"`
	Action      string `json:"action"`
	Cron        string `json:"cron"`
	Enabled     bool   `json:"enabled"`
	LastRun     string `json:"last_run,omitempty"`
	NextRun     string `json:"next_run,omitempty"`
	LastResult  string `json:"last_result,omitempty"`
	CreatedAt   string `json:"created_at,omitempty"`
}

// HistoryEntry records a past sync run.
type HistoryEntry struct {
	ID           string `json:"id"`
	ProfileName  string `json:"profile_name"`
	Action       string `json:"action"`
	State        string `json:"state"`
	StartedAt    string `json:"started_at"`
	FinishedAt   string `json:"finished_at,omitempty"`
	Duration     int64  `json:"duration_secs"`
	Bytes        int64  `json:"bytes"`
	Errors       int    `json:"errors"`
	Files        int    `json:"files"`
	ErrorMessage string `json:"error_message,omitempty"`
}

// HistoryStats aggregates counts across history.
type HistoryStats struct {
	TotalSyncs    int                      `json:"total_syncs"`
	TotalBytes    int64                    `json:"total_bytes"`
	TotalDuration int64                    `json:"total_duration_secs"`
	TotalErrors   int                      `json:"total_errors"`
	ByProfile     map[string]ProfileStats  `json:"by_profile"`
}

// ProfileStats is per-profile aggregate.
type ProfileStats struct {
	Syncs    int   `json:"syncs"`
	Bytes    int64 `json:"bytes"`
	Duration int64 `json:"duration_secs"`
	Errors   int   `json:"errors"`
}

// Board represents a board DAG (lightweight in Phase 2).
type Board struct {
	ID        string `json:"id"`
	Name      string `json:"name"`
	CreatedAt string `json:"created_at"`
	UpdatedAt string `json:"updated_at"`
	// Nodes/edges loaded when board execution / graph is requested.
	Nodes []BoardNode `json:"nodes"`
	Edges []BoardEdge `json:"edges"`
}

// BoardNode is a remote endpoint on the board canvas.
type BoardNode struct {
	ID         string  `json:"id"`
	RemoteName string  `json:"remote_name"`
	Path       string  `json:"path"`
	Label      string  `json:"label"`
	X          float64 `json:"x"`
	Y          float64 `json:"y"`
}

// BoardEdge is a sync connection.
type BoardEdge struct {
	ID        string  `json:"id"`
	SourceID  string  `json:"source_id"`
	TargetID  string  `json:"target_id"`
	Action    string  `json:"action"`
	SyncConfig json.RawMessage `json:"sync_config"`
}

// Flow operation actions (product surface): push | bi | bi-resync.
// Pull is not offered on flow operations (profiles/CLI one-shot may still use pull).
const (
	FlowActionPush     = "push"
	FlowActionBi       = "bi"
	FlowActionBiResync = "bi-resync"
)

// IsValidFlowAction reports whether a is push|bi|bi-resync.
func IsValidFlowAction(a string) bool {
	switch strings.TrimSpace(a) {
	case FlowActionPush, FlowActionBi, FlowActionBiResync:
		return true
	default:
		return false
	}
}

// NormalizeFlowAction returns a valid flow action; unknown/legacy (e.g. pull) → push.
func NormalizeFlowAction(a string) string {
	a = strings.TrimSpace(a)
	if IsValidFlowAction(a) {
		return a
	}
	return FlowActionPush
}

// Operation is a single sync step inside a Flow (Wails models.Operation).
// Paths are fixed slots: Source and Target never swap in storage.
// Action semantics:
//   - push: Source → Target
//   - bi / bi-resync: bidirectional between Source and Target
// Action is stored both in the action column and inside sync_config.action.
type Operation struct {
	ID           string          `json:"id"`
	FlowID       string          `json:"flow_id,omitempty"`
	SourceRemote string          `json:"source_remote"`
	SourcePath   string          `json:"source_path"`
	TargetRemote string          `json:"target_remote"`
	TargetPath   string          `json:"target_path"`
	Action       string          `json:"action"` // push|bi|bi-resync
	SyncConfig   json.RawMessage `json:"sync_config,omitempty"`
	IsExpanded   bool            `json:"is_expanded"`
	SortOrder    int             `json:"sort_order"`
}

// ResolveAction returns the effective flow action (column or sync_config.action),
// always normalized to push|bi|bi-resync.
func (op *Operation) ResolveAction() string {
	if op == nil {
		return FlowActionPush
	}
	if a := strings.TrimSpace(op.Action); a != "" {
		return NormalizeFlowAction(a)
	}
	if a := actionFromSyncConfig(op.SyncConfig); a != "" {
		return NormalizeFlowAction(a)
	}
	return FlowActionPush
}

// NormalizeAction keeps the action column and sync_config.action aligned (Wails SaveFlows).
// Invalid/legacy values (e.g. pull) are coerced to push.
func (op *Operation) NormalizeAction() {
	if op == nil {
		return
	}
	a := op.ResolveAction()
	op.Action = a
	op.SyncConfig = mergeSyncConfigAction(op.SyncConfig, a)
}

func actionFromSyncConfig(raw json.RawMessage) string {
	if len(raw) == 0 {
		return ""
	}
	var m map[string]any
	if err := json.Unmarshal(raw, &m); err != nil {
		return ""
	}
	if v, ok := m["action"].(string); ok {
		return strings.TrimSpace(v)
	}
	return ""
}

func mergeSyncConfigAction(raw json.RawMessage, action string) json.RawMessage {
	m := map[string]any{}
	if len(raw) > 0 {
		_ = json.Unmarshal(raw, &m)
	}
	if m == nil {
		m = map[string]any{}
	}
	m["action"] = action
	b, err := json.Marshal(m)
	if err != nil {
		return raw
	}
	return json.RawMessage(b)
}

// Flow is a named container of sequential Operations (Wails models.Flow).
// Maps: schedule_enabled → Enabled / ScheduleEnabled, cron_expr → ScheduleCron / CronExpr.
type Flow struct {
	ID              string      `json:"id"`
	Name            string      `json:"name"`
	IsCollapsed     bool        `json:"is_collapsed"`
	ScheduleEnabled bool        `json:"schedule_enabled"`
	// Enabled is an alias of ScheduleEnabled for older FE clients.
	Enabled      bool        `json:"enabled"`
	ScheduleCron string      `json:"schedule_cron,omitempty"`
	CronExpr     string      `json:"cron_expr,omitempty"`
	SortOrder    int         `json:"sort_order"`
	Operations   []Operation `json:"operations"`
	CreatedAt    string      `json:"created_at,omitempty"`
	UpdatedAt    string      `json:"updated_at,omitempty"`
}

// ComposePath builds an rclone path from remote name + path.
// Empty remote → treat path as local absolute (or relative as-is).
func ComposePath(remote, path string) string {
	path = strings.TrimSpace(path)
	remote = strings.TrimSpace(remote)
	if remote == "" || remote == "local" {
		if path == "" {
			return "/"
		}
		return path
	}
	if path == "" || path == "/" {
		return remote + ":"
	}
	if strings.HasPrefix(path, "/") {
		return remote + ":" + path
	}
	return remote + ":" + path
}

// DeltaState tracks change-notification state per remote endpoint.
type DeltaState struct {
	RemoteKey    string `json:"remote_key"`
	Provider     string `json:"provider"`
	LastFullSync string `json:"last_full_sync"`
	DeltaCount   int    `json:"delta_count"`
	IsWatching   bool   `json:"is_watching"`
}

// --- Scanning helpers ------------------------------------------------------

// profileSelectColumns is the canonical column list for profile reads.
// Keep in sync with the INSERT below.
const profileSelectColumns = "name, from_path, to_path, direction, included_paths, excluded_paths, bandwidth, parallel, backup_path, cache_path, min_size, max_size, filter_from_file, exclude_if_present, use_regex, max_delete, immutable, conflict_resolution, multi_thread_streams, buffer_size, fast_list, retries, low_level_retries, max_duration, max_age, min_age, max_depth, delete_excluded, dry_run, max_transfer, max_delete_size, suffix, suffix_keep_extension, check_first, order_by, retries_sleep, tps_limit, conn_timeout, io_timeout, size_only, update_mode, ignore_existing, delete_timing, resilient, max_lock, check_access, conflict_loser, conflict_suffix"

// profileUpsertSQL is the INSERT ... ON CONFLICT for profiles.
// ? order MUST match the field order of profileArgs.
const profileUpsertSQL = "INSERT INTO profiles (name, from_path, to_path, direction, included_paths, excluded_paths, bandwidth, parallel, backup_path, cache_path, min_size, max_size, filter_from_file, exclude_if_present, use_regex, max_delete, immutable, conflict_resolution, multi_thread_streams, buffer_size, fast_list, retries, low_level_retries, max_duration, max_age, min_age, max_depth, delete_excluded, dry_run, max_transfer, max_delete_size, suffix, suffix_keep_extension, check_first, order_by, retries_sleep, tps_limit, conn_timeout, io_timeout, size_only, update_mode, ignore_existing, delete_timing, resilient, max_lock, check_access, conflict_loser, conflict_suffix) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?) ON CONFLICT(name) DO UPDATE SET from_path=excluded.from_path, to_path=excluded.to_path, direction=excluded.direction, included_paths=excluded.included_paths, excluded_paths=excluded.excluded_paths, bandwidth=excluded.bandwidth, parallel=excluded.parallel, backup_path=excluded.backup_path, cache_path=excluded.cache_path, min_size=excluded.min_size, max_size=excluded.max_size, filter_from_file=excluded.filter_from_file, exclude_if_present=excluded.exclude_if_present, use_regex=excluded.use_regex, max_delete=excluded.max_delete, immutable=excluded.immutable, conflict_resolution=excluded.conflict_resolution, multi_thread_streams=excluded.multi_thread_streams, buffer_size=excluded.buffer_size, fast_list=excluded.fast_list, retries=excluded.retries, low_level_retries=excluded.low_level_retries, max_duration=excluded.max_duration, max_age=excluded.max_age, min_age=excluded.min_age, max_depth=excluded.max_depth, delete_excluded=excluded.delete_excluded, dry_run=excluded.dry_run, max_transfer=excluded.max_transfer, max_delete_size=excluded.max_delete_size, suffix=excluded.suffix, suffix_keep_extension=excluded.suffix_keep_extension, check_first=excluded.check_first, order_by=excluded.order_by, retries_sleep=excluded.retries_sleep, tps_limit=excluded.tps_limit, conn_timeout=excluded.conn_timeout, io_timeout=excluded.io_timeout, size_only=excluded.size_only, update_mode=excluded.update_mode, ignore_existing=excluded.ignore_existing, delete_timing=excluded.delete_timing, resilient=excluded.resilient, max_lock=excluded.max_lock, check_access=excluded.check_access, conflict_loser=excluded.conflict_loser, conflict_suffix=excluded.conflict_suffix"

// rowScanner is the minimal interface satisfied by both *sql.Row and *sql.Rows.
type rowScanner interface {
	Scan(dest ...any) error
}

func scanProfile(r rowScanner) (*Profile, error) {
	var p Profile
	var includedStr, excludedStr string
	var useRegex, immutable, fastList, deleteExcluded, dryRun, suffixKeepExt,
		checkFirst, sizeOnly, updateMode, ignoreExisting, resilient, checkAccess int
	var maxDelete, multiThreadStreams, retries, lowLevelRetries, maxDepth sql.NullInt64
	var tpsLimit sql.NullFloat64
	err := r.Scan(
		&p.Name, &p.From, &p.To, &p.Direction,
		&includedStr, &excludedStr,
		&p.Bandwidth, &p.Parallel, &p.BackupPath, &p.CachePath,
		&p.MinSize, &p.MaxSize, &p.FilterFromFile, &p.ExcludeIfPresent,
		&useRegex, &maxDelete, &immutable, &p.ConflictResolution,
		&multiThreadStreams, &p.BufferSize, &fastList,
		&retries, &lowLevelRetries, &p.MaxDuration,
		&p.MaxAge, &p.MinAge, &maxDepth, &deleteExcluded, &dryRun,
		&p.MaxTransfer, &p.MaxDeleteSize, &p.Suffix, &suffixKeepExt,
		&checkFirst, &p.OrderBy, &p.RetriesSleep, &tpsLimit,
		&p.ConnTimeout, &p.IoTimeout, &sizeOnly, &updateMode,
		&ignoreExisting, &p.DeleteTiming, &resilient,
		&p.MaxLock, &checkAccess, &p.ConflictLoser, &p.ConflictSuffix,
	)
	if err != nil {
		return nil, err
	}
	p.IncludedPaths = unmarshalStringSlice(includedStr)
	p.ExcludedPaths = unmarshalStringSlice(excludedStr)
	p.UseRegex = useRegex != 0
	p.Immutable = immutable != 0
	p.FastList = fastList != 0
	p.DeleteExcluded = deleteExcluded != 0
	p.DryRun = dryRun != 0
	p.SuffixKeepExtension = suffixKeepExt != 0
	p.CheckFirst = checkFirst != 0
	p.SizeOnly = sizeOnly != 0
	p.UpdateMode = updateMode != 0
	p.IgnoreExisting = ignoreExisting != 0
	p.Resilient = resilient != 0
	p.CheckAccess = checkAccess != 0
	if maxDelete.Valid {
		v := int(maxDelete.Int64)
		p.MaxDelete = &v
	}
	if multiThreadStreams.Valid {
		v := int(multiThreadStreams.Int64)
		p.MultiThreadStreams = &v
	}
	if retries.Valid {
		v := int(retries.Int64)
		p.Retries = &v
	}
	if lowLevelRetries.Valid {
		v := int(lowLevelRetries.Int64)
		p.LowLevelRetries = &v
	}
	if maxDepth.Valid {
		v := int(maxDepth.Int64)
		p.MaxDepth = &v
	}
	if tpsLimit.Valid {
		v := tpsLimit.Float64
		p.TpsLimit = &v
	}
	return &p, nil
}

// rowsScanner is the minimal interface satisfied by *sql.Rows.
type rowsScanner interface {
	Next() bool
	Scan(dest ...any) error
	Err() error
}

func scanProfiles(rows *sql.Rows) ([]Profile, error) {
	return scanProfilesRows(rows)
}

// scanProfilesRows is overridable for tests.
var scanProfilesRows = func(rows rowsScanner) ([]Profile, error) {
	var out []Profile
	for rows.Next() {
		p, err := scanProfile(rows)
		if err != nil {
			return nil, err
		}
		out = append(out, *p)
	}
	return out, rows.Err()
}

func scanHistory(rows *sql.Rows) ([]HistoryEntry, error) {
	return scanHistoryRows(rows)
}

// scanHistoryRows is overridable for tests.
var scanHistoryRows = func(rows rowsScanner) ([]HistoryEntry, error) {
	var out []HistoryEntry
	for rows.Next() {
		var e HistoryEntry
		var files, bytes, errors int64
		if err := rows.Scan(&e.ID, &e.ProfileName, &e.Action, &e.State,
			&e.StartedAt, &e.FinishedAt, &e.Duration,
			&files, &bytes, &errors, &e.ErrorMessage); err != nil {
			return nil, err
		}
		e.Files = int(files)
		e.Bytes = bytes
		e.Errors = int(errors)
		out = append(out, e)
	}
	return out, rows.Err()
}
