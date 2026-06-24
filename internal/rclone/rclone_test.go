package rclone

import (
	"context"
	"errors"
	"io"
	"log/slog"
	"os"
	"path/filepath"
	"strings"
	"sync"
	"testing"
)

func noopLogger() *slog.Logger { return slog.New(slog.NewTextHandler(io.Discard, nil)) }

func newFakeRclone(t *testing.T) string {
	t.Helper()
	dir := t.TempDir()
	bin := filepath.Join(dir, "rclone")
	// Minimal shell script that echoes arguments and exits 0.
	// For tests that need to simulate failure, they set RCLONE_BIN to a
	// script that exits 1 instead.
	script := `#!/bin/sh
echo "fake-rclone $@"
exit 0
`
	if err := os.WriteFile(bin, []byte(script), 0o755); err != nil {
		t.Fatal(err)
	}
	return bin
}

func newFailRclone(t *testing.T) string {
	t.Helper()
	dir := t.TempDir()
	bin := filepath.Join(dir, "rclone-fail")
	script := `#!/bin/sh
echo "faked failure" 1>&2
exit 1
`
	if err := os.WriteFile(bin, []byte(script), 0o755); err != nil {
		t.Fatal(err)
	}
	return bin
}

func newUsageRclone(t *testing.T) string {
	t.Helper()
	dir := t.TempDir()
	bin := filepath.Join(dir, "rclone-usage")
	script := `#!/bin/sh
echo "Usage: rclone <command> [args]" 1>&2
echo "Available commands:" 1>&2
exit 2
`
	if err := os.WriteFile(bin, []byte(script), 0o755); err != nil {
		t.Fatal(err)
	}
	return bin
}

func TestNew_DefaultBinary(t *testing.T) {
	// rclone must be on PATH on the test machine.
	c, err := New(Options{Logger: noopLogger()})
	if err != nil {
		t.Skipf("rclone not on PATH: %v", err)
	}
	if c.Binary() == "" {
		t.Error("Binary() should be non-empty")
	}
}

func TestNew_ExplicitBinary(t *testing.T) {
	bin := newFakeRclone(t)
	c, err := New(Options{BinaryPath: bin, Logger: noopLogger()})
	if err != nil {
		t.Fatal(err)
	}
	if c.Binary() != bin {
		t.Errorf("Binary = %q, want %q", c.Binary(), bin)
	}
}

func TestNew_BinaryNotFound(t *testing.T) {
	if _, err := New(Options{BinaryPath: "/nonexistent/rclone", Logger: noopLogger()}); err == nil {
		t.Error("expected error for missing binary")
	}
}

func TestNew_ConfigPathExplicit(t *testing.T) {
	bin := newFakeRclone(t)
	want := "/custom/path/rclone.conf"
	c, _ := New(Options{BinaryPath: bin, ConfigPath: want, Logger: noopLogger()})
	if c.ConfigPath() != want {
		t.Errorf("ConfigPath = %q, want %q", c.ConfigPath(), want)
	}
}

func TestNew_ConfigPathEmpty(t *testing.T) {
	bin := newFakeRclone(t)
	c, _ := New(Options{BinaryPath: bin, Logger: noopLogger()})
	if c.ConfigPath() != "" {
		t.Errorf("ConfigPath = %q, want empty", c.ConfigPath())
	}
}

func TestVersion(t *testing.T) {
	bin := newFakeRclone(t)
	c, _ := New(Options{BinaryPath: bin, Logger: noopLogger()})
	v, err := c.Version(context.Background())
	if err != nil {
		t.Fatal(err)
	}
	if !strings.Contains(v, "fake-rclone") {
		t.Errorf("Version = %q, want first line of fake-rclone output", v)
	}
}

func TestListFiles_MissingColon(t *testing.T) {
	bin := newFakeRclone(t)
	c, _ := New(Options{BinaryPath: bin, Logger: noopLogger()})
	if _, err := c.ListFiles(context.Background(), "no-colon-here"); err == nil {
		t.Error("expected error when remote path lacks ':'")
	}
}

