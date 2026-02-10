package services

import (
	"context"
	"desktop/backend/events"
	"desktop/backend/models"
	"desktop/backend/rclone"
	"encoding/json"
	"fmt"
	"log"
	"sync"
	"time"

	"github.com/wailsapp/wails/v3/pkg/application"
)

// BoardService manages board flow definitions and orchestrates execution
type BoardService struct {
	app         *application.App
	eventBus    *events.WailsEventBus
	boards      []models.Board
	mutex       sync.RWMutex
	initialized bool

	// Dependencies
	syncService         *SyncService
	notificationService *NotificationService

	// Active executions
	activeFlows map[string]*FlowExecution
	flowMutex   sync.RWMutex
}

// Singleton instance for cross-service access
var boardServiceInstance *BoardService
var boardServiceOnce sync.Once

// GetBoardService returns the singleton BoardService instance
func GetBoardService() *BoardService {
	return boardServiceInstance
}

// SetBoardServiceInstance sets the singleton instance (called from main.go)
func SetBoardServiceInstance(bs *BoardService) {
	boardServiceOnce.Do(func() {
		boardServiceInstance = bs
	})
}

// FlowExecution tracks a running board execution
type FlowExecution struct {
	BoardId      string
	Cancel       context.CancelFunc
	Status       *models.BoardExecutionStatus
	StatusMu     sync.Mutex  // protects Status field from concurrent access
	CleanupTimer *time.Timer // delayed cleanup timer; nil while running
}

// NewBoardService creates a new board service
func NewBoardService(app *application.App) *BoardService {
	return &BoardService{
		app:         app,
		boards:      []models.Board{},
		activeFlows: make(map[string]*FlowExecution),
	}
}

// GetExecutionLogs returns board execution logs (polling workaround for Wails v3 event issues)
func (b *BoardService) GetExecutionLogs(ctx context.Context) []string {
	return GetBoardLogs()
}

// ClearExecutionLogs clears the board execution log buffer
func (b *BoardService) ClearExecutionLogs(ctx context.Context) {
	ClearBoardLogs()
}

// SetApp sets the application reference for events
func (b *BoardService) SetApp(app *application.App) {
	b.app = app
	if bus := GetSharedEventBus(); bus != nil {
		b.eventBus = bus
	} else {
		b.eventBus = events.NewEventBus(app)
	}
}

// SetSyncService sets the sync service dependency
func (b *BoardService) SetSyncService(syncService *SyncService) {
	b.syncService = syncService
}

// SetNotificationService sets the notification service for desktop notifications
func (b *BoardService) SetNotificationService(notificationService *NotificationService) {
	b.notificationService = notificationService
}

// ServiceName returns the name of the service
func (b *BoardService) ServiceName() string {
	return "BoardService"
}

// ServiceStartup is called when the service starts
func (b *BoardService) ServiceStartup(ctx context.Context, options application.ServiceOptions) error {
	log.Printf("BoardService starting up (async)...")
	go func() {
		if err := b.initialize(); err != nil {
			log.Printf("BoardService init error: %v", err)
		}
	}()
	return nil
}

// ServiceShutdown is called when the service shuts down
func (b *BoardService) ServiceShutdown(ctx context.Context) error {
	log.Printf("BoardService shutting down...")
	// Cancel all active flows and stop cleanup timers
	b.flowMutex.Lock()
	for _, flow := range b.activeFlows {
		if flow.Cancel != nil {
			flow.Cancel()
		}
		if flow.CleanupTimer != nil {
			flow.CleanupTimer.Stop()
		}
	}
	b.flowMutex.Unlock()
	return nil
}

// ensureInitialized lazily initializes the service if not yet done
func (b *BoardService) ensureInitialized() error {
	b.mutex.RLock()
	if b.initialized {
		b.mutex.RUnlock()
		return nil
	}
	b.mutex.RUnlock()
	return b.initialize()
}

// initialize loads existing boards from SQLite.
// Returns error if DB is not available (e.g. auth enabled, files encrypted).
// In that case, initialized stays false so ensureInitialized() retries later.
func (b *BoardService) initialize() error {
	b.mutex.Lock()
	defer b.mutex.Unlock()

	if b.initialized {
		return nil
	}

	boards, err := b.loadBoardsFromDB()
	if err != nil {
		// Don't set initialized - ensureInitialized() will retry on next access
		return fmt.Errorf("could not load boards: %w", err)
	}
	b.boards = boards

	// Auto-migrate from profiles if no boards exist
	if len(b.boards) == 0 {
		b.migrateFromProfiles()
	}

	b.initialized = true
	log.Printf("BoardService initialized with %d boards", len(b.boards))
	return nil
}

// GetBoards returns all boards
func (b *BoardService) GetBoards(ctx context.Context) ([]models.Board, error) {
	if err := b.ensureInitialized(); err != nil {
		return nil, err
	}
	b.mutex.RLock()
	defer b.mutex.RUnlock()

	result := make([]models.Board, len(b.boards))
	copy(result, b.boards)
	return result, nil
}

// GetBoard returns a single board by ID
func (b *BoardService) GetBoard(ctx context.Context, boardId string) (*models.Board, error) {
	if err := b.ensureInitialized(); err != nil {
		return nil, err
	}
	b.mutex.RLock()
	defer b.mutex.RUnlock()

	for i := range b.boards {
		if b.boards[i].Id == boardId {
			board := b.boards[i]
			return &board, nil
		}
	}
	return nil, fmt.Errorf("board '%s' not found", boardId)
}

