// Package main is the entry point for the gn-drive CLI.
package main

import (
	"context"
	"fmt"
	"log/slog"
	"net"
	"os"
	"os/signal"
	"syscall"
	"time"

	"github.com/gnasdev/gn-drive/internal/app"
	"github.com/gnasdev/gn-drive/internal/config"
	"github.com/gnasdev/gn-drive/internal/instance"
	"github.com/gnasdev/gn-drive/internal/logging"
	"github.com/gnasdev/gn-drive/internal/ports"
	"github.com/gnasdev/gn-drive/internal/service"
	"github.com/spf13/cobra"
)

func newRunCmd() *cobra.Command {
	var (
		port        int
		noBrowser   bool
		devMode     bool
		serviceMode bool
		password    string
	)
	cmd := &cobra.Command{
		Use:   "run",
		Short: "Start gn-drive in foreground or service mode",
		Long: `Start the sync engine and web UI in the foreground, or as a background service.

Foreground (default):
  $ gn-drive run
  Opens browser automatically, serves on an auto-assigned loopback port.
  Press Ctrl+C to stop.

Service mode:
  $ gn-drive run --service
  Runs as a background daemon managed by your OS init system (systemd/launchd/SCM).
  No browser opens. Logs go to journalctl / log show / Event Viewer.`,
		RunE: func(cmd *cobra.Command, args []string) error {
			return run(cmd.Context(), runOpts{
				port:        port,
				noBrowser:   noBrowser,
				devMode:     devMode,
				serviceMode: serviceMode,
				password:    password,
			})
		},
	}
	cmd.Flags().IntVar(&port, "port", 0, "Bind to a specific port (0 = auto)")
	cmd.Flags().BoolVar(&noBrowser, "no-browser", false, "Do not open the system browser")
	cmd.Flags().BoolVar(&devMode, "dev", false, "Development mode (debug-oriented logging; same portal unlock as normal run)")
	cmd.Flags().BoolVar(&serviceMode, "service", false, "Run as a background service (use 'gn-drive service install' first)")
	cmd.Flags().StringVar(&password, "password", "", "Unlock at process start (service mode / CI). Interactive web run unlocks via the UI instead.")
	return cmd
}

type runOpts struct {
	port        int
	noBrowser   bool
	devMode     bool
	serviceMode bool
	password    string
}

// runDeps is the set of overridable functions used by run. Tests can swap
// these out to avoid binding to real ports, real config dirs, and long-lived
// goroutines.
type runDeps struct {
	allocatePort func(int) (net.Listener, int, error)
	acquireLock  func(string) (locker, error)
	newApp       func(context.Context, app.Options) (*app.App, error)
	signalNotify func(chan<- os.Signal, ...os.Signal)
	serve        func(*app.App, net.Listener) error
}

// defaultRunDeps is the production implementation of runDeps. It is a
// variable so tests can override it.
var defaultRunDeps = func() runDeps {
	return runDeps{
		allocatePort: func(p int) (net.Listener, int, error) {
			ln, port, err := ports.AllocatePort(p)
			if err != nil {
				return nil, 0, err
			}
			return ln, port, nil
		},
		acquireLock: func(dir string) (locker, error) {
			l, err := instance.Acquire(dir)
			return l, err
		},
		newApp:       app.New,
		signalNotify: signal.Notify,
		serve: func(a *app.App, ln net.Listener) error {
			return a.API.Serve(ln)
		},
	}
}

// locker is the subset of instance.Locker we use; lets tests inject a fake.
type locker interface {
	Release() error
}

func run(ctx context.Context, opts runOpts) error {
	return runWithDeps(ctx, opts, defaultRunDeps())
}

func runWithDeps(ctx context.Context, opts runOpts, deps runDeps) error {
	logMode := logging.ModeForeground
	if opts.serviceMode {
		logMode = logging.ModeService
	}

	// 1. Allocate port
	ln, port, err := deps.allocatePort(opts.port)
	if err != nil {
		return fmt.Errorf("allocate port: %w", err)
	}

	// 2. Detect config dir + acquire instance lock
	cfg := config.Detect()
	locker, err := deps.acquireLock(cfg.ConfigDir)
	if err != nil {
		_ = ln.Close()
		return fmt.Errorf("instance lock: %w", err)
	}
	defer locker.Release()

	// 3. Init app — interactive `run` is always a web portal: process starts
	// even when the master password is locked; the SPA shows unlock. Service
	// mode still needs --password (or env) at start because there is no UI.
	appOpts := app.Options{
		LogMode:        logMode,
		UnlockPassword: opts.password,
		Version:        Version,
		// Portal for foreground web UI. Service mode requires pre-unlock.
		PortalMode: !opts.serviceMode,
	}
	if opts.serviceMode && appOpts.UnlockPassword == "" {
		if p := os.Getenv("GN_DRIVE_PASSWORD"); p != "" {
			appOpts.UnlockPassword = p
		}
	}
	_ = opts.devMode // reserved for future dev-only toggles (logging, etc.)
	a, err := deps.newApp(ctx, appOpts)
	if err != nil {
		_ = ln.Close()
		return fmt.Errorf("app init: %w", err)
	}
	defer a.Close()
	a.Listener = ln

	// 4. Start sync engine
	if err := a.SyncEngine.Start(ctx); err != nil {
		return fmt.Errorf("sync engine: %w", err)
	}
	defer func() {
		stopCtx, cancel := context.WithTimeout(context.Background(), 10*time.Second)
		defer cancel()
		_ = a.SyncEngine.Stop(stopCtx)
	}()

	// 4a. Service mode: start health writer
	if opts.serviceMode {
		h := service.NewWriter(cfg.ConfigDir, 5*time.Second)
		if err := h.Start(); err != nil {
			a.Log.Warn("health writer start", "err", err)
		} else {
			h.SetWebPort(port)
			a.Health = h
		}
	}

	// 5. Start API server (goroutine)
	serverErr := make(chan error, 1)
	go func() {
		serverErr <- deps.serve(a, ln)
	}()

	// 6. Print URL
	url := fmt.Sprintf("http://127.0.0.1:%d/", port)
	if opts.serviceMode {
		a.Log.Info("gn-drive service started",
			slog.String("url", url),
			slog.Int("pid", os.Getpid()),
		)
	} else {
		fmt.Printf("gn-drive ready: %s  (Ctrl+C to stop)\n", url)
	}

	// 7. Open browser (foreground only)
	if !opts.noBrowser && !opts.serviceMode {
		if err := a.Browser.Open(url); err != nil {
			fmt.Fprintf(os.Stderr, "warning: could not open browser: %v\n", err)
		}
	}

	// 8. Wait for signal or server error
	sigCh := make(chan os.Signal, 1)
	deps.signalNotify(sigCh, syscall.SIGINT, syscall.SIGTERM)
	select {
	case err := <-serverErr:
		return fmt.Errorf("server: %w", err)
	case sig := <-sigCh:
		a.Log.Info("signal received, shutting down", "signal", sig)
		return nil
	}
}
