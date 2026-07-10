// Package syncengine provides the sync orchestration engine.
//
// Phase 3: cron scheduler, active task registry, goroutine-based sync execution,
// event emission via eventbus. Delta watcher deferred to Phase 6.
package syncengine

import (
    "context"
    "errors"
    "log/slog"
    "sync"
    "time"

    "github.com/google/uuid"
    "github.com/robfig/cron/v3"

    "github.com/gnasdev/gn-drive/internal/eventbus"
    "github.com/gnasdev/gn-drive/internal/rclone"
    "github.com/gnasdev/gn-drive/internal/store"
)

// ErrNotRunning is returned when the engine is stopped.
var ErrNotRunning = errors.New("syncengine: engine is not running")

// ErrProfileBusy is returned by StartSync when a sync for the same profile is
// already in flight. Concurrent syncs on one profile could let two rclone
// processes mutate the same source/dest simultaneously.
var ErrProfileBusy = errors.New("syncengine: a sync for this profile is already running")

// Engine manages scheduled sync tasks and active task lifecycle.
type Engine struct {
	log      *slog.Logger
	bus      *eventbus.Bus
	store    *store.Store
	rclone   SyncClient
	cron     *cron.Cron
	cronMu   sync.Mutex
	schedule map[string]cron.EntryID // scheduleID → cron entry for lookup & removal
	active   sync.Map                // taskID -> *Task
	// running guards against two concurrent syncs for the same profile, which
	// would let two rclone processes mutate the same paths at once (especially
	// dangerous for bisync). Keyed by profile name.
	runningMu sync.Mutex
	running   map[string]struct{}
	ctx      context.Context
	cancel   context.CancelFunc
}

// SyncClient is the subset of rclone.Client used by the engine.
type SyncClient interface {
	Sync(ctx context.Context, cfg rclone.SyncConfig, onProgress func(rclone.Stats)) (*rclone.SyncResult, error)
}

// Deps holds the engine's dependencies.
type Deps struct {
	Logger *slog.Logger
	Bus    *eventbus.Bus
	Store  *store.Store
	Rclone SyncClient
}

// New creates a new sync engine (not yet started).
func New(deps Deps) *Engine {
    if deps.Logger == nil {
        deps.Logger = slog.Default()
    }
    return &Engine{
        log:    deps.Logger,
        bus:    deps.Bus,
        store:  deps.Store,
        rclone: deps.Rclone,
        running: make(map[string]struct{}),
    }
}

// Start launches the cron scheduler goroutine.
func (e *Engine) Start(ctx context.Context) error {
	if e.ctx != nil {
		return nil // already started
	}
	e.ctx, e.cancel = context.WithCancel(ctx)
	e.cron = cron.New(cron.WithSeconds())
	e.schedule = make(map[string]cron.EntryID)

	if e.store != nil {
		e.loadSchedules()
	}

	e.cron.Start()
	e.log.Info("syncengine: started")
	return nil
}

// Ctx returns the engine's context. It is cancelled when the engine stops.
func (e *Engine) Ctx() context.Context {
	return e.ctx
}

// Cancel cancels the engine's context. Useful for tests.
func (e *Engine) Cancel() {
	if e.cancel != nil {
		e.cancel()
	}
}

// Stop gracefully shuts down the engine.
func (e *Engine) Stop(ctx context.Context) error {
    if e.cancel == nil {
        return nil
    }
    e.cancel()
    if e.cron != nil {
        <-e.cron.Stop().Done()
    }
	e.active.Range(func(_, v any) bool {
		if t, ok := v.(*Task); ok {
			t.Cancel()
		}
		return true
	})
    e.active = sync.Map{}
    e.runningMu.Lock()
    e.running = make(map[string]struct{})
    e.runningMu.Unlock()
    e.schedule = nil
    e.cron = nil
    e.ctx = nil
    e.cancel = nil
    e.log.Info("syncengine: stopped")
    return nil
}

// StartSync starts a sync task and returns its taskID.
func (e *Engine) StartSync(ctx context.Context, action, profileName string) (string, error) {
    if e.ctx == nil {
        return "", ErrNotRunning
    }

    p, err := e.store.Profiles().Get(ctx, profileName)
    if err != nil {
        return "", err
    }

    // Reject a concurrent run for the same profile (see ErrProfileBusy).
    // The entry is cleared by runSync's deferred cleanup when the sync ends.
    e.runningMu.Lock()
    if _, busy := e.running[profileName]; busy {
        e.runningMu.Unlock()
        return "", ErrProfileBusy
    }
    e.running[profileName] = struct{}{}
    e.runningMu.Unlock()

    task := &Task{
        ID:     uuid.New().String(),
        Name:   profileName,
        Action: action,
        Status: "running",
    }
    e.active.Store(task.ID, task)
    task.ctx, task.cancel = context.WithCancel(e.ctx)

    go e.runSync(task, p, action)

    e.bus.Publish(eventbus.TopicSyncStarted, eventbus.SyncStartedEvent{
        TaskID:    task.ID,
        ProfileID: profileName,
        Action:    action,
    })

    return task.ID, nil
}

