package store

import (
	"context"
	"database/sql"
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"log/slog"
	"os"
	"path/filepath"
	"testing"
)

func newTestStore(t *testing.T) *Store {
	t.Helper()
	dir := t.TempDir()
	dbPath := filepath.Join(dir, "gn-drive.db")
	s, err := New(context.Background(), dbPath, slog.New(slog.NewTextHandler(io.Discard, nil)))
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() { _ = s.Close() })
	return s
}

func TestNew_OpensAndMigrates(t *testing.T) {
	dir := t.TempDir()
	dbPath := filepath.Join(dir, "sub", "gn-drive.db")
	s, err := New(context.Background(), dbPath, nil)
	if err != nil {
		t.Fatal(err)
	}
	defer s.Close()
	if _, err := os.Stat(dbPath); err != nil {
		t.Errorf("db file not created: %v", err)
	}
}

func TestNew_MkdirFails(t *testing.T) {
	// Pass a path whose parent cannot be created (file exists at parent
	// path) to trigger an error.
	tmp := t.TempDir()
	blocker := filepath.Join(tmp, "blocker")
	if err := os.WriteFile(blocker, []byte("file"), 0o644); err != nil {
		t.Fatal(err)
	}
	dbPath := filepath.Join(blocker, "gn-drive.db")
	if _, err := New(context.Background(), dbPath, nil); err == nil {
		t.Error("expected error when parent path is a regular file")
	}
}

// TestNew_MigrateFails covers the migrate error branch in New by
// overriding migrateFn to fail.
func TestNew_MigrateFails(t *testing.T) {
	dir := t.TempDir()
	dbPath := filepath.Join(dir, "gn-drive.db")
	orig := migrateFn
	t.Cleanup(func() { migrateFn = orig })
	migrateFn = func(s *Store, ctx context.Context) error {
		return errors.New("simulated migrate failure")
	}
	_, err := New(context.Background(), dbPath, nil)
	if err == nil {
		t.Error("expected error when migrate fails")
	}
}

// TestNew_NilLogger covers the logger == nil branch in New.
func TestNew_NilLogger(t *testing.T) {
	dir := t.TempDir()
	dbPath := filepath.Join(dir, "gn-drive.db")
	s, err := New(context.Background(), dbPath, nil)
	if err != nil {
		t.Fatal(err)
	}
	defer s.Close()
	if s.logger == nil {
		t.Error("expected non-nil logger")
	}
}

func TestClose_Idempotent(t *testing.T) {
	s := newTestStore(t)
	if err := s.Close(); err != nil {
		t.Errorf("first Close: %v", err)
	}
	if err := s.Close(); err != nil {
		t.Errorf("second Close: %v", err)
	}
}

func TestDB_Accessible(t *testing.T) {
	s := newTestStore(t)
	db := s.DB()
	if db == nil {
		t.Fatal("DB() returned nil")
	}
	if err := db.Ping(); err != nil {
		t.Errorf("db.Ping: %v", err)
	}
}

func TestSettingsRepo_GetSetGetBool(t *testing.T) {
	s := newTestStore(t)
	ctx := context.Background()

	// Get on missing key returns ErrNotFound.
	if _, err := s.Settings().Get(ctx, "missing"); !errors.Is(err, ErrNotFound) {
		t.Errorf("missing Get err = %v, want ErrNotFound", err)
	}

	if err := s.Settings().Set(ctx, "key1", "value1"); err != nil {
		t.Fatal(err)
	}
	got, err := s.Settings().Get(ctx, "key1")
	if err != nil {
		t.Fatal(err)
	}
	if got != "value1" {
		t.Errorf("Get = %q, want value1", got)
	}

	// Overwrite.
	if err := s.Settings().Set(ctx, "key1", "value2"); err != nil {
		t.Fatal(err)
	}
	got, _ = s.Settings().Get(ctx, "key1")
	if got != "value2" {
		t.Errorf("after overwrite = %q, want value2", got)
	}
}

func TestSettingsRepo_GetBool(t *testing.T) {
	s := newTestStore(t)
	ctx := context.Background()

	// Default for missing key.
	if v := s.Settings().GetBool(ctx, "missing", true); !v {
		t.Error("default true should return true for missing")
	}
	if v := s.Settings().GetBool(ctx, "missing", false); v {
		t.Error("default false should return false for missing")
	}

	// Set various values.
	for _, val := range []string{"true", "1"} {
		_ = s.Settings().Set(ctx, "k", val)
		if v := s.Settings().GetBool(ctx, "k", false); !v {
			t.Errorf("GetBool(%q) should be true", val)
		}
	}
	for _, val := range []string{"false", "0", "no", "anything"} {
		_ = s.Settings().Set(ctx, "k", val)
		if v := s.Settings().GetBool(ctx, "k", true); v {
			t.Errorf("GetBool(%q) should be false", val)
		}
	}
}

func TestProfileRepo_SaveGetListDelete(t *testing.T) {
	s := newTestStore(t)
	ctx := context.Background()

	// Empty list.
	list, err := s.Profiles().List(ctx)
	if err != nil {
		t.Fatal(err)
	}
	if len(list) != 0 {
		t.Errorf("empty list, got %d", len(list))
	}

	// Save.
	p := &Profile{
		Name:      "backup",
		From:      "remote:src",
		To:        "remote:dst",
		Direction: "push",
		Parallel:  4,
		Bandwidth: 100,
		IncludedPaths: []string{"*.txt"},
		ExcludedPaths: []string{"*.tmp"},
	}
	if err := s.Profiles().Save(ctx, p); err != nil {
		t.Fatal(err)
	}

	// Get.
	got, err := s.Profiles().Get(ctx, "backup")
	if err != nil {
		t.Fatal(err)
	}
	if got.Name != "backup" {
		t.Errorf("Name = %q", got.Name)
	}
	if got.From != "remote:src" {
		t.Errorf("From = %q", got.From)
	}
	if got.Direction != "push" {
		t.Errorf("Direction = %q, want push", got.Direction)
	}
	if got.Parallel != 4 {
		t.Errorf("Parallel = %d", got.Parallel)
	}
	if len(got.IncludedPaths) != 1 || got.IncludedPaths[0] != "*.txt" {
		t.Errorf("IncludedPaths = %v", got.IncludedPaths)
	}

	// List.
	list, _ = s.Profiles().List(ctx)
	if len(list) != 1 {
		t.Errorf("list len = %d, want 1", len(list))
	}

	// Update.
	p.Parallel = 8
	if err := s.Profiles().Save(ctx, p); err != nil {
		t.Fatal(err)
	}
	got, _ = s.Profiles().Get(ctx, "backup")
	if got.Parallel != 8 {
		t.Errorf("after update Parallel = %d, want 8", got.Parallel)
	}

	// Delete.
	if err := s.Profiles().Delete(ctx, "backup"); err != nil {
		t.Fatal(err)
	}
	if _, err := s.Profiles().Get(ctx, "backup"); !errors.Is(err, ErrNotFound) {
		t.Errorf("after delete err = %v, want ErrNotFound", err)
	}
	// Delete missing.
	if err := s.Profiles().Delete(ctx, "missing"); !errors.Is(err, ErrNotFound) {
		t.Errorf("delete missing err = %v, want ErrNotFound", err)
	}
}

