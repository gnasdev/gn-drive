package syncengine

import (
    "context"
    "errors"
    "strings"
    "sync"
    "testing"
    "time"

    "github.com/gnasdev/gn-drive/internal/eventbus"
    "github.com/gnasdev/gn-drive/internal/store"
)

// TestRunSync_FailedEventCarriesErrorMessage verifies the sync:failed event now
// includes the failure reason so the UI can surface it in real time.
func TestRunSync_FailedEventCarriesErrorMessage(t *testing.T) {
    eng, stub, bus, st := newFullEngine(t)
    stub.res = nil
    stub.err = errors.New("boom: rclone exploded")
    stub.progressN = 0

    var (
        mu        sync.Mutex
        gotFailed bool
        gotMsg    string
    )
    bus.Subscribe(eventbus.TopicSyncFailed, func(ev eventbus.Event) {
        if pe, ok := ev.(eventbus.SyncProgressEvent); ok {
            mu.Lock()
            gotFailed = true
            gotMsg = pe.ErrorMessage
            mu.Unlock()
        }
    })

    if err := eng.Start(context.Background()); err != nil {
        t.Fatal(err)
    }
    defer eng.Stop(context.Background())

    ctx := context.Background()
    if err := st.Profiles().Save(ctx, &store.Profile{Name: "pf", From: "/s", To: "/d"}); err != nil {
        t.Fatal(err)
    }
    if _, err := eng.StartSync(ctx, "push", "pf"); err != nil {
        t.Fatal(err)
    }
    waitTaskDone(t, eng)

    deadline := time.Now().Add(2 * time.Second)
    for time.Now().Before(deadline) {
        mu.Lock()
        done := gotFailed
        mu.Unlock()
        if done {
            break
        }
        time.Sleep(10 * time.Millisecond)
    }

    mu.Lock()
    defer mu.Unlock()
    if !gotFailed {
        t.Fatal("no sync:failed event received")
    }
    if !strings.Contains(gotMsg, "boom") {
        t.Errorf("failed event ErrorMessage = %q, want it to contain the rclone error", gotMsg)
    }
}