func TestListFiles_EmptyConfig(t *testing.T) {
	bin := newFailRclone(t)
	c, _ := New(Options{BinaryPath: bin, Logger: noopLogger()})
	if _, err := c.ListFiles(context.Background(), "remote:/path"); err == nil {
		t.Error("expected error when rclone fails")
	}
}

func TestListFiles_InvalidJSON(t *testing.T) {
	dir := t.TempDir()
	bin := filepath.Join(dir, "rclone-bad-json")
	script := `#!/bin/sh
echo "not json"
exit 0
`
	if err := os.WriteFile(bin, []byte(script), 0o755); err != nil {
		t.Fatal(err)
	}
	c, _ := New(Options{BinaryPath: bin, Logger: noopLogger()})
	if _, err := c.ListFiles(context.Background(), "remote:/path"); err == nil {
		t.Error("expected error for invalid JSON")
	}
}

func TestListFiles_OK(t *testing.T) {
	dir := t.TempDir()
	bin := filepath.Join(dir, "rclone-ok")
	script := `#!/bin/sh
cat <<'EOF'
[{"Name":"a.txt","Size":100,"IsDir":false,"Path":"a.txt","ID":"1"}]
EOF
exit 0
`
	if err := os.WriteFile(bin, []byte(script), 0o755); err != nil {
		t.Fatal(err)
	}
	c, _ := New(Options{BinaryPath: bin, Logger: noopLogger()})
	entries, err := c.ListFiles(context.Background(), "remote:/path")
	if err != nil {
		t.Fatal(err)
	}
	if len(entries) != 1 || entries[0].Name != "a.txt" {
		t.Errorf("entries = %+v", entries)
	}
}

func TestMkdir(t *testing.T) {
	bin := newFakeRclone(t)
	c, _ := New(Options{BinaryPath: bin, Logger: noopLogger()})
	if err := c.Mkdir(context.Background(), "remote:/dir"); err != nil {
		t.Fatal(err)
	}
}

func TestMkdir_Failure(t *testing.T) {
	bin := newFailRclone(t)
	c, _ := New(Options{BinaryPath: bin, Logger: noopLogger()})
	if err := c.Mkdir(context.Background(), "remote:/dir"); err == nil {
		t.Error("expected error")
	}
}

func TestPurge(t *testing.T) {
	bin := newFakeRclone(t)
	c, _ := New(Options{BinaryPath: bin, Logger: noopLogger()})
	if err := c.Purge(context.Background(), "remote:/dir"); err != nil {
		t.Fatal(err)
	}
}

func TestDeleteFile(t *testing.T) {
	bin := newFakeRclone(t)
	c, _ := New(Options{BinaryPath: bin, Logger: noopLogger()})
	if err := c.DeleteFile(context.Background(), "remote:/file.txt"); err != nil {
		t.Fatal(err)
	}
}

func TestAbout_InvalidJSON(t *testing.T) {
	dir := t.TempDir()
	bin := filepath.Join(dir, "rclone-bad-about")
	script := `#!/bin/sh
echo "not json"
exit 0
`
	if err := os.WriteFile(bin, []byte(script), 0o755); err != nil {
		t.Fatal(err)
	}
	c, _ := New(Options{BinaryPath: bin, Logger: noopLogger()})
	if _, err := c.About(context.Background(), "remote"); err == nil {
		t.Error("expected error for invalid JSON")
	}
}

func TestVersion_Failure(t *testing.T) {
	bin := newFailRclone(t)
	c, _ := New(Options{BinaryPath: bin, Logger: noopLogger()})
	if _, err := c.Version(context.Background()); err == nil {
		t.Error("expected error from failing rclone")
	}
}

