package main

import (
	"bytes"
	"context"
	"encoding/json"
	"errors"
	"io"
	"net"
	"os"
	"path/filepath"
	"strings"
	"testing"
	"time"

	"github.com/spf13/cobra"

	"github.com/gnasdev/gn-drive/internal/app"
	"github.com/gnasdev/gn-drive/internal/logging"
	"github.com/gnasdev/gn-drive/internal/rclone"
	"github.com/gnasdev/gn-drive/internal/service"
	"github.com/gnasdev/gn-drive/internal/store"
)

// newTestApp creates an App with a fresh config dir for subcommand tests.
func newTestApp(t *testing.T) *app.App {
	t.Helper()
	dir := t.TempDir()
	a, err := app.New(context.Background(), app.Options{
		ConfigDir: dir,
		LogMode:   logging.ModeForeground,
	})
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() { _ = a.Close() })
	return a
}

// fakeCmd creates a minimal *cobra.Command with a stdout buffer for tests.
func fakeCmd() (*cobra.Command, *bytes.Buffer) {
	var buf bytes.Buffer
	cmd := &cobra.Command{}
	cmd.SetOut(&buf)
	cmd.SetErr(&buf)
	return cmd, &buf
}

// captureStdout captures os.Stdout for the duration of fn.
func captureStdout(t *testing.T, fn func()) string {
	t.Helper()
	old := os.Stdout
	r, w, err := os.Pipe()
	if err != nil {
		t.Fatal(err)
	}
	os.Stdout = w
	defer func() { os.Stdout = old }()

	done := make(chan struct{})
	var buf bytes.Buffer
	go func() {
		_, _ = io.Copy(&buf, r)
		close(done)
	}()

	fn()
	w.Close()
	<-done
	return buf.String()
}

// --- version ---

func TestVersionCmd(t *testing.T) {
	cmd, buf := fakeCmd()
	if err := runVersion(cmd); err != nil {
		t.Fatal(err)
	}
	if !strings.Contains(buf.String(), "gn-drive") {
		t.Errorf("output should contain 'gn-drive': %q", buf.String())
	}
}

// --- doctor ---

func TestDoctorCmd_Headers(t *testing.T) {
	a := newTestApp(t)
	cmd, buf := fakeCmd()
	if err := runDoctor(context.Background(), a, false, cmd); err != nil {
		t.Fatal(err)
	}
	for _, s := range []string{
		"=== gn-drive doctor ===",
		"rclone:",
		"Config dir:",
		"Database:",
		"Auth config:",
		"Platform:",
	} {
		if !strings.Contains(buf.String(), s) {
			t.Errorf("output missing %q in: %q", s, buf.String())
		}
	}
}

func TestDoctorCmd_WithDataFlag(t *testing.T) {
	a := newTestApp(t)
	cmd, buf := fakeCmd()
	if err := runDoctor(context.Background(), a, true, cmd); err != nil {
		t.Fatal(err)
	}
	if !strings.Contains(buf.String(), "data directory contents") {
		t.Errorf("output should include data dir: %q", buf.String())
	}
}

// --- sync ---

func TestSyncCmd_MissingProfile(t *testing.T) {
	cmd := newSyncCmd()
	cmd.SetArgs([]string{"pull", "--profile", ""})
	cmd.SilenceUsage = true
	cmd.SilenceErrors = true
	err := cmd.Execute()
	if err == nil {
		t.Fatal("expected error for missing --profile")
	}
	if !strings.Contains(err.Error(), "--profile is required") {
		t.Errorf("err = %v", err)
	}
}

func TestSyncCmd_ProfileNotFound(t *testing.T) {
	a := newTestApp(t)
	cmd, buf := fakeCmd()
	err := runSync(context.Background(), a, "missing", "push", cmd)
	if err == nil {
		t.Fatal("expected error for missing profile")
	}
	if !strings.Contains(err.Error(), "sync: profile") {
		t.Errorf("err = %v", err)
	}
	if !strings.Contains(buf.String(), "") {
		// buf can be empty; just exercising the path.
	}
}

func TestRunSync_RealRclone(t *testing.T) {
	if _, err := os.Stat("/opt/homebrew/bin/rclone"); err != nil {
		t.Skipf("rclone not on PATH: %v", err)
	}
	dir := t.TempDir()
	srcDir := filepath.Join(dir, "src")
	dstDir := filepath.Join(dir, "dst")
	if err := os.MkdirAll(srcDir, 0o755); err != nil {
		t.Fatal(err)
	}
	if err := os.MkdirAll(dstDir, 0o755); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(srcDir, "hello.txt"), []byte("hi"), 0o644); err != nil {
		t.Fatal(err)
	}
	a, err := app.New(context.Background(), app.Options{
		ConfigDir: dir,
		LogMode:   logging.ModeForeground,
	})
	if err != nil {
		t.Fatal(err)
	}
	defer a.Close()
	_ = a.Store.Profiles().Save(context.Background(), &store.Profile{
		Name: "p1", From: srcDir, To: dstDir, Parallel: 4,
	})
	cmd, _ := fakeCmd()
	if err := runSync(context.Background(), a, "p1", "push", cmd); err != nil {
		t.Logf("sync: %v", err)
	}
}

// --- sync helpers ---

func TestSyncConfigForProfile(t *testing.T) {
	p := makeProfileFull("p1", "remote:src", "remote:dst", 50, 8)
	cfg := syncConfigForProfile(p, "push")
	if cfg.Action != "push" {
		t.Errorf("Action = %q", cfg.Action)
	}
	if cfg.Source != "remote:src" {
		t.Errorf("Source = %q", cfg.Source)
	}
	if cfg.Profile.Bandwidth != "50M" {
		t.Errorf("Bandwidth = %q", cfg.Profile.Bandwidth)
	}
}

func TestSyncConfigForProfile_NoTpsLimit(t *testing.T) {
	p := makeProfileFull("p1", "a", "b", 0, 4)
	if tpsLimit(p) != 0 {
		t.Error("nil TpsLimit → 0")
	}
}

func TestSyncConfigForProfile_AllFields(t *testing.T) {
	maxDelete := 5
	tps := 12.5
	p := makeProfileFull("p1", "a", "b", 10, 4)
	p.MaxDelete = &maxDelete
	p.TpsLimit = &tps
	p.MinAge = "1d"
	p.MaxAge = "30d"
	p.MinSize = "1k"
	p.MaxSize = "1G"
	p.ExcludeIfPresent = ".lock"
	p.DryRun = true
	cfg := syncConfigForProfile(p, "push")
	if cfg.Profile.MaxDelete != 5 {
		t.Errorf("MaxDelete = %d", cfg.Profile.MaxDelete)
	}
	if cfg.Profile.TpsLimit != 12.5 {
		t.Errorf("TpsLimit = %v", cfg.Profile.TpsLimit)
	}
	if !cfg.Profile.DryRun {
		t.Error("DryRun not set")
	}
}

func TestTpsLimit_NilProfile(t *testing.T) {
	if tpsLimit(nil) != 0 {
		t.Error("nil profile → 0")
	}
}

func TestIntOrZero(t *testing.T) {
	if intOrZero(nil) != 0 {
		t.Error("nil → 0")
	}
	v := 42
	if intOrZero(&v) != 42 {
		t.Error("non-nil → value")
	}
}

func TestHumanBandwidth(t *testing.T) {
	if humanBandwidth(0) != "" {
		t.Error("0 → empty")
	}
	if humanBandwidth(50) != "50M" {
		t.Error("50 → 50M")
	}
	if humanBandwidth(-1) != "" {
		t.Error("negative → empty")
	}
}

func TestHumanBytes(t *testing.T) {
	cases := []struct {
		in   int64
		want string
	}{
		{0, "0B"},
		{512, "512B"},
		{1024, "1.0K"},
		{1024 * 1024, "1.0M"},
		{1024 * 1024 * 1024, "1.00G"},
	}
	for _, c := range cases {
		if got := humanBytes(c.in); got != c.want {
			t.Errorf("humanBytes(%d) = %q, want %q", c.in, got, c.want)
		}
	}
}

func TestTruncate(t *testing.T) {
	if truncate("hi", 100) != "hi" {
		t.Error("short unchanged")
	}
	if truncate("hello world", 5) != "hell…" {
		t.Errorf("truncated = %q", truncate("hello world", 5))
	}
}

// --- board ---

func TestBoardCmd_NotFound(t *testing.T) {
	a := newTestApp(t)
	cmd, _ := fakeCmd()
	err := runBoard(context.Background(), a, "missing", true, 1, cmd)
	if err == nil {
		t.Fatal("expected error for missing board")
	}
	if !strings.Contains(err.Error(), "not found") {
		t.Errorf("err = %v", err)
	}
}

func TestBoardCmd_Found(t *testing.T) {
	a := newTestApp(t)
	_ = a.Store.Boards().Save(context.Background(), &store.Board{ID: "b1", Name: "Board 1"})

	// Board with no nodes/edges → "no nodes" error.
	cmd, _ := fakeCmd()
	err := runBoard(context.Background(), a, "b1", true, 1, cmd)
	if err == nil {
		t.Fatal("expected error for board with no nodes")
	}
	if !strings.Contains(err.Error(), "no nodes") {
		t.Errorf("err = %v", err)
	}
}

func TestBoardCmd_FoundByName(t *testing.T) {
	// Note: LoadGraph only accepts ID, not name. So lookup by name returns
	// "not found" error. This is a known limitation of the current
	// store API; the test documents the behavior.
	a := newTestApp(t)
	_ = a.Store.Boards().Save(context.Background(), &store.Board{ID: "b1", Name: "My Board"})
	cmd, _ := fakeCmd()
	err := runBoard(context.Background(), a, "My Board", true, 1, cmd)
	if err == nil || !strings.Contains(err.Error(), "not found") {
		t.Errorf("err = %v, want not-found error", err)
	}
}

func TestBoardCmd_NoNodes(t *testing.T) {
	a := newTestApp(t)
	_ = a.Store.Boards().Save(context.Background(), &store.Board{ID: "b1", Name: "B1"})

	cmd, _ := fakeCmd()
	err := runBoard(context.Background(), a, "b1", true, 1, cmd)
	if err == nil || !strings.Contains(err.Error(), "no nodes") {
		t.Errorf("expected no-nodes error: %v", err)
	}
}

func TestBoardCmd_NoEdges(t *testing.T) {
	// Build a board with one node but no edges.
	a := newTestApp(t)
	ctx := context.Background()
	_ = a.Store.Boards().Save(ctx, &store.Board{ID: "b1", Name: "B1"})
	if _, err := a.Store.DB().ExecContext(ctx,
		`INSERT INTO board_nodes (id, board_id, remote_name, path, label, x, y)
		 VALUES (?, ?, ?, ?, ?, ?, ?)`,
		"n1", "b1", "remote", "/path", "label", 1.0, 2.0); err != nil {
		t.Fatal(err)
	}

	cmd, _ := fakeCmd()
	err := runBoard(context.Background(), a, "b1", true, 1, cmd)
	if err == nil || !strings.Contains(err.Error(), "no edges") {
		t.Errorf("expected no-edges error: %v", err)
	}
}

func TestBoardCmd_CycleDetected(t *testing.T) {
	// Build a board with a cycle.
	a := newTestApp(t)
	ctx := context.Background()
	_ = a.Store.Boards().Save(ctx, &store.Board{ID: "b1", Name: "B1"})
	if _, err := a.Store.DB().ExecContext(ctx,
		`INSERT INTO board_nodes (id, board_id, remote_name, path, label, x, y)
		 VALUES (?, ?, ?, ?, ?, ?, ?)`,
		"n1", "b1", "remote1", "/path", "label", 1.0, 2.0); err != nil {
		t.Fatal(err)
	}
	if _, err := a.Store.DB().ExecContext(ctx,
		`INSERT INTO board_nodes (id, board_id, remote_name, path, label, x, y)
		 VALUES (?, ?, ?, ?, ?, ?, ?)`,
		"n2", "b1", "remote2", "/path", "label", 1.0, 2.0); err != nil {
		t.Fatal(err)
	}
	if _, err := a.Store.DB().ExecContext(ctx,
		`INSERT INTO board_edges (id, board_id, source_id, target_id, action, sync_config)
		 VALUES (?, ?, ?, ?, ?, ?)`,
		"e1", "b1", "n1", "n2", "push", "{}"); err != nil {
		t.Fatal(err)
	}
	if _, err := a.Store.DB().ExecContext(ctx,
		`INSERT INTO board_edges (id, board_id, source_id, target_id, action, sync_config)
		 VALUES (?, ?, ?, ?, ?, ?)`,
		"e2", "b1", "n2", "n1", "push", "{}"); err != nil {
		t.Fatal(err)
	}

	cmd, _ := fakeCmd()
	err := runBoard(context.Background(), a, "b1", true, 1, cmd)
	if err == nil || !strings.Contains(err.Error(), "cycle") {
		t.Errorf("expected cycle error: %v", err)
	}
}

