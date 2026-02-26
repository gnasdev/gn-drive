package utils

import (
	"bufio"
	_ "embed"
	"fmt"
	"log"
	"os"
	"path/filepath"
	"strings"

	beConfig "desktop/backend/config"
)

func LoadEnvFile(filename string) error {
	file, err := os.Open(filename)
	if err != nil {
		return err
	}
	defer func() {
		if err := file.Close(); err != nil {
			fmt.Printf("Warning: failed to close env file: %v\n", err)
		}
	}()

	scanner := bufio.NewScanner(file)
	for scanner.Scan() {
		line := strings.TrimSpace(scanner.Text())

		// Ignore empty lines or comments
		if len(line) == 0 || strings.HasPrefix(line, "#") {
			continue
		}

		// Split key and value by the first '='
		parts := strings.SplitN(line, "=", 2)
		if len(parts) != 2 {
			continue // Skip invalid lines
		}

		key := strings.TrimSpace(parts[0])
		value := strings.TrimSpace(parts[1])

		// Remove surrounding quotes if present
		value = strings.Trim(value, `"'`)

		// Set the environment variable
		if err := os.Setenv(key, value); err != nil {
			fmt.Printf("Warning: failed to set environment variable %s: %v\n", key, err)
		}
	}

	return scanner.Err()
}

func LoadEnvConfigFromEnvStr(envConfigStr string) beConfig.Config {
	// Create a Config struct with default values (using home directory paths)
	homeDir, err := GetUserHomeDir()
	if err != nil {
		log.Printf("Warning: Could not get user home directory, using relative paths: %v", err)
		homeDir = "."
	}

	cfg := beConfig.Config{
		DebugMode:       false,
		ProfileFilePath: filepath.Join(homeDir, ".config", "gn-drive", "profiles.json"),
		ResyncFilePath:  filepath.Join(homeDir, ".config", "gn-drive", "resync"),
		RcloneFilePath:  filepath.Join(homeDir, ".config", "gn-drive", "rclone.conf"),
	}

	// Parse the env string manually to avoid Viper concurrency issues
	scanner := bufio.NewScanner(strings.NewReader(envConfigStr))
	for scanner.Scan() {
		line := strings.TrimSpace(scanner.Text())

		// Ignore empty lines or comments
		if len(line) == 0 || strings.HasPrefix(line, "#") {
			continue
		}

		// Split key and value by the first '='
		parts := strings.SplitN(line, "=", 2)
		if len(parts) != 2 {
			continue // Skip invalid lines
		}

		key := strings.TrimSpace(parts[0])
		value := strings.TrimSpace(parts[1])

		// Remove surrounding quotes if present
		value = strings.Trim(value, `"'`)

		// Map to config fields
		switch key {
		case "DEBUG_MODE":
			cfg.DebugMode = value == "true"
		case "PROFILE_FILE_PATH":
			if expandedPath, err := ExpandHomePath(value); err == nil {
				cfg.ProfileFilePath = expandedPath
			} else {
				log.Printf("Warning: Could not expand home path for PROFILE_FILE_PATH: %v", err)
				cfg.ProfileFilePath = value
			}
		case "RESYNC_FILE_PATH":
			if expandedPath, err := ExpandHomePath(value); err == nil {
				cfg.ResyncFilePath = expandedPath
			} else {
				log.Printf("Warning: Could not expand home path for RESYNC_FILE_PATH: %v", err)
				cfg.ResyncFilePath = value
			}
		case "RCLONE_FILE_PATH":
			if expandedPath, err := ExpandHomePath(value); err == nil {
				cfg.RcloneFilePath = expandedPath
			} else {
				log.Printf("Warning: Could not expand home path for RCLONE_FILE_PATH: %v", err)
				cfg.RcloneFilePath = value
			}
		}
	}

	if err := scanner.Err(); err != nil {
		log.Printf("Error parsing config: %v", err)
	}

	return cfg
}
