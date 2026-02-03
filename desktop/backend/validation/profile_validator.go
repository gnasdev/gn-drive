package validation

import (
	"desktop/backend/models"
	"fmt"
	"regexp"
	"strings"
	"time"
)

// ProfileValidator handles profile validation
type ProfileValidator struct{}

// NewProfileValidator creates a new profile validator
func NewProfileValidator() *ProfileValidator {
	return &ProfileValidator{}
}

// remoteNamePattern matches valid rclone remote names (alphanumeric, dash, underscore)
var remoteNamePattern = regexp.MustCompile(`^[a-zA-Z0-9_-]+$`)

// sizeSuffixPattern matches rclone size suffix format: number followed by optional unit
var sizeSuffixPattern = regexp.MustCompile(`^(?i)(\d+(\.\d+)?)\s*([KMGTPE](i?B)?|B)?$|^(?i)off$`)

// validConflictResolutions lists allowed bisync conflict resolution strategies
var validConflictResolutions = map[string]bool{
	"":        true, // empty = default (newer)
	"none":    true,
	"newer":   true,
	"older":   true,
	"larger":  true,
	"smaller": true,
	"path1":   true,
	"path2":   true,
}

// ValidationError represents a validation error with field context
type ValidationError struct {
	Field   string
	Message string
}

func (e *ValidationError) Error() string {
	return fmt.Sprintf("%s: %s", e.Field, e.Message)
}

// ValidateProfile validates a complete profile
func (v *ProfileValidator) ValidateProfile(profile models.Profile) error {
	if err := v.ValidateName(profile.Name); err != nil {
		return err
	}
	if err := v.ValidateRclonePath(profile.From, "from"); err != nil {
		return err
	}
	if err := v.ValidateRclonePath(profile.To, "to"); err != nil {
		return err
	}
	if err := v.ValidateParallel(profile.Parallel); err != nil {
		return err
	}
	if err := v.ValidateBandwidth(profile.Bandwidth); err != nil {
		return err
	}
	if err := v.ValidatePaths(profile.IncludedPaths, "included_paths"); err != nil {
		return err
	}
	if err := v.ValidatePaths(profile.ExcludedPaths, "excluded_paths"); err != nil {
		return err
	}

	// Validate new filtering fields
	if err := v.ValidateSizeSuffix(profile.MinSize, "min_size"); err != nil {
		return err
	}
	if err := v.ValidateSizeSuffix(profile.MaxSize, "max_size"); err != nil {
		return err
	}
	if err := v.ValidateConflictResolution(profile.ConflictResolution); err != nil {
		return err
	}
	if err := v.ValidateMaxDelete(profile.MaxDelete); err != nil {
		return err
	}
	if err := v.ValidateSizeSuffix(profile.BufferSize, "buffer_size"); err != nil {
		return err
	}
	if err := v.ValidateDuration(profile.MaxDuration, "max_duration"); err != nil {
		return err
	}
	if err := v.ValidateMultiThreadStreams(profile.MultiThreadStreams); err != nil {
		return err
	}
	if err := v.ValidateRetries(profile.Retries, "retries"); err != nil {
		return err
	}
	if err := v.ValidateRetries(profile.LowLevelRetries, "low_level_retries"); err != nil {
		return err
	}
	if profile.UseRegex {
		if err := v.ValidateRegexPatterns(profile.IncludedPaths, "included_paths"); err != nil {
			return err
		}
		if err := v.ValidateRegexPatterns(profile.ExcludedPaths, "excluded_paths"); err != nil {
			return err
		}
	}

	return nil
}

// ValidateName validates profile name
func (v *ProfileValidator) ValidateName(name string) error {
	if name == "" {
		return &ValidationError{Field: "name", Message: "cannot be empty"}
	}
	if len(name) > 100 {
		return &ValidationError{Field: "name", Message: "cannot exceed 100 characters"}
	}
	// Prevent path traversal characters in name
	if strings.Contains(name, "..") || strings.Contains(name, "/") || strings.Contains(name, "\\") {
		return &ValidationError{Field: "name", Message: "contains invalid characters"}
	}
	return nil
}

// ValidateRclonePath validates an rclone path (local or remote)
func (v *ProfileValidator) ValidateRclonePath(path string, fieldName string) error {
	if path == "" {
		return &ValidationError{Field: fieldName, Message: "cannot be empty"}
	}

	// Check for path traversal attempts
	if strings.Contains(path, "..") {
		return &ValidationError{Field: fieldName, Message: "path traversal not allowed"}
	}

	// Local path (starts with / on Unix or drive letter on Windows)
	if strings.HasPrefix(path, "/") || (len(path) >= 2 && path[1] == ':') {
		return v.validateLocalPath(path, fieldName)
	}

	// Remote path format: remotename:path
	return v.validateRemotePath(path, fieldName)
}

// validateLocalPath validates a local filesystem path
func (v *ProfileValidator) validateLocalPath(path string, fieldName string) error {
	// Basic validation - path shouldn't be empty after prefix
	if path == "/" || (len(path) >= 2 && path[1] == ':' && len(path) <= 3) {
		return &ValidationError{Field: fieldName, Message: "path too short"}
	}

	// Check for null bytes (security)
	if strings.Contains(path, "\x00") {
		return &ValidationError{Field: fieldName, Message: "contains invalid characters"}
	}

	return nil
}

