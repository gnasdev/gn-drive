package main

import (
	be "desktop/backend"
	"desktop/backend/services"
	"desktop/backend/utils"
	"embed"
	"log"
	"os"
	"path/filepath"

	"github.com/wailsapp/wails/v3/pkg/application"
	"github.com/wailsapp/wails/v3/pkg/events"
)

// Version and Commit are set at build time via ldflags
var (
	Version = "dev"
	Commit  = "unknown"
)

//go:embed all:frontend/dist/browser
var assets embed.FS

//go:embed build/appicon.png
var appIcon []byte

func main() {
	// Create service instances
	appService := be.NewApp()
	appService.SetVersionInfo(Version, Commit)
	logService := services.NewLogService()
	authService := services.NewAuthService(nil)
	syncService := services.NewSyncService(nil)
	configService := services.NewConfigService(nil)
	remoteService := services.NewRemoteService(nil)
	tabService := services.NewTabService(nil)
	operationService := services.NewOperationService(nil)
	historyService := services.NewHistoryService(nil)
	schedulerService := services.NewSchedulerService(nil)
	notificationService := services.NewNotificationService(nil)
	cryptService := services.NewCryptService(nil)
	boardService := services.NewBoardService(nil)
	exportService := services.NewExportService(nil)
	importService := services.NewImportService(nil)
	flowService := services.NewFlowService(nil)
	trayService := services.NewTrayService(appIcon)

	// Create application with all services registered
	app := application.New(application.Options{
		Name:        "gn-drive",
		Description: "A desktop application for rclone file synchronization",
		Assets: application.AssetOptions{
			Handler: application.AssetFileServerFS(assets),
		},
		Services: []application.Service{
			application.NewService(appService),
			application.NewService(logService),
			application.NewService(authService),
			application.NewService(syncService),
			application.NewService(configService),
			application.NewService(remoteService),
			application.NewService(tabService),
			application.NewService(operationService),
			application.NewService(historyService),
			application.NewService(schedulerService),
			application.NewService(notificationService),
			application.NewService(cryptService),
			application.NewService(boardService),
			application.NewService(exportService),
			application.NewService(importService),
			application.NewService(flowService),
		},
	})

	// Store the application reference in all services for events
	appService.SetApp(app)
	logService.SetApp(app)
	authService.SetApp(app)
	syncService.SetApp(app)
	configService.SetApp(app)
	remoteService.SetApp(app)
	tabService.SetApp(app)
	operationService.SetApp(app)
	historyService.SetApp(app)
	schedulerService.SetApp(app)
	notificationService.SetApp(app)
	cryptService.SetApp(app)
	boardService.SetApp(app)
	exportService.SetApp(app)
	importService.SetApp(app)
	flowService.SetApp(app)

	// Wire AuthService dependencies
	authService.SetAppService(appService)
	authService.SetNotificationService(notificationService)

	// Load env config and wire to SyncService
	envConfig := utils.LoadEnvConfigFromEnvStr(be.GetEmbeddedEnvConfigStr())
	// Enable debug mode in dev environment (set NS_DRIVE_DEBUG=true)
	if os.Getenv("NS_DRIVE_DEBUG") == "true" {
		envConfig.DebugMode = true
		log.Println("[main] Debug mode enabled via NS_DRIVE_DEBUG env var")
	}
	syncService.SetEnvConfig(envConfig)

	// Wire up service dependencies
	schedulerService.SetSyncService(syncService)
	boardService.SetSyncService(syncService)
	boardService.SetNotificationService(notificationService)
	syncService.SetLogService(logService)
	syncService.SetNotificationService(notificationService)

	// Set singleton instances for cross-service access
	services.SetBoardServiceInstance(boardService)
	services.SetFlowServiceInstance(flowService)
	services.SetTrayServiceInstance(trayService)

	// Wire up tray service dependencies
	trayService.SetApp(app)
	trayService.SetBoardService(boardService)
	trayService.SetFlowService(flowService)

	// Compute shared config once to avoid duplicate file I/O across services
	homeDir, err := os.UserHomeDir()
	if err != nil {
		log.Printf("Warning: Could not get user home directory: %v", err)
		homeDir = "."
	}
	configDir := filepath.Join(homeDir, ".config", "gn-drive")
	wd, _ := os.Getwd()
	services.SetSharedConfig(&services.SharedConfig{
		HomeDir:    homeDir,
		ConfigDir:  configDir,
		WorkingDir: wd,
	})

	// NOTE: Database initialization and settings loading are now handled by AuthService.
	// AuthService.ServiceStartup() will either:
	// - Initialize DB immediately (if no auth configured)
	// - Wait for password unlock, then initialize DB (if auth enabled)

	// Read pre-unlock settings from auth.json for tray/startup behavior
	preSettings := authService.GetPreUnlockSettings()

	// Create the main window
	window := app.Window.NewWithOptions(application.WebviewWindowOptions{
		Title:  "gn-drive",
		Width:  800,
		Height: 800,
	})

	// Set window reference on shared EventBus for window-specific events
	services.SetSharedEventBusWindow(window)

	// Set EventBus on LogService after window is ready
	logService.SetEventBus(services.GetSharedEventBus())

	// Set the window URL to load the frontend
	window.SetURL("/")

	// Handle window close - minimize to tray or quit based on setting
	window.RegisterHook(events.Common.WindowClosing, func(event *application.WindowEvent) {
		if notificationService.IsMinimizeToTray(nil) {
			// Cancel the close event and hide window instead
			event.Cancel()
			window.Hide()
			be.HideFromDock()
		} else {
			// Quit the entire application (including backend)
			app.Quit()
		}
	})

	// Initialize system tray
	trayService.SetWindow(window)
	trayService.SetOnShowCallback(be.ShowInDock)
	if err := trayService.Initialize(); err != nil {
		log.Printf("Warning: Failed to initialize system tray: %v", err)
	}

	// Minimize to tray on startup if setting is enabled (from auth.json or defaults)
	if preSettings.MinimizeToTrayOnStartup {
		window.Hide()
		be.HideFromDock()
	}

	// Run the application
	err = app.Run()
	if err != nil {
		log.Fatal("Error:", err.Error())
	}
}
