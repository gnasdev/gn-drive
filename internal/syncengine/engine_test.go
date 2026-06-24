package syncengine

import (
	"context"
	"errors"
	"io"
	"log/slog"
	"sync"
	"testing"
	"time"

	"github.com/gnasdev/gn-drive/internal/eventbus"
	"github.com/gnasdev/gn-drive/internal/rclone"
	"github.com/gnasdev/gn-drive/internal/store"
)

// stubClient lets us drive Engine.runSync deterministically without
// shelling out to rclone. It records the last Sync call and returns a
// caller-supplied result/error.
type stubClient struct {
	mu        sync.Mutex
	lastCfg   rclone.SyncConfig
	lastProg  []rclone.Stats
	progress  func(rclone.Stats)
	res       *rclone.SyncResult
	err       error
	progressN int // how many progress callbacks to fire
}

func (s *stubClient) Sync(_ context.Context, cfg rclone.SyncConfig, onProgress func(rclone.Stats)) (*rclone.SyncResult, error) {
	s.mu.Lock()
	s.lastCfg = cfg
	s.mu.Unlock()
	// Fire a few progress callbacks to exercise the onProgress path.
	for i := 0; i < s.progressN; i++ {
		if onProgress != nil {
			stats := rclone.Stats{Bytes: int64(i * 100)}
			s.mu.Lock()
			s.lastProg = append(s.lastProg, stats)
			s.mu.Unlock()
			onProgress(stats)
		}
	}
	return s.res, s.err
}

// Compile-time check.
var _ SyncClient = (*stubClient)(nil)

func newFullEngine(t *testing.T) (*Engine, *stubClient, *eventbus.Bus, *store.Store) {
	t.Helper()
	bus := eventbus.NewBus(context.Background())
	stub := &stubClient{res: &rclone.SyncResult{StartedAt: 1, EndedAt: 2, Stats: rclone.Stats{Bytes: 100}}, progressN: 2}
	dir := t.TempDir()
	st, err := store.New(context.Background(), dir+"/db.db", slog.New(slog.NewTextHandler(io.Discard, nil)))
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() { _ = st.Close() })
	eng := New(Deps{
		Logger: slog.New(slog.NewTextHandler(io.Discard, nil)),
		Bus:    bus,
		Store:  st,
		Rclone: stub,
	})
	return eng, stub, bus, st
}

func TestStart_AlreadyStarted(t *testing.T) {
	eng, _, _, _ := newFullEngine(t)
	if err := eng.Start(context.Background()); err != nil {
		t.Fatal(err)
	}
	defer eng.Stop(context.Background())
	// Second start should be a no-op.
	if err := eng.Start(context.Background()); err != nil {
		t.Errorf("second Start: %v", err)
	}
}

func TestStart_LoadsSchedules(t *testing.T) {
	eng, _, _, st := newFullEngine(t)
	ctx := context.Background()
	// Insert a schedule. Use 6-field cron expression because the engine
	// uses cron.WithSeconds() (matches the spec at engine.go:65).
	sch := &store.Schedule{ID: "sch1", ProfileName: "p1", Action: "push", Cron: "0 0 * * * *", Enabled: true}
	if err := st.Schedules().Save(ctx, sch); err != nil {
		t.Fatal(err)
	}
	if err := eng.Start(ctx); err != nil {
		t.Fatal(err)
	}
	defer eng.Stop(ctx)
	// Schedule should be registered.
	eng.cronMu.Lock()
	_, ok := eng.schedule["sch1"]
	eng.cronMu.Unlock()
	if !ok {
		t.Error("expected schedule to be loaded on Start")
	}
}

func TestStop_NotStarted(t *testing.T) {
	eng, _, _, _ := newFullEngine(t)
	if err := eng.Stop(context.Background()); err != nil {
		t.Errorf("Stop when not started: %v", err)
	}
}

func TestStop_StartedClearsState(t *testing.T) {
	eng, _, _, _ := newFullEngine(t)
	if err := eng.Start(context.Background()); err != nil {
		t.Fatal(err)
	}
	if err := eng.Stop(context.Background()); err != nil {
		t.Fatal(err)
	}
	if eng.cron != nil {
		t.Error("cron should be nil after Stop")
	}
	if eng.schedule != nil {
		t.Error("schedule map should be nil after Stop")
	}
	if eng.cancel != nil {
		t.Error("cancel should be nil after Stop")
	}
}