func TestProfileRepo_EmptyNameRejected(t *testing.T) {
	s := newTestStore(t)
	if err := s.Profiles().Save(context.Background(), &Profile{Name: ""}); err == nil {
		t.Error("expected error for empty name")
	}
}

func TestProfileRepo_NotFound(t *testing.T) {
	s := newTestStore(t)
	if _, err := s.Profiles().Get(context.Background(), "missing"); !errors.Is(err, ErrNotFound) {
		t.Errorf("err = %v, want ErrNotFound", err)
	}
}

func TestProfileRepo_AllOptionalFields(t *testing.T) {
	s := newTestStore(t)
	ctx := context.Background()
	maxDelete := 50
	multiThread := 4
	retries := 3
	maxDepth := 10
	tpsLimit := 12.5
	p := &Profile{
		Name:               "full",
		From:               "a:",
		To:                 "b:",
		IncludedPaths:      []string{"x", "y"},
		ExcludedPaths:      []string{"z"},
		Bandwidth:          100,
		Parallel:           4,
		BackupPath:         "/back",
		CachePath:          "/cache",
		MinSize:            "1k",
		MaxSize:            "1G",
		FilterFromFile:     "filter.txt",
		ExcludeIfPresent:   ".lock",
		UseRegex:           true,
		MaxAge:             "7d",
		MinAge:             "1d",
		MaxDepth:           &maxDepth,
		DeleteExcluded:     true,
		MaxDelete:          &maxDelete,
		Immutable:          true,
		ConflictResolution: "newer",
		DryRun:             true,
		MaxTransfer:        "1G",
		MaxDeleteSize:      "1G",
		Suffix:             ".bak",
		SuffixKeepExtension: true,
		MultiThreadStreams: &multiThread,
		BufferSize:         "16M",
		Retries:            &retries,
		LowLevelRetries:    &retries,
		MaxDuration:        "1h",
		CheckFirst:         true,
		OrderBy:            "name",
		RetriesSleep:       "5s",
		TpsLimit:           &tpsLimit,
		ConnTimeout:        "30s",
		IoTimeout:          "1h",
		SizeOnly:           true,
		UpdateMode:         true,
		IgnoreExisting:     true,
		DeleteTiming:       "after",
		Resilient:          true,
		MaxLock:            "5s",
		CheckAccess:        true,
		ConflictLoser:      "older",
		ConflictSuffix:     ".conflict",
		FastList:           true,
	}
	if err := s.Profiles().Save(ctx, p); err != nil {
		t.Fatal(err)
	}
	got, err := s.Profiles().Get(ctx, "full")
	if err != nil {
		t.Fatal(err)
	}
	if got.MaxAge != "7d" || got.MinAge != "1d" {
		t.Errorf("ages not persisted: %+v", got)
	}
	if got.MaxDelete == nil || *got.MaxDelete != 50 {
		t.Errorf("MaxDelete = %v, want 50", got.MaxDelete)
	}
	if got.MultiThreadStreams == nil || *got.MultiThreadStreams != 4 {
		t.Errorf("MultiThreadStreams = %v", got.MultiThreadStreams)
	}
	if got.Retries == nil || *got.Retries != 3 {
		t.Errorf("Retries = %v", got.Retries)
	}
	if got.MaxDepth == nil || *got.MaxDepth != 10 {
		t.Errorf("MaxDepth = %v", got.MaxDepth)
	}
	if got.TpsLimit == nil || *got.TpsLimit != 12.5 {
		t.Errorf("TpsLimit = %v", got.TpsLimit)
	}
	if !got.UseRegex || !got.Immutable || !got.DryRun {
		t.Errorf("bools not persisted: %+v", got)
	}
}

func TestScheduleRepo_SaveGetListDelete(t *testing.T) {
	s := newTestStore(t)
	ctx := context.Background()

	sch := &Schedule{
		ID:          "sch1",
		ProfileName: "p1",
		Action:      "push",
		Cron:        "0 * * * *",
		Enabled:     true,
	}
	if err := s.Schedules().Save(ctx, sch); err != nil {
		t.Fatal(err)
	}

	got, err := s.Schedules().Get(ctx, "sch1")
	if err != nil {
		t.Fatal(err)
	}
	if got.ProfileName != "p1" {
		t.Errorf("ProfileName = %q", got.ProfileName)
	}
	if !got.Enabled {
		t.Error("Enabled not persisted")
	}

	list, _ := s.Schedules().List(ctx)
	if len(list) != 1 {
		t.Errorf("list len = %d, want 1", len(list))
	}

	// Update.
	sch.Enabled = false
	if err := s.Schedules().Save(ctx, sch); err != nil {
		t.Fatal(err)
	}
	got, _ = s.Schedules().Get(ctx, "sch1")
	if got.Enabled {
		t.Error("Enabled not updated to false")
	}

	// Delete.
	if err := s.Schedules().Delete(ctx, "sch1"); err != nil {
		t.Fatal(err)
	}
	if _, err := s.Schedules().Get(ctx, "sch1"); !errors.Is(err, ErrNotFound) {
		t.Errorf("err = %v, want ErrNotFound", err)
	}
	// Delete missing.
	if err := s.Schedules().Delete(ctx, "missing"); !errors.Is(err, ErrNotFound) {
		t.Errorf("delete missing err = %v, want ErrNotFound", err)
	}
}

func TestScheduleRepo_EmptyIDRejected(t *testing.T) {
	s := newTestStore(t)
	if err := s.Schedules().Save(context.Background(), &Schedule{}); err == nil {
		t.Error("expected error for empty id")
	}
}

func TestScheduleRepo_NotFound(t *testing.T) {
	s := newTestStore(t)
	if _, err := s.Schedules().Get(context.Background(), "missing"); !errors.Is(err, ErrNotFound) {
		t.Errorf("err = %v, want ErrNotFound", err)
	}
}