// AddBoard creates a new board
func (b *BoardService) AddBoard(ctx context.Context, board models.Board) error {
	log.Printf("[BoardService] AddBoard called: id=%s name=%s nodes=%d edges=%d", board.Id, board.Name, len(board.Nodes), len(board.Edges))

	if err := b.ensureInitialized(); err != nil {
		log.Printf("[BoardService] AddBoard: ensureInitialized failed: %v", err)
		return err
	}
	b.mutex.Lock()
	defer b.mutex.Unlock()

	// Validate
	if err := b.validateBoard(&board); err != nil {
		log.Printf("[BoardService] AddBoard: validation failed: %v", err)
		return fmt.Errorf("invalid board: %w", err)
	}

	// Check name uniqueness
	for _, existing := range b.boards {
		if existing.Name == board.Name {
			log.Printf("[BoardService] AddBoard: name conflict: '%s'", board.Name)
			return fmt.Errorf("board with name '%s' already exists", board.Name)
		}
	}

	if board.CreatedAt.IsZero() {
		board.CreatedAt = time.Now()
	}
	board.UpdatedAt = time.Now()

	// Save to database
	log.Printf("[BoardService] AddBoard: saving to DB...")
	if err := b.saveBoardToDB(board); err != nil {
		log.Printf("[BoardService] AddBoard: saveBoardToDB failed: %v", err)
		return fmt.Errorf("failed to save board: %w", err)
	}
	log.Printf("[BoardService] AddBoard: saved to DB successfully")

	b.boards = append(b.boards, board)

	b.emitBoardEvent(events.BoardUpdated, board.Id, "", "added", "Board created")
	log.Printf("[BoardService] AddBoard: completed successfully for board %s", board.Id)

	// Refresh system tray menu
	if ts := GetTrayService(); ts != nil {
		go ts.RefreshMenu()
	}

	return nil
}

// UpdateBoard updates an existing board
func (b *BoardService) UpdateBoard(ctx context.Context, board models.Board) error {
	if err := b.ensureInitialized(); err != nil {
		return err
	}
	b.mutex.Lock()
	defer b.mutex.Unlock()

	// Validate
	if err := b.validateBoard(&board); err != nil {
		return fmt.Errorf("invalid board: %w", err)
	}

	found := false
	var oldBoard models.Board
	for i, existing := range b.boards {
		if existing.Id == board.Id {
			oldBoard = existing
			board.UpdatedAt = time.Now()
			board.CreatedAt = existing.CreatedAt
			b.boards[i] = board
			found = true
			break
		}
	}

	if !found {
		return fmt.Errorf("board '%s' not found", board.Id)
	}

	if err := b.saveBoardToDB(board); err != nil {
		// Rollback
		for i, existing := range b.boards {
			if existing.Id == board.Id {
				b.boards[i] = oldBoard
				break
			}
		}
		return fmt.Errorf("failed to save board: %w", err)
	}

	b.emitBoardEvent(events.BoardUpdated, board.Id, "", "updated", "Board updated")

	// Refresh system tray menu
	if ts := GetTrayService(); ts != nil {
		go ts.RefreshMenu()
	}

	return nil
}

// DeleteBoard removes a board
func (b *BoardService) DeleteBoard(ctx context.Context, boardId string) error {
	if err := b.ensureInitialized(); err != nil {
		return err
	}
	b.mutex.Lock()
	defer b.mutex.Unlock()

	found := false
	var deletedBoard models.Board
	for i, board := range b.boards {
		if board.Id == boardId {
			deletedBoard = board
			b.boards = append(b.boards[:i], b.boards[i+1:]...)
			found = true
			break
		}
	}

	if !found {
		return fmt.Errorf("board '%s' not found", boardId)
	}

	if err := b.deleteBoardFromDB(boardId); err != nil {
		b.boards = append(b.boards, deletedBoard)
		return fmt.Errorf("failed to delete board: %w", err)
	}

	b.emitBoardEvent(events.BoardUpdated, boardId, "", "deleted", "Board deleted")

	// Refresh system tray menu
	if ts := GetTrayService(); ts != nil {
		go ts.RefreshMenu()
	}

	return nil
}

