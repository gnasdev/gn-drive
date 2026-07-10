// Package app wires all services via constructor-based dependency injection.
package app

import (
	"context"
	"errors"
	"fmt"
	"log/slog"
	"net"
	"path/filepath"

	"github.com/gnasdev/gn-drive/internal/api"
	"github.com/gnasdev/gn-drive/internal/auth"
	"github.com/gnasdev/gn-drive/internal/boardengine"
	"github.com/gnasdev/gn-drive/internal/browser"
	"github.com/gnasdev/gn-drive/internal/config"
	"github.com/gnasdev/gn-drive/internal/eventbus"
	"github.com/gnasdev/gn-drive/internal/logging"
	"github.com/gnasdev/gn-drive/internal/rclone"
	"github.com/gnasdev/gn-drive/internal/service"
	"github.com/gnasdev/gn-drive/internal/store"
	"github.com/gnasdev/gn-drive/internal/syncengine"
	"github.com/gnasdev/gn-drive/internal/webui"
)

// App holds all application services.
type App struct {
	Config      *config.Paths
	EventBus    *eventbus.Bus
	Log         *slog.Logger
	Store       *store.Store
	Auth        *auth.Service
	Rclone      *rclone.Client
	SyncEngine  *syncengine.Engine
	BoardEngine *boardengine.Engine
	Browser     *browser.Opener
	API         *api.Server
	Listener    net.Listener    // set by Run()
	Health      *service.Writer // non-nil when running in service mode
}

// Options configures App construction.
type Options struct {
	ConfigDir         string
	LogMode           logging.Mode
	RcloneBinary      string
	UnlockStdin       bool
	UnlockPassword    string
	DevUnlockPassword string
}

// New constructs a new App and initializes all services.
func New(ctx context.Context, opts Options) (*App, error) {
	// 1. Config paths
	cfg := config.Detect()
	if opts.ConfigDir != "" {
		cfg.ConfigDir = opts.ConfigDir
	}
	if err := cfg.EnsureConfigDir(); err != nil {
		return nil, fmt.Errorf("ensure config dir: %w", err)
	}

	// 2. Event bus
	bus := eventbus.NewBus(ctx)

	// 3. Logger
	logWrap := logging.New(opts.LogMode)
	log := logWrap.Logger
	if opts.LogMode == logging.ModeService {
		log = log.With("service", "gn-drive")
	}

	// 4. Auth
	authSvc, _ := auth.New(auth.Options{
		ConfigDir: cfg.ConfigDir,
		Logger:    log,
	})

	// 4a. Unlock if requested
	switch {
	case opts.UnlockStdin:
		if err := authSvc.UnlockFromStdin(); err != nil {
			return nil, fmt.Errorf("unlock from stdin: %w", err)
		}
	case opts.DevUnlockPassword != "":
		// Development-only auto unlock: set up the master password if the app
		// has never been configured, then unlock. This is gated by the caller
		// (run --dev + GN_DRIVE_DEV_PASSWORD) and should never be used outside
		// of local development.
		if !authSvc.IsSetup() {
			if err := authSvc.SetupPassword(opts.DevUnlockPassword); err != nil {
				return nil, fmt.Errorf("dev unlock setup: %w", err)
			}
		}
		if err := authSvc.Unlock(opts.DevUnlockPassword); err != nil {
			return nil, fmt.Errorf("dev unlock: %w", err)
		}
	case opts.UnlockPassword != "":
		if err := authSvc.Unlock(opts.UnlockPassword); err != nil {
			return nil, fmt.Errorf("unlock: %w", err)
		}
	}
	if authSvc.IsSetup() && !authSvc.IsUnlocked() {
		return nil, fmt.Errorf("auth: app is locked — provide --password or run 'gn-drive run' to unlock via web UI")
	}

	// 5. Store
	dbPath := filepath.Join(cfg.ConfigDir, "gn-drive.db")
	st, err := store.New(ctx, dbPath, log)
	if err != nil {
		return nil, fmt.Errorf("init store: %w", err)
	}

	// 6. Rclone client
	rcloneCfg := filepath.Join(cfg.ConfigDir, "rclone.conf")
	rc, err := rclone.New(rclone.Options{
		BinaryPath: opts.RcloneBinary,
		ConfigPath: rcloneCfg,
		Logger:     log,
	})
	if err != nil {
		_ = st.Close()
		return nil, fmt.Errorf("init rclone: %w", err)
	}

	// 7. Sync engine
	eng := syncengine.New(syncengine.Deps{
		Logger: log,
		Bus:    bus,
		Store:  st,
		Rclone: rc,
	})

	// 7b. Board DAG engine
	boardEng := boardengine.New(boardengine.Options{
		Store:  st,
		Rclone: rc,
		Bus:    bus,
		Log:    log,
	})

	// 8. Browser opener
	br := browser.New()

	// 9. API server (built but not started)
	apiServer := api.New(&api.AppDeps{
		Auth:        authSvc,
		Store:       st,
		Rclone:      rc,
		SyncEngine:  eng,
		BoardEngine: boardEng,
		Bus:         bus,
		WebUI:       webui.Handler(),
		Service:     nil, // set by run.go when service mode
	}, log)

	return &App{
		Config:      cfg,
		EventBus:    bus,
		Log:         log,
		Store:       st,
		Auth:        authSvc,
		Rclone:      rc,
		SyncEngine:  eng,
		BoardEngine: boardEng,
		Browser:     br,
		API:         apiServer,
	}, nil
}

// Close gracefully shuts down the app.
func (a *App) Close() error {
	var errs []error
	if a.Health != nil {
		a.Health.Stop()
	}
	if a.EventBus != nil {
		errs = append(errs, a.EventBus.Close())
	}
	if a.Store != nil {
		errs = append(errs, a.Store.Close())
	}
	if a.Auth != nil {
		_ = a.Auth.Lock()
	}
	return errors.Join(errs...)
}
