package services

import (
	"context"
	"desktop/backend/models"
	"fmt"
	"testing"
	"time"
)

func newTestBoardService(t *testing.T) *BoardService {
	t.Helper()
	// Clean DB tables for test isolation
	db, _ := GetSharedDB()
	db.Exec("DELETE FROM board_edges")
	db.Exec("DELETE FROM board_nodes")
	db.Exec("DELETE FROM boards")
	return &BoardService{
		boards:      []models.Board{},
		activeFlows: make(map[string]*FlowExecution),
		initialized: true,
	}
}

func makeTestBoard(id, name string) models.Board {
	return models.Board{
		Id:   id,
		Name: name,
		Nodes: []models.BoardNode{
			{Id: "n1", RemoteName: "remote1", Path: "/data", Label: "Remote 1", X: 0, Y: 0},
			{Id: "n2", RemoteName: "remote2", Path: "/backup", Label: "Remote 2", X: 200, Y: 0},
		},
		Edges: []models.BoardEdge{
			{Id: "e1", SourceId: "n1", TargetId: "n2", Action: "push", SyncConfig: models.Profile{}},
		},
		CreatedAt: time.Now(),
		UpdatedAt: time.Now(),
	}
}

// --- CRUD Tests ---

func TestBoardService_AddBoard(t *testing.T) {
	s := newTestBoardService(t)
	ctx := context.Background()

	board := makeTestBoard("board-1", "Test Board")
	err := s.AddBoard(ctx, board)
	if err != nil {
		t.Fatalf("AddBoard failed: %v", err)
	}

	boards, err := s.GetBoards(ctx)
	if err != nil {
		t.Fatalf("GetBoards failed: %v", err)
	}
	if len(boards) != 1 {
		t.Fatalf("expected 1 board, got %d", len(boards))
	}
	if boards[0].Id != "board-1" {
		t.Errorf("expected board id 'board-1', got %q", boards[0].Id)
	}
}

func TestBoardService_AddBoard_DuplicateName(t *testing.T) {
	s := newTestBoardService(t)
	ctx := context.Background()

	board1 := makeTestBoard("board-1", "Same Name")
	if err := s.AddBoard(ctx, board1); err != nil {
		t.Fatalf("AddBoard failed: %v", err)
	}

	board2 := makeTestBoard("board-2", "Same Name")
	err := s.AddBoard(ctx, board2)
	if err == nil {
		t.Error("expected error for duplicate board name")
	}
}

func TestBoardService_AddBoard_MissingId(t *testing.T) {
	s := newTestBoardService(t)
	ctx := context.Background()

	board := makeTestBoard("", "No ID Board")
	err := s.AddBoard(ctx, board)
	if err == nil {
		t.Error("expected error for missing board ID")
	}
}

func TestBoardService_AddBoard_MissingName(t *testing.T) {
	s := newTestBoardService(t)
	ctx := context.Background()

	board := makeTestBoard("board-1", "")
	err := s.AddBoard(ctx, board)
	if err == nil {
		t.Error("expected error for missing board name")
	}
}

func TestBoardService_GetBoard(t *testing.T) {
	s := newTestBoardService(t)
	ctx := context.Background()

	board := makeTestBoard("board-1", "Test Board")
	if err := s.AddBoard(ctx, board); err != nil {
		t.Fatalf("AddBoard failed: %v", err)
	}

	got, err := s.GetBoard(ctx, "board-1")
	if err != nil {
		t.Fatalf("GetBoard failed: %v", err)
	}
	if got.Name != "Test Board" {
		t.Errorf("expected name 'Test Board', got %q", got.Name)
	}
}

func TestBoardService_GetBoard_NotFound(t *testing.T) {
	s := newTestBoardService(t)
	ctx := context.Background()

	_, err := s.GetBoard(ctx, "nonexistent")
	if err == nil {
		t.Error("expected error for nonexistent board")
	}
}

