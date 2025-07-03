package backend

import (
	"context"
	"desktop/backend/dto"
	"desktop/backend/models"
	"desktop/backend/rclone"
	"desktop/backend/utils"
	"encoding/json"
	"errors"
	"fmt"
	"log"
	"net/http"
	"os"
	"time"

	fsConfig "github.com/rclone/rclone/fs/config"
	"github.com/rclone/rclone/fs/rc"
	"github.com/rclone/rclone/lib/oauthutil"
	"github.com/wailsapp/wails/v2/pkg/runtime"
)

func (a *App) Sync(task string, profile models.Profile) int {
	return a.SyncWithTab(task, profile, "")
}

func (a *App) SyncWithTab(task string, profile models.Profile, tabId string) int {
	id := time.Now().Nanosecond()

	ctx, cancel := context.WithCancel(context.Background())

	config := a.ConfigInfo.EnvConfig

	// Send command started event to clear previous output
	var j []byte
	if tabId != "" {
		j, _ = utils.NewCommandStartedDTOWithTab(id, task, tabId).ToJSON()
	} else {
		j, _ = utils.NewCommandStartedDTO(id, task).ToJSON()
	}
	a.oc <- j

	ctx, err := rclone.InitConfig(ctx, config.DebugMode)
	if utils.HandleError(err, "", nil, nil) != nil {
		var j []byte
		if tabId != "" {
			j, _ = utils.NewCommandErrorDTOWithTab(id, err, tabId).ToJSON()
		} else {
			j, _ = utils.NewCommandErrorDTO(id, err).ToJSON()
		}
		a.oc <- j
		cancel()
		return 0
	}

	outLog := make(chan string)

	// Start sync status reporting
	stopSyncStatus := utils.StartSyncStatusReporting(a.oc, id, task, tabId)

	utils.AddCmd(id, func() {
		stopSyncStatus()
		close(outLog)
		cancel()
	})

	if tabId != "" {
		utils.AddTabMapping(id, tabId)
	}

	go func() {
		for {
			logEntry, ok := <-outLog
			if !ok { // channel is closed
				break
			}
			if config.DebugMode {
				fmt.Println("--------------------")
				fmt.Println(logEntry)
			}
			var j []byte
			if tabId != "" {
				j, _ = utils.NewCommandOutputDTOWithTab(id, logEntry, tabId).ToJSON()
			} else {
				j, _ = utils.NewCommandOutputDTO(id, logEntry).ToJSON()
			}
			a.oc <- j
		}
	}()

	go func() {
		switch task {
		case "pull":
			err = rclone.Sync(ctx, config, "pull", profile, outLog)
		case "push":
			err = rclone.Sync(ctx, config, "push", profile, outLog)
		case "bi":
			err = rclone.BiSync(ctx, config, profile, false, outLog)
		case "bi-resync":
			err = rclone.BiSync(ctx, config, profile, true, outLog)
		}

		if utils.HandleError(err, "", nil, nil) != nil {
			var j []byte
			if tabId != "" {
				j, _ = utils.NewCommandErrorDTOWithTab(0, err, tabId).ToJSON()
			} else {
				j, _ = utils.NewCommandErrorDTO(0, err).ToJSON()
			}
			a.oc <- j
		}

		var j []byte
		if tabId != "" {
			j, _ = utils.NewCommandStoppedDTOWithTab(id, tabId).ToJSON()
			utils.RemoveTabMapping(id)
		} else {
			j, _ = utils.NewCommandStoppedDTO(id).ToJSON()
		}
		a.oc <- j

		log.Println("Sync stopped!")
	}()

	return id
}

func (a *App) StopCommand(id int) {
	cancel, exists := utils.GetCmd(id)
	if !exists {
		tabId, hasTab := utils.GetTabMapping(id)
		var j []byte
		if hasTab {
			j, _ = utils.NewCommandErrorDTOWithTab(id, errors.New("command not found"), tabId).ToJSON()
		} else {
			j, _ = utils.NewCommandErrorDTO(id, errors.New("command not found")).ToJSON()
		}
		a.oc <- j
		return
	}

	cancel()

	tabId, hasTab := utils.GetTabMapping(id)
	var res []byte
	if hasTab {
		res, _ = utils.NewCommandStoppedDTOWithTab(id, tabId).ToJSON()
		utils.RemoveTabMapping(id)
	} else {
		res, _ = utils.NewCommandStoppedDTO(id).ToJSON()
	}
	a.oc <- res
}

func (a *App) GetConfigInfo() models.ConfigInfo {
	return a.ConfigInfo
}

func (a *App) UpdateProfiles(profiles models.Profiles) *dto.AppError {
	a.ConfigInfo.Profiles = profiles

	profilesJson, err := profiles.ToJSON()
	if err != nil {
		return dto.NewAppError(err)
	}

	err = os.WriteFile(a.ConfigInfo.EnvConfig.ProfileFilePath, profilesJson, 0644)
	if err != nil {
		return dto.NewAppError(err)
	}

	return nil
}

