# Cloud Providers Support

NS-Drive supports multiple cloud storage providers through rclone integration. This document provides setup instructions for each supported provider.

## Supported Providers

### Google Drive

- **Type**: `drive`
- **Authentication**: OAuth2
- **Features**: Full read/write access to Google Drive files and folders

### Dropbox

- **Type**: `dropbox`
- **Authentication**: OAuth2
- **Features**: Full access to Dropbox files and folders

### OneDrive

- **Type**: `onedrive`
- **Authentication**: OAuth2
- **Features**: Full access to Microsoft OneDrive files and folders

### Yandex Disk

- **Type**: `yandex`
- **Authentication**: OAuth2
- **Features**: Full access to Yandex Disk files and folders

### Google Photos

- **Type**: `gphotos`
- **Authentication**: OAuth2
- **Features**:
  - Read-only access to Google Photos library
  - Download photos and videos
  - Album organization support
- **Limitations**:
  - Upload functionality may be limited due to Google Photos API restrictions
  - Some metadata may not be preserved

### iCloud Drive

- **Type**: `iclouddrive`
- **Authentication**: Apple ID with App-Specific Password
- **Features**:
  - Full read/write access to iCloud Drive files and folders
  - Support for all file types
- **Setup Requirements**:
  - Apple ID with 2FA enabled
  - App-specific password generated from Apple ID settings

## Setup Instructions

### General Setup Process

1. **Open NS-Drive application**
2. **Navigate to Remotes section**
3. **Click "Add Remote" button**
4. **Select provider type from dropdown**
5. **Enter remote name**
6. **Follow authentication flow**

### Provider-Specific Setup

#### Google Photos Setup

1. Select "Google Photos" as provider type
2. Enter a descriptive name (e.g., "my-photos")
3. Click "Add Remote"
4. Browser will open for Google OAuth
5. Sign in to your Google account
6. Grant permissions to access Google Photos
7. Complete the authorization process

#### iCloud Drive Setup

**Important**: iCloud Drive requires manual setup via command line due to interactive authentication requirements.

1. **Prerequisites**:

   - Disable Advanced Data Protection in iCloud settings
   - Enable "Access iCloud Data on the Web" in iPhone Settings > Apple Account > iCloud
   - Have your Apple device nearby for 2FA codes

2. **Setup Process**:

   - In NS-Drive, try to add iCloud Drive remote (this will show setup instructions)
   - Open Terminal/Command Prompt
   - Run: `rclone config`
   - Choose 'n' for new remote
   - Enter your desired remote name
   - Choose 'iclouddrive' as storage type
   - Enter your Apple ID
   - Enter your regular iCloud password (NOT app-specific password)
   - Enter the 2FA code from your Apple device
   - Complete the interactive setup process

3. **After Setup**:
   - Restart NS-Drive application
   - Your iCloud remote will appear in the remotes list

## Usage Tips

### Google Photos

- **Best for**: Backing up photo libraries, organizing albums
- **Sync considerations**: Large photo libraries may take significant time
- **File organization**: Photos are organized by date and album structure

### iCloud Drive

- **Best for**: Document synchronization, file backup
- **Sync considerations**: Requires stable internet connection
- **File types**: Supports all file types stored in iCloud Drive

### Performance Optimization

- **Bandwidth limiting**: Configure bandwidth limits for large transfers
- **Parallel transfers**: Adjust parallel transfer settings based on your connection
- **Filtering**: Use include/exclude patterns to sync specific file types or folders

## Troubleshooting

### Common Issues

#### Authentication Failures

- **Google Photos**: Ensure Google Photos API is enabled and permissions are granted
- **iCloud Drive**:
  - Verify Advanced Data Protection is disabled
  - Use regular iCloud password, not app-specific password
  - Ensure "Access iCloud Data on the Web" is enabled
  - Complete setup via `rclone config` command line tool

#### Sync Errors

- Check internet connection stability
- Verify remote configuration is correct
- Review sync logs for specific error messages

#### Performance Issues

- Reduce parallel transfer count for slower connections
- Enable bandwidth limiting for background syncs
- Consider syncing during off-peak hours for large transfers

### Getting Help

- Check rclone documentation for provider-specific issues
- Review NS-Drive logs for detailed error information
- Ensure you're using the latest version of NS-Drive

## Security Considerations

### Authentication Tokens

- Tokens are stored securely in rclone configuration
- Tokens have expiration dates and will be refreshed automatically
- Never share your configuration files containing authentication tokens

### App-Specific Passwords (iCloud)

- Use unique app-specific passwords for each application
- Revoke unused app-specific passwords regularly
- Never use your main Apple ID password for third-party applications

### Data Privacy

- NS-Drive only accesses data you explicitly grant permission for
- No data is stored on NS-Drive servers - all operations are local
- Review provider permissions regularly and revoke if no longer needed
