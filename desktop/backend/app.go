package backend

import (
	"context"
	"desktop/backend/errors"
	"desktop/backend/models"
	"desktop/backend/rclone"
	"desktop/backend/utils"
	_ "embed"
	"fmt"
	"log"
	"os"
	"sync"

	fsConfig "github.com/rclone/rclone/fs/config"
	"github.com/rclone/rclone/fs/config/configfile"
	"github.com/wailsapp/wails/v3/pkg/application"
)

// AppInfo holds application metadata exposed to frontend
type AppInfo struct {
	Name        string `json:"name"`
	Version     string `json:"version"`
	Commit      string `json:"commit"`
	Description string `json:"description"`
}

// App struct - now implements Wails v3 service interface
type App struct {
	app            *application.App
	oc             chan []byte
	ConfigInfo     models.ConfigInfo
	errorHandler   *errors.Middleware
	frontendLogger *errors.FrontendLogger
	initialized    bool
	initMutex      sync.Mutex
	cachedRemotes  []fsConfig.Remote
	appVersion     string
	appCommit      string
}

// NewApp creates a new App application struct
func NewApp() *App {
	return &App{
		errorHandler:   errors.NewMiddleware(true),     // Enable debug mode for development
		frontendLogger: errors.NewFrontendLogger(true), // Enable debug mode for development
	}
}

// NewAppWithApplication creates a new App with application reference for events
func NewAppWithApplication(app *application.App) *App {
	return &App{
		app:            app,
		errorHandler:   errors.NewMiddleware(true),     // Enable debug mode for development
		frontendLogger: errors.NewFrontendLogger(true), // Enable debug mode for development
	}
}

// SetApp sets the application reference for events
func (a *App) SetApp(app *application.App) {
	a.app = app
}

// SetVersionInfo sets the version and commit info from build-time ldflags
func (a *App) SetVersionInfo(version, commit string) {
	a.appVersion = version
	a.appCommit = commit
}

// GetAppInfo returns application metadata for the frontend
func (a *App) GetAppInfo(ctx context.Context) AppInfo {
	return AppInfo{
		Name:        "GN Drive",
		Version:     a.appVersion,
		Commit:      a.appCommit,
		Description: "A desktop application for rclone file synchronization",
	}
}

//go:embed .env
var envConfigStr string

// GetEmbeddedEnvConfigStr returns the embedded .env config string for use by main.go
func GetEmbeddedEnvConfigStr() string {
	return envConfigStr
}

// ServiceStartup is called when the service starts (Phase 1).
// Sets up working dir, env config, and event channel.
// Rclone config loading is deferred to CompleteInitialization (Phase 2).
func (a *App) ServiceStartup(ctx context.Context, options application.ServiceOptions) error {
	if err := utils.CdToNormalizeWorkingDir(ctx); err != nil {
		a.errorHandler.HandleError(err, "startup", "working_directory")
		utils.LogErrorAndExit(err)
	}

	// Migrate config files from old location to new home directory location
	if err := utils.MigrateConfigFiles(); err != nil {
		log.Printf("Warning: Failed to migrate config files: %v", err)
	}

	a.ConfigInfo.EnvConfig = utils.LoadEnvConfigFromEnvStr(envConfigStr)

	// Load working directory
	wd, err := os.Getwd()
	if err != nil {
		a.errorHandler.HandleError(err, "startup", "get_working_directory")
		utils.LogErrorAndExit(err)
	}
	a.ConfigInfo.WorkingDir = wd

	// Setup event channel for sending messages to the frontend
	a.oc = make(chan []byte, 100) // buffered channel to prevent blocking
	go func() {
		for data := range a.oc {
			if a.app != nil {
				a.app.Event.Emit("tofe", string(data))
			}
		}
	}()

	return nil
}

// CompleteInitialization performs Phase 2 init: loads profiles, rclone config, and caches remotes.
// Called by AuthService after unlock (or immediately if no auth).
func (a *App) CompleteInitialization(ctx context.Context) error {
	a.initMutex.Lock()
	defer a.initMutex.Unlock()

	if a.initialized {
		return nil
	}

	// Load profiles
	if err := a.ConfigInfo.ReadFromFile(a.ConfigInfo.EnvConfig); err != nil {
		a.errorHandler.HandleError(err, "startup", "load_profiles")
		a.ConfigInfo.Profiles = []models.Profile{}
	}

	// Load Rclone config
	if err := fsConfig.SetConfigPath(a.ConfigInfo.EnvConfig.RcloneFilePath); err != nil {
		return fmt.Errorf("failed to set rclone config path: %w", err)
	}
	configfile.Install()

	// Cache initial remotes list
	a.cachedRemotes = fsConfig.GetRemotes()

	// Clean up any orphaned temp crypt remotes from previous crashes
	rclone.CleanupOrphanedTempCryptRemotes()

	a.initialized = true
	log.Printf("App: Initialization completed (rclone config loaded)")
	return nil
}

// initializeConfig initializes the configuration if it hasn't been done yet
func (a *App) initializeConfig() {
	if a.initialized {
		return
	}
	if err := a.CompleteInitialization(context.Background()); err != nil {
		log.Printf("Warning: Failed to complete initialization: %v", err)
	}
}

// invalidateRemotesCache refreshes the cached remotes list from rclone config
func (a *App) invalidateRemotesCache() {
	a.cachedRemotes = fsConfig.GetRemotes()
}

// LogFrontendMessage logs a message from the frontend
func (a *App) LogFrontendMessage(entry models.FrontendLogEntry) error {
	if a.frontendLogger == nil {
		log.Printf("Frontend logger not initialized")
		return fmt.Errorf("frontend logger not initialized")
	}

	// Validate the log entry
	if !entry.IsValid() {
		return fmt.Errorf("invalid log entry: missing required fields")
	}

	// Log the entry
	return a.frontendLogger.LogEntry(&entry)
}