func (a *App) GetRemotes() []fsConfig.Remote {
	return fsConfig.GetRemotes()
}

func (a *App) AddRemote(remoteName string, remoteType string, remoteConfig map[string]string) *dto.AppError {
	ctx := context.Background()

	// Handle special cases for providers that need interactive setup
	switch remoteType {
	case "iclouddrive":
		return a.addICloudRemote(ctx, remoteName, remoteConfig)
	default:
		// Standard OAuth flow for other providers
		_, err := fsConfig.CreateRemote(ctx, remoteName, remoteType, rc.Params{}, fsConfig.UpdateRemoteOpt{})
		if err != nil {
			a.DeleteRemote(remoteName)
		}
		return dto.NewAppError(err)
	}
}

func (a *App) addICloudRemote(ctx context.Context, remoteName string, remoteConfig map[string]string) *dto.AppError {
	// iCloud Drive requires interactive setup with Apple ID, password, and 2FA
	// For now, we'll return an error with instructions for manual setup
	errorMsg := fmt.Sprintf(`iCloud Drive setup requires interactive configuration.

Please use the following steps to set up your iCloud remote:

1. Open Terminal/Command Prompt
2. Run: rclone config
3. Choose 'n' for new remote
4. Enter name: %s
5. Choose 'iclouddrive' as storage type
6. Enter your Apple ID
7. Enter your regular iCloud password (NOT app-specific password)
8. Enter the 2FA code from your Apple device
9. Complete the setup process

After setup, restart NS-Drive to see your iCloud remote.

Note: Advanced Data Protection must be disabled in iCloud settings.`, remoteName)

	return dto.NewAppError(errors.New(errorMsg))
}

func (a *App) OpenICloudSetup(remoteName string) *dto.AppError {
	// This method can be called from frontend to open terminal with rclone config
	// For now, just return instructions
	return a.addICloudRemote(context.Background(), remoteName, nil)
}

func (a *App) StopAddingRemote() *dto.AppError {
	const OAUTH_REDIRECT_URL = oauthutil.RedirectURL
	resp, err := http.Get(OAUTH_REDIRECT_URL)
	if err != nil {
		return dto.NewAppError(err)
	}

	defer resp.Body.Close()
	return nil
}

func (a *App) DeleteRemote(remoteName string) {
	fsConfig.DeleteRemote(remoteName)
}

func (a *App) SyncWithTabId(task string, profile models.Profile, tabId string) int {
	return a.SyncWithTab(task, profile, tabId)
}

// File dialog functions
func (a *App) OpenFileDialog(title string, filters []string) (string, *dto.AppError) {
	filePath, err := runtime.OpenFileDialog(a.ctx, runtime.OpenDialogOptions{
		Title: title,
		Filters: []runtime.FileFilter{
			{
				DisplayName: "JSON Files",
				Pattern:     "*.json",
			},
		},
	})
	if err != nil {
		return "", dto.NewAppError(err)
	}
	return filePath, nil
}

func (a *App) SaveFileDialog(title string, defaultFilename string, filters []string) (string, *dto.AppError) {
	filePath, err := runtime.SaveFileDialog(a.ctx, runtime.SaveDialogOptions{
		Title:           title,
		DefaultFilename: defaultFilename,
		Filters: []runtime.FileFilter{
			{
				DisplayName: "JSON Files",
				Pattern:     "*.json",
			},
		},
	})
	if err != nil {
		return "", dto.NewAppError(err)
	}
	return filePath, nil
}

// Profile import/export functions
func (a *App) ExportProfiles() *dto.AppError {
	filePath, appErr := a.SaveFileDialog("Export Profiles", "profiles.json", []string{"*.json"})
	if appErr != nil {
		return appErr
	}

	if filePath == "" {
		// User cancelled the dialog
		return nil
	}

	profilesJson, err := a.ConfigInfo.Profiles.ToJSON()
	if err != nil {
		return dto.NewAppError(err)
	}

	err = os.WriteFile(filePath, profilesJson, 0644)
	if err != nil {
		return dto.NewAppError(err)
	}

	return nil
}

func (a *App) ImportProfiles() *dto.AppError {
	filePath, appErr := a.OpenFileDialog("Import Profiles", []string{"*.json"})
	if appErr != nil {
		return appErr
	}

	if filePath == "" {
		// User cancelled the dialog
		return nil
	}

	// Read the file
	fileData, err := os.ReadFile(filePath)
	if err != nil {
		return dto.NewAppError(fmt.Errorf("failed to read file: %w", err))
	}

	// Parse the JSON
	var importedProfiles models.Profiles
	err = json.Unmarshal(fileData, &importedProfiles)
	if err != nil {
		return dto.NewAppError(fmt.Errorf("invalid JSON format: %w", err))
	}

	// Validate profiles
	for i, profile := range importedProfiles {
		if profile.Name == "" {
			return dto.NewAppError(fmt.Errorf("profile %d has no name", i+1))
		}
		if profile.From == "" {
			return dto.NewAppError(fmt.Errorf("profile '%s' has no source path", profile.Name))
		}
		if profile.To == "" {
			return dto.NewAppError(fmt.Errorf("profile '%s' has no destination path", profile.Name))
		}
	}

	// Merge with existing profiles (append imported profiles)
	a.ConfigInfo.Profiles = append(a.ConfigInfo.Profiles, importedProfiles...)

	// Save to file
	return a.UpdateProfiles(a.ConfigInfo.Profiles)
}