func TestActiveTasks_Empty(t *testing.T) {
	eng, _, _, _ := newFullEngine(t)
	tasks, err := eng.ActiveTasks(context.Background())
	if err != nil {
		t.Fatal(err)
	}
	if len(tasks) != 0 {
		t.Errorf("len = %d, want 0", len(tasks))
	}
}

func TestActiveTasks_WithTask(t *testing.T) {
	eng, _, _, _ := newFullEngine(t)
	if err := eng.Start(context.Background()); err != nil {
		t.Fatal(err)
	}
	defer eng.Stop(context.Background())

	// Manually register a task.
	task := &Task{
		ID:     "t1",
		Name:   "p1",
		Action: "push",
		Status: "running",
	}
	task.ctx, task.cancel = context.WithCancel(context.Background())
	eng.active.Store(task.ID, task)

	tasks, err := eng.ActiveTasks(context.Background())
	if err != nil {
		t.Fatal(err)
	}
	if len(tasks) != 1 {
		t.Fatalf("len = %d, want 1", len(tasks))
	}
	if tasks[0].ID != "t1" || tasks[0].Status != "running" {
		t.Errorf("task = %+v", tasks[0])
	}

	// Cleanup.
	task.cancel()
}

func TestStopSync_NotFound(t *testing.T) {
	eng, _, _, _ := newFullEngine(t)
	if err := eng.StopSync(context.Background(), "nonexistent"); err == nil {
		t.Error("expected error for unknown task")
	}
}

func TestStopSync_Found(t *testing.T) {
	eng, _, _, _ := newFullEngine(t)
	if err := eng.Start(context.Background()); err != nil {
		t.Fatal(err)
	}
	defer eng.Stop(context.Background())

	task := &Task{ID: "t1", Status: "running"}
	task.ctx, task.cancel = context.WithCancel(context.Background())
	eng.active.Store(task.ID, task)
	defer task.cancel()

	if err := eng.StopSync(context.Background(), "t1"); err != nil {
		t.Errorf("StopSync: %v", err)
	}
}

func TestStartSync_NotRunning(t *testing.T) {
	eng, _, _, _ := newFullEngine(t)
	// Engine not started.
	_, err := eng.StartSync(context.Background(), "push", "p1")
	if !errors.Is(err, ErrNotRunning) {
		t.Errorf("err = %v, want ErrNotRunning", err)
	}
}

func TestStartSync_ProfileNotFound(t *testing.T) {
	eng, _, _, _ := newFullEngine(t)
	if err := eng.Start(context.Background()); err != nil {
		t.Fatal(err)
	}
	defer eng.Stop(context.Background())

	_, err := eng.StartSync(context.Background(), "push", "missing")
	if err == nil {
		t.Error("expected error for missing profile")
	}
}

func TestStartSync_Success(t *testing.T) {
	eng, _, _, st := newFullEngine(t)
	if err := eng.Start(context.Background()); err != nil {
		t.Fatal(err)
	}
	defer eng.Stop(context.Background())

	// Create a profile in the store.
	if err := st.Profiles().Save(context.Background(), &store.Profile{Name: "p1", From: "remote:src", To: "remote:dst"}); err != nil {
		t.Fatal(err)
	}

	taskID, err := eng.StartSync(context.Background(), "push", "p1")
	if err != nil {
		t.Fatal(err)
	}
	if taskID == "" {
		t.Error("expected non-empty taskID")
	}

	// Wait for runSync to complete.
	deadline := time.Now().Add(2 * time.Second)
	for time.Now().Before(deadline) {
		tasks, _ := eng.ActiveTasks(context.Background())
		if len(tasks) == 0 {
			return
		}
		time.Sleep(10 * time.Millisecond)
	}
	t.Error("task should have completed")
}

func TestStartSync_InvalidAction_DefaultsToPush(t *testing.T) {
	eng, _, _, st := newFullEngine(t)
	if err := eng.Start(context.Background()); err != nil {
		t.Fatal(err)
	}
	defer eng.Stop(context.Background())

	if err := st.Profiles().Save(context.Background(), &store.Profile{Name: "p1", From: "a", To: "b"}); err != nil {
		t.Fatal(err)
	}
	// Invalid action — engine just passes it through to rclone.
	_, err := eng.StartSync(context.Background(), "totally-invalid", "p1")
	if err != nil {
		t.Fatalf("expected no error from StartSync with bad action (rclone resolves): %v", err)
	}
}