func TestAbout_OK(t *testing.T) {
	dir := t.TempDir()
	bin := filepath.Join(dir, "rclone-about")
	script := `#!/bin/sh
echo '{"used": 100, "total": 1000, "free": 900}'
exit 0
`
	if err := os.WriteFile(bin, []byte(script), 0o755); err != nil {
		t.Fatal(err)
	}
	c, _ := New(Options{BinaryPath: bin, Logger: noopLogger()})
	q, err := c.About(context.Background(), "any")
	if err != nil {
		t.Fatal(err)
	}
	if q.Used != 100 || q.Total != 1000 || q.Free != 900 {
		t.Errorf("About = %+v, want (100, 1000, 900)", q)
	}
}

func TestAbout_NotJSON(t *testing.T) {
	bin := newFakeRclone(t)
	c, _ := New(Options{BinaryPath: bin, Logger: noopLogger()})
	if _, err := c.About(context.Background(), "any"); err == nil {
		t.Error("expected error for non-JSON output")
	}
}

func TestListRemotes_Empty(t *testing.T) {
	bin := newUsageRclone(t)
	c, _ := New(Options{BinaryPath: bin, Logger: noopLogger()})
	remotes, err := c.ListRemotes(context.Background())
	if err != nil {
		t.Fatal(err)
	}
	if remotes != nil {
		t.Errorf("expected nil for empty list, got %v", remotes)
	}
}

func TestListRemotes_Fail(t *testing.T) {
	bin := newFailRclone(t)
	c, _ := New(Options{BinaryPath: bin, Logger: noopLogger()})
	if _, err := c.ListRemotes(context.Background()); err == nil {
		t.Error("expected error from failing rclone")
	}
}

func TestListRemotes_Multiple(t *testing.T) {
	dir := t.TempDir()
	bin := filepath.Join(dir, "rclone-multi")
	script := `#!/bin/sh
echo "remote1:"
echo "remote2:"
echo "remote3:"
exit 0
`
	if err := os.WriteFile(bin, []byte(script), 0o755); err != nil {
		t.Fatal(err)
	}
	c, _ := New(Options{BinaryPath: bin, Logger: noopLogger()})
	remotes, err := c.ListRemotes(context.Background())
	if err != nil {
		t.Fatal(err)
	}
	if len(remotes) != 3 {
		t.Errorf("expected 3 remotes, got %d: %v", len(remotes), remotes)
	}
}

func TestListRemotes_EmptyLine(t *testing.T) {
	dir := t.TempDir()
	bin := filepath.Join(dir, "rclone-emptyline")
	// Use printf to inject a literal empty line in the middle.
	script := `#!/bin/sh
printf 'a:\n\nb:\n'
exit 0
`
	if err := os.WriteFile(bin, []byte(script), 0o755); err != nil {
		t.Fatal(err)
	}
	c, _ := New(Options{BinaryPath: bin, Logger: noopLogger()})
	remotes, err := c.ListRemotes(context.Background())
	if err != nil {
		t.Fatal(err)
	}
	// Empty middle line is skipped; we should have a and b.
	if len(remotes) != 2 {
		t.Errorf("expected 2 remotes, got %d: %v", len(remotes), remotes)
	}
}

func TestNew_DefaultBinaryNotFound(t *testing.T) {
	// Set PATH to empty so LookPath fails.
	t.Setenv("PATH", "")
	if _, err := New(Options{Logger: noopLogger()}); err == nil {
		t.Error("expected error when rclone not on PATH")
	}
}

func TestNew_NoLogger(t *testing.T) {
	// Logger should default to slog.Default() when nil.
	bin := newFakeRclone(t)
	c, err := New(Options{BinaryPath: bin})
	if err != nil {
		t.Fatal(err)
	}
	if c.logger == nil {
		t.Error("expected default logger")
	}
}

func TestAbout_RunFailure(t *testing.T) {
	bin := newFailRclone(t)
	c, _ := New(Options{BinaryPath: bin, Logger: noopLogger()})
	if _, err := c.About(context.Background(), "x"); err == nil {
		t.Error("expected error from failing rclone")
	}
}