func TestBoardService_UpdateBoard(t *testing.T) {
	s := newTestBoardService(t)
	ctx := context.Background()

	board := makeTestBoard("board-1", "Original")
	if err := s.AddBoard(ctx, board); err != nil {
		t.Fatalf("AddBoard failed: %v", err)
	}

	board.Name = "Updated"
	if err := s.UpdateBoard(ctx, board); err != nil {
		t.Fatalf("UpdateBoard failed: %v", err)
	}

	got, err := s.GetBoard(ctx, "board-1")
	if err != nil {
		t.Fatalf("GetBoard failed: %v", err)
	}
	if got.Name != "Updated" {
		t.Errorf("expected name 'Updated', got %q", got.Name)
	}
}

func TestBoardService_UpdateBoard_NotFound(t *testing.T) {
	s := newTestBoardService(t)
	ctx := context.Background()

	board := makeTestBoard("nonexistent", "Ghost")
	err := s.UpdateBoard(ctx, board)
	if err == nil {
		t.Error("expected error for updating nonexistent board")
	}
}

func TestBoardService_DeleteBoard(t *testing.T) {
	s := newTestBoardService(t)
	ctx := context.Background()

	board := makeTestBoard("board-1", "To Delete")
	if err := s.AddBoard(ctx, board); err != nil {
		t.Fatalf("AddBoard failed: %v", err)
	}

	if err := s.DeleteBoard(ctx, "board-1"); err != nil {
		t.Fatalf("DeleteBoard failed: %v", err)
	}

	boards, err := s.GetBoards(ctx)
	if err != nil {
		t.Fatalf("GetBoards failed: %v", err)
	}
	if len(boards) != 0 {
		t.Errorf("expected 0 boards, got %d", len(boards))
	}
}

func TestBoardService_DeleteBoard_NotFound(t *testing.T) {
	s := newTestBoardService(t)
	ctx := context.Background()

	err := s.DeleteBoard(ctx, "nonexistent")
	if err == nil {
		t.Error("expected error for deleting nonexistent board")
	}
}

// --- Validation Tests ---

func TestBoardService_ValidateBoard_SelfLoop(t *testing.T) {
	s := newTestBoardService(t)
	ctx := context.Background()

	board := models.Board{
		Id:   "board-loop",
		Name: "Self Loop",
		Nodes: []models.BoardNode{
			{Id: "n1", RemoteName: "remote1", Label: "R1"},
		},
		Edges: []models.BoardEdge{
			{Id: "e1", SourceId: "n1", TargetId: "n1", Action: "push"},
		},
	}

	err := s.AddBoard(ctx, board)
	if err == nil {
		t.Error("expected error for self-loop edge")
	}
}

func TestBoardService_ValidateBoard_InvalidAction(t *testing.T) {
	s := newTestBoardService(t)
	ctx := context.Background()

	board := models.Board{
		Id:   "board-bad-action",
		Name: "Bad Action",
		Nodes: []models.BoardNode{
			{Id: "n1", RemoteName: "remote1", Label: "R1"},
			{Id: "n2", RemoteName: "remote2", Label: "R2"},
		},
		Edges: []models.BoardEdge{
			{Id: "e1", SourceId: "n1", TargetId: "n2", Action: "invalid"},
		},
	}

	err := s.AddBoard(ctx, board)
	if err == nil {
		t.Error("expected error for invalid edge action")
	}
}

func TestBoardService_ValidateBoard_EmptyAction(t *testing.T) {
	s := newTestBoardService(t)
	ctx := context.Background()

	board := models.Board{
		Id:   "board-empty-action",
		Name: "Empty Action",
		Nodes: []models.BoardNode{
			{Id: "n1", RemoteName: "remote1", Label: "R1"},
			{Id: "n2", RemoteName: "remote2", Label: "R2"},
		},
		Edges: []models.BoardEdge{
			{Id: "e1", SourceId: "n1", TargetId: "n2", Action: ""},
		},
	}

	err := s.AddBoard(ctx, board)
	if err == nil {
		t.Error("expected error for empty edge action")
	}
}

