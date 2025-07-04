package utils

import (
	"bufio"
	_ "embed"
	"fmt"
	"log"
	"os"
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
	// Create a Config struct with default values
	cfg := beConfig.Config{
		DebugMode:       false,
		ProfileFilePath: ".config/.profiles",
		ResyncFilePath:  ".config/.resync",
		RcloneFilePath:  ".config/rclone.conf",
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
			cfg.ProfileFilePath = value
		case "RESYNC_FILE_PATH":
			cfg.ResyncFilePath = value
		case "RCLONE_FILE_PATH":
			cfg.RcloneFilePath = value
		}
	}

	if err := scanner.Err(); err != nil {
		log.Printf("Error parsing config: %v", err)
	}

	return cfg
}