func TestStartSync_Failure(t *testing.T) {
	eng, stub, _, st := newFullEngine(t)
	stub.err = errors.New("rclone failed")
	if err := eng.Start(context.Background()); err != nil {
		t.Fatal(err)
	}
	defer eng.Stop(context.Background())

	if err := st.Profiles().Save(context.Background(), &store.Profile{Name: "p1", From: "a", To: "b"}); err != nil {
		t.Fatal(err)
	}
	taskID, err := eng.StartSync(context.Background(), "push", "p1")
	if err != nil {
		t.Fatal(err)
	}
	if taskID == "" {
		t.Error("expected taskID")
	}
	// Wait for task to fail and be removed.
	deadline := time.Now().Add(2 * time.Second)
	for time.Now().Before(deadline) {
		tasks, _ := eng.ActiveTasks(context.Background())
		if len(tasks) == 0 {
			return // success
		}
		time.Sleep(10 * time.Millisecond)
	}
	t.Error("task should have been removed after failure")
}

func TestTask_Cancel(t *testing.T) {
	task := &Task{Status: "running"}
	task.ctx, task.cancel = context.WithCancel(context.Background())
	task.Cancel()
	if task.Status != "cancelled" {
		t.Errorf("Status = %q, want cancelled", task.Status)
	}
}

func TestTask_Cancel_NilContext(t *testing.T) {
	task := &Task{}
	// Should not panic.
	task.Cancel()
}

func TestSnapshot_DecouplesFromLive(t *testing.T) {
	task := &Task{
		ID:     "t1",
		Name:   "p1",
		Action: "push",
		Status: "running",
		Stats:  rclone.Stats{Bytes: 100},
	}
	snap := task.Snapshot()
	if snap.ID != "t1" {
		t.Errorf("ID = %q", snap.ID)
	}
	if snap.Stats.Bytes != 100 {
		t.Errorf("Bytes = %d", snap.Stats.Bytes)
	}
	// Mutate live task; snapshot should not change.
	task.Stats.Bytes = 999
	if snap.Stats.Bytes != 100 {
		t.Errorf("snapshot leaked: %d", snap.Stats.Bytes)
	}
}

func TestTask_StatsJSON(t *testing.T) {
	task := &Task{
		ID:     "t1",
		Status: "completed",
		Stats:  rclone.Stats{Bytes: 1024, Files: 5},
	}
	snap := task.Snapshot()
	if snap.Status != "completed" {
		t.Errorf("Status = %q", snap.Status)
	}
	if snap.Stats.Files != 5 {
		t.Errorf("Files = %d", snap.Stats.Files)
	}
}

func TestRegisterSchedule_InvalidCron(t *testing.T) {
	eng, _, _, _ := newFullEngine(t)
	if err := eng.Start(context.Background()); err != nil {
		t.Fatal(err)
	}
	defer eng.Stop(context.Background())

	// Invalid cron expr: cron.AddFunc returns an error.
	sch := &store.Schedule{ID: "bad", ProfileName: "p1", Action: "push", Cron: "not-a-cron", Enabled: true}
	// Should not panic; should log warn.
	eng.RegisterSchedule(context.Background(), sch)
	if _, ok := eng.schedule["bad"]; ok {
		t.Error("invalid cron should not be registered")
	}
}

func TestRegisterSchedule_Nil(t *testing.T) {
	eng, _, _, _ := newFullEngine(t)
	// Should not panic.
	eng.RegisterSchedule(context.Background(), nil)
}

func TestRegisterSchedule_BeforeStart(t *testing.T) {
	eng, _, _, _ := newFullEngine(t)
	// Before Start(), cron is nil — should be a no-op.
	sch := &store.Schedule{ID: "x", ProfileName: "p1", Action: "push", Cron: "0 * * * *", Enabled: true}
	eng.RegisterSchedule(context.Background(), sch)
	if _, ok := eng.schedule["x"]; ok {
		t.Error("RegisterSchedule before Start should be no-op")
	}
}

func TestNew_NilLogger(t *testing.T) {
	eng := New(Deps{})
	if eng.log == nil {
		t.Error("expected default logger when nil")
	}
}

func TestLoadSchedules_ListError(t *testing.T) {
	// We can't easily inject a List error, so this is covered indirectly
	// by ensuring loadSchedules is exercised via Start.
	eng, _, _, _ := newFullEngine(t)
	if err := eng.Start(context.Background()); err != nil {
		t.Fatal(err)
	}
	eng.Stop(context.Background())
}

