package main

import (
	"context"
	"ns-drive/backend"
	"ns-drive/backend/dto"
	"ns-drive/backend/models"

	"github.com/wailsapp/wails/v3/pkg/application"
)

// AppService wraps the backend App to implement the Wails v3 service interface
type AppService struct {
	app *backend.App
}

// NewAppService creates a new AppService
func NewAppService() *AppService {
	return &AppService{
		app: backend.NewApp(),
	}
}

// ServiceStartup is called when the service starts
func (s *AppService) ServiceStartup(ctx context.Context, options application.ServiceOptions) error {
	return s.app.ServiceStartup(ctx, options)
}

// SetApp sets the application reference for events
func (s *AppService) SetApp(app *application.App) {
	s.app.SetApp(app)
}

// Expose all the backend methods through the service

func (s *AppService) Sync(task string, profile models.Profile) int {
	return s.app.Sync(task, profile)
}

func (s *AppService) SyncWithTab(task string, profile models.Profile, tabId string) int {
	return s.app.SyncWithTab(task, profile, tabId)
}

func (s *AppService) GetProfiles() []models.Profile {
	return s.app.ConfigInfo.Profiles
}

func (s *AppService) GetProfile(id string) models.Profile {
	for _, profile := range s.app.ConfigInfo.Profiles {
		if profile.Name == id {
			return profile
		}
	}
	return models.Profile{}
}

func (s *AppService) UpdateProfiles(profiles models.Profiles) *dto.AppError {
	return s.app.UpdateProfiles(profiles)
}

func (s *AppService) GetRemotes() interface{} {
	return s.app.GetRemotes()
}

func (s *AppService) AddRemote(remoteName string, remoteType string, remoteConfig map[string]string) *dto.AppError {
	return s.app.AddRemote(remoteName, remoteType, remoteConfig)
}

func (s *AppService) DeleteRemote(name string) {
	s.app.DeleteRemote(name)
}

func (s *AppService) GetConfigInfo() models.ConfigInfo {
	return s.app.GetConfigInfo()
}

func (s *AppService) StopCommand(id int) {
	s.app.StopCommand(id)
}

// Import/Export methods
func (s *AppService) ExportProfiles() *dto.AppError {
	return s.app.ExportProfiles()
}

func (s *AppService) ImportProfiles() *dto.AppError {
	return s.app.ImportProfiles()
}

func (s *AppService) ExportRemotes() *dto.AppError {
	return s.app.ExportRemotes()
}

func (s *AppService) ImportRemotes() *dto.AppError {
	return s.app.ImportRemotes()
}

func (s *AppService) StopAddingRemote() *dto.AppError {
	return s.app.StopAddingRemote()
}
