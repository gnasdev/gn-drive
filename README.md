# NS-Drive

A modern desktop application for cloud storage synchronization powered by rclone. NS-Drive provides an intuitive GUI for managing cloud remotes and sync profiles with real-time operation monitoring.

## 🚀 Features

- **Multi-Cloud Support**: Connect to Google Drive, Dropbox, OneDrive, Yandex Disk, Google Photos, iCloud Drive, and more
- **Profile Management**: Create and manage sync profiles with custom configurations
- **Real-time Monitoring**: Live output streaming and progress tracking for sync operations
- **Multi-tab Operations**: Run multiple sync operations simultaneously in separate tabs
- **Dark Mode**: Modern dark/light theme with responsive design
- **Cross-platform**: Available for Windows, macOS, and Linux

## 🛠️ Technology Stack

- **Backend**: Go 1.25 with Wails v3 (alpha.57)
- **Frontend**: Angular 21.1 with Tailwind CSS
- **Cloud Sync**: rclone v1.73.0 integration
- **Package Manager**: npm
- **Build Tool**: Taskfile (task runner)

## 📋 Prerequisites

Before building or running NS-Drive, ensure you have the following installed:

- **Go**: v1.25 or later
- **Node.js**: v18 or later (v24+ recommended)
- **npm**: Package manager (comes with Node.js)
- **Taskfile**: Task runner for build automation
- **Wails v3**: Desktop app framework

### Installing Prerequisites

```bash
# Install Go (if not already installed)
# Visit: https://golang.org/dl/

# Install Node.js
# Visit: https://nodejs.org/

# Install Taskfile
# Visit: https://taskfile.dev/installation/

# Install Wails v3
go install github.com/wailsapp/wails/v3/cmd/wails3@latest
```

## 🏗️ Building the Application

### Development Mode

Development requires running two separate processes: the Angular frontend dev server and the Wails backend.

**Terminal 1 - Start Frontend Dev Server:**

```bash
task dev:fe
```

Wait until you see:

```
✔ Building...
Application bundle generation complete.
  ➜  Local:   http://localhost:9245/
```

**Terminal 2 - Start Wails Backend:**

```bash
task dev:be
```

The application window will open automatically once the backend is ready. You should see logs like:

```
INFO Connected to frontend dev server!
NOTICE: SyncService starting up...
NOTICE: ConfigService starting up...
NOTICE: RemoteService starting up...
NOTICE: TabService starting up...
```

**Hot Reload:**
- Frontend changes: Automatically reloaded by Angular dev server
- Backend changes: Wails automatically rebuilds and restarts the Go binary

### Production Build

#### Quick Build (Binary Only)

```bash
task build
# Creates: ns-drive binary in project root
```

#### macOS App Bundle (Recommended)

Use task or the build script to create a signed `.app` bundle:

```bash
# Using task (recommended)
task build:macos

# With custom version
VERSION=1.2.0 task build:macos

# With Apple Developer signing identity
SIGNING_IDENTITY="Developer ID Application: Your Name" task build:macos

# Or using the shell script directly
./scripts/build-macos.sh
```

This creates:
- `ns-drive.app` - Signed macOS application bundle
- Ready to run or distribute

**What the script does:**
1. Checks prerequisites (Go, Node.js, wails3)
2. Generates TypeScript bindings
3. Builds frontend (Angular production build)
4. Builds backend (Go binary with optimizations)
5. Creates `.app` bundle with proper structure
6. Generates app icon (icns format)
7. Signs the app (ad-hoc or with provided identity)

**Running the built app:**

```bash
# Run directly
open ns-drive.app

# Install to Applications
cp -R ns-drive.app /Applications/
```

### Manual Development (Alternative)

If `task` commands don't work, you can run manually:

```bash
# Terminal 1: Frontend
cd desktop/frontend
npm install --legacy-peer-deps
npm start -- --port 9245

# Terminal 2: Backend (after frontend is ready)
cd desktop
go mod tidy
wails3 dev -config ./build/config.yml -port 9245
```

## 🚀 Quick Start

1. **Clone the repository**

   ```bash
   git clone <repository-url>
   cd ns-drive
   ```

2. **Install dependencies**

   ```bash
   # Install Go dependencies
   cd desktop && go mod tidy

   # Install frontend dependencies
   cd frontend && npm install --legacy-peer-deps
   cd ../..
   ```

