package rclone

import (
	"context"
	"desktop/backend/delta"
	"desktop/backend/dto"
	"fmt"
	"log"
	"strings"
	"time"

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

func Sync(ctx context.Context, config beConfig.Config, task string, profile models.Profile, outStatus chan *dto.SyncStatusDTO, deltaSvc *delta.DeltaService) error {
	// Initialize the config
	fsConfig := fs.GetConfig(ctx)
	if profile.Parallel > 0 {
		fsConfig.Transfers = profile.Parallel
		fsConfig.Checkers = profile.Parallel * 2
	}

	switch task {
	case "pull":
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

	// Apply advanced profile options (filtering, safety, performance)
	ctx, err = ApplyProfileOptions(ctx, profile)
	if err != nil {
		return fmt.Errorf("failed to apply profile options: %w", err)
	}

	if err := fsConfig.Reload(ctx); err != nil {
		return err
	}

	// Delta sync: check if we can skip or scope the sync
	srcKey := remoteKey(profile.From)
	dstKey := remoteKey(profile.To)
	usedDelta := false
	var drainedChanges []delta.FileChange

	if deltaSvc != nil {
		// Check if both sides report no changes → skip entirely
		if deltaSvc.ShouldSkipSync(srcKey) && deltaSvc.ShouldSkipSync(dstKey) {
			log.Printf("[delta] No changes detected on either side, skipping sync")
			sendSkippedStatus(outStatus)
			_ = deltaSvc.CommitDelta(srcKey)
			_ = deltaSvc.CommitDelta(dstKey)
			return nil
		}

		// Try to get changes for filter scoping
		srcChanges := deltaSvc.GetChanges(srcKey)
		if srcChanges != nil && srcChanges.HasChanges && len(srcChanges.Changes) < delta.MaxChangesBeforeFallback {
			scopedCtx := applyScopeFilter(ctx, srcChanges.Changes)
			if scopedCtx != ctx {
				ctx = scopedCtx
				usedDelta = true
				drainedChanges = srcChanges.Changes
				log.Printf("[delta] Scoped sync to %d changed files", len(srcChanges.Changes))
			}
		}
	}

	syncErr := utils.RunRcloneWithRetryAndStats(ctx, true, false, outStatus, func() error {
		return utils.HandleError(fssync.Sync(ctx, dstFs, srcFs, false), "Sync failed", nil, nil)
	})

	// Commit delta state after sync
	if deltaSvc != nil {
		if syncErr == nil {
			if usedDelta {
				_ = deltaSvc.CommitDelta(srcKey)
				_ = deltaSvc.CommitDelta(dstKey)
			} else {
				// Full sync completed — establish baseline and start watchers
				_ = deltaSvc.CommitFullSync(srcFs, srcKey)
				_ = deltaSvc.CommitFullSync(dstFs, dstKey)
			}
		} else if usedDelta && len(drainedChanges) > 0 {
			// Scoped delta sync failed — restore drained changes so they're
			// not lost and will be picked up on the next sync attempt.
			deltaSvc.RestoreChanges(srcKey, drainedChanges)
			log.Printf("[delta] Restored %d drained changes after sync failure", len(drainedChanges))
		}
		// On error without delta: watcher continues collecting changes.
		// Next sync will get a fresh changeset or fall back to full sync.
	}

	return syncErr
}

// remoteKey extracts a stable key from a remote path.
// "gdrive:/data" → "gdrive:/data", "/local/path" → "local:/local/path"
func remoteKey(path string) string {
	if idx := strings.Index(path, ":"); idx != -1 {
		return path
	}
	return "local:" + path
}

// applyScopeFilter injects include rules for changed paths and excludes everything else.
func applyScopeFilter(ctx context.Context, changes []delta.FileChange) context.Context {
	filterOpt := CopyFilterOpt(ctx)

	seen := make(map[string]bool)
	for _, c := range changes {
		if seen[c.Path] {
			continue
		}
		seen[c.Path] = true

		if c.EntryType == fs.EntryDirectory {
			filterOpt.IncludeRule = append(filterOpt.IncludeRule, "/"+c.Path+"/**")
		}
		filterOpt.IncludeRule = append(filterOpt.IncludeRule, "/"+c.Path)
	}

	// Exclude everything not in the changeset
	filterOpt.ExcludeRule = append(filterOpt.ExcludeRule, "**")

	newFilter, err := filter.NewFilter(&filterOpt)
	if err != nil {
		log.Printf("[delta] Failed to build scope filter, using full sync: %v", err)
		return ctx
	}
	return filter.ReplaceConfig(ctx, newFilter)
}

// sendSkippedStatus sends a status indicating the sync was skipped (no changes).
func sendSkippedStatus(outStatus chan *dto.SyncStatusDTO) {
	if outStatus == nil {
		return
	}
	status := &dto.SyncStatusDTO{
		Command:      "sync_status",
		Status:       "completed",
		Progress:     100,
		Speed:        "0 B/s",
		ETA:          "-",
		Timestamp:    time.Now(),
		DeltaMode:    true,
		DeltaSkipped: true,
	}
	select {
	case outStatus <- status:
	default:
	}
}
