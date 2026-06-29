package syncengine

import (
    "context"
    "errors"
    "testing"
    "time"

    "github.com/gnasdev/gn-drive/internal/store"
)

// waitTaskDone polls until the engine has no active tasks (the sync goroutine
// removes itself from the active set when runSync returns).
func waitTaskDone(t *testing.T, eng *Engine) {
    t.Helper()
    deadline := time.Now().Add(5 * time.Second)
    for time.Now().Before(deadline) {
        tasks, _ := eng.ActiveTasks(context.Background())
        if len(tasks) == 0 {
            return
        }
        time.Sleep(10 * time.Millisecond)
    }
    t.Fatal("task did not finish within timeout")
}

// TestRunSync_PersistsCompletedHistory verifies that a successful sync writes
// a completed row to the history table (previously runSync only published
// events and never persisted history).
func TestRunSync_PersistsCompletedHistory(t *testing.T) {
    eng, _, _, st := newFullEngine(t)
    if err := eng.Start(context.Background()); err != nil {
        t.Fatal(err)
    }
    defer eng.Stop(context.Background())

    ctx := context.Background()
    if err := st.Profiles().Save(ctx, &store.Profile{Name: "p1", From: "/src", To: "/dst"}); err != nil {
        t.Fatal(err)
    }

    taskID, err := eng.StartSync(ctx, "push", "p1")
    if err != nil {
        t.Fatalf("StartSync: %v", err)
    }
    waitTaskDone(t, eng)

    entries, err := st.History().List(ctx, 10, 0)
    if err != nil {
        t.Fatal(err)
    }
    if len(entries) != 1 {
        t.Fatalf("want 1 history entry, got %d", len(entries))
    }
    e := entries[0]
    if e.ID != taskID {
        t.Errorf("history ID = %q, want %q", e.ID, taskID)
    }
    if e.State != "completed" {
        t.Errorf("history State = %q, want completed", e.State)
    }
    if e.ProfileName != "p1" || e.Action != "push" {
        t.Errorf("history profile/action = %q/%q, want p1/push", e.ProfileName, e.Action)
    }
    if e.Bytes != 100 { // stubClient default result Stats.Bytes = 100
        t.Errorf("history Bytes = %d, want 100", e.Bytes)
    }
    if e.StartedAt == "" || e.FinishedAt == "" {
        t.Errorf("history timestamps empty: start=%q finish=%q", e.StartedAt, e.FinishedAt)
    }
}

// TestRunSync_PersistsFailedHistoryWithError verifies a failed sync writes a
// failed row including the error message (the error_message column was
// previously hardcoded to empty in History.Save).
func TestRunSync_PersistsFailedHistoryWithError(t *testing.T) {
    eng, stub, _, st := newFullEngine(t)
    stub.res = nil
    stub.err = errors.New("boom: rclone exploded")
    stub.progressN = 0

    if err := eng.Start(context.Background()); err != nil {
        t.Fatal(err)
    }
    defer eng.Stop(context.Background())

    ctx := context.Background()
    if err := st.Profiles().Save(ctx, &store.Profile{Name: "p2", From: "/src", To: "/dst"}); err != nil {
        t.Fatal(err)
    }
    if _, err := eng.StartSync(ctx, "pull", "p2"); err != nil {
        t.Fatalf("StartSync: %v", err)
    }
    waitTaskDone(t, eng)

    entries, err := st.History().List(ctx, 10, 0)
    if err != nil {
        t.Fatal(err)
    }
    if len(entries) != 1 {
        t.Fatalf("want 1 history entry, got %d", len(entries))
    }
    e := entries[0]
    if e.State != "failed" {
        t.Errorf("history State = %q, want failed", e.State)
    }
    if e.ErrorMessage == "" {
        t.Error("expected non-empty error_message for failed run")
    }
}