// StopSync cancels an active sync task.
func (e *Engine) StopSync(ctx context.Context, taskID string) error {
    if v, ok := e.active.Load(taskID); ok {
        t := v.(*Task)
        t.Cancel()
        return nil
    }
    return errors.New("syncengine: task not found")
}

// ActiveTasks returns snapshots of all currently running tasks. The
// returned slices are decoupled from the live Task instances: callers
// can iterate, marshal, and modify the snapshots without holding the
// engine's mutex.
func (e *Engine) ActiveTasks(ctx context.Context) ([]TaskSnapshot, error) {
    var tasks []TaskSnapshot
    e.active.Range(func(_, v any) bool {
        t, ok := v.(*Task)
        if !ok {
            return true
        }
        tasks = append(tasks, t.Snapshot())
        return true
    })
    return tasks, nil
}

// RegisterSchedule adds or updates a cron job for a schedule. If a job
// for this schedule ID already exists, it is removed and re-registered
// with the new cron expression (or skipped when disabled).
//
// Cron expressions may be 5-field (UI) or 6-field (seconds-aware). They are
// normalized via NormalizeCron before registration.
func (e *Engine) RegisterSchedule(ctx context.Context, sch *store.Schedule) {
    if sch == nil {
        return
    }
    e.cronMu.Lock()
    defer e.cronMu.Unlock()
    if e.cron == nil {
        return
    }
    // Always drop any prior entry for this schedule ID so re-registration
    // with a new cron expression replaces the old one cleanly.
    e.removeLocked(sch.ID)
    if !sch.Enabled || sch.Cron == "" {
        return
    }
    expr, err := NormalizeCron(sch.Cron)
    if err != nil {
        e.log.Warn("cron: add func failed", "schedule", sch.ID, "err", err)
        return
    }
    id, err := e.cron.AddFunc(expr, func() {
        e.triggerSchedule(sch)
    })
    if err != nil {
        e.log.Warn("cron: add func failed", "schedule", sch.ID, "err", err)
        return
    }
    e.schedule[sch.ID] = id
    e.log.Info("cron: registered", "schedule", sch.ID, "cron", expr)
}

// triggerSchedule runs the body of a cron-fired schedule: log, publish the
// event, and start the sync. It is a separate method so tests can invoke it
// without waiting for the real cron to tick.
func (e *Engine) triggerSchedule(sch *store.Schedule) {
    e.log.Info("cron: triggering", "schedule", sch.ID, "profile", sch.ProfileName)
    e.bus.Publish(eventbus.TopicScheduleTriggered, eventbus.ScheduleTriggeredEvent{
        ScheduleID: sch.ID,
        ProfileID:  sch.ProfileName,
        Action:     sch.Action,
    })
    if _, err := e.StartSync(context.Background(), sch.Action, sch.ProfileName); err != nil {
        e.log.Warn("cron: sync not started", "schedule", sch.ID, "profile", sch.ProfileName, "err", err)
    }
}

// UnregisterSchedule removes the cron entry for the given schedule ID.
// Safe to call before Start (no-op) and when no entry exists (no-op).
func (e *Engine) UnregisterSchedule(id string) {
    e.cronMu.Lock()
    defer e.cronMu.Unlock()
    e.removeLocked(id)
}

// removeLocked is the unsynchronized helper. Caller must hold e.cronMu.
func (e *Engine) removeLocked(id string) {
    entryID, ok := e.schedule[id]
    if !ok {
        return
    }
    if e.cron != nil {
        e.cron.Remove(entryID)
    }
    delete(e.schedule, id)
    e.log.Info("cron: unregistered", "schedule", id)
}

func (e *Engine) loadSchedules() {
    ctx := context.Background()
    schedules, err := e.store.Schedules().List(ctx)
    if err != nil {
        e.log.Warn("syncengine: load schedules", "err", err)
        return
    }
    for i := range schedules {
        e.RegisterSchedule(ctx, &schedules[i])
    }
}

