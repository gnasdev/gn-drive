package errors

import (
	"fmt"
	"time"

	"github.com/google/uuid"
)

// ErrorCode represents different types of errors in the application
type ErrorCode string

const (
	// Validation errors
	ValidationError ErrorCode = "VALIDATION_ERROR"
	InvalidInput    ErrorCode = "INVALID_INPUT"
	MissingField    ErrorCode = "MISSING_FIELD"

	// Authentication and authorization errors
	AuthenticationError ErrorCode = "AUTHENTICATION_ERROR"
	AuthorizationError  ErrorCode = "AUTHORIZATION_ERROR"
	InvalidCredentials  ErrorCode = "INVALID_CREDENTIALS"

	// Resource errors
	NotFoundError    ErrorCode = "NOT_FOUND_ERROR"
	ConflictError    ErrorCode = "CONFLICT_ERROR"
	AlreadyExistsError ErrorCode = "ALREADY_EXISTS_ERROR"

	// System errors
	InternalError     ErrorCode = "INTERNAL_ERROR"
	DatabaseError     ErrorCode = "DATABASE_ERROR"
	FileSystemError   ErrorCode = "FILESYSTEM_ERROR"
	ConfigurationError ErrorCode = "CONFIGURATION_ERROR"

	// External service errors
	ExternalServiceError ErrorCode = "EXTERNAL_SERVICE_ERROR"
	RcloneError         ErrorCode = "RCLONE_ERROR"
	NetworkError        ErrorCode = "NETWORK_ERROR"
	TimeoutError        ErrorCode = "TIMEOUT_ERROR"

	// Business logic errors
	BusinessLogicError ErrorCode = "BUSINESS_LOGIC_ERROR"
	OperationFailed    ErrorCode = "OPERATION_FAILED"
	InvalidState       ErrorCode = "INVALID_STATE"
)

// AppError represents a structured application error
type AppError struct {
	Code      ErrorCode `json:"code"`
	Message   string    `json:"message"`
	Details   string    `json:"details,omitempty"`
	Timestamp time.Time `json:"timestamp"`
	TraceID   string    `json:"trace_id"`
	Cause     error     `json:"-"` // Original error, not serialized
}

// Error implements the error interface
func (e *AppError) Error() string {
	if e.Details != "" {
		return fmt.Sprintf("[%s] %s: %s", e.Code, e.Message, e.Details)
	}
	return fmt.Sprintf("[%s] %s", e.Code, e.Message)
}

// Unwrap returns the underlying error
func (e *AppError) Unwrap() error {
	return e.Cause
}

// NewAppError creates a new AppError with the given code and message
func NewAppError(code ErrorCode, message string) *AppError {
	return &AppError{
		Code:      code,
		Message:   message,
		Timestamp: time.Now(),
		TraceID:   uuid.New().String(),
	}
}

// NewAppErrorWithDetails creates a new AppError with details
func NewAppErrorWithDetails(code ErrorCode, message, details string) *AppError {
	return &AppError{
		Code:      code,
		Message:   message,
		Details:   details,
		Timestamp: time.Now(),
		TraceID:   uuid.New().String(),
	}
}

// NewAppErrorWithCause creates a new AppError wrapping an existing error
func NewAppErrorWithCause(code ErrorCode, message string, cause error) *AppError {
	details := ""
	if cause != nil {
		details = cause.Error()
	}
	
	return &AppError{
		Code:      code,
		Message:   message,
		Details:   details,
		Timestamp: time.Now(),
		TraceID:   uuid.New().String(),
		Cause:     cause,
	}
}

// WrapError wraps an existing error with additional context
func WrapError(err error, code ErrorCode, message string) *AppError {
	if err == nil {
		return nil
	}

	// If it's already an AppError, preserve the original trace ID
	if appErr, ok := err.(*AppError); ok {
		return &AppError{
			Code:      code,
			Message:   message,
			Details:   appErr.Error(),
			Timestamp: time.Now(),
			TraceID:   appErr.TraceID, // Preserve original trace ID
			Cause:     appErr,
		}
	}

	return NewAppErrorWithCause(code, message, err)
}

// IsErrorCode checks if an error has a specific error code
func IsErrorCode(err error, code ErrorCode) bool {
	if appErr, ok := err.(*AppError); ok {
		return appErr.Code == code
	}
	return false
}

// GetErrorCode returns the error code from an error, or empty string if not an AppError
func GetErrorCode(err error) ErrorCode {
	if appErr, ok := err.(*AppError); ok {
		return appErr.Code
	}
	return ""
}

// GetTraceID returns the trace ID from an error, or empty string if not an AppError
func GetTraceID(err error) string {
	if appErr, ok := err.(*AppError); ok {
		return appErr.TraceID
	}
	return ""
}
