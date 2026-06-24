package service

import (
	"encoding/json"
	"errors"
	"os"
	"path/filepath"
	"strings"
	"testing"
	"time"
)

func TestNewManager_UnsupportedPlatform(t *testing.T) {
	// On supported platforms (linux, darwin, windows) NewManager returns
	// a real manager. The unsupported branch is hard to test without
	// cross-compiling; this test just ensures the function doesn't panic.
	_, err := NewManager()
	if err != nil {
		if !errors.Is(err, ErrNotSupported) {
			t.Skipf("manager not supported here: %v", err)
		}
	}
}

func TestSpec_Fields(t *testing.T) {
	spec := Spec{
		Name:        "test-svc",
		DisplayName: "Test Service",
		Description: "a test",
		ExecPath:    "/usr/local/bin/test",
		ConfigDir:   "/etc/test",
		Scope:       ScopeSystem,
		Env:         []string{"FOO=bar", "BAZ=qux"},
	}
	if spec.Name != "test-svc" {
		t.Errorf("Name = %q", spec.Name)
	}
	if spec.Scope != ScopeSystem {
		t.Errorf("Scope = %q", spec.Scope)
	}
}

func TestStatus_Fields(t *testing.T) {
	s := Status{
		Installed:     true,
		Running:       false,
		PID:           12345,
		WebPort:       53241,
		UptimeSecs:    100,
		StartedAt:     "2026-01-01T00:00:00Z",
		LastHeartbeat: "2026-01-01T00:01:00Z",
		LastError:     "",
		Mode:          "service",
		Scope:         "user",
	}
	if !s.Installed {
		t.Error("Installed false")
	}
	if s.Running {
		t.Error("Running true")
	}
}

func TestHealth_ZeroValues(t *testing.T) {
	h := Health{}
	if h.PID != 0 {
		t.Error("PID should be zero")
	}
	if !h.IsStale(time.Hour) {
		t.Error("zero health should be stale")
	}
	if h.Uptime() != 0 {
		t.Error("zero health should have zero uptime")
	}
}

func TestHealth_TimeFields(t *testing.T) {
	now := time.Now().UTC()
	h := Health{
		StartedAt:      now,
		LastHeartbeat:  now,
		LastSyncAt:     now,
		NextScheduleAt: now.Add(time.Hour),
	}
	if h.IsStale(time.Hour) {
		t.Error("heartbeat now should not be stale for 1h threshold")
	}
	up := h.Uptime()
	if up < 0 || up > time.Second {
		t.Errorf("uptime = %v, want ~0", up)
	}
}

func TestHealthFile_LargeFields(t *testing.T) {
	dir := t.TempDir()
	h := Health{
		PID:           99999,
		ServiceName:   "gn-drive",
		Mode:          "service",
		StartedAt:     time.Date(2026, 1, 1, 0, 0, 0, 0, time.UTC),
		LastHeartbeat: time.Date(2026, 1, 1, 0, 1, 0, 0, time.UTC),
		WebPort:       53241,
		LastError:     "a long error message that should be preserved verbatim",
		ActiveTasks:   []string{"a", "b", "c"},
	}
	data, _ := json.MarshalIndent(h, "", "  ")
	if err := os.WriteFile(filepath.Join(dir, "service.health"), data, 0o644); err != nil {
		t.Fatal(err)
	}
	got, err := ReadHealth(dir)
	if err != nil {
		t.Fatal(err)
	}
	if got.LastError != h.LastError {
		t.Errorf("LastError roundtrip lost: got %q", got.LastError)
	}
	if len(got.ActiveTasks) != 3 {
		t.Errorf("ActiveTasks = %v, want 3", got.ActiveTasks)
	}
}

func TestWriter_StopBeforeStart(t *testing.T) {
	dir := t.TempDir()
	w := NewWriter(dir, 50*time.Millisecond)
	// Stop without Start should be a no-op.
	w.Stop()
}

func TestWriter_PeriodZeroUsesDefault(t *testing.T) {
	w := NewWriter("/tmp/x", 0)
	if w.period != 5*time.Second {
		t.Errorf("period = %v, want 5s", w.period)
	}
}

func TestWriter_ActiveTasksEmpty(t *testing.T) {
	dir := t.TempDir()
	w := NewWriter(dir, 50*time.Millisecond)
	w.Start()
	defer w.Stop()

	// After Start, ActiveTasks is initialized to empty slice.
	snap := w.Snapshot()
	if snap.ActiveTasks == nil {
		t.Error("ActiveTasks should be empty slice, not nil")
	}
}

