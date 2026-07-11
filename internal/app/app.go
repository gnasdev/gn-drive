// Package app wires all services via constructor-based dependency injection.
package app

import (
	"context"
	"errors"
	"fmt"
	"log/slog"
	"net"
	"path/filepath"
	"sync"

	"github.com/gnasdev/gn-drive/internal/api"
	"github.com/gnasdev/gn-drive/internal/auth"
	"github.com/gnasdev/gn-drive/internal/boardengine"
	"github.com/gnasdev/gn-drive/internal/browser"
	"github.com/gnasdev/gn-drive/internal/config"
	"github.com/gnasdev/gn-drive/internal/eventbus"
	"github.com/gnasdev/gn-drive/internal/flowengine"
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
	FlowEngine  *flowengine.Engine
	Browser     *browser.Opener
	API         *api.Server
	Listener    net.Listener    // set by Run()
	Health      *service.Writer // non-nil when running in service mode

	deps         *api.AppDeps
	rcloneBinary string
	portalMode   bool
	mu           sync.Mutex
}

// Options configures App construction.
type Options struct {
	ConfigDir      string
	LogMode        logging.Mode
	RcloneBinary   string
	UnlockPassword string
	// PortalMode allows the process to start while the master password is
	// still locked. The HTTP server serves the SPA unlock page; data plane
	// (store/rclone) opens after successful unlock via the web UI.
	// Use for `gn-drive run` / --dev. CLI one-shots leave this false.
	PortalMode bool
	// Version is the running binary version (ldflags). Empty → "dev".
	Version string
}

// New constructs a new App and initializes services.
//
// With PortalMode (web run):
//   - Never fails solely because auth is locked.
//   - Serves unlock UI; store opens after unlock (or immediately if config
//     is already plaintext on disk).
//
// Without PortalMode (CLI / service with --password):
//   - Requires unlock when auth is configured (via UnlockPassword).
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
	authSvc, err := auth.New(auth.Options{
		ConfigDir: cfg.ConfigDir,
		Logger:    log,
	})
	if err != nil {
		return nil, fmt.Errorf("auth: %w", err)
	}

	// Optional unlock at process start (service / CI / CLI --password).
	if opts.UnlockPassword != "" {
		if err := authSvc.Unlock(opts.UnlockPassword); err != nil {
			return nil, fmt.Errorf("unlock: %w", err)
		}
	}

	// Non-portal (CLI): require unlocked if master password is configured.
	if !opts.PortalMode && authSvc.IsSetup() && !authSvc.IsUnlocked() {
		return nil, fmt.Errorf("auth: app is locked — provide --password")
	}

	if opts.PortalMode && authSvc.IsSetup() && !authSvc.IsUnlocked() {
		log.Info("portal: starting locked — unlock via web UI")
	}

	ver := opts.Version
	if ver == "" {
		ver = "dev"
	}

	a := &App{
		Config:       cfg,
		EventBus:     bus,
		Log:          log,
		Auth:         authSvc,
		Browser:      browser.New(),
		rcloneBinary: opts.RcloneBinary,
		portalMode:   opts.PortalMode,
	}

	// Engines exist even before store (AttachStore after unlock).
	a.SyncEngine = syncengine.New(syncengine.Deps{
		Logger: log,
		Bus:    bus,
		Store:  nil,
		Rclone: nil,
	})
	a.BoardEngine = boardengine.New(boardengine.Options{
		Store:  nil,
		Rclone: nil,
		Bus:    bus,
		Log:    log,
	})
	a.FlowEngine = flowengine.New(flowengine.Options{
		Store: nil,
		Sync:  a.SyncEngine,
		Bus:   bus,
		Log:   log,
	})
	// Flow cron jobs call FlowEngine.Execute (interface to avoid import cycle).
	a.SyncEngine.SetFlowExecutor(a.FlowEngine)

	// Open data plane now when config is usable (unlocked, not set up, or
	// locked but still plaintext on disk). Encrypted+locked → defer until
	// web unlock.
	if canOpenDataPlane(authSvc) {
		if err := a.openDataPlane(ctx); err != nil {
			return nil, err
		}
	} else {
		log.Info("portal: data plane deferred until unlock (encrypted config)")
	}

	deps := &api.AppDeps{
		Auth:        authSvc,
		Store:       a.Store,
		Rclone:      a.Rclone,
		SyncEngine: a.SyncEngine,
		FlowEngine: a.FlowEngine,
		Bus:         bus,
		WebUI:       webui.Handler(),
		Service:     nil,
		Version:     ver,
		AfterUnlock: a.AfterUnlock,
		BeforeLock:  a.BeforeLock,
	}
	a.deps = deps
	a.API = api.New(deps, log)

	return a, nil
}

