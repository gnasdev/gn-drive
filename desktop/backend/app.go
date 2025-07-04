package backend

import (
	"context"
	"desktop/backend/errors"
	"desktop/backend/models"
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

// App struct - now implements Wails v3 service interface
type App struct {
	app            *application.App
	oc             chan []byte
	ConfigInfo     models.ConfigInfo
	errorHandler   *errors.Middleware
	frontendLogger *errors.FrontendLogger
	initialized    bool
	initMutex      sync.Mutex
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

//go:embed .env
var envConfigStr string

// ServiceStartup is called when the service starts
func (a *App) ServiceStartup(ctx context.Context, options application.ServiceOptions) error {
	// Note: In Wails v3, we don't have direct access to the application instance from ServiceOptions
	// We'll need to handle events differently or get the app reference another way

	// Debug: Log initial working directory
	initialWd, _ := os.Getwd()
	log.Printf("DEBUG: Initial working directory: %s", initialWd)

	if err := utils.CdToNormalizeWorkingDir(ctx); err != nil {
		a.errorHandler.HandleError(err, "startup", "working_directory")
		utils.LogErrorAndExit(err)
	}

	// Debug: Log working directory after normalization
	normalizedWd, _ := os.Getwd()
	log.Printf("DEBUG: Working directory after normalization: %s", normalizedWd)

	a.ConfigInfo.EnvConfig = utils.LoadEnvConfigFromEnvStr(envConfigStr)
	log.Printf("DEBUG: Loaded env config: %+v", a.ConfigInfo.EnvConfig)

	// Load working directory
	wd, err := os.Getwd()
	if err != nil {
		a.errorHandler.HandleError(err, "startup", "get_working_directory")
		utils.LogErrorAndExit(err)
	}
	a.ConfigInfo.WorkingDir = wd

	// Load profiles
	err = a.ConfigInfo.ReadFromFile(a.ConfigInfo.EnvConfig)
	if err != nil {
		log.Printf("DEBUG: Error loading profiles: %v", err)
		a.errorHandler.HandleError(err, "startup", "load_profiles")
		// Initialize with empty profiles if file doesn't exist instead of exiting
		log.Printf("WARNING: Could not load profiles, initializing with empty profiles")
		a.ConfigInfo.Profiles = []models.Profile{}
	}
	log.Printf("DEBUG: Loaded %d profiles", len(a.ConfigInfo.Profiles))

	// Setup event channel for sending messages to the frontend
	a.oc = make(chan []byte)
	go func() {
		for data := range a.oc {
			// Use Wails v3 events API
			a.app.EmitEvent("tofe", string(data))
		}
	}()

	// Load Rclone config
	if err := fsConfig.SetConfigPath(a.ConfigInfo.EnvConfig.RcloneFilePath); err != nil {
		return fmt.Errorf("failed to set rclone config path: %w", err)
	}
	configfile.Install()

	return nil
}

// initializeConfig initializes the configuration if it hasn't been done yet
func (a *App) initializeConfig() {
	a.initMutex.Lock()
	defer a.initMutex.Unlock()

	// Check if already initialized (double-check pattern)
	if a.initialized {
		return
	}

	ctx := context.Background()

	if err := utils.CdToNormalizeWorkingDir(ctx); err != nil {
		a.errorHandler.HandleError(err, "init_config", "working_directory")
		return
	}

	a.ConfigInfo.EnvConfig = utils.LoadEnvConfigFromEnvStr(envConfigStr)

	// Load working directory
	wd, err := os.Getwd()
	if err != nil {
		a.errorHandler.HandleError(err, "init_config", "get_working_directory")
		return
	}
	a.ConfigInfo.WorkingDir = wd

	// Load profiles
	err = a.ConfigInfo.ReadFromFile(a.ConfigInfo.EnvConfig)
	if err != nil {
		a.errorHandler.HandleError(err, "init_config", "load_profiles")
		// Initialize with empty profiles if file doesn't exist instead of returning
		log.Printf("WARNING: Could not load profiles during initialization, initializing with empty profiles")
		a.ConfigInfo.Profiles = []models.Profile{}
	}

	// Load Rclone config
	if err := fsConfig.SetConfigPath(a.ConfigInfo.EnvConfig.RcloneFilePath); err != nil {
		// Log error but don't fail initialization
		fmt.Printf("Warning: failed to set rclone config path: %v\n", err)
	}
	configfile.Install()

	// Mark as initialized
	a.initialized = true
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
