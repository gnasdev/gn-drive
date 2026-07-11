// Package syncengine provides the sync orchestration engine:
// task registry, profile and flow cron schedules, and event emission.
package syncengine

import (
	"context"
	"errors"
	"fmt"
	"log/slog"
	"strings"
	"sync"
	"time"

	"github.com/google/uuid"
	"github.com/robfig/cron/v3"

	"github.com/gnasdev/gn-drive/internal/eventbus"
	"github.com/gnasdev/gn-drive/internal/rclone"
	"github.com/gnasdev/gn-drive/internal/store"
)

// flowSchedulePrefix namespaces cron IDs for flow schedules (vs profile schedules).
const flowSchedulePrefix = "flow:"

// FlowScheduleID returns the cron map key for a flow.
func FlowScheduleID(flowID string) string {
	return flowSchedulePrefix + strings.TrimSpace(flowID)
}

// FlowExecutor runs a scheduled flow. Implemented by flowengine.Engine;
// kept as an interface to avoid an import cycle.
type FlowExecutor interface {
	Execute(ctx context.Context, flowID string) error
}

// ErrNotRunning is returned when the engine is stopped.
var ErrNotRunning = errors.New("syncengine: engine is not running")

// ErrProfileBusy is returned by StartSync when a sync for the same profile is
// already in flight. Concurrent syncs on one profile could let two rclone
// processes mutate the same source/dest simultaneously.
var ErrProfileBusy = errors.New("syncengine: a sync for this profile is already running")

// ErrTaskFailed is returned by WaitTask when the sync finished unsuccessfully.
var ErrTaskFailed = errors.New("syncengine: task failed")

// ErrTaskCancelled is returned by WaitTask when the task was cancelled.
var ErrTaskCancelled = errors.New("syncengine: task cancelled")

// taskOutcome is stored when a task leaves the active map so WaitTask can
// distinguish success vs failure (previously WaitTask always returned nil
// once the task disappeared, so flowengine treated failed ops as completed).
type taskOutcome struct {
	status string // completed | failed | cancelled
	errMsg string
}