func TestBoardService_ValidateBoard_DuplicateNodeId(t *testing.T) {
	s := newTestBoardService(t)
	ctx := context.Background()

	board := models.Board{
		Id:   "board-dup",
		Name: "Dup Nodes",
		Nodes: []models.BoardNode{
			{Id: "n1", RemoteName: "remote1", Label: "R1"},
			{Id: "n1", RemoteName: "remote2", Label: "R2"},
		},
		Edges: []models.BoardEdge{},
	}

	err := s.AddBoard(ctx, board)
	if err == nil {
		t.Error("expected error for duplicate node ID")
	}
}

func TestBoardService_ValidateBoard_DuplicateEdgeId(t *testing.T) {
	s := newTestBoardService(t)
	ctx := context.Background()

	board := models.Board{
		Id:   "board-dup-edge",
		Name: "Dup Edges",
		Nodes: []models.BoardNode{
			{Id: "n1", RemoteName: "r1", Label: "R1"},
			{Id: "n2", RemoteName: "r2", Label: "R2"},
			{Id: "n3", RemoteName: "r3", Label: "R3"},
		},
		Edges: []models.BoardEdge{
			{Id: "e1", SourceId: "n1", TargetId: "n2", Action: "push"},
			{Id: "e1", SourceId: "n2", TargetId: "n3", Action: "push"},
		},
	}

	err := s.AddBoard(ctx, board)
	if err == nil {
		t.Error("expected error for duplicate edge ID")
	}
}

func TestBoardService_ValidateBoard_UnknownSourceNode(t *testing.T) {
	s := newTestBoardService(t)
	ctx := context.Background()

	board := models.Board{
		Id:   "board-bad-ref",
		Name: "Bad Ref",
		Nodes: []models.BoardNode{
			{Id: "n1", RemoteName: "r1", Label: "R1"},
		},
		Edges: []models.BoardEdge{
			{Id: "e1", SourceId: "nope", TargetId: "n1", Action: "push"},
		},
	}

	err := s.AddBoard(ctx, board)
	if err == nil {
		t.Error("expected error for unknown source node reference")
	}
}

func TestBoardService_ValidateBoard_UnknownTargetNode(t *testing.T) {
	s := newTestBoardService(t)
	ctx := context.Background()

	board := models.Board{
		Id:   "board-bad-tgt",
		Name: "Bad Target",
		Nodes: []models.BoardNode{
			{Id: "n1", RemoteName: "r1", Label: "R1"},
		},
		Edges: []models.BoardEdge{
			{Id: "e1", SourceId: "n1", TargetId: "nope", Action: "push"},
		},
	}

	err := s.AddBoard(ctx, board)
	if err == nil {
		t.Error("expected error for unknown target node reference")
	}
}

func TestBoardService_ValidateBoard_NoEdges(t *testing.T) {
	s := newTestBoardService(t)
	ctx := context.Background()

	board := models.Board{
		Id:   "board-no-edges",
		Name: "No Edges",
		Nodes: []models.BoardNode{
			{Id: "n1", RemoteName: "r1", Label: "R1"},
		},
		Edges: []models.BoardEdge{},
	}

	// Board with no edges should be valid (just can't be executed)
	err := s.AddBoard(ctx, board)
	if err != nil {
		t.Errorf("board with no edges should be valid, got: %v", err)
	}
}

// --- Cycle Detection Tests ---

func TestBoardService_DetectCycles_NoCycle(t *testing.T) {
	s := newTestBoardService(t)

	board := &models.Board{
		Nodes: []models.BoardNode{
			{Id: "a"}, {Id: "b"}, {Id: "c"},
		},
		Edges: []models.BoardEdge{
			{Id: "e1", SourceId: "a", TargetId: "b", Action: "push"},
			{Id: "e2", SourceId: "b", TargetId: "c", Action: "push"},
		},
	}

	err := s.detectCycles(board)
	if err != nil {
		t.Errorf("expected no cycle, got: %v", err)
	}
}

