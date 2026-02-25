package services

import (
	"context"
	"desktop/backend/config"
	"desktop/backend/events"
	"desktop/backend/models"
	"desktop/backend/validation"
	"fmt"
	"log"
	"os"
	"path/filepath"
	"sync"

	"github.com/wailsapp/wails/v3/pkg/application"
)

// ConfigService handles configuration and profile management
type ConfigService struct {
	app         *application.App
	eventBus    *events.WailsEventBus
	configInfo  *models.ConfigInfo
	mutex       sync.RWMutex
	initialized bool
	validator   *validation.ProfileValidator
}

// NewConfigService creates a new config service
func NewConfigService(app *application.App) *ConfigService {
	return &ConfigService{
		app:        app,
		configInfo: &models.ConfigInfo{},
		validator:  validation.NewProfileValidator(),
	}
}

// SetApp sets the application reference for events
func (c *ConfigService) SetApp(app *application.App) {
	c.app = app
	// Use shared EventBus or create new one
	if bus := GetSharedEventBus(); bus != nil {
		c.eventBus = bus
	} else {
		c.eventBus = events.NewEventBus(app)
	}
}

// ServiceName returns the name of the service
func (c *ConfigService) ServiceName() string {
	return "ConfigService"
}

// ServiceStartup is called when the service starts.
// NOTE: Don't call initializeConfig() here. When auth is enabled, the DB
// is encrypted and not available yet. Lazy initialization via GetConfigInfo/
// GetProfiles will handle loading on first access after unlock.
func (c *ConfigService) ServiceStartup(ctx context.Context, options application.ServiceOptions) error {
	log.Printf("ConfigService starting up...")
	return nil
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

	// Set up config paths (still needed for rclone config path)
	shared := GetSharedConfig()
	if shared != nil {
		c.configInfo.EnvConfig = config.Config{
			ProfileFilePath: filepath.Join(shared.ConfigDir, "profiles.json"),
			RcloneFilePath:  filepath.Join(shared.ConfigDir, "rclone.conf"),
		}
		c.configInfo.WorkingDir = shared.WorkingDir
	} else {
		homeDir, err := os.UserHomeDir()
		if err != nil {
			homeDir = "."
		}
		c.configInfo.EnvConfig = config.Config{
			ProfileFilePath: filepath.Join(homeDir, ".config", "ng-drive", "profiles.json"),
			RcloneFilePath:  filepath.Join(homeDir, ".config", "ng-drive", "rclone.conf"),
		}
		wd, _ := os.Getwd()
		c.configInfo.WorkingDir = wd
	}

	// Load profiles from SQLite
	profiles, err := c.loadProfilesFromDB()
	if err != nil {
		log.Printf("Warning: Could not load profiles from database: %v", err)
		c.configInfo.Profiles = []models.Profile{}
	} else {
		c.configInfo.Profiles = profiles
	}

	c.initialized = true
	log.Printf("ConfigService initialized with %d profiles", len(c.configInfo.Profiles))

	c.emitConfigEvent(events.ConfigUpdated, "", c.configInfo)
	return nil
}

// GetConfigInfo returns the current configuration information
func (c *ConfigService) GetConfigInfo(ctx context.Context) (*models.ConfigInfo, error) {
	// Check initialization status first without holding lock
	c.mutex.RLock()
	initialized := c.initialized
	c.mutex.RUnlock()

	// Initialize if needed (initializeConfig acquires its own lock)
	if !initialized {
		if err := c.initializeConfig(ctx); err != nil {
			return nil, err
		}
	}

	// Now acquire read lock for returning data
	c.mutex.RLock()
	defer c.mutex.RUnlock()

	// Return a copy to prevent external modifications
	configCopy := *c.configInfo
	configCopy.Profiles = make([]models.Profile, len(c.configInfo.Profiles))
	copy(configCopy.Profiles, c.configInfo.Profiles)

	return &configCopy, nil
}

