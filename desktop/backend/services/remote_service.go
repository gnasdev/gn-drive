package services

import (
	"context"
	"desktop/backend/events"
	"desktop/backend/validation"
	"fmt"
	"log"
	"sync"

	"github.com/rclone/rclone/fs"
	fsConfig "github.com/rclone/rclone/fs/config"
	"github.com/rclone/rclone/fs/config/configfile"
	"github.com/rclone/rclone/fs/rc"
	"github.com/wailsapp/wails/v3/pkg/application"
)

// RemoteService handles remote storage operations
type RemoteService struct {
	app         *application.App
	eventBus    *events.WailsEventBus
	mutex       sync.RWMutex
	initialized bool
}

// RemoteInfo represents information about a remote
type RemoteInfo struct {
	Name        string            `json:"name"`
	Type        string            `json:"type"`
	Config      map[string]string `json:"config"`
	Description string            `json:"description"`
}

// NewRemoteService creates a new remote service
func NewRemoteService(app *application.App) *RemoteService {
	return &RemoteService{
		app: app,
	}
}

// SetApp sets the application reference for events
func (r *RemoteService) SetApp(app *application.App) {
	r.app = app
	// Use shared EventBus or create new one
	if bus := GetSharedEventBus(); bus != nil {
		r.eventBus = bus
	} else {
		r.eventBus = events.NewEventBus(app)
	}
}

// ServiceName returns the name of the service
func (r *RemoteService) ServiceName() string {
	return "RemoteService"
}

// ServiceStartup is called when the service starts
func (r *RemoteService) ServiceStartup(ctx context.Context, options application.ServiceOptions) error {
	log.Printf("RemoteService starting up...")
	return r.initializeRcloneConfig()
}

// ServiceShutdown is called when the service shuts down
func (r *RemoteService) ServiceShutdown(ctx context.Context) error {
	log.Printf("RemoteService shutting down...")
	return nil
}

// initializeRcloneConfig initializes the rclone configuration
func (r *RemoteService) initializeRcloneConfig() error {
	r.mutex.Lock()
	defer r.mutex.Unlock()

	if r.initialized {
		return nil
	}

	// Initialize rclone config
	// Note: The config path should be set by the main application
	configfile.Install()

	r.initialized = true
	log.Printf("RemoteService initialized")
	return nil
}

// GetRemotes returns all configured remotes
func (r *RemoteService) GetRemotes(ctx context.Context) ([]RemoteInfo, error) {
	// Check initialization without holding lock
	r.mutex.RLock()
	initialized := r.initialized
	r.mutex.RUnlock()

	if !initialized {
		if err := r.initializeRcloneConfig(); err != nil {
			return nil, fmt.Errorf("failed to initialize rclone config: %w", err)
		}
	}

	r.mutex.RLock()
	defer r.mutex.RUnlock()

	// Get all configured remotes from rclone
	rcloneRemotes := fsConfig.GetRemotes()
	remotes := make([]RemoteInfo, 0, len(rcloneRemotes))

	for _, remote := range rcloneRemotes {
		remoteInfo := RemoteInfo{
			Name:        remote.Name,
			Type:        remote.Type,
			Config:      make(map[string]string),
			Description: r.getRemoteDescription(remote.Type),
		}
		remotes = append(remotes, remoteInfo)
	}

	log.Printf("RemoteService: Found %d configured remotes", len(remotes))
	return remotes, nil
}

// AddRemote adds a new remote configuration
func (r *RemoteService) AddRemote(ctx context.Context, name, remoteType string, config map[string]string) error {
	// Validate remote name
	if err := validation.ValidateRemoteName(name); err != nil {
		return err
	}
	if remoteType == "" {
		return fmt.Errorf("remote type cannot be empty")
	}

	r.mutex.Lock()
	defer r.mutex.Unlock()

	// Check if remote already exists
	existingRemotes := fsConfig.GetRemotes()
	for _, existing := range existingRemotes {
		if existing.Name == name {
			return fmt.Errorf("remote '%s' already exists", name)
		}
	}

	// Convert config to rc.Params
	rcParams := rc.Params{}
	for k, v := range config {
		rcParams[k] = v
	}

	// Create the remote using rclone's config
	_, err := fsConfig.CreateRemote(ctx, name, remoteType, rcParams, fsConfig.UpdateRemoteOpt{})
	if err != nil {
		log.Printf("Failed to create remote '%s': %v", name, err)
		return fmt.Errorf("failed to create remote: %w", err)
	}

	// Create remote info for event
	remoteInfo := RemoteInfo{
		Name:        name,
		Type:        remoteType,
		Config:      config,
		Description: r.getRemoteDescription(remoteType),
	}

	// Emit remote added event
	r.emitRemoteEvent(events.RemoteAdded, name, remoteInfo)

	log.Printf("Remote '%s' of type '%s' added successfully", name, remoteType)
	return nil
}

