# Contributing to NG Drive

Thank you for your interest in contributing to NG Drive! This document provides guidelines and best practices for contributing.

## Getting Started

1. Fork the repository
2. Clone your fork: `git clone <your-fork-url>`
3. Set up development environment (see [DEVELOPMENT.md](DEVELOPMENT.md))
4. Create a feature branch: `git checkout -b feature/my-feature`

## Code Style

### Go (Backend)

- Follow standard Go conventions
- Run `task lint:be` before committing
- Use `gofmt` for formatting
- Handle errors explicitly (no ignored errors)
- Use context for cancellation
- Add comments for exported functions

Example:
```go
// ValidateProfile validates profile fields and returns an error if invalid.
func (v *ProfileValidator) ValidateProfile(profile models.Profile) error {
    if profile.Name == "" {
        return &ValidationError{Field: "name", Message: "cannot be empty"}
    }
    return nil
}
```

### TypeScript (Frontend)

- Run `task lint:fe` before committing
- Use TypeScript strict mode
- Prefer RxJS observables for async state
- Document public interfaces
- Use meaningful variable names

Example:
```typescript
// Handle sync events from backend
private handleSyncEvent(event: SyncEvent): void {
    if (event.tabId) {
        this.tabService.handleTypedSyncEvent(event);
        return;
    }
    // Handle global sync events
    // ...
}
```

## Commit Messages

Use clear, descriptive commit messages:

```
feat: Add profile validation for path traversal prevention
fix: Correct mutex deadlock in ConfigService
docs: Update architecture documentation
refactor: Unify event system through EventBus
test: Add tests for profile validation
```

Prefixes:
- `feat:` - New feature
- `fix:` - Bug fix
- `docs:` - Documentation
- `refactor:` - Code refactoring
- `test:` - Tests
- `chore:` - Build/tooling changes

## Pull Request Process

1. **Create a feature branch**
   ```bash
   git checkout -b feature/my-feature
   ```

2. **Make your changes**
   - Follow code style guidelines
   - Add tests for new functionality
   - Update documentation if needed

3. **Run checks**
   ```bash
   # Lint both frontend and backend
   task lint

   # Run Go tests
   cd desktop && go test ./...

   # Build to verify
   task build
   ```

4. **Commit your changes**
   ```bash
   git add .
   git commit -m "feat: Description of your changes"
   ```

5. **Push and create PR**
   ```bash
   git push origin feature/my-feature
   ```

6. **Fill out PR template**
   - Describe what the PR does
   - Link related issues
   - List any breaking changes

## Testing

### Go Tests

```bash
cd desktop
go test ./...

# With coverage
go test -cover ./...

# Specific package
go test ./backend/validation/...
```

### Frontend Tests

```bash
cd desktop/frontend
npm test
```

### Manual Testing

Before submitting:
1. Test the full workflow (add profile, run sync, delete)
2. Test error cases (invalid input, network errors)
3. Test on multiple platforms if possible

## Architecture Guidelines

### Adding Backend Services

1. Create service in `desktop/backend/services/`
2. Implement required interfaces:
   ```go
   type MyService struct {
       app      *application.App
       eventBus *events.WailsEventBus
       // ... other fields
   }

   func (s *MyService) SetApp(app *application.App) {
       s.app = app
       s.eventBus = events.NewEventBus(app)
   }
   ```
3. Register in `desktop/main.go`
4. Use EventBus for frontend communication

### Adding Frontend Features

1. Follow Angular component patterns
2. Use RxJS for state management
3. Handle events from backend properly:
   ```typescript
   // Add type guard
   export function isMyEvent(event: unknown): event is MyEvent {
       // ...
   }

   // Handle in AppService
   if (isMyEvent(parsedEvent)) {
       this.handleMyEvent(parsedEvent);
   }
   ```

### Adding Event Types

1. Define in `desktop/backend/events/types.go`
2. Add TypeScript type in `desktop/frontend/src/app/models/events.ts`
3. Add type guard function
4. Handle in appropriate service

## Security Considerations

- Never store credentials in code
- Validate all user input
- Use secure file permissions (0600)
- Prevent path traversal attacks
- Handle sensitive data carefully

## Questions?

- Check existing issues and documentation
- Open a GitHub issue for questions
- Be patient and respectful

Thank you for contributing!
