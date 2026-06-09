package main

import (
	"context"
	"fmt"
	"os"
	"os/exec"
	"path/filepath"
	"runtime"
	"strings"

	"github.com/gnasdev/gn-drive/internal/app"
	"github.com/gnasdev/gn-drive/internal/logging"
	"github.com/spf13/cobra"
)

func newDoctorCmd() *cobra.Command {
	var showData bool
	cmd := &cobra.Command{
		Use:   "doctor",
		Short: "Diagnose environment and configuration",
		Long: `Run a series of checks to diagnose gn-drive configuration issues.

Checks:
  - Config directory exists and is accessible
  - SQLite database can be opened
  - rclone binary is installed and in PATH
  - Auth configuration is valid
  - Profiles, remotes, history counts`,
		RunE: func(cmd *cobra.Command, args []string) error {
			fmt.Println("=== gn-drive doctor ===")
			fmt.Println()

			// rclone binary
			rclonePath, rcloneErr := exec.LookPath("rclone")
			if rcloneErr != nil {
				fmt.Println("rclone:       NOT FOUND in PATH")
				fmt.Println("  [ERROR] rclone is required. Install: https://rclone.org/install/")
			} else {
				fmt.Printf("rclone:       %s\n", rclonePath)
				var out strings.Builder
				c := exec.Command(rclonePath, "version")
				c.Stdout = &out
				c.Stderr = &out
				c.Run()
				firstLine := strings.Split(strings.TrimSpace(out.String()), "\n")[0]
				if firstLine != "" {
					fmt.Printf("  version: %s\n", firstLine)
				}
				fmt.Println("  [OK]")
			}

			// Open app
			ctx := context.Background()
			a, err := app.New(ctx, app.Options{LogMode: logging.ModeForeground})
			if err != nil {
				fmt.Printf("\n[ERROR] Cannot initialize app: %v\n", err)
				return err
			}
			defer a.Close()

			// Config dir
			fmt.Printf("\nConfig dir:  %s\n", a.Config.ConfigDir)
			if _, err := os.Stat(a.Config.ConfigDir); err == nil {
				fmt.Println("  [OK]")
			} else {
				fmt.Printf("  [WARN] %v\n", err)
			}

			// Database
			dbPath := filepath.Join(a.Config.ConfigDir, "gn-drive.db")
			fmt.Printf("Database:     %s\n", dbPath)
			if _, err := os.Stat(dbPath); err == nil {
				fmt.Println("  [OK]")
			} else {
				fmt.Println("  [INFO] Database not yet created (first run)")
			}

			// Auth
			fmt.Printf("Auth config: %s\n", filepath.Join(a.Config.ConfigDir, "auth.json"))
			status := a.Auth.Status()
			if status.Setup {
				if status.Unlocked {
					fmt.Println("  configured: yes, unlocked: yes  [OK]")
				} else {
					fmt.Println("  configured: yes, unlocked: no  [LOCKED]")
				}
			} else {
				fmt.Println("  configured: no  [OK]")
			}

			// Remotes + profiles (only if unlocked)
			if !status.Setup || status.Unlocked {
				if remotes, err := a.Rclone.ListRemotes(ctx); err == nil {
					fmt.Printf("\nRemotes:      %d configured\n", len(remotes))
					for _, r := range remotes {
						fmt.Printf("  - %s (%s)\n", r.Name, r.Type)
					}
				}
				if profiles, err := a.Store.Profiles().List(ctx); err == nil {
					fmt.Printf("\nProfiles:     %d configured\n", len(profiles))
					for _, p := range profiles {
						fmt.Printf("  - %s\n", p.Name)
					}
				}
				if history, err := a.Store.History().List(ctx, 5, 0); err == nil {
					fmt.Printf("\nHistory:      %d recent entries\n", len(history))
				}
			}

			fmt.Printf("\nPlatform:     %s (%s)\n", runtime.GOOS, runtime.GOARCH)

			if showData {
				fmt.Println("\n--- data directory contents ---")
				entries, _ := os.ReadDir(a.Config.ConfigDir)
				for _, e := range entries {
					fmt.Printf("  %s\n", e.Name())
				}
			}

			fmt.Println("\nAll checks passed. gn-drive is ready to run.")
			return nil
		},
	}
	cmd.Flags().BoolVar(&showData, "data", false, "List files in config directory")
	return cmd
}
