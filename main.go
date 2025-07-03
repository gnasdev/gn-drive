package main

import (
	"embed"
	_ "embed"
	"log"

	"github.com/wailsapp/wails/v3/pkg/application"
)

// Wails uses Go's `embed` package to embed the frontend files into the binary.
// Any files in the frontend/dist folder will be embedded into the binary and
// made available to the frontend.
// See https://pkg.go.dev/embed for more information.

//go:embed all:frontend/dist/browser
var assets embed.FS

// main function serves as the application's entry point. It initializes the application, creates a window,
// and starts the application.
func main() {
	// Create an instance of the app service
	appService := NewAppService()

	// Create a new Wails application by providing the necessary options.
	app := application.New(application.Options{
		Name:        "ns-drive",
		Description: "A desktop application for rclone file synchronization",
		Services: []application.Service{
			application.NewService(appService),
		},
		Assets: application.AssetOptions{
			Handler: application.AssetFileServerFS(assets),
		},
		Mac: application.MacOptions{
			ApplicationShouldTerminateAfterLastWindowClosed: true,
		},
	})

	// Store the application reference in the service for events
	appService.SetApp(app)

	// Create the main window
	window := app.NewWebviewWindowWithOptions(application.WebviewWindowOptions{
		Title:  "ns-drive",
		Width:  768,
		Height: 768,
		Mac: application.MacWindow{
			InvisibleTitleBarHeight: 50,
			Backdrop:                application.MacBackdropTranslucent,
			TitleBar:                application.MacTitleBarHiddenInset,
		},
		BackgroundColour: application.NewRGB(27, 38, 54),
	})

	// Set the window URL to load the frontend
	window.SetURL("/")

	// Run the application
	err := app.Run()
	if err != nil {
		log.Fatal("Error:", err.Error())
	}
}
