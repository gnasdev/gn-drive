package rclone

import (
	"context"
	"testing"

	"github.com/rclone/rclone/fs"
	"github.com/rclone/rclone/fs/config"
)

// TestProviderRegistration tests that all required providers are registered
func TestProviderRegistration(t *testing.T) {
	// Test cases for each provider
	testCases := []struct {
		name     string
		provider string
	}{
		{"Google Drive", "drive"},
		{"Dropbox", "dropbox"},
		{"OneDrive", "onedrive"},
		{"Yandex Disk", "yandex"},
		{"Google Photos", "googlephotos"},
		{"iCloud Drive", "iclouddrive"},
		{"Local", "local"},
		{"Cache", "cache"},
	}

	for _, tc := range testCases {
		t.Run(tc.name, func(t *testing.T) {
			// Check if provider is registered
			_, err := fs.Find(tc.provider)
			if err != nil {
				t.Errorf("Provider %s (%s) is not registered: %v", tc.name, tc.provider, err)
			}
		})
	}
}

// TestRemoteTypeValidation tests the remote type validation
func TestRemoteTypeValidation(t *testing.T) {
	validTypes := []string{
		"drive",
		"dropbox",
		"onedrive",
		"yandex",
		"googlephotos",
		"iclouddrive",
		"local",
		"cache",
	}

	for _, remoteType := range validTypes {
		t.Run("Valid_"+remoteType, func(t *testing.T) {
			_, err := fs.Find(remoteType)
			if err != nil {
				t.Errorf("Remote type %s should be valid but got error: %v", remoteType, err)
			}
		})
	}

	// Test invalid types
	invalidTypes := []string{
		"invalid",
		"nonexistent",
		"",
	}

	for _, remoteType := range invalidTypes {
		t.Run("Invalid_"+remoteType, func(t *testing.T) {
			_, err := fs.Find(remoteType)
			if err == nil {
				t.Errorf("Remote type %s should be invalid but was found", remoteType)
			}
		})
	}
}

// TestConfigCreation tests basic config creation for new providers
func TestConfigCreation(t *testing.T) {
	ctx := context.Background()

	testCases := []struct {
		name         string
		remoteType   string
		shouldCreate bool
	}{
		{"Google Photos", "googlephotos", true},
		{"iCloud Drive", "iclouddrive", true},
		{"Invalid Type", "invalid", false},
	}

	for _, tc := range testCases {
		t.Run(tc.name, func(t *testing.T) {
			remoteName := "test_" + tc.remoteType

			// Clean up any existing config
			config.DeleteRemote(remoteName)

			// Try to create remote config
			_, err := config.CreateRemote(ctx, remoteName, tc.remoteType, nil, config.UpdateRemoteOpt{})

			if tc.shouldCreate {
				// For valid types, we expect either success or authentication error
				// (since we're not providing actual credentials)
				if err != nil {
					// Check if it's an authentication/config error (expected)
					// or a "type not found" error (unexpected)
					if err == fs.ErrorNotFoundInConfigFile {
						t.Errorf("Remote type %s not found in config, provider may not be registered", tc.remoteType)
					}
					// Other errors (like missing auth) are expected in tests
				}
			} else {
				// For invalid types, we expect an error
				if err == nil {
					t.Errorf("Expected error for invalid remote type %s", tc.remoteType)
				}
			}

			// Clean up
			config.DeleteRemote(remoteName)
		})
	}
}
