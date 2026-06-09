package main

import (
	"fmt"
	"runtime"
	"runtime/debug"

	"github.com/spf13/cobra"
)

func newVersionCmd() *cobra.Command {
	return &cobra.Command{
		Use:   "version",
		Short: "Print version information",
		RunE: func(cmd *cobra.Command, args []string) error {
			fmt.Printf("gn-drive %s (commit=%s, go=%s)\n", Version, Commit, runtime.Version())
			bi, ok := debug.ReadBuildInfo()
			if !ok {
				return nil
			}
			fmt.Printf("  path:   %s\n", bi.Path)
			for _, s := range bi.Settings {
				if s.Key == "vcs.modified" || s.Key == "vcs.revision" {
					fmt.Printf("  %s:   %s\n", s.Key, s.Value)
				}
			}
			return nil
		},
	}
}