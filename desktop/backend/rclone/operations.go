package rclone

import (
	"context"
	"desktop/backend/dto"
	"fmt"

	beConfig "desktop/backend/config"
	"desktop/backend/models"
	"desktop/backend/utils"

	"github.com/rclone/rclone/fs"
	"github.com/rclone/rclone/fs/filter"
	"github.com/rclone/rclone/fs/operations"
	fssync "github.com/rclone/rclone/fs/sync"
)

// Copy performs a one-way copy from source to destination (no deleting destination files).
func Copy(ctx context.Context, config beConfig.Config, profile models.Profile, outStatus chan *dto.SyncStatusDTO) error {
	fsConfig := fs.GetConfig(ctx)
	fsConfig.Transfers = profile.Parallel
	fsConfig.Checkers = profile.Parallel * 2

	srcFs, err := fs.NewFs(ctx, profile.From)
	if utils.HandleError(err, "Failed to initialize source filesystem", nil, nil) != nil {
		return err
	}

	dstFs, err := fs.NewFs(ctx, profile.To)
	if utils.HandleError(err, "Failed to initialize destination filesystem", nil, nil) != nil {
		return err
	}

	ctx = applyFiltersAndBandwidth(ctx, fsConfig, profile)

	ctx, err = ApplyProfileOptions(ctx, profile)
	if err != nil {
		return fmt.Errorf("failed to apply profile options: %w", err)
	}

	if err := fsConfig.Reload(ctx); err != nil {
		return err
	}

	return utils.RunRcloneWithRetryAndStats(ctx, true, false, outStatus, func() error {
		return utils.HandleError(fssync.CopyDir(ctx, dstFs, srcFs, false), "Copy failed", nil, nil)
	})
}

// Move performs a copy then deletes files from the source.
func Move(ctx context.Context, config beConfig.Config, profile models.Profile, outStatus chan *dto.SyncStatusDTO) error {
	fsConfig := fs.GetConfig(ctx)
	fsConfig.Transfers = profile.Parallel
	fsConfig.Checkers = profile.Parallel * 2

	srcFs, err := fs.NewFs(ctx, profile.From)
	if utils.HandleError(err, "Failed to initialize source filesystem", nil, nil) != nil {
		return err
	}

	dstFs, err := fs.NewFs(ctx, profile.To)
	if utils.HandleError(err, "Failed to initialize destination filesystem", nil, nil) != nil {
		return err
	}

	ctx = applyFiltersAndBandwidth(ctx, fsConfig, profile)

	ctx, err = ApplyProfileOptions(ctx, profile)
	if err != nil {
		return fmt.Errorf("failed to apply profile options: %w", err)
	}

	if err := fsConfig.Reload(ctx); err != nil {
		return err
	}

	return utils.RunRcloneWithRetryAndStats(ctx, true, false, outStatus, func() error {
		return utils.HandleError(fssync.MoveDir(ctx, dstFs, srcFs, false, false), "Move failed", nil, nil)
	})
}

// Check compares source and destination and reports differences.
func Check(ctx context.Context, config beConfig.Config, profile models.Profile, outStatus chan *dto.SyncStatusDTO) error {
	fsConfig := fs.GetConfig(ctx)
	fsConfig.Checkers = profile.Parallel

	srcFs, err := fs.NewFs(ctx, profile.From)
	if utils.HandleError(err, "Failed to initialize source filesystem", nil, nil) != nil {
		return err
	}

	dstFs, err := fs.NewFs(ctx, profile.To)
	if utils.HandleError(err, "Failed to initialize destination filesystem", nil, nil) != nil {
		return err
	}

	ctx = applyFiltersAndBandwidth(ctx, fsConfig, profile)

	ctx, err = ApplyProfileOptions(ctx, profile)
	if err != nil {
		return fmt.Errorf("failed to apply profile options: %w", err)
	}

	if err := fsConfig.Reload(ctx); err != nil {
		return err
	}

	return utils.RunRcloneWithRetryAndStats(ctx, false, false, outStatus, func() error {
		return utils.HandleError(operations.Check(ctx, &operations.CheckOpt{
			Fsrc: srcFs,
			Fdst: dstFs,
		}), "Check failed", nil, nil)
	})
}

