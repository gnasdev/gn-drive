package services

import (
	"context"
	"desktop/backend/config"
	"desktop/backend/events"
	"desktop/backend/models"
	"desktop/backend/utils"
	"fmt"
	"log"
	"os"
	"sync"

	"github.com/wailsapp/wails/v3/pkg/application"
)

// ConfigService handles configuration and profile management
type ConfigService struct {
	app         *application.App
	configInfo  *models.ConfigInfo
	mutex       sync.RWMutex
	initialized bool
}

// NewConfigService creates a new config service
func NewConfigService(app *application.App) *ConfigService {
	return &ConfigService{
		app:        app,
		configInfo: &models.ConfigInfo{},
	}
}

// SetApp sets the application reference for events
func (c *ConfigService) SetApp(app *application.App) {
	c.app = app
}

// ServiceName returns the name of the service
func (c *ConfigService) ServiceName() string {
	return "ConfigService"
}

// ServiceStartup is called when the service starts
func (c *ConfigService) ServiceStartup(ctx context.Context, options application.ServiceOptions) error {
	log.Printf("ConfigService starting up...")
	return c.initializeConfig(ctx)
}

// ServiceShutdown is called when the service shuts down
func (c *ConfigService) ServiceShutdown(ctx context.Context) error {
	log.Printf("ConfigService shutting down...")
	return nil
}

// initializeConfig initializes the configuration
func (c *ConfigService) initializeConfig(ctx context.Context) error {
	c.mutex.Lock()
	defer c.mutex.Unlock()

	if c.initialized {
		return nil
	}

	// Set working directory
	if err := utils.CdToNormalizeWorkingDir(ctx); err != nil {
		return fmt.Errorf("failed to set working directory: %w", err)
	}

	// Load environment config (embedded .env)
	// This would need to be passed from the main app or loaded differently
	// For now, we'll create a placeholder
	c.configInfo.EnvConfig = config.Config{
		ProfileFilePath: "profiles.json",
		RcloneFilePath:  "rclone.conf",
	}

	// Get working directory
	wd, err := os.Getwd()
	if err != nil {
		return fmt.Errorf("failed to get working directory: %w", err)
	}
	c.configInfo.WorkingDir = wd

	// Load profiles from file
	if err := c.configInfo.ReadFromFile(c.configInfo.EnvConfig); err != nil {
		log.Printf("Warning: Could not load profiles: %v", err)
		// Initialize with empty profiles if file doesn't exist
		c.configInfo.Profiles = []models.Profile{}
	}

	c.initialized = true
	log.Printf("ConfigService initialized with %d profiles", len(c.configInfo.Profiles))

	// Emit config updated event
	c.emitConfigEvent(events.ConfigUpdated, "", c.configInfo)

	return nil
}

// GetConfigInfo returns the current configuration information
func (c *ConfigService) GetConfigInfo(ctx context.Context) (*models.ConfigInfo, error) {
	c.mutex.RLock()
	defer c.mutex.RUnlock()

	if !c.initialized {
		if err := c.initializeConfig(ctx); err != nil {
			return nil, err
		}
	}

	// Return a copy to prevent external modifications
	configCopy := *c.configInfo
	configCopy.Profiles = make([]models.Profile, len(c.configInfo.Profiles))
	copy(configCopy.Profiles, c.configInfo.Profiles)

	return &configCopy, nil
}

// GetProfiles returns all profiles
func (c *ConfigService) GetProfiles(ctx context.Context) ([]models.Profile, error) {
	c.mutex.RLock()
	defer c.mutex.RUnlock()

	if !c.initialized {
		if err := c.initializeConfig(ctx); err != nil {
			return nil, err
		}
	}

	// Return a copy to prevent external modifications
	profiles := make([]models.Profile, len(c.configInfo.Profiles))
	copy(profiles, c.configInfo.Profiles)

	return profiles, nil
}

