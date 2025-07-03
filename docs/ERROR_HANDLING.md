# Error Handling System Documentation

## Overview

NS-Drive implements a comprehensive error handling system across both backend (Go) and frontend (Angular) to provide consistent, user-friendly error management and debugging capabilities.

## Architecture

### Backend Error Handling (Go)

#### Error Types (`desktop/backend/errors/types.go`)

The system defines standardized error codes for different categories:

- **Validation Errors**: `VALIDATION_ERROR`, `INVALID_INPUT`, `MISSING_FIELD`
- **Authentication/Authorization**: `AUTHENTICATION_ERROR`, `AUTHORIZATION_ERROR`
- **Resource Errors**: `NOT_FOUND_ERROR`, `CONFLICT_ERROR`, `ALREADY_EXISTS_ERROR`
- **System Errors**: `INTERNAL_ERROR`, `DATABASE_ERROR`, `FILESYSTEM_ERROR`
- **External Service Errors**: `RCLONE_ERROR`, `NETWORK_ERROR`, `TIMEOUT_ERROR`
- **Business Logic Errors**: `BUSINESS_LOGIC_ERROR`, `OPERATION_FAILED`

#### AppError Structure

```go
type AppError struct {
    Code      ErrorCode `json:"code"`
    Message   string    `json:"message"`
    Details   string    `json:"details,omitempty"`
    Timestamp time.Time `json:"timestamp"`
    TraceID   string    `json:"trace_id"`
    Cause     error     `json:"-"`
}
```

#### Error Handler (`desktop/backend/errors/handlers.go`)

Provides centralized error processing with:

- Automatic error categorization
- Structured logging with trace IDs
- Stack trace capture in debug mode
- Panic recovery

#### Middleware (`desktop/backend/errors/middleware.go`)

Offers wrapper functions for common operations:

- `WrapCommand()` - Command execution with error handling
- `WrapSync()` - Sync operations with rclone error handling
- `WrapRemoteOperation()` - Remote operations
- `WrapFileOperation()` - File system operations

### Frontend Error Handling (Angular)

#### Error Service (`desktop/frontend/src/app/services/error.service.ts`)

Central service for error management:

- API error handling with structured responses
- Validation error handling
- Network and timeout error handling
- Material UI snackbar notifications
- Error severity classification

#### Global Error Handler (`desktop/frontend/src/app/services/global-error-handler.service.ts`)

Catches unhandled errors globally:

- JavaScript runtime errors
- Chunk loading errors
- Network connectivity issues

#### HTTP Interceptor (`desktop/frontend/src/app/interceptors/error.interceptor.ts`)

Intercepts HTTP errors:

- Automatic retry for network/server errors
- Status code-specific error handling
- Exponential backoff for retries

#### Error Display Component (`desktop/frontend/src/app/components/error-display/error-display.component.ts`)

Visual error notifications:

- Floating error cards with severity indicators
- Expandable details sections
- Persistent errors display (non-auto-hide)
- Tailwind CSS styling with dark mode support

#### Toast Component (`desktop/frontend/src/app/components/toast/toast.component.ts`)

Temporary notifications:

- Auto-dismissing toast notifications
- Success, info, warning messages
- Tailwind CSS styling
- Mobile responsive design

## Usage Examples

### Backend Usage

#### Basic Error Handling

```go
// In your service method
func (a *App) SomeOperation() error {
    err := someRiskyOperation()
    if err != nil {
        return a.errorHandler.HandleWithCode(err, errors.OperationFailed, "operation_context")
    }
    return nil
}
```

#### Validation Errors

```go
func (a *App) ValidateInput(name string) error {
    if name == "" {
        return a.errorHandler.HandleValidation("name", "Name is required")
    }
    return nil
}
```

#### File Operations

```go
func (a *App) SaveFile(path string, data []byte) error {
    err := a.errorHandler.WrapFileOperation(ctx, path, func() error {
        return os.WriteFile(path, data, 0644)
    })
    return err
}
```

### Frontend Usage

