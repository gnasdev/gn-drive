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

// Engine manages scheduled sync tasks and active task lifecycle.
type Engine struct {
    log     *slog.Logger
    bus     *eventbus.Bus
    store   *store.Store
    rclone  *rclone.Client
    cron    *cron.Cron
    cronMu  sync.Mutex
    active  sync.Map // taskID -> *Task
    ctx     context.Context
    cancel  context.CancelFunc
}

// Deps holds the engine's dependencies.
type Deps struct {
    Logger *slog.Logger
    Bus    *eventbus.Bus
    Store  *store.Store
    Rclone *rclone.Client
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
    }
}

// Start launches the cron scheduler goroutine.
func (e *Engine) Start(ctx context.Context) error {
    if e.ctx != nil {
        return nil // already started
    }
    e.ctx, e.cancel = context.WithCancel(ctx)
    e.cron = cron.New(cron.WithSeconds())

    if e.store != nil {
        e.loadSchedules()
    }

    e.cron.Start()
    e.log.Info("syncengine: started")
    return nil
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
    e.active.Range(func(key, _ any) bool {
        if t, ok := key.(*Task); ok {
            t.Cancel()
        }
        return true
    })
    e.active = sync.Map{}
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

// ActiveTasks returns all currently running tasks (snapshots).
func (e *Engine) ActiveTasks(ctx context.Context) ([]Task, error) {
    var tasks []Task
    e.active.Range(func(_, v any) bool {
        t, ok := v.(*Task)
        if !ok {
            return true
        }
        snap := Task{
            ID:     t.ID,
            Name:   t.Name,
            Action: t.Action,
            Status: t.Status,
        }
        t.Mu.Lock()
        snap.Stats = t.Stats
        snap.StartedAt = t.StartedAt
        snap.EndedAt = t.EndedAt
        t.Mu.Unlock()
        tasks = append(tasks, snap)
        return true
    })
    return tasks, nil
}

// RegisterSchedule adds or updates a cron job for a schedule.
func (e *Engine) RegisterSchedule(ctx context.Context, sch *store.Schedule) {
    if sch == nil || !sch.Enabled || sch.Cron == "" {
        return
    }
    e.cronMu.Lock()
    defer e.cronMu.Unlock()
    if e.cron == nil {
        return
    }
    _, err := e.cron.AddFunc(sch.Cron, func() {
        e.log.Info("cron: triggering", "schedule", sch.ID, "profile", sch.ProfileName)
        e.bus.Publish(eventbus.TopicScheduleTriggered, eventbus.ScheduleTriggeredEvent{
            ScheduleID: sch.ID,
            ProfileID:  sch.ProfileName,
            Action:     sch.Action,
        })
        e.StartSync(context.Background(), sch.Action, sch.ProfileName)
    })
    if err != nil {
        e.log.Warn("cron: add func failed", "schedule", sch.ID, "err", err)
        return
    }
    e.log.Info("cron: registered", "schedule", sch.ID, "cron", sch.Cron)
}

// UnregisterSchedule is a stub — cron/v3 doesn't expose direct removal.
func (e *Engine) UnregisterSchedule(id string) {}

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
    }()

    e.log.Info("sync: started", "task", t.ID, "profile", p.Name, "action", action)

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

    t.Mu.Lock()
    t.EndedAt = time.Now()
    if err != nil {
        t.Status = "failed"
        e.bus.Publish(eventbus.TopicSyncFailed, eventbus.SyncProgressEvent{
            TaskID:    t.ID,
            ProfileID: p.Name,
            Action:    action,
            State:     "failed",
        })
        e.log.Error("sync: failed", "task", t.ID, "profile", p.Name, "err", err)
    } else {
        t.Status = "completed"
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
}