func TestParseInt(t *testing.T) {
	tests := []struct {
		s        string
		wantN    int64
		wantOK   bool
	}{
		{"abc 123", 123, true},
		{"abc 0", 0, true},
		{"abc", 0, false},
		{"abc xyz", 0, false},
		// Overflow int64.
		{"abc 99999999999999999999", 0, false},
	}
	for _, tt := range tests {
		n, ok := parseInt(tt.s)
		if n != tt.wantN || ok != tt.wantOK {
			t.Errorf("parseInt(%q) = (%d, %v), want (%d, %v)", tt.s, n, ok, tt.wantN, tt.wantOK)
		}
	}
}

func TestParseSize(t *testing.T) {
	tests := []struct {
		s    string
		want int64
	}{
		{"", 0},
		{"0", 0},
		{"abc", 0},   // empty numStr
		{"1k", 1024},
		{"1kb", 1024},
		{"1m", 1024 * 1024},
		{"1mb", 1024 * 1024},
		{"1g", 1024 * 1024 * 1024},
		{"1t", 1024 * 1024 * 1024 * 1024},
		{"2.5k", 2560},
		{"100", 100},
		// Invalid float triggers ParseFloat error path.
		{".", 0},
	}
	for _, tt := range tests {
		got := parseSize(tt.s)
		if got != tt.want {
			t.Errorf("parseSize(%q) = %d, want %d", tt.s, got, tt.want)
		}
	}
}

func TestParseFraction(t *testing.T) {
	if _, _, ok := parseFraction("nospaces"); ok {
		t.Error("parseFraction('nospaces') should fail without space")
	}
	if _, _, ok := parseFraction("TRANSFER: 1k"); ok {
		t.Error("parseFraction('TRANSFER: 1k') should fail without slash")
	}
	left, right, ok := parseFraction("TRANSFER: 100/200")
	if !ok || left != 100 || right != 200 {
		t.Errorf("parseFraction('TRANSFER: 100/200') = (%d, %d, %v), want (100, 200, true)", left, right, ok)
	}
}

func TestCreateRemote(t *testing.T) {
	bin := newFakeRclone(t)
	c, _ := New(Options{BinaryPath: bin, Logger: noopLogger()})
	if err := c.CreateRemote(context.Background(), "r1", "drive", []string{"key=val", "other=thing"}); err != nil {
		t.Fatal(err)
	}
}

func TestDeleteRemote(t *testing.T) {
	bin := newFakeRclone(t)
	c, _ := New(Options{BinaryPath: bin, Logger: noopLogger()})
	if err := c.DeleteRemote(context.Background(), "r1"); err != nil {
		t.Fatal(err)
	}
}

func TestTestRemote(t *testing.T) {
	bin := newFakeRclone(t)
	c, _ := New(Options{BinaryPath: bin, Logger: noopLogger()})
	if err := c.TestRemote(context.Background(), "r1"); err != nil {
		t.Fatal(err)
	}
}

func TestSync_UnknownAction(t *testing.T) {
	bin := newFakeRclone(t)
	c, _ := New(Options{BinaryPath: bin, Logger: noopLogger()})
	_, err := c.Sync(context.Background(), SyncConfig{Action: Action("bogus"), Source: "a", Dest: "b"}, nil)
	if err == nil {
		t.Error("expected error for unknown action")
	}
}

func TestSync_ResolvesEndpoints(t *testing.T) {
	bin := newFakeRclone(t)
	c, _ := New(Options{BinaryPath: bin, Logger: noopLogger()})
	// Missing Source/Dest/SourceRemote.
	_, err := c.Sync(context.Background(), SyncConfig{Action: ActionPush}, nil)
	if err == nil {
		t.Error("expected error for missing endpoints")
	}
}

func TestSync_Pull(t *testing.T) {
	bin := newFakeRclone(t)
	c, _ := New(Options{BinaryPath: bin, Logger: noopLogger()})
	res, err := c.Sync(context.Background(), SyncConfig{
		Action: ActionPull, Source: "remote:src", Dest: "remote:dst",
	}, nil)
	if err != nil {
		t.Fatal(err)
	}
	if res.ExitCode != 0 {
		t.Errorf("exit = %d, want 0", res.ExitCode)
	}
	if res.EndedAt < res.StartedAt {
		t.Error("EndedAt < StartedAt")
	}
}