#### Service Error Handling

```typescript
// In your service
async getData(): Promise<Data> {
  try {
    const data = await this.apiCall();
    return data;
  } catch (error) {
    this.errorService.handleApiError(error, 'get_data');
    throw error;
  }
}
```

#### Component Error Handling

```typescript
// In your component
async saveData(): Promise<void> {
  try {
    await this.dataService.save(this.data);
    this.errorService.showSuccess('Data saved successfully!');
  } catch (error) {
    // Error is already handled by the service
    console.error('Save failed:', error);
  }
}
```

#### Validation Errors

```typescript
validateForm(): boolean {
  if (!this.form.name) {
    this.errorService.handleValidationError('name', 'Name is required');
    return false;
  }
  return true;
}
```

## Error Response Format

### Backend API Response

```json
{
  "error": {
    "code": "VALIDATION_ERROR",
    "message": "User-friendly error message",
    "details": "Technical details for debugging",
    "timestamp": "2025-01-03T10:00:00Z",
    "trace_id": "uuid-trace-id"
  }
}
```

### Frontend Error Notification

```typescript
interface ErrorNotification {
  id: string;
  severity: ErrorSeverity;
  title: string;
  message: string;
  details?: string;
  timestamp: Date;
  dismissed: boolean;
  autoHide: boolean;
  duration?: number;
}
```

## Configuration

### Backend Configuration

```go
// Enable debug mode for detailed logging
app := &App{
    errorHandler: errors.NewMiddleware(true), // debug mode
}
```

### Frontend Configuration

Error handling is automatically configured in `app.config.ts`:

```typescript
export const appConfig: ApplicationConfig = {
  providers: [
    provideHttpClient(),
    {
      provide: HTTP_INTERCEPTORS,
      useClass: ErrorInterceptor,
      multi: true,
    },
    {
      provide: ErrorHandler,
      useClass: GlobalErrorHandler,
    },
  ],
};
```

Note: Angular Material is not required - the system uses pure Tailwind CSS for styling.

## Logging

### Backend Logging

Errors are logged to `ns-drive-errors.log` with:

- Timestamp and trace ID
- Error code and message
- Stack trace (in debug mode)
- Context information

### Frontend Logging

Errors are logged to browser console with:

- Error details and context
- API request/response information
- User action context

## Best Practices

### Backend

1. **Always use error codes**: Categorize errors with appropriate codes
2. **Provide context**: Include operation context in error handling
3. **Use trace IDs**: Enable error correlation across logs
4. **Handle panics**: Use recovery middleware for critical operations
5. **Log appropriately**: Use different log levels based on error severity

### Frontend

1. **Handle at service level**: Catch and process errors in services
2. **Show user-friendly messages**: Use ErrorService for user notifications
3. **Provide feedback**: Show success messages for completed operations
4. **Don't block UI**: Handle errors gracefully without breaking user flow
5. **Log for debugging**: Keep console logs for development debugging

## Testing

### Backend Testing

```go
func TestErrorHandling(t *testing.T) {
    handler := errors.NewErrorHandler(false)

    err := errors.New("test error")
    appErr := handler.Handle(err, "test_context")

    assert.NotNil(t, appErr)
    assert.Equal(t, errors.InternalError, appErr.Code)
}
```

### Frontend Testing

```typescript
describe("ErrorService", () => {
  it("should handle API errors", () => {
    const error = { error: { code: "TEST_ERROR", message: "Test message" } };
    service.handleApiError(error, "test_context");

    expect(snackBarSpy).toHaveBeenCalled();
  });
});
```

## Troubleshooting

### Common Issues

1. **Missing trace IDs**: Ensure error handler is properly initialized
2. **Snackbar not showing**: Check if Material modules are imported
3. **Errors not logged**: Verify log file permissions
4. **Interceptor not working**: Check HTTP_INTERCEPTORS configuration

### Debug Mode

Enable debug mode for detailed error information:

- Backend: Set debug flag in error handler
- Frontend: Check browser console for detailed logs