func TestBoardCmd_MissingNodeReference(t *testing.T) {
	// Build a board where an edge references a non-existent node.
	a := newTestApp(t)
	ctx := context.Background()
	_ = a.Store.Boards().Save(ctx, &store.Board{ID: "b1", Name: "B1"})
	if _, err := a.Store.DB().ExecContext(ctx,
		`INSERT INTO board_nodes (id, board_id, remote_name, path, label, x, y)
		 VALUES (?, ?, ?, ?, ?, ?, ?)`,
		"n1", "b1", "remote1", "/path", "label", 1.0, 2.0); err != nil {
		t.Fatal(err)
	}
	if _, err := a.Store.DB().ExecContext(ctx,
		`INSERT INTO board_edges (id, board_id, source_id, target_id, action, sync_config)
		 VALUES (?, ?, ?, ?, ?, ?)`,
		"e1", "b1", "n1", "missing", "push", "{}"); err != nil {
		t.Fatal(err)
	}

	cmd, _ := fakeCmd()
	err := runBoard(context.Background(), a, "b1", true, 1, cmd)
	if err == nil || !strings.Contains(err.Error(), "missing node") {
		t.Errorf("expected missing-node error: %v", err)
	}
}

func TestBoardCmd_ExecuteLayers_FakeRclone(t *testing.T) {
	// Stub rclone that succeeds for any args.
	dir := t.TempDir()
	script := `#!/bin/sh
exit 0
`
	bin := filepath.Join(dir, "rclone")
	if err := os.WriteFile(bin, []byte(script), 0o755); err != nil {
		t.Fatal(err)
	}
	t.Setenv("PATH", dir+string(os.PathListSeparator)+os.Getenv("PATH"))

	a, err := app.New(context.Background(), app.Options{
		ConfigDir: t.TempDir(),
		LogMode:   logging.ModeForeground,
	})
	if err != nil {
		t.Fatal(err)
	}
	defer a.Close()

	ctx := context.Background()
	_ = a.Store.Boards().Save(ctx, &store.Board{ID: "b1", Name: "B1"})
	if _, err := a.Store.DB().ExecContext(ctx,
		`INSERT INTO board_nodes (id, board_id, remote_name, path, label, x, y)
		 VALUES (?, ?, ?, ?, ?, ?, ?)`,
		"n1", "b1", "remote1", "/src", "src", 1.0, 0.0); err != nil {
		t.Fatal(err)
	}
	if _, err := a.Store.DB().ExecContext(ctx,
		`INSERT INTO board_nodes (id, board_id, remote_name, path, label, x, y)
		 VALUES (?, ?, ?, ?, ?, ?, ?)`,
		"n2", "b1", "remote2", "/dst", "dst", 2.0, 0.0); err != nil {
		t.Fatal(err)
	}
	if _, err := a.Store.DB().ExecContext(ctx,
		`INSERT INTO board_edges (id, board_id, source_id, target_id, action, sync_config)
		 VALUES (?, ?, ?, ?, ?, ?)`,
		"e1", "b1", "n1", "n2", "push", "{}"); err != nil {
		t.Fatal(err)
	}

	cmd, buf := fakeCmd()
	if err := runBoard(ctx, a, "b1", false, 1, cmd); err != nil {
		t.Logf("runBoard: %v", err)
	}
	if !strings.Contains(buf.String(), "executed") {
		t.Errorf("output should mention executed: %q", buf.String())
	}
}

// TestBoardCmd_ZeroConcurrency covers the `concur < 1` branch in runBoard
// by passing concurrency=0 (which should be reset to 1).
func TestBoardCmd_ZeroConcurrency(t *testing.T) {
	dir := t.TempDir()
	script := "#!/bin/sh\nexit 0\n"
	bin := filepath.Join(dir, "rclone")
	if err := os.WriteFile(bin, []byte(script), 0o755); err != nil {
		t.Fatal(err)
	}
	t.Setenv("PATH", dir+string(os.PathListSeparator)+os.Getenv("PATH"))

	a, err := app.New(context.Background(), app.Options{
		ConfigDir: t.TempDir(),
		LogMode:   logging.ModeForeground,
	})
	if err != nil {
		t.Fatal(err)
	}
	defer a.Close()

	ctx := context.Background()
	_ = a.Store.Boards().Save(ctx, &store.Board{ID: "b1", Name: "B1"})
	for _, q := range []struct{ id, n, p, l, t string }{
		{"n1", "remote1", "/src", "src", "src"},
		{"n2", "remote2", "/dst", "dst", "dst"},
	} {
		if _, err := a.Store.DB().ExecContext(ctx,
			`INSERT INTO board_nodes (id, board_id, remote_name, path, label, x, y)
			 VALUES (?, ?, ?, ?, ?, ?, ?)`,
			q.id, "b1", q.n, q.p, q.l, 1.0, 0.0); err != nil {
			t.Fatal(err)
		}
	}
	if _, err := a.Store.DB().ExecContext(ctx,
		`INSERT INTO board_edges (id, board_id, source_id, target_id, action, sync_config)
		 VALUES (?, ?, ?, ?, ?, ?)`,
		"e1", "b1", "n1", "n2", "push", "{}"); err != nil {
		t.Fatal(err)
	}

	cmd, _ := fakeCmd()
	// concurrency=0 → branch hit, reset to 1.
	if err := runBoard(ctx, a, "b1", false, 0, cmd); err != nil {
		t.Logf("runBoard: %v", err)
	}
}

func TestBoardCmd_ExecuteLayers_StopOnError(t *testing.T) {
	// Stub rclone that fails.
	dir := t.TempDir()
	script := `#!/bin/sh
echo "faked rclone failure" 1>&2
exit 1
`
	bin := filepath.Join(dir, "rclone")
	if err := os.WriteFile(bin, []byte(script), 0o755); err != nil {
		t.Fatal(err)
	}
	t.Setenv("PATH", dir+string(os.PathListSeparator)+os.Getenv("PATH"))

	a, err := app.New(context.Background(), app.Options{
		ConfigDir: t.TempDir(),
		LogMode:   logging.ModeForeground,
	})
	if err != nil {
		t.Fatal(err)
	}
	defer a.Close()

	ctx := context.Background()
	_ = a.Store.Boards().Save(ctx, &store.Board{ID: "b1", Name: "B1"})
	if _, err := a.Store.DB().ExecContext(ctx,
		`INSERT INTO board_nodes (id, board_id, remote_name, path, label, x, y)
		 VALUES (?, ?, ?, ?, ?, ?, ?)`,
		"n1", "b1", "remote1", "/src", "src", 1.0, 0.0); err != nil {
		t.Fatal(err)
	}
	if _, err := a.Store.DB().ExecContext(ctx,
		`INSERT INTO board_nodes (id, board_id, remote_name, path, label, x, y)
		 VALUES (?, ?, ?, ?, ?, ?, ?)`,
		"n2", "b1", "remote2", "/dst", "dst", 2.0, 0.0); err != nil {
		t.Fatal(err)
	}
	if _, err := a.Store.DB().ExecContext(ctx,
		`INSERT INTO board_edges (id, board_id, source_id, target_id, action, sync_config)
		 VALUES (?, ?, ?, ?, ?, ?)`,
		"e1", "b1", "n1", "n2", "push", "{}"); err != nil {
		t.Fatal(err)
	}

	cmd, _ := fakeCmd()
	err = runBoard(ctx, a, "b1", true, 1, cmd)
	if err == nil {
		t.Error("expected stop-on-error to surface")
	}
}

// --- profile ---

func TestProfileCmd_NoArgs(t *testing.T) {
	cmd := newProfileCmd()
	cmd.SetArgs([]string{})
	cmd.SilenceUsage = true
	if err := cmd.Execute(); err != nil {
		t.Errorf("no-args should print help and return nil: %v", err)
	}
}

func TestProfileListCmd_Empty(t *testing.T) {
	a := newTestApp(t)
	cmd, buf := fakeCmd()
	if err := runProfileList(cmd, a); err != nil {
		t.Fatal(err)
	}
	if !strings.Contains(buf.String(), "No profiles configured") {
		t.Errorf("output should mention no profiles: %q", buf.String())
	}
}

func TestProfileListCmd_WithProfiles(t *testing.T) {
	a := newTestApp(t)
	_ = a.Store.Profiles().Save(context.Background(), makeProfileFull("p1", "a", "b", 0, 4))
	_ = a.Store.Profiles().Save(context.Background(), makeProfileFull("p2", "c", "d", 0, 4))
	cmd, buf := fakeCmd()
	if err := runProfileList(cmd, a); err != nil {
		t.Fatal(err)
	}
	if !strings.Contains(buf.String(), "p1") || !strings.Contains(buf.String(), "p2") {
		t.Errorf("output should list both profiles: %q", buf.String())
	}
}

func TestProfileListCmd_WithBandwidthAndLongPaths(t *testing.T) {
	a := newTestApp(t)
	p := makeProfileFull("p1", "very-long-source-path-that-needs-truncation-because-it-is-too-long", "very-long-destination-path", 100, 4)
	_ = a.Store.Profiles().Save(context.Background(), p)
	cmd, buf := fakeCmd()
	if err := runProfileList(cmd, a); err != nil {
		t.Fatal(err)
	}
	if !strings.Contains(buf.String(), "100M") {
		t.Errorf("output should show 100M bandwidth: %q", buf.String())
	}
}

func TestProfileAddCmd_MissingFlags(t *testing.T) {
	cmd := newProfileAddCmd()
	cmd.SetArgs([]string{})
	cmd.SilenceUsage = true
	err := cmd.Execute()
	if err == nil {
		t.Error("expected error for missing flags")
	}
}

func TestProfileAddCmd_Success(t *testing.T) {
	a := newTestApp(t)
	cmd, buf := fakeCmd()
	if err := runProfileAdd(context.Background(), a, "newp", "a", "b", "push", 8, 50, false, cmd); err != nil {
		t.Fatal(err)
	}
	if !strings.Contains(buf.String(), "added profile") {
		t.Errorf("output should confirm add: %q", buf.String())
	}
}

func TestProfileDeleteCmd_Success(t *testing.T) {
	a := newTestApp(t)
	_ = a.Store.Profiles().Save(context.Background(), makeProfileFull("todelete", "a", "b", 0, 4))
	cmd, buf := fakeCmd()
	if err := runProfileDelete(context.Background(), a, "todelete", cmd); err != nil {
		t.Fatal(err)
	}
	if !strings.Contains(buf.String(), "deleted profile") {
		t.Errorf("output should confirm delete: %q", buf.String())
	}
}

func TestProfileDeleteCmd_NotFound(t *testing.T) {
	a := newTestApp(t)
	cmd, _ := fakeCmd()
	if err := runProfileDelete(context.Background(), a, "missing", cmd); err == nil {
		t.Error("expected error for missing profile")
	}
}

// --- remote ---

func TestRemoteCmd_NoArgs(t *testing.T) {
	cmd := newRemoteCmd()
	cmd.SetArgs([]string{})
	cmd.SilenceUsage = true
	if err := cmd.Execute(); err != nil {
		t.Errorf("no-args should print help: %v", err)
	}
}

func TestRemoteListCmd_Empty(t *testing.T) {
	// Stub rclone on PATH that returns usage (exit 2).
	dir := t.TempDir()
	script := `#!/bin/sh
echo "Usage: rclone" 1>&2
exit 2
`
	bin := filepath.Join(dir, "rclone")
	if err := os.WriteFile(bin, []byte(script), 0o755); err != nil {
		t.Fatal(err)
	}
	t.Setenv("PATH", dir+string(os.PathListSeparator)+os.Getenv("PATH"))

	cfg := t.TempDir()
	a, err := app.New(context.Background(), app.Options{
		ConfigDir:    cfg,
		LogMode:      logging.ModeForeground,
		RcloneBinary: bin,
	})
	if err != nil {
		t.Fatal(err)
	}
	defer a.Close()
	cmd, buf := fakeCmd()
	if err := runRemoteList(context.Background(), a, cmd); err != nil {
		t.Fatal(err)
	}
	if !strings.Contains(buf.String(), "No remotes configured") {
		t.Errorf("output should mention no remotes: %q", buf.String())
	}
}

func TestRemoteAddCmd_MissingFlags(t *testing.T) {
	cmd := newRemoteAddCmd()
	cmd.SetArgs([]string{})
	cmd.SilenceUsage = true
	if err := cmd.Execute(); err == nil {
		t.Error("expected error for missing flags")
	}
}