func TestHealth_JSONShape(t *testing.T) {
	h := Health{PID: 1, ServiceName: "s", Mode: "service"}
	data, err := json.Marshal(h)
	if err != nil {
		t.Fatal(err)
	}
	// Verify all fields are present.
	for _, field := range []string{
		`"pid":1`, `"service_name":"s"`, `"mode":"service"`,
		`"started_at"`, `"last_heartbeat"`, `"web_port":0`,
		`"active_tasks":null`,
	} {
		if !contains(string(data), field) {
			t.Errorf("JSON missing field %q in: %s", field, data)
		}
	}
}

func TestStatus_JSONShape(t *testing.T) {
	s := Status{Installed: true, Mode: "service", Scope: "user"}
	data, err := json.Marshal(s)
	if err != nil {
		t.Fatal(err)
	}
	for _, field := range []string{
		`"installed":true`, `"mode":"service"`, `"scope":"user"`,
	} {
		if !contains(string(data), field) {
			t.Errorf("JSON missing field %q in: %s", field, data)
		}
	}
}

func contains(s, sub string) bool {
	for i := 0; i+len(sub) <= len(s); i++ {
		if s[i:i+len(sub)] == sub {
			return true
		}
	}
	return false
}

// --- additional Writer coverage --------------------------------------

func TestHealthWriter_Loop(t *testing.T) {
	dir := t.TempDir()
	w := NewWriter(dir, 10*1000*1000) // 10ms period
	if err := w.Start(); err != nil {
		t.Fatalf("Start: %v", err)
	}
	// Wait for the loop to write a heartbeat at least once.
	time.Sleep(50 * time.Millisecond)
	w.Stop()
	// Force one last sync write so the file is fully flushed.
	w.mu.Lock()
	_ = w.writeLocked()
	w.mu.Unlock()
	// Verify LastHeartbeat updated relative to started_at.
	h, err := ReadHealth(dir)
	if err != nil {
		t.Fatalf("ReadHealth: %v", err)
	}
	if h.LastHeartbeat.IsZero() {
		t.Error("LastHeartbeat should not be zero after loop ran")
	}
}

func TestHealthWriter_StopIdempotent(t *testing.T) {
	dir := t.TempDir()
	w := NewWriter(dir, 50*1000*1000)
	if err := w.Start(); err != nil {
		t.Fatal(err)
	}
	w.Stop()
	// Second close should not panic.
	w.Stop()
}

func TestHealthWriter_SetLastSyncAt(t *testing.T) {
	dir := t.TempDir()
	w := NewWriter(dir, 1*1000*1000)
	defer w.Stop()
	w.SetLastSyncAt(time.Date(2026, 6, 23, 12, 0, 0, 0, time.UTC))
	snap := w.Snapshot()
	if snap.LastSyncAt.IsZero() {
		t.Error("LastSyncAt should not be zero after Set")
	}
	if snap.LastSyncAt.Year() != 2026 {
		t.Errorf("LastSyncAt.Year = %d, want 2026", snap.LastSyncAt.Year())
	}
}

func TestHealthWriter_SetNextScheduleAt(t *testing.T) {
	dir := t.TempDir()
	w := NewWriter(dir, 1*1000*1000)
	defer w.Stop()
	w.SetNextScheduleAt(time.Date(2026, 6, 24, 12, 0, 0, 0, time.UTC))
	snap := w.Snapshot()
	if snap.NextScheduleAt.IsZero() {
		t.Error("NextScheduleAt should not be zero after Set")
	}
}

func TestHealthWriter_SetActiveTasks_Nil(t *testing.T) {
	dir := t.TempDir()
	w := NewWriter(dir, 1*1000*1000)
	defer w.Stop()
	// nil should normalize to empty slice.
	w.SetActiveTasks(nil)
	snap := w.Snapshot()
	if snap.ActiveTasks == nil {
		t.Error("ActiveTasks should be empty slice, not nil")
	}
	if len(snap.ActiveTasks) != 0 {
		t.Errorf("ActiveTasks = %v, want empty", snap.ActiveTasks)
	}
}

func TestHealthWriter_StartFailsOnReadOnlyDir(t *testing.T) {
	if os.Getuid() == 0 {
		t.Skip("running as root, can write to read-only dir")
	}
	dir := t.TempDir()
	ro := filepath.Join(dir, "ro")
	if err := os.Mkdir(ro, 0o500); err != nil {
		t.Fatal(err)
	}
	defer os.Chmod(ro, 0o700)
	w := NewWriter(ro, 1*1000*1000)
	if err := w.Start(); err == nil {
		t.Error("Start should fail on read-only dir")
		w.Stop()
	}
}