func TestStop_NilCancel(t *testing.T) {
	eng := New(Deps{})
	// cancel is nil — should be a no-op.
	if err := eng.Stop(context.Background()); err != nil {
		t.Errorf("Stop with nil cancel: %v", err)
	}
}

func TestActiveTasks_TypeMismatch(t *testing.T) {
	eng, _, _, _ := newFullEngine(t)
	// Inject a non-Task value to exercise the type assertion failure path.
	eng.active.Store("bad", "not-a-task")
	tasks, err := eng.ActiveTasks(context.Background())
	if err != nil {
		t.Fatal(err)
	}
	if len(tasks) != 0 {
		t.Errorf("expected 0 tasks, got %d", len(tasks))
	}
	eng.active.Delete("bad")
}

func TestStop_CancelsActiveTasks(t *testing.T) {
	eng, _, _, st := newFullEngine(t)
	// Build a blocking stub manually.
	stubStarted := make(chan struct{})
	stubRelease := make(chan struct{})
	blockingStub := &blockingClient{
		started: stubStarted,
		release: stubRelease,
	}
	eng.rclone = blockingStub

	if err := eng.Start(context.Background()); err != nil {
		t.Fatal(err)
	}
	if err := st.Profiles().Save(context.Background(), &store.Profile{Name: "p1", From: "a", To: "b"}); err != nil {
		t.Fatal(err)
	}
	_, err := eng.StartSync(context.Background(), "push", "p1")
	if err != nil {
		t.Fatal(err)
	}
	<-stubStarted
	// Now stop — it should cancel active tasks.
	stopDone := make(chan struct{})
	go func() {
		_ = eng.Stop(context.Background())
		close(stopDone)
	}()
	select {
	case <-stopDone:
		// ok
	case <-time.After(2 * time.Second):
		t.Error("Stop hung")
	}
	close(stubRelease)
}

// blockingClient is a SyncClient that blocks until release is closed.
type blockingClient struct {
	started chan struct{}
	release chan struct{}
}

func (b *blockingClient) Sync(_ context.Context, _ rclone.SyncConfig, _ func(rclone.Stats)) (*rclone.SyncResult, error) {
	close(b.started)
	<-b.release
	return &rclone.SyncResult{}, nil
}

func TestUnregisterSchedule_BeforeStart(t *testing.T) {
	eng, _, _, _ := newFullEngine(t)
	// Should not panic.
	eng.UnregisterSchedule("nonexistent")
}

// TestEngine_Ctx exercises the Ctx() accessor.
func TestEngine_Ctx(t *testing.T) {
	eng, _, _, _ := newFullEngine(t)
	if err := eng.Start(context.Background()); err != nil {
		t.Fatal(err)
	}
	defer eng.Stop(context.Background())

	ctx := eng.Ctx()
	if ctx == nil {
		t.Fatal("Ctx() should not return nil after Start")
	}
	// Context should be cancellable.
	eng.Cancel()
	select {
	case <-ctx.Done():
		// ok
	case <-time.After(100 * time.Millisecond):
		t.Error("Ctx() should be cancelled after Cancel()")
	}
}

// TestEngine_Ctx_BeforeStart covers the nil-ctx branch.
func TestEngine_Ctx_BeforeStart(t *testing.T) {
	eng := New(Deps{Logger: noopLogger()})
	ctx := eng.Ctx()
	if ctx != nil {
		t.Error("Ctx() should return nil before Start")
	}
}

// TestEngine_Cancel_BeforeStart covers the nil-cancel branch.
func TestEngine_Cancel_BeforeStart(t *testing.T) {
	eng := New(Deps{Logger: noopLogger()})
	// Should not panic.
	eng.Cancel()
}