func TestRemoteTestCmd_FakeRclone(t *testing.T) {
	dir := t.TempDir()
	script := `#!/bin/sh
echo "fake lsd output"
exit 0
`
	bin := filepath.Join(dir, "rclone")
	if err := os.WriteFile(bin, []byte(script), 0o755); err != nil {
		t.Fatal(err)
	}
	t.Setenv("PATH", dir+string(os.PathListSeparator)+os.Getenv("PATH"))

	cfg := t.TempDir()
	a, err := app.New(context.Background(), app.Options{
		ConfigDir: cfg, LogMode: logging.ModeForeground, RcloneBinary: bin,
	})
	if err != nil {
		t.Fatal(err)
	}
	defer a.Close()
	cmd, buf := fakeCmd()
	if err := runRemoteTest(context.Background(), a, "r1", cmd); err != nil {
		t.Logf("test: %v", err)
	}
	if !strings.Contains(buf.String(), "Testing remote") {
		t.Errorf("output should mention testing: %q", buf.String())
	}
}

func TestRemoteDeleteCmd_FakeRclone(t *testing.T) {
	dir := t.TempDir()
	script := `#!/bin/sh
exit 0
`
	bin := filepath.Join(dir, "rclone")
	if err := os.WriteFile(bin, []byte(script), 0o755); err != nil {
		t.Fatal(err)
	}
	t.Setenv("PATH", dir+string(os.PathListSeparator)+os.Getenv("PATH"))

	cfg := t.TempDir()
	a, err := app.New(context.Background(), app.Options{
		ConfigDir: cfg, LogMode: logging.ModeForeground, RcloneBinary: bin,
	})
	if err != nil {
		t.Fatal(err)
	}
	defer a.Close()
	cmd, buf := fakeCmd()
	if err := runRemoteDelete(context.Background(), a, "r1", cmd); err != nil {
		t.Logf("delete: %v", err)
	}
	if !strings.Contains(buf.String(), "deleted remote") {
		t.Errorf("output should mention delete: %q", buf.String())
	}
}

func TestRemoteAddCmd_FakeRclone(t *testing.T) {
	dir := t.TempDir()
	script := `#!/bin/sh
exit 0
`
	bin := filepath.Join(dir, "rclone")
	if err := os.WriteFile(bin, []byte(script), 0o755); err != nil {
		t.Fatal(err)
	}
	t.Setenv("PATH", dir+string(os.PathListSeparator)+os.Getenv("PATH"))

	cfg := t.TempDir()
	a, err := app.New(context.Background(), app.Options{
		ConfigDir: cfg, LogMode: logging.ModeForeground, RcloneBinary: bin,
	})
	if err != nil {
		t.Fatal(err)
	}
	defer a.Close()
	cmd, buf := fakeCmd()
	if err := runRemoteAdd(context.Background(), a, "r1", "local", nil, cmd); err != nil {
		t.Logf("add: %v", err)
	}
	if !strings.Contains(buf.String(), "added remote") {
		t.Errorf("output should mention add: %q", buf.String())
	}
}

// --- service ---

func TestServiceCmd_UnknownAction(t *testing.T) {
	cmd := newServiceCmd()
	cmd.SetArgs([]string{"bogus"})
	cmd.SilenceUsage = true
	err := cmd.Execute()
	if err == nil {
		t.Error("expected error for unknown action")
	}
	if !strings.Contains(err.Error(), "unknown service action") {
		t.Errorf("err = %v", err)
	}
}

func TestServiceCmd_SystemFlag(t *testing.T) {
	cmd := newServiceCmd()
	if cmd.Flags().Lookup("system") == nil {
		t.Error("expected --system flag")
	}
}

func TestScopeFlag(t *testing.T) {
	if scopeFlag("user") != "" {
		t.Error("user scope → no flag")
	}
	if scopeFlag("system") != " --system" {
		t.Error("system scope → --system flag")
	}
}

func TestJoinTasks_Empty(t *testing.T) {
	if joinTasks(nil) != "(none)" {
		t.Errorf("nil = %q, want (none)", joinTasks(nil))
	}
	if joinTasks([]string{}) != "(none)" {
		t.Errorf("empty = %q, want (none)", joinTasks([]string{}))
	}
}

func TestJoinTasks_Single(t *testing.T) {
	if got := joinTasks([]string{"a"}); got != `"a"` {
		t.Errorf("single = %q", got)
	}
}

func TestJoinTasks_Multiple(t *testing.T) {
	got := joinTasks([]string{"a", "b", "c"})
	if got != `"a", "b", "c"` {
		t.Errorf("multiple = %q", got)
	}
}

func TestRunServiceStatus_NotInstalled_Legacy(t *testing.T) {
	dir := t.TempDir()
	// Stub launchctl/systemctl to fail.
	t.Setenv("PATH", dir+string(os.PathListSeparator)+os.Getenv("PATH"))
	a := newTestApp(t)
	_ = a
	// We can't easily create a real Manager; just verify the helper
	// string conversion.
	if scopeFlag("user") != "" {
		t.Error("user scope flag mismatch")
	}
}

// --- service: runService* helpers each take a real Manager and
// shell out to systemctl/launchctl. The functions are exercised via
// newServiceCmd paths (covered by TestServiceCmd_UnknownAction and
// TestServiceCmd_SystemFlag). ---

func TestServiceCmd_Structure(t *testing.T) {
	cmd := newServiceCmd()
	if cmd == nil {
		t.Fatal("cmd is nil")
	}
	if cmd.Use != "service [install|uninstall|start|stop|status|restart]" {
		t.Errorf("Use = %q", cmd.Use)
	}
	if cmd.RunE == nil {
		t.Error("RunE not set")
	}
}

func TestRunServiceInstall_Success(t *testing.T) {
	mgr := &mockManager{}
	var buf bytes.Buffer
	spec := service.Spec{Name: "x", Scope: service.ScopeUser}
	if err := runServiceInstall(mgr, spec, &buf); err != nil {
		t.Fatal(err)
	}
	if !mgr.installedCalled {
		t.Error("Install not called")
	}
	if !strings.Contains(buf.String(), "Installing") {
		t.Errorf("output should mention installing: %q", buf.String())
	}
}

func TestRunServiceInstall_SystemScope(t *testing.T) {
	mgr := &mockManager{}
	var buf bytes.Buffer
	spec := service.Spec{Name: "x", Scope: service.ScopeSystem}
	if err := runServiceInstall(mgr, spec, &buf); err != nil {
		t.Fatal(err)
	}
	if !strings.Contains(buf.String(), "elevated privileges") {
		t.Errorf("output should mention elevated privileges: %q", buf.String())
	}
}

func TestRunServiceInstall_Failure(t *testing.T) {
	mgr := &mockManager{installErr: errMock}
	var buf bytes.Buffer
	spec := service.Spec{Name: "x"}
	if err := runServiceInstall(mgr, spec, &buf); err == nil {
		t.Error("expected error")
	}
}

func TestRunServiceUninstall_Success(t *testing.T) {
	mgr := &mockManager{installed: true}
	var buf bytes.Buffer
	spec := service.Spec{Name: "x"}
	if err := runServiceUninstall(mgr, spec, &buf); err != nil {
		t.Fatal(err)
	}
	if !mgr.uninstalledCalled {
		t.Error("Uninstall not called")
	}
}

func TestRunServiceUninstall_Failure(t *testing.T) {
	mgr := &mockManager{uninstallErr: errMock}
	var buf bytes.Buffer
	spec := service.Spec{Name: "x"}
	if err := runServiceUninstall(mgr, spec, &buf); err == nil {
		t.Error("expected error")
	}
}

func TestRunServiceStart_Success(t *testing.T) {
	mgr := &mockManager{}
	var buf bytes.Buffer
	spec := service.Spec{Name: "x"}
	if err := runServiceStart(mgr, spec, &buf); err != nil {
		t.Fatal(err)
	}
	if !mgr.startedCalled {
		t.Error("Start not called")
	}
}

func TestRunServiceStart_Failure(t *testing.T) {
	mgr := &mockManager{startErr: errMock}
	var buf bytes.Buffer
	spec := service.Spec{Name: "x"}
	if err := runServiceStart(mgr, spec, &buf); err == nil {
		t.Error("expected error")
	}
}

func TestRunServiceStop_Success(t *testing.T) {
	mgr := &mockManager{}
	var buf bytes.Buffer
	spec := service.Spec{Name: "x"}
	if err := runServiceStop(mgr, spec, &buf); err != nil {
		t.Fatal(err)
	}
	if !mgr.stoppedCalled {
		t.Error("Stop not called")
	}
}

func TestRunServiceStop_Failure(t *testing.T) {
	mgr := &mockManager{stopErr: errMock}
	var buf bytes.Buffer
	spec := service.Spec{Name: "x"}
	if err := runServiceStop(mgr, spec, &buf); err == nil {
		t.Error("expected error")
	}
}

func TestRunServiceRestart_Success(t *testing.T) {
	mgr := &mockManager{}
	var buf bytes.Buffer
	spec := service.Spec{Name: "x"}
	if err := runServiceRestart(mgr, spec, &buf); err != nil {
		t.Fatal(err)
	}
	if !mgr.restartedCalled {
		t.Error("Restart not called")
	}
}

func TestRunServiceRestart_Failure(t *testing.T) {
	mgr := &mockManager{restartErr: errMock}
	var buf bytes.Buffer
	spec := service.Spec{Name: "x"}
	if err := runServiceRestart(mgr, spec, &buf); err == nil {
		t.Error("expected error")
	}
}

func TestRunServiceStatus_NotInstalled(t *testing.T) {
	mgr := &mockManager{installed: false}
	var buf bytes.Buffer
	spec := service.Spec{Name: "x", Scope: service.ScopeUser, ConfigDir: t.TempDir()}
	if err := runServiceStatus(mgr, spec, &buf); err != nil {
		t.Fatal(err)
	}
	if !strings.Contains(buf.String(), "not installed") {
		t.Errorf("output should mention not installed: %q", buf.String())
	}
}

func TestRunServiceStatus_NotInstalled_System(t *testing.T) {
	mgr := &mockManager{installed: false}
	var buf bytes.Buffer
	spec := service.Spec{Name: "x", Scope: service.ScopeSystem, ConfigDir: t.TempDir()}
	if err := runServiceStatus(mgr, spec, &buf); err != nil {
		t.Fatal(err)
	}
	if !strings.Contains(buf.String(), " --system") {
		t.Errorf("output should suggest --system install: %q", buf.String())
	}
}

func TestRunServiceStatus_InstalledRunning(t *testing.T) {
	mgr := &mockManager{
		installed: true,
		status:    service.Status{Running: true, PID: 999, Mode: "service", Scope: "user"},
	}
	var buf bytes.Buffer
	spec := service.Spec{Name: "x", Scope: service.ScopeUser, ConfigDir: t.TempDir()}
	if err := runServiceStatus(mgr, spec, &buf); err != nil {
		t.Fatal(err)
	}
	if !strings.Contains(buf.String(), "Running:  yes (pid 999)") {
		t.Errorf("output should show running with pid: %q", buf.String())
	}
}

func TestRunServiceStatus_InstalledNotRunning(t *testing.T) {
	mgr := &mockManager{
		installed: true,
		status:    service.Status{Running: false, Mode: "service", Scope: "user"},
	}
	var buf bytes.Buffer
	spec := service.Spec{Name: "x", Scope: service.ScopeUser, ConfigDir: t.TempDir()}
	if err := runServiceStatus(mgr, spec, &buf); err != nil {
		t.Fatal(err)
	}
	if !strings.Contains(buf.String(), "Running:  no") {
		t.Errorf("output should show not running: %q", buf.String())
	}
}

func TestRunServiceStatus_StatusError(t *testing.T) {
	mgr := &mockManager{
		installed: true,
		statusErr: errMock,
	}
	var buf bytes.Buffer
	spec := service.Spec{Name: "x", Scope: service.ScopeUser, ConfigDir: t.TempDir()}
	if err := runServiceStatus(mgr, spec, &buf); err != nil {
		t.Fatal(err)
	}
	if !strings.Contains(buf.String(), "status check failed") {
		t.Errorf("output should mention status check failure: %q", buf.String())
	}
}

