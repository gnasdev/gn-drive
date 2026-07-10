package main

import (
	"context"
	"fmt"

	"github.com/gnasdev/gn-drive/internal/app"
	"github.com/gnasdev/gn-drive/internal/boardengine"
	"github.com/gnasdev/gn-drive/internal/logging"
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
// It always builds a boardengine with newSyncExecutor so tests can inject stubs.
func runBoard(ctx context.Context, a *app.App, boardID string, stopOnError bool, concurrency int, cmd *cobra.Command) error {
	_ = concurrency
	eng := boardengine.New(boardengine.Options{
		Store:  a.Store,
		Rclone: newSyncExecutor(a),
		Bus:    a.EventBus,
		Log:    a.Log,
	})

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

	err = eng.ExecuteSync(ctx, boardID, stopOnError, func(layer, idx, total int, edge store.BoardEdge, src, dst store.BoardNode, edgeErr error) {
		fmt.Fprintf(cmd.OutOrStdout(), "  │ [%d/%d] layer=%d %s — %s : %s → %s\n",
			idx, total, layer+1, edge.ID, edge.Action, src.Label, dst.Label)
		if edgeErr != nil {
			fmt.Fprintf(cmd.OutOrStdout(), "  │   ✗ edge %s failed: %v\n", edge.ID, edgeErr)
			return
		}
		fmt.Fprintf(cmd.OutOrStdout(), "  │   ✓ edge %s ok\n", edge.ID)
	})
	if err != nil {
		return err
	}
	fmt.Fprintf(cmd.OutOrStdout(), "✓ board %q executed successfully\n", b.ID)
	return nil
}

// Compatibility wrappers for existing cmd_test.go tests.

func runEdge(ctx context.Context, a *app.App, edge store.BoardEdge, src, dst store.BoardNode) error {
	return runEdgeWith(ctx, newSyncExecutor(a), edge, src, dst)
}

func runEdgeWith(ctx context.Context, exec boardengine.SyncExecutor, edge store.BoardEdge, src, dst store.BoardNode) error {
	return boardengine.RunEdge(ctx, exec, edge, src, dst)
}

func topoLayers(nodes []store.BoardNode, edges []store.BoardEdge) ([][]store.BoardEdge, error) {
	return boardengine.TopoLayers(nodes, edges)
}

// syncExecutor is an alias so existing cmd_test.go can reference the type name.
type syncExecutor = boardengine.SyncExecutor

var newSyncExecutor = func(a *app.App) syncExecutor {
	return a.Rclone
}

// appNewFn is overridable for tests; defaults to app.New.
var appNewFn = app.New