// ExecuteBoard starts executing a board flow
func (b *BoardService) ExecuteBoard(ctx context.Context, boardId string) (*models.BoardExecutionStatus, error) {
	log.Printf("[BoardService] ExecuteBoard called: boardId=%s", boardId)

	if err := b.ensureInitialized(); err != nil {
		log.Printf("[BoardService] ExecuteBoard: ensureInitialized failed: %v", err)
		return nil, err
	}

	if b.syncService == nil {
		log.Printf("[BoardService] ExecuteBoard: sync service not available")
		return nil, fmt.Errorf("sync service not available")
	}

	// Check if board is already executing
	b.flowMutex.Lock()
	if existing, exists := b.activeFlows[boardId]; exists {
		existing.StatusMu.Lock()
		status := existing.Status.Status
		existing.StatusMu.Unlock()
		if status == "running" {
			b.flowMutex.Unlock()
			return nil, fmt.Errorf("board '%s' is already executing", boardId)
		}
		// Terminal state (completed/failed/cancelled): stop cleanup timer, remove stale entry
		if existing.CleanupTimer != nil {
			existing.CleanupTimer.Stop()
		}
		delete(b.activeFlows, boardId)
	}
	b.flowMutex.Unlock()

	// Get board
	b.mutex.RLock()
	var board *models.Board
	for i := range b.boards {
		if b.boards[i].Id == boardId {
			boardCopy := b.boards[i]
			board = &boardCopy
			break
		}
	}
	b.mutex.RUnlock()

	if board == nil {
		log.Printf("[BoardService] ExecuteBoard: board '%s' not found", boardId)
		return nil, fmt.Errorf("board '%s' not found", boardId)
	}

	log.Printf("[BoardService] ExecuteBoard: found board '%s' with %d nodes, %d edges", board.Name, len(board.Nodes), len(board.Edges))

	if len(board.Edges) == 0 {
		return nil, fmt.Errorf("board '%s' has no edges to execute", board.Name)
	}

	// Validate DAG (cycle detection)
	if err := b.detectCycles(board); err != nil {
		log.Printf("[BoardService] ExecuteBoard: cycle detection failed: %v", err)
		return nil, err
	}

	// Compute execution layers
	layers := b.computeExecutionLayers(board)
	log.Printf("[BoardService] ExecuteBoard: computed %d execution layers", len(layers))

	// Initialize execution status
	edgeStatuses := make([]models.EdgeExecutionStatus, len(board.Edges))
	for i, edge := range board.Edges {
		edgeStatuses[i] = models.EdgeExecutionStatus{
			EdgeId: edge.Id,
			Status: "pending",
		}
	}

	status := &models.BoardExecutionStatus{
		BoardId:      boardId,
		Status:       "running",
		EdgeStatuses: edgeStatuses,
		StartTime:    time.Now(),
	}

	// Create cancellable context from Background (not from the Wails RPC context,
	// which gets cancelled when the method call returns)
	flowCtx, cancel := context.WithCancel(context.Background())

	flow := &FlowExecution{
		BoardId: boardId,
		Cancel:  cancel,
		Status:  status,
	}

	b.flowMutex.Lock()
	b.activeFlows[boardId] = flow
	b.flowMutex.Unlock()

	b.emitBoardEvent(events.BoardExecutionStarted, boardId, "", "running", "Board execution started")

	// Execute in goroutine
	go b.executeFlow(flowCtx, board, layers, flow)

	return status, nil
}

// StopBoardExecution cancels a running board execution
func (b *BoardService) StopBoardExecution(ctx context.Context, boardId string) error {
	b.flowMutex.Lock()
	flow, exists := b.activeFlows[boardId]
	if !exists {
		b.flowMutex.Unlock()
		// Execution already finished — not an error
		return nil
	}

	flow.Cancel()
	flow.StatusMu.Lock()
	flow.Status.Status = "cancelled"
	flow.StatusMu.Unlock()
	b.flowMutex.Unlock()

	b.emitBoardEvent(events.BoardExecutionCancelled, boardId, "", "cancelled", "Board execution cancelled")
	return nil
}

// GetBoardExecutionStatus returns the current execution status of a board
func (b *BoardService) GetBoardExecutionStatus(ctx context.Context, boardId string) (*models.BoardExecutionStatus, error) {
	b.flowMutex.RLock()
	flow, exists := b.activeFlows[boardId]
	b.flowMutex.RUnlock()

	if !exists {
		return nil, fmt.Errorf("no active execution for board '%s'", boardId)
	}

	flow.StatusMu.Lock()
	status := *flow.Status // return a copy
	flow.StatusMu.Unlock()
	return &status, nil
}

// executeFlow runs the board execution through layers
func (b *BoardService) executeFlow(ctx context.Context, board *models.Board, layers [][]models.BoardEdge, flow *FlowExecution) {
	log.Printf("[BoardService] executeFlow started: boardId=%s layers=%d totalEdges=%d", board.Id, len(layers), len(board.Edges))
	defer func() {
		endTime := time.Now()
		flow.StatusMu.Lock()
		flow.Status.EndTime = &endTime
		flow.StatusMu.Unlock()
		log.Printf("[BoardService] executeFlow finished: boardId=%s finalStatus=%s", board.Id, flow.Status.Status)
		// Delay cleanup to give frontend polling time to catch the terminal status.
		// The flow stays in activeFlows with its final status for a grace period.
		// Store the timer so ExecuteBoard can cancel it for immediate re-execution.
		flow.CleanupTimer = time.AfterFunc(10*time.Second, func() {
			b.flowMutex.Lock()
			delete(b.activeFlows, board.Id)
			b.flowMutex.Unlock()
			log.Printf("[BoardService] executeFlow cleaned up: boardId=%s", board.Id)
		})
	}()

	// Track failed nodes to skip downstream
	failedNodes := make(map[string]bool)

	for _, layer := range layers {
		// Check if context is cancelled
		select {
		case <-ctx.Done():
			flow.StatusMu.Lock()
			flow.Status.Status = "cancelled"
			b.markRemainingSkipped(flow.Status, layer)
			flow.StatusMu.Unlock()
			b.emitBoardEvent(events.BoardExecutionCancelled, board.Id, "", "cancelled", "Board execution cancelled")
			return
		default:
		}

		// Pass 1: Sequential skip check (safe read of failedNodes, no concurrent writers yet)
		var edgesToRun []models.BoardEdge
		for _, edge := range layer {
			if failedNodes[edge.SourceId] {
				flow.StatusMu.Lock()
				b.updateEdgeStatus(flow.Status, edge.Id, "skipped", "Skipped: upstream edge failed")
				flow.StatusMu.Unlock()
				failedNodes[edge.TargetId] = true
				b.emitBoardEvent(events.BoardExecutionProgress, board.Id, edge.Id, "skipped", "Skipped: upstream edge failed")
				continue
			}
			edgesToRun = append(edgesToRun, edge)
		}

		// Pass 2: Parallel execution (writes to failedNodes are protected by layerMu)
		var wg sync.WaitGroup
		var layerMu sync.Mutex
		layerHasFailure := false

		for _, edge := range edgesToRun {
			wg.Add(1)
			go func(e models.BoardEdge) {
				defer wg.Done()

				err := b.executeEdge(ctx, board, &e, flow)

				if err != nil {
					layerMu.Lock()
					layerHasFailure = true
					failedNodes[e.TargetId] = true
					layerMu.Unlock()
				}
			}(edge)
		}

		// After Wait(), failedNodes is safe to read in the next layer iteration
		wg.Wait()

		if layerHasFailure {
			// Mark all downstream edges as skipped
			flow.StatusMu.Lock()
			b.markDownstreamSkipped(flow.Status, failedNodes, layers)
			flow.StatusMu.Unlock()
		}
	}

	// Determine final status
	flow.StatusMu.Lock()
	hasFailure := false
	for _, es := range flow.Status.EdgeStatuses {
		if es.Status == "failed" {
			hasFailure = true
			break
		}
	}

	if hasFailure {
		flow.Status.Status = "failed"
	} else {
		flow.Status.Status = "completed"
	}
	flow.StatusMu.Unlock()

	if hasFailure {
		b.emitBoardEvent(events.BoardExecutionFailed, board.Id, "", "failed", "Board execution completed with failures")
		b.sendBoardNotification(board, false, flow.Status)
	} else {
		b.emitBoardEvent(events.BoardExecutionCompleted, board.Id, "", "completed", "Board execution completed successfully")
		b.sendBoardNotification(board, true, flow.Status)
	}
}