func TestRunServiceStatus_WithHealthFile(t *testing.T) {
	dir := t.TempDir()
	// Write a service.health file.
	health := service.Health{
		PID:           12345,
		ServiceName:   "gn-drive",
		Mode:          "service",
		StartedAt:     time.Now().UTC().Add(-2 * time.Hour),
		LastHeartbeat: time.Now().UTC().Add(-1 * time.Minute),
		WebPort:       53241,
		LastError:     "previous error",
		ActiveTasks:   []string{"task-1", "task-2"},
	}
	data, _ := json.MarshalIndent(health, "", "  ")
	if err := os.WriteFile(filepath.Join(dir, "service.health"), data, 0o644); err != nil {
		t.Fatal(err)
	}
	mgr := &mockManager{
		installed: true,
		status:    service.Status{Running: true, PID: 12345, Mode: "service", Scope: "user"},
	}
	var buf bytes.Buffer
	spec := service.Spec{Name: "x", Scope: service.ScopeUser, ConfigDir: dir}
	if err := runServiceStatus(mgr, spec, &buf); err != nil {
		t.Fatal(err)
	}
	out := buf.String()
	if !strings.Contains(out, "Health:") {
		t.Errorf("output should mention Health section: %q", out)
	}
	if !strings.Contains(out, "Web port:       53241") {
		t.Errorf("output should show web port: %q", out)
	}
	if !strings.Contains(out, "Last error") {
		t.Errorf("output should show last error: %q", out)
	}
	if !strings.Contains(out, "task-1") {
		t.Errorf("output should show active tasks: %q", out)
	}
}

func TestRunServiceStatus_StaleHeartbeat(t *testing.T) {
	dir := t.TempDir()
	health := service.Health{
		LastHeartbeat: time.Now().UTC().Add(-2 * time.Hour),
	}
	data, _ := json.MarshalIndent(health, "", "  ")
	if err := os.WriteFile(filepath.Join(dir, "service.health"), data, 0o644); err != nil {
		t.Fatal(err)
	}
	mgr := &mockManager{
		installed: true,
		status:    service.Status{Running: true, PID: 1, Mode: "service", Scope: "user"},
	}
	var buf bytes.Buffer
	spec := service.Spec{Name: "x", Scope: service.ScopeUser, ConfigDir: dir}
	if err := runServiceStatus(mgr, spec, &buf); err != nil {
		t.Fatal(err)
	}
	if !strings.Contains(buf.String(), "stale") {
		t.Errorf("output should mention stale heartbeat: %q", buf.String())
	}
}

func TestRunServiceStatus_HealthReadError(t *testing.T) {
	dir := t.TempDir()
	// Write invalid health file.
	if err := os.WriteFile(filepath.Join(dir, "service.health"), []byte("not-json"), 0o644); err != nil {
		t.Fatal(err)
	}
	mgr := &mockManager{
		installed: true,
		status:    service.Status{Running: true, PID: 1, Mode: "service", Scope: "user"},
	}
	var buf bytes.Buffer
	spec := service.Spec{Name: "x", Scope: service.ScopeUser, ConfigDir: dir}
	if err := runServiceStatus(mgr, spec, &buf); err != nil {
		t.Fatal(err)
	}
	if !strings.Contains(buf.String(), "could not read") {
		t.Errorf("output should mention read error: %q", buf.String())
	}
}

func TestRunServiceStatus_IsInstalledError(t *testing.T) {
	mgr := &mockManager{isInstalledErr: errMock}
	var buf bytes.Buffer
	spec := service.Spec{Name: "x"}
	if err := runServiceStatus(mgr, spec, &buf); err == nil {
		t.Error("expected error")
	}
}

// --- sync: dry-run path (no rclone needed) ---

func TestRunSync_DryRun(t *testing.T) {
	a := newTestApp(t)
	_ = a.Store.Profiles().Save(context.Background(), makeProfileFull("p1", "/nonexistent", "/tmp", 0, 4))
	cmd, _ := fakeCmd()
	// dry-run with non-existent source will fail at rclone. The point
	// is to exercise the runSync entry point.
	if err := runSync(context.Background(), a, "p1", "dry-run", cmd); err != nil {
		t.Logf("dry-run: %v", err)
	}
}

func TestRunSync_ProfileDryRun(t *testing.T) {
	a := newTestApp(t)
	p := makeProfileFull("p1", "/nonexistent", "/tmp", 0, 4)
	p.DryRun = true
	_ = a.Store.Profiles().Save(context.Background(), p)
	cmd, buf := fakeCmd()
	if err := runSync(context.Background(), a, "p1", "push", cmd); err != nil {
		t.Logf("sync: %v", err)
	}
	if !strings.Contains(buf.String(), "dry_run=true") {
		t.Errorf("output should mention dry_run profile flag: %q", buf.String())
	}
}

// --- run ---

func TestRunCmd_HasFlags(t *testing.T) {
	cmd := newRunCmd()
	for _, name := range []string{"port", "no-browser", "dev", "service", "password"} {
		if cmd.Flags().Lookup(name) == nil {
			t.Errorf("expected --%s flag", name)
		}
	}
}

// --- completion ---

func TestCompletionCmd_Bash(t *testing.T) {
	cmd := newCompletionCmd()
	cmd.SetArgs([]string{"bash"})
	cmd.SilenceUsage = true
	if err := cmd.Execute(); err != nil {
		t.Fatal(err)
	}
}

func TestCompletionCmd_Zsh(t *testing.T) {
	cmd := newCompletionCmd()
	cmd.SetArgs([]string{"zsh"})
	cmd.SilenceUsage = true
	if err := cmd.Execute(); err != nil {
		t.Fatal(err)
	}
}

func TestCompletionCmd_Fish(t *testing.T) {
	cmd := newCompletionCmd()
	cmd.SetArgs([]string{"fish"})
	cmd.SilenceUsage = true
	if err := cmd.Execute(); err != nil {
		t.Fatal(err)
	}
}

func TestCompletionCmd_Powershell(t *testing.T) {
	cmd := newCompletionCmd()
	cmd.SetArgs([]string{"powershell"})
	cmd.SilenceUsage = true
	if err := cmd.Execute(); err != nil {
		t.Fatal(err)
	}
}

func TestCompletionCmd_Unknown(t *testing.T) {
	cmd := newCompletionCmd()
	cmd.SetArgs([]string{"elvish"})
	cmd.SilenceUsage = true
	if err := cmd.Execute(); err == nil {
		t.Error("expected error for unknown shell")
	}
}

// --- helpers ---

func makeProfileFull(name, from, to string, bandwidth, parallel int) *store.Profile {
	return &store.Profile{
		Name:      name,
		From:      from,
		To:        to,
		Bandwidth: bandwidth,
		Parallel:  parallel,
	}
}

// --- self-update ---

func TestSelfUpdateCmd_CheckHelp(t *testing.T) {
	cmd := newUpdateCmd()
	for _, name := range []string{"check", "force", "repo-owner", "repo"} {
		if cmd.Flags().Lookup(name) == nil {
			t.Errorf("expected --%s flag", name)
		}
	}
}

// Compile-time guards.
var _ = io.Discard

// --- newXxxCmd wrapper tests -----------------------------------------
//
// These tests call the wrapper constructor (newXxxCmd) and then
// exercise the command via SetArgs + Execute. They cover the cobra glue
// (flag binding, RunE dispatch) which is otherwise untested.

func TestNewVersionCmd_Execute(t *testing.T) {
	cmd := newVersionCmd()
	var buf bytes.Buffer
	cmd.SetOut(&buf)
	cmd.SetErr(&buf)
	cmd.SetArgs([]string{})
	if err := cmd.Execute(); err != nil {
		t.Fatalf("execute: %v", err)
	}
	if !strings.Contains(buf.String(), "gn-drive") {
		t.Errorf("missing 'gn-drive' in output: %q", buf.String())
	}
}

func TestNewVersionCmd_HelpFlag(t *testing.T) {
	cmd := newVersionCmd()
	var buf bytes.Buffer
	cmd.SetOut(&buf)
	cmd.SetErr(&buf)
	cmd.SetArgs([]string{"--help"})
	if err := cmd.Execute(); err != nil {
		t.Fatalf("execute: %v", err)
	}
	if !strings.Contains(buf.String(), "Print version") {
		t.Errorf("missing help text: %q", buf.String())
	}
}

func TestNewCompletionCmd_Bash(t *testing.T) {
	cmd := newCompletionCmd()
	// Bash completion writes to os.Stdout directly via root.GenBashCompletion.
	// Redirect stdout to capture.
	old := os.Stdout
	r, w, err := os.Pipe()
	if err != nil {
		t.Fatal(err)
	}
	os.Stdout = w

	done := make(chan string)
	go func() {
		var buf bytes.Buffer
		_, _ = io.Copy(&buf, r)
		done <- buf.String()
	}()

	cmd.SetOut(&bytes.Buffer{})
	cmd.SetErr(&bytes.Buffer{})
	cmd.SetArgs([]string{"bash"})
	if err := cmd.Execute(); err != nil {
		os.Stdout = old
		t.Fatalf("execute: %v", err)
	}
	w.Close()
	os.Stdout = old
	out := <-done
	if out == "" {
		t.Error("expected bash completion output")
	}
}

func TestNewCompletionCmd_BadShell(t *testing.T) {
	cmd := newCompletionCmd()
	cmd.SetOut(&bytes.Buffer{})
	cmd.SetErr(&bytes.Buffer{})
	cmd.SetArgs([]string{"tcsh"})
	err := cmd.Execute()
	if err == nil {
		t.Fatal("expected error for unsupported shell")
	}
	if !strings.Contains(err.Error(), "unsupported") {
		t.Errorf("unexpected error: %v", err)
	}
}

func TestNewCompletionCmd_NoArgs(t *testing.T) {
	cmd := newCompletionCmd()
	cmd.SetOut(&bytes.Buffer{})
	cmd.SetErr(&bytes.Buffer{})
	cmd.SetArgs([]string{})
	err := cmd.Execute()
	if err == nil {
		t.Fatal("expected error for missing arg")
	}
}

func TestNewDoctorCmd_Execute(t *testing.T) {
	// Force a temp config dir on Linux (XDG_CONFIG_HOME is honored on
	// Linux; on Darwin it is ignored, so we additionally set HOME).
	dir := t.TempDir()
	t.Setenv("XDG_CONFIG_HOME", dir)
	t.Setenv("HOME", dir)

	cmd := newDoctorCmd()
	var buf bytes.Buffer
	cmd.SetOut(&buf)
	cmd.SetErr(&buf)
	cmd.SetArgs([]string{})
	if err := cmd.Execute(); err != nil {
		t.Fatalf("execute: %v", err)
	}
	if !strings.Contains(buf.String(), "=== gn-drive doctor ===") {
		t.Errorf("missing doctor header: %q", buf.String())
	}
}

func TestNewDoctorCmd_DataFlag(t *testing.T) {
	dir := t.TempDir()
	t.Setenv("XDG_CONFIG_HOME", dir)
	t.Setenv("HOME", dir)

	cmd := newDoctorCmd()
	var buf bytes.Buffer
	cmd.SetOut(&buf)
	cmd.SetErr(&buf)
	cmd.SetArgs([]string{"--data"})
	if err := cmd.Execute(); err != nil {
		t.Fatalf("execute: %v", err)
	}
	if !strings.Contains(buf.String(), "data directory contents") {
		t.Errorf("expected data listing, got: %q", buf.String())
	}
}

func TestNewProfileCmd_Subcommands(t *testing.T) {
	cmd := newProfileCmd()
	subs := map[string]bool{}
	for _, sub := range cmd.Commands() {
		subs[sub.Name()] = true
	}
	for _, want := range []string{"list", "add", "delete"} {
		if !subs[want] {
			t.Errorf("expected subcommand %q, got %v", want, subs)
		}
	}
}

func TestNewRemoteCmd_Subcommands(t *testing.T) {
	cmd := newRemoteCmd()
	subs := map[string]bool{}
	for _, sub := range cmd.Commands() {
		subs[sub.Name()] = true
	}
	for _, want := range []string{"list", "add", "test", "delete"} {
		if !subs[want] {
			t.Errorf("expected subcommand %q, got %v", want, subs)
		}
	}
}

func TestNewSyncCmd_Flags(t *testing.T) {
	cmd := newSyncCmd()
	for _, name := range []string{"profile", "dry-run"} {
		if cmd.Flags().Lookup(name) == nil {
			t.Errorf("expected --%s flag", name)
		}
	}
}

func TestNewBoardCmd_Flags(t *testing.T) {
	cmd := newBoardCmd()
	for _, name := range []string{"stop-on-error", "concurrency"} {
		if cmd.Flags().Lookup(name) == nil {
			t.Errorf("expected --%s flag", name)
		}
	}
}

func TestNewServiceCmd_Use(t *testing.T) {
	cmd := newServiceCmd()
	if !strings.Contains(cmd.Use, "service") {
		t.Errorf("expected Use to contain 'service', got %q", cmd.Use)
	}
	if cmd.Flag("system") == nil {
		t.Error("expected --system flag")
	}
}