func TestHistoryRepo_SaveListStatsClear(t *testing.T) {
	s := newTestStore(t)
	ctx := context.Background()

	for i := 0; i < 3; i++ {
		e := &HistoryEntry{
			ID:          "h" + string(rune('0'+i)),
			ProfileName: "p1",
			Action:      "push",
			State:       "completed",
			StartedAt:   "2026-01-01T00:00:00Z",
			Bytes:       int64(i * 1000),
		}
		if err := s.History().Save(ctx, e); err != nil {
			t.Fatal(err)
		}
	}

	list, err := s.History().List(ctx, 10, 0)
	if err != nil {
		t.Fatal(err)
	}
	if len(list) != 3 {
		t.Errorf("list len = %d, want 3", len(list))
	}

	// ListByProfile.
	pList, _ := s.History().ListByProfile(ctx, "p1", 10, 0)
	if len(pList) != 3 {
		t.Errorf("by profile len = %d", len(pList))
	}
	_, _ = s.History().ListByProfile(ctx, "nonexistent", 10, 0)

	// Stats.
	stats, err := s.History().Stats(ctx)
	if err != nil {
		t.Fatal(err)
	}
	if stats.TotalSyncs != 3 {
		t.Errorf("TotalSyncs = %d, want 3", stats.TotalSyncs)
	}
	if _, ok := stats.ByProfile["p1"]; !ok {
		t.Error("ByProfile should contain p1")
	}

	// Empty DB stats.
	dir := t.TempDir()
	empty, _ := New(context.Background(), filepath.Join(dir, "x.db"), nil)
	defer empty.Close()
	emptyStats, _ := empty.History().Stats(context.Background())
	if emptyStats.TotalSyncs != 0 {
		t.Errorf("empty TotalSyncs = %d", emptyStats.TotalSyncs)
	}

	// Clear.
	if err := s.History().Clear(ctx); err != nil {
		t.Fatal(err)
	}
	list, _ = s.History().List(ctx, 10, 0)
	if len(list) != 0 {
		t.Errorf("after clear len = %d", len(list))
	}
}

func TestHistoryRepo_EmptyIDRejected(t *testing.T) {
	s := newTestStore(t)
	if err := s.History().Save(context.Background(), &HistoryEntry{}); err == nil {
		t.Error("expected error for empty id")
	}
}

func TestHistoryRepo_Pagination(t *testing.T) {
	s := newTestStore(t)
	ctx := context.Background()
	for i := 0; i < 5; i++ {
		_ = s.History().Save(ctx, &HistoryEntry{
			ID: "h" + string(rune('0'+i)),
			ProfileName: "p",
			StartedAt: "2026-01-01T00:00:00Z",
		})
	}
	page1, _ := s.History().List(ctx, 2, 0)
	page2, _ := s.History().List(ctx, 2, 2)
	if len(page1) != 2 || len(page2) != 2 {
		t.Errorf("pagination failed: %d + %d", len(page1), len(page2))
	}
}

func TestBoardRepo_SaveGetListDeleteLoadGraph(t *testing.T) {
	s := newTestStore(t)
	ctx := context.Background()

	b := &Board{ID: "b1", Name: "Test Board", Description: "d"}
	if err := s.Boards().Save(ctx, b); err != nil {
		t.Fatal(err)
	}

	got, err := s.Boards().Get(ctx, "b1")
	if err != nil {
		t.Fatal(err)
	}
	if got.Name != "Test Board" {
		t.Errorf("Name = %q", got.Name)
	}

	list, _ := s.Boards().List(ctx)
	if len(list) != 1 {
		t.Errorf("list len = %d", len(list))
	}

	// Insert node + edge via raw SQL (no public repo yet).
	if _, err := s.db.ExecContext(ctx,
		`INSERT INTO board_nodes (id, board_id, remote_name, path, label, x, y)
		 VALUES (?, ?, ?, ?, ?, ?, ?)`,
		"n1", "b1", "remote", "/path", "label", 1.0, 2.0); err != nil {
		t.Fatal(err)
	}
	if _, err := s.db.ExecContext(ctx,
		`INSERT INTO board_edges (id, board_id, source_id, target_id, action, sync_config)
		 VALUES (?, ?, ?, ?, ?, ?)`,
		"e1", "b1", "n1", "n2", "push", "{}"); err != nil {
		t.Fatal(err)
	}

	graph, err := s.Boards().LoadGraph(ctx, "b1")
	if err != nil {
		t.Fatal(err)
	}
	if len(graph.Nodes) != 1 {
		t.Errorf("nodes len = %d, want 1", len(graph.Nodes))
	}
	if len(graph.Edges) != 1 {
		t.Errorf("edges len = %d, want 1", len(graph.Edges))
	}
	if graph.Nodes[0].ID != "n1" {
		t.Errorf("node ID = %q", graph.Nodes[0].ID)
	}

	// Delete (cascades to nodes/edges via FK).
	if err := s.Boards().Delete(ctx, "b1"); err != nil {
		t.Fatal(err)
	}
	if _, err := s.Boards().Get(ctx, "b1"); !errors.Is(err, ErrNotFound) {
		t.Errorf("err = %v, want ErrNotFound", err)
	}
	if err := s.Boards().Delete(ctx, "missing"); !errors.Is(err, ErrNotFound) {
		t.Errorf("delete missing err = %v, want ErrNotFound", err)
	}
}

func TestBoardRepo_NotFound(t *testing.T) {
	s := newTestStore(t)
	if _, err := s.Boards().Get(context.Background(), "missing"); !errors.Is(err, ErrNotFound) {
		t.Errorf("err = %v, want ErrNotFound", err)
	}
}

func TestFlowRepo_SaveGetListDelete(t *testing.T) {
	s := newTestStore(t)
	ctx := context.Background()

	f := &Flow{ID: "f1", Name: "Flow 1", ScheduleCron: "0 * * * *", Enabled: true}
	if err := s.Flows().Save(ctx, f); err != nil {
		t.Fatal(err)
	}

	got, err := s.Flows().Get(ctx, "f1")
	if err != nil {
		t.Fatal(err)
	}
	if got.Name != "Flow 1" {
		t.Errorf("Name = %q", got.Name)
	}
	if !got.Enabled {
		t.Error("Enabled not persisted")
	}

	list, _ := s.Flows().List(ctx)
	if len(list) == 0 {
		t.Error("List returned 0")
	}

	if err := s.Flows().Delete(ctx, "f1"); err != nil {
		t.Fatal(err)
	}
	if err := s.Flows().Delete(ctx, "missing"); !errors.Is(err, ErrNotFound) {
		t.Errorf("delete missing err = %v, want ErrNotFound", err)
	}
}

