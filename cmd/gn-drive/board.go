package main

import (
	"context"
	"fmt"

	"github.com/gnasdev/gn-drive/internal/app"
	"github.com/gnasdev/gn-drive/internal/logging"
	"github.com/spf13/cobra"
)

func newBoardCmd() *cobra.Command {
	cmd := &cobra.Command{
		Use:   "board [board-id]",
		Short: "Execute a board DAG",
		Long: `Execute all edges in a board DAG in topological order.
Each edge runs its associated sync profile in sequence.

Example:
  gn-drive board my-daily-backup`,
		Args: cobra.ExactArgs(1),
		RunE: func(cmd *cobra.Command, args []string) error {
			boardID := args[0]
			ctx := context.Background()
			a, err := app.New(ctx, app.Options{LogMode: logging.ModeForeground})
			if err != nil {
				return err
			}
			defer a.Close()

			// Phase 2: load board metadata only; full DAG execution is Phase 3.
			boards, err := a.Store.Boards().List(ctx)
			if err != nil {
				return fmt.Errorf("board: list: %w", err)
			}
			found := false
			for _, b := range boards {
				if b.ID == boardID || b.Name == boardID {
					fmt.Printf("Board: %s (%s) — %s\n", b.Name, b.ID, b.Description)
					fmt.Printf("  created: %s, updated: %s\n", b.CreatedAt, b.UpdatedAt)
					found = true
					break
				}
			}
			if !found {
				return fmt.Errorf("board: %q not found (use 'gn-drive run' for full UI)", boardID)
			}
			fmt.Println("TODO(phase3): execute DAG edges in topological order")
			return nil
		},
	}
	return cmd
}