func TestBoardService_DetectCycles_SimpleCycle(t *testing.T) {
	s := newTestBoardService(t)

	board := &models.Board{
		Name: "Cyclic",
		Nodes: []models.BoardNode{
			{Id: "a"}, {Id: "b"}, {Id: "c"},
		},
		Edges: []models.BoardEdge{
			{Id: "e1", SourceId: "a", TargetId: "b", Action: "push"},
			{Id: "e2", SourceId: "b", TargetId: "c", Action: "push"},
			{Id: "e3", SourceId: "c", TargetId: "a", Action: "push"},
		},
	}

	err := s.detectCycles(board)
	if err == nil {
		t.Error("expected cycle detection error")
	}
}

func TestBoardService_DetectCycles_TwoNodeCycle(t *testing.T) {
	s := newTestBoardService(t)

	board := &models.Board{
		Name: "Two Node Cycle",
		Nodes: []models.BoardNode{
			{Id: "a"}, {Id: "b"},
		},
		Edges: []models.BoardEdge{
			{Id: "e1", SourceId: "a", TargetId: "b", Action: "push"},
			{Id: "e2", SourceId: "b", TargetId: "a", Action: "push"},
		},
	}

	err := s.detectCycles(board)
	if err == nil {
		t.Error("expected cycle detection error for two-node cycle")
	}
}

func TestBoardService_DetectCycles_DiamondNoCycle(t *testing.T) {
	s := newTestBoardService(t)

	// Diamond: A->B, A->C, B->D, C->D (no cycle)
	board := &models.Board{
		Nodes: []models.BoardNode{
			{Id: "a"}, {Id: "b"}, {Id: "c"}, {Id: "d"},
		},
		Edges: []models.BoardEdge{
			{Id: "e1", SourceId: "a", TargetId: "b", Action: "push"},
			{Id: "e2", SourceId: "a", TargetId: "c", Action: "push"},
			{Id: "e3", SourceId: "b", TargetId: "d", Action: "push"},
			{Id: "e4", SourceId: "c", TargetId: "d", Action: "push"},
		},
	}

	err := s.detectCycles(board)
	if err != nil {
		t.Errorf("diamond graph should not have cycle, got: %v", err)
	}
}

func TestBoardService_DetectCycles_DisconnectedWithCycle(t *testing.T) {
	s := newTestBoardService(t)

	// Disconnected components: A->B (no cycle), C->D->C (cycle)
	board := &models.Board{
		Name: "Disconnected Cycle",
		Nodes: []models.BoardNode{
			{Id: "a"}, {Id: "b"}, {Id: "c"}, {Id: "d"},
		},
		Edges: []models.BoardEdge{
			{Id: "e1", SourceId: "a", TargetId: "b", Action: "push"},
			{Id: "e2", SourceId: "c", TargetId: "d", Action: "push"},
			{Id: "e3", SourceId: "d", TargetId: "c", Action: "push"},
		},
	}

	err := s.detectCycles(board)
	if err == nil {
		t.Error("expected cycle detection in disconnected component")
	}
}

// --- Topological Sort / Execution Layers Tests ---

func TestBoardService_ComputeExecutionLayers_Linear(t *testing.T) {
	s := newTestBoardService(t)

	// Linear chain: A->B->C
	board := &models.Board{
		Nodes: []models.BoardNode{
			{Id: "a"}, {Id: "b"}, {Id: "c"},
		},
		Edges: []models.BoardEdge{
			{Id: "e1", SourceId: "a", TargetId: "b", Action: "push"},
			{Id: "e2", SourceId: "b", TargetId: "c", Action: "push"},
		},
	}

	layers := s.computeExecutionLayers(board)
	if len(layers) != 2 {
		t.Fatalf("expected 2 layers for linear chain, got %d", len(layers))
	}
	if len(layers[0]) != 1 || layers[0][0].Id != "e1" {
		t.Errorf("layer 0 should contain only e1, got %v", layers[0])
	}
	if len(layers[1]) != 1 || layers[1][0].Id != "e2" {
		t.Errorf("layer 1 should contain only e2, got %v", layers[1])
	}
}

