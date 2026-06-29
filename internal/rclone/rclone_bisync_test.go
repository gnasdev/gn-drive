package rclone

import (
	"strings"
	"testing"
)

// TestBuildArgs_BiIsIncremental verifies the fix for the always-resync bug:
// ActionBi must run an incremental bisync (no --resync) and must not create a
// temp cleanup file or pass the bogus --resync-mode-path flag.
func TestBuildArgs_BiIsIncremental(t *testing.T) {
	c, _ := New(Options{BinaryPath: newFakeRclone(t), Logger: noopLogger()})
	args, cleanup, err := c.buildArgs(SyncConfig{Action: ActionBi, Source: "remote:src", Dest: "remote:dst"})
	if err != nil {
		t.Fatal(err)
	}
	if cleanup != "" {
		t.Errorf("ActionBi should not create a cleanup temp file, got %q", cleanup)
	}
	joined := strings.Join(args, " ")
	if !strings.Contains(joined, "bisync remote:src remote:dst") {
		t.Errorf("expected bisync src dst, got %v", args)
	}
	if strings.Contains(joined, "--resync") {
		t.Errorf("ActionBi must NOT pass --resync (incremental), got %v", args)
	}
	if strings.Contains(joined, "--resync-mode-path") {
		t.Errorf("ActionBi must not pass the bogus --resync-mode-path flag, got %v", args)
	}
}

// TestBuildArgs_BiResyncEstablishesBaseline verifies ActionBiResync still
// passes --resync (and --force) to (re)establish the bisync baseline.
func TestBuildArgs_BiResyncEstablishesBaseline(t *testing.T) {
	c, _ := New(Options{BinaryPath: newFakeRclone(t), Logger: noopLogger()})
	args, _, err := c.buildArgs(SyncConfig{Action: ActionBiResync, Source: "remote:src", Dest: "remote:dst"})
	if err != nil {
		t.Fatal(err)
	}
	joined := strings.Join(args, " ")
	if !strings.Contains(joined, "--resync") {
		t.Errorf("ActionBiResync must pass --resync, got %v", args)
	}
	if !strings.Contains(joined, "--force") {
		t.Errorf("ActionBiResync must pass --force, got %v", args)
	}
}
