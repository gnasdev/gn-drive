package rclone

import (
	"context"
	"desktop/backend/models"
	"desktop/backend/utils"
	"fmt"
	"os"
	"runtime/pprof"
	"sync"

	"github.com/rclone/rclone/fs"
	"github.com/rclone/rclone/fs/accounting"
	fscache "github.com/rclone/rclone/fs/cache"
	"github.com/rclone/rclone/fs/config/configfile"
	"github.com/rclone/rclone/fs/filter"
	fslog "github.com/rclone/rclone/fs/log"
	"github.com/rclone/rclone/fs/rc"
	"github.com/rclone/rclone/fs/rc/rcserver"
	"github.com/rclone/rclone/lib/atexit"
	"github.com/rclone/rclone/lib/terminal"
)

var (
	initOnce      sync.Once
	initErr       error
	initDebugMode bool
)

// InitGlobal performs one-time global rclone initialization.
// It is safe to call multiple times; only the first call takes effect.
func InitGlobal(isDebugMode bool) error {
	initOnce.Do(func() {
		initDebugMode = isDebugMode
		initErr = doInitGlobal(isDebugMode)
	})
	return initErr
}

func doInitGlobal(isDebugMode bool) error {
	// Set the global options from the flags
	err := fs.GlobalOptionsInit()
	if utils.HandleError(err, "Failed to initialise global options", nil, nil) != nil {
		return err
	}

	// Start the logger
	fslog.InitLogging()

	// Load the config
	configfile.Install()

	// Start accounting
	accounting.Start(context.Background())

	// Skip Google Docs/Sheets/Slides — they are cloud-native and cannot be
	// synced as regular files without explicit import/export format conversion.
	os.Setenv("RCLONE_DRIVE_SKIP_GDOCS", "true")

	// Initialize the global config (baseline that fs.AddConfig will copy from)
	fsConfig := fs.GetConfig(context.Background())
	fsConfig.UseServerModTime = true
	fsConfig.UseListR = true

	// Fix case
	fsConfig.NoUnicodeNormalization = false
	fsConfig.IgnoreCaseSync = true

	// Configure console
	if fsConfig.NoConsole {
		terminal.HideConsole()
	} else {
		terminal.EnableColorsStdout()
	}

	// Write the args for debug purposes
	fs.Debugf("rclone", "Version %q starting with parameters %q", fs.Version, os.Args)

	// Inform user about systemd log support now that we have a logger
	if fslog.Opt.LogSystemdSupport {
		fs.Debugf("rclone", "systemd logging support activated")
	}

	// Start the metrics server if configured
	_, err = rcserver.MetricsStart(context.Background(), &rc.Opt)
	if utils.HandleError(err, "Failed to start metrics server", nil, nil) != nil {
		return err
	}

	// Log level
	logLevel := fs.LogLevelInfo
	if isDebugMode {
		logLevel = fs.LogLevelDebug
	}
	err = fsConfig.StatsLogLevel.Set(logLevel.String())
	if utils.HandleError(err, "Failed to set stats log level", nil, nil) != nil {
		return err
	}
	err = fsConfig.LogLevel.Set(logLevel.String())
	if utils.HandleError(err, "Failed to set log level", nil, nil) != nil {
		return err
	}

	// Setup CPU profiling if desired
	cpuProfile := ""
	if cpuProfile != "" {
		fs.Infof(nil, "Creating CPU profile %q\n", cpuProfile)
		f, err := os.Create(cpuProfile)
		if utils.HandleError(err, "", nil, nil) != nil {
			err = fs.CountError(context.Background(), err)
			return err
		}
		err = pprof.StartCPUProfile(f)
		if utils.HandleError(err, "", nil, nil) != nil {
			err = fs.CountError(context.Background(), err)
			return err
		}
		atexit.Register(func() {
			pprof.StopCPUProfile()
			err := f.Close()
			if utils.HandleError(err, "", nil, nil) != nil {
				_ = fs.CountError(context.Background(), err)
			}
		})
	}

	// Setup memory profiling if desired
	memProfile := ""
	if memProfile != "" {
		atexit.Register(func() {
			fs.Infof(nil, "Saving Memory profile %q\n", memProfile)
			f, err := os.Create(memProfile)
			if utils.HandleError(err, "", nil, nil) != nil {
				_ = fs.CountError(context.Background(), err)
			}
			err = pprof.WriteHeapProfile(f)
			if utils.HandleError(err, "", nil, nil) != nil {
				_ = fs.CountError(context.Background(), err)
			}
			err = f.Close()
			if utils.HandleError(err, "", nil, nil) != nil {
				_ = fs.CountError(context.Background(), err)
			}
		})
	}

	return nil
}

// ClearFsCache clears the global rclone filesystem cache.
// Call before sync operations to ensure fresh filesystem objects.
func ClearFsCache() {
	fscache.Clear()
}

