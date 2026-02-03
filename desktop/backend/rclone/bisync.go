package rclone

import (
	"context"
	"crypto/sha256"
	"desktop/backend/config"
	"desktop/backend/models"
	"desktop/backend/utils"
	"fmt"
	"os"
	"path/filepath"
	"strings"

	"github.com/rclone/rclone/cmd/bisync"
	"github.com/rclone/rclone/fs"
	"github.com/rclone/rclone/fs/filter"
)

func BiSync(ctx context.Context, config config.Config, profile models.Profile, resync bool, outLog chan string) error {
	var err error

	// Initialize the config
	fsConfig := fs.GetConfig(ctx)
	opt := &bisync.Options{}
	opt.Force = true
	opt.CompareFlag = "size,modtime,checksum"
	opt.Recover = true
	opt.CreateEmptySrcDirs = true

	// Conflict resolution: use profile setting or default to "newer"
	conflictStrategy := profile.ConflictResolution
	if conflictStrategy == "" {
		conflictStrategy = bisync.PreferNewer.String()
	}
	if err = opt.ConflictResolve.Set(conflictStrategy); err != nil {
		return fmt.Errorf("invalid conflict_resolution %q: %w", conflictStrategy, err)
	}

	if err = opt.ConflictLoser.Set(bisync.ConflictLoserDelete.String()); err != nil {
		return err
	}

	if err = opt.CheckSync.Set(bisync.CheckSyncTrue.String()); err != nil {
		return err
	}

	// Handle resync
	if resync {
		opt.Resync = true
	} else {
		dir, err := os.Getwd()
		if utils.HandleError(err, "Failed to get current working directory", nil, nil) != nil {
			return err
		}
		path := filepath.Join(dir, config.ResyncFilePath)
		if _, err := os.Stat(path); os.IsNotExist(err) {
			_, err = os.Create(path)
			if utils.HandleError(err, "Failed to create .resync file", nil, nil) != nil {
				return err
			}
		}
		filterFileChecksum := "0"
		if len(profile.IncludedPaths)+len(profile.ExcludedPaths) > 0 {
			filterFileChecksum, err = utils.CalculateContentHash(utils.MergeBytes([]byte(strings.Join(profile.IncludedPaths, "")), []byte(strings.Join(profile.ExcludedPaths, ""))), sha256.New)
			if utils.HandleError(err, "Failed to calculate hash of filter file", nil, nil) != nil {
				return err
			}
		}
		filterFileChecksumLinePrefix := profile.From + "|" + profile.To + "|"
		filterFileChecksumLine := filterFileChecksumLinePrefix + filterFileChecksum
		filterFileChecksumLineContained := false
		resyncContent, err := os.ReadFile(path)
		if utils.HandleError(err, "Failed to read .resync file", nil, nil) != nil {
			return err
		}
		resyncContentStrArr := strings.Split(string(resyncContent), "\n")
		for _, line := range resyncContentStrArr {
			if line == filterFileChecksumLine {
				filterFileChecksumLineContained = true
				break
			}
		}
		if !filterFileChecksumLineContained {
			// Update .resync file content
			newResyncContentStrArr := []string{}
			isAdded := false
			for _, line := range resyncContentStrArr {
				if line == "" {
					continue
				}
				if strings.HasPrefix(line, filterFileChecksumLinePrefix) {
					newResyncContentStrArr = append(newResyncContentStrArr, filterFileChecksumLine)
					isAdded = true
				} else {
					newResyncContentStrArr = append(newResyncContentStrArr, line)
				}
			}
			if !isAdded {
				newResyncContentStrArr = append(newResyncContentStrArr, filterFileChecksumLine)
			}
			// Write to .resync file
			err = os.WriteFile(path, []byte(strings.Join(newResyncContentStrArr, "\n")), 0644)
			if utils.HandleError(err, "Failed to write to .resync file", nil, nil) != nil {
				return err
			}
			// Set resync flag
			opt.Resync = true
			// if err = opt.ResyncMode.Set(bisync.PreferNewer.String()); err != nil {
			// 	return err
			// }
		}
	}

	srcFs, err := fs.NewFs(ctx, profile.From)
	if utils.HandleError(err, "Failed to initialize source filesystem", nil, nil) != nil {
		return err
	}

	dstFs, err := fs.NewFs(ctx, profile.To)
	if utils.HandleError(err, "Failed to initialize destination filesystem", nil, nil) != nil {
		return err
	}

	// Set up filter rules
	filterOpt := filter.GetConfig(ctx).Opt
	filterOpt.IncludeRule = append(filterOpt.IncludeRule, profile.IncludedPaths...)
	filterOpt.ExcludeRule = append(filterOpt.ExcludeRule, profile.ExcludedPaths...)
	newFilter, err := filter.NewFilter(&filterOpt)
	if err := utils.HandleError(err, "Invalid filters file", nil, func() {
		ctx = filter.ReplaceConfig(ctx, newFilter)
	}); err != nil {
		return err
	}

	// Set bandwidth limit
	if profile.Bandwidth > 0 {
		if err := utils.HandleError(fsConfig.BwLimit.Set(fmt.Sprint(profile.Bandwidth)+"M"), "Failed to set bandwidth limit", nil, nil); err != nil {
			return err
		}
	}

	// Set parallel transfers
	fsConfig.Transfers = profile.Parallel

	fsConfig.Progress = true

	// Apply advanced profile options (filtering, safety, performance)
	ctx, err = ApplyProfileOptions(ctx, profile)
	if err != nil {
		return fmt.Errorf("failed to apply profile options: %w", err)
	}

	if err := fsConfig.Reload(ctx); err != nil {
		return err
	}

	return utils.RunRcloneWithRetryAndStats(ctx, true, false, outLog, func() error {
		return utils.HandleError(bisync.Bisync(ctx, dstFs, srcFs, opt), "Sync failed", nil, nil)
	})
}
