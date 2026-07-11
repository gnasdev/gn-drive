package rclone

import (
	"context"
	"os"
	"path/filepath"
	"sync"
	"testing"
)

// TestParseJSONStatsLine verifies structured rclone --use-json-log stats parse
// correctly and that non-stats / non-JSON lines are rejected (so the caller
// falls back to the text parser).
func TestParseJSONStatsLine(t *testing.T) {
	line := `{"time":"2026-06-28T23:49:00+07:00","level":"notice","msg":"...","stats":{"bytes":512000,"checks":3,"deletes":1,"errors":2,"eta":null,"speed":1234.5,"totalBytes":1024000,"totalChecks":6,"totalTransfers":4,"transfers":2}}`
	var s Stats
	if !parseJSONStatsLine(line, &s) {
		t.Fatal("expected JSON stats line to parse")
	}
	if s.Bytes != 512000 || s.BytesTotal != 1024000 {
		t.Errorf("bytes = %d/%d, want 512000/1024000", s.Bytes, s.BytesTotal)
	}
	if s.Files != 2 || s.FilesTotal != 4 {
		t.Errorf("files = %d/%d, want 2/4", s.Files, s.FilesTotal)
	}
	if s.Checks != 3 || s.ChecksTotal != 6 {
		t.Errorf("checks = %d/%d, want 3/6", s.Checks, s.ChecksTotal)
	}
	if s.Deletes != 1 || s.Errors != 2 {
		t.Errorf("deletes/errors = %d/%d, want 1/2", s.Deletes, s.Errors)
	}
	if s.Speed != 1234.5 {
		t.Errorf("speed = %v, want 1234.5", s.Speed)
	}

	// A legacy text line must NOT parse as JSON.
	var s2 Stats
	if parseJSONStatsLine("2025/01/15 10:00:00 INFO  : TRANSFER: 1k/2k", &s2) {
		t.Error("text line should not parse as JSON stats")
	}
	// JSON without a stats object must return false.
	if parseJSONStatsLine(`{"level":"info","msg":"hi"}`, &s2) {
		t.Error("JSON without stats object should return false")
	}
}

// TestSync_JSONStatsParsed exercises the full Sync path with a fake rclone that
// emits a JSON-log stats line, verifying the progress callback receives parsed
// values.
func TestSync_JSONStatsParsed(t *testing.T) {
	dir := t.TempDir()
	bin := filepath.Join(dir, "rclone-json")
	// Real rclone emits --use-json-log stats on STDERR.
	script := "#!/bin/sh\n" +
		`echo '{"level":"info","msg":"x","stats":{"bytes":2048,"totalBytes":4096,"transfers":3,"totalTransfers":6,"errors":0}}' 1>&2` + "\n" +
		"exit 0\n"
	if err := os.WriteFile(bin, []byte(script), 0o755); err != nil {
		t.Fatal(err)
	}

	c, _ := New(Options{BinaryPath: bin, Logger: noopLogger()})
	var (
		mu    sync.Mutex
		last  Stats
		calls int
	)
	_, err := c.Sync(context.Background(), SyncConfig{
		Action: ActionPush, Source: "r:src", Dest: "r:dst",
	}, func(s Stats) {
		mu.Lock()
		last = s
		calls++
		mu.Unlock()
	})
	if err != nil {
		t.Fatal(err)
	}
	mu.Lock()
	defer mu.Unlock()
	if calls == 0 {
		t.Fatal("onProgress not called")
	}
	if last.Bytes != 2048 || last.BytesTotal != 4096 {
		t.Errorf("bytes = %d/%d, want 2048/4096", last.Bytes, last.BytesTotal)
	}
	if last.Files != 3 {
		t.Errorf("files = %d, want 3", last.Files)
	}
}

// TestSync_JSONStatsOnStderr is the production shape: all rclone logs on stderr.
func TestSync_JSONStatsOnStderr(t *testing.T) {
	dir := t.TempDir()
	bin := filepath.Join(dir, "rclone-json-err")
	script := "#!/bin/sh\n" +
		`echo '{"level":"info","msg":"Copied","object":"photo.jpg"}' 1>&2` + "\n" +
		`echo '{"level":"info","msg":"stats","stats":{"bytes":9000,"totalBytes":10000,"transfers":1,"totalTransfers":2,"checks":1,"totalChecks":2,"speed":123.0,"eta":1}}' 1>&2` + "\n" +
		"exit 0\n"
	if err := os.WriteFile(bin, []byte(script), 0o755); err != nil {
		t.Fatal(err)
	}
	c, _ := New(Options{BinaryPath: bin, Logger: noopLogger()})
	var last Stats
	var calls int
	var mu sync.Mutex
	_, err := c.Sync(context.Background(), SyncConfig{
		Action: ActionPush, Source: "/a", Dest: "/b",
	}, func(s Stats) {
		mu.Lock()
		last = s
		calls++
		mu.Unlock()
	})
	if err != nil {
		t.Fatal(err)
	}
	mu.Lock()
	defer mu.Unlock()
	if calls < 1 {
		t.Fatal("onProgress not called for stderr stats")
	}
	if last.Bytes != 9000 || last.BytesTotal != 10000 {
		t.Errorf("bytes = %d/%d", last.Bytes, last.BytesTotal)
	}
	if last.CurrentFile != "photo.jpg" && last.CurrentFile != "" {
		// current file may be overwritten by stats line without object; accept either
		t.Logf("current_file=%q", last.CurrentFile)
	}
	if last.Speed != 123.0 {
		t.Errorf("speed = %v, want 123", last.Speed)
	}
}
