// Package main is the entry point for the gn-drive CLI.
package main

import (
	"fmt"
	"os"

	"github.com/spf13/cobra"
)

// Build-time ldflags — set by the build system.
var (
	Version = "dev"
	Commit  = "unknown"
)

// osExit is overridable for tests.
var osExit = os.Exit

func main() {
	root := &cobra.Command{
		Use:   "gn-drive",
		Short: "GN Drive — local-only sync engine with web UI",
		Long: `gn-drive is a single-process CLI that runs a sync engine (rclone)
and a Vue 3 web UI in the same process, serving on loopback only.

Subcommands:
  run          Start foreground or service mode
  service      Install, uninstall, start, stop, or check service status
  sync         One-shot sync (pull, push, bi, bi-resync)
  board        Execute a board DAG
  profile      Manage sync profiles
  remote       Manage rclone remotes
  self-update  Download and apply updates from GitHub Releases
  version      Print version info
  doctor       Diagnose environment and configuration
  completion   Generate shell completion scripts`,
		SilenceUsage: true,
	}

	root.AddCommand(
		newRunCmd(),
		newServiceCmd(),
		newSyncCmd(),
		newBoardCmd(),
		newProfileCmd(),
		newRemoteCmd(),
		newUpdateCmd(),
		newVersionCmd(),
		newDoctorCmd(),
		newCompletionCmd(),
	)

	if err := root.Execute(); err != nil {
		osExit(1)
	}
}

func fatal(format string, args ...any) {
	fmt.Fprintf(os.Stderr, "fatal: "+format+"\n", args...)
	osExit(1)
}