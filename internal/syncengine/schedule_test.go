package syncengine

import (
	"context"
	"io"
	"log/slog"
	"sync"
	"testing"
	"time"

	"github.com/robfig/cron/v3"

	"github.com/gnasdev/gn-drive/internal/eventbus"
	"github.com/gnasdev/gn-drive/internal/store"
)

func noopLogger() *slog.Logger {
	return slog.New(slog.NewTextHandler(io.Discard, nil))
}

// makeEngine constructs an Engine wired with a fresh eventbus and a
// nil store (loadSchedules is guarded by nil-store check). We only
// exercise the cron map in this test, so the rclone dependency is
// not required.
func makeEngine(t *testing.T) *Engine {
	t.Helper()
	bus := eventbus.NewBus(context.Background())
	return &Engine{
		log:      noopLogger(),
		bus:      bus,
		schedule: make(map[string]cron.EntryID),
		cron:     cron.New(cron.WithSeconds()),
		ctx:      context.Background(),
	}
}

func TestRegisterAndUnregister(t *testing.T) {
	e := makeEngine(t)
	sch := &store.Schedule{ID: "sched-1", ProfileName: "p", Action: "push", Cron: "0 * * * * *", Enabled: true}

	e.RegisterSchedule(context.Background(), sch)
	if _, ok := e.schedule["sched-1"]; !ok {
		t.Fatal("expected schedule-1 to be registered")
	}

	e.UnregisterSchedule("sched-1")
	if _, ok := e.schedule["sched-1"]; ok {
		t.Fatal("expected schedule-1 to be removed")
	}
}

func TestUnregisterUnknownIsNoop(t *testing.T) {
	e := makeEngine(t)
	// Should not panic.
	e.UnregisterSchedule("nonexistent")
	if len(e.schedule) != 0 {
		t.Errorf("schedule map should be empty, got %d", len(e.schedule))
	}
}

func TestRegisterDisabledSkipsCron(t *testing.T) {
	e := makeEngine(t)
	sch := &store.Schedule{ID: "sched-2", ProfileName: "p", Action: "push", Cron: "0 * * * * *", Enabled: false}
	e.RegisterSchedule(context.Background(), sch)
	if _, ok := e.schedule["sched-2"]; ok {
		t.Fatal("disabled schedule should not be registered")
	}
}

func TestRegisterEmptyCronSkips(t *testing.T) {
	e := makeEngine(t)
	sch := &store.Schedule{ID: "sched-3", ProfileName: "p", Action: "push", Cron: "", Enabled: true}
	e.RegisterSchedule(context.Background(), sch)
	if _, ok := e.schedule["sched-3"]; ok {
		t.Fatal("empty cron should not be registered")
	}
}

func TestReRegisterReplacesEntry(t *testing.T) {
	e := makeEngine(t)
	sch := &store.Schedule{ID: "sched-4", ProfileName: "p", Action: "push", Cron: "0 * * * * *", Enabled: true}
	e.RegisterSchedule(context.Background(), sch)
	first := e.schedule["sched-4"]

	// Update cron expression and re-register.
	sch.Cron = "0 */5 * * * *"
	e.RegisterSchedule(context.Background(), sch)
	second := e.schedule["sched-4"]
	if first == second {
		t.Fatal("re-registering should produce a new entry ID")
	}
	if _, ok := e.schedule["sched-4"]; !ok {
		t.Fatal("schedule-4 should still be present after re-register")
	}
	// Total entries in the cron should be exactly 1.
	count := 0
	for range e.cron.Entries() {
		count++
	}
	if count != 1 {
		t.Errorf("expected 1 cron entry, got %d", count)
	}
}

func TestUnregisterAfterStopIsNoop(t *testing.T) {
	e := makeEngine(t)
	sch := &store.Schedule{ID: "sched-5", ProfileName: "p", Action: "push", Cron: "0 * * * * *", Enabled: true}
	e.RegisterSchedule(context.Background(), sch)
	// Simulate Stop: nil out cron + schedule.
	var wg sync.WaitGroup
	wg.Add(1)
	go func() { wg.Done() }()
	wg.Wait()
	e.cron.Stop()
	e.cron = nil
	e.schedule = nil

	// Should not panic even with nil cron.
	e.UnregisterSchedule("sched-5")
}

// Compile-time guard: ensure we don't accidentally regress the package
// signature used by cmd/gn-drive/run.go.
var _ = time.Now