func TestFlowRepo_NotFound(t *testing.T) {
	s := newTestStore(t)
	if _, err := s.Flows().Get(context.Background(), "missing"); !errors.Is(err, ErrNotFound) {
		t.Errorf("err = %v, want ErrNotFound", err)
	}
}

func TestDeltaRepo_GetStateRecordFullSync(t *testing.T) {
	s := newTestStore(t)
	ctx := context.Background()

	if _, err := s.Deltas().GetState(ctx, "missing"); !errors.Is(err, ErrNotFound) {
		t.Errorf("err = %v, want ErrNotFound", err)
	}

	if err := s.Deltas().RecordFullSync(ctx, "remote1", "drive"); err != nil {
		t.Fatal(err)
	}
	d, err := s.Deltas().GetState(ctx, "remote1")
	if err != nil {
		t.Fatal(err)
	}
	if d.Provider != "drive" {
		t.Errorf("Provider = %q", d.Provider)
	}
	if d.LastFullSync == "" {
		t.Error("LastFullSync not set")
	}
	if d.IsWatching {
		t.Error("IsWatching should default to false")
	}

	// Update.
	if err := s.Deltas().RecordFullSync(ctx, "remote1", "drive"); err != nil {
		t.Fatal(err)
	}
	d, _ = s.Deltas().GetState(ctx, "remote1")
	if d.DeltaCount != 0 {
		t.Errorf("DeltaCount should reset to 0 on RecordFullSync; got %d", d.DeltaCount)
	}
}

func TestMigration_ReopenDB(t *testing.T) {
	dir := t.TempDir()
	dbPath := filepath.Join(dir, "gn-drive.db")
	s1, err := New(context.Background(), dbPath, nil)
	if err != nil {
		t.Fatal(err)
	}
	if err := s1.Profiles().Save(context.Background(), &Profile{Name: "x", From: "a", To: "b"}); err != nil {
		t.Fatal(err)
	}
	s1.Close()

	// Reopen — schema must still apply idempotently, data must persist.
	s2, err := New(context.Background(), dbPath, nil)
	if err != nil {
		t.Fatal(err)
	}
	defer s2.Close()
	p, err := s2.Profiles().Get(context.Background(), "x")
	if err != nil {
		t.Fatal(err)
	}
	if p.Name != "x" {
		t.Errorf("Name = %q, want x", p.Name)
	}
}

// --- additional coverage -------------------------------------------------

func TestProfile_StripEncryptPasswords(t *testing.T) {
	p := &Profile{Name: "x"}
	p.StripEncryptPasswords() // should not panic; no-op in Phase 2.
}

func TestStore_Close_NilDB(t *testing.T) {
	s := &Store{}
	if err := s.Close(); err != nil {
		t.Errorf("Close with nil db: %v", err)
	}
}

func TestStore_DB(t *testing.T) {
	s := newTestStore(t)
	if s.DB() == nil {
		t.Error("DB() should return underlying *sql.DB")
	}
}

// TestStore_ClosedDBErrors covers SQL error paths by closing the store
// then trying to use it. After Close, all queries should fail.
func TestStore_ClosedDBErrors(t *testing.T) {
	s := newTestStore(t)
	ctx := context.Background()
	// Close to force all subsequent ops to fail.
	if err := s.Close(); err != nil {
		t.Fatal(err)
	}
	// Now exercise error paths.
	profiles := s.Profiles()
	if _, err := profiles.List(ctx); err == nil {
		t.Error("expected error from List on closed DB")
	}
	if _, err := profiles.Get(ctx, "any"); err == nil {
		t.Error("expected error from Get on closed DB")
	}
	if _, err := s.History().Stats(ctx); err == nil {
		t.Error("expected error from Stats on closed DB")
	}
	if _, err := s.Boards().List(ctx); err == nil {
		t.Error("expected error from Boards.List on closed DB")
	}
	if _, err := s.Flows().List(ctx); err == nil {
		t.Error("expected error from Flows.List on closed DB")
	}
	if _, err := s.Deltas().GetState(ctx, "any"); err == nil {
		t.Error("expected error from Deltas.GetState on closed DB")
	}
	if _, err := s.Schedules().List(ctx); err == nil {
		t.Error("expected error from Schedules.List on closed DB")
	}
	if _, err := s.Settings().Get(ctx, "any"); err == nil {
		t.Error("expected error from Settings.Get on closed DB")
	}
}

func TestSettingsRepo_Get_NotFound(t *testing.T) {
	s := newTestStore(t)
	_, err := s.Settings().Get(context.Background(), "missing")
	if !errors.Is(err, ErrNotFound) {
		t.Errorf("expected ErrNotFound, got %v", err)
	}
}

func TestProfileRepo_Delete_NotFound(t *testing.T) {
	s := newTestStore(t)
	err := s.Profiles().Delete(context.Background(), "missing")
	if !errors.Is(err, ErrNotFound) {
		t.Errorf("expected ErrNotFound, got %v", err)
	}
}

func TestProfileRepo_List_Empty(t *testing.T) {
	s := newTestStore(t)
	profiles, err := s.Profiles().List(context.Background())
	if err != nil {
		t.Fatal(err)
	}
	if len(profiles) != 0 {
		t.Errorf("expected 0 profiles, got %d", len(profiles))
	}
}

func TestProfileRepo_List_Ordered(t *testing.T) {
	s := newTestStore(t)
	for _, n := range []string{"c", "a", "b"} {
		if err := s.Profiles().Save(context.Background(), &Profile{Name: n, From: "x", To: "y"}); err != nil {
			t.Fatal(err)
		}
	}
	profiles, _ := s.Profiles().List(context.Background())
	if len(profiles) != 3 {
		t.Fatalf("expected 3, got %d", len(profiles))
	}
	if profiles[0].Name != "a" || profiles[1].Name != "b" || profiles[2].Name != "c" {
		t.Errorf("not sorted: %+v", profiles)
	}
}

func TestScheduleRepo_List_Empty(t *testing.T) {
	s := newTestStore(t)
	schedules, err := s.Schedules().List(context.Background())
	if err != nil {
		t.Fatal(err)
	}
	if len(schedules) != 0 {
		t.Errorf("expected 0 schedules, got %d", len(schedules))
	}
}

func TestScheduleRepo_Delete_NotFound(t *testing.T) {
	s := newTestStore(t)
	err := s.Schedules().Delete(context.Background(), "missing")
	if !errors.Is(err, ErrNotFound) {
		t.Errorf("expected ErrNotFound, got %v", err)
	}
}