// Remote import/export functions
func (a *App) ExportRemotes() *dto.AppError {
	filePath, appErr := a.SaveFileDialog("Export Remotes", "remotes.json", []string{"*.json"})
	if appErr != nil {
		return appErr
	}

	if filePath == "" {
		// User cancelled the dialog
		return nil
	}

	// Get current remotes
	remotes := fsConfig.GetRemotes()

	// Get the config storage to access all configuration data
	configData := fsConfig.Data()

	// Convert to our models.Remotes format for export
	var exportRemotes models.Remotes
	for _, remote := range remotes {
		exportRemote := models.Remote{
			Name:  remote.Name,
			Type:  remote.Type,
			Token: make(map[string]any),
		}

		// Export ALL configuration data for this remote
		if configData.HasSection(remote.Name) {
			keyList := configData.GetKeyList(remote.Name)
			for _, key := range keyList {
				if value, found := configData.GetValue(remote.Name, key); found {
					exportRemote.Token[key] = value
				}
			}
		}

		// Add basic remote info to Token field for export
		exportRemote.Token["source"] = remote.Source
		exportRemote.Token["description"] = remote.Description

		exportRemotes = append(exportRemotes, exportRemote)
	}

	remotesJson, err := exportRemotes.ToJSON()
	if err != nil {
		return dto.NewAppError(err)
	}

	err = os.WriteFile(filePath, remotesJson, 0644)
	if err != nil {
		return dto.NewAppError(err)
	}

	return nil
}

func (a *App) ImportRemotes() *dto.AppError {
	filePath, appErr := a.OpenFileDialog("Import Remotes", []string{"*.json"})
	if appErr != nil {
		return appErr
	}

	if filePath == "" {
		// User cancelled the dialog
		return nil
	}

	// Read the file
	fileData, err := os.ReadFile(filePath)
	if err != nil {
		return dto.NewAppError(fmt.Errorf("failed to read file: %w", err))
	}

	// Parse the JSON
	var importedRemotes models.Remotes
	err = json.Unmarshal(fileData, &importedRemotes)
	if err != nil {
		return dto.NewAppError(fmt.Errorf("invalid JSON format: %w", err))
	}

	if len(importedRemotes) == 0 {
		return dto.NewAppError(fmt.Errorf("no remotes found in the import file"))
	}

	// Get existing remotes once to avoid repeated calls
	existingRemotes := fsConfig.GetRemotes()
	existingNames := make(map[string]bool)
	for _, existing := range existingRemotes {
		existingNames[existing.Name] = true
	}

	// Validate all remotes first before creating any
	var errors []string
	for _, remote := range importedRemotes {
		if remote.Name == "" {
			errors = append(errors, "found remote with no name")
			continue
		}
		if remote.Type == "" {
			errors = append(errors, fmt.Sprintf("remote '%s' has no type", remote.Name))
			continue
		}
		if existingNames[remote.Name] {
			errors = append(errors, fmt.Sprintf("remote '%s' already exists", remote.Name))
			continue
		}
	}

	if len(errors) > 0 {
		return dto.NewAppError(fmt.Errorf("validation errors: %v", errors))
	}

	// Create remotes one by one
	var createdRemotes []string
	var failedRemotes []string

	// Get the config storage to set configuration data
	configData := fsConfig.Data()

	for _, remote := range importedRemotes {
		// First, set all the configuration data for this remote
		for key, value := range remote.Token {
			// Skip metadata fields that aren't actual config
			if key == "source" || key == "description" {
				continue
			}
			// Convert value to string for storage
			if strValue, ok := value.(string); ok {
				configData.SetValue(remote.Name, key, strValue)
			}
		}

		// Set the type
		configData.SetValue(remote.Name, "type", remote.Type)

		// Save the configuration
		err := configData.Save()
		if err != nil {
			// If saving fails, clean up and track the failure
			configData.DeleteSection(remote.Name)
			failedRemotes = append(failedRemotes, fmt.Sprintf("%s (config save failed: %s)", remote.Name, err.Error()))
			continue
		}

		createdRemotes = append(createdRemotes, remote.Name)
	}

	// Report results
	if len(failedRemotes) > 0 {
		if len(createdRemotes) > 0 {
			return dto.NewAppError(fmt.Errorf("partially successful: created %d remotes (%v), failed %d remotes (%v)",
				len(createdRemotes), createdRemotes, len(failedRemotes), failedRemotes))
		} else {
			return dto.NewAppError(fmt.Errorf("failed to create any remotes: %v", failedRemotes))
		}
	}

	return nil
}