3. **Run in development mode** (requires 2 terminals)

   ```bash
   # Terminal 1: Start frontend dev server
   task dev:fe
   # Wait for "Local: http://localhost:9245/" message

   # Terminal 2: Start Wails backend (after frontend is ready)
   task dev:be
   # App window will open automatically
   ```

4. **Build for production**

   ```bash
   task build
   ```

5. **Run the built application**

   ```bash
   # macOS/Linux
   ./ns-drive

   # Windows
   ./ns-drive.exe
   ```

## 📖 Usage Guide

### Setting Up Cloud Remotes

1. **Open NS-Drive application**
2. **Navigate to Remotes section**
3. **Click "Add Remote" button**
4. **Select your cloud provider**
5. **Follow the authentication flow**

### Creating Sync Profiles

1. **Go to Profiles section**
2. **Click "Add Profile" button**
3. **Configure sync settings**:
   - Select remote and local paths
   - Set sync direction (pull/push/bi-sync)
   - Configure bandwidth and parallel transfers
   - Add include/exclude patterns

### Running Sync Operations

1. **Navigate to Home dashboard**
2. **Create a new operation tab**
3. **Select a profile to run**
4. **Monitor real-time progress**
5. **Manage multiple operations simultaneously**

## 🔧 Available Commands

| Command                      | Description                                           | Status     |
| ---------------------------- | ----------------------------------------------------- | ---------- |
| `task build`                 | Build the application for current platform            | ✅ Working |
| `task build:macos`           | Build signed macOS .app bundle                        | ✅ Working |
| `task build:macos:bundle`    | Create macOS .app bundle (without signing)            | ✅ Working |
| `task build:macos:sign`      | Sign existing macOS .app bundle                       | ✅ Working |
| `task dev:fe`                | Start frontend development server                     | ✅ Working |
| `task dev:be`                | Start Wails dev server (requires frontend dev server) | ✅ Working |
| `task lint:fe`               | Run ESLint on frontend code                           | ✅ Working |
| `task lint:be`               | Run golangci-lint on backend code                     | ✅ Working |
| `task lint`                  | Run linting on both frontend and backend              | ✅ Working |
| `task clean`                 | Clean all build artifacts                             | ✅ Working |

## 🌐 Supported Cloud Providers

- **Google Drive** - Full read/write access
- **Dropbox** - Complete file synchronization
- **OneDrive** - Microsoft cloud storage
- **Yandex Disk** - Russian cloud service
- **Google Photos** - Photo library backup (read-only)
- **iCloud Drive** - Apple cloud storage
- **And many more** - Any provider supported by rclone