// ClearStatsCache resets the global rclone accounting stats.
// Call before sync operations to ensure no stale status data carries over.
func ClearStatsCache() {
	accounting.GlobalStats().ResetCounters()
	accounting.GlobalStats().ResetErrors()
}

// newDefaultFilterOpts returns fresh default filter options for per-task isolation.
func newDefaultFilterOpts() filter.Options {
	return filter.Options{
		MinAge:  fs.DurationOff,
		MaxAge:  fs.DurationOff,
		MinSize: fs.SizeSuffix(-1),
		MaxSize: fs.SizeSuffix(-1),
	}
}

// NewTaskContext creates an isolated rclone context for a sync/operation task.
// Each task gets its own config copy, stats group, and fresh filter config.
func NewTaskContext(parentCtx context.Context, taskId int) (context.Context, error) {
	if err := InitGlobal(initDebugMode); err != nil {
		return nil, err
	}

	// 1. Isolated config copy (shallow copy of global config stored in context)
	ctx, _ := fs.AddConfig(parentCtx)

	// 2. Isolated stats group
	ctx = accounting.WithStatsGroup(ctx, fmt.Sprintf("task-%d", taskId))
	stats := accounting.Stats(ctx)
	stats.ResetCounters()
	stats.ResetErrors()

	// 3. Fresh default filter (no shared slices)
	filterOpts := newDefaultFilterOpts()
	f, err := filter.NewFilter(&filterOpts)
	if err != nil {
		return nil, fmt.Errorf("failed to create default filter: %w", err)
	}
	ctx = filter.ReplaceConfig(ctx, f)

	return ctx, nil
}

// SimpleContext creates an isolated rclone context for lightweight operations
// (ListFiles, Mkdir, etc.) that don't need stats isolation.
func SimpleContext(parentCtx context.Context) (context.Context, error) {
	if err := InitGlobal(initDebugMode); err != nil {
		return nil, err
	}

	// Isolated config copy
	ctx, _ := fs.AddConfig(parentCtx)

	// Fresh default filter
	filterOpts := newDefaultFilterOpts()
	f, err := filter.NewFilter(&filterOpts)
	if err != nil {
		return nil, fmt.Errorf("failed to create default filter: %w", err)
	}
	ctx = filter.ReplaceConfig(ctx, f)

	return ctx, nil
}

// CopyFilterOpt returns a deep copy of the current filter options from context,
// safe to mutate without affecting other concurrent operations.
func CopyFilterOpt(ctx context.Context) filter.Options {
	existing := filter.GetConfig(ctx)
	opt := existing.Opt // struct value copy
	// Deep-copy slices to break aliasing with the original backing arrays
	opt.IncludeRule = append([]string(nil), opt.IncludeRule...)
	opt.ExcludeRule = append([]string(nil), opt.ExcludeRule...)
	opt.FilterFrom = append([]string(nil), opt.FilterFrom...)
	opt.ExcludeFile = append([]string(nil), opt.ExcludeFile...)
	return opt
}

