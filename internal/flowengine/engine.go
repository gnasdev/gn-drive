// Package flowengine runs Flow operations sequentially (Wails FlowsService.executeFlow).
package flowengine

import (
	"context"
	"errors"
	"log/slog"
	"strings"
	"sync"

	"github.com/gnasdev/gn-drive/internal/eventbus"
	"github.com/gnasdev/gn-drive/internal/store"
	"github.com/gnasdev/gn-drive/internal/syncengine"
)

var (
	ErrAlreadyRunning = errors.New("flowengine: flow is already running")
	ErrNotRunning     = errors.New("flowengine: flow is not running")
	ErrEmptyFlow      = errors.New("flowengine: flow has no operations")
	ErrEngineNotReady = errors.New("flowengine: sync engine not ready")
)

// Engine executes flows (operations in order).
type Engine struct {
	store *store.Store
	sync  *syncengine.Engine
	bus   *eventbus.Bus
	log   *slog.Logger

	mu   sync.Mutex
	runs map[string]*run // flowID → active run
	// lastStatus retains the most recent terminal status per flow so API
	// clients (and FE poll) can observe completed/failed after the run ends.
	// Without this, Status() returns "idle" immediately and the UI poll
	// incorrectly upgrades failures to "completed".
	lastStatus map[string]string
}

type run struct {
	cancel context.CancelFunc
	status string
}

// Options configures the engine.
type Options struct {
	Store *store.Store
	Sync  *syncengine.Engine
	Bus   *eventbus.Bus
	Log   *slog.Logger
}

// New creates a flow engine.
func New(opts Options) *Engine {
	log := opts.Log
	if log == nil {
		log = slog.Default()
	}
	return &Engine{
		store:      opts.Store,
		sync:       opts.Sync,
		bus:        opts.Bus,
		log:        log,
		runs:       make(map[string]*run),
		lastStatus: make(map[string]string),
	}
}

// Attach wires store/sync after portal unlock.
func (e *Engine) Attach(st *store.Store, se *syncengine.Engine) {
	e.store = st
	e.sync = se
}

// Detach clears references before re-encrypt.
func (e *Engine) Detach() {
	e.store = nil
	// keep sync pointer; it is detached separately
}

// IsRunning reports whether a flow is mid-execution.
func (e *Engine) IsRunning(flowID string) bool {
	e.mu.Lock()
	defer e.mu.Unlock()
	return e.runs[flowID] != nil
}

// Status returns runtime status for a flow.
// Active run → running/cancelling; after finish → last terminal status;
// never started → "idle".
func (e *Engine) Status(flowID string) string {
	e.mu.Lock()
	defer e.mu.Unlock()
	if r := e.runs[flowID]; r != nil {
		return r.status
	}
	if s, ok := e.lastStatus[flowID]; ok && s != "" {
		return s
	}
	return "idle"
}

// Execute starts sequential execution of a flow's operations in a goroutine.
// The load uses ctx; the background run uses an independent cancellable
// context so HTTP request cancellation does not abort the flow.
func (e *Engine) Execute(ctx context.Context, flowID string) error {
	if e.store == nil || e.sync == nil {
		return ErrEngineNotReady
	}
	if ctx == nil {
		ctx = context.Background()
	}
	f, err := e.store.Flows().Get(ctx, flowID)
	if err != nil {
		return err
	}
	if len(f.Operations) == 0 {
		return ErrEmptyFlow
	}

	e.mu.Lock()
	if e.runs[flowID] != nil {
		e.mu.Unlock()
		return ErrAlreadyRunning
	}
	// Independent of the caller's context (e.g. HTTP request).
	runCtx, cancel := context.WithCancel(context.Background())
	e.runs[flowID] = &run{cancel: cancel, status: "running"}
	// Clear previous terminal status while a new run is active.
	delete(e.lastStatus, flowID)
	e.mu.Unlock()

	e.publish(flowID, "", "running", "")

	go e.run(runCtx, f)
	return nil
}

// Stop cancels an in-flight flow.
func (e *Engine) Stop(flowID string) error {
	e.mu.Lock()
	r := e.runs[flowID]
	if r == nil {
		e.mu.Unlock()
		return ErrNotRunning
	}
	r.status = "cancelling"
	cancel := r.cancel
	e.mu.Unlock()
	cancel()
	e.publish(flowID, "", "cancelling", "")
	return nil
}

func (e *Engine) run(ctx context.Context, f *store.Flow) {
	final := "completed"
	defer func() {
		e.mu.Lock()
		delete(e.runs, f.ID)
		e.lastStatus[f.ID] = final
		e.mu.Unlock()
		e.publish(f.ID, "", final, "")
		e.log.Info("flow finished", "flow", f.ID, "status", final)
	}()

	for i := range f.Operations {
		if ctx.Err() != nil {
			final = "cancelled"
			return
		}
		op := f.Operations[i]
		// Wails board executeEdge: From=source, To=target always.
		// Pull reverses data flow inside rclone (not by swapping stored paths).
		action := op.ResolveAction()
		from := store.ComposePath(op.SourceRemote, op.SourcePath)
		to := store.ComposePath(op.TargetRemote, op.TargetPath)
		// Full Wails SyncConfig → profile flags (parallel, filters, bisync, …).
		opts := profileFromSyncConfig(op.SyncConfig)

		e.publish(f.ID, op.ID, "running", "")

		busyKey := f.ID + ":" + op.ID
		taskID, err := e.sync.StartPathSync(ctx, action, busyKey, from, to, opts)
		if err != nil {
			e.log.Error("flow op start failed", "flow", f.ID, "op", op.ID, "err", err)
			e.publish(f.ID, op.ID, "failed", err.Error())
			final = "failed"
			return
		}
		if err := e.sync.WaitTask(ctx, taskID); err != nil {
			if errors.Is(err, context.Canceled) || errors.Is(err, syncengine.ErrTaskCancelled) || ctx.Err() != nil {
				_ = e.sync.StopSync(context.Background(), taskID)
				// Empty message — FE shows a friendly "Stopped" label, not raw stderr.
				e.publish(f.ID, op.ID, "cancelled", "")
				final = "cancelled"
				return
			}
			// Real sync failure — do NOT treat as completed.
			msg := friendlyTaskErr(err)
			e.log.Error("flow op failed", "flow", f.ID, "op", op.ID, "err", err)
			e.publish(f.ID, op.ID, "failed", msg)
			final = "failed"
			return
		}
		e.publish(f.ID, op.ID, "completed", "")
	}
}

func (e *Engine) publish(flowID, opID, status, msg string) {
	if e.bus == nil {
		return
	}
	// Prefer dedicated flow topic; also emit board:execution for older clients.
	ev := eventbus.FlowExecutionEvent{
		FlowID: flowID,
		OpID:   opID,
		Status: status,
		Error:  msg,
	}
	e.bus.Publish(eventbus.TopicFlowExecution, ev)
	e.bus.Publish(eventbus.TopicBoardExecution, eventbus.BoardExecutionEvent{
		BoardID: flowID,
		NodeID:  opID,
		Status:  status,
		Action:  msg,
	})
}

func friendlyTaskErr(err error) string {
	if err == nil {
		return ""
	}
	s := err.Error()
	s = strings.TrimPrefix(s, "syncengine: task failed\n")
	s = strings.TrimPrefix(s, "syncengine: task failed: ")
	if i := strings.Index(s, "(stderr:"); i >= 0 {
		s = strings.TrimSpace(s[:i])
	}
	if len(s) > 200 {
		s = s[:200] + "…"
	}
	return s
}