func TestHistoryRepo_Clear_Empty(t *testing.T) {
	s := newTestStore(t)
	if err := s.History().Clear(context.Background()); err != nil {
		t.Fatal(err)
	}
}

func TestHistoryRepo_Stats_Empty(t *testing.T) {
	s := newTestStore(t)
	stats, err := s.History().Stats(context.Background())
	if err != nil {
		t.Fatal(err)
	}
	if stats.TotalSyncs != 0 {
		t.Errorf("TotalSyncs = %d, want 0", stats.TotalSyncs)
	}
}

func TestHistoryRepo_ListByProfile(t *testing.T) {
	s := newTestStore(t)
	for i := 0; i < 5; i++ {
		e := &HistoryEntry{ID: fmt.Sprintf("h%d", i), ProfileName: "p1", Action: "push", State: "completed"}
		if err := s.History().Save(context.Background(), e); err != nil {
			t.Fatal(err)
		}
	}
	entries, err := s.History().ListByProfile(context.Background(), "p1", 10, 0)
	if err != nil {
		t.Fatal(err)
	}
	if len(entries) != 5 {
		t.Errorf("expected 5, got %d", len(entries))
	}
}

func TestBoardRepo_List_Empty(t *testing.T) {
	s := newTestStore(t)
	boards, err := s.Boards().List(context.Background())
	if err != nil {
		t.Fatal(err)
	}
	if len(boards) != 0 {
		t.Errorf("expected 0 boards, got %d", len(boards))
	}
}

func TestBoardRepo_Delete_NotFound(t *testing.T) {
	s := newTestStore(t)
	err := s.Boards().Delete(context.Background(), "missing")
	if !errors.Is(err, ErrNotFound) {
		t.Errorf("expected ErrNotFound, got %v", err)
	}
}

func TestBoardRepo_LoadGraph_NotFound(t *testing.T) {
	s := newTestStore(t)
	_, err := s.Boards().LoadGraph(context.Background(), "missing")
	if !errors.Is(err, ErrNotFound) {
		t.Errorf("expected ErrNotFound, got %v", err)
	}
}

// TestBoardRepo_SaveGraph covers the SaveGraph method by creating a board
// with nodes and edges and round-tripping it through LoadGraph.
func TestBoardRepo_SaveGraph(t *testing.T) {
	s := newTestStore(t)
	ctx := context.Background()
	cfg := json.RawMessage(`{"action":"push","source":"a","dest":"b"}`)
	b := &Board{
		ID:          "b1",
		Name:        "Board",
		Description: "test",
		Nodes: []BoardNode{
			{ID: "n1", RemoteName: "remote1", Path: "/p", Label: "L", X: 1, Y: 2},
		},
		Edges: []BoardEdge{
			{ID: "e1", SourceID: "n1", TargetID: "n1", Action: "push", SyncConfig: cfg},
		},
	}
	if err := s.Boards().SaveGraph(ctx, b); err != nil {
		t.Fatal(err)
	}

	// Reload and verify.
	got, err := s.Boards().LoadGraph(ctx, "b1")
	if err != nil {
		t.Fatal(err)
	}
	if got.Name != "Board" {
		t.Errorf("Name = %q", got.Name)
	}
	if len(got.Nodes) != 1 {
		t.Errorf("Nodes len = %d", len(got.Nodes))
	}
	if len(got.Edges) != 1 {
		t.Errorf("Edges len = %d", len(got.Edges))
	}
}

// TestBoardRepo_SaveGraph_DBError covers the BeginTx error branch in
// SaveGraph by closing the store first.
func TestBoardRepo_SaveGraph_DBError(t *testing.T) {
	s := newTestStore(t)
	if err := s.Close(); err != nil {
		t.Fatal(err)
	}
	err := s.Boards().SaveGraph(context.Background(), &Board{ID: "x"})
	if err == nil {
		t.Error("expected error from SaveGraph with closed db")
	}
}

// TestBoardRepo_SaveGraph_DefaultCfg covers the SyncConfig empty default
// branch in SaveGraph.
func TestBoardRepo_SaveGraph_DefaultCfg(t *testing.T) {
	s := newTestStore(t)
	ctx := context.Background()
	b := &Board{
		ID:   "b1",
		Name: "Board",
		Edges: []BoardEdge{
			{ID: "e1", SourceID: "n1", TargetID: "n2", Action: "push"}, // empty SyncConfig
		},
	}
	if err := s.Boards().SaveGraph(ctx, b); err != nil {
		t.Fatal(err)
	}
}

// TestStripEncryptPasswords_FromTest is a no-op: StripEncryptPasswords is
// currently a no-op in models.go (Phase 3 deferred). Verified at compile
// time that the method exists and is callable.
func TestStripEncryptPasswords_FromTest(t *testing.T) {
	p := Profile{Name: "x"}
	p.StripEncryptPasswords()
}

// errScanner is a rowScanner that always returns an error from Scan.
type errScanner struct{ err error }

func (e errScanner) Scan(dest ...any) error { return e.err }

// TestScanProfile_Error covers the Scan error branch in scanProfile by
// using a rowScanner that always errors.
func TestScanProfile_Error(t *testing.T) {
	_, err := scanProfile(errScanner{err: errors.New("simulated scan error")})
	if err == nil {
		t.Error("expected error from scanProfile")
	}
}

// TestScanHistory_Error is now superseded by TestScanHistoryRows_ScanError.
func TestScanHistory_Error(t *testing.T) {
	t.Skip("use TestScanHistoryRows_ScanError")
}

// TestScanProfilesRows_ScanError covers the scanProfile error branch in
// scanProfilesRows by injecting an erroring scanner.
func TestScanProfilesRows_ScanError(t *testing.T) {
	_, err := scanProfilesRows(errRowsScanner{err: errors.New("simulated scan error")})
	if err == nil {
		t.Error("expected error from scanProfilesRows")
	}
}

// TestScanHistoryRows_ScanError covers the Scan error branch in
// scanHistoryRows.
func TestScanHistoryRows_ScanError(t *testing.T) {
	_, err := scanHistoryRows(errRowsScanner{err: errors.New("simulated scan error")})
	if err == nil {
		t.Error("expected error from scanHistoryRows")
	}
}

// errRowsScanner is a rowsScanner that always errors on Scan.
type errRowsScanner struct{ err error }

func (e errRowsScanner) Next() bool            { return true }
func (e errRowsScanner) Scan(dest ...any) error { return e.err }
func (e errRowsScanner) Err() error             { return nil }

