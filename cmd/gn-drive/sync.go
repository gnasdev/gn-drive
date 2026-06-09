package main

import (
	"context"
	"fmt"
	"strconv"

	"github.com/gnasdev/gn-drive/internal/app"
	"github.com/gnasdev/gn-drive/internal/logging"
	"github.com/gnasdev/gn-drive/internal/rclone"
	"github.com/gnasdev/gn-drive/internal/store"
	"github.com/spf13/cobra"
)

func newSyncCmd() *cobra.Command {
	var (
		profileName string
		dryRun      bool
	)
	cmd := &cobra.Command{
		Use:   "sync [pull|push|bi|bi-resync|dry-run]",
		Short: "Run a one-shot sync operation",
		Long: `Run a one-shot sync between two remotes without starting the web UI.

This command is useful for cron jobs, scripts, or CI pipelines.
It does not start the sync engine or web server.

Examples:
  gn-drive sync pull --profile backup
  gn-drive sync push --profile photos
  gn-drive sync bi --profile workspace
  gn-drive sync dry-run --profile backup  # preview only`,
		Args: cobra.ExactArgs(1),
		RunE: func(cmd *cobra.Command, args []string) error {
			action := rclone.Action(args[0])
			if profileName == "" {
				fatal("sync: --profile is required")
			}
			if dryRun {
				action = rclone.ActionDryRun
			}

			ctx := context.Background()
			a, err := app.New(ctx, app.Options{LogMode: logging.ModeForeground})
			if err != nil {
				return err
			}
			defer a.Close()

			p, err := a.Store.Profiles().Get(ctx, profileName)
			if err != nil {
				return fmt.Errorf("sync: profile %q: %w", profileName, err)
			}
			fmt.Printf("→ %s sync: %s → %s (action=%s)\n", p.Name, p.From, p.To, action)
			if p.DryRun {
				fmt.Println("  [profile has dry_run=true, no actual changes will be made]")
			}

			lastProgress := rclone.Stats{}
			res, err := a.Rclone.Sync(ctx, syncConfigForProfile(p, action), func(s rclone.Stats) {
				if s.Bytes != lastProgress.Bytes || s.Files != lastProgress.Files {
					fmt.Printf("\r  %s / %s | %d files | %d errors   ",
						humanBytes(s.Bytes), humanBytes(s.BytesTotal), s.Files, s.Errors)
					lastProgress = s
				}
			})
			fmt.Println()
			if err != nil {
				fmt.Printf("✗ sync failed: %v\n", err)
				return err
			}
			fmt.Printf("✓ sync completed in %ds — %s transferred, %d errors\n",
				res.EndedAt-res.StartedAt, humanBytes(res.Stats.Bytes), res.Stats.Errors)
			return nil
		},
	}
	cmd.Flags().StringVar(&profileName, "profile", "", "Profile name to sync")
	cmd.Flags().BoolVar(&dryRun, "dry-run", false, "Preview only — do not change files")
	cmd.MarkFlagRequired("profile")
	return cmd
}

func syncConfigForProfile(p *store.Profile, action rclone.Action) rclone.SyncConfig {
	return rclone.SyncConfig{
		Action: action,
		Source: p.From,
		Dest:   p.To,
		Profile: &rclone.ProfileFlags{
			Bandwidth:        humanBandwidth(p.Bandwidth),
			Transfers:        p.Parallel,
			TpsLimit:         tpsLimit(p),
			MinAge:           p.MinAge,
			MaxAge:           p.MaxAge,
			MinSize:          p.MinSize,
			MaxSize:          p.MaxSize,
			ExcludeIfPresent: p.ExcludeIfPresent,
			MaxDelete:        intOrZero(p.MaxDelete),
			DryRun:           p.DryRun,
		},
	}
}

func tpsLimit(p *store.Profile) float64 {
	if p.TpsLimit == nil {
		return 0
	}
	return *p.TpsLimit
}

func intOrZero(p *int) int {
	if p == nil {
		return 0
	}
	return *p
}

func humanBandwidth(mbps int) string {
	if mbps <= 0 {
		return ""
	}
	return strconv.Itoa(mbps) + "M"
}
