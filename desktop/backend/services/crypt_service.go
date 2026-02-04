package services

import (
	"context"
	"desktop/backend/events"
	"fmt"
	"log"
	"sync"

	"github.com/rclone/rclone/fs/config"
	"github.com/rclone/rclone/fs/config/obscure"
	"github.com/rclone/rclone/fs/rc"
	"github.com/wailsapp/wails/v3/pkg/application"
)

// CryptRemoteConfig holds configuration for creating a crypt remote
type CryptRemoteConfig struct {
	Name             string `json:"name"`              // Name of the crypt remote
	WrappedRemote    string `json:"wrapped_remote"`    // Remote to encrypt e.g. "gdrive:encrypted"
	Password         string `json:"password"`          // Encryption password
	Password2        string `json:"password2"`         // Salt password (optional)
	FilenameEncrypt  string `json:"filename_encrypt"`  // "standard", "obfuscate", "off"
	DirectoryEncrypt bool   `json:"directory_encrypt"` // Encrypt directory names
}

// CryptService manages crypt (encryption) remotes and config encryption
type CryptService struct {
	app         *application.App
	eventBus    *events.WailsEventBus
	mutex       sync.RWMutex
	initialized bool
}

// NewCryptService creates a new crypt service
func NewCryptService(app *application.App) *CryptService {
	return &CryptService{
		app: app,
	}
}

// SetApp sets the application reference for events
func (c *CryptService) SetApp(app *application.App) {
	c.app = app
	if bus := GetSharedEventBus(); bus != nil {
		c.eventBus = bus
	} else {
		c.eventBus = events.NewEventBus(app)
	}
}

// ServiceName returns the name of the service
func (c *CryptService) ServiceName() string {
	return "CryptService"
}

// ServiceStartup is called when the service starts
func (c *CryptService) ServiceStartup(ctx context.Context, options application.ServiceOptions) error {
	log.Printf("CryptService starting up...")
	return c.initialize()
}

// ServiceShutdown is called when the service shuts down
func (c *CryptService) ServiceShutdown(ctx context.Context) error {
	log.Printf("CryptService shutting down...")
	return nil
}

// initialize ensures crypt service is ready
func (c *CryptService) initialize() error {
	c.mutex.Lock()
	defer c.mutex.Unlock()

	if c.initialized {
		return nil
	}

	// configfile.Install() is called once by App.ServiceStartup — no need to repeat here.
	c.initialized = true
	return nil
}

// CreateCryptRemote creates a new crypt remote wrapping an existing remote
func (c *CryptService) CreateCryptRemote(ctx context.Context, cfg CryptRemoteConfig) error {
	c.mutex.Lock()
	defer c.mutex.Unlock()

	if cfg.Name == "" {
		return fmt.Errorf("crypt remote name cannot be empty")
	}
	if cfg.WrappedRemote == "" {
		return fmt.Errorf("wrapped remote path cannot be empty")
	}
	if cfg.Password == "" {
		return fmt.Errorf("encryption password cannot be empty")
	}

	// Check if remote already exists
	remotes := config.FileSections()
	for _, r := range remotes {
		if r == cfg.Name {
			return fmt.Errorf("remote '%s' already exists", cfg.Name)
		}
	}

	// Obscure password for storage
	obscuredPassword, err := obscure.Obscure(cfg.Password)
	if err != nil {
		return fmt.Errorf("failed to obscure password: %w", err)
	}

	filenameEncrypt := cfg.FilenameEncrypt
	if filenameEncrypt == "" {
		filenameEncrypt = "standard"
	}

	dirNameEncrypt := "true"
	if !cfg.DirectoryEncrypt {
		dirNameEncrypt = "false"
	}

	// Build params for CreateRemote
	params := rc.Params{
		"remote":                    cfg.WrappedRemote,
		"password":                  obscuredPassword,
		"filename_encryption":       filenameEncrypt,
		"directory_name_encryption": dirNameEncrypt,
	}

	if cfg.Password2 != "" {
		obscuredPassword2, err := obscure.Obscure(cfg.Password2)
		if err != nil {
			return fmt.Errorf("failed to obscure password2: %w", err)
		}
		params["password2"] = obscuredPassword2
	}

	// Create the crypt remote via rclone config API
	_, err = config.CreateRemote(ctx, cfg.Name, "crypt", params, config.UpdateRemoteOpt{
		NonInteractive: true,
		Obscure:        false, // We already obscured passwords
	})
	if err != nil {
		return fmt.Errorf("failed to create crypt remote: %w", err)
	}

	c.emitCryptEvent(events.CryptRemoteCreated, cfg.Name, cfg)
	log.Printf("Crypt remote '%s' created wrapping '%s'", cfg.Name, cfg.WrappedRemote)
	return nil
}

// DeleteCryptRemote deletes a crypt remote
func (c *CryptService) DeleteCryptRemote(ctx context.Context, name string) error {
	c.mutex.Lock()
	defer c.mutex.Unlock()

	// Verify remote exists and is of type crypt
	remoteType, _ := config.FileGetValue(name, "type")
	if remoteType == "" {
		return fmt.Errorf("remote '%s' not found", name)
	}
	if remoteType != "crypt" {
		return fmt.Errorf("remote '%s' is not a crypt remote (type: %s)", name, remoteType)
	}

	config.DeleteRemote(name)

	c.emitCryptEvent(events.CryptRemoteDeleted, name, nil)
	log.Printf("Crypt remote '%s' deleted", name)
	return nil
}

// ListCryptRemotes returns all crypt remotes
func (c *CryptService) ListCryptRemotes(ctx context.Context) ([]string, error) {
	c.mutex.RLock()
	defer c.mutex.RUnlock()

	var cryptRemotes []string
	remotes := config.FileSections()
	for _, r := range remotes {
		remoteType, _ := config.FileGetValue(r, "type")
		if remoteType == "crypt" {
			cryptRemotes = append(cryptRemotes, r)
		}
	}
	return cryptRemotes, nil
}

// emitCryptEvent emits a crypt event
func (c *CryptService) emitCryptEvent(eventType events.EventType, remoteName string, data interface{}) {
	event := events.NewCryptEvent(eventType, remoteName, data)
	if c.eventBus != nil {
		if err := c.eventBus.EmitCryptEvent(event); err != nil {
			log.Printf("Failed to emit crypt event: %v", err)
		}
	} else if c.app != nil {
		c.app.Event.Emit("tofe", event)
	}
}
