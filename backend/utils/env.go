package utils

import (
	"bufio"
	"bytes"
	_ "embed"
	"log"
	"os"
	"strings"

	beConfig "ns-drive/backend/config"

	"github.com/spf13/viper"
)

func LoadEnvFile(filename string) error {
	file, err := os.Open(filename)
	if err != nil {
		return err
	}
	defer file.Close()

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
		os.Setenv(key, value)
	}

	return scanner.Err()
}

func LoadEnvConfigFromEnvStr(envConfigStr string) beConfig.Config {
	viper.SetConfigType("env")
	err := viper.ReadConfig(bytes.NewBuffer([]byte(envConfigStr)))
	if err != nil {
		log.Printf("Error reading config: %v", err)
	}

	// Create a Config struct
	var cfg beConfig.Config

	// Parse environment variables
	err = viper.Unmarshal(&cfg)
	if err != nil {
		log.Fatalf("Error parsing configuration: %s", err)
	}

	return cfg
}
