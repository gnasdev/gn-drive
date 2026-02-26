package errors

import (
	"context"
	"encoding/json"
	"fmt"
	"log"
	"os"
	"path/filepath"
	"runtime/debug"
	"time"
)

// ErrorHandler provides centralized error handling functionality
type ErrorHandler struct {
	logger *log.Logger
	debug  bool
}

// NewErrorHandler creates a new error handler
func NewErrorHandler(debug bool) *ErrorHandler {
	// Create logger that writes to both console and file
	logFile := getLogFile()
	logger := log.New(logFile, "[ERROR] ", log.LstdFlags|log.Lshortfile)

	return &ErrorHandler{
		logger: logger,
		debug:  debug,
	}
}

// getLogFile returns a file handle for logging errors
func getLogFile() *os.File {
	wd, err := os.Getwd()
	if err != nil {
		log.Printf("Failed to get working directory: %v", err)
		return os.Stderr
	}

	logPath := filepath.Join(wd, "gn-drive-errors.log")
	file, err := os.OpenFile(logPath, os.O_APPEND|os.O_CREATE|os.O_WRONLY, 0644)
	if err != nil {
		log.Printf("Failed to open log file: %v", err)
		return os.Stderr
	}

	return file
}

// Handle processes an error and returns a standardized AppError
func (h *ErrorHandler) Handle(err error, context ...string) *AppError {
	if err == nil {
		return nil
	}

	// If it's already an AppError, just log and return
	if appErr, ok := err.(*AppError); ok {
		h.logError(appErr, context...)
		return appErr
	}

	// Convert regular error to AppError
	appErr := h.convertToAppError(err, context...)
	h.logError(appErr, context...)
	return appErr
}

// HandleWithCode processes an error with a specific error code
func (h *ErrorHandler) HandleWithCode(err error, code ErrorCode, message string, context ...string) *AppError {
	if err == nil {
		return nil
	}

	appErr := WrapError(err, code, message)
	h.logError(appErr, context...)
	return appErr
}

// HandleValidation handles validation errors
func (h *ErrorHandler) HandleValidation(field, message string) *AppError {
	appErr := NewAppErrorWithDetails(ValidationError,
		fmt.Sprintf("Validation failed for field '%s'", field), message)
	h.logError(appErr, "validation")
	return appErr
}

// HandleNotFound handles resource not found errors
func (h *ErrorHandler) HandleNotFound(resource, identifier string) *AppError {
	appErr := NewAppErrorWithDetails(NotFoundError,
		fmt.Sprintf("%s not found", resource),
		fmt.Sprintf("No %s found with identifier: %s", resource, identifier))
	h.logError(appErr, "not_found")
	return appErr
}

// HandleRcloneError handles rclone-specific errors
func (h *ErrorHandler) HandleRcloneError(err error, operation string) *AppError {
	if err == nil {
		return nil
	}

	appErr := NewAppErrorWithCause(RcloneError,
		fmt.Sprintf("Rclone operation '%s' failed", operation), err)
	h.logError(appErr, "rclone", operation)
	return appErr
}

// HandleFileSystemError handles file system errors
func (h *ErrorHandler) HandleFileSystemError(err error, path string) *AppError {
	if err == nil {
		return nil
	}

	appErr := NewAppErrorWithDetails(FileSystemError,
		"File system operation failed",
		fmt.Sprintf("Path: %s, Error: %s", path, err.Error()))
	h.logError(appErr, "filesystem")
	return appErr
}

// convertToAppError converts a regular error to AppError
func (h *ErrorHandler) convertToAppError(err error, context ...string) *AppError {
	// Try to categorize the error based on its content
	errMsg := err.Error()

	switch {
	case contains(errMsg, "not found", "no such file", "does not exist"):
		return NewAppErrorWithCause(NotFoundError, "Resource not found", err)
	case contains(errMsg, "permission denied", "access denied"):
		return NewAppErrorWithCause(AuthorizationError, "Access denied", err)
	case contains(errMsg, "timeout", "deadline exceeded"):
		return NewAppErrorWithCause(TimeoutError, "Operation timed out", err)
	case contains(errMsg, "network", "connection", "dial"):
		return NewAppErrorWithCause(NetworkError, "Network error", err)
	case contains(errMsg, "invalid", "malformed", "parse"):
		return NewAppErrorWithCause(ValidationError, "Invalid input", err)
	default:
		return NewAppErrorWithCause(InternalError, "Internal server error", err)
	}
}

// logError logs the error with context and stack trace if in debug mode
func (h *ErrorHandler) logError(appErr *AppError, context ...string) {
	contextStr := ""
	if len(context) > 0 {
		contextStr = fmt.Sprintf(" [Context: %v]", context)
	}

	// Basic error logging
	h.logger.Printf("TraceID: %s | Code: %s | Message: %s%s",
		appErr.TraceID, appErr.Code, appErr.Message, contextStr)

	if appErr.Details != "" {
		h.logger.Printf("TraceID: %s | Details: %s", appErr.TraceID, appErr.Details)
	}

	// In debug mode, log stack trace
	if h.debug {
		h.logger.Printf("TraceID: %s | Stack Trace:\n%s", appErr.TraceID, debug.Stack())
	}

	// If there's a cause, log it too
	if appErr.Cause != nil {
		h.logger.Printf("TraceID: %s | Underlying Error: %v", appErr.TraceID, appErr.Cause)
	}
}

// contains checks if any of the substrings exist in the main string (case-insensitive)
func contains(s string, substrings ...string) bool {
	// s is already a string, no conversion needed
	for _, substr := range substrings {
		if len(s) >= len(substr) {
			for i := 0; i <= len(s)-len(substr); i++ {
				match := true
				for j := 0; j < len(substr); j++ {
					if s[i+j] != substr[j] && s[i+j] != substr[j]-32 && s[i+j] != substr[j]+32 {
						match = false
						break
					}
				}
				if match {
					return true
				}
			}
		}
	}
	return false
}

// Recovery handles panic recovery and converts to AppError
func (h *ErrorHandler) Recovery(ctx context.Context) *AppError {
	if r := recover(); r != nil {
		var err error
		switch x := r.(type) {
		case string:
			err = fmt.Errorf("panic: %s", x)
		case error:
			err = fmt.Errorf("panic: %w", x)
		default:
			err = fmt.Errorf("panic: %v", x)
		}

		appErr := NewAppErrorWithCause(InternalError, "Application panic recovered", err)

		// Log panic with full stack trace
		h.logger.Printf("PANIC RECOVERED | TraceID: %s | Error: %v", appErr.TraceID, err)
		h.logger.Printf("PANIC STACK TRACE | TraceID: %s |\n%s", appErr.TraceID, debug.Stack())

		return appErr
	}
	return nil
}

// ToJSON converts AppError to JSON for API responses
func (appErr *AppError) ToJSON() ([]byte, error) {
	response := map[string]interface{}{
		"error": map[string]interface{}{
			"code":      appErr.Code,
			"message":   appErr.Message,
			"timestamp": appErr.Timestamp.Format(time.RFC3339),
			"trace_id":  appErr.TraceID,
		},
	}

	if appErr.Details != "" {
		response["error"].(map[string]interface{})["details"] = appErr.Details
	}

	return json.Marshal(response)
}
