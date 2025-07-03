package rclone

import (
	"context"
	"fmt"

	beConfig "ns-drive/backend/config"
	"ns-drive/backend/models"
	"ns-drive/backend/utils"

	"github.com/rclone/rclone/fs"
	"github.com/rclone/rclone/fs/filter"

	fssync "github.com/rclone/rclone/fs/sync"

	// import fs drivers

	_ "github.com/rclone/rclone/backend/cache"

	_ "github.com/rclone/rclone/backend/drive"

	_ "github.com/rclone/rclone/backend/local"

	_ "github.com/rclone/rclone/backend/dropbox"
	_ "github.com/rclone/rclone/backend/googlephotos"
	_ "github.com/rclone/rclone/backend/iclouddrive"
	_ "github.com/rclone/rclone/backend/onedrive"
	_ "github.com/rclone/rclone/backend/yandex"
)

func Sync(ctx context.Context, config beConfig.Config, task string, profile models.Profile, outLog chan string) error {
	// Initialize the config
	fsConfig := fs.GetConfig(ctx)
	fsConfig.Transfers = profile.Parallel
	fsConfig.Checkers = profile.Parallel

	var err error

	switch task {
	// case "pull":
	case "push":
		profile.From, profile.To = profile.To, profile.From
	}

	srcFs, err := fs.NewFs(ctx, profile.From)
	if utils.HandleError(err, "Failed to initialize source filesystem", nil, nil) != nil {
		return err
	}

	dstFs, err := fs.NewFs(ctx, profile.To)
	if utils.HandleError(err, "Failed to initialize destination filesystem", nil, nil) != nil {
		return err
	}

	// Set bandwidth limit
	if profile.Bandwidth > 0 {
		utils.HandleError(fsConfig.BwLimit.Set(fmt.Sprint(profile.Bandwidth)+"M"), "Failed to set bandwidth limit", nil, nil)
	}

	// Set up filter rules
	filterOpt := filter.GetConfig(ctx).Opt
	filterOpt.IncludeRule = append(filterOpt.IncludeRule, profile.IncludedPaths...)
	filterOpt.ExcludeRule = append(filterOpt.ExcludeRule, profile.ExcludedPaths...)
	newFilter, err := filter.NewFilter(&filterOpt)
	utils.HandleError(err, "Invalid filters file", nil, func() {
		ctx = filter.ReplaceConfig(ctx, newFilter)
	})

	fsConfig.Reload(ctx)

	return utils.RunRcloneWithRetryAndStats(ctx, true, false, outLog, func() error {
		return utils.HandleError(fssync.Sync(ctx, dstFs, srcFs, false), "Sync failed", nil, nil)
	})
}
