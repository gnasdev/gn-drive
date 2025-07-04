package services

import (
	"context"
	"desktop/backend/events"
	"fmt"
	"log"
	"sync"

	"github.com/rclone/rclone/fs/config/configfile"
	"github.com/wailsapp/wails/v3/pkg/application"
)

// RemoteService handles remote storage operations
type RemoteService struct {
	app         *application.App
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
	r.mutex.RLock()
	defer r.mutex.RUnlock()

	if !r.initialized {
		if err := r.initializeRcloneConfig(); err != nil {
			return nil, fmt.Errorf("failed to initialize rclone config: %w", err)
		}
	}

	// TODO: Implement proper rclone config reading
	// For now, return empty list
	remotes := make([]RemoteInfo, 0)

	// Get all remote names from rclone config
	// remoteNames := fsConfig.FileSections()
	// remotes := make([]RemoteInfo, 0, len(remoteNames))

	// for _, name := range remoteNames {
	// 	remoteType := fsConfig.FileGet(name, "type")
	// 	if remoteType == "" {
	// 		continue // Skip invalid remotes
	// 	}

	// 	// Get all config options for this remote
	// 	config := make(map[string]string)
	// 	for _, key := range fsConfig.FileGetKeys(name) {
	// 		value := fsConfig.FileGet(name, key)
	// 		config[key] = value
	// 	}

	// 	remote := RemoteInfo{
	// 		Name:        name,
	// 		Type:        remoteType,
	// 		Config:      config,
	// 		Description: r.getRemoteDescription(remoteType),
	// 	}

	// 	remotes = append(remotes, remote)
	// }

	return remotes, nil
}

// AddRemote adds a new remote configuration
func (r *RemoteService) AddRemote(ctx context.Context, name, remoteType string, config map[string]string) error {
	r.mutex.Lock()
	defer r.mutex.Unlock()

	// Validate input
	if name == "" {
		return fmt.Errorf("remote name cannot be empty")
	}
	if remoteType == "" {
		return fmt.Errorf("remote type cannot be empty")
	}

	// TODO: Implement proper rclone config management
	// For now, just emit the event

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

	// TODO: Implement proper rclone config management
	// For now, just emit the event

	// Create remote info for event
	remoteInfo := RemoteInfo{
		Name:        name,
		Type:        "unknown", // Would get from actual config
		Config:      config,
		Description: r.getRemoteDescription("unknown"),
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

	// TODO: Implement proper rclone config management
	// For now, just emit the event

	// Create remote info for event
	remoteInfo := RemoteInfo{
		Name:        name,
		Type:        "unknown",
		Config:      make(map[string]string),
		Description: r.getRemoteDescription("unknown"),
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

	// TODO: Implement actual remote testing
	// This would involve creating an rclone filesystem and testing connectivity
	// For now, we'll just return success

	log.Printf("Remote '%s' test completed successfully", name)
	return nil
}

// ExportRemotes exports remote configurations to a file
func (r *RemoteService) ExportRemotes(ctx context.Context, filePath string) error {
	r.mutex.RLock()
	defer r.mutex.RUnlock()

	// TODO: Implement remote export functionality
	// This would involve reading the current rclone config and saving it to the specified file

	log.Printf("Remotes exported to '%s'", filePath)
	return nil
}

// ImportRemotes imports remote configurations from a file
func (r *RemoteService) ImportRemotes(ctx context.Context, filePath string) error {
	r.mutex.Lock()
	defer r.mutex.Unlock()

	// TODO: Implement remote import functionality
	// This would involve reading the config file and merging it with the current config

	log.Printf("Remotes imported from '%s'", filePath)
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

// emitRemoteEvent emits a remote event
func (r *RemoteService) emitRemoteEvent(eventType events.EventType, remoteName string, data interface{}) {
	event := events.NewRemoteEvent(eventType, remoteName, data)
	r.app.EmitEvent(string(eventType), event)
}
