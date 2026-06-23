package main

import (
	"testing"

	"github.com/gnasdev/gn-drive/internal/store"
)

// Test: linear chain A -> B -> C should produce 2 layers:
// layer 0 = e1 (A is a source, indeg 0),
// layer 1 = e2 (B's indeg goes from 1 to 0 after e1 fires).
func TestTopoLayers_LinearChain(t *testing.T) {
	nodes := []store.BoardNode{
		{ID: "A", Label: "A"},
		{ID: "B", Label: "B"},
		{ID: "C", Label: "C"},
	}
	edges := []store.BoardEdge{
		{ID: "e1", SourceID: "A", TargetID: "B"},
		{ID: "e2", SourceID: "B", TargetID: "C"},
	}
	layers, err := topoLayers(nodes, edges)
	if err != nil {
		t.Fatalf("topoLayers: %v", err)
	}
	if len(layers) != 2 {
		t.Fatalf("expected 2 layers, got %d (%+v)", len(layers), layers)
	}
	if layers[0][0].ID != "e1" || layers[1][0].ID != "e2" {
		t.Errorf("unexpected layer order: %+v", layers)
	}
}

// Test: diamond A -> B, A -> C, B -> D, C -> D. Edges e1+e2 share
// layer 0 (both sources from A, which has no incoming), and e3+e4 share
// layer 1 (B and C both have indeg 0 after layer 0).
func TestTopoLayers_Diamond(t *testing.T) {
	nodes := []store.BoardNode{
		{ID: "A"}, {ID: "B"}, {ID: "C"}, {ID: "D"},
	}
	edges := []store.BoardEdge{
		{ID: "e1", SourceID: "A", TargetID: "B"},
		{ID: "e2", SourceID: "A", TargetID: "C"},
		{ID: "e3", SourceID: "B", TargetID: "D"},
		{ID: "e4", SourceID: "C", TargetID: "D"},
	}
	layers, err := topoLayers(nodes, edges)
	if err != nil {
		t.Fatalf("topoLayers: %v", err)
	}
	if len(layers) != 2 {
		t.Fatalf("expected 2 layers, got %d (%+v)", len(layers), layers)
	}
	if len(layers[0]) != 2 || layers[0][0].ID != "e1" || layers[0][1].ID != "e2" {
		t.Errorf("layer 0 wrong: %+v", layers[0])
	}
	if len(layers[1]) != 2 || layers[1][0].ID != "e3" || layers[1][1].ID != "e4" {
		t.Errorf("layer 1 wrong: %+v", layers[1])
	}
}

// Test: cycle A -> B -> A should error.
func TestTopoLayers_Cycle(t *testing.T) {
	nodes := []store.BoardNode{{ID: "A"}, {ID: "B"}}
	edges := []store.BoardEdge{
		{ID: "e1", SourceID: "A", TargetID: "B"},
		{ID: "e2", SourceID: "B", TargetID: "A"},
	}
	_, err := topoLayers(nodes, edges)
	if err == nil {
		t.Fatal("expected cycle error, got nil")
	}
}

// Test: disconnected components — A->B and C->D — should each schedule
// in 1 layer.
func TestTopoLayers_Disconnected(t *testing.T) {
	nodes := []store.BoardNode{
		{ID: "A"}, {ID: "B"}, {ID: "C"}, {ID: "D"},
	}
	edges := []store.BoardEdge{
		{ID: "e1", SourceID: "A", TargetID: "B"},
		{ID: "e2", SourceID: "C", TargetID: "D"},
	}
	layers, err := topoLayers(nodes, edges)
	if err != nil {
		t.Fatalf("topoLayers: %v", err)
	}
	if len(layers) != 1 {
		t.Fatalf("expected 1 layer for 2 independent edges, got %d", len(layers))
	}
	if len(layers[0]) != 2 {
		t.Errorf("expected 2 edges in layer 0, got %d", len(layers[0]))
	}
}
