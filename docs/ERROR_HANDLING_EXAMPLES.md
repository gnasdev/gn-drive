# Error Handling Examples

This document provides practical examples of how to use the error handling system in NS-Drive.

## Backend Examples

### 1. Basic Error Handling in Commands

```go
func (a *App) SyncOperation(profile models.Profile) error {
    // Use error middleware to wrap the operation
    return a.errorHandler.WrapSync(context.Background(), "sync", func() error {
        // Your sync logic here
        err := rclone.Sync(ctx, config, "pull", profile, outLog)
        return err
    })
}
```

### 2. Validation with Custom Error Messages

```go
func (a *App) ValidateProfile(profile models.Profile) error {
    // Check required fields
    if profile.Name == "" {
        return a.errorHandler.HandleValidation("name", "Profile name is required")
    }
    
    if profile.RemotePath == "" {
        return a.errorHandler.HandleValidation("remote_path", "Remote path cannot be empty")
    }
    
    if profile.LocalPath == "" {
        return a.errorHandler.HandleValidation("local_path", "Local path cannot be empty")
    }
    
    return nil
}
```

### 3. File Operations with Error Handling

```go
func (a *App) SaveConfiguration(config models.ConfigInfo) error {
    configPath := config.EnvConfig.ProfileFilePath
    
    return a.errorHandler.WrapFileOperation(context.Background(), configPath, func() error {
        data, err := json.Marshal(config)
        if err != nil {
            return err
        }
        
        return os.WriteFile(configPath, data, 0644)
    })
}
```

### 4. Remote Operations with Context

```go
func (a *App) TestRemoteConnection(remoteName string) error {
    return a.errorHandler.WrapRemoteOperation(
        context.Background(), 
        remoteName, 
        "test_connection", 
        func() error {
            // Test remote connection logic
            return rclone.TestRemote(remoteName)
        },
    )
}
```

### 5. Custom Error Creation

```go
func (a *App) ProcessFile(filePath string) error {
    if !fileExists(filePath) {
        return a.errorHandler.HandleNotFound("file", filePath)
    }
    
    fileInfo, err := os.Stat(filePath)
    if err != nil {
        return a.errorHandler.HandleWithCode(
            err, 
            errors.FileSystemError, 
            "Failed to get file information",
            "file_stat",
            filePath,
        )
    }
    
    if fileInfo.Size() > maxFileSize {
        return errors.NewAppErrorWithDetails(
            errors.ValidationError,
            "File too large",
            fmt.Sprintf("File size %d exceeds maximum %d", fileInfo.Size(), maxFileSize),
        )
    }
    
    return nil
}
```

## Frontend Examples

### 1. Service-Level Error Handling

```typescript
// data.service.ts
@Injectable()
export class DataService {
  constructor(
    private http: HttpClient,
    private errorService: ErrorService
  ) {}

  async loadProfiles(): Promise<Profile[]> {
    try {
      const profiles = await this.http.get<Profile[]>('/api/profiles').toPromise();
      return profiles || [];
    } catch (error) {
      this.errorService.handleApiError(error, 'load_profiles');
      throw error; // Re-throw to let component handle if needed
    }
  }

  async saveProfile(profile: Profile): Promise<void> {
    // Validate before sending
    if (!profile.name?.trim()) {
      this.errorService.handleValidationError('name', 'Profile name is required');
      throw new Error('Validation failed');
    }

    try {
      await this.http.post('/api/profiles', profile).toPromise();
      this.errorService.showSuccess('Profile saved successfully!');
    } catch (error) {
      this.errorService.handleApiError(error, 'save_profile');
      throw error;
    }
  }
}
```

### 2. Component Error Handling