// executeEdge executes a single edge sync operation
func (b *BoardService) executeEdge(ctx context.Context, board *models.Board, edge *models.BoardEdge, flow *FlowExecution) error {
	// Find source and target nodes
	var sourceNode, targetNode *models.BoardNode
	for i := range board.Nodes {
		if board.Nodes[i].Id == edge.SourceId {
			sourceNode = &board.Nodes[i]
		}
		if board.Nodes[i].Id == edge.TargetId {
			targetNode = &board.Nodes[i]
		}
	}

	if sourceNode == nil || targetNode == nil {
		msg := "source or target node not found"
		flow.StatusMu.Lock()
		b.updateEdgeStatus(flow.Status, edge.Id, "failed", msg)
		flow.StatusMu.Unlock()
		b.emitBoardEvent(events.BoardExecutionProgress, board.Id, edge.Id, "failed", msg)
		return fmt.Errorf("%s", msg)
	}

	// Build profile from edge config with From/To from nodes
	profile := edge.SyncConfig
	profile.From = b.buildRemotePath(sourceNode)
	profile.To = b.buildRemotePath(targetNode)
	if profile.Name == "" {
		profile.Name = fmt.Sprintf("%s->%s", sourceNode.Label, targetNode.Label)
	}

	log.Printf("[BoardService] executeEdge: action=%s from=%s to=%s", edge.Action, profile.From, profile.To)

	// Mark edge as running
	startTime := time.Now()
	flow.StatusMu.Lock()
	b.updateEdgeStatusWithTime(flow.Status, edge.Id, "running", "", &startTime, nil)
	flow.StatusMu.Unlock()
	b.emitBoardEvent(events.BoardExecutionProgress, board.Id, edge.Id, "running", fmt.Sprintf("Syncing %s -> %s", sourceNode.Label, targetNode.Label))

	// Clear caches before sync to ensure fresh state
	rclone.ClearFsCache()
	rclone.ClearStatsCache()

	// Start sync via SyncService
	// Use IDs directly since they already have "board-" and "edge-" prefixes
	tabId := fmt.Sprintf("%s-%s", board.Id, edge.Id)
	log.Printf("[BoardService] executeEdge: starting sync with tabId=%s profile=%+v", tabId, profile)
	result, err := b.syncService.StartSync(ctx, edge.Action, profile, tabId)
	log.Printf("[BoardService] executeEdge: StartSync returned: result=%+v err=%v", result, err)
	if err != nil {
		msg := fmt.Sprintf("Failed to start sync: %v", err)
		endTime := time.Now()
		flow.StatusMu.Lock()
		b.updateEdgeStatusWithTime(flow.Status, edge.Id, "failed", msg, nil, &endTime)
		flow.StatusMu.Unlock()
		b.emitBoardEvent(events.BoardExecutionProgress, board.Id, edge.Id, "failed", msg)
		return err
	}

	// Wait for task completion
	log.Printf("[BoardService] executeEdge: waiting for task %d to complete", result.TaskId)
	err = b.syncService.WaitForTask(ctx, result.TaskId)
	log.Printf("[BoardService] executeEdge: WaitForTask returned: err=%v", err)

	endTime := time.Now()
	if err != nil {
		msg := fmt.Sprintf("Sync failed: %v", err)
		log.Printf("[BoardService] executeEdge: sync failed: %s", msg)
		flow.StatusMu.Lock()
		b.updateEdgeStatusWithTime(flow.Status, edge.Id, "failed", msg, nil, &endTime)
		flow.StatusMu.Unlock()
		b.emitBoardEvent(events.BoardExecutionProgress, board.Id, edge.Id, "failed", msg)
		return err
	}

	log.Printf("[BoardService] executeEdge: sync completed successfully for edge %s", edge.Id)
	flow.StatusMu.Lock()
	b.updateEdgeStatusWithTime(flow.Status, edge.Id, "completed", "Sync completed", nil, &endTime)
	flow.StatusMu.Unlock()
	b.emitBoardEvent(events.BoardExecutionProgress, board.Id, edge.Id, "completed", "Sync completed")
	return nil
}

