# NS-Drive Project Rules & Architecture

## Project Overview

NS-Drive is a desktop application built with **Wails v2** that provides a GUI for **rclone** file synchronization operations. It allows users to manage cloud storage remotes and sync profiles through a modern Angular Material interface.

### Core Technologies

- **Backend**: Go 1.23.4 with Wails v2 framework
- **Frontend**: Angular 19.2.3 with Angular Material 19.2.19
- **Package Manager**: Yarn 4.9.2
- **Build Tool**: Taskfile (task runner)
- **Cloud Sync**: rclone integration

## Architecture Overview

### Backend Structure (`desktop/backend/`)

- **`app.go`**: Main application struct, startup logic, event channel management
- **`commands.go`**: Core sync operations (pull, push, bi-sync), remote management
- **`models/`**: Data structures (Profile, ConfigInfo, etc.)
- **`dto/`**: Data Transfer Objects for frontend communication
- **`rclone/`**: rclone integration and configuration
- **`config/`**: Environment configuration management
- **`utils/`**: Utility functions and error handling

### Frontend Structure (`desktop/frontend/src/app/`)

- **`app.component.ts/html`**: Main shell with responsive sidebar navigation
- **`app.service.ts`**: Central service for backend communication and state management
- **`tab.service.ts`**: Tab management for multi-threaded operations
- **`home/`**: Main operation interface with vertical tab system
- **`profiles/`**: Profile management with form validation
- **`remotes/`**: Cloud storage remote configuration

## Key Features & Functionality

### 1. Multi-Tab Operation System

- **Vertical tab layout** on home page
- Each tab runs **independent sync operations** in separate threads
- Tab creation/deletion with automatic cleanup
- Tab renaming functionality
- **Only active tab content is visible** (inactive tabs are hidden)

### 2. Sync Operations

- **Pull**: Remote → Local synchronization
- **Push**: Local → Remote synchronization
- **Bi-Sync**: Bidirectional synchronization
- **Bi-Resync**: Bidirectional with force resync
- **Real-time output** streaming to frontend
- **Command stopping** capability per tab

### 3. Profile Management

- **Selector + Input combinations** for paths (remote type + path)
- **Dropdown selectors** for parallel (1-32) and bandwidth (1-100)
- **Include/exclude paths** with folder/file selectors
- **Auto-append '/**'\*\* for folder selections
- **Form validation** and error handling

### 4. Remote Management

- Support for multiple cloud providers (Google Drive, Dropbox, OneDrive, Yandex Disk, Google Photos, iCloud Drive, etc.)
- **OAuth integration** for authentication
- **Add/Delete remotes** with confirmation dialogs
- **Real-time remote list** updates

## UI/UX Design Principles

### Material Design 3 Implementation

- **White/Gray/Black color theme** throughout interface
- **Gray background** for entire application
- **Simple input fields** without unnecessary prefixes
- **Dark mode compatibility** with light colored text
- **Transparent borders** for mat-mdc-select and mat-mdc-input components

### Responsive Design

- **Mobile-first approach** with Android appearance
- **Breakpoint observer** for handset detection
- **Collapsible sidebar** on small screens
- **Functional action buttons** in all screen sizes

### Component Styling Guidelines

- **Avoid custom CSS** - use component attributes for styling
- **Consistent Material icons** throughout interface
- **Outline appearance** for form fields
- **Raised buttons** for primary actions
- **Icon buttons** for secondary actions

## Data Flow & Communication

### Backend ↔ Frontend Communication

- **Wails binding** for Go function exposure
- **Event system** (`EventsOn`) for real-time updates
- **JSON serialization** for data transfer
- **Channel-based** output streaming

### State Management

- **BehaviorSubject** for reactive state management
- **Central AppService** for global state
- **TabService** for tab-specific state
- **Automatic change detection** with OnPush strategy

### Event Handling

- **Command lifecycle events**: started, output, stopped, error
- **Tab-specific events** with tab_id routing
- **Error propagation** with user-friendly messages
- **Real-time UI updates** based on backend events

## File Structure & Conventions

### Configuration Files