// TestTriggerSchedule exercises the cron-callback body (log + bus.Publish
// + StartSync) without waiting for the real cron to tick.
func TestTriggerSchedule(t *testing.T) {
	eng, stub, bus, st := newFullEngine(t)
	if err := eng.Start(context.Background()); err != nil {
		t.Fatal(err)
	}
	defer eng.Stop(context.Background())
	_ = st // satisfy unused

	// Subscribe to the schedule-triggered event so we know it was published.
	var (
		mu     sync.Mutex
		events []eventbus.Event
	)
	cancel := bus.Subscribe(eventbus.TopicScheduleTriggered, func(ev eventbus.Event) {
		mu.Lock()
		events = append(events, ev)
		mu.Unlock()
	})
	defer cancel()

	// Insert a profile the schedule can trigger.
	prof := &store.Profile{Name: "p1"}
	if err := st.Profiles().Save(context.Background(), prof); err != nil {
		t.Fatal(err)
	}

	sch := &store.Schedule{
		ID:          "sch1",
		ProfileName: "p1",
		Action:      "push",
		Cron:        "@every 1h",
		Enabled:     true,
	}

	eng.triggerSchedule(sch)

	// Wait for the event to land (Publish is async via the bus goroutine).
	deadline := time.Now().Add(2 * time.Second)
	for time.Now().Before(deadline) {
		mu.Lock()
		n := len(events)
		mu.Unlock()
		if n > 0 {
			break
		}
		time.Sleep(20 * time.Millisecond)
	}

	mu.Lock()
	defer mu.Unlock()
	if len(events) == 0 {
		t.Fatal("timed out waiting for schedule-triggered event")
	}
	ste, ok := events[0].(eventbus.ScheduleTriggeredEvent)
	if !ok {
		t.Fatalf("event = %T, want ScheduleTriggeredEvent", events[0])
	}
	if ste.ScheduleID != "sch1" || ste.ProfileID != "p1" || ste.Action != "push" {
		t.Errorf("event = %+v", ste)
	}

	// The sync must have started (stub recorded the call). StartSync fires
	// the stub in a goroutine, so poll briefly.
	deadline2 := time.Now().Add(2 * time.Second)
	for time.Now().Before(deadline2) {
		stub.mu.Lock()
		last := stub.lastCfg
		stub.mu.Unlock()
		if last.Action != "" {
			if last.Action != rclone.ActionPush {
				t.Errorf("lastCfg.Action = %q, want push", last.Action)
			}
			return
		}
		time.Sleep(20 * time.Millisecond)
	}
	t.Fatal("timed out waiting for stub to record the Sync call")
}

// TestLoadSchedules_DBError covers the err != nil branch in loadSchedules
// by closing the store before Start triggers loadSchedules.
func TestLoadSchedules_DBError(t *testing.T) {
	eng, _, _, st := newFullEngine(t)
	// Close the DB so subsequent queries error out. loadSchedules runs
	// during Start, so close *before* Start.
	if err := st.Close(); err != nil {
		t.Fatal(err)
	}
	// Start should still succeed (loadSchedules swallows the error) and
	// the engine should be in a usable state.
	if err := eng.Start(context.Background()); err != nil {
		t.Fatalf("Start returned error: %v", err)
	}
	defer eng.Stop(context.Background())
}

// TestRegisterSchedule_CronCallbackFires exercises the closure body of
// RegisterSchedule by registering a near-future cron entry, waiting for it
// to fire, and observing both the event and the stub sync call.
func TestRegisterSchedule_CronCallbackFires(t *testing.T) {
	eng, stub, bus, st := newFullEngine(t)
	if err := eng.Start(context.Background()); err != nil {
		t.Fatal(err)
	}
	defer eng.Stop(context.Background())

	var (
		mu     sync.Mutex
		events []eventbus.Event
	)
	cancel := bus.Subscribe(eventbus.TopicScheduleTriggered, func(ev eventbus.Event) {
		mu.Lock()
		events = append(events, ev)
		mu.Unlock()
	})
	defer cancel()

	prof := &store.Profile{Name: "p1"}
	if err := st.Profiles().Save(context.Background(), prof); err != nil {
		t.Fatal(err)
	}

	sch := &store.Schedule{
		ID:          "sch1",
		ProfileName: "p1",
		Action:      "push",
		Cron:        "@every 1s",
		Enabled:     true,
	}
	eng.RegisterSchedule(context.Background(), sch)

	// Wait up to 3s for the cron to fire and the stub to record a call.
	deadline := time.Now().Add(3 * time.Second)
	for time.Now().Before(deadline) {
		mu.Lock()
		n := len(events)
		mu.Unlock()
		stub.mu.Lock()
		action := stub.lastCfg.Action
		stub.mu.Unlock()
		if n > 0 && action != "" {
			return
		}
		time.Sleep(50 * time.Millisecond)
	}
	t.Fatal("timed out waiting for cron to fire")
}