// AddProfile adds a new profile
func (c *ConfigService) AddProfile(ctx context.Context, profile models.Profile) error {
	c.mutex.Lock()
	defer c.mutex.Unlock()

	// Validate profile
	if err := c.validateProfile(profile); err != nil {
		return fmt.Errorf("invalid profile: %w", err)
	}

	// Check for duplicate names
	for _, existingProfile := range c.configInfo.Profiles {
		if existingProfile.Name == profile.Name {
			return fmt.Errorf("profile with name '%s' already exists", profile.Name)
		}
	}

	// Add profile
	c.configInfo.Profiles = append(c.configInfo.Profiles, profile)

	// Save to file
	if err := c.saveProfiles(); err != nil {
		// Rollback
		c.configInfo.Profiles = c.configInfo.Profiles[:len(c.configInfo.Profiles)-1]
		return fmt.Errorf("failed to save profiles: %w", err)
	}

	// Emit profile added event
	c.emitConfigEvent(events.ProfileAdded, profile.Name, profile)

	log.Printf("Profile '%s' added successfully", profile.Name)
	return nil
}

// UpdateProfile updates an existing profile
func (c *ConfigService) UpdateProfile(ctx context.Context, profile models.Profile) error {
	c.mutex.Lock()
	defer c.mutex.Unlock()

	// Validate profile
	if err := c.validateProfile(profile); err != nil {
		return fmt.Errorf("invalid profile: %w", err)
	}

	// Find and update profile
	found := false
	var oldProfile models.Profile
	for i, existingProfile := range c.configInfo.Profiles {
		if existingProfile.Name == profile.Name {
			oldProfile = existingProfile
			c.configInfo.Profiles[i] = profile
			found = true
			break
		}
	}

	if !found {
		return fmt.Errorf("profile '%s' not found", profile.Name)
	}

	// Save to file
	if err := c.saveProfiles(); err != nil {
		// Rollback
		for i, existingProfile := range c.configInfo.Profiles {
			if existingProfile.Name == profile.Name {
				c.configInfo.Profiles[i] = oldProfile
				break
			}
		}
		return fmt.Errorf("failed to save profiles: %w", err)
	}

	// Emit profile updated event
	c.emitConfigEvent(events.ProfileUpdated, profile.Name, profile)

	log.Printf("Profile '%s' updated successfully", profile.Name)
	return nil
}

// DeleteProfile deletes a profile
func (c *ConfigService) DeleteProfile(ctx context.Context, profileName string) error {
	c.mutex.Lock()
	defer c.mutex.Unlock()

	// Find and remove profile
	found := false
	var deletedProfile models.Profile
	for i, profile := range c.configInfo.Profiles {
		if profile.Name == profileName {
			deletedProfile = profile
			c.configInfo.Profiles = append(c.configInfo.Profiles[:i], c.configInfo.Profiles[i+1:]...)
			found = true
			break
		}
	}

	if !found {
		return fmt.Errorf("profile '%s' not found", profileName)
	}

	// Save to file
	if err := c.saveProfiles(); err != nil {
		// Rollback
		c.configInfo.Profiles = append(c.configInfo.Profiles, deletedProfile)
		return fmt.Errorf("failed to save profiles: %w", err)
	}

	// Emit profile deleted event
	c.emitConfigEvent(events.ProfileDeleted, profileName, deletedProfile)

	log.Printf("Profile '%s' deleted successfully", profileName)
	return nil
}

// validateProfile validates a profile
func (c *ConfigService) validateProfile(profile models.Profile) error {
	if profile.Name == "" {
		return fmt.Errorf("profile name cannot be empty")
	}
	if profile.From == "" {
		return fmt.Errorf("from path cannot be empty")
	}
	if profile.To == "" {
		return fmt.Errorf("to path cannot be empty")
	}
	return nil
}

// saveProfiles saves profiles to file
func (c *ConfigService) saveProfiles() error {
	return c.configInfo.WriteToFile(c.configInfo.EnvConfig)
}

// emitConfigEvent emits a configuration event
func (c *ConfigService) emitConfigEvent(eventType events.EventType, profileId string, data interface{}) {
	event := events.NewConfigEvent(eventType, profileId, data)
	c.app.EmitEvent(string(eventType), event)
}