func TestBoardService_ComputeExecutionLayers_Parallel(t *testing.T) {
	s := newTestBoardService(t)

	// Parallel: A->B, C->D (independent edges)
	board := &models.Board{
		Nodes: []models.BoardNode{
			{Id: "a"}, {Id: "b"}, {Id: "c"}, {Id: "d"},
		},
		Edges: []models.BoardEdge{
			{Id: "e1", SourceId: "a", TargetId: "b", Action: "push"},
			{Id: "e2", SourceId: "c", TargetId: "d", Action: "push"},
		},
	}

	layers := s.computeExecutionLayers(board)
	if len(layers) != 1 {
		t.Fatalf("expected 1 layer for independent edges, got %d", len(layers))
	}
	if len(layers[0]) != 2 {
		t.Errorf("layer 0 should contain 2 edges, got %d", len(layers[0]))
	}
}

func TestBoardService_ComputeExecutionLayers_Diamond(t *testing.T) {
	s := newTestBoardService(t)

	// Diamond: A->B, A->C, B->D, C->D
	board := &models.Board{
		Nodes: []models.BoardNode{
			{Id: "a"}, {Id: "b"}, {Id: "c"}, {Id: "d"},
		},
		Edges: []models.BoardEdge{
			{Id: "e1", SourceId: "a", TargetId: "b", Action: "push"},
			{Id: "e2", SourceId: "a", TargetId: "c", Action: "push"},
			{Id: "e3", SourceId: "b", TargetId: "d", Action: "push"},
			{Id: "e4", SourceId: "c", TargetId: "d", Action: "push"},
		},
	}

	layers := s.computeExecutionLayers(board)
	if len(layers) != 2 {
		t.Fatalf("expected 2 layers for diamond, got %d", len(layers))
	}
	// Layer 0: e1, e2 (both start from A, no deps)
	if len(layers[0]) != 2 {
		t.Errorf("layer 0 should have 2 edges, got %d", len(layers[0]))
	}
	// Layer 1: e3, e4 (depend on e1 and e2 respectively)
	if len(layers[1]) != 2 {
		t.Errorf("layer 1 should have 2 edges, got %d", len(layers[1]))
	}
}

func TestBoardService_ComputeExecutionLayers_FanOut(t *testing.T) {
	s := newTestBoardService(t)

	// Fan-out: A->B, A->C, A->D
	board := &models.Board{
		Nodes: []models.BoardNode{
			{Id: "a"}, {Id: "b"}, {Id: "c"}, {Id: "d"},
		},
		Edges: []models.BoardEdge{
			{Id: "e1", SourceId: "a", TargetId: "b", Action: "push"},
			{Id: "e2", SourceId: "a", TargetId: "c", Action: "push"},
			{Id: "e3", SourceId: "a", TargetId: "d", Action: "push"},
		},
	}

	layers := s.computeExecutionLayers(board)
	if len(layers) != 1 {
		t.Fatalf("expected 1 layer for fan-out, got %d", len(layers))
	}
	if len(layers[0]) != 3 {
		t.Errorf("layer 0 should have 3 edges, got %d", len(layers[0]))
	}
}

func TestBoardService_ComputeExecutionLayers_FanIn(t *testing.T) {
	s := newTestBoardService(t)

	// Fan-in: A->D, B->D, C->D
	board := &models.Board{
		Nodes: []models.BoardNode{
			{Id: "a"}, {Id: "b"}, {Id: "c"}, {Id: "d"},
		},
		Edges: []models.BoardEdge{
			{Id: "e1", SourceId: "a", TargetId: "d", Action: "push"},
			{Id: "e2", SourceId: "b", TargetId: "d", Action: "push"},
			{Id: "e3", SourceId: "c", TargetId: "d", Action: "push"},
		},
	}

	layers := s.computeExecutionLayers(board)
	if len(layers) != 1 {
		t.Fatalf("expected 1 layer for fan-in, got %d", len(layers))
	}
	if len(layers[0]) != 3 {
		t.Errorf("layer 0 should have 3 edges, got %d", len(layers[0]))
	}
}

