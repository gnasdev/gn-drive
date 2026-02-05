package main

import (
	be "desktop/backend"
	"desktop/backend/services"
	"embed"
	"log"
	"os"
	"path/filepath"

	"github.com/wailsapp/wails/v3/pkg/application"
)

//go:embed all:frontend/dist/browser
var assets embed.FS

func main() {
	// Create service instances
	appService := be.NewApp()
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

	// Create application with all services registered
	app := application.New(application.Options{
		Name:        "ns-drive",
		Description: "A desktop application for rclone file synchronization",
		Assets: application.AssetOptions{
			Handler: application.AssetFileServerFS(assets),
		},
		Services: []application.Service{
			application.NewService(appService),
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
		},
	})

	// Store the application reference in all services for events
	appService.SetApp(app)
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

	// Wire up service dependencies
	schedulerService.SetSyncService(syncService)
	boardService.SetSyncService(syncService)

	// Compute shared config once to avoid duplicate file I/O across services
	homeDir, err := os.UserHomeDir()
	if err != nil {
		log.Printf("Warning: Could not get user home directory: %v", err)
		homeDir = "."
	}
	configDir := filepath.Join(homeDir, ".config", "ns-drive")
	wd, _ := os.Getwd()
	services.SetSharedConfig(&services.SharedConfig{
		HomeDir:    homeDir,
		ConfigDir:  configDir,
		WorkingDir: wd,
	})

	// Create the main window
	window := app.Window.NewWithOptions(application.WebviewWindowOptions{
		Title:  "ns-drive",
		Width:  768,
		Height: 768,
	})

	// Set window reference on shared EventBus for window-specific events
	services.SetSharedEventBusWindow(window)

	// Set the window URL to load the frontend
	window.SetURL("/")

	// Run the application
	err = app.Run()
	if err != nil {
		log.Fatal("Error:", err.Error())
	}
}
