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

```bash
# Option 1: Two-terminal setup (recommended)
# Terminal 1: Start frontend dev server
task dev:fe

# Terminal 2: Start Wails dev server (with hot reload)
task dev:be
```

### Production Build

```bash
task build
# Creates: ns-drive binary in project root
```

## 🚀 Quick Start

1. **Clone the repository**

   ```bash
   git clone <repository-url>
   cd ns-drive
   ```

2. **Install dependencies**

   ```bash
   cd desktop/frontend && npm install --legacy-peer-deps
   ```

3. **Run in development mode**

   ```bash
   # Start frontend dev server (Terminal 1)
   task dev:fe

   # Start Wails dev server (Terminal 2)
   task dev:be
   ```

4. **Build for production**

   ```bash
   task build
   ```

5. **Run the application**
   - Execute `./ns-drive` (macOS/Linux) or `./ns-drive.exe` (Windows)

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

| Command        | Description                                           | Status     |
| -------------- | ----------------------------------------------------- | ---------- |
| `task build`   | Build the application for current platform            | ✅ Working |
| `task dev:fe`  | Start frontend development server                     | ✅ Working |
| `task dev:be`  | Start Wails dev server (requires frontend dev server) | ✅ Working |
| `task lint:fe` | Run ESLint on frontend code                           | ✅ Working |
| `task lint:be` | Run golangci-lint on backend code                     | ✅ Working |
| `task lint`    | Run linting on both frontend and backend              | ✅ Working |

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
├── docs/                  # Documentation
├── screenshots/           # Application screenshots
├── Taskfile.yml          # Build tasks
├── ns-drive              # Built binary (after build)
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

## 🐛 Troubleshooting

### Common Issues

1. **Build failures**: Check Go/Node versions and dependencies
2. **Sync errors**: Verify remote configuration and permissions
3. **UI freezing**: Check for blocking operations on main thread
4. **Memory leaks**: Ensure proper cleanup of subscriptions

### Debug Commands

```bash
# Enable debug mode with hot reload
task dev:be

# Run linting
task lint

# Install frontend dependencies
cd desktop/frontend && npm install --legacy-peer-deps

# Build frontend
cd desktop/frontend && npm run build

# Generate TypeScript bindings (done automatically during build)
cd desktop && wails3 generate bindings
```

### Common Issues & Solutions

1. **Build fails with "no matching files found"**

   ```bash
   # Solution: Build frontend first
   cd desktop/frontend && npm run build
   task build
   ```

2. **Dev server fails to start**

   ```bash
   # Solution: Start frontend dev server first
   # Terminal 1:
   task dev:fe

   # Terminal 2:
   task dev:be
   ```

3. **Wails3 command not found**

   ```bash
   # Solution: Install Wails v3
   go install github.com/wailsapp/wails/v3/cmd/wails3@latest
   ```

4. **Frontend dependencies missing**

   ```bash
   # Solution: Install dependencies
   cd desktop/frontend && npm install --legacy-peer-deps
   ```

5. **Linker warnings about macOS version**

   ```
   ld: warning: object file was built for newer 'macOS' version (26.0) than being linked (11.0)
   ```

   These warnings are harmless and don't affect functionality.

For more detailed troubleshooting, see [RULES.md](RULES.md).

## 📞 Support

- **Architecture**: See [Architecture Documentation](docs/ARCHITECTURE.md) for technical details
- **Cloud Setup**: Refer to [rclone documentation](https://rclone.org/docs/) for cloud provider setup
- **Issues**: Report bugs and feature requests via GitHub Issues

---

**NS-Drive** - Simplifying cloud storage synchronization with a modern, intuitive interface.