For detailed setup instructions for each provider, refer to the [rclone documentation](https://rclone.org/docs/).

## 📱 Screenshots

### Dashboard

![Homepage](./screenshots/s1.png)

_Multi-tab operation dashboard with real-time monitoring_

### Profile Management

![Profile Manager Page](./screenshots/s2.png)

_Create and configure sync profiles with advanced settings_

### Remote Configuration

![Remote Manager Page](./screenshots/s3.png)

_Manage cloud storage connections and authentication_

## 🏗️ Project Structure

```
ns-drive/
├── desktop/                 # Main application directory
│   ├── backend/            # Go backend code
│   │   ├── app.go         # Main application logic
│   │   ├── services/      # Domain services
│   │   ├── models/        # Data structures
│   │   ├── errors/        # Error handling
│   │   └── utils/         # Utility functions
│   ├── frontend/          # Angular frontend
│   │   ├── src/app/       # Application components
│   │   ├── bindings/      # Wails generated bindings
│   │   └── dist/          # Built frontend assets
│   ├── build/             # Build configuration
│   ├── go.mod             # Go module definition
│   └── main.go            # Application entry point
├── scripts/               # Build and utility scripts
│   └── build-macos.sh    # macOS production build script
├── docs/                  # Documentation
├── screenshots/           # Application screenshots
├── Taskfile.yml          # Build tasks
├── ns-drive              # Built binary (after build)
├── ns-drive.app          # macOS app bundle (after build)
└── README.md             # This file
```

## 🤝 Contributing

1. Fork the repository
2. Create a feature branch
3. Make your changes
4. Run linting: `task lint`
5. Submit a pull request

## 📄 License

This project is licensed under the MIT License - see the LICENSE file for details.

## 🔧 Development Environment

### Environment Variables

Ensure your Go environment is properly configured:

```bash
# Check Go installation
go version  # Should be 1.25+

# Ensure GOPATH/bin is in PATH (for wails3 command)
export PATH="$PATH:$(go env GOPATH)/bin"

# Verify wails3 is available
wails3 version
```

### Configuration Files

| File | Location | Description |
|------|----------|-------------|
| `desktop/build/config.yml` | Project | Wails dev mode configuration |
| `desktop/go.mod` | Project | Go module dependencies |
| `desktop/frontend/package.json` | Project | npm dependencies |
| `~/.config/ns-drive/profiles.json` | User home | Sync profiles configuration |
| `~/.config/ns-drive/rclone.conf` | User home | Rclone remotes configuration |

### Generating Bindings

When you modify Go services or models, regenerate TypeScript bindings:

```bash
cd desktop
wails3 generate bindings
```

Bindings are generated to `desktop/frontend/bindings/` (symlinked as `wailsjs/` for compatibility).

### Linting

```bash
# Lint both frontend and backend
task lint

# Lint frontend only (ESLint)
task lint:fe

# Lint backend only (golangci-lint)
task lint:be
```

## 🐛 Troubleshooting

### Common Issues & Solutions

1. **`go.mod file not found` error when running `task dev:be`**

   ```bash
   # Solution: Run go mod tidy from desktop directory first
   cd desktop && go mod tidy

   # Then retry
   task dev:be
   ```

2. **Build fails with "no matching files found"**

   ```bash
   # Solution: Build frontend first
   cd desktop/frontend && npm run build
   task build
   ```

3. **Dev server fails to connect to frontend**

   ```bash
   # Solution: Ensure frontend is running on correct port
   # Terminal 1 - Start frontend FIRST:
   task dev:fe
   # Wait for "Local: http://localhost:9245/" message

   # Terminal 2 - Then start backend:
   task dev:be
   ```

4. **Wails3 command not found**

   ```bash
   # Solution: Install Wails v3 and update PATH
   go install github.com/wailsapp/wails/v3/cmd/wails3@latest

   # Add to your shell profile (~/.zshrc or ~/.bashrc):
   export PATH="$PATH:$(go env GOPATH)/bin"
   ```

5. **Frontend dependencies errors**

   ```bash
   # Solution: Install with legacy peer deps flag
   cd desktop/frontend && npm install --legacy-peer-deps
   ```

6. **Linker warnings about macOS version**

   ```
   ld: warning: object file was built for newer 'macOS' version (26.0) than being linked (11.0)
   ```

   These warnings are harmless and don't affect functionality. They occur due to CGO compilation targeting older macOS versions.

7. **Port 9245 already in use**

   ```bash
   # Find and kill process using the port
   lsof -i :9245
   kill -9 <PID>

   # Or use a different port
   WAILS_VITE_PORT=9246 task dev:fe
   WAILS_VITE_PORT=9246 task dev:be
   ```

8. **Changes not reflecting in app**

   - Frontend changes: Should auto-reload. If not, refresh the app window
   - Backend changes: Wails watches `*.go` files and auto-rebuilds
   - If stuck, restart both dev servers

### Debug Commands

```bash
# Check if frontend server is running
curl http://localhost:9245

# Check Go module status
cd desktop && go mod verify

# Clean and rebuild
cd desktop/frontend && rm -rf node_modules dist && npm install --legacy-peer-deps
cd desktop && go clean -cache

# View backend logs in real-time
task dev:be  # Logs appear in terminal

# Generate fresh bindings
cd desktop && wails3 generate bindings
```

For architecture details, see [Architecture Documentation](docs/ARCHITECTURE.md).

## 📞 Support

- **Architecture**: See [Architecture Documentation](docs/ARCHITECTURE.md) for technical details
- **Cloud Setup**: Refer to [rclone documentation](https://rclone.org/docs/) for cloud provider setup
- **Issues**: Report bugs and feature requests via GitHub Issues

---

**NS-Drive** - Simplifying cloud storage synchronization with a modern, intuitive interface.
