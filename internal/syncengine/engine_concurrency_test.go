package syncengine

import (
    "context"
    "errors"
    "testing"
    "time"

    "github.com/gnasdev/gn-drive/internal/eventbus"
    "github.com/gnasdev/gn-drive/internal/rclone"
    "github.com/gnasdev/gn-drive/internal/store"
)

// gateClient blocks every Sync call until release is closed, signalling each
// start on the started channel. Unlike blockingClient it supports concurrent
// calls (used to exercise the per-profile concurrency guard).
type gateClient struct {
    started chan string
    release chan struct{}
}

func (g *gateClient) Sync(_ context.Context, cfg rclone.SyncConfig, _ func(rclone.Stats)) (*rclone.SyncResult, error) {
    g.started <- cfg.Source
    <-g.release
    return &rclone.SyncResult{}, nil
}

func TestStartSync_RejectsConcurrentSameProfile(t *testing.T) {
    bus := eventbus.NewBus(context.Background())
    gate := &gateClient{started: make(chan string, 8), release: make(chan struct{})}
    dir := t.TempDir()
    st, err := store.New(context.Background(), dir+"/db.db", noopLogger())
    if err != nil {
        t.Fatal(err)
    }
    t.Cleanup(func() { _ = st.Close() })

    eng := New(Deps{Logger: noopLogger(), Bus: bus, Store: st, Rclone: gate})
    if err := eng.Start(context.Background()); err != nil {
        t.Fatal(err)
    }
    defer eng.Stop(context.Background())

    ctx := context.Background()
    if err := st.Profiles().Save(ctx, &store.Profile{Name: "p1", From: "p1src", To: "dst"}); err != nil {
        t.Fatal(err)
    }
    if err := st.Profiles().Save(ctx, &store.Profile{Name: "p2", From: "p2src", To: "dst"}); err != nil {
        t.Fatal(err)
    }

    if _, err := eng.StartSync(ctx, "push", "p1"); err != nil {
        t.Fatalf("first StartSync(p1): %v", err)
    }
    <-gate.started // p1 sync now in flight and blocked

    // A second sync for the SAME profile must be rejected.
    if _, err := eng.StartSync(ctx, "push", "p1"); !errors.Is(err, ErrProfileBusy) {
        t.Fatalf("concurrent same-profile: got %v, want ErrProfileBusy", err)
    }

    // A DIFFERENT profile must be allowed to start.
    if _, err := eng.StartSync(ctx, "push", "p2"); err != nil {
        t.Fatalf("different profile should start: %v", err)
    }
    <-gate.started // p2 started

    // Release the blocked syncs and wait for them to drain.
    close(gate.release)
    deadline := time.Now().Add(3 * time.Second)
    for time.Now().Before(deadline) {
        tasks, _ := eng.ActiveTasks(ctx)
        if len(tasks) == 0 {
            break
        }
        time.Sleep(10 * time.Millisecond)
    }

    // After completion, the same profile can run again (guard was released).
    if _, err := eng.StartSync(ctx, "push", "p1"); err != nil {
        t.Fatalf("after completion, p1 should start again: %v", err)
    }
    <-gate.started
}
