// Package boardengine executes board DAGs (nodes + edges) in topological order.
// Shared by the CLI `board` command and the HTTP API execute/stop endpoints.
package boardengine

import (
	"context"
	"errors"
	"fmt"
	"log/slog"
	"sort"
	"sync"

	"github.com/google/uuid"
	"github.com/gnasdev/gn-drive/internal/eventbus"
	"github.com/gnasdev/gn-drive/internal/rclone"
	"github.com/gnasdev/gn-drive/internal/store"
)

var (
	// ErrAlreadyRunning is returned when Execute is called for a board that is mid-run.
	ErrAlreadyRunning = errors.New("boardengine: board is already running")
	// ErrNotRunning is returned by Stop when no active run exists.
	ErrNotRunning = errors.New("boardengine: board is not running")
	// ErrEmptyBoard is returned when the board has no nodes/edges.
	ErrEmptyBoard = errors.New("boardengine: board has no nodes or edges")
)

// SyncExecutor is the subset of rclone.Client used to run edges.
type SyncExecutor interface {
	Sync(ctx context.Context, cfg rclone.SyncConfig, onProgress func(rclone.Stats)) (*rclone.SyncResult, error)
}

// Options configures an Engine.
type Options struct {
	Store  *store.Store
	Rclone SyncExecutor
	Bus    *eventbus.Bus
	Log    *slog.Logger
}

// Engine manages concurrent board runs (one active run per board ID).
type Engine struct {
	store  *store.Store
	rclone SyncExecutor
	bus    *eventbus.Bus
	log    *slog.Logger

	mu   sync.Mutex
	runs map[string]*run // boardID → active run
}

type run struct {
	id     string
	cancel context.CancelFunc
	done   chan struct{}
	err    error
}

// New creates a board execution engine.
func New(opts Options) *Engine {
	log := opts.Log
	if log == nil {
		log = slog.Default()
	}
	return &Engine{
		store:  opts.Store,
		rclone: opts.Rclone,
		bus:    opts.Bus,
		log:    log,
		runs:   make(map[string]*run),
	}
}

// Attach wires store + rclone after portal unlock.
func (e *Engine) Attach(st *store.Store, rc SyncExecutor) {
	e.store = st
	e.rclone = rc
}

// Detach drops store/rclone before lock re-encrypts config files.
func (e *Engine) Detach() {
	e.store = nil
	e.rclone = nil
}

// RunStatus describes an active or recently finished board run.
type RunStatus struct {
	RunID   string `json:"run_id"`
	BoardID string `json:"board_id"`
	Status  string `json:"status"` // running | completed | failed | cancelled
	Error   string `json:"error,omitempty"`
}

// Status returns the current run status for a board, if any.
func (e *Engine) Status(boardID string) (RunStatus, bool) {
	e.mu.Lock()
	defer e.mu.Unlock()
	r, ok := e.runs[boardID]
	if !ok {
		return RunStatus{}, false
	}
	select {
	case <-r.done:
		st := "completed"
		errMsg := ""
		if r.err != nil {
			if errors.Is(r.err, context.Canceled) {
				st = "cancelled"
			} else {
				st = "failed"
				errMsg = r.err.Error()
			}
		}
		return RunStatus{RunID: r.id, BoardID: boardID, Status: st, Error: errMsg}, true
	default:
		return RunStatus{RunID: r.id, BoardID: boardID, Status: "running"}, true
	}
}

// Execute starts board execution in a background goroutine.
// stopOnError stops at the first failed edge when true.
func (e *Engine) Execute(parent context.Context, boardID string, stopOnError bool) (runID string, err error) {
	if e.store == nil || e.rclone == nil {
		return "", errors.New("boardengine: not configured")
	}
	b, err := e.store.Boards().LoadGraph(parent, boardID)
	if err != nil {
		return "", fmt.Errorf("boardengine: load: %w", err)
	}
	if len(b.Nodes) == 0 || len(b.Edges) == 0 {
		return "", ErrEmptyBoard
	}

	e.mu.Lock()
	if existing, ok := e.runs[boardID]; ok {
		select {
		case <-existing.done:
			// previous finished — replace
		default:
			e.mu.Unlock()
			return "", ErrAlreadyRunning
		}
	}
	ctx, cancel := context.WithCancel(parent)
	r := &run{id: uuid.New().String(), cancel: cancel, done: make(chan struct{})}
	e.runs[boardID] = r
	e.mu.Unlock()

	e.publish(boardID, "", "", "running", "")

	go func() {
		err := e.runBoard(ctx, b, stopOnError)
		e.mu.Lock()
		r.err = err
		close(r.done)
		// Keep finished run until next Execute overwrites, so Status() still works.
		e.mu.Unlock()
		if err != nil {
			status := "failed"
			if errors.Is(err, context.Canceled) {
				status = "cancelled"
			}
			e.publish(boardID, "", "", status, err.Error())
			e.log.Error("boardengine: finished with error", "board", boardID, "err", err)
			return
		}
		e.publish(boardID, "", "", "completed", "")
		e.log.Info("boardengine: completed", "board", boardID, "run", r.id)
	}()

	return r.id, nil
}

