package rclone

import (
	"context"
	"desktop/backend/models"
	"desktop/backend/utils"
	"fmt"
	"os"
	"runtime/pprof"

	"github.com/rclone/rclone/fs"
	"github.com/rclone/rclone/fs/accounting"
	"github.com/rclone/rclone/fs/config/configfile"
	"github.com/rclone/rclone/fs/filter"
	fslog "github.com/rclone/rclone/fs/log"
	"github.com/rclone/rclone/fs/rc"
	"github.com/rclone/rclone/fs/rc/rcserver"
	"github.com/rclone/rclone/lib/atexit"
	"github.com/rclone/rclone/lib/terminal"
)

func InitConfig(ctx context.Context, isDebugMode bool) (context.Context, error) {
	// Set the global options from the flags
	err := fs.GlobalOptionsInit()

	if utils.HandleError(err, "Failed to initialise global options", nil, nil) != nil {
		return nil, err
	}

	// Start the logger
	fslog.InitLogging()

	// Load the config
	// config.SetConfigPath(filepath.Join(dir, "rclone.conf"))
	configfile.Install()

	// Start accounting and ensure clean state
	stats := accounting.Stats(ctx)
	stats.ResetCounters()
	stats.ResetErrors()
	accounting.Start(ctx)

	// Initialize the global config
	fsConfig := fs.GetConfig(ctx)
	fsConfig.CheckSum = true
	fsConfig.Progress = true
	fsConfig.TrackRenames = true
	fsConfig.Metadata = true
	fsConfig.UseServerModTime = true

	// Fix case
	fsConfig.NoUnicodeNormalization = false
	fsConfig.IgnoreCaseSync = true
	fsConfig.FixCase = true

	// Configure console
	if fsConfig.NoConsole {
		// Hide the console window
		terminal.HideConsole()
	} else {
		// Enable color support on stdout if possible.
		// This enables virtual terminal processing on Windows 10,
		// adding native support for ANSI/VT100 escape sequences.
		terminal.EnableColorsStdout()
	}

	// Write the args for debug purposes
	fs.Debugf("rclone", "Version %q starting with parameters %q", fs.Version, os.Args)

	// Inform user about systemd log support now that we have a logger
	if fslog.Opt.LogSystemdSupport {
		fs.Debugf("rclone", "systemd logging support activated")
	}

	// Start the metrics server if configured
	_, err = rcserver.MetricsStart(ctx, &rc.Opt)
	if utils.HandleError(err, "Failed to start metrics server", nil, nil) != nil {
		return nil, err
	}

	// Log level
	logLevel := fs.LogLevelInfo
	if isDebugMode {
		logLevel = fs.LogLevelDebug
	}
	err = fsConfig.StatsLogLevel.Set(logLevel.String()) // EMERGENCY ALERT CRITICAL ERROR WARNING NOTICE INFO DEBUG
	if utils.HandleError(err, "Failed to set stats log level", nil, nil) != nil {
		return nil, err
	}
	err = fsConfig.LogLevel.Set(logLevel.String())
	if utils.HandleError(err, "Failed to set log level", nil, nil) != nil {
		return nil, err
	}

	// Setup the default filters
	filterOpts := filter.Options{
		MinAge:  fs.DurationOff, // These have to be set here as the options are parsed once before the defaults are set
		MaxAge:  fs.DurationOff,
		MinSize: fs.SizeSuffix(-1),
		MaxSize: fs.SizeSuffix(-1),
	}
	filterConfig, err := filter.NewFilter(&filterOpts)
	if utils.HandleError(err, "Failed to load filter file", nil, nil) != nil {
		return nil, err
	}
	ctx = filter.ReplaceConfig(ctx, filterConfig)

	// Setup CPU profiling if desired
	cpuProfile := ""
	// cpuProfile := "Debugging"
	if cpuProfile != "" {
		fs.Infof(nil, "Creating CPU profile %q\n", cpuProfile)
		f, err := os.Create(cpuProfile)
		if utils.HandleError(err, "", nil, nil) != nil {
			err = fs.CountError(ctx, err)
			return nil, err
		}
		err = pprof.StartCPUProfile(f)
		if utils.HandleError(err, "", nil, nil) != nil {
			err = fs.CountError(ctx, err)
			return nil, err
		}
		atexit.Register(func() {
			pprof.StopCPUProfile()
			err := f.Close()
			if utils.HandleError(err, "", nil, nil) != nil {
				_ = fs.CountError(ctx, err)
			}
		})
	}

	// Setup memory profiling if desired
	memProfile := ""
	// memProfile := "Debugging"
	if memProfile != "" {
		atexit.Register(func() {
			fs.Infof(nil, "Saving Memory profile %q\n", memProfile)
			f, err := os.Create(memProfile)
			if utils.HandleError(err, "", nil, nil) != nil {
				_ = fs.CountError(ctx, err)
			}
			err = pprof.WriteHeapProfile(f)
			if utils.HandleError(err, "", nil, nil) != nil {
				_ = fs.CountError(ctx, err)
			}
			err = f.Close()
			if utils.HandleError(err, "", nil, nil) != nil {
				_ = fs.CountError(ctx, err)
			}
		})
	}

	return ctx, nil
}

// ApplyProfileOptions maps Profile fields to rclone's fs.ConfigInfo and filter.Options.
// It applies filtering, safety, and performance settings from the profile to the context.
// Returns the updated context with new filter configuration.
func ApplyProfileOptions(ctx context.Context, profile models.Profile) (context.Context, error) {
	fsConfig := fs.GetConfig(ctx)
	filterOpt := filter.GetConfig(ctx).Opt

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

	// Rebuild filter with updated options
	newFilter, err := filter.NewFilter(&filterOpt)
	if err != nil {
		return ctx, fmt.Errorf("failed to create filter: %w", err)
	}
	ctx = filter.ReplaceConfig(ctx, newFilter)

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

	// Performance: fast list (recursive listing)
	if profile.FastList {
		fsConfig.UseListR = true
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

	return ctx, nil
}
