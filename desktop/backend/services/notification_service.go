package services

import (
	"context"
	"encoding/json"
	"log"
	"os"
	"path/filepath"
	"sync"

	"github.com/gen2brain/beeep"
	"github.com/wailsapp/wails/v3/pkg/application"
)

// AppSettings holds persisted application settings
type AppSettings struct {
	NotificationsEnabled bool `json:"notifications_enabled"`
	DebugMode            bool `json:"debug_mode"`
}

// NotificationService handles desktop notifications and app settings persistence
type NotificationService struct {
	app      *application.App
	settings AppSettings
	filePath string
	mutex    sync.RWMutex
}

// NewNotificationService creates a new notification service
func NewNotificationService(app *application.App) *NotificationService {
	return &NotificationService{
		app: app,
		settings: AppSettings{
			NotificationsEnabled: true,
			DebugMode:            false,
		},
	}
}

// SetApp sets the application reference
func (n *NotificationService) SetApp(app *application.App) {
	n.app = app
}

// ServiceName returns the name of the service
func (n *NotificationService) ServiceName() string {
	return "NotificationService"
}

// ServiceStartup is called when the service starts
func (n *NotificationService) ServiceStartup(ctx context.Context, options application.ServiceOptions) error {
	log.Printf("NotificationService starting up...")

	// Determine settings file path
	shared := GetSharedConfig()
	if shared != nil {
		n.filePath = filepath.Join(shared.ConfigDir, "settings.json")
	} else {
		homeDir, err := os.UserHomeDir()
		if err != nil {
			homeDir = "."
		}
		n.filePath = filepath.Join(homeDir, ".config", "ns-drive", "settings.json")
	}

	// Load persisted settings
	n.loadSettings()
	return nil
}

// ServiceShutdown is called when the service shuts down
func (n *NotificationService) ServiceShutdown(ctx context.Context) error {
	log.Printf("NotificationService shutting down...")
	return nil
}

// SendNotification sends a desktop notification
func (n *NotificationService) SendNotification(ctx context.Context, title, body string) error {
	n.mutex.RLock()
	enabled := n.settings.NotificationsEnabled
	n.mutex.RUnlock()

	if !enabled {
		return nil
	}

	if err := beeep.Notify(title, body, ""); err != nil {
		log.Printf("Failed to send notification: %v", err)
		return err
	}
	return nil
}

// SetEnabled enables or disables notifications
func (n *NotificationService) SetEnabled(ctx context.Context, enabled bool) {
	n.mutex.Lock()
	n.settings.NotificationsEnabled = enabled
	n.mutex.Unlock()
	n.saveSettings()
}

// IsEnabled returns whether notifications are enabled
func (n *NotificationService) IsEnabled(ctx context.Context) bool {
	n.mutex.RLock()
	defer n.mutex.RUnlock()
	return n.settings.NotificationsEnabled
}

// SetDebugMode enables or disables debug mode
func (n *NotificationService) SetDebugMode(ctx context.Context, enabled bool) {
	n.mutex.Lock()
	n.settings.DebugMode = enabled
	n.mutex.Unlock()
	n.saveSettings()
}

// IsDebugMode returns whether debug mode is enabled
func (n *NotificationService) IsDebugMode(ctx context.Context) bool {
	n.mutex.RLock()
	defer n.mutex.RUnlock()
	return n.settings.DebugMode
}

// GetSettings returns all current app settings
func (n *NotificationService) GetSettings(ctx context.Context) AppSettings {
	n.mutex.RLock()
	defer n.mutex.RUnlock()
	return n.settings
}

func (n *NotificationService) loadSettings() {
	data, err := os.ReadFile(n.filePath)
	if err != nil {
		if !os.IsNotExist(err) {
			log.Printf("Warning: Could not load settings: %v", err)
		}
		return
	}
	if len(data) == 0 {
		return
	}
	n.mutex.Lock()
	defer n.mutex.Unlock()
	if err := json.Unmarshal(data, &n.settings); err != nil {
		log.Printf("Warning: Could not parse settings: %v", err)
	}
}

func (n *NotificationService) saveSettings() {
	n.mutex.RLock()
	data, err := json.MarshalIndent(n.settings, "", "  ")
	n.mutex.RUnlock()
	if err != nil {
		log.Printf("Warning: Could not marshal settings: %v", err)
		return
	}
	if err := os.MkdirAll(filepath.Dir(n.filePath), 0755); err != nil {
		log.Printf("Warning: Could not create settings directory: %v", err)
		return
	}
	if err := os.WriteFile(n.filePath, data, 0644); err != nil {
		log.Printf("Warning: Could not save settings: %v", err)
	}
}