func TestSync_Push(t *testing.T) {
	bin := newFakeRclone(t)
	c, _ := New(Options{BinaryPath: bin, Logger: noopLogger()})
	_, err := c.Sync(context.Background(), SyncConfig{
		Action: ActionPush, Source: "remote:src", Dest: "remote:dst",
	}, nil)
	if err != nil {
		t.Fatal(err)
	}
}

func TestSync_Bi_CreatesResyncPath(t *testing.T) {
	bin := newFakeRclone(t)
	c, _ := New(Options{BinaryPath: bin, Logger: noopLogger()})
	_, err := c.Sync(context.Background(), SyncConfig{
		Action: ActionBi, Source: "remote:src", Dest: "remote:dst",
	}, nil)
	if err != nil {
		t.Fatal(err)
	}
}

func TestSync_BiResync(t *testing.T) {
	bin := newFakeRclone(t)
	c, _ := New(Options{BinaryPath: bin, Logger: noopLogger()})
	_, err := c.Sync(context.Background(), SyncConfig{
		Action: ActionBiResync, Source: "remote:src", Dest: "remote:dst",
	}, nil)
	if err != nil {
		t.Fatal(err)
	}
}

func TestSync_Copy(t *testing.T) {
	bin := newFakeRclone(t)
	c, _ := New(Options{BinaryPath: bin, Logger: noopLogger()})
	_, err := c.Sync(context.Background(), SyncConfig{
		Action: ActionCopy, Source: "remote:src", Dest: "remote:dst",
	}, nil)
	if err != nil {
		t.Fatal(err)
	}
}

func TestSync_Move(t *testing.T) {
	bin := newFakeRclone(t)
	c, _ := New(Options{BinaryPath: bin, Logger: noopLogger()})
	_, err := c.Sync(context.Background(), SyncConfig{
		Action: ActionMove, Source: "remote:src", Dest: "remote:dst",
	}, nil)
	if err != nil {
		t.Fatal(err)
	}
}

func TestSync_Check(t *testing.T) {
	bin := newFakeRclone(t)
	c, _ := New(Options{BinaryPath: bin, Logger: noopLogger()})
	_, err := c.Sync(context.Background(), SyncConfig{
		Action: ActionCheck, Source: "remote:src", Dest: "remote:dst",
	}, nil)
	if err != nil {
		t.Fatal(err)
	}
}

func TestSync_DryRun(t *testing.T) {
	bin := newFakeRclone(t)
	c, _ := New(Options{BinaryPath: bin, Logger: noopLogger()})
	_, err := c.Sync(context.Background(), SyncConfig{
		Action: ActionDryRun, Source: "remote:src", Dest: "remote:dst",
	}, nil)
	if err != nil {
		t.Fatal(err)
	}
}

func TestSync_WithProfile(t *testing.T) {
	bin := newFakeRclone(t)
	c, _ := New(Options{BinaryPath: bin, Logger: noopLogger()})
	_, err := c.Sync(context.Background(), SyncConfig{
		Action: ActionPush, Source: "remote:src", Dest: "remote:dst",
		Profile: &ProfileFlags{
			Bandwidth: "10M", Transfers: 4, Checkers: 2, TpsLimit: 5.5,
			MinAge: "1d", MaxAge: "30d", MinSize: "1k", MaxSize: "1G",
			ExcludeIfPresent: ".lock", MaxDelete: 5,
			DryRun: true, NoUnicodeNormalize: true,
		},
	}, nil)
	if err != nil {
		t.Fatal(err)
	}
}

func TestSync_FailureCapturesStderr(t *testing.T) {
	bin := newFailRclone(t)
	c, _ := New(Options{BinaryPath: bin, Logger: noopLogger()})
	res, err := c.Sync(context.Background(), SyncConfig{
		Action: ActionPush, Source: "remote:src", Dest: "remote:dst",
	}, nil)
	if err == nil {
		t.Fatal("expected error")
	}
	if res == nil {
		t.Fatal("expected non-nil result on failure")
	}
	if res.ExitCode != 1 {
		t.Errorf("exit = %d, want 1", res.ExitCode)
	}
	if !strings.Contains(res.Stderr, "faked failure") {
		t.Errorf("stderr = %q, want 'faked failure'", res.Stderr)
	}
}