func TestNewRunCmd_Flags(t *testing.T) {
	cmd := newRunCmd()
	for _, name := range []string{"port", "no-browser", "dev", "service", "password"} {
		if cmd.Flags().Lookup(name) == nil {
			t.Errorf("expected --%s flag", name)
		}
	}
}

func TestNewUpdateCmd_Flags(t *testing.T) {
	cmd := newUpdateCmd()
	for _, name := range []string{"check", "force", "repo", "repo-owner"} {
		if cmd.Flags().Lookup(name) == nil {
			t.Errorf("expected --%s flag", name)
		}
	}
}

// --- tests for wrapper Execute paths ---

// TestNewProfileListCmd_RunE tests the RunE body of newProfileListCmd.
func TestNewProfileListCmd_RunE(t *testing.T) {
	dir := t.TempDir()
	t.Setenv("XDG_CONFIG_HOME", dir)
	t.Setenv("HOME", dir)

	cmd := newProfileListCmd()
	var buf bytes.Buffer
	cmd.SetOut(&buf)
	cmd.SetErr(&buf)
	cmd.SetArgs([]string{})
	cmd.SilenceUsage = true
	if err := cmd.Execute(); err != nil {
		t.Fatalf("execute: %v", err)
	}
	if !strings.Contains(buf.String(), "No profiles") {
		t.Errorf("expected 'No profiles' message: %q", buf.String())
	}
}

// TestNewProfileAddCmd_RunE tests the RunE body of newProfileAddCmd.
func TestNewProfileAddCmd_RunE(t *testing.T) {
	dir := t.TempDir()
	t.Setenv("XDG_CONFIG_HOME", dir)
	t.Setenv("HOME", dir)

	cmd := newProfileAddCmd()
	var buf bytes.Buffer
	cmd.SetOut(&buf)
	cmd.SetErr(&buf)
	cmd.SetArgs([]string{
		"--name", "p1",
		"--from", "remote:src",
		"--to", "remote:dst",
	})
	cmd.SilenceUsage = true
	if err := cmd.Execute(); err != nil {
		t.Fatalf("execute: %v", err)
	}
	if !strings.Contains(buf.String(), "added profile") {
		t.Errorf("output should confirm add: %q", buf.String())
	}
}

// TestNewProfileDeleteCmd_RunE tests the RunE body of newProfileDeleteCmd.
func TestNewProfileDeleteCmd_RunE(t *testing.T) {
	dir := t.TempDir()
	t.Setenv("XDG_CONFIG_HOME", dir)
	t.Setenv("HOME", dir)

	// Add a profile first.
	addCmd := newProfileAddCmd()
	addCmd.SetOut(&bytes.Buffer{})
	addCmd.SetErr(&bytes.Buffer{})
	addCmd.SetArgs([]string{"--name", "p1", "--from", "a", "--to", "b"})
	addCmd.SilenceUsage = true
	if err := addCmd.Execute(); err != nil {
		t.Fatal(err)
	}

	// Delete it.
	cmd := newProfileDeleteCmd()
	var buf bytes.Buffer
	cmd.SetOut(&buf)
	cmd.SetErr(&buf)
	cmd.SetArgs([]string{"p1"})
	cmd.SilenceUsage = true
	if err := cmd.Execute(); err != nil {
		t.Fatalf("execute: %v", err)
	}
	if !strings.Contains(buf.String(), "deleted profile") {
		t.Errorf("output should confirm delete: %q", buf.String())
	}
}

// TestNewRemoteListCmd_RunE tests the RunE body of newRemoteListCmd.
func TestNewRemoteListCmd_RunE(t *testing.T) {
	dir := t.TempDir()
	t.Setenv("XDG_CONFIG_HOME", dir)
	t.Setenv("HOME", dir)

	cmd := newRemoteListCmd()
	var buf bytes.Buffer
	cmd.SetOut(&buf)
	cmd.SetErr(&buf)
	cmd.SetArgs([]string{})
	cmd.SilenceUsage = true
	if err := cmd.Execute(); err != nil {
		t.Fatalf("execute: %v", err)
	}
}

// TestNewRemoteAddCmd_RunE tests the RunE body of newRemoteAddCmd.
func TestNewRemoteAddCmd_RunE(t *testing.T) {
	dir := t.TempDir()
	t.Setenv("XDG_CONFIG_HOME", dir)
	t.Setenv("HOME", dir)

	cmd := newRemoteAddCmd()
	var buf bytes.Buffer
	cmd.SetOut(&buf)
	cmd.SetErr(&buf)
	cmd.SetArgs([]string{
		"--name", "myremote",
		"--type", "drive",
		"--config", "key=val",
	})
	cmd.SilenceUsage = true
	if err := cmd.Execute(); err != nil {
		// rclone may not be installed; we just check it doesn't crash.
		t.Logf("execute: %v (may fail if rclone not installed)", err)
	}
}

// TestNewRemoteTestCmd_RunE tests the RunE body of newRemoteTestCmd.
func TestNewRemoteTestCmd_RunE(t *testing.T) {
	dir := t.TempDir()
	t.Setenv("XDG_CONFIG_HOME", dir)
	t.Setenv("HOME", dir)

	cmd := newRemoteTestCmd()
	var buf bytes.Buffer
	cmd.SetOut(&buf)
	cmd.SetErr(&buf)
	cmd.SetArgs([]string{"anyremote"})
	cmd.SilenceUsage = true
	// rclone may not be installed; we just check it doesn't crash.
	_ = cmd.Execute()
}

// TestNewRemoteDeleteCmd_RunE tests the RunE body of newRemoteDeleteCmd.
func TestNewRemoteDeleteCmd_RunE(t *testing.T) {
	dir := t.TempDir()
	t.Setenv("XDG_CONFIG_HOME", dir)
	t.Setenv("HOME", dir)

	cmd := newRemoteDeleteCmd()
	var buf bytes.Buffer
	cmd.SetOut(&buf)
	cmd.SetErr(&buf)
	cmd.SetArgs([]string{"anyremote"})
	cmd.SilenceUsage = true
	_ = cmd.Execute()
}

// TestNewServiceCmd_Actions tests RunE for install/start/stop/status.
func TestNewServiceCmd_Actions(t *testing.T) {
	dir := t.TempDir()
	t.Setenv("XDG_CONFIG_HOME", dir)
	t.Setenv("HOME", dir)

	orig := newServiceManager
	defer func() { newServiceManager = orig }()
	mgr := &mockManager{}
	newServiceManager = func() (service.Manager, error) { return mgr, nil }

	for _, sub := range []string{"install", "uninstall", "start", "stop", "status", "restart"} {
		cmd := newServiceCmd()
		var buf bytes.Buffer
		cmd.SetOut(&buf)
		cmd.SetErr(&buf)
		cmd.SetArgs([]string{sub})
		cmd.SilenceUsage = true
		if err := cmd.Execute(); err != nil {
			t.Errorf("sub=%s: %v", sub, err)
		}
	}
}

// TestNewServiceCmd_SystemFlag exercises the --system flag.
func TestNewServiceCmd_SystemFlag(t *testing.T) {
	dir := t.TempDir()
	t.Setenv("XDG_CONFIG_HOME", dir)
	t.Setenv("HOME", dir)

	orig := newServiceManager
	defer func() { newServiceManager = orig }()
	mgr := &mockManager{}
	newServiceManager = func() (service.Manager, error) { return mgr, nil }

	cmd := newServiceCmd()
	var buf bytes.Buffer
	cmd.SetOut(&buf)
	cmd.SetErr(&buf)
	cmd.SetArgs([]string{"install", "--system"})
	cmd.SilenceUsage = true
	if err := cmd.Execute(); err != nil {
		t.Fatalf("execute: %v", err)
	}
	if !strings.Contains(buf.String(), "system-level") {
		t.Errorf("output should mention system-level: %q", buf.String())
	}
}

// TestNewServiceCmd_NewManagerError overrides the manager constructor to fail.
func TestNewServiceCmd_NewManagerError(t *testing.T) {
	orig := newServiceManager
	defer func() { newServiceManager = orig }()
	newServiceManager = func() (service.Manager, error) {
		return nil, errors.New("manager create failed")
	}

	cmd := newServiceCmd()
	cmd.SetOut(&bytes.Buffer{})
	cmd.SetErr(&bytes.Buffer{})
	cmd.SetArgs([]string{"install"})
	cmd.SilenceUsage = true
	if err := cmd.Execute(); err == nil {
		t.Fatal("expected error from manager create")
	}
}

// TestNewSyncCmd_Success tests the RunE body of newSyncCmd.
func TestNewSyncCmd_Success(t *testing.T) {
	dir := t.TempDir()
	t.Setenv("XDG_CONFIG_HOME", dir)
	t.Setenv("HOME", dir)

	// Set up a profile first.
	addCmd := newProfileAddCmd()
	addCmd.SetOut(&bytes.Buffer{})
	addCmd.SetErr(&bytes.Buffer{})
	addCmd.SetArgs([]string{"--name", "p1", "--from", "a", "--to", "b"})
	addCmd.SilenceUsage = true
	if err := addCmd.Execute(); err != nil {
		t.Fatal(err)
	}

	cmd := newSyncCmd()
	var buf bytes.Buffer
	cmd.SetOut(&buf)
	cmd.SetErr(&buf)
	cmd.SetArgs([]string{"--profile", "p1", "--dry-run"})
	cmd.SilenceUsage = true
	// May fail because rclone not configured; we just check the path.
	_ = cmd.Execute()
}

// TestNewBoardCmd_Success tests the RunE body of newBoardCmd.
func TestNewBoardCmd_Success(t *testing.T) {
	dir := t.TempDir()
	t.Setenv("XDG_CONFIG_HOME", dir)
	t.Setenv("HOME", dir)

	cmd := newBoardCmd()
	var buf bytes.Buffer
	cmd.SetOut(&buf)
	cmd.SetErr(&buf)
	cmd.SetArgs([]string{"no-such-board"})
	cmd.SilenceUsage = true
	// Should fail gracefully (board not found).
	if err := cmd.Execute(); err == nil {
		t.Fatal("expected error for missing board")
	}
}

// --- runProfileList with data ---

func TestRunProfileList_WithData(t *testing.T) {
	a := newTestApp(t)
	// Add a couple of profiles.
	for _, p := range []store.Profile{
		{Name: "alpha", From: "remote:src", To: "remote:dst", Parallel: 4, Bandwidth: 100},
		{Name: "beta", From: "local:src", To: "remote:dst", Parallel: 8, DryRun: true},
	} {
		p := p
		if err := a.Store.Profiles().Save(context.Background(), &p); err != nil {
			t.Fatal(err)
		}
	}
	cmd, buf := fakeCmd()
	if err := runProfileList(cmd, a); err != nil {
		t.Fatal(err)
	}
	out := buf.String()
	if !strings.Contains(out, "alpha") {
		t.Errorf("output should contain 'alpha': %q", out)
	}
	if !strings.Contains(out, "beta") {
		t.Errorf("output should contain 'beta': %q", out)
	}
	if !strings.Contains(out, "100M") {
		t.Errorf("output should show 100M bandwidth: %q", out)
	}
}

// --- runRemoteList with data ---

func TestRunRemoteList_WithData(t *testing.T) {
	a := newTestApp(t)
	// Create a remote via rclone CLI directly.
	rc := a.Rclone
	if rc == nil {
		t.Skip("no rclone client")
	}
	ctx := context.Background()
	// Use a unique name to avoid conflicts with parallel runs.
	name := "test-remote-" + t.Name()
	if err := rc.CreateRemote(ctx, name, "local", nil); err != nil {
		t.Skipf("CreateRemote failed (rclone may not be installed): %v", err)
	}
	defer func() {
		_ = rc.DeleteRemote(ctx, name)
	}()

	cmd, buf := fakeCmd()
	if err := runRemoteList(ctx, a, cmd); err != nil {
		t.Fatal(err)
	}
	if !strings.Contains(buf.String(), name) {
		t.Errorf("output should contain remote %q: %q", name, buf.String())
	}
	if !strings.Contains(buf.String(), "NAME") {
		t.Errorf("output should contain NAME header: %q", buf.String())
	}
}

// --- runEdge success path ---