// buildRemotePath constructs rclone path from a board node
func (b *BoardService) buildRemotePath(node *models.BoardNode) string {
	if node.RemoteName == "local" || node.RemoteName == "" {
		return node.Path
	}
	if node.Path == "" {
		return node.RemoteName + ":"
	}
	return node.RemoteName + ":" + node.Path
}

// computeExecutionLayers groups edges into execution layers using topological sort
// Edges in the same layer can run in parallel; layers execute sequentially
func (b *BoardService) computeExecutionLayers(board *models.Board) [][]models.BoardEdge {
	// Build node dependency map: nodeId -> set of edges that must complete before edges from this node can start
	// An edge E2 (B->C) depends on all edges E1 where E1.TargetId == E2.SourceId

	// Map: nodeId -> incoming edges (edges targeting this node)
	incomingEdges := make(map[string][]string) // nodeId -> []edgeId
	edgeMap := make(map[string]models.BoardEdge)

	for _, edge := range board.Edges {
		edgeMap[edge.Id] = edge
		incomingEdges[edge.TargetId] = append(incomingEdges[edge.TargetId], edge.Id)
	}

	// Compute edge dependencies: edgeId -> set of edge IDs that must complete first
	edgeDeps := make(map[string]map[string]bool)
	for _, edge := range board.Edges {
		deps := make(map[string]bool)
		// This edge depends on all edges whose target is this edge's source
		for _, depEdgeId := range incomingEdges[edge.SourceId] {
			deps[depEdgeId] = true
		}
		edgeDeps[edge.Id] = deps
	}

	// Kahn's algorithm on edges
	var layers [][]models.BoardEdge
	resolved := make(map[string]bool)
	remaining := make(map[string]bool)

	for _, edge := range board.Edges {
		remaining[edge.Id] = true
	}

	for len(remaining) > 0 {
		// Find edges with all dependencies resolved
		var layer []models.BoardEdge
		for edgeId := range remaining {
			allResolved := true
			for depId := range edgeDeps[edgeId] {
				if !resolved[depId] {
					allResolved = false
					break
				}
			}
			if allResolved {
				layer = append(layer, edgeMap[edgeId])
			}
		}

		if len(layer) == 0 {
			// Should not happen if DAG validation passed, but break to avoid infinite loop
			log.Printf("Warning: could not resolve remaining edges, possible cycle")
			break
		}

		// Mark layer edges as resolved
		for _, edge := range layer {
			resolved[edge.Id] = true
			delete(remaining, edge.Id)
		}

		layers = append(layers, layer)
	}

	return layers
}

// detectCycles checks if the board graph contains cycles using DFS
func (b *BoardService) detectCycles(board *models.Board) error {
	// Build adjacency list: nodeId -> []nodeId
	adj := make(map[string][]string)
	for _, edge := range board.Edges {
		adj[edge.SourceId] = append(adj[edge.SourceId], edge.TargetId)
	}

	const (
		white = 0 // unvisited
		gray  = 1 // in current path
		black = 2 // fully processed
	)

	colors := make(map[string]int)
	for _, node := range board.Nodes {
		colors[node.Id] = white
	}

	var dfs func(nodeId string) bool
	dfs = func(nodeId string) bool {
		colors[nodeId] = gray
		for _, neighbor := range adj[nodeId] {
			if colors[neighbor] == gray {
				return true // cycle detected
			}
			if colors[neighbor] == white {
				if dfs(neighbor) {
					return true
				}
			}
		}
		colors[nodeId] = black
		return false
	}

	for _, node := range board.Nodes {
		if colors[node.Id] == white {
			if dfs(node.Id) {
				return fmt.Errorf("board '%s' contains a cycle, which is not allowed", board.Name)
			}
		}
	}

	return nil
}

// validateBoard performs basic validation on a board
func (b *BoardService) validateBoard(board *models.Board) error {
	if board.Id == "" {
		return fmt.Errorf("board ID is required")
	}
	if board.Name == "" {
		return fmt.Errorf("board name is required")
	}

	// Build node ID set
	nodeIds := make(map[string]bool)
	for _, node := range board.Nodes {
		if node.Id == "" {
			return fmt.Errorf("node ID is required")
		}
		if nodeIds[node.Id] {
			return fmt.Errorf("duplicate node ID: %s", node.Id)
		}
		nodeIds[node.Id] = true
	}

	// Validate edges reference valid nodes
	edgeIds := make(map[string]bool)
	for _, edge := range board.Edges {
		if edge.Id == "" {
			return fmt.Errorf("edge ID is required")
		}
		if edgeIds[edge.Id] {
			return fmt.Errorf("duplicate edge ID: %s", edge.Id)
		}
		edgeIds[edge.Id] = true

		if !nodeIds[edge.SourceId] {
			return fmt.Errorf("edge '%s' references unknown source node '%s'", edge.Id, edge.SourceId)
		}
		if !nodeIds[edge.TargetId] {
			return fmt.Errorf("edge '%s' references unknown target node '%s'", edge.Id, edge.TargetId)
		}
		if edge.SourceId == edge.TargetId {
			return fmt.Errorf("edge '%s' cannot connect a node to itself", edge.Id)
		}

		// Validate action
		switch edge.Action {
		case "pull", "push", "bi", "bi-resync":
			// valid
		default:
			return fmt.Errorf("edge '%s' has invalid action '%s'", edge.Id, edge.Action)
		}
	}

	// Check for cycles
	return b.detectCycles(board)
}

