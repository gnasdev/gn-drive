# GN Drive

Desktop application for cloud storage synchronization powered by [rclone](https://rclone.org/). Provides a GUI for managing cloud remotes, sync profiles, and automated workflows.

## Features

- **Multi-Cloud Sync** - Google Drive, Dropbox, OneDrive, iCloud Drive, Yandex Disk, Google Photos, and [any rclone-supported provider](https://rclone.org/docs/)
- **Sync Profiles** - Configurable pull/push/bi-sync with bandwidth limits, parallel transfers, and include/exclude patterns
- **Visual Workflow Editor** - Drag-drop board interface for designing multi-step sync workflows (DAG execution)
- **Scheduling** - Cron-based automated sync
- **Multi-tab Operations** - Run and monitor multiple sync operations simultaneously
- **File Operations** - Copy, move, check, dedupe, browse, and delete files on remotes
- **Encrypted Remotes** - Create and manage rclone crypt remotes
- **Import/Export** - Backup and restore all configurations
- **System Tray & Notifications** - Minimize to tray, start at login, desktop notifications
- **Dark/Light Theme** - Responsive UI with theme switching

## Tech Stack

| Component | Technology |
|-----------|------------|
| Backend | Go 1.25 + Wails v3 |
| Frontend | Angular 21 + Tailwind CSS + PrimeNG 21 |
| Database | SQLite |
| Cloud Sync | rclone v1.73.0 |

## Installation

Download the latest release for your platform, or build from source with `task build` (see [Development Guide](docs/DEV_GUIDE.md)).

**macOS note:** On first launch, macOS may block the app. Fix with:

```bash
xattr -cr /Applications/gn-drive.app
```

Or right-click the app > "Open" > click "Open" in the dialog.

## Usage

1. **Add Remotes** - Go to Remotes section, add your cloud provider, and authenticate
2. **Create Profiles** - Set up sync profiles with source/destination paths and sync direction
3. **Run Sync** - Open a tab, select a profile, and monitor real-time progress
4. **Automate** - Create boards for multi-step workflows or schedules for recurring syncs

## License

MIT
