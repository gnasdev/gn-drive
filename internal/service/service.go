// Package service provides cross-platform service management.
//
// Phase 4: opt-in service install/uninstall/start/stop/status/restart for
// systemd (Linux), launchd (macOS), and SCM (Windows).
//
// User-level is the default; --system opt-in requires elevated privileges
// and writes to the system-wide init location.
package service

import (
	"errors"
	"os"
	"path/filepath"
	"runtime"
	"strings"
)

// ErrNotSupported is returned when an action is not supported on this platform.
var ErrNotSupported = errors.New("service: not supported on this platform")

// ErrNotInstalled is returned when querying state of a service that was never installed.
var ErrNotInstalled = errors.New("service: not installed")

// ErrAlreadyInstalled is returned when installing a service that already exists.
var ErrAlreadyInstalled = errors.New("service: already installed")

// Scope determines whether the service is user-level or system-level.
type Scope string

const (
	ScopeUser   Scope = "user"
	ScopeSystem Scope = "system"
)

// Action is a service action.
type Action string

const (
	ActionInstall   Action = "install"
	ActionUninstall Action = "uninstall"
	ActionStart     Action = "start"
	ActionStop      Action = "stop"
	ActionRestart   Action = "restart"
	ActionStatus    Action = "status"
)

// Status describes the runtime state of a service.
type Status struct {
	Installed     bool   `json:"installed"`
	Running       bool   `json:"running"`
	PID           int    `json:"pid"`
	WebPort       int    `json:"web_port"`
	UptimeSecs    int    `json:"uptime_secs"`
	StartedAt     string `json:"started_at,omitempty"`
	LastHeartbeat string `json:"last_heartbeat,omitempty"`
	LastError     string `json:"last_error,omitempty"`
	Mode          string `json:"mode"` // foreground | service
	Scope         string `json:"scope"`
}

// Spec describes a service to install.
type Spec struct {
	// Name is the service name (default: "gn-drive").
	Name string
	// DisplayName is the human-readable name (default: "GN Drive").
	DisplayName string
	// Description is shown in service listings.
	Description string
	// ExecPath is the absolute path to the binary (default: os.Executable()).
	ExecPath string
	// ConfigDir is the gn-drive config directory (passed as --config-dir).
	ConfigDir string
	// Scope is user-level (default) or system-level.
	Scope Scope
	// Env contains additional environment variables (KEY=VALUE).
	Env []string
}

// Platform returns the current OS as a service-platform identifier.
func Platform() string {
	switch runtime.GOOS {
	case "linux":
		return "systemd"
	case "darwin":
		return "launchd"
	case "windows":
		return "scm"
	default:
		return runtime.GOOS
	}
}

// Manager is the cross-platform service interface.
type Manager interface {
	// Install generates the service definition and registers it with the init system.
	Install(spec Spec) error
	// Uninstall removes the service definition from the init system.
	Uninstall(spec Spec) error
	// Start asks the init system to start the service.
	Start(spec Spec) error
	// Stop asks the init system to stop the service.
	Stop(spec Spec) error
	// Restart = Stop + Start.
	Restart(spec Spec) error
	// Status returns the current state.
	Status(spec Spec) (Status, error)
	// IsInstalled returns true if the service definition exists.
	IsInstalled(spec Spec) (bool, error)
}

// NewManager returns a Manager for the current platform.
func NewManager() (Manager, error) {
	return newPlatformManager()
}

// DefaultSpec returns a Spec with sensible defaults filled in.
func DefaultSpec(scope Scope) Spec {
	exec, _ := os.Executable()
	home, _ := os.UserHomeDir()
	configDir := filepath.Join(home, ".config", "gn-drive")

	return Spec{
		Name:        "gn-drive",
		DisplayName: "GN Drive",
		Description: "GN Drive sync engine and web UI",
		ExecPath:    exec,
		ConfigDir:   configDir,
		Scope:       scope,
		Env:         []string{"GN_DRIVE_MODE=service"},
	}
}

// QuoteValue quotes a value for inclusion in a unit/plist file.
func QuoteValue(v string) string {
	if v == "" {
		return `""`
	}
	// Escape backslashes and double quotes.
	v = strings.ReplaceAll(v, `\`, `\\`)
	v = strings.ReplaceAll(v, `"`, `\"`)
	return `"` + v + `"`
}
