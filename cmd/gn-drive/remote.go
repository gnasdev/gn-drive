package main

import (
	"context"
	"fmt"
	"text/tabwriter"

	"github.com/gnasdev/gn-drive/internal/app"
	"github.com/gnasdev/gn-drive/internal/logging"
	"github.com/spf13/cobra"
)

func newRemoteCmd() *cobra.Command {
	cmd := &cobra.Command{
		Use:   "remote [list|add|test|delete]",
		Short: "Manage rclone remotes",
		Long: `Manage rclone remotes in rclone.conf.

Examples:
  gn-drive remote list
  gn-drive remote add --name gdrive --type drive
  gn-drive remote test gdrive
  gn-drive remote delete gdrive`,
		Args: cobra.NoArgs,
		RunE: func(cmd *cobra.Command, args []string) error {
			cmd.Help()
			return nil
		},
	}

	cmd.AddCommand(
		newRemoteListCmd(),
		newRemoteAddCmd(),
		newRemoteTestCmd(),
		newRemoteDeleteCmd(),
	)
	return cmd
}

func newRemoteListCmd() *cobra.Command {
	return &cobra.Command{
		Use:   "list",
		Short: "List all remotes",
		RunE: func(cmd *cobra.Command, args []string) error {
			ctx := context.Background()
			a, err := app.New(ctx, app.Options{LogMode: logging.ModeForeground})
			if err != nil {
				return err
			}
			defer a.Close()

			remotes, err := a.Rclone.ListRemotes(ctx)
			if err != nil {
				return err
			}
			if len(remotes) == 0 {
				fmt.Println("No remotes configured. Use 'gn-drive remote add' to configure one.")
				return nil
			}
			w := tabwriter.NewWriter(cmd.OutOrStdout(), 0, 0, 2, ' ', 0)
			fmt.Fprintln(w, "NAME\tTYPE")
			fmt.Fprintln(w, "----\t----")
			for _, r := range remotes {
				fmt.Fprintf(w, "%s\t%s\n", r.Name, r.Type)
			}
			return w.Flush()
		},
	}
}

func newRemoteAddCmd() *cobra.Command {
	var (
		name, remoteType string
		configKVs        []string
	)
	cmd := &cobra.Command{
		Use:   "add",
		Short: "Add a new remote (non-interactive)",
		Long: `Add a new remote non-interactively. For OAuth-based providers (drive, onedrive, etc.),
use 'gn-drive run' to add via the web UI which handles the OAuth flow.

Example (local filesystem):
  gn-drive remote add --name localbk --type local`,
		RunE: func(cmd *cobra.Command, args []string) error {
			if name == "" || remoteType == "" {
				return fmt.Errorf("remote add: --name and --type are required")
			}
			ctx := context.Background()
			a, err := app.New(ctx, app.Options{LogMode: logging.ModeForeground})
			if err != nil {
				return err
			}
			defer a.Close()

			if err := a.Rclone.CreateRemote(ctx, name, remoteType, configKVs); err != nil {
				return err
			}
			fmt.Printf("✓ added remote %q (type=%s)\n", name, remoteType)
			return nil
		},
	}
	cmd.Flags().StringVar(&name, "name", "", "Remote name (required)")
	cmd.Flags().StringVar(&remoteType, "type", "", "Remote type: drive, s3, local, sftp, ... (required)")
	cmd.Flags().StringSliceVar(&configKVs, "config", nil, "Config k=v pairs (repeatable)")
	return cmd
}

func newRemoteTestCmd() *cobra.Command {
	return &cobra.Command{
		Use:   "test [name]",
		Short: "Test a remote connection",
		Args:  cobra.ExactArgs(1),
		RunE: func(cmd *cobra.Command, args []string) error {
			name := args[0]
			ctx := context.Background()
			a, err := app.New(ctx, app.Options{LogMode: logging.ModeForeground})
			if err != nil {
				return err
			}
			defer a.Close()

			fmt.Printf("Testing remote %q... ", name)
			if err := a.Rclone.TestRemote(ctx, name); err != nil {
				fmt.Println("✗ FAILED")
				return err
			}
			fmt.Println("✓ OK")
			return nil
		},
	}
}

func newRemoteDeleteCmd() *cobra.Command {
	return &cobra.Command{
		Use:   "delete [name]",
		Short: "Delete a remote",
		Args:  cobra.ExactArgs(1),
		RunE: func(cmd *cobra.Command, args []string) error {
			name := args[0]
			ctx := context.Background()
			a, err := app.New(ctx, app.Options{LogMode: logging.ModeForeground})
			if err != nil {
				return err
			}
			defer a.Close()

			if err := a.Rclone.DeleteRemote(ctx, name); err != nil {
				return err
			}
			fmt.Printf("✓ deleted remote %q\n", name)
			return nil
		},
	}
}
