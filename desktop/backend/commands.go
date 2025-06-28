package backend

import (
	"context"
	"desktop/backend/dto"
	"desktop/backend/models"
	"desktop/backend/rclone"
	"desktop/backend/utils"
	"errors"
	"fmt"
	"log"
	"net/http"
	"os"
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
	utils.AddCmd(id, func() {
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
	_, err := fsConfig.CreateRemote(ctx, remoteName, remoteType, rc.Params{}, fsConfig.UpdateRemoteOpt{})
	if err != nil {
		a.DeleteRemote(remoteName)
	}

	return dto.NewAppError(err)
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