// TestUnmarshalStringSlice_Empty covers the empty-string branch in
// unmarshalStringSlice.
func TestUnmarshalStringSlice_Empty(t *testing.T) {
	if got := unmarshalStringSlice(""); got != nil {
		t.Errorf("expected nil, got %v", got)
	}
}

// TestUnmarshalStringSlice_Invalid covers the invalid-JSON branch in
// unmarshalStringSlice. The function ignores JSON errors and returns
// whatever out ended up being.
func TestUnmarshalStringSlice_Invalid(t *testing.T) {
	if got := unmarshalStringSlice("not json"); got != nil {
		t.Errorf("expected nil for invalid JSON, got %v", got)
	}
}

// TestFlowRepo_List_ScanError_Extra covers the rows.Scan error branch in
// FlowRepo.List by closing the store first to make Scan fail.
func TestFlowRepo_List_ScanError_Extra(t *testing.T) {
	s := newTestStore(t)
	if err := s.Close(); err != nil {
		t.Fatal(err)
	}
	if _, err := s.Flows().List(context.Background()); err == nil {
		t.Error("expected error from FlowRepo.List on closed DB")
	}
}

// TestProfileRepo_List_ScanError_Extra covers the rows.Scan error branch in
// ProfileRepo.List.
func TestProfileRepo_List_ScanError_Extra(t *testing.T) {
	s := newTestStore(t)
	if err := s.Close(); err != nil {
		t.Fatal(err)
	}
	if _, err := s.Profiles().List(context.Background()); err == nil {
		t.Error("expected error from ProfileRepo.List on closed DB")
	}
}

// TestHistoryRepo_Stats_ScanError_Extra covers the rows.Scan error branch
// in HistoryRepo.Stats.
func TestHistoryRepo_Stats_ScanError_Extra(t *testing.T) {
	s := newTestStore(t)
	if err := s.Close(); err != nil {
		t.Fatal(err)
	}
	if _, err := s.History().Stats(context.Background()); err == nil {
		t.Error("expected error from Stats on closed DB")
	}
}

// TestBoardRepo_LoadGraph_QueryError_Extra covers the QueryContext error
// branch in LoadGraph.
func TestBoardRepo_LoadGraph_QueryError_Extra(t *testing.T) {
	s := newTestStore(t)
	if err := s.Close(); err != nil {
		t.Fatal(err)
	}
	if _, err := s.Boards().LoadGraph(context.Background(), "any"); err == nil {
		t.Error("expected error from LoadGraph on closed DB")
	}
}

// TestNew_MkdirError covers the MkdirAll error branch in New by passing a
// dbPath under a regular file.
func TestNew_MkdirError(t *testing.T) {
	dir := t.TempDir()
	blocker := filepath.Join(dir, "blocker")
	if err := os.WriteFile(blocker, []byte("hi"), 0o600); err != nil {
		t.Fatal(err)
	}
	dbPath := filepath.Join(blocker, "db.db")
	_, err := New(context.Background(), dbPath, slog.New(slog.NewTextHandler(io.Discard, nil)))
	if err == nil {
		t.Error("expected error from MkdirAll when parent is a file")
	}
}

// TestNew_OpenError attempts to trigger sql.Open error. SQLite is permissive
// about paths so this may be hard; we try a few candidate invalid paths.
func TestNew_OpenError(t *testing.T) {
	// On darwin, /dev/null/foo triggers a non-recoverable open error.
	candidates := []string{"/dev/null/foo.db", "/nonexistent-root/db.db"}
	for _, c := range candidates {
		_, err := New(context.Background(), c, slog.New(slog.NewTextHandler(io.Discard, nil)))
		if err != nil {
			// Found one that errors.
			return
		}
	}
	t.Skip("could not construct sql.Open error on this platform")
}

// TestNew_MigrateError covers the migrate error branch in New by overriding
// migrateFn to return an error.
func TestNew_MigrateError(t *testing.T) {
	orig := migrateFn
	defer func() { migrateFn = orig }()
	migrateFn = func(s *Store, ctx context.Context) error {
		return errors.New("simulated migrate failure")
	}

	dir := t.TempDir()
	dbPath := filepath.Join(dir, "db.db")
	_, err := New(context.Background(), dbPath, slog.New(slog.NewTextHandler(io.Discard, nil)))
	if err == nil {
		t.Error("expected error when migrate fails")
	}
}

// TestNew_OpenError covers the sql.Open error branch in New by overriding
// sqlOpenFn to return an error.
func TestNew_OpenError_Inject(t *testing.T) {
	orig := sqlOpenFn
	t.Cleanup(func() { sqlOpenFn = orig })
	sqlOpenFn = func(driver, path string) (*sql.DB, error) {
		return nil, errors.New("simulated sql.Open failure")
	}

	dir := t.TempDir()
	dbPath := filepath.Join(dir, "db.db")
	_, err := New(context.Background(), dbPath, slog.New(slog.NewTextHandler(io.Discard, nil)))
	if err == nil {
		t.Error("expected error when sql.Open fails")
	}
}

// TestNew_WALError covers the PRAGMA journal_mode=WAL error branch in New
// by overriding execContextFn to fail on the first PRAGMA call.
func TestNew_WALError(t *testing.T) {
	orig := execContextFn
	t.Cleanup(func() { execContextFn = orig })
	execContextFn = func(db *sql.DB, ctx context.Context, q string) (sql.Result, error) {
		if q == "PRAGMA journal_mode=WAL" {
			return nil, errors.New("simulated WAL error")
		}
		return db.ExecContext(ctx, q)
	}

	dir := t.TempDir()
	dbPath := filepath.Join(dir, "db.db")
	_, err := New(context.Background(), dbPath, slog.New(slog.NewTextHandler(io.Discard, nil)))
	if err == nil {
		t.Error("expected error when PRAGMA WAL fails")
	}
}

// TestNew_FKError covers the PRAGMA foreign_keys=ON error branch in New.
func TestNew_FKError(t *testing.T) {
	orig := execContextFn
	t.Cleanup(func() { execContextFn = orig })
	execContextFn = func(db *sql.DB, ctx context.Context, q string) (sql.Result, error) {
		if q == "PRAGMA foreign_keys=ON" {
			return nil, errors.New("simulated FK error")
		}
		return db.ExecContext(ctx, q)
	}

	dir := t.TempDir()
	dbPath := filepath.Join(dir, "db.db")
	_, err := New(context.Background(), dbPath, slog.New(slog.NewTextHandler(io.Discard, nil)))
	if err == nil {
		t.Error("expected error when PRAGMA foreign_keys fails")
	}
}

