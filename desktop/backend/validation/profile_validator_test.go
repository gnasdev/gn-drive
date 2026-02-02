package validation

import (
	"desktop/backend/models"
	"testing"
)

func TestValidateProfile_ValidProfile(t *testing.T) {
	v := NewProfileValidator()

	profile := models.Profile{
		Name:     "my-backup",
		From:     "/home/user/documents",
		To:       "gdrive:backup/documents",
		Parallel: 16,
	}

	err := v.ValidateProfile(profile)
	if err != nil {
		t.Errorf("Expected no error for valid profile, got: %v", err)
	}
}

func TestValidateProfile_EmptyName(t *testing.T) {
	v := NewProfileValidator()

	profile := models.Profile{
		Name: "",
		From: "/home/user/documents",
		To:   "gdrive:backup",
	}

	err := v.ValidateProfile(profile)
	if err == nil {
		t.Error("Expected error for empty name")
	}
}

func TestValidateProfile_PathTraversalInName(t *testing.T) {
	v := NewProfileValidator()

	profile := models.Profile{
		Name: "../etc/passwd",
		From: "/home/user",
		To:   "gdrive:backup",
	}

	err := v.ValidateProfile(profile)
	if err == nil {
		t.Error("Expected error for path traversal in name")
	}
}

func TestValidateRclonePath_LocalPath(t *testing.T) {
	v := NewProfileValidator()

	tests := []struct {
		path    string
		wantErr bool
	}{
		{"/home/user/documents", false},
		{"/", true},       // Too short
		{"", true},        // Empty
		{"../etc", true},  // Path traversal
	}

	for _, tt := range tests {
		err := v.ValidateRclonePath(tt.path, "test")
		if (err != nil) != tt.wantErr {
			t.Errorf("ValidateRclonePath(%q) error = %v, wantErr %v", tt.path, err, tt.wantErr)
		}
	}
}

func TestValidateRclonePath_RemotePath(t *testing.T) {
	v := NewProfileValidator()

	tests := []struct {
		path    string
		wantErr bool
	}{
		{"gdrive:backup", false},
		{"my-remote:path/to/files", false},
		{"remote_name:folder", false},
		{":path", true},           // Empty remote name
		{"invalid@name:path", true}, // Invalid characters in remote name
		{"name", true},             // No colon separator
	}

	for _, tt := range tests {
		err := v.ValidateRclonePath(tt.path, "test")
		if (err != nil) != tt.wantErr {
			t.Errorf("ValidateRclonePath(%q) error = %v, wantErr %v", tt.path, err, tt.wantErr)
		}
	}
}

func TestValidateParallel(t *testing.T) {
	v := NewProfileValidator()

	tests := []struct {
		parallel int
		wantErr  bool
	}{
		{0, false},
		{16, false},
		{256, false},
		{-1, true},  // Negative
		{257, true}, // Too high
	}

	for _, tt := range tests {
		err := v.ValidateParallel(tt.parallel)
		if (err != nil) != tt.wantErr {
			t.Errorf("ValidateParallel(%d) error = %v, wantErr %v", tt.parallel, err, tt.wantErr)
		}
	}
}

func TestValidateBandwidth(t *testing.T) {
	v := NewProfileValidator()

	tests := []struct {
		bandwidth int
		wantErr   bool
	}{
		{0, false},
		{100, false},
		{10000, false},
		{-1, true},    // Negative
		{10001, true}, // Too high
	}

	for _, tt := range tests {
		err := v.ValidateBandwidth(tt.bandwidth)
		if (err != nil) != tt.wantErr {
			t.Errorf("ValidateBandwidth(%d) error = %v, wantErr %v", tt.bandwidth, err, tt.wantErr)
		}
	}
}

func TestValidateRemoteName(t *testing.T) {
	tests := []struct {
		name    string
		wantErr bool
	}{
		{"gdrive", false},
		{"my-remote", false},
		{"remote_123", false},
		{"", true},              // Empty
		{"invalid@name", true},  // Invalid character
		{"name with space", true}, // Space
	}

	for _, tt := range tests {
		err := ValidateRemoteName(tt.name)
		if (err != nil) != tt.wantErr {
			t.Errorf("ValidateRemoteName(%q) error = %v, wantErr %v", tt.name, err, tt.wantErr)
		}
	}
}
