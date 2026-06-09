// Package main is the entry point for the gn-drive CLI.
package main

import (
	"context"
	"fmt"
	"log/slog"
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
		port         int
		noBrowser    bool
		devMode      bool
		serviceMode  bool
		password     string
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
	cmd.Flags().BoolVar(&devMode, "dev", false, "Development mode (enables debug logging)")
	cmd.Flags().BoolVar(&serviceMode, "service", false, "Run as a background service (use 'gn-drive service install' first)")
	cmd.Flags().StringVar(&password, "password", "", "Unlock with password (for service mode or CI)")
	return cmd
}

type runOpts struct {
	port        int
	noBrowser   bool
	devMode     bool
	serviceMode bool
	password    string
}

func run(ctx context.Context, opts runOpts) error {
	logMode := logging.ModeForeground
	if opts.serviceMode {
		logMode = logging.ModeService
	}

	// 1. Allocate port
	ln, port, err := ports.AllocatePort(opts.port)
	if err != nil {
		return fmt.Errorf("allocate port: %w", err)
	}

	// 2. Detect config dir + acquire instance lock
	cfg := config.Detect()
	locker, err := instance.Acquire(cfg.ConfigDir)
	if err != nil {
		_ = ln.Close()
		return fmt.Errorf("instance lock: %w", err)
	}
	defer locker.Release()

	// 3. Init app
	a, err := app.New(ctx, app.Options{
		LogMode:        logMode,
		UnlockPassword: opts.password,
	})
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
		serverErr <- a.API.Serve(ln)
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
	signal.Notify(sigCh, syscall.SIGINT, syscall.SIGTERM)
	select {
	case err := <-serverErr:
		return fmt.Errorf("server: %w", err)
	case sig := <-sigCh:
		a.Log.Info("signal received, shutting down", "signal", sig)
		return nil
	}
}
