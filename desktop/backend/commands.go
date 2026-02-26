package backend

import (
	"context"
	"desktop/backend/dto"
	"desktop/backend/models"
	"desktop/backend/rclone"
	"desktop/backend/utils"
	"desktop/backend/validation"
	"encoding/json"
	"errors"
	"fmt"
	"log"
	"net/http"
	"os"
	"strings"
	"sync"
	"time"

	fsConfig "github.com/rclone/rclone/fs/config"
	"github.com/rclone/rclone/fs/rc"
	"github.com/rclone/rclone/lib/oauthutil"
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

	if a.oc == nil {
		log.Printf("ERROR: Event channel is nil, skipping event send")
		return 0
	}
	a.oc <- j

	ctx, err := rclone.NewTaskContext(ctx, id)
	if err != nil {
		var j []byte
		if tabId != "" {
			j, _ = a.errorHandler.HandleErrorWithTab(err, tabId, "sync", "init_config").ToJSON()
		} else {
			j, _ = a.errorHandler.HandleError(err, "sync", "init_config").ToJSON()
		}
		a.oc <- j
		cancel()
		return 0
	}

	outStatus := make(chan *dto.SyncStatusDTO, 100)
	var outStatusClosed bool
	var outStatusMutex sync.Mutex
	closeOutStatus := func() {
		outStatusMutex.Lock()
		defer outStatusMutex.Unlock()
		if !outStatusClosed {
			outStatusClosed = true
			close(outStatus)
		}
	}

	utils.AddCmd(id, func() {
		closeOutStatus()
		cancel()
	})

	if tabId != "" {
		utils.AddTabMapping(id, tabId)
	}

	go func() {
		for status := range outStatus {
			// Enrich DTO with task identity
			status.Id = &id
			if tabId != "" {
				status.TabId = &tabId
			}
			status.Action = task

			// Extract LogMessages for backward-compatible text output (non-blocking)
			for _, logMsg := range status.LogMessages {
				if config.DebugMode {
					fmt.Println("--------------------")
					fmt.Println(logMsg)
				}
				var j []byte
				if tabId != "" {
					j, _ = utils.NewCommandOutputDTOWithTab(id, logMsg, tabId).ToJSON()
				} else {
					j, _ = utils.NewCommandOutputDTO(id, logMsg).ToJSON()
				}
				select {
				case a.oc <- j:
				default:
					// Drop log message if channel full to prevent blocking status delivery
				}
			}

			// Send structured SyncStatusDTO with timeout to guarantee delivery
			if j, err := status.ToJSON(); err == nil {
				select {
				case a.oc <- j:
				case <-time.After(100 * time.Millisecond):
				}
			}
		}
	}()

	go func() {
		switch task {
		case "pull":
			err = rclone.Sync(ctx, config, "pull", profile, outStatus, nil)
		case "push":
			err = rclone.Sync(ctx, config, "push", profile, outStatus, nil)
		case "bi":
			err = rclone.BiSync(ctx, config, profile, false, outStatus, nil)
		case "bi-resync":
			err = rclone.BiSync(ctx, config, profile, true, outStatus, nil)
		}

		// Close the outStatus channel to unblock the reader goroutine
		closeOutStatus()

		if err != nil {
			var j []byte
			if tabId != "" {
				j, _ = a.errorHandler.HandleErrorWithTab(err, tabId, "sync", task).ToJSON()
			} else {
				j, _ = a.errorHandler.HandleError(err, "sync", task).ToJSON()
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
	if !a.initialized {
		a.initializeConfig()
	}
	return a.ConfigInfo
}

func (a *App) UpdateProfiles(profiles models.Profiles) *dto.AppError {
	a.ConfigInfo.Profiles = profiles

	profilesJson, err := json.MarshalIndent(profiles, "", "    ")
	if err != nil {
		a.errorHandler.HandleError(err, "update_profiles", "json_marshal")
		return dto.NewAppError(err)
	}

	err = os.WriteFile(a.ConfigInfo.EnvConfig.ProfileFilePath, profilesJson, 0600)
	if err != nil {
		a.errorHandler.HandleError(err, "update_profiles", "write_file", a.ConfigInfo.EnvConfig.ProfileFilePath)
		return dto.NewAppError(err)
	}

	return nil
}

func (a *App) GetRemotes() []fsConfig.Remote {
	if !a.initialized {
		a.initializeConfig()
	}

	if a.cachedRemotes != nil {
		return a.cachedRemotes
	}
	a.cachedRemotes = fsConfig.GetRemotes()
	return a.cachedRemotes
}

func (a *App) AddRemote(remoteName string, remoteType string, remoteConfig map[string]string) *dto.AppError {
	ctx := context.Background()

	// Validate remote name
	if err := validation.ValidateRemoteName(remoteName); err != nil {
		return dto.NewAppError(err)
	}

	// Handle special cases for providers that need interactive setup
	var err *dto.AppError
	switch remoteType {
	case "iclouddrive":
		err = a.addICloudRemote(ctx, remoteName, remoteConfig)
	default:
		// Standard OAuth flow for other providers
		err = a.addOAuthRemote(ctx, remoteName, remoteType, remoteConfig)
	}

	if err == nil {
		a.invalidateRemotesCache()
	}
	return err
}

func (a *App) addOAuthRemote(ctx context.Context, remoteName string, remoteType string, remoteConfig map[string]string) *dto.AppError {

	// Prepare configuration parameters
	configParams := rc.Params{}

	// Set basic configuration
	for key, value := range remoteConfig {
		configParams[key] = value
	}

	// For OAuth providers, we need to handle the authentication flow
	switch remoteType {
	case "drive", "dropbox", "onedrive", "box", "yandex", "gphotos":
		// These providers require OAuth authentication
		// Use auto config for desktop applications
		configParams["config_is_local"] = "true"

		// Create the remote with OAuth flow
		_, err := fsConfig.CreateRemote(ctx, remoteName, remoteType, configParams, fsConfig.UpdateRemoteOpt{
			NonInteractive: false, // Allow interactive OAuth
			NoObscure:      false, // Allow password obscuring
		})

		if err != nil {
			a.errorHandler.HandleError(err, "add_oauth_remote", remoteType, remoteName)
			a.DeleteRemote(remoteName) // Clean up on failure
			return dto.NewAppError(err)
		}

		return nil

	default:
		// For non-OAuth providers, use standard creation
		_, err := fsConfig.CreateRemote(ctx, remoteName, remoteType, configParams, fsConfig.UpdateRemoteOpt{})
		if err != nil {
			a.errorHandler.HandleError(err, "add_remote", remoteType, remoteName)
			a.DeleteRemote(remoteName)
			return dto.NewAppError(err)
		}

		return nil
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

After setup, restart GN Drive to see your iCloud remote.

Note: Advanced Data Protection must be disabled in iCloud settings.`, remoteName)

	return dto.NewAppError(errors.New(errorMsg))
}

func (a *App) OpenICloudSetup(remoteName string) *dto.AppError {
	// This method can be called from frontend to open terminal with rclone config
	// For now, just return instructions
	return a.addICloudRemote(context.Background(), remoteName, nil)
}

func (a *App) GetOAuthURL(remoteType string) (string, *dto.AppError) {
	// Return the OAuth authorization URL for the given remote type
	// This can be used to open the browser for authentication
	switch remoteType {
	case "drive":
		return "https://accounts.google.com/oauth/authorize", nil
	case "dropbox":
		return "https://www.dropbox.com/oauth2/authorize", nil
	case "onedrive":
		return "https://login.microsoftonline.com/common/oauth2/v2.0/authorize", nil
	case "box":
		return "https://account.box.com/api/oauth2/authorize", nil
	case "yandex":
		return "https://oauth.yandex.com/authorize", nil
	default:
		return "", dto.NewAppError(fmt.Errorf("OAuth not supported for remote type: %s", remoteType))
	}
}

func (a *App) ReauthRemote(remoteName string) *dto.AppError {
	if !a.initialized {
		a.initializeConfig()
	}

	// Find current remote to get its type
	remotes := fsConfig.GetRemotes()
	var remoteType string
	found := false
	for _, r := range remotes {
		if r.Name == remoteName {
			remoteType = r.Type
			found = true
			break
		}
	}
	if !found {
		return dto.NewAppError(fmt.Errorf("remote %q not found", remoteName))
	}

	// iCloud requires manual re-authentication via terminal
	if remoteType == "iclouddrive" {
		return dto.NewAppError(fmt.Errorf("iCloud Drive requires manual re-authentication via terminal: rclone config reconnect %s:", remoteName))
	}

	// Call CreateRemote with existing name to trigger OAuth re-authentication
	ctx := context.Background()
	configParams := rc.Params{
		"config_is_local": "true",
	}

	_, err := fsConfig.CreateRemote(ctx, remoteName, remoteType, configParams, fsConfig.UpdateRemoteOpt{
		NonInteractive: false,
		NoObscure:      false,
	})
	if err != nil {
		a.errorHandler.HandleError(err, "reauth_remote", remoteType, remoteName)
		// Do NOT delete remote on failure — preserve existing config
		return dto.NewAppError(err)
	}

	a.invalidateRemotesCache()
	return nil
}

func (a *App) StopAddingRemote() *dto.AppError {
	const OAUTH_REDIRECT_URL = oauthutil.RedirectURL
	resp, err := http.Get(OAUTH_REDIRECT_URL)
	if err != nil {
		return dto.NewAppError(err)
	}

	defer func() {
		if err := resp.Body.Close(); err != nil {
			fmt.Printf("Warning: failed to close response body: %v\n", err)
		}
	}()
	return nil
}

func (a *App) DeleteRemote(remoteName string) *dto.AppError {
	if !a.initialized {
		a.initializeConfig()
	}

	// Find and remove profiles that use this remote
	var profilesToKeep []models.Profile
	deletedProfileCount := 0

	for _, profile := range a.ConfigInfo.Profiles {
		// Check if profile uses this remote in from or to paths
		fromRemote := ""
		toRemote := ""

		// Parse from path
		if colonIndex := strings.Index(profile.From, ":"); colonIndex > 0 {
			fromRemote = profile.From[:colonIndex]
		}

		// Parse to path
		if colonIndex := strings.Index(profile.To, ":"); colonIndex > 0 {
			toRemote = profile.To[:colonIndex]
		}

		// Keep profile only if it doesn't use the remote being deleted
		if fromRemote != remoteName && toRemote != remoteName {
			profilesToKeep = append(profilesToKeep, profile)
		} else {
			deletedProfileCount++
		}
	}

	// Update profiles if any were deleted
	if deletedProfileCount > 0 {
		a.ConfigInfo.Profiles = profilesToKeep

		// Save updated profiles to file
		profilesJson, err := json.MarshalIndent(a.ConfigInfo.Profiles, "", "    ")
		if err != nil {
			a.errorHandler.HandleError(err, "delete_remote", "json_marshal")
			return dto.NewAppError(err)
		}

		err = os.WriteFile(a.ConfigInfo.EnvConfig.ProfileFilePath, profilesJson, 0600)
		if err != nil {
			a.errorHandler.HandleError(err, "delete_remote", "write_file", a.ConfigInfo.EnvConfig.ProfileFilePath)
			return dto.NewAppError(err)
		}

	}

	// Delete the remote from rclone config
	fsConfig.DeleteRemote(remoteName)
	a.invalidateRemotesCache()

	return nil
}

func (a *App) SyncWithTabId(task string, profile models.Profile, tabId string) int {
	return a.SyncWithTab(task, profile, tabId)
}
