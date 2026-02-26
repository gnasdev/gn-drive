package backend

import (
	"desktop/backend/models"
	"encoding/json"
	"os"
	"path/filepath"
	"testing"
)

func TestDeleteRemote_CascadeDeleteProfiles(t *testing.T) {
	// Create a temporary directory for test files
	tempDir, err := os.MkdirTemp("", "gn-drive-test")
	if err != nil {
		t.Fatalf("Failed to create temp dir: %v", err)
	}
	defer func() {
		if err := os.RemoveAll(tempDir); err != nil {
			t.Logf("Warning: failed to remove temp dir: %v", err)
		}
	}()

	// Create test app instance
	app := NewApp()

	// Mark as initialized to prevent initializeConfig() from overwriting our test config
	app.initialized = true

	// Set up test configuration
	app.ConfigInfo.EnvConfig.ProfileFilePath = filepath.Join(tempDir, "profiles.json")
	app.ConfigInfo.EnvConfig.RcloneFilePath = filepath.Join(tempDir, "rclone.conf")

	// Create test profiles
	testProfiles := []models.Profile{
		{
			Name: "profile1",
			From: "testremote:/source",
			To:   "/local/dest",
		},
		{
			Name: "profile2",
			From: "/local/source",
			To:   "testremote:/dest",
		},
		{
			Name: "profile3",
			From: "otherremote:/source",
			To:   "anotherremote:/dest",
		},
		{
			Name: "profile4",
			From: "/local/source",
			To:   "/local/dest",
		},
	}

	app.ConfigInfo.Profiles = testProfiles

	// Save initial profiles to file
	profilesJson, err := json.Marshal(models.Profiles(app.ConfigInfo.Profiles))
	if err != nil {
		t.Fatalf("Failed to marshal profiles: %v", err)
	}

	err = os.WriteFile(app.ConfigInfo.EnvConfig.ProfileFilePath, profilesJson, 0644)
	if err != nil {
		t.Fatalf("Failed to write profiles file: %v", err)
	}

	// Create empty rclone config file
	err = os.WriteFile(app.ConfigInfo.EnvConfig.RcloneFilePath, []byte(""), 0644)
	if err != nil {
		t.Fatalf("Failed to write rclone config file: %v", err)
	}

	// Test: Delete remote that is used by profiles
	appErr := app.DeleteRemote("testremote")
	if appErr != nil {
		t.Fatalf("DeleteRemote failed: %v", appErr)
	}

	// Verify profiles were updated correctly
	expectedProfiles := []models.Profile{
		{
			Name: "profile3",
			From: "otherremote:/source",
			To:   "anotherremote:/dest",
		},
		{
			Name: "profile4",
			From: "/local/source",
			To:   "/local/dest",
		},
	}

	if len(app.ConfigInfo.Profiles) != len(expectedProfiles) {
		t.Errorf("Expected %d profiles, got %d", len(expectedProfiles), len(app.ConfigInfo.Profiles))
	}

	// Check that the correct profiles remain
	for i, expected := range expectedProfiles {
		if i >= len(app.ConfigInfo.Profiles) {
			t.Errorf("Missing profile at index %d", i)
			continue
		}

		actual := app.ConfigInfo.Profiles[i]
		if actual.Name != expected.Name {
			t.Errorf("Profile %d: expected name %s, got %s", i, expected.Name, actual.Name)
		}
		if actual.From != expected.From {
			t.Errorf("Profile %d: expected from %s, got %s", i, expected.From, actual.From)
		}
		if actual.To != expected.To {
			t.Errorf("Profile %d: expected to %s, got %s", i, expected.To, actual.To)
		}
	}

	// Verify profiles file was updated
	updatedProfilesData, err := os.ReadFile(app.ConfigInfo.EnvConfig.ProfileFilePath)
	if err != nil {
		t.Fatalf("Failed to read updated profiles file: %v", err)
	}

	// The file should contain the remaining profiles
	if len(updatedProfilesData) == 0 {
		t.Error("Profiles file is empty after update")
	}
}

func TestDeleteRemote_NoProfilesAffected(t *testing.T) {
	// Create a temporary directory for test files
	tempDir, err := os.MkdirTemp("", "gn-drive-test")
	if err != nil {
		t.Fatalf("Failed to create temp dir: %v", err)
	}
	defer func() {
		if err := os.RemoveAll(tempDir); err != nil {
			t.Logf("Warning: failed to remove temp dir: %v", err)
		}
	}()

	// Create test app instance
	app := NewApp()

	// Mark as initialized to prevent initializeConfig() from overwriting our test config
	app.initialized = true

	// Set up test configuration
	app.ConfigInfo.EnvConfig.ProfileFilePath = filepath.Join(tempDir, "profiles.json")
	app.ConfigInfo.EnvConfig.RcloneFilePath = filepath.Join(tempDir, "rclone.conf")

	// Create test profiles that don't use the remote to be deleted
	testProfiles := []models.Profile{
		{
			Name: "profile1",
			From: "otherremote:/source",
			To:   "/local/dest",
		},
		{
			Name: "profile2",
			From: "/local/source",
			To:   "/local/dest",
		},
	}

	app.ConfigInfo.Profiles = testProfiles

	// Save initial profiles to file
	profilesJson, err := json.Marshal(models.Profiles(app.ConfigInfo.Profiles))
	if err != nil {
		t.Fatalf("Failed to marshal profiles: %v", err)
	}

	err = os.WriteFile(app.ConfigInfo.EnvConfig.ProfileFilePath, profilesJson, 0644)
	if err != nil {
		t.Fatalf("Failed to write profiles file: %v", err)
	}

	// Create empty rclone config file
	err = os.WriteFile(app.ConfigInfo.EnvConfig.RcloneFilePath, []byte(""), 0644)
	if err != nil {
		t.Fatalf("Failed to write rclone config file: %v", err)
	}

	// Test: Delete remote that is NOT used by any profiles
	appErr := app.DeleteRemote("unusedremote")
	if appErr != nil {
		t.Fatalf("DeleteRemote failed: %v", appErr)
	}

	// Verify all profiles remain unchanged
	if len(app.ConfigInfo.Profiles) != len(testProfiles) {
		t.Errorf("Expected %d profiles, got %d", len(testProfiles), len(app.ConfigInfo.Profiles))
	}

	for i, expected := range testProfiles {
		if i >= len(app.ConfigInfo.Profiles) {
			t.Errorf("Missing profile at index %d", i)
			continue
		}

		actual := app.ConfigInfo.Profiles[i]
		if actual.Name != expected.Name {
			t.Errorf("Profile %d: expected name %s, got %s", i, expected.Name, actual.Name)
		}
		if actual.From != expected.From {
			t.Errorf("Profile %d: expected from %s, got %s", i, expected.From, actual.From)
		}
		if actual.To != expected.To {
			t.Errorf("Profile %d: expected to %s, got %s", i, expected.To, actual.To)
		}
	}
}