- **`wails.json`**: Wails project configuration
- **`package.json`**: Frontend dependencies and scripts
- **`Taskfile.yml`**: Build and development tasks
- **`.env`**: Environment configuration (embedded in binary)

### Code Organization

- **Modular backend packages** with clear separation of concerns
- **Angular standalone components** with explicit imports
- **Type-safe interfaces** between frontend and backend
- **Consistent naming conventions** (camelCase frontend, snake_case backend)

## Development Workflow

### Build Commands

- **Development**: `task dev` (runs Wails dev server)
- **Mac Build**: `task build-mac` (creates .app bundle)
- **Windows Build**: `task build-win` (creates .exe)

### Package Management

- **Always use yarn** for frontend dependencies
- **Go modules** for backend dependencies
- **Wails CLI** for framework operations

### Testing Strategy

- **Unit tests** for business logic
- **Integration tests** for rclone operations
- **E2E tests** for critical user workflows
- **Manual testing** for UI/UX validation

## Security & Error Handling

### Error Management

- **Centralized error handling** in utils package
- **User-friendly error messages** in frontend
- **Graceful degradation** for failed operations
- **Proper cleanup** on application exit

### Data Validation

- **Input validation** on both frontend and backend
- **Type safety** with TypeScript and Go structs
- **Path validation** for file system operations
- **Remote configuration validation**

## Performance Considerations

### Optimization Strategies

- **OnPush change detection** for Angular components
- **Lazy loading** for large data sets
- **Efficient event handling** with proper cleanup
- **Memory management** for long-running operations

### Resource Management

- **Context cancellation** for Go routines
- **Proper subscription cleanup** in Angular
- **File handle management** for sync operations
- **Background thread management** for tabs

## Development Guidelines

### Code Quality Standards

- **Type safety first**: Use TypeScript strict mode and Go's type system
- **Error handling**: Always handle errors gracefully with user feedback
- **Documentation**: Comment complex business logic and API interfaces
- **Testing**: Write tests for new features and bug fixes
- **Performance**: Profile and optimize resource-intensive operations

### Git Workflow

- **Feature branches** for new development
- **Descriptive commit messages** following conventional commits
- **Code review** for all changes
- **Automated testing** before merge
- **Version tagging** for releases

### Debugging & Troubleshooting

- **Debug mode** available via environment configuration
- **Console logging** for development (removed in production)
- **Error tracking** with stack traces
- **Performance profiling** for sync operations
- **Network debugging** for remote operations

## Common Patterns & Best Practices

### Frontend Patterns

- **Reactive programming** with RxJS observables
- **Immutable state updates** for predictable behavior
- **Component composition** over inheritance
- **Service injection** for shared functionality
- **Type guards** for runtime type checking

### Backend Patterns

- **Context propagation** for cancellation and timeouts
- **Channel communication** for concurrent operations
- **Interface segregation** for testable code
- **Dependency injection** through struct composition
- **Error wrapping** with contextual information

### Integration Patterns

- **Event-driven architecture** for real-time updates
- **Command pattern** for user actions
- **Observer pattern** for state changes
- **Factory pattern** for object creation
- **Strategy pattern** for different sync algorithms

## Troubleshooting Guide

### Common Issues

1. **Build failures**: Check Go/Node versions and dependencies
2. **Sync errors**: Verify remote configuration and permissions
3. **UI freezing**: Check for blocking operations on main thread
4. **Memory leaks**: Ensure proper cleanup of subscriptions and channels
5. **Performance issues**: Profile and optimize critical paths

### Debug Commands

- `wails dev -debug`: Enable debug mode
- `go test ./...`: Run backend tests
- `yarn test`: Run frontend tests
- `yarn lint`: Check code quality
- `task build-mac -v 2`: Verbose build output

## Future Extensibility

### Planned Features

- **Additional cloud providers** support
- **Advanced filtering options** for sync operations
- **Scheduling capabilities** for automated syncs
- **Backup and restore** functionality
- **Performance monitoring** and analytics

### Architecture Considerations

- **Plugin system** for custom sync providers
- **Configuration migration** for version updates
- **API versioning** for backend compatibility
- **Theme customization** system
- **Internationalization** support
