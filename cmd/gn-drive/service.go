package main

import (
	"fmt"
	"io"
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
			return runServiceAction(cmd, args[0], cmd.OutOrStdout())
		},
	}

	cmd.Flags().Bool("system", false, "Install as a system-level service (requires sudo)")
	return cmd
}

// runServiceAction is the testable inner of newServiceCmd.RunE. It builds
// the spec + manager and dispatches to the right run* helper.
func runServiceAction(cmd *cobra.Command, sub string, out io.Writer) error {
	system, _ := cmd.Flags().GetBool("system")
	scope := service.ScopeUser
	if system {
		scope = service.ScopeSystem
	}
	mgr, err := newServiceManager()
	if err != nil {
		return err
	}
	spec := service.DefaultSpec(scope)
	spec.ConfigDir = config.Detect().ConfigDir

	switch sub {
	case "install":
		return runServiceInstall(mgr, spec, out)
	case "uninstall":
		return runServiceUninstall(mgr, spec, out)
	case "start":
		return runServiceStart(mgr, spec, out)
	case "stop":
		return runServiceStop(mgr, spec, out)
	case "status":
		return runServiceStatus(mgr, spec, out)
	case "restart":
		return runServiceRestart(mgr, spec, out)
	default:
		return fmt.Errorf("unknown service action: %q (want install|uninstall|start|stop|status|restart)", sub)
	}
}

// newServiceManager is the testable inner of service.NewManager().
var newServiceManager = func() (service.Manager, error) {
	return service.NewManager()
}

func runServiceInstall(mgr service.Manager, spec service.Spec, out io.Writer) error {
	if spec.Scope == service.ScopeSystem {
		fmt.Fprintln(out, "Note: system-level install requires elevated privileges (sudo).")
	}
	fmt.Fprintf(out, "Installing gn-drive service (%s, %s)...\n", service.Platform(), spec.Scope)
	if err := mgr.Install(spec); err != nil {
		return fmt.Errorf("install: %w", err)
	}
	fmt.Fprintln(out, "✓ installed.")
	fmt.Fprintln(out)
	fmt.Fprintln(out, "Status:")
	if err := runServiceStatus(mgr, spec, out); err != nil {
		fmt.Fprintln(out, "  (could not read status: "+err.Error()+")")
	}
	return nil
}

func runServiceUninstall(mgr service.Manager, spec service.Spec, out io.Writer) error {
	fmt.Fprintf(out, "Uninstalling gn-drive service (%s, %s)...\n", service.Platform(), spec.Scope)
	if err := mgr.Uninstall(spec); err != nil {
		return fmt.Errorf("uninstall: %w", err)
	}
	fmt.Fprintln(out, "✓ uninstalled.")
	return nil
}

func runServiceStart(mgr service.Manager, spec service.Spec, out io.Writer) error {
	fmt.Fprintln(out, "Starting gn-drive service...")
	if err := mgr.Start(spec); err != nil {
		return fmt.Errorf("start: %w", err)
	}
	fmt.Fprintln(out, "✓ started.")
	return nil
}

func runServiceStop(mgr service.Manager, spec service.Spec, out io.Writer) error {
	fmt.Fprintln(out, "Stopping gn-drive service...")
	if err := mgr.Stop(spec); err != nil {
		return fmt.Errorf("stop: %w", err)
	}
	fmt.Fprintln(out, "✓ stopped.")
	return nil
}

func runServiceStatus(mgr service.Manager, spec service.Spec, out io.Writer) error {
	installed, err := mgr.IsInstalled(spec)
	if err != nil {
		return err
	}
	if !installed {
		fmt.Fprintln(out, "Service: not installed.")
		fmt.Fprintln(out)
		fmt.Fprintln(out, "To install:")
		fmt.Fprintf(out, "  gn-drive service install%s\n", scopeFlag(spec.Scope))
		return nil
	}

	st, err := mgr.Status(spec)
	if err != nil {
		fmt.Fprintln(out, "Service: installed (status check failed: "+err.Error()+")")
		return nil
	}

	fmt.Fprintln(out, "Service:")
	fmt.Fprintf(out, "  Mode:     %s\n", st.Mode)
	fmt.Fprintf(out, "  Scope:    %s\n", st.Scope)
	fmt.Fprintf(out, "  Platform: %s\n", service.Platform())
	if st.Running {
		fmt.Fprintf(out, "  Running:  yes (pid %d)\n", st.PID)
	} else {
		fmt.Fprintln(out, "  Running:  no")
	}

	health, herr := service.ReadHealth(spec.ConfigDir)
	if herr == nil {
		fmt.Fprintln(out)
		fmt.Fprintln(out, "Health:")
		if !health.StartedAt.IsZero() {
			fmt.Fprintf(out, "  Started:        %s\n", health.StartedAt.Format(time.RFC3339))
		}
		if !health.LastHeartbeat.IsZero() {
			fmt.Fprintf(out, "  Last heartbeat: %s\n", health.LastHeartbeat.Format(time.RFC3339))
			if health.IsStale(60 * time.Second) {
				fmt.Fprintln(out, "  ⚠ heartbeat stale (>60s old) — service may be unresponsive")
			}
		}
		if health.WebPort > 0 {
			fmt.Fprintf(out, "  Web port:       %d\n", health.WebPort)
		}
		if health.Uptime() > 0 {
			fmt.Fprintf(out, "  Uptime:         %s\n", health.Uptime().Round(time.Second))
		}
		if health.LastError != "" {
			fmt.Fprintf(out, "  Last error:     %s\n", health.LastError)
		}
		if !health.LastSyncAt.IsZero() {
			fmt.Fprintf(out, "  Last sync:      %s\n", health.LastSyncAt.Format(time.RFC3339))
		}
		if !health.NextScheduleAt.IsZero() {
			fmt.Fprintf(out, "  Next schedule:  %s\n", health.NextScheduleAt.Format(time.RFC3339))
		}
		if len(health.ActiveTasks) > 0 {
			fmt.Fprintf(out, "  Active tasks:   %s\n", joinTasks(health.ActiveTasks))
		}
	} else if herr != service.ErrNotInstalled {
		fmt.Fprintln(out)
		fmt.Fprintln(out, "Health: (could not read: "+herr.Error()+")")
	}
	return nil
}

func runServiceRestart(mgr service.Manager, spec service.Spec, out io.Writer) error {
	fmt.Fprintln(out, "Restarting gn-drive service...")
	if err := mgr.Restart(spec); err != nil {
		return fmt.Errorf("restart: %w", err)
	}
	fmt.Fprintln(out, "✓ restarted.")
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