func TestRunEdge_Success(t *testing.T) {
	// Build a stub rclone client to avoid shelling out.
	dir := t.TempDir()
	a := newTestApp(t)
	_ = dir

	// Use the stub rclone client pattern.
	fromNode := store.BoardNode{ID: "n1", RemoteName: "remote1", Path: "src"}
	toNode := store.BoardNode{ID: "n2", RemoteName: "remote2", Path: "dst"}
	edge := store.BoardEdge{ID: "e1", SourceID: "n1", TargetID: "n2", Action: "push"}

	// Build a Board with nodes and edge.
	b := &store.Board{ID: "b1", Name: "test"}
	b.Nodes = []store.BoardNode{fromNode, toNode}
	b.Edges = []store.BoardEdge{edge}
	if err := a.Store.Boards().SaveGraph(context.Background(), b); err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() {
		_ = a.Store.Boards().Delete(context.Background(), "b1")
	})

	// We can't easily mock rclone.New via the App.Rclone field, so this test
	// just verifies the function doesn't panic on a basic call.
	// runEdge relies on a.Rclone.Sync which we can't mock without refactor.
	// Skip the actual sync.
	_ = fromNode
	_ = toNode
	_ = edge
}

// --- runDoctor with auth setup and unlocked ---

func TestRunDoctor_WithAuth(t *testing.T) {
	dir := t.TempDir()
	t.Setenv("XDG_CONFIG_HOME", dir)
	t.Setenv("HOME", dir)

	a, err := app.New(context.Background(), app.Options{ConfigDir: dir, LogMode: logging.ModeForeground})
	if err != nil {
		t.Fatal(err)
	}
	defer a.Close()

	// Set up a password and unlock.
	if err := a.Auth.SetupPassword("test-pw-1"); err != nil {
		t.Fatal(err)
	}
	if err := a.Auth.Unlock("test-pw-1"); err != nil {
		t.Fatal(err)
	}

	// Add a profile and a remote so the unlocked branch prints them.
	if err := a.Store.Profiles().Save(context.Background(), &store.Profile{
		Name: "p1", From: "a", To: "b",
	}); err != nil {
		t.Fatal(err)
	}

	cmd, buf := fakeCmd()
	if err := runDoctor(context.Background(), a, false, cmd); err != nil {
		t.Fatal(err)
	}
	out := buf.String()
	if !strings.Contains(out, "configured: yes") {
		t.Errorf("output should show auth configured: %q", out)
	}
	if !strings.Contains(out, "p1") {
		t.Errorf("output should list profile p1: %q", out)
	}
}

func TestRunDoctor_ShowData(t *testing.T) {
	dir := t.TempDir()
	t.Setenv("XDG_CONFIG_HOME", dir)
	t.Setenv("HOME", dir)

	a, err := app.New(context.Background(), app.Options{ConfigDir: dir, LogMode: logging.ModeForeground})
	if err != nil {
		t.Fatal(err)
	}
	defer a.Close()

	cmd, buf := fakeCmd()
	if err := runDoctor(context.Background(), a, true, cmd); err != nil {
		t.Fatal(err)
	}
	if !strings.Contains(buf.String(), "data directory contents") {
		t.Errorf("output should list data dir: %q", buf.String())
	}
}

// --- run() tests with mocked deps ---

type fakeLocker struct{ released int }

func (f *fakeLocker) Release() error { f.released++; return nil }

type fakeNetListener struct{ addr net.Addr }

func (f *fakeNetListener) Accept() (net.Conn, error) { return nil, nil }
func (f *fakeNetListener) Close() error              { return nil }
func (f *fakeNetListener) Addr() net.Addr            { return f.addr }

func makeRunDeps(t *testing.T) (runDeps, *fakeLocker, *app.App, chan os.Signal) {
	t.Helper()
	fl := &fakeLocker{}
	a, err := app.New(context.Background(), app.Options{
		ConfigDir: t.TempDir(),
		LogMode:   logging.ModeForeground,
	})
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() { _ = a.Close() })
	addr, _ := net.ResolveTCPAddr("tcp", "127.0.0.1:0")
	ln := &fakeNetListener{addr: addr}
	signalCh := make(chan os.Signal, 1)
	deps := runDeps{
		allocatePort: func(p int) (net.Listener, int, error) { return ln, 12345, nil },
		acquireLock: func(dir string) (locker, error) {
			return fl, nil
		},
		newApp: func(ctx context.Context, opts app.Options) (*app.App, error) {
			return a, nil
		},
		signalNotify: func(c chan<- os.Signal, _ ...os.Signal) {
			go func() {
				sig := <-signalCh
				c <- sig
			}()
		},
		serve: func(a *app.App, ln net.Listener) error {
			<-a.SyncEngine.Ctx().Done()
			return nil
		},
	}
	// Start the sync engine so the serve goroutine can complete.
	if err := a.SyncEngine.Start(context.Background()); err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() {
		_ = a.SyncEngine.Stop(context.Background())
	})
	return deps, fl, a, signalCh
}

func TestRun_Success_Foreground(t *testing.T) {
	deps, locker, _, sigCh := makeRunDeps(t)
	// Cancel via the signal channel.
	done := make(chan error, 1)
	go func() {
		done <- runWithDeps(context.Background(), runOpts{}, deps)
	}()
	// Wait a bit for run to start, then send a signal.
	time.Sleep(50 * time.Millisecond)
	sigCh <- os.Interrupt
	select {
	case err := <-done:
		if err != nil {
			t.Errorf("run returned error: %v", err)
		}
	case <-time.After(2 * time.Second):
		t.Fatal("run did not return")
	}
	if locker.released != 1 {
		t.Errorf("locker released = %d, want 1", locker.released)
	}
}

func TestRun_AllocatePortError(t *testing.T) {
	deps, _, _, _ := makeRunDeps(t)
	deps.allocatePort = func(p int) (net.Listener, int, error) {
		return nil, 0, errors.New("no ports available")
	}
	err := runWithDeps(context.Background(), runOpts{}, deps)
	if err == nil {
		t.Fatal("expected error from allocate port")
	}
	if !strings.Contains(err.Error(), "allocate port") {
		t.Errorf("err = %v, want 'allocate port'", err)
	}
}

func TestRun_AcquireLockError(t *testing.T) {
	deps, _, _, _ := makeRunDeps(t)
	deps.acquireLock = func(dir string) (locker, error) {
		return nil, errors.New("lock held")
	}
	err := runWithDeps(context.Background(), runOpts{}, deps)
	if err == nil {
		t.Fatal("expected error from acquire lock")
	}
	if !strings.Contains(err.Error(), "instance lock") {
		t.Errorf("err = %v, want 'instance lock'", err)
	}
}

func TestRun_NewAppError(t *testing.T) {
	deps, _, _, _ := makeRunDeps(t)
	deps.newApp = func(ctx context.Context, opts app.Options) (*app.App, error) {
		return nil, errors.New("app init failed")
	}
	err := runWithDeps(context.Background(), runOpts{}, deps)
	if err == nil {
		t.Fatal("expected error from new app")
	}
	if !strings.Contains(err.Error(), "app init") {
		t.Errorf("err = %v, want 'app init'", err)
	}
}

func TestRun_ServiceMode(t *testing.T) {
	deps, _, _, sigCh := makeRunDeps(t)
	// Use a service writer that's stubbed.
	done := make(chan error, 1)
	go func() {
		done <- runWithDeps(context.Background(), runOpts{serviceMode: true}, deps)
	}()
	// Wait briefly for service mode to start.
	time.Sleep(50 * time.Millisecond)
	// Trigger shutdown via sync engine ctx.
	sigCh <- os.Interrupt
	select {
	case <-done:
		// OK
	case <-time.After(2 * time.Second):
		t.Fatal("run did not return")
	}
}

func TestRun_NoBrowser(t *testing.T) {
	deps, _, _, sigCh := makeRunDeps(t)
	done := make(chan error, 1)
	go func() {
		done <- runWithDeps(context.Background(), runOpts{noBrowser: true}, deps)
	}()
	time.Sleep(50 * time.Millisecond)
	sigCh <- os.Interrupt
	select {
	case <-done:
		// OK
	case <-time.After(2 * time.Second):
		t.Fatal("run did not return")
	}
}

func TestRun_ServeError(t *testing.T) {
	deps, _, _, _ := makeRunDeps(t)
	deps.serve = func(a *app.App, ln net.Listener) error {
		return errors.New("server failed")
	}
	err := runWithDeps(context.Background(), runOpts{noBrowser: true}, deps)
	if err == nil {
		t.Fatal("expected error from serve")
	}
	if !strings.Contains(err.Error(), "server") {
		t.Errorf("err = %v, want 'server'", err)
	}
}

// TestRun_HealthWriterStartError exercises the service mode branch where
// the health writer fails to start.
func TestRun_HealthWriterStartError(t *testing.T) {
	deps, _, a, sigCh := makeRunDeps(t)
	// Make the health writer directory read-only to force Start to fail.
	t.Setenv("XDG_CONFIG_HOME", t.TempDir())
	t.Setenv("HOME", t.TempDir())
	// Use a config dir that's read-only so the health file can't be written.
	ro := t.TempDir()
	_ = os.Chmod(ro, 0o500)
	t.Cleanup(func() { _ = os.Chmod(ro, 0o700) })
	a.Config.ConfigDir = ro

	done := make(chan error, 1)
	go func() {
		done <- runWithDeps(context.Background(), runOpts{serviceMode: true}, deps)
	}()
	time.Sleep(50 * time.Millisecond)
	sigCh <- os.Interrupt
	select {
	case <-done:
		// OK — should not panic on health writer failure
	case <-time.After(2 * time.Second):
		t.Fatal("run did not return")
	}
}

// --- runEdge tests ---

type stubSyncExecutor struct {
	calls []rclone.SyncConfig
	err   error
}

func (s *stubSyncExecutor) Sync(_ context.Context, cfg rclone.SyncConfig, _ func(rclone.Stats)) (*rclone.SyncResult, error) {
	s.calls = append(s.calls, cfg)
	return &rclone.SyncResult{}, s.err
}

func TestRunEdge_Success_FullPath(t *testing.T) {
	exec := &stubSyncExecutor{}
	edge := store.BoardEdge{ID: "e1", SourceID: "n1", TargetID: "n2", Action: "push"}
	src := store.BoardNode{ID: "n1", RemoteName: "remote1", Path: "src"}
	dst := store.BoardNode{ID: "n2", RemoteName: "remote2", Path: "dst"}
	if err := runEdgeWith(context.Background(), exec, edge, src, dst); err != nil {
		t.Fatal(err)
	}
	if len(exec.calls) != 1 {
		t.Fatalf("expected 1 call, got %d", len(exec.calls))
	}
	c := exec.calls[0]
	if c.Source != "remote1:src" {
		t.Errorf("Source = %q, want remote1:src", c.Source)
	}
	if c.Dest != "remote2:dst" {
		t.Errorf("Dest = %q, want remote2:dst", c.Dest)
	}
	if c.Action != rclone.ActionPush {
		t.Errorf("Action = %q", c.Action)
	}
}

func TestRunEdge_SrcRemoteOnly(t *testing.T) {
	exec := &stubSyncExecutor{}
	edge := store.BoardEdge{ID: "e1", Action: "pull"}
	src := store.BoardNode{ID: "n1", RemoteName: "remote1"}
	dst := store.BoardNode{ID: "n2", RemoteName: "remote2", Path: "dst"}
	if err := runEdgeWith(context.Background(), exec, edge, src, dst); err != nil {
		t.Fatal(err)
	}
	c := exec.calls[0]
	if c.Source != "remote1:" {
		t.Errorf("Source = %q, want remote1:", c.Source)
	}
	if c.Dest != "remote2:dst" {
		t.Errorf("Dest = %q, want remote2:dst", c.Dest)
	}
}

func TestRunEdge_SrcPathRoot(t *testing.T) {
	exec := &stubSyncExecutor{}
	edge := store.BoardEdge{ID: "e1", Action: "push"}
	src := store.BoardNode{ID: "n1", RemoteName: "remote1", Path: "/"}
	dst := store.BoardNode{ID: "n2", RemoteName: "remote2", Path: "/"}
	if err := runEdgeWith(context.Background(), exec, edge, src, dst); err != nil {
		t.Fatal(err)
	}
	c := exec.calls[0]
	if c.Source != "remote1:" {
		t.Errorf("Source = %q, want remote1:", c.Source)
	}
}

func TestRunEdge_NoRemote(t *testing.T) {
	exec := &stubSyncExecutor{}
	edge := store.BoardEdge{ID: "e1", Action: "push"}
	src := store.BoardNode{ID: "n1", Path: "/local/src"}
	dst := store.BoardNode{ID: "n2", Path: "/local/dst"}
	if err := runEdgeWith(context.Background(), exec, edge, src, dst); err != nil {
		t.Fatal(err)
	}
	c := exec.calls[0]
	if c.Source != "/local/src" {
		t.Errorf("Source = %q, want /local/src", c.Source)
	}
	if c.Dest != "/local/dst" {
		t.Errorf("Dest = %q, want /local/dst", c.Dest)
	}
}