func (e *Engine) runSync(t *Task, p *store.Profile, action string) {
    defer func() {
        e.active.Delete(t.ID)
        e.runningMu.Lock()
        delete(e.running, p.Name)
        e.runningMu.Unlock()
    }()

    startedAt := time.Now()
    t.Mu.Lock()
    t.StartedAt = startedAt
    t.Mu.Unlock()

    e.log.Info("sync: started", "task", t.ID, "profile", p.Name, "action", action)

    // Record the run as in-progress so the History view shows it immediately.
    // History().Save upserts by task ID, so the terminal Save below updates
    // this same row rather than inserting a duplicate.
    e.saveHistory(&store.HistoryEntry{
        ID:          t.ID,
        ProfileName: p.Name,
        Action:      action,
        State:       "running",
        StartedAt:   startedAt.UTC().Format(time.RFC3339),
    })

    res, err := e.rclone.Sync(t.ctx, rclone.SyncConfig{
        Action: rclone.Action(action),
        Source: p.From,
        Dest:   p.To,
        Profile: &rclone.ProfileFlags{
            Transfers: p.Parallel,
            DryRun:    p.DryRun,
            MaxAge:    p.MaxAge,
            MaxSize:   p.MaxSize,
        },
    }, func(s rclone.Stats) {
        t.Mu.Lock()
        t.Stats = s
        t.Mu.Unlock()
        e.bus.Publish(eventbus.TopicSyncProgress, eventbus.SyncProgressEvent{
            TaskID:          t.ID,
            ProfileID:       p.Name,
            Action:          action,
            State:           "running",
            Transferred:     s.Bytes,
            Total:           s.BytesTotal,
            FilesTransferred: int(s.Files),
            TotalFiles:      int(s.FilesTotal),
            Errors:          int(s.Errors),
            CurrentFile:     s.CurrentFile,
        })
    })

    endedAt := time.Now()

    // Prefer the result's parsed stats; fall back to the last progress
    // snapshot if the run errored before producing a SyncResult.
    finalStats := t.Stats
    if res != nil {
        finalStats = res.Stats
    }

    t.Mu.Lock()
    t.EndedAt = endedAt
    var state string
    if err != nil {
        state = "failed"
        t.Status = state
        e.bus.Publish(eventbus.TopicSyncFailed, eventbus.SyncProgressEvent{
            TaskID:       t.ID,
            ProfileID:    p.Name,
            Action:       action,
            State:        "failed",
            ErrorMessage: truncateErrMsg(err.Error(), 1000),
        })
        e.log.Error("sync: failed", "task", t.ID, "profile", p.Name, "err", err)
    } else {
        state = "completed"
        t.Status = state
        e.bus.Publish(eventbus.TopicSyncCompleted, eventbus.SyncCompletedEvent{
            TaskID:    t.ID,
            ProfileID: p.Name,
            Action:    action,
            Duration:  res.EndedAt - res.StartedAt,
            Bytes:     res.Stats.Bytes,
            Errors:    int(res.Stats.Errors),
        })
        e.log.Info("sync: completed", "task", t.ID, "profile", p.Name, "bytes", res.Stats.Bytes, "errors", res.Stats.Errors)
    }
    t.Mu.Unlock()

    // Persist the terminal outcome to history (upsert over the running row).
    entry := &store.HistoryEntry{
        ID:          t.ID,
        ProfileName: p.Name,
        Action:      action,
        State:       state,
        StartedAt:   startedAt.UTC().Format(time.RFC3339),
        FinishedAt:  endedAt.UTC().Format(time.RFC3339),
        Duration:    int64(endedAt.Sub(startedAt).Seconds()),
        Bytes:       finalStats.Bytes,
        Files:       int(finalStats.Files),
        Errors:      int(finalStats.Errors),
    }
    if err != nil {
        entry.ErrorMessage = truncateErrMsg(err.Error(), 1000)
    }
    e.saveHistory(entry)
}

// saveHistory persists a sync history entry. It tolerates a nil store (some
// tests construct the engine without one) and logs—rather than fails—on
// error, so a sync outcome is not lost on a transient write hiccup.
func (e *Engine) saveHistory(entry *store.HistoryEntry) {
    if e.store == nil {
        return
    }
    if err := e.store.History().Save(context.Background(), entry); err != nil {
        e.log.Warn("sync: persist history failed", "task", entry.ID, "err", err)
    }
}

// truncateErrMsg bounds the length of an error string stored in history.
func truncateErrMsg(s string, n int) string {
    if len(s) <= n {
        return s
    }
    return s[:n]
}
