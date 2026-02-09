package services

import (
	"context"
	"log"
	"os"
	"sync"

	"github.com/emersion/go-autostart"
	"github.com/wailsapp/wails/v3/pkg/application"
)

// AppSettings holds persisted application settings
type AppSettings struct {
	NotificationsEnabled    bool `json:"notifications_enabled"`
	DebugMode               bool `json:"debug_mode"`
	MinimizeToTray          bool `json:"minimize_to_tray"`
	StartAtLogin            bool `json:"start_at_login"`
	MinimizeToTrayOnStartup bool `json:"minimize_to_tray_on_startup"`
}

// NotificationService handles desktop notifications and app settings persistence
type NotificationService struct {
	app      *application.App
	settings AppSettings
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

// ServiceStartup is called when the service starts.
// Note: LoadSettings() is NOT called here because when auth is enabled,
// the DB is encrypted and not available yet. AuthService.initializeApp()
// will call LoadSettings() after decryption.
func (n *NotificationService) ServiceStartup(ctx context.Context, options application.ServiceOptions) error {
	log.Printf("NotificationService starting up...")
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

	if err := sendPlatformNotification(title, body); err != nil {
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
	n.saveSetting("notifications_enabled", boolToStr(enabled))
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
	n.saveSetting("debug_mode", boolToStr(enabled))
}

// IsDebugMode returns whether debug mode is enabled
func (n *NotificationService) IsDebugMode(ctx context.Context) bool {
	n.mutex.RLock()
	defer n.mutex.RUnlock()
	return n.settings.DebugMode
}

// SetMinimizeToTray enables or disables minimize to tray on close
func (n *NotificationService) SetMinimizeToTray(ctx context.Context, enabled bool) {
	n.mutex.Lock()
	n.settings.MinimizeToTray = enabled
	n.mutex.Unlock()
	n.saveSetting("minimize_to_tray", boolToStr(enabled))
}

// IsMinimizeToTray returns whether minimize to tray is enabled
func (n *NotificationService) IsMinimizeToTray(ctx context.Context) bool {
	n.mutex.RLock()
	defer n.mutex.RUnlock()
	return n.settings.MinimizeToTray
}

// GetSettings returns all current app settings
func (n *NotificationService) GetSettings(ctx context.Context) AppSettings {
	n.mutex.RLock()
	defer n.mutex.RUnlock()
	return n.settings
}

// SetStartAtLogin enables or disables starting the app at login
func (n *NotificationService) SetStartAtLogin(ctx context.Context, enabled bool) error {
	// Get executable path
	execPath, err := os.Executable()
	if err != nil {
		log.Printf("Failed to get executable path: %v", err)
		return err
	}

	app := &autostart.App{
		Name:        "NS-Drive",
		DisplayName: "NS-Drive",
		Exec:        []string{execPath},
	}

	if enabled {
		if err := app.Enable(); err != nil {
			log.Printf("Failed to enable start at login: %v", err)
			return err
		}
	} else {
		if err := app.Disable(); err != nil {
			log.Printf("Failed to disable start at login: %v", err)
			return err
		}
	}

	n.mutex.Lock()
	n.settings.StartAtLogin = enabled
	n.mutex.Unlock()
	n.saveSetting("start_at_login", boolToStr(enabled))

	return nil
}

// IsStartAtLogin returns whether start at login is enabled
func (n *NotificationService) IsStartAtLogin(ctx context.Context) bool {
	n.mutex.RLock()
	defer n.mutex.RUnlock()
	return n.settings.StartAtLogin
}

// SetMinimizeToTrayOnStartup enables or disables minimizing to tray on startup
func (n *NotificationService) SetMinimizeToTrayOnStartup(ctx context.Context, enabled bool) {
	n.mutex.Lock()
	n.settings.MinimizeToTrayOnStartup = enabled
	n.mutex.Unlock()
	n.saveSetting("minimize_to_tray_on_startup", boolToStr(enabled))
}

// IsMinimizeToTrayOnStartup returns whether minimize to tray on startup is enabled
func (n *NotificationService) IsMinimizeToTrayOnStartup(ctx context.Context) bool {
	n.mutex.RLock()
	defer n.mutex.RUnlock()
	return n.settings.MinimizeToTrayOnStartup
}

// LoadSettings loads settings from the database. Exported for early loading in main.go.
func (n *NotificationService) LoadSettings() {
	db, err := GetSharedDB()
	if err != nil {
		log.Printf("Warning: Could not get database for settings: %v", err)
		return
	}

	rows, err := db.Query("SELECT key, value FROM settings")
	if err != nil {
		log.Printf("Warning: Could not load settings: %v", err)
		return
	}
	defer rows.Close()

	n.mutex.Lock()
	defer n.mutex.Unlock()

	for rows.Next() {
		var key, value string
		if err := rows.Scan(&key, &value); err != nil {
			continue
		}
		switch key {
		case "notifications_enabled":
			n.settings.NotificationsEnabled = value == "true"
		case "debug_mode":
			n.settings.DebugMode = value == "true"
		case "minimize_to_tray":
			n.settings.MinimizeToTray = value == "true"
		case "start_at_login":
			n.settings.StartAtLogin = value == "true"
		case "minimize_to_tray_on_startup":
			n.settings.MinimizeToTrayOnStartup = value == "true"
		}
	}
}

// authSettingsSyncer is set by AuthService to sync settings to auth.json
var authSettingsSyncer func(AppSettings)

// SetAuthSettingsSyncer sets the callback for syncing settings to auth.json
func SetAuthSettingsSyncer(syncer func(AppSettings)) {
	authSettingsSyncer = syncer
}

func (n *NotificationService) saveSetting(key, value string) {
	db, err := GetSharedDB()
	if err != nil {
		log.Printf("Warning: Could not get database for saving setting: %v", err)
		return
	}

	if _, err := db.Exec("INSERT OR REPLACE INTO settings (key, value) VALUES (?, ?)", key, value); err != nil {
		log.Printf("Warning: Could not save setting %s: %v", key, err)
	}

	// Sync tray/startup settings to auth.json (needed before unlock on next start)
	if authSettingsSyncer != nil {
		n.mutex.RLock()
		settings := n.settings
		n.mutex.RUnlock()
		authSettingsSyncer(settings)
	}
}