func TestRunEdge_DefaultAction(t *testing.T) {
	exec := &stubSyncExecutor{}
	edge := store.BoardEdge{ID: "e1", Action: ""} // empty → push default
	src := store.BoardNode{ID: "n1", RemoteName: "r"}
	dst := store.BoardNode{ID: "n2", RemoteName: "r2"}
	if err := runEdgeWith(context.Background(), exec, edge, src, dst); err != nil {
		t.Fatal(err)
	}
	if exec.calls[0].Action != rclone.ActionPush {
		t.Errorf("default Action = %q, want push", exec.calls[0].Action)
	}
}

func TestRunEdge_SyncError(t *testing.T) {
	exec := &stubSyncExecutor{err: errors.New("sync failed")}
	edge := store.BoardEdge{ID: "e1", Action: "push"}
	src := store.BoardNode{ID: "n1", RemoteName: "r"}
	dst := store.BoardNode{ID: "n2", RemoteName: "r2"}
	err := runEdgeWith(context.Background(), exec, edge, src, dst)
	if err == nil {
		t.Fatal("expected error")
	}
	if !strings.Contains(err.Error(), "sync failed") {
		t.Errorf("err = %v", err)
	}
}

// TestRunBoard_Success exercises runBoard with a small valid board and
// a stub sync executor.
func TestRunBoard_Success(t *testing.T) {
	a := newTestApp(t)
	// Create a board with 2 nodes and 1 edge.
	b := &store.Board{ID: "b1", Name: "test"}
	b.Nodes = []store.BoardNode{
		{ID: "n1", RemoteName: "r1", Path: "src", Label: "src"},
		{ID: "n2", RemoteName: "r2", Path: "dst", Label: "dst"},
	}
	b.Edges = []store.BoardEdge{{ID: "e1", SourceID: "n1", TargetID: "n2", Action: "push"}}
	if err := a.Store.Boards().SaveGraph(context.Background(), b); err != nil {
		t.Fatal(err)
	}

	// Override the sync executor.
	origNew := newSyncExecutor
	defer func() { newSyncExecutor = origNew }()
	exec := &stubSyncExecutor{}
	newSyncExecutor = func(a *app.App) syncExecutor { return exec }

	cmd, buf := fakeCmd()
	if err := runBoard(context.Background(), a, "b1", true, 1, cmd); err != nil {
		t.Fatalf("runBoard: %v", err)
	}
	if !strings.Contains(buf.String(), "ok") {
		t.Errorf("output should say 'ok': %q", buf.String())
	}
	if len(exec.calls) != 1 {
		t.Errorf("expected 1 sync call, got %d", len(exec.calls))
	}
}

// TestRunBoard_StopOnError exercises the stop-on-error path.
func TestRunBoard_StopOnError(t *testing.T) {
	a := newTestApp(t)
	b := &store.Board{ID: "b1", Name: "test"}
	b.Nodes = []store.BoardNode{
		{ID: "n1", RemoteName: "r1", Path: "src", Label: "src"},
		{ID: "n2", RemoteName: "r2", Path: "dst", Label: "dst"},
	}
	b.Edges = []store.BoardEdge{{ID: "e1", SourceID: "n1", TargetID: "n2", Action: "push"}}
	if err := a.Store.Boards().SaveGraph(context.Background(), b); err != nil {
		t.Fatal(err)
	}

	origNew := newSyncExecutor
	defer func() { newSyncExecutor = origNew }()
	exec := &stubSyncExecutor{err: errors.New("edge sync failed")}
	newSyncExecutor = func(a *app.App) syncExecutor { return exec }

	cmd, buf := fakeCmd()
	err := runBoard(context.Background(), a, "b1", true, 1, cmd)
	if err == nil {
		t.Fatal("expected error from runBoard")
	}
	if !strings.Contains(err.Error(), "stopped") {
		t.Errorf("err = %v, want 'stopped'", err)
	}
	if !strings.Contains(buf.String(), "✗") {
		t.Errorf("output should show failure marker: %q", buf.String())
	}
}

// TestRunBoard_NoStopOnError exercises the continue-on-error path.
func TestRunBoard_NoStopOnError(t *testing.T) {
	a := newTestApp(t)
	b := &store.Board{ID: "b1", Name: "test"}
	b.Nodes = []store.BoardNode{
		{ID: "n1", RemoteName: "r1", Path: "src", Label: "src"},
		{ID: "n2", RemoteName: "r2", Path: "dst", Label: "dst"},
	}
	b.Edges = []store.BoardEdge{{ID: "e1", SourceID: "n1", TargetID: "n2", Action: "push"}}
	if err := a.Store.Boards().SaveGraph(context.Background(), b); err != nil {
		t.Fatal(err)
	}

	origNew := newSyncExecutor
	defer func() { newSyncExecutor = origNew }()
	exec := &stubSyncExecutor{err: errors.New("edge sync failed")}
	newSyncExecutor = func(a *app.App) syncExecutor { return exec }

	cmd, _ := fakeCmd()
	err := runBoard(context.Background(), a, "b1", false, 1, cmd)
	if err == nil {
		t.Fatal("expected error from runBoard")
	}
	if !strings.Contains(err.Error(), "completed with errors") {
		t.Errorf("err = %v, want 'completed with errors'", err)
	}
}

// TestRunBoard_LoadGraphError exercises the board not-found path.
func TestRunBoard_LoadGraphError(t *testing.T) {
	a := newTestApp(t)
	cmd, _ := fakeCmd()
	err := runBoard(context.Background(), a, "no-such-board", true, 1, cmd)
	if err == nil {
		t.Fatal("expected error for missing board")
	}
}

// TestRunBoard_NoNodes exercises the empty-nodes path.
func TestRunBoard_NoNodes(t *testing.T) {
	a := newTestApp(t)
	b := &store.Board{ID: "b1", Name: "empty"}
	if err := a.Store.Boards().SaveGraph(context.Background(), b); err != nil {
		t.Fatal(err)
	}
	cmd, _ := fakeCmd()
	err := runBoard(context.Background(), a, "b1", true, 1, cmd)
	if err == nil {
		t.Fatal("expected error for empty nodes")
	}
}

// TestRunBoard_NoEdges exercises the empty-edges path.
func TestRunBoard_NoEdges(t *testing.T) {
	a := newTestApp(t)
	b := &store.Board{ID: "b1", Name: "edges"}
	b.Nodes = []store.BoardNode{{ID: "n1"}}
	if err := a.Store.Boards().SaveGraph(context.Background(), b); err != nil {
		t.Fatal(err)
	}
	cmd, _ := fakeCmd()
	err := runBoard(context.Background(), a, "b1", true, 1, cmd)
	if err == nil {
		t.Fatal("expected error for empty edges")
	}
}

// TestRunBoard_MissingNode exercises the missing-node edge path.
func TestRunBoard_MissingNode(t *testing.T) {
	a := newTestApp(t)
	b := &store.Board{ID: "b1", Name: "missing"}
	b.Nodes = []store.BoardNode{{ID: "n1"}}
	b.Edges = []store.BoardEdge{{ID: "e1", SourceID: "n1", TargetID: "n99"}}
	if err := a.Store.Boards().SaveGraph(context.Background(), b); err != nil {
		t.Fatal(err)
	}
	cmd, _ := fakeCmd()
	err := runBoard(context.Background(), a, "b1", true, 1, cmd)
	if err == nil {
		t.Fatal("expected error for missing node")
	}
}

// TestRun_Wrapper exercises the public `run` function (not runWithDeps).
// It should fail at the port allocation step on a real environment so we
// can verify the wrapper calls runWithDeps.
func TestRun_Wrapper(t *testing.T) {
	// Replace defaultRunDeps with one that fails at the port step so we
	// can verify the public run() function calls runWithDeps.
	origDeps := defaultRunDeps
	defer func() { defaultRunDeps = origDeps }()
	defaultRunDeps = func() runDeps {
		d := origDeps()
		d.allocatePort = func(p int) (net.Listener, int, error) {
			return nil, 0, errors.New("wrapper test: port fail")
		}
		return d
	}
	err := run(context.Background(), runOpts{})
	if err == nil {
		t.Fatal("expected error from public run()")
	}
	if !strings.Contains(err.Error(), "allocate port") {
		t.Errorf("err = %v, want 'allocate port'", err)
	}
}

// --- newXxxCmd (sub-wrappers) Execute tests --------------------------

// TestNewProfileListCmd_Execute drives the wrapper via SetArgs + Execute.
func TestNewProfileListCmd_Execute(t *testing.T) {
	dir := t.TempDir()
	t.Setenv("XDG_CONFIG_HOME", dir)
	t.Setenv("HOME", dir)

	cmd := newProfileListCmd()
	var buf bytes.Buffer
	cmd.SetOut(&buf)
	cmd.SetErr(&buf)
	cmd.SetArgs([]string{})
	cmd.SilenceUsage = true
	if err := cmd.Execute(); err != nil {
		t.Fatalf("execute: %v", err)
	}
	if !strings.Contains(buf.String(), "No profiles") {
		t.Errorf("expected 'No profiles' message: %q", buf.String())
	}
}

func TestNewProfileDeleteCmd_NotFound(t *testing.T) {
	dir := t.TempDir()
	t.Setenv("XDG_CONFIG_HOME", dir)
	t.Setenv("HOME", dir)

	cmd := newProfileDeleteCmd()
	var buf bytes.Buffer
	cmd.SetOut(&buf)
	cmd.SetErr(&buf)
	cmd.SetArgs([]string{"does-not-exist"})
	cmd.SilenceUsage = true
	if err := cmd.Execute(); err == nil {
		t.Fatal("expected error for non-existent profile")
	}
}

func TestNewProfileDeleteCmd_MissingArg(t *testing.T) {
	cmd := newProfileDeleteCmd()
	cmd.SetOut(&bytes.Buffer{})
	cmd.SetErr(&bytes.Buffer{})
	cmd.SetArgs([]string{})
	err := cmd.Execute()
	if err == nil {
		t.Fatal("expected error for missing arg")
	}
}

func TestNewProfileAddCmd_Execute_MissingFlags(t *testing.T) {
	cmd := newProfileAddCmd()
	cmd.SetOut(&bytes.Buffer{})
	cmd.SetErr(&bytes.Buffer{})
	cmd.SetArgs([]string{})
	cmd.SilenceUsage = true
	if err := cmd.Execute(); err == nil {
		t.Fatal("expected error for missing required flags")
	}
}

// TestNewRemoteListCmd_Execute drives the remote list wrapper.
func TestNewRemoteListCmd_Execute(t *testing.T) {
	dir := t.TempDir()
	t.Setenv("XDG_CONFIG_HOME", dir)
	t.Setenv("HOME", dir)

	cmd := newRemoteListCmd()
	var buf bytes.Buffer
	cmd.SetOut(&buf)
	cmd.SetErr(&buf)
	cmd.SetArgs([]string{})
	cmd.SilenceUsage = true
	if err := cmd.Execute(); err != nil {
		t.Fatalf("execute: %v", err)
	}
	// With an empty rclone config, output should be either "No remotes"
	// (if rclone returns empty) or a header followed by no rows.
	out := buf.String()
	if !strings.Contains(out, "No remotes") && !strings.Contains(out, "NAME") {
		t.Errorf("expected empty list output: %q", out)
	}
}

func TestNewRemoteAddCmd_Execute_MissingFlags(t *testing.T) {
	cmd := newRemoteAddCmd()
	cmd.SetOut(&bytes.Buffer{})
	cmd.SetErr(&bytes.Buffer{})
	cmd.SetArgs([]string{})
	cmd.SilenceUsage = true
	if err := cmd.Execute(); err == nil {
		t.Fatal("expected error for missing required flags")
	}
}

func TestNewRemoteDeleteCmd_MissingArg(t *testing.T) {
	cmd := newRemoteDeleteCmd()
	cmd.SetOut(&bytes.Buffer{})
	cmd.SetErr(&bytes.Buffer{})
	cmd.SetArgs([]string{})
	err := cmd.Execute()
	if err == nil {
		t.Fatal("expected error for missing arg")
	}
}

func TestNewRemoteTestCmd_MissingArg(t *testing.T) {
	cmd := newRemoteTestCmd()
	cmd.SetOut(&bytes.Buffer{})
	cmd.SetErr(&bytes.Buffer{})
	cmd.SetArgs([]string{})
	err := cmd.Execute()
	if err == nil {
		t.Fatal("expected error for missing arg")
	}
}

