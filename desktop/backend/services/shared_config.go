package services

import "sync"

// SharedConfig holds common configuration computed once at startup
// to avoid duplicate file I/O across services.
type SharedConfig struct {
	HomeDir    string
	ConfigDir  string // e.g. ~/.config/ng-drive/
	WorkingDir string
}

var (
	sharedConfig      *SharedConfig
	sharedConfigMutex sync.RWMutex
)

// SetSharedConfig sets the shared configuration for all services.
// Should be called once from main.go before app.Run().
func SetSharedConfig(cfg *SharedConfig) {
	sharedConfigMutex.Lock()
	defer sharedConfigMutex.Unlock()
	sharedConfig = cfg
}

// GetSharedConfig returns the shared configuration.
func GetSharedConfig() *SharedConfig {
	sharedConfigMutex.RLock()
	defer sharedConfigMutex.RUnlock()
	return sharedConfig
}
