package utils

import (
	"context"
	"fmt"
	"log"
	"os"
	"path/filepath"
	"runtime"
	"runtime/debug"
	"strings"
	"sync"

	"desktop/backend/constants"
)

func GetPlatform(ctx context.Context) string {
	return runtime.GOOS // windows, darwin, linux
}

func GetCurrentEnvName(ctx context.Context) string {
	// Check for Wails dev mode environment variable
	if os.Getenv("WAILS_DEV_MODE") == "true" {
		return constants.Development.String()
	}

	// Check if we're in development mode by looking for debug info
	if info, ok := debug.ReadBuildInfo(); ok {
		for _, setting := range info.Settings {
			if setting.Key == "-tags" && setting.Value == "dev" {
				return constants.Development.String()
			}
		}
	}

	// Check if we're running from a development directory structure
	// In dev mode, the working directory typically contains "desktop" in the path
	wd, err := os.Getwd()
	if err == nil && strings.Contains(wd, "/desktop") {
		return constants.Development.String()
	}

	return constants.Production.String()
}

func CdToNormalizeWorkingDir(ctx context.Context) error {
	var wd string
	var err error

	if GetCurrentEnvName(ctx) == constants.Development.String() {
		wd, err = os.Getwd()
		if err != nil {
			return err
		}
		// In development, we're in /path/to/gn-drive/desktop, we want to go to /path/to/gn-drive
		if filepath.Base(wd) == "desktop" {
			wd = filepath.Dir(wd)
		}
	} else {
		exePath, err := os.Executable()
		if err != nil {
			return err
		}
		wd = filepath.Dir(exePath)
		if GetPlatform(ctx) != constants.Windows.String() {
			wd = filepath.Clean(wd + "/../../../")
		}
	}

	return os.Chdir(wd)
}

var (
	cmdStore      = make(map[int]func())
	cmdStoreMutex sync.RWMutex
	tabStore      = make(map[int]string)
	tabStoreMutex sync.RWMutex
)

func AddCmd(pid int, cancel func()) {
	cmdStoreMutex.Lock()
	defer cmdStoreMutex.Unlock()
	cmdStore[pid] = cancel
}

func GetCmd(pid int) (func(), bool) {
	cmdStoreMutex.RLock()
	defer cmdStoreMutex.RUnlock()
	cancel, exists := cmdStore[pid]
	return cancel, exists
}

func RemoveCmd(pid int) {
	cmdStoreMutex.Lock()
	defer cmdStoreMutex.Unlock()
	delete(cmdStore, pid)
}

func AddTabMapping(pid int, tabId string) {
	tabStoreMutex.Lock()
	defer tabStoreMutex.Unlock()
	tabStore[pid] = tabId
}

func GetTabMapping(pid int) (string, bool) {
	tabStoreMutex.RLock()
	defer tabStoreMutex.RUnlock()
	tabId, exists := tabStore[pid]
	return tabId, exists
}

func RemoveTabMapping(pid int) {
	tabStoreMutex.Lock()
	defer tabStoreMutex.Unlock()
	delete(tabStore, pid)
}

// logError logs the error to a file on the desktop, including stack trace and error line number
func LogError(inErr error) {
	if wd, err := os.Getwd(); err == nil {
		if f, fileErr := os.OpenFile(filepath.Join(wd, "desktop.log"), os.O_APPEND|os.O_CREATE|os.O_WRONLY, 0644); fileErr == nil {
			defer func() {
				if err := f.Close(); err != nil {
					log.Printf("Warning: failed to close log file: %v", err)
				}
			}()
			logger := log.New(f, "", log.LstdFlags)
			logger.Printf("Error: %v\nStack Trace:\n%s", inErr, debug.Stack())
		} else {
			log.Printf("Failed to open log file: %v", fileErr)
		}
	} else {
		log.Printf("Failed to get working directory: %v", err)
	}
}

func LogErrorAndExit(err error) {
	LogError(err)
	log.Fatal(err)
}

func HandleError(err error, msg string, onError func(err error), onClear func()) error {
	if err != nil {
		log.Printf("%s: %v", msg, err)

		if onError != nil {
			onError(err)
		}
	} else {
		if onClear != nil {
			onClear()
		}
	}

	return err
}

// GetUserHomeDir returns the user's home directory
func GetUserHomeDir() (string, error) {
	homeDir, err := os.UserHomeDir()
	if err != nil {
		return "", err
	}
	return homeDir, nil
}

// ExpandHomePath expands ~ in file paths to the user's home directory
func ExpandHomePath(path string) (string, error) {
	if !strings.HasPrefix(path, "~") {
		return path, nil
	}

	homeDir, err := GetUserHomeDir()
	if err != nil {
		return path, err
	}

	if path == "~" {
		return homeDir, nil
	}

	if strings.HasPrefix(path, "~/") {
		return filepath.Join(homeDir, path[2:]), nil
	}

	return path, nil
}

// MigrateConfigFiles migrates config files from old relative paths to new home directory paths
func MigrateConfigFiles() error {
	homeDir, err := GetUserHomeDir()
	if err != nil {
		return err
	}

	// Define old and new paths
	oldConfigDir := ".config"
	newConfigDir := filepath.Join(homeDir, ".config", "gn-drive")

	// Create new config directory if it doesn't exist
	if err := os.MkdirAll(newConfigDir, 0755); err != nil {
		return fmt.Errorf("failed to create new config directory: %w", err)
	}

	// Files to migrate
	filesToMigrate := map[string]string{
		filepath.Join(oldConfigDir, ".profiles"):   filepath.Join(newConfigDir, "profiles.json"),
		filepath.Join(oldConfigDir, ".resync"):     filepath.Join(newConfigDir, "resync"),
		filepath.Join(oldConfigDir, "rclone.conf"): filepath.Join(newConfigDir, "rclone.conf"),
	}

	for oldPath, newPath := range filesToMigrate {
		// Check if old file exists
		if _, err := os.Stat(oldPath); err == nil {
			// Check if new file doesn't exist (don't overwrite)
			if _, err := os.Stat(newPath); os.IsNotExist(err) {
				// Copy file content
				data, err := os.ReadFile(oldPath)
				if err != nil {
					log.Printf("Warning: Failed to read old config file %s: %v", oldPath, err)
					continue
				}

				if err := os.WriteFile(newPath, data, 0644); err != nil {
					log.Printf("Warning: Failed to write new config file %s: %v", newPath, err)
					continue
				}

				log.Printf("Migrated config file from %s to %s", oldPath, newPath)
			} else {
				log.Printf("New config file %s already exists, skipping migration", newPath)
			}
		}
	}

	return nil
}
