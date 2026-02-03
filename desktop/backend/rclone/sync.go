package rclone

import (
	"context"
	"fmt"

	beConfig "desktop/backend/config"
	"desktop/backend/models"
	"desktop/backend/utils"

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

	switch task {
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
		if err := utils.HandleError(fsConfig.BwLimit.Set(fmt.Sprint(profile.Bandwidth)+"M"), "Failed to set bandwidth limit", nil, nil); err != nil {
			return err
		}
	}

	// Set up filter rules (include/exclude patterns)
	filterOpt := filter.GetConfig(ctx).Opt
	filterOpt.IncludeRule = append(filterOpt.IncludeRule, profile.IncludedPaths...)
	filterOpt.ExcludeRule = append(filterOpt.ExcludeRule, profile.ExcludedPaths...)
	newFilter, err := filter.NewFilter(&filterOpt)
	if err := utils.HandleError(err, "Invalid filters file", nil, func() {
		ctx = filter.ReplaceConfig(ctx, newFilter)
	}); err != nil {
		return err
	}

	// Apply advanced profile options (filtering, safety, performance)
	ctx, err = ApplyProfileOptions(ctx, profile)
	if err != nil {
		return fmt.Errorf("failed to apply profile options: %w", err)
	}

	if err := fsConfig.Reload(ctx); err != nil {
		return err
	}

	return utils.RunRcloneWithRetryAndStats(ctx, true, false, outLog, func() error {
		return utils.HandleError(fssync.Sync(ctx, dstFs, srcFs, false), "Sync failed", nil, nil)
	})
}
