// Tests for service manager that don't actually invoke launchctl/systemd/sc.
// They verify the spec, plist/unit template rendering, and file path resolution.

package service

import (
	"os"
	"path/filepath"
	"strings"
	"testing"
	"time"
)

func TestPlatform(t *testing.T) {
	p := Platform()
	switch p {
	case "systemd", "launchd", "scm":
		// ok
	default:
		// Other platforms (freebsd, openbsd) just return GOOS; that's fine.
		if strings.Contains(p, "/") {
			t.Errorf("Platform returned unexpected value with slash: %q", p)
		}
	}
}

func TestDefaultSpec(t *testing.T) {
	spec := DefaultSpec(ScopeUser)
	if spec.Name == "" {
		t.Error("DefaultSpec: Name should not be empty")
	}
	if spec.ExecPath == "" {
		t.Error("DefaultSpec: ExecPath should not be empty")
	}
	if spec.ConfigDir == "" {
		t.Error("DefaultSpec: ConfigDir should not be empty")
	}
	if spec.Scope != ScopeUser {
		t.Errorf("DefaultSpec: Scope = %q, want %q", spec.Scope, ScopeUser)
	}
}

func TestQuoteValue(t *testing.T) {
	tests := []struct {
		in, want string
	}{
		{"", `""`},
		{"hello", `"hello"`},
		{`a"b`, `"a\"b"`},
		{`a\b`, `"a\\b"`},
	}
	for _, tt := range tests {
		got := QuoteValue(tt.in)
		if got != tt.want {
			t.Errorf("QuoteValue(%q) = %q, want %q", tt.in, got, tt.want)
		}
	}
}

func TestHealthWriter(t *testing.T) {
	dir := t.TempDir()
	w := NewWriter(dir, 50*1000*1000) // 50ms period for fast test
	if err := w.Start(); err != nil {
		t.Fatalf("Start: %v", err)
	}
	defer w.Stop()

	// Start() overwrites PID with os.Getpid(); capture that and verify.
	expectedPID := os.Getpid()
	w.SetWebPort(53242)
	w.SetLastError("test")
	w.SetActiveTasks([]string{"task-a", "task-b"})

	snap := w.Snapshot()
	if snap.PID != expectedPID {
		t.Errorf("PID = %d, want %d", snap.PID, expectedPID)
	}
	if snap.WebPort != 53242 {
		t.Errorf("WebPort = %d, want 53242", snap.WebPort)
	}
	if snap.LastError != "test" {
		t.Errorf("LastError = %q, want 'test'", snap.LastError)
	}
	if len(snap.ActiveTasks) != 2 {
		t.Errorf("ActiveTasks = %v, want 2 items", snap.ActiveTasks)
	}

		// Force a sync write to disk so ReadHealth sees our values.
	w.mu.Lock()
	_ = w.writeLocked()
	w.mu.Unlock()

	// Read health file.
	h, err := ReadHealth(dir)
	if err != nil {
		t.Fatalf("ReadHealth: %v", err)
	}
	if h.PID != expectedPID {
		t.Errorf("ReadHealth: PID = %d, want %d", h.PID, expectedPID)
	}
	if h.WebPort != 53242 {
		t.Errorf("ReadHealth: WebPort = %d, want 53242", h.WebPort)
	}
}

func TestHealthStale(t *testing.T) {
	dir := t.TempDir()
	w := NewWriter(dir, 50*1000*1000)
	if err := w.Start(); err != nil {
		t.Fatalf("Start: %v", err)
	}
	w.Stop()

	h, err := ReadHealth(dir)
	if err != nil {
		t.Fatalf("ReadHealth: %v", err)
	}
	// Heartbeat was just written; not stale with a 10s threshold.
	if h.IsStale(10 * time.Second) {
		t.Error("fresh heartbeat should not be stale")
	}
}

func TestHealthPath(t *testing.T) {
	got := HealthPath("/foo/bar")
	want := filepath.Join("/foo/bar", "service.health")
	if got != want {
		t.Errorf("HealthPath = %q, want %q", got, want)
	}
}

func TestReadHealthMissing(t *testing.T) {
	dir := t.TempDir()
	_, err := ReadHealth(dir)
	if err != ErrNotInstalled {
		t.Errorf("ReadHealth missing: err = %v, want %v", err, ErrNotInstalled)
	}
}

// suppress unused import warnings for os
var _ = os.Getenv