func TestHealth_UptimeZero(t *testing.T) {
	var h Health
	if u := h.Uptime(); u != 0 {
		t.Errorf("Uptime on zero StartedAt = %v, want 0", u)
	}
}

func TestHealth_IsStaleZero(t *testing.T) {
	var h Health
	if !h.IsStale(time.Second) {
		t.Error("IsStale should be true when LastHeartbeat is zero")
	}
}

func TestReadHealth_BadJSON(t *testing.T) {
	dir := t.TempDir()
	if err := os.WriteFile(HealthPath(dir), []byte("not json"), 0o644); err != nil {
		t.Fatal(err)
	}
	_, err := ReadHealth(dir)
	if err == nil {
		t.Error("ReadHealth should fail on bad JSON")
	}
}

func TestPlatform_DefaultBranch(t *testing.T) {
	// Platform returns "systemd", "launchd", "scm", or GOOS.
	// We can't easily switch platforms, so just verify the result
	// does not contain a slash and is not empty.
	if p := Platform(); p == "" || strings.ContainsRune(p, '/') {
		t.Errorf("Platform() = %q", p)
	}
}

func TestDefaultSpec_System(t *testing.T) {
	spec := DefaultSpec(ScopeSystem)
	if spec.Scope != ScopeSystem {
		t.Errorf("Scope = %q, want %q", spec.Scope, ScopeSystem)
	}
	if len(spec.Env) == 0 {
		t.Error("Env should be non-empty")
	}
}

func TestSpec_Defaults(t *testing.T) {
	// Verify QuoteValue with various special characters.
	tests := []struct {
		in, want string
	}{
		{"", `""`},
		{"simple", `"simple"`},
		{"with space", `"with space"`},
		{`quote"inside`, `"quote\"inside"`},
		{`back\slash`, `"back\\slash"`},
		{`mix"and\both`, `"mix\"and\\both"`},
	}
	for _, tt := range tests {
		got := QuoteValue(tt.in)
		if got != tt.want {
			t.Errorf("QuoteValue(%q) = %q, want %q", tt.in, got, tt.want)
		}
	}
}

func TestErrVars(t *testing.T) {
	if ErrNotSupported == nil {
		t.Error("ErrNotSupported should not be nil")
	}
	if ErrNotInstalled == nil {
		t.Error("ErrNotInstalled should not be nil")
	}
	if ErrAlreadyInstalled == nil {
		t.Error("ErrAlreadyInstalled should not be nil")
	}
	if ErrNotSupported.Error() == "" {
		t.Error("ErrNotSupported should have a message")
	}
}

// TestWriter_WriteLocked_MarshalError covers the marshal error branch in
// writeLocked. We override marshalHealth to return an error.
func TestWriter_WriteLocked_MarshalError(t *testing.T) {
	dir := t.TempDir()
	w := NewWriter(dir, 50*1000*1000)
	orig := marshalHealth
	defer func() { marshalHealth = orig }()
	marshalHealth = func(h Health) ([]byte, error) {
		return nil, errors.New("marshal forced failure")
	}
	err := w.writeLocked()
	if err == nil {
		t.Fatal("expected writeLocked to fail on marshal error")
	}
	if !strings.Contains(err.Error(), "marshal health") {
		t.Errorf("err = %v, want 'marshal health'", err)
	}
}

// TestWriter_WriteLocked_PathError covers the WriteFile error path.
func TestWriter_WriteLocked_PathError(t *testing.T) {
	dir := t.TempDir()
	w := NewWriter(dir, 50*1000*1000)
	// Set path to invalid location to force WriteFile error.
	w.path = "/nonexistent-parent-dir-" + t.Name() + "/health"
	err := w.writeLocked()
	if err == nil {
		t.Fatal("expected writeLocked to fail on invalid path")
	}
}

// TestReadHealth_ReadError covers the read-error branch in ReadHealth by
// passing a configDir where the health file path is a directory.
func TestReadHealth_ReadError(t *testing.T) {
	dir := t.TempDir()
	// Create a directory at the health file path so ReadFile fails.
	hp := HealthPath(dir)
	if err := os.MkdirAll(hp, 0o755); err != nil {
		t.Fatal(err)
	}
	_, err := ReadHealth(dir)
	if err == nil {
		t.Error("expected error from ReadHealth when health path is a directory")
	}
}