func TestBoardService_ComputeExecutionLayers_Complex(t *testing.T) {
	s := newTestBoardService(t)

	// Complex: A->B, A->C, B->D, C->D, D->E
	board := &models.Board{
		Nodes: []models.BoardNode{
			{Id: "a"}, {Id: "b"}, {Id: "c"}, {Id: "d"}, {Id: "e"},
		},
		Edges: []models.BoardEdge{
			{Id: "e1", SourceId: "a", TargetId: "b", Action: "push"},
			{Id: "e2", SourceId: "a", TargetId: "c", Action: "push"},
			{Id: "e3", SourceId: "b", TargetId: "d", Action: "push"},
			{Id: "e4", SourceId: "c", TargetId: "d", Action: "push"},
			{Id: "e5", SourceId: "d", TargetId: "e", Action: "push"},
		},
	}

	layers := s.computeExecutionLayers(board)
	if len(layers) != 3 {
		t.Fatalf("expected 3 layers, got %d", len(layers))
	}
	// Layer 0: e1, e2 (from A)
	if len(layers[0]) != 2 {
		t.Errorf("layer 0 should have 2 edges, got %d", len(layers[0]))
	}
	// Layer 1: e3, e4 (B->D, C->D)
	if len(layers[1]) != 2 {
		t.Errorf("layer 1 should have 2 edges, got %d", len(layers[1]))
	}
	// Layer 2: e5 (D->E)
	if len(layers[2]) != 1 {
		t.Errorf("layer 2 should have 1 edge, got %d", len(layers[2]))
	}
}

func TestBoardService_ComputeExecutionLayers_Empty(t *testing.T) {
	s := newTestBoardService(t)

	board := &models.Board{
		Nodes: []models.BoardNode{{Id: "a"}},
		Edges: []models.BoardEdge{},
	}

	layers := s.computeExecutionLayers(board)
	if len(layers) != 0 {
		t.Errorf("expected 0 layers for empty edges, got %d", len(layers))
	}
}

// --- Path Parsing Tests ---

func TestParseRemoteName(t *testing.T) {
	tests := []struct {
		input    string
		expected string
	}{
		{"gdrive:/path/to/folder", "gdrive"},
		{"onedrive:", "onedrive"},
		{"s3:bucket/key", "s3"},
		{"/local/path", ""},
		{"./relative/path", ""},
		{"", ""},
		{"remote-name:path", "remote-name"},
	}

	for _, tt := range tests {
		got := parseRemoteName(tt.input)
		if got != tt.expected {
			t.Errorf("parseRemoteName(%q) = %q, want %q", tt.input, got, tt.expected)
		}
	}
}

func TestParseRemotePath(t *testing.T) {
	tests := []struct {
		input    string
		expected string
	}{
		{"gdrive:/path/to/folder", "/path/to/folder"},
		{"onedrive:", ""},
		{"s3:bucket/key", "bucket/key"},
		{"/local/path", "/local/path"},
		{"./relative/path", "./relative/path"},
		{"", ""},
		{"remote-name:path", "path"},
	}

	for _, tt := range tests {
		got := parseRemotePath(tt.input)
		if got != tt.expected {
			t.Errorf("parseRemotePath(%q) = %q, want %q", tt.input, got, tt.expected)
		}
	}
}

func TestBoardService_BuildRemotePath(t *testing.T) {
	s := newTestBoardService(t)

	tests := []struct {
		node     models.BoardNode
		expected string
	}{
		{models.BoardNode{RemoteName: "gdrive", Path: "/data"}, "gdrive:/data"},
		{models.BoardNode{RemoteName: "gdrive", Path: ""}, "gdrive:"},
		{models.BoardNode{RemoteName: "local", Path: "/home/user/data"}, "/home/user/data"},
		{models.BoardNode{RemoteName: "", Path: "/home/user/data"}, "/home/user/data"},
		{models.BoardNode{RemoteName: "s3", Path: "bucket/prefix"}, "s3:bucket/prefix"},
	}

	for _, tt := range tests {
		got := s.buildRemotePath(&tt.node)
		if got != tt.expected {
			t.Errorf("buildRemotePath(%+v) = %q, want %q", tt.node, got, tt.expected)
		}
	}
}

