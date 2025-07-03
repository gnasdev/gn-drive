package errors

import (
	"errors"
	"testing"
	"time"
)

func TestNewAppError(t *testing.T) {
	code := ValidationError
	message := "Test validation error"
	
	appErr := NewAppError(code, message)
	
	if appErr.Code != code {
		t.Errorf("Expected code %s, got %s", code, appErr.Code)
	}
	
	if appErr.Message != message {
		t.Errorf("Expected message %s, got %s", message, appErr.Message)
	}
	
	if appErr.TraceID == "" {
		t.Error("Expected non-empty trace ID")
	}
	
	if appErr.Timestamp.IsZero() {
		t.Error("Expected non-zero timestamp")
	}
}

func TestNewAppErrorWithDetails(t *testing.T) {
	code := ValidationError
	message := "Test error"
	details := "Detailed error information"
	
	appErr := NewAppErrorWithDetails(code, message, details)
	
	if appErr.Details != details {
		t.Errorf("Expected details %s, got %s", details, appErr.Details)
	}
}

func TestNewAppErrorWithCause(t *testing.T) {
	code := InternalError
	message := "Wrapped error"
	cause := errors.New("original error")
	
	appErr := NewAppErrorWithCause(code, message, cause)
	
	if appErr.Cause != cause {
		t.Errorf("Expected cause %v, got %v", cause, appErr.Cause)
	}
	
	if appErr.Details != cause.Error() {
		t.Errorf("Expected details to be cause error: %s, got %s", cause.Error(), appErr.Details)
	}
}

func TestWrapError(t *testing.T) {
	originalErr := errors.New("original error")
	code := NetworkError
	message := "Network operation failed"
	
	wrappedErr := WrapError(originalErr, code, message)
	
	if wrappedErr.Code != code {
		t.Errorf("Expected code %s, got %s", code, wrappedErr.Code)
	}
	
	if wrappedErr.Message != message {
		t.Errorf("Expected message %s, got %s", message, wrappedErr.Message)
	}
	
	if wrappedErr.Cause != originalErr {
		t.Errorf("Expected cause %v, got %v", originalErr, wrappedErr.Cause)
	}
}

func TestWrapErrorWithAppError(t *testing.T) {
	originalAppErr := NewAppError(ValidationError, "Original error")
	originalTraceID := originalAppErr.TraceID
	
	// Wait a bit to ensure different timestamp
	time.Sleep(1 * time.Millisecond)
	
	wrappedErr := WrapError(originalAppErr, InternalError, "Wrapped error")
	
	// Should preserve original trace ID
	if wrappedErr.TraceID != originalTraceID {
		t.Errorf("Expected trace ID %s, got %s", originalTraceID, wrappedErr.TraceID)
	}
	
	if wrappedErr.Code != InternalError {
		t.Errorf("Expected code %s, got %s", InternalError, wrappedErr.Code)
	}
}

func TestIsErrorCode(t *testing.T) {
	appErr := NewAppError(ValidationError, "Test error")
	
	if !IsErrorCode(appErr, ValidationError) {
		t.Error("Expected IsErrorCode to return true for matching code")
	}
	
	if IsErrorCode(appErr, NetworkError) {
		t.Error("Expected IsErrorCode to return false for non-matching code")
	}
	
	regularErr := errors.New("regular error")
	if IsErrorCode(regularErr, ValidationError) {
		t.Error("Expected IsErrorCode to return false for non-AppError")
	}
}

func TestGetErrorCode(t *testing.T) {
	appErr := NewAppError(ValidationError, "Test error")
	
	code := GetErrorCode(appErr)
	if code != ValidationError {
		t.Errorf("Expected code %s, got %s", ValidationError, code)
	}
	
	regularErr := errors.New("regular error")
	code = GetErrorCode(regularErr)
	if code != "" {
		t.Errorf("Expected empty code for regular error, got %s", code)
	}
}

func TestGetTraceID(t *testing.T) {
	appErr := NewAppError(ValidationError, "Test error")
	
	traceID := GetTraceID(appErr)
	if traceID != appErr.TraceID {
		t.Errorf("Expected trace ID %s, got %s", appErr.TraceID, traceID)
	}
	
	regularErr := errors.New("regular error")
	traceID = GetTraceID(regularErr)
	if traceID != "" {
		t.Errorf("Expected empty trace ID for regular error, got %s", traceID)
	}
}

func TestAppErrorError(t *testing.T) {
	// Test error without details
	appErr := NewAppError(ValidationError, "Test message")
	expected := "[VALIDATION_ERROR] Test message"
	if appErr.Error() != expected {
		t.Errorf("Expected error string %s, got %s", expected, appErr.Error())
	}
	
	// Test error with details
	appErrWithDetails := NewAppErrorWithDetails(ValidationError, "Test message", "Additional details")
	expectedWithDetails := "[VALIDATION_ERROR] Test message: Additional details"
	if appErrWithDetails.Error() != expectedWithDetails {
		t.Errorf("Expected error string %s, got %s", expectedWithDetails, appErrWithDetails.Error())
	}
}

func TestAppErrorUnwrap(t *testing.T) {
	originalErr := errors.New("original error")
	appErr := NewAppErrorWithCause(InternalError, "Wrapped error", originalErr)
	
	unwrapped := appErr.Unwrap()
	if unwrapped != originalErr {
		t.Errorf("Expected unwrapped error %v, got %v", originalErr, unwrapped)
	}
	
	// Test error without cause
	appErrNoCause := NewAppError(ValidationError, "No cause")
	unwrappedNoCause := appErrNoCause.Unwrap()
	if unwrappedNoCause != nil {
		t.Errorf("Expected nil unwrapped error, got %v", unwrappedNoCause)
	}
}

func TestErrorHandler(t *testing.T) {
	handler := NewErrorHandler(false) // debug mode off
	
	// Test handling regular error
	regularErr := errors.New("test error")
	appErr := handler.Handle(regularErr, "test_context")
	
	if appErr == nil {
		t.Fatal("Expected non-nil AppError")
	}
	
	if appErr.Code != InternalError {
		t.Errorf("Expected code %s, got %s", InternalError, appErr.Code)
	}
	
	// Test handling AppError (should return as-is)
	existingAppErr := NewAppError(ValidationError, "Existing error")
	handledAppErr := handler.Handle(existingAppErr, "test_context")
	
	if handledAppErr != existingAppErr {
		t.Error("Expected handler to return existing AppError as-is")
	}
	
	// Test handling nil error
	nilResult := handler.Handle(nil, "test_context")
	if nilResult != nil {
		t.Error("Expected nil result for nil error")
	}
}

func TestErrorHandlerSpecificMethods(t *testing.T) {
	handler := NewErrorHandler(false)
	
	// Test validation error
	validationErr := handler.HandleValidation("email", "Invalid email format")
	if validationErr.Code != ValidationError {
		t.Errorf("Expected validation error code, got %s", validationErr.Code)
	}
	
	// Test not found error
	notFoundErr := handler.HandleNotFound("user", "123")
	if notFoundErr.Code != NotFoundError {
		t.Errorf("Expected not found error code, got %s", notFoundErr.Code)
	}
	
	// Test rclone error
	rcloneErr := handler.HandleRcloneError(errors.New("rclone failed"), "sync")
	if rcloneErr.Code != RcloneError {
		t.Errorf("Expected rclone error code, got %s", rcloneErr.Code)
	}
}