// validateRemotePath validates an rclone remote path (format: remote:path)
func (v *ProfileValidator) validateRemotePath(path string, fieldName string) error {
	parts := strings.SplitN(path, ":", 2)
	if len(parts) != 2 {
		return &ValidationError{Field: fieldName, Message: "invalid remote path format (expected remote:path)"}
	}

	remoteName := parts[0]
	remotePath := parts[1]

	// Validate remote name
	if remoteName == "" {
		return &ValidationError{Field: fieldName, Message: "remote name cannot be empty"}
	}
	if !remoteNamePattern.MatchString(remoteName) {
		return &ValidationError{Field: fieldName, Message: "remote name contains invalid characters (only alphanumeric, dash, underscore allowed)"}
	}
	if len(remoteName) > 50 {
		return &ValidationError{Field: fieldName, Message: "remote name too long (max 50 characters)"}
	}

	// Remote path can be empty (root of remote)
	// But check for null bytes
	if strings.Contains(remotePath, "\x00") {
		return &ValidationError{Field: fieldName, Message: "path contains invalid characters"}
	}

	return nil
}

// ValidateParallel validates the parallel transfers setting
func (v *ProfileValidator) ValidateParallel(parallel int) error {
	if parallel < 0 {
		return &ValidationError{Field: "parallel", Message: "cannot be negative"}
	}
	if parallel > 256 {
		return &ValidationError{Field: "parallel", Message: "cannot exceed 256"}
	}
	return nil
}

// ValidateBandwidth validates the bandwidth limit setting
func (v *ProfileValidator) ValidateBandwidth(bandwidth int) error {
	if bandwidth < 0 {
		return &ValidationError{Field: "bandwidth", Message: "cannot be negative"}
	}
	// Max 10 Gbps (10000 MB/s) - reasonable limit
	if bandwidth > 10000 {
		return &ValidationError{Field: "bandwidth", Message: "cannot exceed 10000 MB/s"}
	}
	return nil
}

// ValidatePaths validates include/exclude path patterns
func (v *ProfileValidator) ValidatePaths(paths []string, fieldName string) error {
	for i, path := range paths {
		if strings.Contains(path, "\x00") {
			return &ValidationError{
				Field:   fmt.Sprintf("%s[%d]", fieldName, i),
				Message: "contains invalid characters",
			}
		}
		// Check for extremely long patterns
		if len(path) > 1000 {
			return &ValidationError{
				Field:   fmt.Sprintf("%s[%d]", fieldName, i),
				Message: "pattern too long (max 1000 characters)",
			}
		}
	}
	return nil
}

// ValidateRemoteName validates an rclone remote name
func ValidateRemoteName(name string) error {
	if name == "" {
		return &ValidationError{Field: "remote_name", Message: "cannot be empty"}
	}
	if !remoteNamePattern.MatchString(name) {
		return &ValidationError{Field: "remote_name", Message: "contains invalid characters (only alphanumeric, dash, underscore allowed)"}
	}
	if len(name) > 50 {
		return &ValidationError{Field: "remote_name", Message: "too long (max 50 characters)"}
	}
	return nil
}

// ValidateSizeSuffix validates an rclone size suffix string (e.g. "100K", "10M", "1G", "off")
func (v *ProfileValidator) ValidateSizeSuffix(value string, fieldName string) error {
	if value == "" {
		return nil
	}
	if !sizeSuffixPattern.MatchString(value) {
		return &ValidationError{
			Field:   fieldName,
			Message: "invalid size format (use number with optional K/M/G/T/P suffix, e.g. '100K', '10M', '1G', or 'off')",
		}
	}
	return nil
}

// ValidateDuration validates a Go duration string (e.g. "1h30m", "45s")
func (v *ProfileValidator) ValidateDuration(value string, fieldName string) error {
	if value == "" {
		return nil
	}
	if _, err := time.ParseDuration(value); err != nil {
		return &ValidationError{
			Field:   fieldName,
			Message: fmt.Sprintf("invalid duration format (use Go duration syntax, e.g. '1h30m', '45s'): %v", err),
		}
	}
	return nil
}

// ValidateConflictResolution validates a bisync conflict resolution strategy
func (v *ProfileValidator) ValidateConflictResolution(value string) error {
	if !validConflictResolutions[value] {
		return &ValidationError{
			Field:   "conflict_resolution",
			Message: "must be one of: none, newer, older, larger, smaller, path1, path2",
		}
	}
	return nil
}

// ValidateMaxDelete validates the max delete limit
func (v *ProfileValidator) ValidateMaxDelete(value *int) error {
	if value == nil {
		return nil
	}
	if *value < -1 {
		return &ValidationError{Field: "max_delete", Message: "cannot be less than -1 (-1 means unlimited)"}
	}
	return nil
}

// ValidateMultiThreadStreams validates the multi-thread streams setting
func (v *ProfileValidator) ValidateMultiThreadStreams(value *int) error {
	if value == nil {
		return nil
	}
	if *value < 0 {
		return &ValidationError{Field: "multi_thread_streams", Message: "cannot be negative"}
	}
	if *value > 64 {
		return &ValidationError{Field: "multi_thread_streams", Message: "cannot exceed 64"}
	}
	return nil
}

// ValidateRetries validates retry count settings
func (v *ProfileValidator) ValidateRetries(value *int, fieldName string) error {
	if value == nil {
		return nil
	}
	if *value < 0 {
		return &ValidationError{Field: fieldName, Message: "cannot be negative"}
	}
	if *value > 100 {
		return &ValidationError{Field: fieldName, Message: "cannot exceed 100"}
	}
	return nil
}

// ValidateRegexPatterns validates that patterns are valid regular expressions
func (v *ProfileValidator) ValidateRegexPatterns(patterns []string, fieldName string) error {
	for i, pattern := range patterns {
		if _, err := regexp.Compile(pattern); err != nil {
			return &ValidationError{
				Field:   fmt.Sprintf("%s[%d]", fieldName, i),
				Message: fmt.Sprintf("invalid regex pattern: %v", err),
			}
		}
	}
	return nil
}
