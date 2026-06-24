// Package config provides platform and path detection for gn-drive.
//
// Phase 1: HomeDir, ConfigDir, Platform, Env (dev/prod).
// All other config (env vars, flags) is added in later phases.
package config

import (
	"os"
	"path/filepath"
	"runtime"
	"strings"
)

// Env describes the runtime environment.
type Env string

const (
	EnvDevelopment Env = "development"
	EnvProduction  Env = "production"
)

// Platform describes the operating system.
type Platform string

const (
	PlatformLinux   Platform = "linux"
	PlatformDarwin  Platform = "darwin"
	PlatformWindows Platform = "windows"
)

// Paths holds all filesystem paths used by gn-drive.
type Paths struct {
	// HomeDir is the user's home directory (~).
	HomeDir string
	// ConfigDir is ~/.config/gn-drive (or $XDG_CONFIG_HOME/gn-drive on Linux).
	ConfigDir string
	// LogDir is ~/.local/state/gn-drive (Linux) or ConfigDir (Darwin/Windows).
	LogDir string
	// WorkingDir is the directory containing the running binary.
	WorkingDir string
	// Platform is the current OS.
	Platform Platform
	// Env is "development" or "production".
	Env Env
}

// runtimeGOOS is overridable for tests so we can exercise the
// platform-specific code paths on any platform.
var runtimeGOOS = func() string { return runtime.GOOS }

// Detect returns the Paths by probing the environment.
// It does not create any directories — callers must ensure ConfigDir exists.
func Detect() *Paths {
	home, _ := os.UserHomeDir()
	if home == "" {
		home = "/tmp"
	}

	goos := runtimeGOOS()
	cfgDir := os.Getenv("XDG_CONFIG_HOME")
	if cfgDir == "" || goos != "linux" {
		cfgDir = filepath.Join(home, ".config", "gn-drive")
	} else {
		cfgDir = filepath.Join(cfgDir, "gn-drive")
	}

	var logDir string
	if goos == "linux" {
		xdgState := os.Getenv("XDG_STATE_HOME")
		if xdgState == "" {
			xdgState = filepath.Join(home, ".local", "state")
		}
		logDir = filepath.Join(xdgState, "gn-drive")
	} else {
		logDir = cfgDir
	}

	workDir, _ := os.Executable()
	if workDir != "" {
		workDir = filepath.Dir(workDir)
	}

	platform := Platform(goos)
	env := EnvProduction
	if isDevEnv() {
		env = EnvDevelopment
	}

	return &Paths{
		HomeDir:    home,
		ConfigDir:  cfgDir,
		LogDir:     logDir,
		WorkingDir: workDir,
		Platform:   platform,
		Env:        env,
	}
}

// osExecutable is overridable for tests.
var osExecutable = os.Executable

// isDevEnv returns true when running from a development build.
// Detected by checking if the binary lives inside a "bin/" directory
// next to the desktop source tree, or if GN_DRIVE_DEV env var is set.
func isDevEnv() bool {
	if os.Getenv("GN_DRIVE_DEV") != "" {
		return true
	}
	// Heuristic: dev builds land in gn-drive/bin/ during `task dev`
	wd, _ := osExecutable()
	if wd != "" {
		// /path/to/gn-drive/bin/gn-drive → dev
		if strings.HasSuffix(filepath.Dir(wd), filepath.Join("gn-drive", "bin")) {
			return true
		}
		// gn-drive/desktop/bin/gn-drive → also dev (wails dev)
		if strings.Contains(wd, filepath.Join("desktop", "bin")) {
			return true
		}
	}
	return false
}

// EnsureConfigDir creates ConfigDir if it does not exist.
func (p *Paths) EnsureConfigDir() error {
	return os.MkdirAll(p.ConfigDir, 0o700)
}