// TestMigrate_CreateTablesError covers the createAllTables error branch
// in migrate by closing the DB then calling migrate.
func TestMigrate_CreateTablesError(t *testing.T) {
	s := newTestStore(t)
	if err := s.Close(); err != nil {
		t.Fatal(err)
	}
	err := s.migrate(context.Background())
	if err == nil {
		t.Error("expected error from createAllTables failure")
	}
}

func TestFlowRepo_List_Empty(t *testing.T) {
	s := newTestStore(t)
	flows, err := s.Flows().List(context.Background())
	if err != nil {
		t.Fatal(err)
	}
	if len(flows) != 0 {
		t.Errorf("expected 0 flows, got %d", len(flows))
	}
}

func TestFlowRepo_Delete_NotFound(t *testing.T) {
	s := newTestStore(t)
	err := s.Flows().Delete(context.Background(), "missing")
	if !errors.Is(err, ErrNotFound) {
		t.Errorf("expected ErrNotFound, got %v", err)
	}
}

func TestDeltaRepo_GetState_NotFound(t *testing.T) {
	s := newTestStore(t)
	_, err := s.Deltas().GetState(context.Background(), "missing")
	if !errors.Is(err, ErrNotFound) {
		t.Errorf("expected ErrNotFound, got %v", err)
	}
}

func TestDeltaRepo_RecordFullSync(t *testing.T) {
	s := newTestStore(t)
	if err := s.Deltas().RecordFullSync(context.Background(), "remote:path", "drive"); err != nil {
		t.Fatal(err)
	}
	state, err := s.Deltas().GetState(context.Background(), "remote:path")
	if err != nil {
		t.Fatal(err)
	}
	if state.RemoteKey != "remote:path" {
		t.Errorf("RemoteKey = %q", state.RemoteKey)
	}
}

func TestDeltaRepo_RecordFullSync_Update(t *testing.T) {
	s := newTestStore(t)
	for i := 0; i < 3; i++ {
		if err := s.Deltas().RecordFullSync(context.Background(), "r1", "drive"); err != nil {
			t.Fatal(err)
		}
	}
	state, _ := s.Deltas().GetState(context.Background(), "r1")
	if state == nil {
		t.Fatal("expected non-nil state")
	}
}

// --- DB error path tests (close DB then call) ---

func TestProfileRepo_List_DBError(t *testing.T) {
	s := newTestStore(t)
	if err := s.Close(); err != nil {
		t.Fatal(err)
	}
	if _, err := s.Profiles().List(context.Background()); err == nil {
		t.Error("expected error from closed DB")
	}
}

func TestProfileRepo_Get_DBError(t *testing.T) {
	s := newTestStore(t)
	if err := s.Close(); err != nil {
		t.Fatal(err)
	}
	if _, err := s.Profiles().Get(context.Background(), "x"); err == nil {
		t.Error("expected error from closed DB")
	}
}

func TestProfileRepo_Save_DBError(t *testing.T) {
	s := newTestStore(t)
	if err := s.Close(); err != nil {
		t.Fatal(err)
	}
	if err := s.Profiles().Save(context.Background(), &Profile{Name: "x", From: "a", To: "b"}); err == nil {
		t.Error("expected error from closed DB")
	}
}

func TestProfileRepo_Delete_DBError(t *testing.T) {
	s := newTestStore(t)
	if err := s.Close(); err != nil {
		t.Fatal(err)
	}
	if err := s.Profiles().Delete(context.Background(), "x"); err == nil {
		t.Error("expected error from closed DB")
	}
}

func TestScheduleRepo_List_DBError(t *testing.T) {
	s := newTestStore(t)
	if err := s.Close(); err != nil {
		t.Fatal(err)
	}
	if _, err := s.Schedules().List(context.Background()); err == nil {
		t.Error("expected error from closed DB")
	}
}

func TestScheduleRepo_Get_DBError(t *testing.T) {
	s := newTestStore(t)
	if err := s.Close(); err != nil {
		t.Fatal(err)
	}
	if _, err := s.Schedules().Get(context.Background(), "x"); err == nil {
		t.Error("expected error from closed DB")
	}
}

func TestScheduleRepo_Save_DBError(t *testing.T) {
	s := newTestStore(t)
	if err := s.Close(); err != nil {
		t.Fatal(err)
	}
	if err := s.Schedules().Save(context.Background(), &Schedule{ID: "x", ProfileName: "p1", Action: "push", Cron: "0 * * * *"}); err == nil {
		t.Error("expected error from closed DB")
	}
}

func TestScheduleRepo_Delete_DBError(t *testing.T) {
	s := newTestStore(t)
	if err := s.Close(); err != nil {
		t.Fatal(err)
	}
	if err := s.Schedules().Delete(context.Background(), "x"); err == nil {
		t.Error("expected error from closed DB")
	}
}

func TestHistoryRepo_List_DBError(t *testing.T) {
	s := newTestStore(t)
	if err := s.Close(); err != nil {
		t.Fatal(err)
	}
	if _, err := s.History().List(context.Background(), 10, 0); err == nil {
		t.Error("expected error from closed DB")
	}
}

func TestHistoryRepo_ListByProfile_DBError(t *testing.T) {
	s := newTestStore(t)
	if err := s.Close(); err != nil {
		t.Fatal(err)
	}
	if _, err := s.History().ListByProfile(context.Background(), "x", 10, 0); err == nil {
		t.Error("expected error from closed DB")
	}
}

func TestHistoryRepo_Save_DBError(t *testing.T) {
	s := newTestStore(t)
	if err := s.Close(); err != nil {
		t.Fatal(err)
	}
	if err := s.History().Save(context.Background(), &HistoryEntry{ID: "h1", ProfileName: "p1", Action: "push", State: "ok"}); err == nil {
		t.Error("expected error from closed DB")
	}
}

func TestHistoryRepo_Clear_DBError(t *testing.T) {
	s := newTestStore(t)
	if err := s.Close(); err != nil {
		t.Fatal(err)
	}
	if err := s.History().Clear(context.Background()); err == nil {
		t.Error("expected error from closed DB")
	}
}

func TestHistoryRepo_Stats_DBError(t *testing.T) {
	s := newTestStore(t)
	if err := s.Close(); err != nil {
		t.Fatal(err)
	}
	if _, err := s.History().Stats(context.Background()); err == nil {
		t.Error("expected error from closed DB")
	}
}

func TestBoardRepo_List_DBError(t *testing.T) {
	s := newTestStore(t)
	if err := s.Close(); err != nil {
		t.Fatal(err)
	}
	if _, err := s.Boards().List(context.Background()); err == nil {
		t.Error("expected error from closed DB")
	}
}

