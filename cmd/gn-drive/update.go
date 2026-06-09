package main

import (
	"fmt"

	"github.com/spf13/cobra"
)

func newUpdateCmd() *cobra.Command {
	cmd := &cobra.Command{
		Use:   "self-update",
		Short: "Download and apply the latest release",
		Long: `Check GitHub Releases for a newer version and update the binary.

  1. Fetches the latest release from GitHub.
  2. Downloads the binary for your platform.
  3. Verifies SHA256 checksum.
  4. Replaces the current binary atomically.
  5. Prints restart instructions.

Requires write access to the directory containing the binary.`,
		RunE: func(cmd *cobra.Command, args []string) error {
			fmt.Println("gn-drive self-update")
			fmt.Println("TODO(phase4): fetch GitHub releases, download, verify SHA256, atomic swap")
			fmt.Println("Current version:", Version)
			return nil
		},
	}
	return cmd
}