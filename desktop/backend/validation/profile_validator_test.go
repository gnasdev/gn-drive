package validation

import (
	"desktop/backend/models"
	"fmt"
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

func TestValidateSizeSuffix(t *testing.T) {
	v := NewProfileValidator()

	tests := []struct {
		value   string
		wantErr bool
	}{
		{"", false},       // Empty = not set
		{"100", false},    // Plain number
		{"100K", false},   // Kilobytes
		{"10M", false},    // Megabytes
		{"1G", false},     // Gigabytes
		{"1.5G", false},   // Decimal
		{"100KiB", false}, // Explicit binary
		{"off", false},    // Special value
		{"OFF", false},    // Case insensitive
		{"abc", true},     // Invalid
		{"-10M", true},    // Negative
		{"10X", true},     // Invalid suffix
	}

	for _, tt := range tests {
		err := v.ValidateSizeSuffix(tt.value, "test_field")
		if (err != nil) != tt.wantErr {
			t.Errorf("ValidateSizeSuffix(%q) error = %v, wantErr %v", tt.value, err, tt.wantErr)
		}
	}
}

func TestValidateDuration(t *testing.T) {
	v := NewProfileValidator()

	tests := []struct {
		value   string
		wantErr bool
	}{
		{"", false},       // Empty = not set
		{"30s", false},    // Seconds
		{"5m", false},     // Minutes
		{"1h", false},     // Hours
		{"1h30m", false},  // Combined
		{"2h45m30s", false}, // Full format
		{"invalid", true},   // Invalid
		{"abc", true},       // Invalid
		{"1x", true},        // Invalid unit
	}

	for _, tt := range tests {
		err := v.ValidateDuration(tt.value, "test_field")
		if (err != nil) != tt.wantErr {
			t.Errorf("ValidateDuration(%q) error = %v, wantErr %v", tt.value, err, tt.wantErr)
		}
	}
}

func TestValidateConflictResolution(t *testing.T) {
	v := NewProfileValidator()

	tests := []struct {
		value   string
		wantErr bool
	}{
		{"", false},        // Empty = default
		{"newer", false},
		{"older", false},
		{"larger", false},
		{"smaller", false},
		{"path1", false},
		{"path2", false},
		{"none", false},
		{"invalid", true},
		{"newest", true},
	}

	for _, tt := range tests {
		err := v.ValidateConflictResolution(tt.value)
		if (err != nil) != tt.wantErr {
			t.Errorf("ValidateConflictResolution(%q) error = %v, wantErr %v", tt.value, err, tt.wantErr)
		}
	}
}

func TestValidateMaxDelete(t *testing.T) {
	v := NewProfileValidator()

	intPtr := func(i int) *int { return &i }

	tests := []struct {
		value   *int
		wantErr bool
	}{
		{nil, false},         // nil = unlimited
		{intPtr(0), false},   // Zero
		{intPtr(100), false}, // Normal
		{intPtr(-1), false},  // -1 = unlimited
		{intPtr(-2), true},   // Invalid negative
	}

	for _, tt := range tests {
		err := v.ValidateMaxDelete(tt.value)
		if (err != nil) != tt.wantErr {
			label := "nil"
			if tt.value != nil {
				label = fmt.Sprintf("%d", *tt.value)
			}
			t.Errorf("ValidateMaxDelete(%s) error = %v, wantErr %v", label, err, tt.wantErr)
		}
	}
}

func TestValidateMultiThreadStreams(t *testing.T) {
	v := NewProfileValidator()

	intPtr := func(i int) *int { return &i }

	tests := []struct {
		value   *int
		wantErr bool
	}{
		{nil, false},        // nil = default
		{intPtr(0), false},  // Zero
		{intPtr(4), false},  // Default
		{intPtr(64), false}, // Max
		{intPtr(-1), true},  // Negative
		{intPtr(65), true},  // Over max
	}

	for _, tt := range tests {
		err := v.ValidateMultiThreadStreams(tt.value)
		if (err != nil) != tt.wantErr {
			label := "nil"
			if tt.value != nil {
				label = fmt.Sprintf("%d", *tt.value)
			}
			t.Errorf("ValidateMultiThreadStreams(%s) error = %v, wantErr %v", label, err, tt.wantErr)
		}
	}
}

func TestValidateRetries(t *testing.T) {
	v := NewProfileValidator()

	intPtr := func(i int) *int { return &i }

	tests := []struct {
		value   *int
		wantErr bool
	}{
		{nil, false},         // nil = default
		{intPtr(0), false},   // Zero
		{intPtr(3), false},   // Default
		{intPtr(100), false}, // Max
		{intPtr(-1), true},   // Negative
		{intPtr(101), true},  // Over max
	}

	for _, tt := range tests {
		err := v.ValidateRetries(tt.value, "retries")
		if (err != nil) != tt.wantErr {
			label := "nil"
			if tt.value != nil {
				label = fmt.Sprintf("%d", *tt.value)
			}
			t.Errorf("ValidateRetries(%s) error = %v, wantErr %v", label, err, tt.wantErr)
		}
	}
}

func TestValidateRegexPatterns(t *testing.T) {
	v := NewProfileValidator()

	tests := []struct {
		patterns []string
		wantErr  bool
	}{
		{[]string{}, false},                         // Empty
		{[]string{`.*\.txt$`}, false},               // Valid regex
		{[]string{`^test`, `\d+`}, false},           // Multiple valid
		{[]string{`[invalid`}, true},                // Invalid regex (unclosed bracket)
		{[]string{`valid`, `[invalid`}, true},       // One invalid
	}

	for _, tt := range tests {
		err := v.ValidateRegexPatterns(tt.patterns, "test_field")
		if (err != nil) != tt.wantErr {
			t.Errorf("ValidateRegexPatterns(%v) error = %v, wantErr %v", tt.patterns, err, tt.wantErr)
		}
	}
}

func TestValidateProfile_WithNewFields(t *testing.T) {
	v := NewProfileValidator()

	intPtr := func(i int) *int { return &i }

	// Valid profile with all new fields
	profile := models.Profile{
		Name:               "test-profile",
		From:               "/home/user/docs",
		To:                 "gdrive:backup",
		Parallel:           16,
		MinSize:            "100K",
		MaxSize:            "1G",
		ConflictResolution: "newer",
		MaxDelete:          intPtr(100),
		MultiThreadStreams:  intPtr(4),
		BufferSize:         "64M",
		Retries:            intPtr(3),
		LowLevelRetries:    intPtr(10),
		MaxDuration:        "2h",
	}

	err := v.ValidateProfile(profile)
	if err != nil {
		t.Errorf("Expected no error for valid profile with new fields, got: %v", err)
	}

	// Invalid min_size
	profile2 := models.Profile{
		Name:    "test",
		From:    "/home/user",
		To:      "gdrive:backup",
		MinSize: "invalid",
	}
	err = v.ValidateProfile(profile2)
	if err == nil {
		t.Error("Expected error for invalid min_size")
	}

	// Invalid conflict resolution
	profile3 := models.Profile{
		Name:               "test",
		From:               "/home/user",
		To:                 "gdrive:backup",
		ConflictResolution: "invalid_strategy",
	}
	err = v.ValidateProfile(profile3)
	if err == nil {
		t.Error("Expected error for invalid conflict_resolution")
	}

	// Invalid max duration
	profile4 := models.Profile{
		Name:        "test",
		From:        "/home/user",
		To:          "gdrive:backup",
		MaxDuration: "not_a_duration",
	}
	err = v.ValidateProfile(profile4)
	if err == nil {
		t.Error("Expected error for invalid max_duration")
	}
}

func TestValidateProfile_WithRegexPatterns(t *testing.T) {
	v := NewProfileValidator()

	// Valid regex patterns with UseRegex enabled
	profile := models.Profile{
		Name:          "test-regex",
		From:          "/home/user/docs",
		To:            "gdrive:backup",
		UseRegex:      true,
		IncludedPaths: []string{`.*\.txt$`, `^important`},
		ExcludedPaths: []string{`\.tmp$`},
	}

	err := v.ValidateProfile(profile)
	if err != nil {
		t.Errorf("Expected no error for valid regex patterns, got: %v", err)
	}

	// Invalid regex pattern with UseRegex enabled
	profile2 := models.Profile{
		Name:          "test-regex-invalid",
		From:          "/home/user/docs",
		To:            "gdrive:backup",
		UseRegex:      true,
		IncludedPaths: []string{`[invalid`},
	}

	err = v.ValidateProfile(profile2)
	if err == nil {
		t.Error("Expected error for invalid regex pattern when UseRegex is true")
	}

	// Invalid regex pattern but UseRegex disabled (should pass)
	profile3 := models.Profile{
		Name:          "test-no-regex",
		From:          "/home/user/docs",
		To:            "gdrive:backup",
		UseRegex:      false,
		IncludedPaths: []string{`[not-checked-as-regex`},
	}

	err = v.ValidateProfile(profile3)
	if err != nil {
		t.Errorf("Expected no error when UseRegex is false, got: %v", err)
	}
}
