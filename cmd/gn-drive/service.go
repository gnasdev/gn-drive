package main

import (
	"fmt"
	"strconv"
	"time"

	"github.com/gnasdev/gn-drive/internal/config"
	"github.com/gnasdev/gn-drive/internal/service"
	"github.com/spf13/cobra"
)

func newServiceCmd() *cobra.Command {
	cmd := &cobra.Command{
		Use:   "service [install|uninstall|start|stop|status|restart]",
		Short: "Manage the gn-drive background service",
		Long: `Install, uninstall, start, stop, restart, or check the status of the
gn-drive background service.

Platform support:
  Linux        systemd user-level (default) or system-level (--system)
  macOS        launchd LaunchAgent
  Windows      SCM (Service Control Manager)

Examples:
  gn-drive service install                    # Install for current user
  gn-drive service install --system           # Install system-wide (requires sudo)
  gn-drive service start
  gn-drive service stop
  gn-drive service status
  gn-drive service restart`,
		Args: cobra.ExactArgs(1),
		RunE: func(cmd *cobra.Command, args []string) error {
			sub := args[0]
			system, _ := cmd.Flags().GetBool("system")
			scope := service.ScopeUser
			if system {
				scope = service.ScopeSystem
			}
			mgr, err := service.NewManager()
			if err != nil {
				return err
			}
			spec := service.DefaultSpec(scope)
			// Refresh ExecPath/ConfigDir with detected values.
			spec.ConfigDir = config.Detect().ConfigDir

			switch sub {
			case "install":
				return runServiceInstall(mgr, spec)
			case "uninstall":
				return runServiceUninstall(mgr, spec)
			case "start":
				return runServiceStart(mgr, spec)
			case "stop":
				return runServiceStop(mgr, spec)
			case "status":
				return runServiceStatus(mgr, spec)
			case "restart":
				return runServiceRestart(mgr, spec)
			default:
				return fmt.Errorf("unknown service action: %q (want install|uninstall|start|stop|status|restart)", sub)
			}
		},
	}

	cmd.Flags().Bool("system", false, "Install as a system-level service (requires sudo)")
	return cmd
}

func runServiceInstall(mgr service.Manager, spec service.Spec) error {
	if spec.Scope == service.ScopeSystem {
		fmt.Println("Note: system-level install requires elevated privileges (sudo).")
	}
	fmt.Printf("Installing gn-drive service (%s, %s)...\n", service.Platform(), spec.Scope)
	if err := mgr.Install(spec); err != nil {
		return fmt.Errorf("install: %w", err)
	}
	fmt.Printf("✓ installed.\n")
	fmt.Println()
	fmt.Println("Status:")
	if err := runServiceStatus(mgr, spec); err != nil {
		fmt.Println("  (could not read status: " + err.Error() + ")")
	}
	return nil
}

func runServiceUninstall(mgr service.Manager, spec service.Spec) error {
	fmt.Printf("Uninstalling gn-drive service (%s, %s)...\n", service.Platform(), spec.Scope)
	if err := mgr.Uninstall(spec); err != nil {
		return fmt.Errorf("uninstall: %w", err)
	}
	fmt.Println("✓ uninstalled.")
	return nil
}

func runServiceStart(mgr service.Manager, spec service.Spec) error {
	fmt.Printf("Starting gn-drive service...\n")
	if err := mgr.Start(spec); err != nil {
		return fmt.Errorf("start: %w", err)
	}
	fmt.Println("✓ started.")
	return nil
}

func runServiceStop(mgr service.Manager, spec service.Spec) error {
	fmt.Printf("Stopping gn-drive service...\n")
	if err := mgr.Stop(spec); err != nil {
		return fmt.Errorf("stop: %w", err)
	}
	fmt.Println("✓ stopped.")
	return nil
}

func runServiceStatus(mgr service.Manager, spec service.Spec) error {
	installed, err := mgr.IsInstalled(spec)
	if err != nil {
		return err
	}
	if !installed {
		fmt.Println("Service: not installed.")
		fmt.Println()
		fmt.Println("To install:")
		fmt.Printf("  gn-drive service install%s\n", scopeFlag(spec.Scope))
		return nil
	}

	st, err := mgr.Status(spec)
	if err != nil {
		// Status check failed but service is installed; report what we can.
		fmt.Println("Service: installed (status check failed: " + err.Error() + ")")
		return nil
	}

	fmt.Println("Service:")
	fmt.Printf("  Mode:     %s\n", st.Mode)
	fmt.Printf("  Scope:    %s\n", st.Scope)
	fmt.Printf("  Platform: %s\n", service.Platform())
	if st.Running {
		fmt.Printf("  Running:  yes (pid %d)\n", st.PID)
	} else {
		fmt.Println("  Running:  no")
	}

	// Read health file for richer info.
	health, herr := service.ReadHealth(spec.ConfigDir)
	if herr == nil {
		fmt.Println()
		fmt.Println("Health:")
		if !health.StartedAt.IsZero() {
			fmt.Printf("  Started:        %s\n", health.StartedAt.Format(time.RFC3339))
		}
		if !health.LastHeartbeat.IsZero() {
			fmt.Printf("  Last heartbeat: %s\n", health.LastHeartbeat.Format(time.RFC3339))
			if health.IsStale(60 * time.Second) {
				fmt.Println("  ⚠ heartbeat stale (>60s old) — service may be unresponsive")
			}
		}
		if health.WebPort > 0 {
			fmt.Printf("  Web port:       %d\n", health.WebPort)
		}
		if health.Uptime() > 0 {
			fmt.Printf("  Uptime:         %s\n", health.Uptime().Round(time.Second))
		}
		if health.LastError != "" {
			fmt.Printf("  Last error:     %s\n", health.LastError)
		}
		if !health.LastSyncAt.IsZero() {
			fmt.Printf("  Last sync:      %s\n", health.LastSyncAt.Format(time.RFC3339))
		}
		if !health.NextScheduleAt.IsZero() {
			fmt.Printf("  Next schedule:  %s\n", health.NextScheduleAt.Format(time.RFC3339))
		}
		if len(health.ActiveTasks) > 0 {
			fmt.Printf("  Active tasks:   %s\n", joinTasks(health.ActiveTasks))
		}
	} else if herr != service.ErrNotInstalled {
		fmt.Println()
		fmt.Println("Health: (could not read: " + herr.Error() + ")")
	}
	return nil
}

func runServiceRestart(mgr service.Manager, spec service.Spec) error {
	fmt.Println("Restarting gn-drive service...")
	if err := mgr.Restart(spec); err != nil {
		return fmt.Errorf("restart: %w", err)
	}
	fmt.Println("✓ restarted.")
	return nil
}

func scopeFlag(s service.Scope) string {
	if s == service.ScopeSystem {
		return " --system"
	}
	return ""
}

func joinTasks(tasks []string) string {
	if len(tasks) == 0 {
		return "(none)"
	}
	out := ""
	for i, t := range tasks {
		if i > 0 {
			out += ", "
		}
		out += strconv.Quote(t)
	}
	return out
}