// Engine manages scheduled sync tasks and active task lifecycle.
type Engine struct {
	log      *slog.Logger
	bus      *eventbus.Bus
	store    *store.Store
	rclone   SyncClient
	flowExec FlowExecutor
	cron     *cron.Cron
	cronMu   sync.Mutex
	schedule map[string]cron.EntryID // scheduleID → cron entry for lookup & removal
	active   sync.Map                // taskID -> *Task
	// outcomes holds terminal status for WaitTask after active.Delete.
	outcomes sync.Map // taskID -> taskOutcome
	// running guards against two concurrent syncs for the same profile, which
	// would let two rclone processes mutate the same paths at once (especially
	// dangerous for bisync). Keyed by profile name.
	runningMu sync.Mutex
	running   map[string]struct{}
	ctx       context.Context
	cancel    context.CancelFunc
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

// SetFlowExecutor wires the flow runner used by flow cron schedules.
func (e *Engine) SetFlowExecutor(fe FlowExecutor) {
	e.flowExec = fe
}

// AttachStore wires store + rclone after portal unlock (deferred data plane).
func (e *Engine) AttachStore(st *store.Store, rc SyncClient) {
	e.store = st
	e.rclone = rc
	if e.ctx != nil && st != nil {
		e.loadSchedules()
	}
}

// DetachStore drops store/rclone references before lock re-encrypts files.
func (e *Engine) DetachStore() {
	e.store = nil
	e.rclone = nil
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

// StartSync starts a sync task for a named profile and returns its taskID.
func (e *Engine) StartSync(ctx context.Context, action, profileName string) (string, error) {
    if e.ctx == nil {
        return "", ErrNotRunning
    }
    if e.store == nil {
        return "", errors.New("syncengine: store not ready")
    }

    p, err := e.store.Profiles().Get(ctx, profileName)
    if err != nil {
        return "", err
    }
    return e.startWithProfile(ctx, action, profileName, p)
}

// StartPathSync runs an ad-hoc sync (flow operation) without a stored profile.
// busyKey is used for concurrency locking (e.g. flowID:opID).
// opts may be nil; when set, rclone flags (parallel, bandwidth, filters, …)
// are taken from the profile fields (Name/From/To are overwritten).
func (e *Engine) StartPathSync(ctx context.Context, action, busyKey, from, to string, opts *store.Profile) (string, error) {
	if e.ctx == nil {
		return "", ErrNotRunning
	}
	if from == "" || to == "" {
		return "", errors.New("syncengine: from and to are required")
	}
	var p store.Profile
	if opts != nil {
		p = *opts
	}
	p.Name = busyKey
	p.From = from
	p.To = to
	if p.Parallel <= 0 {
		p.Parallel = 4
	}
	return e.startWithProfile(ctx, action, busyKey, &p)
}

func (e *Engine) startWithProfile(ctx context.Context, action, busyKey string, p *store.Profile) (string, error) {
    // Reject a concurrent run for the same key (see ErrProfileBusy).
    e.runningMu.Lock()
    if _, busy := e.running[busyKey]; busy {
        e.runningMu.Unlock()
        return "", ErrProfileBusy
    }
    e.running[busyKey] = struct{}{}
    e.runningMu.Unlock()

    task := &Task{
        ID:     uuid.New().String(),
        Name:   busyKey,
        Action: action,
        Status: "running",
    }
    e.active.Store(task.ID, task)
    task.ctx, task.cancel = context.WithCancel(e.ctx)

    go e.runSync(task, p, action)

    e.bus.Publish(eventbus.TopicSyncStarted, eventbus.SyncStartedEvent{
        TaskID:    task.ID,
        ProfileID: busyKey,
        Action:    action,
    })

    return task.ID, nil
}

// WaitTask blocks until the given task finishes and returns nil only on success.
// Failed syncs return ErrTaskFailed; cancellations return ErrTaskCancelled
// (or ctx.Err() if the waiter's context was cancelled first).
func (e *Engine) WaitTask(ctx context.Context, taskID string) error {
	ticker := time.NewTicker(100 * time.Millisecond)
	defer ticker.Stop()
	for {
		if v, ok := e.outcomes.Load(taskID); ok {
			o := v.(taskOutcome)
			// One-shot: free memory after the waiter consumes the outcome.
			e.outcomes.Delete(taskID)
			switch o.status {
			case "completed":
				return nil
			case "cancelled":
				return ErrTaskCancelled
			default:
				if o.errMsg != "" {
					return errors.Join(ErrTaskFailed, errors.New(o.errMsg))
				}
				return ErrTaskFailed
			}
		}
		select {
		case <-ctx.Done():
			return ctx.Err()
		case <-ticker.C:
		}
	}
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
	} else {
		for i := range schedules {
			e.RegisterSchedule(ctx, &schedules[i])
		}
	}
	e.loadFlowSchedules(ctx)
}

func (e *Engine) loadFlowSchedules(ctx context.Context) {
	if e.store == nil {
		return
	}
	flows, err := e.store.Flows().List(ctx)
	if err != nil {
		e.log.Warn("syncengine: load flow schedules", "err", err)
		return
	}
	for i := range flows {
		e.SyncFlowSchedule(&flows[i])
	}
}

// SyncFlowSchedule registers or removes a flow's cron job from its persisted fields.
// Safe to call before Start (no-op until cron exists) and after flow save/delete.
func (e *Engine) SyncFlowSchedule(f *store.Flow) {
	if f == nil || strings.TrimSpace(f.ID) == "" {
		return
	}
	id := FlowScheduleID(f.ID)
	enabled := f.ScheduleEnabled || f.Enabled
	cronExpr := strings.TrimSpace(f.ScheduleCron)
	if cronExpr == "" {
		cronExpr = strings.TrimSpace(f.CronExpr)
	}

	e.cronMu.Lock()
	defer e.cronMu.Unlock()
	if e.cron == nil {
		return
	}
	e.removeLocked(id)
	if !enabled || cronExpr == "" || e.flowExec == nil {
		return
	}
	expr, err := NormalizeCron(cronExpr)
	if err != nil {
		e.log.Warn("cron: flow schedule invalid", "flow", f.ID, "err", err)
		return
	}
	flowID := f.ID
	entryID, err := e.cron.AddFunc(expr, func() {
		e.triggerFlowSchedule(flowID)
	})
	if err != nil {
		e.log.Warn("cron: flow add func failed", "flow", f.ID, "err", err)
		return
	}
	e.schedule[id] = entryID
	e.log.Info("cron: registered flow", "flow", f.ID, "cron", expr)
}

// UnregisterFlowSchedule removes a flow's cron entry (e.g. on delete).
func (e *Engine) UnregisterFlowSchedule(flowID string) {
	e.UnregisterSchedule(FlowScheduleID(flowID))
}

func (e *Engine) triggerFlowSchedule(flowID string) {
	e.log.Info("cron: triggering flow", "flow", flowID)
	e.bus.Publish(eventbus.TopicScheduleTriggered, eventbus.ScheduleTriggeredEvent{
		ScheduleID: FlowScheduleID(flowID),
		ProfileID:  flowID,
		Action:     "flow",
	})
	if e.flowExec == nil {
		e.log.Warn("cron: flow executor not set", "flow", flowID)
		return
	}
	if err := e.flowExec.Execute(context.Background(), flowID); err != nil {
		// Already running is expected when a long flow overlaps the next tick.
		e.log.Warn("cron: flow execute", "flow", flowID, "err", err)
	}
}

func (e *Engine) runSync(t *Task, p *store.Profile, action string) {
	defer func() {
		// Publish outcome BEFORE deleting from active so WaitTask never
		// observes "gone" without a terminal result.
		t.Mu.Lock()
		st := t.Status
		errMsg := t.Error
		t.Mu.Unlock()
		if st == "" || st == "running" {
			st = "failed"
		}
		e.outcomes.Store(t.ID, taskOutcome{status: st, errMsg: errMsg})
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
        Action:  rclone.Action(action),
        Source:  p.From,
        Dest:    p.To,
        Profile: profileFlagsFromStore(p),
    }, func(s rclone.Stats) {
        t.Mu.Lock()
        t.Stats = s
        t.Mu.Unlock()
		e.publishSyncProgress(t.ID, p.Name, action, "running", s, "")
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
		// Prefer cancelled when the task context (or cancel flag) says so.
		if t.ctx != nil && t.ctx.Err() != nil {
			state = "cancelled"
		} else if t.Status == "cancelled" {
			state = "cancelled"
		} else {
			state = "failed"
		}
		t.Status = state
		// User-facing message only — never dump rclone JSON stderr on cancel.
		if state == "cancelled" {
			t.Error = ""
		} else {
			t.Error = truncateErrMsg(sanitizeRcloneErr(err.Error()), 240)
		}
		// Final progress carries FileTransfers so the Pending/Complete/Failed
		// tabs still render after the run ends (completed event has no list).
		e.publishSyncProgress(t.ID, p.Name, action, state, finalStats, t.Error)
		e.bus.Publish(eventbus.TopicSyncFailed, eventbus.SyncProgressEvent{
			TaskID:       t.ID,
			ProfileID:    p.Name,
			Action:       action,
			State:        state,
			ErrorMessage: t.Error,
			Transfers:    fileTransfersFromStats(finalStats),
		})
		e.log.Error("sync: "+state, "task", t.ID, "profile", p.Name, "err", err)
	} else {
		state = "completed"
		t.Status = state
		e.publishSyncProgress(t.ID, p.Name, action, state, finalStats, "")
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

// profileFlagsFromStore maps store.Profile (and flow SyncConfig-filled profiles)
// onto rclone.ProfileFlags for the CLI shell-out.
func profileFlagsFromStore(p *store.Profile) *rclone.ProfileFlags {
	if p == nil {
		return nil
	}
	f := &rclone.ProfileFlags{
		Transfers:           p.Parallel,
		DryRun:              p.DryRun,
		MaxAge:              p.MaxAge,
		MinAge:              p.MinAge,
		MaxSize:             p.MaxSize,
		MinSize:             p.MinSize,
		ExcludeIfPresent:    p.ExcludeIfPresent,
		Includes:            append([]string(nil), p.IncludedPaths...),
		Excludes:            append([]string(nil), p.ExcludedPaths...),
		BufferSize:          p.BufferSize,
		MaxDuration:         p.MaxDuration,
		RetriesSleep:        p.RetriesSleep,
		ConnTimeout:         p.ConnTimeout,
		IoTimeout:           p.IoTimeout,
		OrderBy:             p.OrderBy,
		CheckFirst:          p.CheckFirst,
		Immutable:           p.Immutable,
		MaxTransfer:         p.MaxTransfer,
		MaxDeleteSize:       p.MaxDeleteSize,
		Suffix:              p.Suffix,
		SuffixKeepExtension: p.SuffixKeepExtension,
		BackupDir:           p.BackupPath,
		SizeOnly:            p.SizeOnly,
		UpdateMode:          p.UpdateMode,
		IgnoreExisting:      p.IgnoreExisting,
		DeleteExcluded:      p.DeleteExcluded,
		DeleteTiming:        p.DeleteTiming,
		ConflictResolve:     p.ConflictResolution,
		ConflictLoser:       p.ConflictLoser,
		ConflictSuffix:      p.ConflictSuffix,
		Resilient:           p.Resilient,
		MaxLock:             p.MaxLock,
		CheckAccess:         p.CheckAccess,
	}
	if p.Bandwidth > 0 {
		f.Bandwidth = fmt.Sprintf("%dM", p.Bandwidth)
	}
	if p.TpsLimit != nil && *p.TpsLimit > 0 {
		f.TpsLimit = *p.TpsLimit
	}
	if p.MultiThreadStreams != nil && *p.MultiThreadStreams > 0 {
		f.MultiThreadStreams = *p.MultiThreadStreams
	}
	if p.Retries != nil && *p.Retries > 0 {
		f.Retries = *p.Retries
	}
	if p.LowLevelRetries != nil && *p.LowLevelRetries > 0 {
		f.LowLevelRetries = *p.LowLevelRetries
	}
	if p.MaxDelete != nil && *p.MaxDelete > 0 {
		f.MaxDelete = *p.MaxDelete
	}
	if p.MaxDepth != nil && *p.MaxDepth > 0 {
		f.MaxDepth = *p.MaxDepth
	}
	return f
}

func fileTransfersFromStats(s rclone.Stats) []eventbus.FileTransferEvent {
	transfers := make([]eventbus.FileTransferEvent, 0, len(s.FileTransfers))
	for _, ft := range s.FileTransfers {
		transfers = append(transfers, eventbus.FileTransferEvent{
			Name:     ft.Name,
			Size:     ft.Size,
			Bytes:    ft.Bytes,
			Progress: ft.Progress,
			Status:   ft.Status,
			Speed:    ft.Speed,
			Error:    ft.Error,
		})
	}
	return transfers
}

func (e *Engine) publishSyncProgress(taskID, profileID, action, state string, s rclone.Stats, errMsg string) {
	if e.bus == nil {
		return
	}
	e.bus.Publish(eventbus.TopicSyncProgress, eventbus.SyncProgressEvent{
		TaskID:           taskID,
		ProfileID:        profileID,
		Action:           action,
		State:            state,
		Transferred:      s.Bytes,
		Total:            s.BytesTotal,
		BytesPerSec:      s.Speed,
		ETA:              s.ETA,
		FilesTransferred: int(s.Files),
		TotalFiles:       int(s.FilesTotal),
		Errors:           int(s.Errors),
		CurrentFile:      s.CurrentFile,
		Checks:           s.Checks,
		TotalChecks:      s.ChecksTotal,
		Deletes:          s.Deletes,
		Renames:          s.Renames,
		Transfers:        fileTransfersFromStats(s),
		ErrorMessage:     errMsg,
	})
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

// sanitizeRcloneErr strips rclone JSON stderr dumps and "signal: killed" noise
// so the UI never shows multi-kilobyte log blobs.
func sanitizeRcloneErr(s string) string {
	lower := strings.ToLower(s)
	if strings.Contains(lower, "signal: killed") ||
		strings.Contains(lower, "signal: interrupt") ||
		strings.Contains(lower, "context canceled") ||
		strings.Contains(lower, "context cancelled") {
		return ""
	}
	// Drop parenthetical stderr payloads: "rclone: exit status 1 (stderr: {...})"
	if i := strings.Index(s, "(stderr:"); i >= 0 {
		s = strings.TrimSpace(s[:i])
	}
	if strings.HasPrefix(strings.TrimSpace(s), "{") {
		return "sync failed"
	}
	return s
}
