package main

import (
	"context"
	"fmt"
	"sort"

	"github.com/gnasdev/gn-drive/internal/app"
	"github.com/gnasdev/gn-drive/internal/logging"
	"github.com/gnasdev/gn-drive/internal/rclone"
	"github.com/gnasdev/gn-drive/internal/store"
	"github.com/spf13/cobra"
)

func newBoardCmd() *cobra.Command {
	var (
		stopOnError bool
		concurrency int
	)
	cmd := &cobra.Command{
		Use:   "board [board-id]",
		Short: "Execute a board DAG",
		Long: `Execute all edges in a board DAG in topological order.

Each edge is a sync between two nodes (source → target). Nodes without
incoming edges run first (sources). Edges at the same topological layer
run sequentially in deterministic order so output is reproducible.

Examples:
  gn-drive board my-daily-backup
  gn-drive board my-daily-backup --no-stop-on-error`,
		Args: cobra.ExactArgs(1),
		RunE: func(cmd *cobra.Command, args []string) error {
			boardID := args[0]
			ctx := context.Background()
			a, err := appNewFn(ctx, app.Options{LogMode: logging.ModeForeground})
			if err != nil {
				return err
			}
			defer a.Close()
			return runBoard(ctx, a, boardID, stopOnError, concurrency, cmd)
		},
	}
	cmd.Flags().BoolVar(&stopOnError, "stop-on-error", true, "Stop execution at the first failed edge")
	cmd.Flags().IntVar(&concurrency, "concurrency", 1, "Max edges to run in parallel per layer (reserved)")
	return cmd
}

// runBoard is the testable inner work of the board command.
func runBoard(ctx context.Context, a *app.App, boardID string, stopOnError bool, concurrency int, cmd *cobra.Command) error {
	b, err := a.Store.Boards().LoadGraph(ctx, boardID)
	if err != nil {
		return fmt.Errorf("board: %q: %w", boardID, err)
	}
	if len(b.Nodes) == 0 {
		return fmt.Errorf("board %q has no nodes — define nodes in the web UI first", boardID)
	}
	if len(b.Edges) == 0 {
		return fmt.Errorf("board %q has no edges", boardID)
	}

	fmt.Fprintf(cmd.OutOrStdout(), "▶ Board: %s (%s) — %d nodes, %d edges\n",
		b.Name, b.ID, len(b.Nodes), len(b.Edges))

	nodeByID := make(map[string]store.BoardNode, len(b.Nodes))
	for _, n := range b.Nodes {
		nodeByID[n.ID] = n
	}

	layers, err := topoLayers(b.Nodes, b.Edges)
	if err != nil {
		return fmt.Errorf("board: %w", err)
	}

	concur := concurrency
	if concur < 1 {
		concur = 1
	}

	idx := 0
	total := len(b.Edges)
	var firstErr error
	for layerI, layer := range layers {
		fmt.Fprintf(cmd.OutOrStdout(), "  ┌─ layer %d (%d edge(s))\n", layerI+1, len(layer))
		for _, edge := range layer {
			idx++
			src, sok := nodeByID[edge.SourceID]
			dst, dok := nodeByID[edge.TargetID]
			if !sok || !dok {
				return fmt.Errorf("edge %s references missing node (source=%s target=%s)",
					edge.ID, edge.SourceID, edge.TargetID)
			}
			fmt.Fprintf(cmd.OutOrStdout(), "  │ [%d/%d] %s — %s : %s → %s (action=%s)\n",
				idx, total, edge.ID, edge.Action, src.Label, dst.Label, edge.Action)
			if err := runEdge(ctx, a, edge, src, dst); err != nil {
				fmt.Fprintf(cmd.OutOrStdout(), "  │   ✗ edge %s failed: %v\n", edge.ID, err)
				if stopOnError {
					return fmt.Errorf("board: stopped at edge %s: %w", edge.ID, err)
				}
				if firstErr == nil {
					firstErr = err
				}
				continue
			}
			fmt.Fprintf(cmd.OutOrStdout(), "  │   ✓ edge %s ok\n", edge.ID)
		}
		fmt.Fprintf(cmd.OutOrStdout(), "  └─ layer %d done\n", layerI+1)
		_ = concur
	}

	if firstErr != nil {
		return fmt.Errorf("board: completed with errors (first: %w)", firstErr)
	}
	fmt.Fprintf(cmd.OutOrStdout(), "✓ board %q executed %d edges successfully\n", b.ID, total)
	return nil
}

// syncExecutor is the subset of rclone.Client used by runEdge. It allows
// tests to inject a stub without shelling out to rclone.
type syncExecutor interface {
	Sync(ctx context.Context, cfg rclone.SyncConfig, onProgress func(rclone.Stats)) (*rclone.SyncResult, error)
}

// newSyncExecutor is the production constructor; tests may override it.
var newSyncExecutor = func(a *app.App) syncExecutor {
	return a.Rclone
}

func runEdge(ctx context.Context, a *app.App, edge store.BoardEdge, src, dst store.BoardNode) error {
	return runEdgeWith(ctx, newSyncExecutor(a), edge, src, dst)
}

// runEdgeWith is the testable inner of runEdge that accepts an injected
// syncExecutor.
func runEdgeWith(ctx context.Context, exec syncExecutor, edge store.BoardEdge, src, dst store.BoardNode) error {
	source := src.RemoteName
	if source != "" && src.Path != "" && src.Path != "/" {
		source = source + ":" + src.Path
	} else if source != "" {
		source = source + ":"
	}
	dest := dst.RemoteName
	if dest != "" && dst.Path != "" && dst.Path != "/" {
		dest = dest + ":" + dst.Path
	} else if dest != "" {
		dest = dest + ":"
	}
	if source == "" {
		source = src.Path
	}
	if dest == "" {
		dest = dst.Path
	}

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

// topoLayers returns the edges grouped by topological layer. The first
// layer contains edges whose source nodes have no incoming edges; each
// subsequent layer contains edges whose source nodes are in earlier layers.
//
// A cycle produces an error.
func topoLayers(nodes []store.BoardNode, edges []store.BoardEdge) ([][]store.BoardEdge, error) {
	// indeg[nodeID] = number of edges whose target is that node.
	indeg := make(map[string]int, len(nodes))
	for _, n := range nodes {
		indeg[n.ID] = 0
	}
	for _, e := range edges {
		indeg[e.TargetID]++
	}

	// Track which edges are still pending.
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
		// Mark consumed + decrement indeg of each target.
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

// appNewFn is overridable for tests; defaults to app.New.
var appNewFn = app.New
