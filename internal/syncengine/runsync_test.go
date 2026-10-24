package syncengine

import (
	"context"
	"io"
	"log/slog"
	"os"
	"os/exec"
	"path/filepath"
	"sync/atomic"
	"testing"
	"time"

	"github.com/gnasdev/gn-drive/internal/eventbus"
	"github.com/gnasdev/gn-drive/internal/rclone"
	"github.com/gnasdev/gn-drive/internal/store"
)

// requireRealRclone gates tests that shell out to the host rclone. They only
// use local paths (never OAuth), but stay opt-in so the suite never runs the
// user's binary or touches their network.
func requireRealRclone(t *testing.T) {
	t.Helper()
	if os.Getenv("GN_DRIVE_REAL_RCLONE_TESTS") != "1" {
		t.Skip("set GN_DRIVE_REAL_RCLONE_TESTS=1 to run against the real rclone")
	}
	if _, err := exec.LookPath("rclone"); err != nil {
		t.Skipf("rclone not found: %v", err)
	}
}

// TestRunSync_RealRclone exercises the full runSync path using the system
// rclone binary. It copies a single file to a local "remote" (a path with
// a colon, treated by rclone as a local filesystem path).
func TestRunSync_RealRclone(t *testing.T) {
	requireRealRclone(t)

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

	bus := eventbus.NewBus(context.Background())
	rc, err := rclone.New(rclone.Options{
		BinaryPath: "rclone",
		ConfigPath: filepath.Join(dir, "rclone.conf"),
		Logger:     slog.New(slog.NewTextHandler(io.Discard, nil)),
	})
	if err != nil {
		t.Fatal(err)
	}
	st, err := store.New(context.Background(), filepath.Join(dir, "db.db"), slog.New(slog.NewTextHandler(io.Discard, nil)))
	if err != nil {
		t.Fatal(err)
	}
	defer st.Close()

	// Subscribe to events to verify they're emitted.
	var (
		gotStarted  atomic.Bool
		gotComplete atomic.Bool
	)
	bus.Subscribe(eventbus.TopicSyncStarted, func(ev eventbus.Event) {
		gotStarted.Store(true)
	})
	bus.Subscribe(eventbus.TopicSyncCompleted, func(ev eventbus.Event) {
		gotComplete.Store(true)
	})

	eng := New(Deps{Logger: slog.New(slog.NewTextHandler(io.Discard, nil)), Bus: bus, Store: st, Rclone: rc})
	if err := eng.Start(context.Background()); err != nil {
		t.Fatal(err)
	}
	defer eng.Stop(context.Background())

	ctx := context.Background()
	if err := st.Profiles().Save(ctx, &store.Profile{
		Name: "p1", From: srcDir, To: dstDir,
	}); err != nil {
		t.Fatal(err)
	}

	taskID, err := eng.StartSync(ctx, "push", "p1")
	if err != nil {
		t.Fatalf("StartSync: %v", err)
	}
	if taskID == "" {
		t.Fatal("empty taskID")
	}

	// Wait for completion (or timeout).
	deadline := time.Now().Add(15 * time.Second)
	for time.Now().Before(deadline) {
		tasks, _ := eng.ActiveTasks(ctx)
		if len(tasks) == 0 {
			break
		}
		time.Sleep(100 * time.Millisecond)
	}

	if !gotStarted.Load() {
		t.Error("sync:started event not emitted")
	}
	if !gotComplete.Load() {
		t.Error("sync:completed event not emitted")
	}
	// File should now exist in dstDir.
	if _, err := os.Stat(filepath.Join(dstDir, "hello.txt")); err != nil {
		t.Errorf("destination file missing: %v", err)
	}
}

// TestStartSync_PropagatesError covers the error branch of runSync
// by pointing the rclone config to a path that will cause failures.
func TestStartSync_PropagatesError(t *testing.T) {
	requireRealRclone(t)

	dir := t.TempDir()
	bus := eventbus.NewBus(context.Background())
	rc, err := rclone.New(rclone.Options{
		BinaryPath: "rclone",
		ConfigPath: filepath.Join(dir, "rclone.conf"),
		Logger:     slog.New(slog.NewTextHandler(io.Discard, nil)),
	})
	if err != nil {
		t.Fatal(err)
	}
	st, _ := store.New(context.Background(), filepath.Join(dir, "db.db"), slog.New(slog.NewTextHandler(io.Discard, nil)))
	defer st.Close()

	var gotFailed atomic.Bool
	bus.Subscribe(eventbus.TopicSyncFailed, func(ev eventbus.Event) {
		gotFailed.Store(true)
	})

	eng := New(Deps{Logger: slog.New(slog.NewTextHandler(io.Discard, nil)), Bus: bus, Store: st, Rclone: rc})
	if err := eng.Start(context.Background()); err != nil {
		t.Fatal(err)
	}
	defer eng.Stop(context.Background())

	ctx := context.Background()
	// Profile with non-existent source path → rclone fails.
	if err := st.Profiles().Save(ctx, &store.Profile{
		Name: "bad", From: "/nonexistent/path/that/does/not/exist", To: filepath.Join(dir, "dst"),
	}); err != nil {
		t.Fatal(err)
	}

	_, err = eng.StartSync(ctx, "push", "bad")
	if err != nil {
		t.Fatalf("StartSync: %v", err)
	}

	// Wait for failed event.
	deadline := time.Now().Add(10 * time.Second)
	for time.Now().Before(deadline) {
		if gotFailed.Load() {
			break
		}
		time.Sleep(100 * time.Millisecond)
	}
	if !gotFailed.Load() {
		t.Error("sync:failed event not emitted")
	}
}
