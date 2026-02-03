package services

import (
	"context"
	"log"
	"sync"

	"github.com/gen2brain/beeep"
	"github.com/wailsapp/wails/v3/pkg/application"
)

// NotificationService handles desktop notifications
type NotificationService struct {
	app     *application.App
	enabled bool
	mutex   sync.RWMutex
}

// NewNotificationService creates a new notification service
func NewNotificationService(app *application.App) *NotificationService {
	return &NotificationService{
		app:     app,
		enabled: true,
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
	enabled := n.enabled
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
	defer n.mutex.Unlock()
	n.enabled = enabled
}

// IsEnabled returns whether notifications are enabled
func (n *NotificationService) IsEnabled(ctx context.Context) bool {
	n.mutex.RLock()
	defer n.mutex.RUnlock()
	return n.enabled
}