// updateEdgeStatus updates the status of an edge in the execution status
func (b *BoardService) updateEdgeStatus(status *models.BoardExecutionStatus, edgeId, newStatus, message string) {
	for i := range status.EdgeStatuses {
		if status.EdgeStatuses[i].EdgeId == edgeId {
			status.EdgeStatuses[i].Status = newStatus
			status.EdgeStatuses[i].Message = message
			return
		}
	}
}

// updateEdgeStatusWithTime updates edge status with optional start/end times
func (b *BoardService) updateEdgeStatusWithTime(status *models.BoardExecutionStatus, edgeId, newStatus, message string, startTime, endTime *time.Time) {
	for i := range status.EdgeStatuses {
		if status.EdgeStatuses[i].EdgeId == edgeId {
			status.EdgeStatuses[i].Status = newStatus
			status.EdgeStatuses[i].Message = message
			if startTime != nil {
				status.EdgeStatuses[i].StartTime = startTime
			}
			if endTime != nil {
				status.EdgeStatuses[i].EndTime = endTime
			}
			return
		}
	}
}

// markRemainingSkipped marks all pending edges as skipped
func (b *BoardService) markRemainingSkipped(status *models.BoardExecutionStatus, currentLayer []models.BoardEdge) {
	for i := range status.EdgeStatuses {
		if status.EdgeStatuses[i].Status == "pending" {
			status.EdgeStatuses[i].Status = "skipped"
			status.EdgeStatuses[i].Message = "Skipped: execution cancelled"
		}
	}
}

// markDownstreamSkipped marks edges downstream of failed nodes as skipped
func (b *BoardService) markDownstreamSkipped(status *models.BoardExecutionStatus, failedNodes map[string]bool, layers [][]models.BoardEdge) {
	for _, layer := range layers {
		for _, edge := range layer {
			if failedNodes[edge.SourceId] {
				b.updateEdgeStatus(status, edge.Id, "skipped", "Skipped: upstream edge failed")
				failedNodes[edge.TargetId] = true
			}
		}
	}
}

// migrateFromProfiles auto-migrates existing profiles to boards
func (b *BoardService) migrateFromProfiles() {
	db, err := GetSharedDB()
	if err != nil {
		return
	}

	// Read profiles from the database
	rows, err := db.Query("SELECT name, from_path, to_path FROM profiles")
	if err != nil {
		return
	}
	defer rows.Close()

	type simpleProfile struct {
		Name string
		From string
		To   string
	}
	var profiles []simpleProfile
	for rows.Next() {
		var p simpleProfile
		if err := rows.Scan(&p.Name, &p.From, &p.To); err != nil {
			continue
		}
		profiles = append(profiles, p)
	}

	if len(profiles) == 0 {
		return
	}

	now := time.Now()
	for i, profile := range profiles {
		boardId := fmt.Sprintf("migrated-%d-%d", i, now.UnixMilli())

		// Parse source remote
		sourceNode := models.BoardNode{
			Id:         fmt.Sprintf("node-src-%d", i),
			RemoteName: parseRemoteName(profile.From),
			Path:       parseRemotePath(profile.From),
			Label:      parseRemoteName(profile.From),
			X:          100,
			Y:          100 + float64(i)*150,
		}
		if sourceNode.RemoteName == "" {
			sourceNode.RemoteName = "local"
			sourceNode.Label = "Local"
		}

		// Parse target remote
		targetNode := models.BoardNode{
			Id:         fmt.Sprintf("node-tgt-%d", i),
			RemoteName: parseRemoteName(profile.To),
			Path:       parseRemotePath(profile.To),
			Label:      parseRemoteName(profile.To),
			X:          500,
			Y:          100 + float64(i)*150,
		}
		if targetNode.RemoteName == "" {
			targetNode.RemoteName = "local"
			targetNode.Label = "Local"
		}

		edge := models.BoardEdge{
			Id:       fmt.Sprintf("edge-%d", i),
			SourceId: sourceNode.Id,
			TargetId: targetNode.Id,
			Action:   "push",
		}

		board := models.Board{
			Id:        boardId,
			Name:      profile.Name,
			Nodes:     []models.BoardNode{sourceNode, targetNode},
			Edges:     []models.BoardEdge{edge},
			CreatedAt: now,
			UpdatedAt: now,
		}

		b.boards = append(b.boards, board)
	}

	// Save all migrated boards to DB
	for _, board := range b.boards {
		if err := b.saveBoardToDB(board); err != nil {
			log.Printf("Warning: Failed to save migrated board '%s': %v", board.Name, err)
		}
	}
	log.Printf("Migrated %d profiles to boards", len(profiles))
}

// parseRemoteName extracts the remote name from an rclone path (e.g., "gdrive:/path" -> "gdrive")
func parseRemoteName(path string) string {
	for i, c := range path {
		if c == ':' {
			return path[:i]
		}
		if c == '/' {
			return "" // local path
		}
	}
	return ""
}

// parseRemotePath extracts the path portion from an rclone path (e.g., "gdrive:/path" -> "/path")
func parseRemotePath(path string) string {
	for i, c := range path {
		if c == ':' {
			if i+1 < len(path) {
				return path[i+1:]
			}
			return ""
		}
		if c == '/' {
			return path // entire string is a local path
		}
	}
	return path
}

