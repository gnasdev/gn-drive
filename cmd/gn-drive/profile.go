package main

import (
	"context"
	"fmt"
	"text/tabwriter"

	"github.com/gnasdev/gn-drive/internal/app"
	"github.com/gnasdev/gn-drive/internal/logging"
	"github.com/gnasdev/gn-drive/internal/store"
	"github.com/spf13/cobra"
)

func newProfileCmd() *cobra.Command {
	cmd := &cobra.Command{
		Use:   "profile [list|add|edit|delete]",
		Short: "Manage sync profiles",
		Long: `Manage sync profiles stored in the gn-drive database.

Examples:
  gn-drive profile list
  gn-drive profile add --name backup --from gdrive: --to gdrive: --direction pull
  gn-drive profile delete backup`,
		Args: cobra.NoArgs,
		RunE: func(cmd *cobra.Command, args []string) error {
			cmd.Help()
			return nil
		},
	}

	cmd.AddCommand(
		newProfileListCmd(),
		newProfileAddCmd(),
		newProfileDeleteCmd(),
	)
	return cmd
}

func newProfileListCmd() *cobra.Command {
	return &cobra.Command{
		Use:   "list",
		Short: "List all profiles",
		RunE: func(cmd *cobra.Command, args []string) error {
			ctx := context.Background()
			a, err := appNewFn(ctx, app.Options{LogMode: logging.ModeForeground})
			if err != nil {
				return err
			}
			defer a.Close()
			return runProfileList(cmd, a)
		},
	}
}

// runProfileList is the testable inner work of newProfileListCmd.
func runProfileList(cmd *cobra.Command, a *app.App) error {
	ctx := context.Background()
	profiles, err := a.Store.Profiles().List(ctx)
	if err != nil {
		return err
	}
	if len(profiles) == 0 {
		fmt.Fprintln(cmd.OutOrStdout(), "No profiles configured. Use 'gn-drive profile add' to manage via UI.")
		return nil
	}

	w := tabwriter.NewWriter(cmd.OutOrStdout(), 0, 0, 2, ' ', 0)
	fmt.Fprintln(w, "NAME\tFROM\tTO\tPARALLEL\tBANDWIDTH\tDRY-RUN")
	fmt.Fprintln(w, "----\t----\t--\t--------\t---------\t-------")
	for _, p := range profiles {
		bw := ""
		if p.Bandwidth > 0 {
			bw = fmt.Sprintf("%dM", p.Bandwidth)
		}
		fmt.Fprintf(w, "%s\t%s\t%s\t%d\t%s\t%v\n",
			p.Name, truncate(p.From, 40), truncate(p.To, 40), p.Parallel, bw, p.DryRun)
	}
	return w.Flush()
}

func newProfileAddCmd() *cobra.Command {
	var (
		name, from, to string
		parallel       int
		bandwidth      int
		dryRun         bool
	)
	cmd := &cobra.Command{
		Use:   "add",
		Short: "Add a new profile",
		RunE: func(cmd *cobra.Command, args []string) error {
			if name == "" || from == "" || to == "" {
				return fmt.Errorf("profile add: --name, --from, --to are required")
			}
			ctx := context.Background()
			a, err := appNewFn(ctx, app.Options{LogMode: logging.ModeForeground})
			if err != nil {
				return err
			}
			defer a.Close()
			return runProfileAdd(ctx, a, name, from, to, parallel, bandwidth, dryRun, cmd)
		},
	}
	cmd.Flags().StringVar(&name, "name", "", "Profile name (required)")
	cmd.Flags().StringVar(&from, "from", "", "Source remote:path (required)")
	cmd.Flags().StringVar(&to, "to", "", "Destination remote:path (required)")
	cmd.Flags().IntVar(&parallel, "parallel", 4, "Concurrent transfers")
	cmd.Flags().IntVar(&bandwidth, "bandwidth", 0, "Bandwidth limit in MB/s (0=unlimited)")
	cmd.Flags().BoolVar(&dryRun, "dry-run", false, "Default to dry-run mode")
	return cmd
}

// runProfileAdd is the testable inner work of newProfileAddCmd.
func runProfileAdd(ctx context.Context, a *app.App, name, from, to string, parallel, bandwidth int, dryRun bool, cmd *cobra.Command) error {
	p := &store.Profile{
		Name:      name,
		From:      from,
		To:        to,
		Parallel:  parallel,
		Bandwidth: bandwidth,
		DryRun:    dryRun,
	}
	if err := a.Store.Profiles().Save(ctx, p); err != nil {
		return err
	}
	fmt.Fprintf(cmd.OutOrStdout(), "✓ added profile %q\n", name)
	return nil
}

func newProfileDeleteCmd() *cobra.Command {
	return &cobra.Command{
		Use:   "delete [name]",
		Short: "Delete a profile",
		Args:  cobra.ExactArgs(1),
		RunE: func(cmd *cobra.Command, args []string) error {
			name := args[0]
			ctx := context.Background()
			a, err := appNewFn(ctx, app.Options{LogMode: logging.ModeForeground})
			if err != nil {
				return err
			}
			defer a.Close()
			return runProfileDelete(ctx, a, name, cmd)
		},
	}
}

// runProfileDelete is the testable inner work of newProfileDeleteCmd.
func runProfileDelete(ctx context.Context, a *app.App, name string, cmd *cobra.Command) error {
	if err := a.Store.Profiles().Delete(ctx, name); err != nil {
		return err
	}
	fmt.Fprintf(cmd.OutOrStdout(), "✓ deleted profile %q\n", name)
	return nil
}

func truncate(s string, n int) string {
	if len(s) <= n {
		return s
	}
	return s[:n-1] + "…"
}

func humanBytes(n int64) string {
	const k = 1024
	switch {
	case n < k:
		return fmt.Sprintf("%dB", n)
	case n < k*k:
		return fmt.Sprintf("%.1fK", float64(n)/k)
	case n < k*k*k:
		return fmt.Sprintf("%.1fM", float64(n)/(k*k))
	default:
		return fmt.Sprintf("%.2fG", float64(n)/(k*k*k))
	}
}