// --- Execution Tests ---

func TestBoardService_ExecuteBoard_NoSyncService(t *testing.T) {
	s := newTestBoardService(t)
	ctx := context.Background()

	board := makeTestBoard("board-1", "Test")
	if err := s.AddBoard(ctx, board); err != nil {
		t.Fatalf("AddBoard failed: %v", err)
	}

	_, err := s.ExecuteBoard(ctx, "board-1")
	if err == nil {
		t.Error("expected error when sync service is not available")
	}
}

func TestBoardService_ExecuteBoard_NotFound(t *testing.T) {
	s := newTestBoardService(t)
	s.syncService = &SyncService{activeTasks: make(map[int]*SyncTask)}
	ctx := context.Background()

	_, err := s.ExecuteBoard(ctx, "nonexistent")
	if err == nil {
		t.Error("expected error for nonexistent board")
	}
}

func TestBoardService_ExecuteBoard_NoEdges(t *testing.T) {
	s := newTestBoardService(t)
	s.syncService = &SyncService{activeTasks: make(map[int]*SyncTask)}
	ctx := context.Background()

	board := models.Board{
		Id:    "board-no-edges",
		Name:  "No Edges",
		Nodes: []models.BoardNode{{Id: "n1", RemoteName: "r1", Label: "R1"}},
		Edges: []models.BoardEdge{},
	}
	if err := s.AddBoard(ctx, board); err != nil {
		t.Fatalf("AddBoard failed: %v", err)
	}

	_, err := s.ExecuteBoard(ctx, "board-no-edges")
	if err == nil {
		t.Error("expected error for board with no edges")
	}
}

func TestBoardService_ExecuteBoard_WithCycle(t *testing.T) {
	s := newTestBoardService(t)
	s.syncService = &SyncService{activeTasks: make(map[int]*SyncTask)}
	ctx := context.Background()

	// Manually add a cyclic board (bypass validation for this test)
	s.mutex.Lock()
	s.boards = append(s.boards, models.Board{
		Id:   "cyclic",
		Name: "Cyclic Board",
		Nodes: []models.BoardNode{
			{Id: "a"}, {Id: "b"}, {Id: "c"},
		},
		Edges: []models.BoardEdge{
			{Id: "e1", SourceId: "a", TargetId: "b", Action: "push"},
			{Id: "e2", SourceId: "b", TargetId: "c", Action: "push"},
			{Id: "e3", SourceId: "c", TargetId: "a", Action: "push"},
		},
	})
	s.mutex.Unlock()

	_, err := s.ExecuteBoard(ctx, "cyclic")
	if err == nil {
		t.Error("expected error for cyclic board execution")
	}
}

func TestBoardService_StopExecution_NotRunning(t *testing.T) {
	s := newTestBoardService(t)
	ctx := context.Background()

	// Should not error — graceful no-op when no active execution
	err := s.StopBoardExecution(ctx, "nonexistent")
	if err != nil {
		t.Errorf("expected no error when stopping non-running board, got: %v", err)
	}
}

func TestBoardService_GetExecutionStatus_NotRunning(t *testing.T) {
	s := newTestBoardService(t)
	ctx := context.Background()

	_, err := s.GetBoardExecutionStatus(ctx, "nonexistent")
	if err == nil {
		t.Error("expected error when getting status of non-running board")
	}
}

// --- Persistence Tests ---

