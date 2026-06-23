package main

import (
	"context"
	"errors"
	"fmt"
	"os"
	"time"

	"github.com/gnasdev/gn-drive/internal/selfupdate"
	"github.com/spf13/cobra"
)

func newUpdateCmd() *cobra.Command {
	var (
		owner     string
		repo      string
		force     bool
		checkOnly bool
	)
	cmd := &cobra.Command{
		Use:   "self-update",
		Short: "Download and apply the latest release",
		Long: `Check GitHub Releases for a newer version and update the binary in place.

  1. Fetches the latest release from GitHub.
  2. Compares the tag with the current version.
  3. Downloads the binary archive for your platform.
  4. Verifies SHA256 checksum against the sidecar asset.
  5. Atomically replaces the running binary (kept as <bin>.bak on POSIX).
  6. Prints restart instructions.

The command must have write access to the directory containing the binary.
Set GITHUB_TOKEN to avoid GitHub's anonymous rate limit on the API.`,
		RunE: func(cmd *cobra.Command, args []string) error {
			ctx, cancel := context.WithTimeout(cmd.Context(), 5*time.Minute)
			defer cancel()

			if checkOnly {
				cur, latest, err := selfupdate.Check(ctx, selfupdate.Options{
					RepoOwner:      owner,
					RepoName:       repo,
					CurrentVersion: Version,
				})
				if err != nil {
					return err
				}
				fmt.Printf("current=%s latest=%s\n", cur, latest)
				if cur == latest {
					fmt.Println("✓ already on latest version")
					return nil
				}
				fmt.Println("↑ update available — run 'gn-drive self-update' to apply")
				return nil
			}

			fmt.Printf("gn-drive self-update (current=%s)\n", Version)
			res, err := selfupdate.Update(ctx, selfupdate.Options{
				RepoOwner:      owner,
				RepoName:       repo,
				CurrentVersion: Version,
				Force:          force,
				Stdout:         os.Stdout,
			})
			if errors.Is(err, selfupdate.ErrAlreadyUpToDate) {
				fmt.Println("✓ already on latest version")
				return nil
			}
			if err != nil {
				return fmt.Errorf("self-update failed: %w", err)
			}
			if res == nil {
				return nil
			}
			fmt.Printf("✓ updated %s → %s\n", res.OldVersion, res.NewVersion)
			fmt.Printf("  binary: %s\n", res.BinaryPath)
			if res.RestartHint != "" {
				fmt.Printf("  next:   %s\n", res.RestartHint)
			}
			return nil
		},
	}
	cmd.Flags().StringVar(&owner, "repo-owner", "", "GitHub repo owner (default: gnasdev)")
	cmd.Flags().StringVar(&repo, "repo", "", "GitHub repo name (default: gn-drive)")
	cmd.Flags().BoolVar(&force, "force", false, "Reinstall even if version matches")
	cmd.Flags().BoolVar(&checkOnly, "check", false, "Only check for updates; do not download or install")
	return cmd
}