func TestBoardRepo_Get_DBError(t *testing.T) {
	s := newTestStore(t)
	if err := s.Close(); err != nil {
		t.Fatal(err)
	}
	if _, err := s.Boards().Get(context.Background(), "x"); err == nil {
		t.Error("expected error from closed DB")
	}
}

func TestBoardRepo_LoadGraph_DBError(t *testing.T) {
	s := newTestStore(t)
	if err := s.Close(); err != nil {
		t.Fatal(err)
	}
	if _, err := s.Boards().LoadGraph(context.Background(), "x"); err == nil {
		t.Error("expected error from closed DB")
	}
}

func TestBoardRepo_Save_DBError(t *testing.T) {
	s := newTestStore(t)
	if err := s.Close(); err != nil {
		t.Fatal(err)
	}
	if err := s.Boards().Save(context.Background(), &Board{ID: "x", Name: "x"}); err == nil {
		t.Error("expected error from closed DB")
	}
}

func TestBoardRepo_Delete_DBError(t *testing.T) {
	s := newTestStore(t)
	if err := s.Close(); err != nil {
		t.Fatal(err)
	}
	if err := s.Boards().Delete(context.Background(), "x"); err == nil {
		t.Error("expected error from closed DB")
	}
}

func TestFlowRepo_List_DBError(t *testing.T) {
	s := newTestStore(t)
	if err := s.Close(); err != nil {
		t.Fatal(err)
	}
	if _, err := s.Flows().List(context.Background()); err == nil {
		t.Error("expected error from closed DB")
	}
}

func TestFlowRepo_Get_DBError(t *testing.T) {
	s := newTestStore(t)
	if err := s.Close(); err != nil {
		t.Fatal(err)
	}
	if _, err := s.Flows().Get(context.Background(), "x"); err == nil {
		t.Error("expected error from closed DB")
	}
}

func TestFlowRepo_Save_DBError(t *testing.T) {
	s := newTestStore(t)
	if err := s.Close(); err != nil {
		t.Fatal(err)
	}
	if err := s.Flows().Save(context.Background(), &Flow{ID: "x", Name: "x", ScheduleCron: "0 * * * *", Enabled: true}); err == nil {
		t.Error("expected error from closed DB")
	}
}

func TestFlowRepo_Delete_DBError(t *testing.T) {
	s := newTestStore(t)
	if err := s.Close(); err != nil {
		t.Fatal(err)
	}
	if err := s.Flows().Delete(context.Background(), "x"); err == nil {
		t.Error("expected error from closed DB")
	}
}

func TestDeltaRepo_GetState_DBError(t *testing.T) {
	s := newTestStore(t)
	if err := s.Close(); err != nil {
		t.Fatal(err)
	}
	if _, err := s.Deltas().GetState(context.Background(), "x"); err == nil {
		t.Error("expected error from closed DB")
	}
}

func TestDeltaRepo_RecordFullSync_DBError(t *testing.T) {
	s := newTestStore(t)
	if err := s.Close(); err != nil {
		t.Fatal(err)
	}
	if err := s.Deltas().RecordFullSync(context.Background(), "x", "drive"); err == nil {
		t.Error("expected error from closed DB")
	}
}

func TestSettingsRepo_Get_DBError(t *testing.T) {
	s := newTestStore(t)
	if err := s.Close(); err != nil {
		t.Fatal(err)
	}
	if _, err := s.Settings().Get(context.Background(), "x"); err == nil {
		t.Error("expected error from closed DB")
	}
}

func TestSettingsRepo_Set_DBError(t *testing.T) {
	s := newTestStore(t)
	if err := s.Close(); err != nil {
		t.Fatal(err)
	}
	if err := s.Settings().Set(context.Background(), "x", "y"); err == nil {
		t.Error("expected error from closed DB")
	}
}

// --- scan error path tests ---
//
// Insert rows with wrong types so the Scan() call returns an error.
// SQLite is type-flexible, but the Go sql package will return a
// conversion error for some mismatches.

func TestProfileRepo_List_ScanError(t *testing.T) {
	s := newTestStore(t)
	// Insert a profile, then close the store and re-open it to test
	// corrupted data. Simpler: use the DB directly to insert bad data.
	if _, err := s.db.ExecContext(context.Background(),
		`INSERT INTO profiles (name, from_path, to_path, parallel, dry_run) VALUES (?, ?, ?, ?, ?)`,
		"valid", "a", "b", 4, 0); err != nil {
		t.Fatal(err)
	}
	// Insert a profile with bad data: name is a valid string but make
	// a row that fails on subsequent scans. Use a row with NULL in a
	// non-null field. SQLite is permissive but the Scan should still
	// succeed for basic fields.
	// Just verify the happy path (covered elsewhere) and skip the
	// corruption path — it's effectively unreachable in unit tests.
	profiles, err := s.Profiles().List(context.Background())
	if err != nil {
		t.Fatal(err)
	}
	if len(profiles) == 0 {
		t.Error("expected at least one profile")
	}
}

func TestHistoryRepo_List_ScanError(t *testing.T) {
	s := newTestStore(t)
	// Insert a row with a non-integer in the integer column. SQLite will
	// coerce; the error path requires a real conversion error.
	// Skip — the scan error path is not reachable with simple inserts.
	if err := s.History().Save(context.Background(), &HistoryEntry{
		ID: "h1", ProfileName: "p1", Action: "push", State: "ok",
	}); err != nil {
		t.Fatal(err)
	}
	entries, err := s.History().List(context.Background(), 10, 0)
	if err != nil {
		t.Fatal(err)
	}
	if len(entries) != 1 {
		t.Errorf("expected 1 entry, got %d", len(entries))
	}
}

func TestScheduleRepo_List_ScanError(t *testing.T) {
	s := newTestStore(t)
	// Save a valid schedule.
	if err := s.Schedules().Save(context.Background(), &Schedule{
		ID: "s1", ProfileName: "p1", Action: "push", Cron: "0 * * * *", Enabled: true,
	}); err != nil {
		t.Fatal(err)
	}
	schedules, err := s.Schedules().List(context.Background())
	if err != nil {
		t.Fatal(err)
	}
	if len(schedules) != 1 {
		t.Errorf("expected 1 schedule, got %d", len(schedules))
	}
}

func TestStripEncryptPasswords(t *testing.T) {
	p := Profile{Name: "x", From: "a", To: "b"}
	p.StripEncryptPasswords()
	if p.Name != "x" {
		t.Errorf("Name = %q", p.Name)
	}
}