```typescript
// profile.component.ts
@Component({...})
export class ProfileComponent {
  constructor(
    private dataService: DataService,
    private errorService: ErrorService
  ) {}

  async onSave(): Promise<void> {
    try {
      // Validate form
      if (!this.validateForm()) {
        return;
      }

      // Show loading state
      this.isLoading = true;

      // Save data
      await this.dataService.saveProfile(this.profile);
      
      // Navigate away or update UI
      this.router.navigate(['/profiles']);
      
    } catch (error) {
      // Error is already handled by service, just log
      console.error('Save operation failed:', error);
    } finally {
      this.isLoading = false;
    }
  }

  private validateForm(): boolean {
    let isValid = true;

    if (!this.profile.name?.trim()) {
      this.errorService.handleValidationError('name', 'Profile name is required');
      isValid = false;
    }

    if (!this.profile.remotePath?.trim()) {
      this.errorService.handleValidationError('remotePath', 'Remote path is required');
      isValid = false;
    }

    if (!this.profile.localPath?.trim()) {
      this.errorService.handleValidationError('localPath', 'Local path is required');
      isValid = false;
    }

    return isValid;
  }

  async onDelete(profileId: string): Promise<void> {
    try {
      const confirmed = await this.confirmDialog.open({
        title: 'Delete Profile',
        message: 'Are you sure you want to delete this profile?'
      }).afterClosed().toPromise();

      if (!confirmed) return;

      await this.dataService.deleteProfile(profileId);
      this.errorService.showSuccess('Profile deleted successfully!');
      
      // Refresh list
      await this.loadProfiles();
      
    } catch (error) {
      console.error('Delete operation failed:', error);
    }
  }
}
```

### 3. Form Validation with Error Display

```typescript
// form-validator.service.ts
@Injectable()
export class FormValidatorService {
  constructor(private errorService: ErrorService) {}

  validateProfileForm(profile: Profile): boolean {
    const errors: string[] = [];

    if (!profile.name?.trim()) {
      errors.push('Profile name is required');
    } else if (profile.name.length > 50) {
      errors.push('Profile name must be less than 50 characters');
    }

    if (!profile.remotePath?.trim()) {
      errors.push('Remote path is required');
    } else if (!this.isValidRemotePath(profile.remotePath)) {
      errors.push('Remote path format is invalid');
    }

    if (!profile.localPath?.trim()) {
      errors.push('Local path is required');
    } else if (!this.isValidLocalPath(profile.localPath)) {
      errors.push('Local path does not exist');
    }

    if (errors.length > 0) {
      errors.forEach(error => {
        this.errorService.handleValidationError('form', error);
      });
      return false;
    }

    return true;
  }

  private isValidRemotePath(path: string): boolean {
    // Implement remote path validation logic
    return path.includes(':') && path.length > 3;
  }

  private isValidLocalPath(path: string): boolean {
    // Implement local path validation logic
    return path.startsWith('/') || path.match(/^[A-Z]:\\/);
  }
}
```

### 4. Custom Error Handling for Specific Operations

```typescript
// sync.service.ts
@Injectable()
export class SyncService {
  constructor(
    private errorService: ErrorService,
    private appService: AppService
  ) {}

  async startSync(profileId: string, operation: 'pull' | 'push' | 'bi'): Promise<void> {
    try {
      // Validate profile exists
      const profile = await this.getProfile(profileId);
      if (!profile) {
        this.errorService.handleApiError({
          error: {
            code: 'NOT_FOUND_ERROR',
            message: 'Profile not found',
            details: `No profile found with ID: ${profileId}`,
            timestamp: new Date().toISOString(),
            trace_id: this.generateTraceId()
          }
        }, 'start_sync');
        return;
      }

      // Check if sync is already running
      if (this.isSyncRunning(profileId)) {
        this.errorService.showWarning('Sync operation is already running for this profile');
        return;
      }

      // Start sync operation
      const syncId = await this.appService.sync(operation, profile);
      this.errorService.showInfo(`${operation.toUpperCase()} sync started for ${profile.name}`);
      
      // Monitor sync progress
      this.monitorSync(syncId, profile.name);
      
    } catch (error) {
      this.errorService.handleApiError(error, 'start_sync');
    }
  }

  private async monitorSync(syncId: number, profileName: string): Promise<void> {
    // Implementation for monitoring sync progress
    // Show progress notifications, handle completion/errors
  }

  private generateTraceId(): string {
    return `client_${Date.now()}_${Math.random().toString(36).substr(2, 9)}`;
  }
}
```