// canOpenDataPlane reports whether sqlite/rclone config files are readable now.
// Unlocked, never-setup, or locked-with-plaintext all qualify. Locked+encrypted does not.
func canOpenDataPlane(authSvc *auth.Service) bool {
	if !authSvc.IsSetup() || authSvc.IsUnlocked() {
		return true
	}
	// Locked: only if no .enc files (previous process left plaintext).
	return !authSvc.HasEncryptedConfig()
}

// openDataPlane opens store + rclone and attaches them to engines / API deps.
func (a *App) openDataPlane(ctx context.Context) error {
	a.mu.Lock()
	defer a.mu.Unlock()

	if a.Store != nil {
		return nil
	}

	dbPath := filepath.Join(a.Config.ConfigDir, "gn-drive.db")
	st, err := store.New(ctx, dbPath, a.Log)
	if err != nil {
		return fmt.Errorf("init store: %w", err)
	}

	rcloneCfg := filepath.Join(a.Config.ConfigDir, "rclone.conf")
	rc, err := rclone.New(rclone.Options{
		BinaryPath: a.rcloneBinary,
		ConfigPath: rcloneCfg,
		Logger:     a.Log,
	})
	if err != nil {
		_ = st.Close()
		return fmt.Errorf("init rclone: %w", err)
	}

	a.Store = st
	a.Rclone = rc
	a.SyncEngine.AttachStore(st, rc)
	a.BoardEngine.Attach(st, rc)
	if a.FlowEngine != nil {
		a.FlowEngine.Attach(st, a.SyncEngine)
	}

	if a.deps != nil {
		a.deps.Store = st
		a.deps.Rclone = rc
		a.deps.FlowEngine = a.FlowEngine
	}

	a.Log.Info("data plane ready", "db", dbPath)
	return nil
}

// AfterUnlock is invoked by the HTTP unlock/setup handlers after auth succeeds.
// It opens the data plane if it was deferred (encrypted start).
func (a *App) AfterUnlock(ctx context.Context) error {
	return a.openDataPlane(ctx)
}

// BeforeLock closes the data plane so auth can re-encrypt config files safely.
func (a *App) BeforeLock() error {
	if a == nil {
		return nil
	}
	a.mu.Lock()
	defer a.mu.Unlock()

	if a.SyncEngine != nil {
		a.SyncEngine.DetachStore()
	}
	if a.BoardEngine != nil {
		a.BoardEngine.Detach()
	}
	if a.FlowEngine != nil {
		a.FlowEngine.Detach()
	}

	var errs []error
	if a.Store != nil {
		errs = append(errs, a.Store.Close())
		a.Store = nil
	}
	a.Rclone = nil
	if a.deps != nil {
		a.deps.Store = nil
		a.deps.Rclone = nil
	}
	return errors.Join(errs...)
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
	// Close store before encrypt so file handles are released.
	if err := a.BeforeLock(); err != nil {
		errs = append(errs, err)
	}
	if a.Auth != nil {
		_ = a.Auth.Lock()
	}
	return errors.Join(errs...)
}