// ListFiles lists files at the given remote path and returns FileEntry items.
// Returns an empty slice (not an error) when the path is invalid or listing fails.
func ListFiles(ctx context.Context, remotePath string, recursive bool) ([]models.FileEntry, error) {
	remoteFs, err := fs.NewFs(ctx, remotePath)
	if err != nil {
		return nil, fmt.Errorf("failed to access %s: %w", remotePath, err)
	}

	var entries []models.FileEntry

	opt := operations.ListJSONOpt{
		NoModTime:  false,
		NoMimeType: false,
		Recurse:    recursive,
	}

	err = operations.ListJSON(ctx, remoteFs, "", &opt, func(item *operations.ListJSONItem) error {
		entries = append(entries, models.FileEntry{
			Path:     item.Path,
			Name:     item.Name,
			Size:     item.Size,
			ModTime:  item.ModTime.When.Format("2006-01-02T15:04:05Z"),
			IsDir:    item.IsDir,
			MimeType: item.MimeType,
		})
		return nil
	})
	if err != nil {
		return nil, fmt.Errorf("failed to list files in %s: %w", remotePath, err)
	}

	return entries, nil
}

// DeleteFile deletes a single file at the given remote path.
func DeleteFile(ctx context.Context, remotePath string) error {
	remoteFs, err := fs.NewFs(ctx, remotePath)
	if err != nil {
		return fmt.Errorf("failed to initialize filesystem %q: %w", remotePath, err)
	}

	return operations.Delete(ctx, remoteFs)
}

// Purge removes the path and all of its contents.
func Purge(ctx context.Context, remotePath string) error {
	remoteFs, err := fs.NewFs(ctx, remotePath)
	if err != nil {
		return fmt.Errorf("failed to initialize filesystem %q: %w", remotePath, err)
	}

	return operations.Purge(ctx, remoteFs, "")
}

// Mkdir creates the directory at the given remote path.
func Mkdir(ctx context.Context, remotePath string) error {
	remoteFs, err := fs.NewFs(ctx, remotePath)
	if err != nil {
		return fmt.Errorf("failed to initialize filesystem %q: %w", remotePath, err)
	}

	return operations.Mkdir(ctx, remoteFs, "")
}

// About returns quota information for the given remote.
func About(ctx context.Context, remoteName string) (*models.QuotaInfo, error) {
	remoteFs, err := fs.NewFs(ctx, remoteName+":")
	if err != nil {
		return nil, fmt.Errorf("failed to initialize filesystem %q: %w", remoteName, err)
	}

	usage, err := remoteFs.Features().About(ctx)
	if err != nil {
		return nil, fmt.Errorf("about not supported or failed: %w", err)
	}

	qi := &models.QuotaInfo{}
	if usage.Total != nil {
		qi.Total = *usage.Total
	}
	if usage.Used != nil {
		qi.Used = *usage.Used
	}
	if usage.Free != nil {
		qi.Free = *usage.Free
	}
	if usage.Trashed != nil {
		qi.Trashed = *usage.Trashed
	}

	return qi, nil
}

// GetSize returns the total number of objects and their size at the given remote path.
func GetSize(ctx context.Context, remotePath string) (int64, int64, error) {
	remoteFs, err := fs.NewFs(ctx, remotePath)
	if err != nil {
		return 0, 0, fmt.Errorf("failed to initialize filesystem %q: %w", remotePath, err)
	}

	objects, size, _, err := operations.Count(ctx, remoteFs)
	if err != nil {
		return 0, 0, fmt.Errorf("failed to count: %w", err)
	}

	return objects, size, nil
}

// applyFiltersAndBandwidth sets up filter rules and bandwidth from profile.
// Returns the updated context.
func applyFiltersAndBandwidth(ctx context.Context, fsConfig *fs.ConfigInfo, profile models.Profile) context.Context {
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
	if err == nil {
		ctx = filter.ReplaceConfig(ctx, newFilter)
	}

	// Set bandwidth limit
	if profile.Bandwidth > 0 {
		_ = fsConfig.BwLimit.Set(fmt.Sprint(profile.Bandwidth) + "M")
	}

	return ctx
}
