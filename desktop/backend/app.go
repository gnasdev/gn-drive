package backend

import (
	"context"
	"desktop/backend/errors"
	"desktop/backend/models"
	"desktop/backend/utils"
	_ "embed"
	"os"

	fsConfig "github.com/rclone/rclone/fs/config"
	"github.com/rclone/rclone/fs/config/configfile"
	"github.com/wailsapp/wails/v3/pkg/application"
)

// App struct - now implements Wails v3 service interface
type App struct {
	app          *application.App
	oc           chan []byte
	ConfigInfo   models.ConfigInfo
	errorHandler *errors.Middleware
}

// NewApp creates a new App application struct
func NewApp() *App {
	return &App{
		errorHandler: errors.NewMiddleware(true), // Enable debug mode for development
	}
}

// NewAppWithApplication creates a new App with application reference for events
func NewAppWithApplication(app *application.App) *App {
	return &App{
		app:          app,
		errorHandler: errors.NewMiddleware(true), // Enable debug mode for development
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

	if err := utils.CdToNormalizeWorkingDir(ctx); err != nil {
		a.errorHandler.HandleError(err, "startup", "working_directory")
		utils.LogErrorAndExit(err)
	}

	a.ConfigInfo.EnvConfig = utils.LoadEnvConfigFromEnvStr(envConfigStr)

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
		a.errorHandler.HandleError(err, "startup", "load_profiles")
		utils.LogErrorAndExit(err)
	}

	// Setup event channel for sending messages to the frontend
	a.oc = make(chan []byte)
	go func() {
		for data := range a.oc {
			// Use Wails v3 events API
			a.app.EmitEvent("tofe", string(data))
		}
	}()

	// Load Rclone config
	fsConfig.SetConfigPath(a.ConfigInfo.EnvConfig.RcloneFilePath)
	configfile.Install()

	return nil
}