func TestBoardService_Persistence(t *testing.T) {
	ctx := context.Background()

	// Create first service instance and add a board
	s1 := &BoardService{
		boards:      []models.Board{},
		activeFlows: make(map[string]*FlowExecution),
		initialized: true,
	}

	board := makeTestBoard("persist-board-1", "Persistent Board")
	if err := s1.AddBoard(ctx, board); err != nil {
		t.Fatalf("AddBoard failed: %v", err)
	}

	// Create second service instance and load from DB
	s2 := &BoardService{
		boards:      []models.Board{},
		activeFlows: make(map[string]*FlowExecution),
	}
	if err := s2.initialize(); err != nil {
		t.Fatalf("initialize failed: %v", err)
	}

	found := false
	for _, b := range s2.boards {
		if b.Id == "persist-board-1" && b.Name == "Persistent Board" {
			found = true
			break
		}
	}
	if !found {
		t.Error("expected to find 'Persistent Board' after loading from DB")
	}
}

// --- UpdateBoard preserves CreatedAt ---

func TestBoardService_UpdateBoard_PreservesCreatedAt(t *testing.T) {
	s := newTestBoardService(t)
	ctx := context.Background()

	board := makeTestBoard("board-1", "Original")
	board.CreatedAt = time.Date(2024, 1, 1, 0, 0, 0, 0, time.UTC)
	if err := s.AddBoard(ctx, board); err != nil {
		t.Fatalf("AddBoard failed: %v", err)
	}

	board.Name = "Updated"
	board.CreatedAt = time.Now() // try to change it
	if err := s.UpdateBoard(ctx, board); err != nil {
		t.Fatalf("UpdateBoard failed: %v", err)
	}

	got, _ := s.GetBoard(ctx, "board-1")
	if got.CreatedAt.Year() != 2024 {
		t.Errorf("expected CreatedAt to be preserved (2024), got %v", got.CreatedAt)
	}
}

// --- Edge Config in Board ---

func TestBoardService_AddBoard_WithSyncConfig(t *testing.T) {
	s := newTestBoardService(t)
	ctx := context.Background()

	board := models.Board{
		Id:   "board-config",
		Name: "Config Board",
		Nodes: []models.BoardNode{
			{Id: "n1", RemoteName: "gdrive", Path: "/data", Label: "GDrive"},
			{Id: "n2", RemoteName: "local", Path: "/backup", Label: "Local"},
		},
		Edges: []models.BoardEdge{
			{
				Id:       "e1",
				SourceId: "n1",
				TargetId: "n2",
				Action:   "pull",
				SyncConfig: models.Profile{
					Name:     "gdrive-to-local",
					Parallel: 8,
				},
			},
		},
	}

	if err := s.AddBoard(ctx, board); err != nil {
		t.Fatalf("AddBoard failed: %v", err)
	}

	got, err := s.GetBoard(ctx, "board-config")
	if err != nil {
		t.Fatalf("GetBoard failed: %v", err)
	}

	if len(got.Edges) != 1 {
		t.Fatalf("expected 1 edge, got %d", len(got.Edges))
	}
	if got.Edges[0].SyncConfig.Parallel != 8 {
		t.Errorf("expected parallel=8, got %d", got.Edges[0].SyncConfig.Parallel)
	}
}

// --- Valid board actions ---

func TestBoardService_ValidActions(t *testing.T) {
	s := newTestBoardService(t)
	ctx := context.Background()

	validActions := []string{"push", "pull", "bi", "bi-resync"}
	for i, action := range validActions {
		board := models.Board{
			Id:   fmt.Sprintf("board-%d", i),
			Name: fmt.Sprintf("Board %s", action),
			Nodes: []models.BoardNode{
				{Id: "n1", RemoteName: "r1", Label: "R1"},
				{Id: "n2", RemoteName: "r2", Label: "R2"},
			},
			Edges: []models.BoardEdge{
				{Id: "e1", SourceId: "n1", TargetId: "n2", Action: action},
			},
		}

		if err := s.AddBoard(ctx, board); err != nil {
			t.Errorf("expected action %q to be valid, got error: %v", action, err)
		}
	}
}
