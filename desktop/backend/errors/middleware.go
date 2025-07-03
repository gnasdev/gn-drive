package errors

import (
	"context"
	"desktop/backend/dto"
	"fmt"
	"log"
)

// Middleware provides error handling middleware for the application
type Middleware struct {
	handler *ErrorHandler
}

// NewMiddleware creates a new error handling middleware
func NewMiddleware(debug bool) *Middleware {
	return &Middleware{
		handler: NewErrorHandler(debug),
	}
}

// WrapCommand wraps a command execution with error handling
func (m *Middleware) WrapCommand(ctx context.Context, commandName string, fn func() error) error {
	// Add panic recovery
	defer func() {
		if appErr := m.handler.Recovery(ctx); appErr != nil {
			log.Printf("Command '%s' panicked: %v", commandName, appErr)
		}
	}()

	err := fn()
	if err != nil {
		return m.handler.HandleWithCode(err, OperationFailed,
			"Command execution failed", commandName)
	}
	return nil
}

// WrapSync wraps sync operations with specific error handling
func (m *Middleware) WrapSync(ctx context.Context, operation string, fn func() error) error {
	defer func() {
		if appErr := m.handler.Recovery(ctx); appErr != nil {
			log.Printf("Sync operation '%s' panicked: %v", operation, appErr)
		}
	}()

	err := fn()
	if err != nil {
		return m.handler.HandleRcloneError(err, operation)
	}
	return nil
}

// WrapRemoteOperation wraps remote operations with error handling
func (m *Middleware) WrapRemoteOperation(ctx context.Context, remoteName, operation string, fn func() error) error {
	defer func() {
		if appErr := m.handler.Recovery(ctx); appErr != nil {
			log.Printf("Remote operation '%s' on '%s' panicked: %v", operation, remoteName, appErr)
		}
	}()

	err := fn()
	if err != nil {
		return m.handler.HandleWithCode(err, ExternalServiceError,
			"Remote operation failed", remoteName, operation)
	}
	return nil
}

// WrapFileOperation wraps file system operations with error handling
func (m *Middleware) WrapFileOperation(ctx context.Context, path string, fn func() error) error {
	defer func() {
		if appErr := m.handler.Recovery(ctx); appErr != nil {
			log.Printf("File operation on '%s' panicked: %v", path, appErr)
		}
	}()

	err := fn()
	if err != nil {
		return m.handler.HandleFileSystemError(err, path)
	}
	return nil
}

// HandleError processes any error and returns appropriate DTO for frontend
func (m *Middleware) HandleError(err error, context ...string) dto.CommandDTO {
	if err == nil {
		return dto.CommandDTO{}
	}

	appErr := m.handler.Handle(err, context...)

	// Convert to CommandDTO for frontend communication
	errorString := appErr.Error()
	return dto.CommandDTO{
		Command: dto.Error.String(),
		Error:   &errorString,
	}
}

// HandleErrorWithTab processes error and returns DTO with tab ID
func (m *Middleware) HandleErrorWithTab(err error, tabId string, context ...string) dto.CommandDTO {
	if err == nil {
		return dto.CommandDTO{}
	}

	appErr := m.handler.Handle(err, context...)

	errorString := appErr.Error()
	return dto.CommandDTO{
		Command: dto.Error.String(),
		Error:   &errorString,
		TabId:   &tabId,
	}
}

// ValidateRequired validates that required fields are present
func (m *Middleware) ValidateRequired(fields map[string]interface{}) error {
	for fieldName, value := range fields {
		if value == nil {
			return m.handler.HandleValidation(fieldName, "Field is required")
		}

		// Check for empty strings
		if str, ok := value.(string); ok && str == "" {
			return m.handler.HandleValidation(fieldName, "Field cannot be empty")
		}

		// Check for empty slices
		if slice, ok := value.([]interface{}); ok && len(slice) == 0 {
			return m.handler.HandleValidation(fieldName, "Field cannot be empty")
		}
	}
	return nil
}

// ValidateStringLength validates string length constraints
func (m *Middleware) ValidateStringLength(fieldName, value string, minLen, maxLen int) error {
	if len(value) < minLen {
		return m.handler.HandleValidation(fieldName,
			fmt.Sprintf("Field must be at least %d characters long", minLen))
	}
	if maxLen > 0 && len(value) > maxLen {
		return m.handler.HandleValidation(fieldName,
			fmt.Sprintf("Field must be no more than %d characters long", maxLen))
	}
	return nil
}

// ValidatePathExists validates that a file or directory path exists
func (m *Middleware) ValidatePathExists(fieldName, path string) error {
	if path == "" {
		return m.handler.HandleValidation(fieldName, "Path cannot be empty")
	}

	// This would typically check if path exists, but since we're dealing with
	// remote paths that might not be locally accessible, we'll do basic validation
	if len(path) > 1000 {
		return m.handler.HandleValidation(fieldName, "Path is too long")
	}

	return nil
}

// CreateErrorResponse creates a standardized error response for API calls
func (m *Middleware) CreateErrorResponse(err error) ([]byte, error) {
	if err == nil {
		return nil, nil
	}

	appErr := m.handler.Handle(err)
	return appErr.ToJSON()
}

// LogInfo logs informational messages with trace ID if available
func (m *Middleware) LogInfo(message string, err error) {
	traceID := GetTraceID(err)
	if traceID != "" {
		log.Printf("[INFO] TraceID: %s | %s", traceID, message)
	} else {
		log.Printf("[INFO] %s", message)
	}
}

// LogWarning logs warning messages with trace ID if available
func (m *Middleware) LogWarning(message string, err error) {
	traceID := GetTraceID(err)
	if traceID != "" {
		log.Printf("[WARNING] TraceID: %s | %s", traceID, message)
	} else {
		log.Printf("[WARNING] %s", message)
	}
}