// ============ SQLite Persistence ============

// loadBoardsFromDB loads all boards with their nodes and edges from SQLite
func (b *BoardService) loadBoardsFromDB() ([]models.Board, error) {
	db, err := GetSharedDB()
	if err != nil {
		return nil, err
	}

	rows, err := db.Query(`SELECT id, name, description, created_at, updated_at,
		schedule_enabled, cron_expr, last_run, next_run, last_result
		FROM boards ORDER BY name`)
	if err != nil {
		return nil, fmt.Errorf("failed to query boards: %w", err)
	}
	defer rows.Close()

	var boards []models.Board
	for rows.Next() {
		var board models.Board
		var createdAt, updatedAt string
		var scheduleEnabled int
		var lastRun, nextRun *string
		if err := rows.Scan(&board.Id, &board.Name, &board.Description, &createdAt, &updatedAt,
			&scheduleEnabled, &board.CronExpr, &lastRun, &nextRun, &board.LastResult); err != nil {
			return nil, fmt.Errorf("failed to scan board: %w", err)
		}
		board.ScheduleEnabled = scheduleEnabled != 0
		if t, err := time.Parse(time.RFC3339, createdAt); err == nil {
			board.CreatedAt = t
		}
		if t, err := time.Parse(time.RFC3339, updatedAt); err == nil {
			board.UpdatedAt = t
		}
		if lastRun != nil {
			if t, err := time.Parse(time.RFC3339, *lastRun); err == nil {
				board.LastRun = &t
			}
		}
		if nextRun != nil {
			if t, err := time.Parse(time.RFC3339, *nextRun); err == nil {
				board.NextRun = &t
			}
		}
		board.Nodes = []models.BoardNode{}
		board.Edges = []models.BoardEdge{}
		boards = append(boards, board)
	}
	if err := rows.Err(); err != nil {
		return nil, err
	}

	// Load nodes and edges for each board
	for i := range boards {
		nodes, err := b.loadBoardNodesFromDB(boards[i].Id)
		if err != nil {
			return nil, err
		}
		boards[i].Nodes = nodes

		edges, err := b.loadBoardEdgesFromDB(boards[i].Id)
		if err != nil {
			return nil, err
		}
		boards[i].Edges = edges
	}

	if boards == nil {
		boards = []models.Board{}
	}
	return boards, nil
}

func (b *BoardService) loadBoardNodesFromDB(boardId string) ([]models.BoardNode, error) {
	db, err := GetSharedDB()
	if err != nil {
		return nil, err
	}

	rows, err := db.Query("SELECT id, remote_name, path, label, x, y FROM board_nodes WHERE board_id = ?", boardId)
	if err != nil {
		return nil, fmt.Errorf("failed to query board nodes: %w", err)
	}
	defer rows.Close()

	var nodes []models.BoardNode
	for rows.Next() {
		var node models.BoardNode
		if err := rows.Scan(&node.Id, &node.RemoteName, &node.Path, &node.Label, &node.X, &node.Y); err != nil {
			return nil, fmt.Errorf("failed to scan board node: %w", err)
		}
		nodes = append(nodes, node)
	}
	if nodes == nil {
		nodes = []models.BoardNode{}
	}
	return nodes, rows.Err()
}

func (b *BoardService) loadBoardEdgesFromDB(boardId string) ([]models.BoardEdge, error) {
	db, err := GetSharedDB()
	if err != nil {
		return nil, err
	}

	rows, err := db.Query("SELECT id, source_id, target_id, action, sync_config FROM board_edges WHERE board_id = ?", boardId)
	if err != nil {
		return nil, fmt.Errorf("failed to query board edges: %w", err)
	}
	defer rows.Close()

	var edges []models.BoardEdge
	for rows.Next() {
		var edge models.BoardEdge
		var syncConfigJSON string
		if err := rows.Scan(&edge.Id, &edge.SourceId, &edge.TargetId, &edge.Action, &syncConfigJSON); err != nil {
			return nil, fmt.Errorf("failed to scan board edge: %w", err)
		}
		if syncConfigJSON != "" && syncConfigJSON != "{}" {
			if jsonErr := json.Unmarshal([]byte(syncConfigJSON), &edge.SyncConfig); jsonErr != nil {
				log.Printf("[BoardService] Warning: failed to unmarshal sync_config for edge %s: %v", edge.Id, jsonErr)
			}
		}
		edges = append(edges, edge)
	}
	if edges == nil {
		edges = []models.BoardEdge{}
	}
	return edges, rows.Err()
}