// ExecuteSync runs the board on the calling goroutine (used by CLI).
func (e *Engine) ExecuteSync(ctx context.Context, boardID string, stopOnError bool, onProgress func(layer, idx, total int, edge store.BoardEdge, src, dst store.BoardNode, err error)) error {
	if e.store == nil || e.rclone == nil {
		return errors.New("boardengine: not configured")
	}
	b, err := e.store.Boards().LoadGraph(ctx, boardID)
	if err != nil {
		return fmt.Errorf("boardengine: load: %w", err)
	}
	if len(b.Nodes) == 0 || len(b.Edges) == 0 {
		return ErrEmptyBoard
	}
	return e.runBoardWithProgress(ctx, b, stopOnError, onProgress)
}

// Stop cancels an active board run.
func (e *Engine) Stop(boardID string) error {
	e.mu.Lock()
	defer e.mu.Unlock()
	r, ok := e.runs[boardID]
	if !ok {
		return ErrNotRunning
	}
	select {
	case <-r.done:
		return ErrNotRunning
	default:
		r.cancel()
		return nil
	}
}

func (e *Engine) runBoard(ctx context.Context, b *store.Board, stopOnError bool) error {
	return e.runBoardWithProgress(ctx, b, stopOnError, nil)
}

func (e *Engine) runBoardWithProgress(
	ctx context.Context,
	b *store.Board,
	stopOnError bool,
	onProgress func(layer, idx, total int, edge store.BoardEdge, src, dst store.BoardNode, err error),
) error {
	nodeByID := make(map[string]store.BoardNode, len(b.Nodes))
	for _, n := range b.Nodes {
		nodeByID[n.ID] = n
	}
	layers, err := TopoLayers(b.Nodes, b.Edges)
	if err != nil {
		return err
	}

	idx := 0
	total := len(b.Edges)
	var firstErr error
	for layerI, layer := range layers {
		for _, edge := range layer {
			if err := ctx.Err(); err != nil {
				return err
			}
			idx++
			src, sok := nodeByID[edge.SourceID]
			dst, dok := nodeByID[edge.TargetID]
			if !sok || !dok {
				return fmt.Errorf("edge %s references missing node (source=%s target=%s)",
					edge.ID, edge.SourceID, edge.TargetID)
			}
			e.publish(b.ID, edge.SourceID, edge.ID, "running", edge.Action)
			err := RunEdge(ctx, e.rclone, edge, src, dst)
			if onProgress != nil {
				onProgress(layerI, idx, total, edge, src, dst, err)
			}
			if err != nil {
				e.publish(b.ID, edge.SourceID, edge.ID, "failed", err.Error())
				if stopOnError {
					return fmt.Errorf("board: stopped at edge %s: %w", edge.ID, err)
				}
				if firstErr == nil {
					firstErr = err
				}
				continue
			}
			e.publish(b.ID, edge.SourceID, edge.ID, "completed", edge.Action)
		}
	}
	if firstErr != nil {
		return fmt.Errorf("board: completed with errors (first: %w)", firstErr)
	}
	return nil
}

func (e *Engine) publish(boardID, nodeID, edgeID, status, actionOrErr string) {
	if e.bus == nil {
		return
	}
	ev := eventbus.BoardExecutionEvent{
		BoardID: boardID,
		NodeID:  nodeID,
		EdgeID:  edgeID,
		Status:  status,
	}
	if status == "failed" {
		// action field reused as error detail for SSE consumers
		ev.Action = actionOrErr
	} else {
		ev.Action = actionOrErr
	}
	e.bus.Publish(eventbus.TopicBoardExecution, ev)
}

// RunEdge executes a single board edge via the sync executor.
func RunEdge(ctx context.Context, exec SyncExecutor, edge store.BoardEdge, src, dst store.BoardNode) error {
	source := nodePath(src)
	dest := nodePath(dst)
	action := rclone.Action(edge.Action)
	if action == "" {
		action = rclone.ActionPush
	}
	cfg := rclone.SyncConfig{
		Action: action,
		Source: source,
		Dest:   dest,
		Profile: &rclone.ProfileFlags{
			Transfers: 4,
		},
	}
	_, err := exec.Sync(ctx, cfg, nil)
	return err
}

func nodePath(n store.BoardNode) string {
	if n.RemoteName != "" {
		if n.Path != "" && n.Path != "/" {
			return n.RemoteName + ":" + n.Path
		}
		return n.RemoteName + ":"
	}
	return n.Path
}

// TopoLayers returns edges grouped by topological layer.
// A cycle produces an error.
func TopoLayers(nodes []store.BoardNode, edges []store.BoardEdge) ([][]store.BoardEdge, error) {
	indeg := make(map[string]int, len(nodes))
	for _, n := range nodes {
		indeg[n.ID] = 0
	}
	for _, e := range edges {
		indeg[e.TargetID]++
	}

	pending := make([]bool, len(edges))
	for i := range pending {
		pending[i] = true
	}

	var layers [][]store.BoardEdge
	processed := 0
	for processed < len(edges) {
		var layer []store.BoardEdge
		for i, e := range edges {
			if !pending[i] {
				continue
			}
			if indeg[e.SourceID] == 0 {
				layer = append(layer, e)
			}
		}
		if len(layer) == 0 {
			return nil, fmt.Errorf("cycle detected: %d edges could not be ordered", len(edges)-processed)
		}
		sort.SliceStable(layer, func(i, j int) bool { return layer[i].ID < layer[j].ID })
		for _, e := range layer {
			for i, x := range edges {
				if pending[i] && x.ID == e.ID {
					pending[i] = false
					break
				}
			}
			indeg[e.TargetID]--
		}
		layers = append(layers, layer)
		processed += len(layer)
	}
	return layers, nil
}