// UpdateRemote updates an existing remote configuration
func (r *RemoteService) UpdateRemote(ctx context.Context, name string, config map[string]string) error {
	r.mutex.Lock()
	defer r.mutex.Unlock()

	// Check if remote exists
	existingRemotes := fsConfig.GetRemotes()
	var existingType string
	found := false
	for _, existing := range existingRemotes {
		if existing.Name == name {
			existingType = existing.Type
			found = true
			break
		}
	}
	if !found {
		return fmt.Errorf("remote '%s' not found", name)
	}

	// Convert config to rc.Params
	rcParams := rc.Params{}
	for k, v := range config {
		rcParams[k] = v
	}

	// Update the remote configuration
	_, err := fsConfig.CreateRemote(ctx, name, existingType, rcParams, fsConfig.UpdateRemoteOpt{})
	if err != nil {
		log.Printf("Failed to update remote '%s': %v", name, err)
		return fmt.Errorf("failed to update remote: %w", err)
	}

	// Create remote info for event
	remoteInfo := RemoteInfo{
		Name:        name,
		Type:        existingType,
		Config:      config,
		Description: r.getRemoteDescription(existingType),
	}

	// Emit remote updated event
	r.emitRemoteEvent(events.RemoteUpdated, name, remoteInfo)

	log.Printf("Remote '%s' updated successfully", name)
	return nil
}

// DeleteRemote deletes a remote configuration
func (r *RemoteService) DeleteRemote(ctx context.Context, name string) error {
	r.mutex.Lock()
	defer r.mutex.Unlock()

	// Check if remote exists and get its type
	existingRemotes := fsConfig.GetRemotes()
	var remoteType string
	found := false
	for _, existing := range existingRemotes {
		if existing.Name == name {
			remoteType = existing.Type
			found = true
			break
		}
	}
	if !found {
		return fmt.Errorf("remote '%s' not found", name)
	}

	// Delete the remote from rclone config
	fsConfig.DeleteRemote(name)

	// Create remote info for event
	remoteInfo := RemoteInfo{
		Name:        name,
		Type:        remoteType,
		Config:      make(map[string]string),
		Description: r.getRemoteDescription(remoteType),
	}

	// Emit remote deleted event
	r.emitRemoteEvent(events.RemoteDeleted, name, remoteInfo)

	log.Printf("Remote '%s' deleted successfully", name)
	return nil
}

// TestRemote tests the connection to a remote
func (r *RemoteService) TestRemote(ctx context.Context, name string) error {
	r.mutex.RLock()
	defer r.mutex.RUnlock()

	// Check if remote exists
	existingRemotes := fsConfig.GetRemotes()
	found := false
	for _, existing := range existingRemotes {
		if existing.Name == name {
			found = true
			break
		}
	}
	if !found {
		return fmt.Errorf("remote '%s' not found", name)
	}

	// Test connection by creating a filesystem
	remotePath := name + ":"
	_, err := fs.NewFs(ctx, remotePath)
	if err != nil {
		log.Printf("Remote '%s' test failed: %v", name, err)
		return fmt.Errorf("connection test failed: %w", err)
	}

	log.Printf("Remote '%s' test completed successfully", name)
	return nil
}

// getRemoteDescription returns a description for a remote type
func (r *RemoteService) getRemoteDescription(remoteType string) string {
	descriptions := map[string]string{
		"s3":       "Amazon S3 Compatible Storage",
		"drive":    "Google Drive",
		"dropbox":  "Dropbox",
		"onedrive": "Microsoft OneDrive",
		"gdrive":   "Google Drive",
		"box":      "Box",
		"mega":     "Mega",
		"pcloud":   "pCloud",
		"webdav":   "WebDAV",
		"ftp":      "FTP",
		"sftp":     "SFTP",
		"local":    "Local Filesystem",
		"memory":   "In Memory",
		"crypt":    "Encrypted Remote",
		"compress": "Compressed Remote",
		"cache":    "Cached Remote",
	}

	if desc, exists := descriptions[remoteType]; exists {
		return desc
	}
	return fmt.Sprintf("Remote type: %s", remoteType)
}

// emitRemoteEvent emits a remote event via unified EventBus
func (r *RemoteService) emitRemoteEvent(eventType events.EventType, remoteName string, data interface{}) {
	event := events.NewRemoteEvent(eventType, remoteName, data)
	if r.eventBus != nil {
		if err := r.eventBus.EmitRemoteEvent(event); err != nil {
			log.Printf("Failed to emit remote event: %v", err)
		}
	} else if r.app != nil {
		// Fallback to direct emission if EventBus not initialized
		r.app.Event.Emit("tofe", event)
	}
}
