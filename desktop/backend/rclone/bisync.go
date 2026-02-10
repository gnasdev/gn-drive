package rclone

import (
	"context"
	"crypto/sha256"
	"desktop/backend/config"
	"desktop/backend/delta"
	"desktop/backend/dto"
	"desktop/backend/models"
	"desktop/backend/utils"
	"fmt"
	"log"
	"os"
	"path/filepath"
	"strings"

	"github.com/rclone/rclone/cmd/bisync"
	"github.com/rclone/rclone/fs"
	"github.com/rclone/rclone/fs/filter"
)

func BiSync(ctx context.Context, config config.Config, profile models.Profile, resync bool, outStatus chan *dto.SyncStatusDTO, deltaSvc *delta.DeltaService) error {
	var err error

	// Initialize the config
	fsConfig := fs.GetConfig(ctx)
	opt := &bisync.Options{}
	opt.Force = true
	opt.CompareFlag = "size,modtime"
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

	// Conflict loser: use profile setting or default to "delete"
	conflictLoser := profile.ConflictLoser
	if conflictLoser == "" {
		conflictLoser = bisync.ConflictLoserDelete.String()
	}
	if err = opt.ConflictLoser.Set(conflictLoser); err != nil {
		return fmt.Errorf("invalid conflict_loser %q: %w", conflictLoser, err)
	}

	// Conflict suffix
	if profile.ConflictSuffix != "" {
		opt.ConflictSuffixFlag = profile.ConflictSuffix
	}

	if err = opt.CheckSync.Set(bisync.CheckSyncFalse.String()); err != nil {
		return err
	}

	// Bisync-specific options from profile
	if profile.Resilient {
		opt.Resilient = true
	}
	if profile.MaxLock != "" {
		var d fs.Duration
		if parseErr := d.Set(profile.MaxLock); parseErr != nil {
			return fmt.Errorf("invalid max_lock %q: %w", profile.MaxLock, parseErr)
		}
		opt.MaxLock = d
	}
	if profile.CheckAccess {
		opt.CheckAccess = true
	}
	if profile.DryRun {
		opt.DryRun = true
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

	// Set up filter rules (prefix with {{regexp:}} if UseRegex is enabled)
	filterOpt := CopyFilterOpt(ctx)
	for _, p := range profile.IncludedPaths {
		if profile.UseRegex {
			filterOpt.IncludeRule = append(filterOpt.IncludeRule, "{{regexp:}}"+p)
		} else {
			filterOpt.IncludeRule = append(filterOpt.IncludeRule, p)
		}
	}
	for _, p := range profile.ExcludedPaths {
		if profile.UseRegex {
			filterOpt.ExcludeRule = append(filterOpt.ExcludeRule, "{{regexp:}}"+p)
		} else {
			filterOpt.ExcludeRule = append(filterOpt.ExcludeRule, p)
		}
	}
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

	// Set parallel transfers and checkers (skip if 0 to preserve rclone defaults)
	if profile.Parallel > 0 {
		fsConfig.Transfers = profile.Parallel
		fsConfig.Checkers = profile.Parallel * 2
	}

	// Apply advanced profile options (filtering, safety, performance)
	ctx, err = ApplyProfileOptions(ctx, profile)
	if err != nil {
		return fmt.Errorf("failed to apply profile options: %w", err)
	}

	if err := fsConfig.Reload(ctx); err != nil {
		return err
	}

	// Delta sync: check if we can skip bisync entirely (not for resync)
	srcKey := remoteKey(profile.From)
	dstKey := remoteKey(profile.To)

	if deltaSvc != nil && !resync && !opt.Resync {
		if deltaSvc.ShouldSkipSync(srcKey) && deltaSvc.ShouldSkipSync(dstKey) {
			log.Printf("[delta] No changes on either side, skipping bisync")
			sendSkippedStatus(outStatus)
			_ = deltaSvc.CommitDelta(srcKey)
			_ = deltaSvc.CommitDelta(dstKey)
			return nil
		}
	}

	syncErr := utils.RunRcloneWithRetryAndStats(ctx, true, false, outStatus, func() error {
		return utils.HandleError(bisync.Bisync(ctx, dstFs, srcFs, opt), "Sync failed", nil, nil)
	})

	// Commit delta state after bisync
	if deltaSvc != nil && syncErr == nil {
		// After bisync (or resync), establish baseline and start watchers
		_ = deltaSvc.CommitFullSync(srcFs, srcKey)
		_ = deltaSvc.CommitFullSync(dstFs, dstKey)
	}

	return syncErr
}