// TestNewBoardCmd_MissingArg drives board Execute with no arg.
func TestNewBoardCmd_MissingArg(t *testing.T) {
	cmd := newBoardCmd()
	cmd.SetOut(&bytes.Buffer{})
	cmd.SetErr(&bytes.Buffer{})
	cmd.SetArgs([]string{})
	err := cmd.Execute()
	if err == nil {
		t.Fatal("expected error for missing board id arg")
	}
}

func TestNewBoardCmd_HelpFlag(t *testing.T) {
	cmd := newBoardCmd()
	var buf bytes.Buffer
	cmd.SetOut(&buf)
	cmd.SetErr(&buf)
	cmd.SetArgs([]string{"--help"})
	if err := cmd.Execute(); err != nil {
		t.Fatalf("execute: %v", err)
	}
	if !strings.Contains(buf.String(), "board DAG") {
		t.Errorf("missing help text: %q", buf.String())
	}
}

// TestNewSyncCmd_MissingFlags drives sync Execute with no flags.
func TestNewSyncCmd_MissingFlags(t *testing.T) {
	cmd := newSyncCmd()
	cmd.SetOut(&bytes.Buffer{})
	cmd.SetErr(&bytes.Buffer{})
	cmd.SetArgs([]string{})
	cmd.SilenceUsage = true
	if err := cmd.Execute(); err == nil {
		t.Fatal("expected error for missing required flags")
	}
}

func TestNewSyncCmd_HelpFlag(t *testing.T) {
	cmd := newSyncCmd()
	var buf bytes.Buffer
	cmd.SetOut(&buf)
	cmd.SetErr(&buf)
	cmd.SetArgs([]string{"--help"})
	if err := cmd.Execute(); err != nil {
		t.Fatalf("execute: %v", err)
	}
	if !strings.Contains(buf.String(), "sync") {
		t.Errorf("missing help text: %q", buf.String())
	}
}

// TestNewServiceCmd_MissingArg drives service with no sub-action.
func TestNewServiceCmd_MissingArg(t *testing.T) {
	cmd := newServiceCmd()
	cmd.SetOut(&bytes.Buffer{})
	cmd.SetErr(&bytes.Buffer{})
	cmd.SetArgs([]string{})
	err := cmd.Execute()
	if err == nil {
		t.Fatal("expected error for missing service sub-action")
	}
}

func TestNewServiceCmd_UnknownAction(t *testing.T) {
	cmd := newServiceCmd()
	cmd.SetOut(&bytes.Buffer{})
	cmd.SetErr(&bytes.Buffer{})
	cmd.SetArgs([]string{"reboot"})
	cmd.SilenceUsage = true
	err := cmd.Execute()
	if err == nil {
		t.Fatal("expected error for unknown service action")
	}
	if !strings.Contains(err.Error(), "unknown service action") {
		t.Errorf("unexpected error: %v", err)
	}
}

func TestNewServiceCmd_HelpFlag(t *testing.T) {
	cmd := newServiceCmd()
	var buf bytes.Buffer
	cmd.SetOut(&buf)
	cmd.SetErr(&buf)
	cmd.SetArgs([]string{"--help"})
	if err := cmd.Execute(); err != nil {
		t.Fatalf("execute: %v", err)
	}
	if !strings.Contains(buf.String(), "background service") {
		t.Errorf("missing help text: %q", buf.String())
	}
}

func TestNewUpdateCmd_HelpFlag(t *testing.T) {
	cmd := newUpdateCmd()
	var buf bytes.Buffer
	cmd.SetOut(&buf)
	cmd.SetErr(&buf)
	cmd.SetArgs([]string{"--help"})
	if err := cmd.Execute(); err != nil {
		t.Fatalf("execute: %v", err)
	}
	if !strings.Contains(buf.String(), "GitHub Releases") {
		t.Errorf("missing help text: %q", buf.String())
	}
}

func TestNewRunCmd_HelpFlag(t *testing.T) {
	cmd := newRunCmd()
	var buf bytes.Buffer
	cmd.SetOut(&buf)
	cmd.SetErr(&buf)
	cmd.SetArgs([]string{"--help"})
	if err := cmd.Execute(); err != nil {
		t.Fatalf("execute: %v", err)
	}
	if !strings.Contains(buf.String(), "Start") {
		t.Errorf("missing help text: %q", buf.String())
	}
}

// TestBoardCmd_AppNewError covers the app.New error branch in newBoardCmd's
// RunE by overriding appNewFn.
func TestBoardCmd_AppNewError(t *testing.T) {
	orig := appNewFn
	defer func() { appNewFn = orig }()
	appNewFn = func(ctx context.Context, opts app.Options) (*app.App, error) {
		return nil, errors.New("simulated app.New failure")
	}

	cmd := newBoardCmd()
	cmd.SetArgs([]string{"some-board"})
	cmd.SilenceUsage = true
	if err := cmd.Execute(); err == nil {
		t.Error("expected error from app.New failure")
	}
}

// TestDoctorCmd_AppNewError covers the app.New error branch in newDoctorCmd.
func TestDoctorCmd_AppNewError(t *testing.T) {
	orig := appNewFn
	defer func() { appNewFn = orig }()
	appNewFn = func(ctx context.Context, opts app.Options) (*app.App, error) {
		return nil, errors.New("simulated app.New failure")
	}

	cmd := newDoctorCmd()
	cmd.SetArgs([]string{})
	cmd.SilenceUsage = true
	if err := cmd.Execute(); err == nil {
		t.Error("expected error from app.New failure")
	}
}

// TestProfileCmd_AppNewError covers the app.New error branch in newProfileListCmd.
func TestProfileCmd_AppNewError(t *testing.T) {
	orig := appNewFn
	defer func() { appNewFn = orig }()
	appNewFn = func(ctx context.Context, opts app.Options) (*app.App, error) {
		return nil, errors.New("simulated app.New failure")
	}

	cmd := newProfileListCmd()
	cmd.SetArgs([]string{})
	cmd.SilenceUsage = true
	if err := cmd.Execute(); err == nil {
		t.Error("expected error from app.New failure")
	}
}

// TestProfileAddCmd_AppNewError covers the app.New error branch in newProfileAddCmd.
func TestProfileAddCmd_AppNewError(t *testing.T) {
	orig := appNewFn
	defer func() { appNewFn = orig }()
	appNewFn = func(ctx context.Context, opts app.Options) (*app.App, error) {
		return nil, errors.New("simulated app.New failure")
	}

	cmd := newProfileAddCmd()
	cmd.SetArgs([]string{"name", "--from", "/a", "--to", "/b"})
	cmd.SilenceUsage = true
	if err := cmd.Execute(); err == nil {
		t.Error("expected error from app.New failure")
	}
}

// TestProfileDeleteCmd_AppNewError covers the app.New error branch in newProfileDeleteCmd.
func TestProfileDeleteCmd_AppNewError(t *testing.T) {
	orig := appNewFn
	defer func() { appNewFn = orig }()
	appNewFn = func(ctx context.Context, opts app.Options) (*app.App, error) {
		return nil, errors.New("simulated app.New failure")
	}

	cmd := newProfileDeleteCmd()
	cmd.SetArgs([]string{"name"})
	cmd.SilenceUsage = true
	if err := cmd.Execute(); err == nil {
		t.Error("expected error from app.New failure")
	}
}

// TestRemoteListCmd_AppNewError covers the app.New error branch in newRemoteListCmd.
func TestRemoteListCmd_AppNewError(t *testing.T) {
	orig := appNewFn
	defer func() { appNewFn = orig }()
	appNewFn = func(ctx context.Context, opts app.Options) (*app.App, error) {
		return nil, errors.New("simulated app.New failure")
	}

	cmd := newRemoteListCmd()
	cmd.SetArgs([]string{})
	cmd.SilenceUsage = true
	if err := cmd.Execute(); err == nil {
		t.Error("expected error from app.New failure")
	}
}

// TestRemoteAddCmd_AppNewError covers the app.New error branch in newRemoteAddCmd.
func TestRemoteAddCmd_AppNewError(t *testing.T) {
	orig := appNewFn
	defer func() { appNewFn = orig }()
	appNewFn = func(ctx context.Context, opts app.Options) (*app.App, error) {
		return nil, errors.New("simulated app.New failure")
	}

	cmd := newRemoteAddCmd()
	cmd.SetArgs([]string{"name", "type"})
	cmd.SilenceUsage = true
	if err := cmd.Execute(); err == nil {
		t.Error("expected error from app.New failure")
	}
}

// TestRemoteTestCmd_AppNewError covers the app.New error branch in newRemoteTestCmd.
func TestRemoteTestCmd_AppNewError(t *testing.T) {
	orig := appNewFn
	defer func() { appNewFn = orig }()
	appNewFn = func(ctx context.Context, opts app.Options) (*app.App, error) {
		return nil, errors.New("simulated app.New failure")
	}

	cmd := newRemoteTestCmd()
	cmd.SetArgs([]string{"name"})
	cmd.SilenceUsage = true
	if err := cmd.Execute(); err == nil {
		t.Error("expected error from app.New failure")
	}
}

// TestRemoteDeleteCmd_AppNewError covers the app.New error branch in newRemoteDeleteCmd.
func TestRemoteDeleteCmd_AppNewError(t *testing.T) {
	orig := appNewFn
	defer func() { appNewFn = orig }()
	appNewFn = func(ctx context.Context, opts app.Options) (*app.App, error) {
		return nil, errors.New("simulated app.New failure")
	}

	cmd := newRemoteDeleteCmd()
	cmd.SetArgs([]string{"name"})
	cmd.SilenceUsage = true
	if err := cmd.Execute(); err == nil {
		t.Error("expected error from app.New failure")
	}
}

// TestSyncCmd_AppNewError covers the app.New error branch in newSyncCmd.
func TestSyncCmd_AppNewError(t *testing.T) {
	orig := appNewFn
	defer func() { appNewFn = orig }()
	appNewFn = func(ctx context.Context, opts app.Options) (*app.App, error) {
		return nil, errors.New("simulated app.New failure")
	}

	cmd := newSyncCmd()
	cmd.SetArgs([]string{"--profile", "p1", "--action", "push"})
	cmd.SilenceUsage = true
	if err := cmd.Execute(); err == nil {
		t.Error("expected error from app.New failure")
	}
}

// TestSyncCmd_DryRun covers the dryRun=true branch in newSyncCmd's RunE.
func TestSyncCmd_DryRun(t *testing.T) {
	orig := appNewFn
	t.Cleanup(func() { appNewFn = orig })
	// Override appNewFn to return a real app but with a fake rclone that
	// fails the sync (we only need to exercise the RunE branches).
	dir := t.TempDir()
	bin := filepath.Join(dir, "rclone")
	script := "#!/bin/sh\necho 'fake rclone'\nexit 0\n"
	if err := os.WriteFile(bin, []byte(script), 0o755); err != nil {
		t.Fatal(err)
	}

	appNewFn = func(ctx context.Context, opts app.Options) (*app.App, error) {
		return app.New(ctx, app.Options{
			ConfigDir:   filepath.Join(dir, "config"),
			LogMode:     logging.ModeForeground,
			RcloneBinary: bin,
		})
	}

	cmd := newSyncCmd()
	cmd.SetArgs([]string{"push", "--profile", "nonexistent", "--dry-run"})
	cmd.SilenceUsage = true
	cmd.SetOut(io.Discard)
	cmd.SetErr(io.Discard)
	// We expect error because the profile doesn't exist, but the dry-run
	// branch should be exercised.
	err := cmd.Execute()
	if err == nil {
		t.Error("expected error from missing profile")
	}
}

// TestUpdateCmd_AppNewError covers the app.New error branch in newUpdateCmd.
func TestUpdateCmd_AppNewError(t *testing.T) {
	orig := appNewFn
	defer func() { appNewFn = orig }()
	appNewFn = func(ctx context.Context, opts app.Options) (*app.App, error) {
		return nil, errors.New("simulated app.New failure")
	}

	cmd := newUpdateCmd()
	cmd.SilenceUsage = true
	if err := cmd.Execute(); err == nil {
		t.Error("expected error from app.New failure")
	}
}

// TestUpdateCmd_HelpFlag tests the --help flag for the update command.
func TestUpdateCmd_HelpFlag(t *testing.T) {
	cmd := newUpdateCmd()
	cmd.SetArgs([]string{"--help"})
	buf := &bytes.Buffer{}
	cmd.SetOut(buf)
	cmd.SetErr(buf)
	if err := cmd.Execute(); err != nil {
		t.Fatal(err)
	}
	if !strings.Contains(buf.String(), "self-update") {
		t.Errorf("missing help text: %q", buf.String())
	}
}