// ApplyProfileOptions maps Profile fields to rclone's fs.ConfigInfo and filter.Options.
// It applies filtering, safety, and performance settings from the profile to the context.
// Returns the updated context with new filter configuration.
func ApplyProfileOptions(ctx context.Context, profile models.Profile) (context.Context, error) {
	fsConfig := fs.GetConfig(ctx)
	filterOpt := CopyFilterOpt(ctx)

	// Filtering: min/max size
	if profile.MinSize != "" {
		if err := filterOpt.MinSize.Set(profile.MinSize); err != nil {
			return ctx, fmt.Errorf("invalid min_size %q: %w", profile.MinSize, err)
		}
	}
	if profile.MaxSize != "" {
		if err := filterOpt.MaxSize.Set(profile.MaxSize); err != nil {
			return ctx, fmt.Errorf("invalid max_size %q: %w", profile.MaxSize, err)
		}
	}

	// Filtering: filter-from file
	if profile.FilterFromFile != "" {
		filterOpt.FilterFrom = append(filterOpt.FilterFrom, profile.FilterFromFile)
	}

	// Filtering: exclude-if-present
	if profile.ExcludeIfPresent != "" {
		filterOpt.ExcludeFile = append(filterOpt.ExcludeFile, profile.ExcludeIfPresent)
	}

	// Filtering: max/min age
	if profile.MaxAge != "" {
		if err := filterOpt.MaxAge.Set(profile.MaxAge); err != nil {
			return ctx, fmt.Errorf("invalid max_age %q: %w", profile.MaxAge, err)
		}
	}
	if profile.MinAge != "" {
		if err := filterOpt.MinAge.Set(profile.MinAge); err != nil {
			return ctx, fmt.Errorf("invalid min_age %q: %w", profile.MinAge, err)
		}
	}

	// Filtering: delete excluded files on destination
	if profile.DeleteExcluded {
		filterOpt.DeleteExcluded = true
	}

	// Rebuild filter with updated options
	newFilter, err := filter.NewFilter(&filterOpt)
	if err != nil {
		return ctx, fmt.Errorf("failed to create filter: %w", err)
	}
	ctx = filter.ReplaceConfig(ctx, newFilter)

	// Filtering: max depth
	if profile.MaxDepth != nil {
		fsConfig.MaxDepth = *profile.MaxDepth
	}

	// Safety: backup directory
	if profile.BackupPath != "" {
		fsConfig.BackupDir = profile.BackupPath
	}

	// Safety: max delete limit
	if profile.MaxDelete != nil {
		fsConfig.MaxDelete = int64(*profile.MaxDelete)
	}

	// Safety: immutable mode
	if profile.Immutable {
		fsConfig.Immutable = true
	}

	// Safety: dry run
	if profile.DryRun {
		fsConfig.DryRun = true
	}

	// Safety: max transfer
	if profile.MaxTransfer != "" {
		if err := fsConfig.MaxTransfer.Set(profile.MaxTransfer); err != nil {
			return ctx, fmt.Errorf("invalid max_transfer %q: %w", profile.MaxTransfer, err)
		}
	}

	// Safety: max delete size
	if profile.MaxDeleteSize != "" {
		if err := fsConfig.MaxDeleteSize.Set(profile.MaxDeleteSize); err != nil {
			return ctx, fmt.Errorf("invalid max_delete_size %q: %w", profile.MaxDeleteSize, err)
		}
	}

	// Safety: suffix for changed files
	if profile.Suffix != "" {
		fsConfig.Suffix = profile.Suffix
	}
	if profile.SuffixKeepExtension {
		fsConfig.SuffixKeepExtension = true
	}

	// Performance: multi-thread streams
	if profile.MultiThreadStreams != nil {
		fsConfig.MultiThreadStreams = *profile.MultiThreadStreams
	}

	// Performance: buffer size
	if profile.BufferSize != "" {
		if err := fsConfig.BufferSize.Set(profile.BufferSize); err != nil {
			return ctx, fmt.Errorf("invalid buffer_size %q: %w", profile.BufferSize, err)
		}
	}

	// Performance: retries
	if profile.Retries != nil {
		fsConfig.Retries = *profile.Retries
	}
	if profile.LowLevelRetries != nil {
		fsConfig.LowLevelRetries = *profile.LowLevelRetries
	}

	// Performance: max duration
	if profile.MaxDuration != "" {
		var maxDuration fs.Duration
		if err := maxDuration.Set(profile.MaxDuration); err != nil {
			return ctx, fmt.Errorf("invalid max_duration %q: %w", profile.MaxDuration, err)
		}
		fsConfig.MaxDuration = maxDuration
	}

	// Performance: check first
	if profile.CheckFirst {
		fsConfig.CheckFirst = true
	}

	// Performance: order by
	if profile.OrderBy != "" {
		fsConfig.OrderBy = profile.OrderBy
	}

	// Performance: retries sleep
	if profile.RetriesSleep != "" {
		var d fs.Duration
		if err := d.Set(profile.RetriesSleep); err != nil {
			return ctx, fmt.Errorf("invalid retries_sleep %q: %w", profile.RetriesSleep, err)
		}
		fsConfig.RetriesInterval = d
	}

	// Performance: TPS limit
	if profile.TpsLimit != nil {
		fsConfig.TPSLimit = *profile.TpsLimit
	}

	// Performance: connect timeout
	if profile.ConnTimeout != "" {
		var d fs.Duration
		if err := d.Set(profile.ConnTimeout); err != nil {
			return ctx, fmt.Errorf("invalid conn_timeout %q: %w", profile.ConnTimeout, err)
		}
		fsConfig.ConnectTimeout = d
	}

	// Performance: IO timeout
	if profile.IoTimeout != "" {
		var d fs.Duration
		if err := d.Set(profile.IoTimeout); err != nil {
			return ctx, fmt.Errorf("invalid io_timeout %q: %w", profile.IoTimeout, err)
		}
		fsConfig.Timeout = d
	}

	// Comparison: size only
	if profile.SizeOnly {
		fsConfig.SizeOnly = true
	}

	// Comparison: update mode (skip newer destination files)
	if profile.UpdateMode {
		fsConfig.UpdateOlder = true
	}

	// Comparison: ignore existing
	if profile.IgnoreExisting {
		fsConfig.IgnoreExisting = true
	}

	// Sync-specific: delete timing
	if profile.DeleteTiming != "" {
		switch profile.DeleteTiming {
		case "before":
			fsConfig.DeleteMode = fs.DeleteModeBefore
		case "during":
			fsConfig.DeleteMode = fs.DeleteModeDuring
		case "after":
			fsConfig.DeleteMode = fs.DeleteModeAfter
		}
	}

	return ctx, nil
}