func TestSync_OnProgressCalled(t *testing.T) {
	dir := t.TempDir()
	bin := filepath.Join(dir, "rclone-stats")
	script := `#!/bin/sh
echo "2025/01/15 10:00:00 INFO  : TRANSFER: 1.024k/2.048k BYTES 10/20 ERRORS 0"
echo "2025/01/15 10:00:01 INFO  : CHECK: 5/10 TRANSFER: 1.024k/2.048k"
exit 0
`
	if err := os.WriteFile(bin, []byte(script), 0o755); err != nil {
		t.Fatal(err)
	}
	c, _ := New(Options{BinaryPath: bin, Logger: noopLogger()})
	var (
		mu        sync.Mutex
		lastStats Stats
		calls     int
	)
	_, err := c.Sync(context.Background(), SyncConfig{
		Action: ActionPush, Source: "remote:src", Dest: "remote:dst",
	}, func(s Stats) {
		mu.Lock()
		lastStats = s
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
	if lastStats.Bytes == 0 {
		t.Error("Bytes not parsed from stats line")
	}
}

func TestParseStatsLine_NoInfo(t *testing.T) {
	s := &Stats{}
	parseStatsLine("some non-info line", s)
	if s.Bytes != 0 || s.Checks != 0 || s.Errors != 0 {
		t.Errorf("non-info line modified stats: %+v", s)
	}
}

func TestParseStatsLine_Transfer(t *testing.T) {
	s := &Stats{}
	parseStatsLine("2025/01/15 10:00:00 INFO  : TRANSFER: 1k/2M BYTES", s)
	if s.Bytes != 1024 {
		t.Errorf("Bytes = %d", s.Bytes)
	}
	if s.BytesTotal != 2*1024*1024 {
		t.Errorf("BytesTotal = %d", s.BytesTotal)
	}
}

func TestParseStatsLine_Check(t *testing.T) {
	s := &Stats{}
	parseStatsLine("2025/01/15 10:00:00 INFO  : CHECK: 3/7", s)
	if s.Checks != 3 || s.ChecksTotal != 7 {
		t.Errorf("Checks = %d, ChecksTotal = %d", s.Checks, s.ChecksTotal)
	}
}

func TestParseStatsLine_Errors(t *testing.T) {
	s := &Stats{}
	parseStatsLine("2025/01/15 10:00:00 INFO  : ERRORS: 4", s)
	if s.Errors != 4 {
		t.Errorf("Errors = %d", s.Errors)
	}
}

func TestParseStatsLine_Deleted(t *testing.T) {
	s := &Stats{}
	parseStatsLine("2025/01/15 10:00:00 INFO  : DELETED: 12", s)
	if s.Deletes != 12 {
		t.Errorf("Deletes = %d", s.Deletes)
	}
}

func TestParseSize_Variants(t *testing.T) {
	cases := []struct {
		in   string
		want int64
	}{
		{"", 0},
		{"abc", 0},
		{"1024", 1024},
		{"1k", 1024},
		{"1K", 1024},
		{"1kb", 1024},
		{"1m", 1024 * 1024},
		{"1MB", 1024 * 1024},
		{"1g", 1024 * 1024 * 1024},
		{"1.5g", int64(1.5 * 1024 * 1024 * 1024)},
		{"1t", 1024 * 1024 * 1024 * 1024},
	}
	for _, c := range cases {
		if got := parseSize(c.in); got != c.want {
			t.Errorf("parseSize(%q) = %d, want %d", c.in, got, c.want)
		}
	}
}

func TestApplyResolverPolicy_RateLimited(t *testing.T) {
	for _, t2 := range []string{"drive", "onedrive", "dropbox", "box", "icloud", "iclouddrive",
		"googlephotos", "mega", "pcloud", "yandex", "mailru", "sharepoint"} {
		p := ApplyResolverPolicy(t2, "local")
		if p.Transfers != 4 || p.Checkers != 4 || p.TPSLimit != 4 {
			t.Errorf("rate-limited %s: %+v", t2, p)
		}
	}
}

func TestApplyResolverPolicy_Permissive(t *testing.T) {
	p := ApplyResolverPolicy("local", "local")
	if p.Transfers != 8 || p.Checkers != 8 || p.TPSLimit != 0 {
		t.Errorf("permissive: %+v", p)
	}
}

func TestIsRateLimited_Known(t *testing.T) {
	for _, t2 := range []string{"drive", "onedrive", "dropbox", "box", "icloud", "iclouddrive",
		"googlephotos", "mega", "pcloud", "yandex", "mailru", "sharepoint"} {
		if !isRateLimited(t2) {
			t.Errorf("%s should be rate-limited", t2)
		}
	}
}

func TestIsRateLimited_Unknown(t *testing.T) {
	for _, t2 := range []string{"local", "sftp", "s3", "unknown"} {
		if isRateLimited(t2) {
			t.Errorf("%s should not be rate-limited", t2)
		}
	}
}

func TestProfileToFlags_Nil(t *testing.T) {
	if flags := profileToFlags(nil); flags != nil {
		t.Errorf("nil Profile: %v", flags)
	}
}

func TestProfileToFlags_Empty(t *testing.T) {
	if flags := profileToFlags(&ProfileFlags{}); len(flags) != 0 {
		t.Errorf("empty Profile: %v", flags)
	}
}

func TestTruncate(t *testing.T) {
	if got := truncate("hi", 100); got != "hi" {
		t.Errorf("short = %q", got)
	}
	long := strings.Repeat("x", 200)
	got := truncate(long, 50)
	if !strings.HasPrefix(got, "xxxxx") {
		t.Errorf("truncated prefix wrong: %q", got)
	}
	if !strings.HasSuffix(got, "...(truncated)") {
		t.Errorf("truncated suffix wrong: %q", got)
	}
}

func TestNowUnix(t *testing.T) {
	if nowUnix() <= 0 {
		t.Errorf("nowUnix = %d, want positive", nowUnix())
	}
}

func TestResolveEndpoints_SourceDest(t *testing.T) {
	c := &Client{}
	src, dst, err := c.resolveEndpoints(SyncConfig{Source: "a", Dest: "b"})
	if err != nil || src != "a" || dst != "b" {
		t.Errorf("got src=%q dst=%q err=%v", src, dst, err)
	}
}

func TestResolveEndpoints_RemotePath(t *testing.T) {
	c := &Client{}
	src, dst, err := c.resolveEndpoints(SyncConfig{
		SourceRemote: "remote1", SourcePath: "/path1",
		DestRemote: "remote2", DestPath: "/path2",
	})
	if err != nil {
		t.Fatal(err)
	}
	if src != "remote1:/path1" || dst != "remote2:/path2" {
		t.Errorf("got src=%q dst=%q", src, dst)
	}
}

func TestSync_StartFailure_NoBinary(t *testing.T) {
	// New() validates the binary path and returns an error if it does not
	// exist, so Sync never gets a chance to run. Verify the early check.
	bin := "/nonexistent/rclone-binary-xyz"
	if _, err := New(Options{BinaryPath: bin, Logger: noopLogger()}); err == nil {
		t.Error("expected error when binary does not exist at construction")
	}
}

// stubCmd implements execCmd for tests; each step is independently controllable.
type stubCmd struct {
	stdoutPipeFn func() (io.ReadCloser, error)
	stderrPipeFn func() (io.ReadCloser, error)
	startFn      func() error
	waitFn       func() error

	startCalls int
}

func (s *stubCmd) StdoutPipe() (io.ReadCloser, error) { return s.stdoutPipeFn() }
func (s *stubCmd) StderrPipe() (io.ReadCloser, error) { return s.stderrPipeFn() }
func (s *stubCmd) Start() error {
	s.startCalls++
	return s.startFn()
}
func (s *stubCmd) Wait() error { return s.waitFn() }

func withNewExecCommand(t *testing.T, factory func(ctx context.Context, name string, args ...string) execCmd) {
	t.Helper()
	orig := newExecCommand
	t.Cleanup(func() { newExecCommand = orig })
	newExecCommand = factory
}

func TestExecute_StdoutPipeError(t *testing.T) {
	bin := newFakeRclone(t)
	c, err := New(Options{BinaryPath: bin, Logger: noopLogger()})
	if err != nil {
		t.Fatal(err)
	}
	withNewExecCommand(t, func(context.Context, string, ...string) execCmd {
		return &stubCmd{
			stdoutPipeFn: func() (io.ReadCloser, error) { return nil, errors.New("stdout pipe broken") },
		}
	})
	_, err = c.execute(context.Background(), []string{"version"}, nil, "")
	if err == nil {
		t.Fatal("expected error from stdout pipe")
	}
	if !strings.Contains(err.Error(), "rclone: stdout pipe") {
		t.Errorf("err = %q, want wrap of 'rclone: stdout pipe'", err)
	}
}

func TestExecute_StderrPipeError(t *testing.T) {
	bin := newFakeRclone(t)
	c, err := New(Options{BinaryPath: bin, Logger: noopLogger()})
	if err != nil {
		t.Fatal(err)
	}
	withNewExecCommand(t, func(context.Context, string, ...string) execCmd {
		return &stubCmd{
			stdoutPipeFn: func() (io.ReadCloser, error) { return io.NopCloser(strings.NewReader("")), nil },
			stderrPipeFn: func() (io.ReadCloser, error) { return nil, errors.New("stderr pipe broken") },
		}
	})
	_, err = c.execute(context.Background(), []string{"version"}, nil, "")
	if err == nil {
		t.Fatal("expected error from stderr pipe")
	}
	if !strings.Contains(err.Error(), "rclone: stderr pipe") {
		t.Errorf("err = %q, want wrap of 'rclone: stderr pipe'", err)
	}
}

func TestExecute_StartError(t *testing.T) {
	bin := newFakeRclone(t)
	c, err := New(Options{BinaryPath: bin, Logger: noopLogger()})
	if err != nil {
		t.Fatal(err)
	}
	withNewExecCommand(t, func(context.Context, string, ...string) execCmd {
		return &stubCmd{
			stdoutPipeFn: func() (io.ReadCloser, error) { return io.NopCloser(strings.NewReader("")), nil },
			stderrPipeFn: func() (io.ReadCloser, error) { return io.NopCloser(strings.NewReader("")), nil },
			startFn:      func() error { return errors.New("start failed") },
		}
	})
	_, err = c.execute(context.Background(), []string{"version"}, nil, "")
	if err == nil {
		t.Fatal("expected error from Start")
	}
	if !strings.Contains(err.Error(), "rclone: start") {
		t.Errorf("err = %q, want wrap of 'rclone: start'", err)
	}
}

func TestListRemotes_NameTrimmedToEmpty(t *testing.T) {
	// rclone prints "remote:" per line. A line that is JUST a colon
	// (":") has no name and must be skipped silently.
	dir := t.TempDir()
	bin := filepath.Join(dir, "rclone-justcolon")
	script := `#!/bin/sh
printf 'good:\n:\nalso-good:\n'
exit 0
`
	if err := os.WriteFile(bin, []byte(script), 0o755); err != nil {
		t.Fatal(err)
	}
	c, err := New(Options{BinaryPath: bin, Logger: noopLogger()})
	if err != nil {
		t.Fatal(err)
	}
	remotes, err := c.ListRemotes(context.Background())
	if err != nil {
		t.Fatal(err)
	}
	if len(remotes) != 2 {
		t.Errorf("expected 2 remotes (':' line skipped), got %d: %v", len(remotes), remotes)
	}
}
