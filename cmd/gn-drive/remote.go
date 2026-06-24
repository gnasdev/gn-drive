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
			a, err := appNewFn(ctx, app.Options{LogMode: logging.ModeForeground})
			if err != nil {
				return err
			}
			defer a.Close()
			return runRemoteList(ctx, a, cmd)
		},
	}
}

// runRemoteList is the testable inner work of newRemoteListCmd.
func runRemoteList(ctx context.Context, a *app.App, cmd *cobra.Command) error {
	remotes, err := a.Rclone.ListRemotes(ctx)
	if err != nil {
		return err
	}
	if len(remotes) == 0 {
		fmt.Fprintln(cmd.OutOrStdout(), "No remotes configured. Use 'gn-drive remote add' to configure one.")
		return nil
	}
	w := tabwriter.NewWriter(cmd.OutOrStdout(), 0, 0, 2, ' ', 0)
	fmt.Fprintln(w, "NAME\tTYPE")
	fmt.Fprintln(w, "----\t----")
	for _, r := range remotes {
		fmt.Fprintf(w, "%s\t%s\n", r.Name, r.Type)
	}
	return w.Flush()
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
			a, err := appNewFn(ctx, app.Options{LogMode: logging.ModeForeground})
			if err != nil {
				return err
			}
			defer a.Close()
			return runRemoteAdd(ctx, a, name, remoteType, configKVs, cmd)
		},
	}
	cmd.Flags().StringVar(&name, "name", "", "Remote name (required)")
	cmd.Flags().StringVar(&remoteType, "type", "", "Remote type: drive, s3, local, sftp, ... (required)")
	cmd.Flags().StringSliceVar(&configKVs, "config", nil, "Config k=v pairs (repeatable)")
	return cmd
}

// runRemoteAdd is the testable inner work of newRemoteAddCmd.
func runRemoteAdd(ctx context.Context, a *app.App, name, remoteType string, configKVs []string, cmd *cobra.Command) error {
	if err := a.Rclone.CreateRemote(ctx, name, remoteType, configKVs); err != nil {
		return err
	}
	fmt.Fprintf(cmd.OutOrStdout(), "✓ added remote %q (type=%s)\n", name, remoteType)
	return nil
}

func newRemoteTestCmd() *cobra.Command {
	return &cobra.Command{
		Use:   "test [name]",
		Short: "Test a remote connection",
		Args:  cobra.ExactArgs(1),
		RunE: func(cmd *cobra.Command, args []string) error {
			name := args[0]
			ctx := context.Background()
			a, err := appNewFn(ctx, app.Options{LogMode: logging.ModeForeground})
			if err != nil {
				return err
			}
			defer a.Close()
			return runRemoteTest(ctx, a, name, cmd)
		},
	}
}

// runRemoteTest is the testable inner work of newRemoteTestCmd.
func runRemoteTest(ctx context.Context, a *app.App, name string, cmd *cobra.Command) error {
	fmt.Fprintf(cmd.OutOrStdout(), "Testing remote %q... ", name)
	if err := a.Rclone.TestRemote(ctx, name); err != nil {
		fmt.Fprintln(cmd.OutOrStdout(), "✗ FAILED")
		return err
	}
	fmt.Fprintln(cmd.OutOrStdout(), "✓ OK")
	return nil
}

func newRemoteDeleteCmd() *cobra.Command {
	return &cobra.Command{
		Use:   "delete [name]",
		Short: "Delete a remote",
		Args:  cobra.ExactArgs(1),
		RunE: func(cmd *cobra.Command, args []string) error {
			name := args[0]
			ctx := context.Background()
			a, err := appNewFn(ctx, app.Options{LogMode: logging.ModeForeground})
			if err != nil {
				return err
			}
			defer a.Close()
			return runRemoteDelete(ctx, a, name, cmd)
		},
	}
}

// runRemoteDelete is the testable inner work of newRemoteDeleteCmd.
func runRemoteDelete(ctx context.Context, a *app.App, name string, cmd *cobra.Command) error {
	if err := a.Rclone.DeleteRemote(ctx, name); err != nil {
		return err
	}
	fmt.Fprintf(cmd.OutOrStdout(), "✓ deleted remote %q\n", name)
	return nil
}