// saveBoardToDB saves a board with all its nodes and edges to the database
func (b *BoardService) saveBoardToDB(board models.Board) error {
	db, err := GetSharedDB()
	if err != nil {
		return err
	}

	tx, err := db.Begin()
	if err != nil {
		return err
	}
	defer tx.Rollback()

	// Upsert the board
	_, err = tx.Exec(`INSERT OR REPLACE INTO boards (id, name, description, created_at, updated_at, schedule_enabled, cron_expr, last_run, next_run, last_result)
		VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)`,
		board.Id, board.Name, board.Description,
		board.CreatedAt.UTC().Format(time.RFC3339), board.UpdatedAt.UTC().Format(time.RFC3339),
		boolToInt(board.ScheduleEnabled), board.CronExpr,
		timePtrToNullable(board.LastRun), timePtrToNullable(board.NextRun), board.LastResult)
	if err != nil {
		return fmt.Errorf("failed to save board: %w", err)
	}

	// Delete old nodes and edges, then insert new ones
	if _, err := tx.Exec("DELETE FROM board_nodes WHERE board_id = ?", board.Id); err != nil {
		return fmt.Errorf("failed to delete old nodes: %w", err)
	}
	if _, err := tx.Exec("DELETE FROM board_edges WHERE board_id = ?", board.Id); err != nil {
		return fmt.Errorf("failed to delete old edges: %w", err)
	}

	// Insert nodes
	for _, node := range board.Nodes {
		if _, err := tx.Exec(`INSERT INTO board_nodes (id, board_id, remote_name, path, label, x, y)
			VALUES (?, ?, ?, ?, ?, ?, ?)`,
			node.Id, board.Id, node.RemoteName, node.Path, node.Label, node.X, node.Y); err != nil {
			return fmt.Errorf("failed to save node: %w", err)
		}
	}

	// Insert edges
	for _, edge := range board.Edges {
		syncConfigJSON, jsonErr := json.Marshal(edge.SyncConfig)
		if jsonErr != nil {
			log.Printf("[BoardService] Warning: failed to marshal sync_config for edge %s: %v", edge.Id, jsonErr)
			syncConfigJSON = []byte("{}")
		}
		if _, err := tx.Exec(`INSERT INTO board_edges (id, board_id, source_id, target_id, action, sync_config)
			VALUES (?, ?, ?, ?, ?, ?)`,
			edge.Id, board.Id, edge.SourceId, edge.TargetId, edge.Action, string(syncConfigJSON)); err != nil {
			return fmt.Errorf("failed to save edge: %w", err)
		}
	}

	return tx.Commit()
}

// deleteBoardFromDB removes a board and all its nodes/edges (via CASCADE)
func (b *BoardService) deleteBoardFromDB(boardId string) error {
	db, err := GetSharedDB()
	if err != nil {
		return err
	}
	_, err = db.Exec("DELETE FROM boards WHERE id = ?", boardId)
	return err
}

// sendBoardNotification sends a desktop notification for board execution completion/failure
func (b *BoardService) sendBoardNotification(board *models.Board, success bool, status *models.BoardExecutionStatus) {
	if b.notificationService == nil {
		return
	}

	boardName := board.Name
	if boardName == "" {
		boardName = "Unnamed board"
	}

	var title, body string
	if success {
		title = "Board Execution Completed"
		completedCount := 0
		for _, es := range status.EdgeStatuses {
			if es.Status == "completed" {
				completedCount++
			}
		}
		body = fmt.Sprintf("Board \"%s\" completed successfully. %d sync(s) executed.", boardName, completedCount)
	} else {
		title = "Board Execution Failed"
		failedCount := 0
		for _, es := range status.EdgeStatuses {
			if es.Status == "failed" {
				failedCount++
			}
		}
		body = fmt.Sprintf("Board \"%s\" completed with %d failure(s).", boardName, failedCount)
	}

	// Send notification (context.Background() since flow context may be cancelled)
	if err := b.notificationService.SendNotification(context.Background(), title, body); err != nil {
		log.Printf("Failed to send board notification: %v", err)
	}
}

// emitBoardEvent emits a board event
func (b *BoardService) emitBoardEvent(eventType events.EventType, boardId, edgeId, status, message string) {
	event := events.NewBoardEvent(eventType, boardId, edgeId, status, message)
	if b.eventBus != nil {
		if err := b.eventBus.EmitBoardEvent(event); err != nil {
			log.Printf("Failed to emit board event: %v", err)
		}
	} else if b.app != nil {
		b.app.Event.Emit("tofe", event)
	}
}

// OnRemoteDeleted handles cleanup when a remote is deleted
// Removes nodes referencing the deleted remote and any edges connected to them
func (b *BoardService) OnRemoteDeleted(remoteName string) error {
	if err := b.ensureInitialized(); err != nil {
		return err
	}

	b.mutex.Lock()
	defer b.mutex.Unlock()

	modified := false
	for i := range b.boards {
		board := &b.boards[i]

		// Find nodes that reference the deleted remote
		nodesToRemove := make(map[string]bool)
		for _, node := range board.Nodes {
			if node.RemoteName == remoteName {
				nodesToRemove[node.Id] = true
			}
		}

		if len(nodesToRemove) == 0 {
			continue
		}

		// Remove edges connected to the nodes being removed
		newEdges := make([]models.BoardEdge, 0, len(board.Edges))
		for _, edge := range board.Edges {
			if !nodesToRemove[edge.SourceId] && !nodesToRemove[edge.TargetId] {
				newEdges = append(newEdges, edge)
			}
		}

		// Remove the nodes
		newNodes := make([]models.BoardNode, 0, len(board.Nodes))
		for _, node := range board.Nodes {
			if !nodesToRemove[node.Id] {
				newNodes = append(newNodes, node)
			}
		}

		board.Nodes = newNodes
		board.Edges = newEdges
		board.UpdatedAt = time.Now()
		modified = true

		// Save updated board to DB
		if err := b.saveBoardToDB(*board); err != nil {
			log.Printf("Warning: failed to save board '%s' after remote deletion: %v", board.Name, err)
		}

		log.Printf("Board '%s': removed %d nodes and associated edges for deleted remote '%s'",
			board.Name, len(nodesToRemove), remoteName)
	}

	if modified {
		// Emit event to notify frontend about board updates
		b.emitBoardEvent(events.BoardUpdated, "", "", "remote_deleted", fmt.Sprintf("Boards updated: remote '%s' was deleted", remoteName))
	}

	return nil
}