### 5. Error Recovery and Retry Logic

```typescript
// retry.service.ts
@Injectable()
export class RetryService {
  constructor(private errorService: ErrorService) {}

  async withRetry<T>(
    operation: () => Promise<T>,
    maxRetries: number = 3,
    delay: number = 1000,
    context: string = 'operation'
  ): Promise<T> {
    let lastError: any;

    for (let attempt = 1; attempt <= maxRetries; attempt++) {
      try {
        return await operation();
      } catch (error) {
        lastError = error;
        
        if (attempt === maxRetries) {
          // Final attempt failed
          this.errorService.handleApiError(error, `${context}_final_attempt`);
          throw error;
        }

        // Show retry warning
        this.errorService.showWarning(
          `Operation failed (attempt ${attempt}/${maxRetries}). Retrying in ${delay}ms...`
        );

        // Wait before retry
        await this.delay(delay);
        
        // Exponential backoff
        delay *= 2;
      }
    }

    throw lastError;
  }

  private delay(ms: number): Promise<void> {
    return new Promise(resolve => setTimeout(resolve, ms));
  }
}
```

## Testing Examples

### Backend Testing

```go
func TestErrorHandlingInService(t *testing.T) {
    // Setup
    app := &App{
        errorHandler: errors.NewMiddleware(false),
    }

    // Test validation error
    profile := models.Profile{Name: ""} // Invalid profile
    err := app.ValidateProfile(profile)
    
    assert.NotNil(t, err)
    assert.True(t, errors.IsErrorCode(err, errors.ValidationError))

    // Test successful operation
    validProfile := models.Profile{
        Name: "Test Profile",
        RemotePath: "remote:path",
        LocalPath: "/local/path",
    }
    err = app.ValidateProfile(validProfile)
    assert.Nil(t, err)
}
```

### Frontend Testing

```typescript
describe('ErrorService', () => {
  let service: ErrorService;
  let snackBar: jasmine.SpyObj<MatSnackBar>;

  beforeEach(() => {
    const snackBarSpy = jasmine.createSpyObj('MatSnackBar', ['open']);
    
    TestBed.configureTestingModule({
      providers: [
        ErrorService,
        { provide: MatSnackBar, useValue: snackBarSpy }
      ]
    });
    
    service = TestBed.inject(ErrorService);
    snackBar = TestBed.inject(MatSnackBar) as jasmine.SpyObj<MatSnackBar>;
  });

  it('should handle API errors', () => {
    const error = {
      error: {
        code: 'VALIDATION_ERROR',
        message: 'Test error message',
        timestamp: new Date().toISOString(),
        trace_id: 'test-trace-id'
      }
    };

    service.handleApiError(error, 'test_context');

    expect(snackBar.open).toHaveBeenCalledWith(
      'Test error message',
      'Close',
      jasmine.objectContaining({
        panelClass: ['warning-snackbar']
      })
    );
  });

  it('should show success messages', () => {
    service.showSuccess('Operation completed');

    expect(snackBar.open).toHaveBeenCalledWith(
      'Operation completed',
      'Close',
      jasmine.objectContaining({
        duration: 3000,
        panelClass: ['success-snackbar']
      })
    );
  });
});
```

## Best Practices Summary

1. **Always provide context** when handling errors
2. **Use appropriate error codes** for different error types
3. **Show user-friendly messages** while logging technical details
4. **Implement retry logic** for transient failures
5. **Validate input** at both frontend and backend
6. **Use trace IDs** for error correlation
7. **Test error scenarios** thoroughly
8. **Document error handling** for team members
