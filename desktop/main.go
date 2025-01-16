package main

import (
	be "desktop/backend"
	"desktop/backend/constants"
	"desktop/backend/dto"
	"embed"

	"github.com/wailsapp/wails/v2"
	"github.com/wailsapp/wails/v2/pkg/options"
	"github.com/wailsapp/wails/v2/pkg/options/assetserver"
)

//go:embed all:frontend/dist/browser
var assets embed.FS

func main() {
	// Create an instance of the app structure
	app := be.NewApp()

	// Create application with options
	err := wails.Run(&options.App{
		Title:  "ns-drive",
		Width:  768,
		Height: 768,
		AssetServer: &assetserver.Options{
			Assets: assets,
		},
		OnStartup: app.Startup,
		Bind: []interface{}{
			app,
		},
		EnumBind: []interface{}{
			[]struct {
				Value  constants.Platform
				TSName string
			}{
				{constants.Windows, constants.Windows.String()},
				{constants.Darwin, constants.Darwin.String()},
				{constants.Linux, constants.Linux.String()},
			},
			[]struct {
				Value  constants.Environment
				TSName string
			}{
				{constants.Development, constants.Development.String()},
				{constants.Production, constants.Production.String()},
			},
			[]struct {
				Value  dto.Command
				TSName string
			}{
				{dto.CommandStoped, dto.CommandStoped.String()},
				{dto.CommandOutput, dto.CommandOutput.String()},
				{dto.CommandStarted, dto.CommandStarted.String()},
				{dto.WorkingDirUpdated, dto.WorkingDirUpdated.String()},
				{dto.Error, dto.Error.String()},
			},
		},
	})

	if err != nil {
		println("Error:", err.Error())
	}
}
