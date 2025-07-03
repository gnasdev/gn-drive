package main

import (
	be "desktop/backend"
	"embed"
	"log"

	"github.com/wailsapp/wails/v3/pkg/application"
)

//go:embed all:frontend/dist/browser
var assets embed.FS

func main() {
	// Create an instance of the app service
	appService := be.NewApp()

	// Create application with options
	app := application.New(application.Options{
		Name:        "ns-drive",
		Description: "A desktop application for rclone file synchronization",
		Assets: application.AssetOptions{
			Handler: application.AssetFileServerFS(assets),
		},
		Services: []application.Service{
			application.NewService(appService),
		},
	})

	// Store the application reference in the service for events
	appService.SetApp(app)

	// Create the main window
	window := app.NewWebviewWindowWithOptions(application.WebviewWindowOptions{
		Title:  "ns-drive",
		Width:  768,
		Height: 768,
	})

	// Set the window URL to load the frontend
	window.SetURL("/")

	// Run the application
	err := app.Run()
	if err != nil {
		log.Fatal("Error:", err.Error())
	}
}