// GetProfiles returns all profiles
func (c *ConfigService) GetProfiles(ctx context.Context) ([]models.Profile, error) {
	// Check initialization status first without holding lock
	c.mutex.RLock()
	initialized := c.initialized
	c.mutex.RUnlock()

	// Initialize if needed (initializeConfig acquires its own lock)
	if !initialized {
		if err := c.initializeConfig(ctx); err != nil {
			return nil, err
		}
	}

	// Now acquire read lock for returning data
	c.mutex.RLock()
	defer c.mutex.RUnlock()

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

	// Save to database
	if err := c.saveProfileToDB(profile); err != nil {
		return fmt.Errorf("failed to save profile: %w", err)
	}

	// Add to in-memory cache
	c.configInfo.Profiles = append(c.configInfo.Profiles, profile)

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

	// Save to database
	if err := c.saveProfileToDB(profile); err != nil {
		// Rollback in-memory
		for i, existingProfile := range c.configInfo.Profiles {
			if existingProfile.Name == profile.Name {
				c.configInfo.Profiles[i] = oldProfile
				break
			}
		}
		return fmt.Errorf("failed to save profile: %w", err)
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

	// Delete from database
	if err := c.deleteProfileFromDB(profileName); err != nil {
		// Rollback in-memory
		c.configInfo.Profiles = append(c.configInfo.Profiles, deletedProfile)
		return fmt.Errorf("failed to delete profile: %w", err)
	}

	// Emit profile deleted event
	c.emitConfigEvent(events.ProfileDeleted, profileName, deletedProfile)

	log.Printf("Profile '%s' deleted successfully", profileName)
	return nil
}

// validateProfile validates a profile using the comprehensive validator
func (c *ConfigService) validateProfile(profile models.Profile) error {
	return c.validator.ValidateProfile(profile)
}

// saveProfiles saves a single profile to the database (INSERT or UPDATE)
func (c *ConfigService) saveProfileToDB(p models.Profile) error {
	db, err := GetSharedDB()
	if err != nil {
		return err
	}

	_, err = db.Exec(`INSERT OR REPLACE INTO profiles (name, from_path, to_path, included_paths, excluded_paths,
		bandwidth, parallel, backup_path, cache_path, min_size, max_size, filter_from_file,
		exclude_if_present, use_regex, max_delete, immutable, conflict_resolution,
		multi_thread_streams, buffer_size, retries, low_level_retries, max_duration)
		VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)`,
		p.Name, p.From, p.To,
		marshalStringSlice(p.IncludedPaths), marshalStringSlice(p.ExcludedPaths),
		p.Bandwidth, p.Parallel, p.BackupPath, p.CachePath,
		p.MinSize, p.MaxSize, p.FilterFromFile, p.ExcludeIfPresent,
		boolToInt(p.UseRegex), intPtrToNullable(p.MaxDelete), boolToInt(p.Immutable),
		p.ConflictResolution, intPtrToNullable(p.MultiThreadStreams),
		p.BufferSize,
		intPtrToNullable(p.Retries), intPtrToNullable(p.LowLevelRetries), p.MaxDuration)
	return err
}

// deleteProfileFromDB deletes a profile from the database
func (c *ConfigService) deleteProfileFromDB(name string) error {
	db, err := GetSharedDB()
	if err != nil {
		return err
	}
	_, err = db.Exec("DELETE FROM profiles WHERE name = ?", name)
	return err
}

// loadProfilesFromDB loads all profiles from the database
func (c *ConfigService) loadProfilesFromDB() ([]models.Profile, error) {
	db, err := GetSharedDB()
	if err != nil {
		return nil, err
	}

	rows, err := db.Query(`SELECT name, from_path, to_path, included_paths, excluded_paths,
		bandwidth, parallel, backup_path, cache_path, min_size, max_size, filter_from_file,
		exclude_if_present, use_regex, max_delete, immutable, conflict_resolution,
		multi_thread_streams, buffer_size, retries, low_level_retries, max_duration
		FROM profiles ORDER BY name`)
	if err != nil {
		return nil, err
	}
	defer rows.Close()

	var profiles []models.Profile
	for rows.Next() {
		var p models.Profile
		var includedPaths, excludedPaths string
		var useRegex, immutable int
		var maxDelete, multiThreadStreams, retries, lowLevelRetries *int

		if err := rows.Scan(&p.Name, &p.From, &p.To, &includedPaths, &excludedPaths,
			&p.Bandwidth, &p.Parallel, &p.BackupPath, &p.CachePath,
			&p.MinSize, &p.MaxSize, &p.FilterFromFile, &p.ExcludeIfPresent,
			&useRegex, &maxDelete, &immutable, &p.ConflictResolution,
			&multiThreadStreams, &p.BufferSize,
			&retries, &lowLevelRetries, &p.MaxDuration); err != nil {
			return nil, fmt.Errorf("failed to scan profile: %w", err)
		}

		p.IncludedPaths = unmarshalStringSlice(includedPaths)
		p.ExcludedPaths = unmarshalStringSlice(excludedPaths)
		p.UseRegex = useRegex != 0
		p.Immutable = immutable != 0
		p.MaxDelete = maxDelete
		p.MultiThreadStreams = multiThreadStreams
		p.Retries = retries
		p.LowLevelRetries = lowLevelRetries

		profiles = append(profiles, p)
	}

	if profiles == nil {
		profiles = []models.Profile{}
	}
	return profiles, rows.Err()
}

// emitConfigEvent emits a configuration event via unified EventBus
func (c *ConfigService) emitConfigEvent(eventType events.EventType, profileId string, data interface{}) {
	event := events.NewConfigEvent(eventType, profileId, data)
	if c.eventBus != nil {
		if err := c.eventBus.EmitConfigEvent(event); err != nil {
			log.Printf("Failed to emit config event: %v", err)
		}
	} else if c.app != nil {
		// Fallback to direct emission if EventBus not initialized
		c.app.Event.Emit("tofe", event)
	}
}
