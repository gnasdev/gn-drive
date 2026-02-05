package utils

import (
	"context"
	"desktop/backend/dto"
	"fmt"
	"log/slog"
	"strings"
	"sync"
	"time"

	"github.com/rclone/rclone/fs/accounting"
	"github.com/rclone/rclone/fs/operations"
)

const (
	syncStatusInterval = 1 * time.Second // Send sync status every second
)

// SyncStatusHandler handles sync status updates
type SyncStatusHandler struct {
	originalHandler     slog.Handler
	statusChannel       chan []byte
	isStatusClosed      *bool
	statusClosedRecover func()
	taskId              int
	action              string
	tabId               string
	startTime           time.Time
}

func (h *SyncStatusHandler) Handle(ctx context.Context, r slog.Record) error {
	// Also call the original handler
	return h.originalHandler.Handle(ctx, r)
}

func (h *SyncStatusHandler) WithAttrs(attrs []slog.Attr) slog.Handler {
	return &SyncStatusHandler{
		originalHandler:     h.originalHandler.WithAttrs(attrs),
		statusChannel:       h.statusChannel,
		isStatusClosed:      h.isStatusClosed,
		statusClosedRecover: h.statusClosedRecover,
		taskId:              h.taskId,
		action:              h.action,
		tabId:               h.tabId,
		startTime:           h.startTime,
	}
}

func (h *SyncStatusHandler) WithGroup(name string) slog.Handler {
	return &SyncStatusHandler{
		originalHandler:     h.originalHandler.WithGroup(name),
		statusChannel:       h.statusChannel,
		isStatusClosed:      h.isStatusClosed,
		statusClosedRecover: h.statusClosedRecover,
		taskId:              h.taskId,
		action:              h.action,
		tabId:               h.tabId,
		startTime:           h.startTime,
	}
}

func (h *SyncStatusHandler) Enabled(ctx context.Context, level slog.Level) bool {
	return h.originalHandler.Enabled(ctx, level)
}

// createSyncStatusFromStats creates a SyncStatusDTO from rclone accounting stats
func (h *SyncStatusHandler) createSyncStatusFromStats() *dto.SyncStatusDTO {
	stats := accounting.GlobalStats()

	var syncStatus *dto.SyncStatusDTO
	if h.tabId != "" {
		syncStatus = dto.NewSyncStatusDTOWithTab(h.taskId, h.action, h.tabId)
	} else {
		syncStatus = dto.NewSyncStatusDTO(h.taskId, h.action)
	}

	// Get basic stats
	syncStatus.FilesTransferred = stats.GetTransfers()
	syncStatus.BytesTransferred = stats.GetBytes()
	syncStatus.Errors = int(stats.GetErrors())
	syncStatus.Checks = stats.GetChecks()
	syncStatus.Deletes = stats.GetDeletes()
	// Note: GetRenames() might not be available in all rclone versions
	// syncStatus.Renames = stats.GetRenames()
	syncStatus.Renames = 0

	// Calculate elapsed time
	elapsed := time.Since(h.startTime)
	syncStatus.ElapsedTime = formatDuration(elapsed)

	// Get speed and ETA
	if elapsed > 0 {
		bytesPerSecond := float64(syncStatus.BytesTransferred) / elapsed.Seconds()
		syncStatus.Speed = formatSpeed(bytesPerSecond)

		// Calculate ETA if we have total bytes
		if syncStatus.TotalBytes > 0 && bytesPerSecond > 0 {
			remainingBytes := syncStatus.TotalBytes - syncStatus.BytesTransferred
			etaSeconds := float64(remainingBytes) / bytesPerSecond
			syncStatus.ETA = formatDuration(time.Duration(etaSeconds) * time.Second)
		}
	}

	// Calculate progress percentage
	if syncStatus.TotalBytes > 0 {
		syncStatus.Progress = float64(syncStatus.BytesTransferred) / float64(syncStatus.TotalBytes) * 100
	} else if syncStatus.TotalFiles > 0 {
		syncStatus.Progress = float64(syncStatus.FilesTransferred) / float64(syncStatus.TotalFiles) * 100
	}

	// Determine status
	if stats.GetErrors() > 0 {
		syncStatus.Status = "error"
	} else {
		// Check if sync is completed by looking at progress
		if syncStatus.Progress >= 100.0 {
			syncStatus.Status = "completed"
		} else {
			syncStatus.Status = "running"
		}
	}

	return syncStatus
}

// formatSpeed formats bytes per second to human readable format
func formatSpeed(bytesPerSecond float64) string {
	if bytesPerSecond < 1024 {
		return fmt.Sprintf("%.1f B/s", bytesPerSecond)
	} else if bytesPerSecond < 1024*1024 {
		return fmt.Sprintf("%.1f KB/s", bytesPerSecond/1024)
	} else if bytesPerSecond < 1024*1024*1024 {
		return fmt.Sprintf("%.1f MB/s", bytesPerSecond/(1024*1024))
	} else {
		return fmt.Sprintf("%.1f GB/s", bytesPerSecond/(1024*1024*1024))
	}
}

// formatDuration formats duration to human readable format
func formatDuration(d time.Duration) string {
	if d < time.Minute {
		return fmt.Sprintf("%ds", int(d.Seconds()))
	} else if d < time.Hour {
		return fmt.Sprintf("%dm%ds", int(d.Minutes()), int(d.Seconds())%60)
	} else {
		return fmt.Sprintf("%dh%dm", int(d.Hours()), int(d.Minutes())%60)
	}
}

// StartSyncStatusReporting starts sending sync status updates
func StartSyncStatusReporting(statusChannel chan []byte, taskId int, action string, tabId string) func() {
	isStatusClosed := false
	statusClosedRecover := func() {
		if r := recover(); r != nil {
			isStatusClosed = true
		}
	}

	// Create custom handler for sync status
	originalLogger := slog.Default()
	statusHandler := &SyncStatusHandler{
		originalHandler:     originalLogger.Handler(),
		statusChannel:       statusChannel,
		isStatusClosed:      &isStatusClosed,
		statusClosedRecover: statusClosedRecover,
		taskId:              taskId,
		action:              action,
		tabId:               tabId,
		startTime:           time.Now(),
	}

	// Set the custom logger
	customLogger := slog.New(statusHandler)
	slog.SetDefault(customLogger)

	// Intercept output from functions such as HashLister to stdout
	// Throttle to avoid flooding the channel - only send at most once per 500ms
	oldSyncPrint := operations.SyncPrintf
	var lastSyncPrint time.Time
	var syncPrintMu sync.Mutex
	operations.SyncPrintf = func(format string, a ...interface{}) {
		defer statusClosedRecover()
		if !isStatusClosed && statusChannel != nil {
			syncPrintMu.Lock()
			now := time.Now()
			if now.Sub(lastSyncPrint) < 500*time.Millisecond {
				syncPrintMu.Unlock()
				return
			}
			lastSyncPrint = now
			syncPrintMu.Unlock()

			logMsg := fmt.Sprintf(format, a...)
			statusChannel <- []byte(formatToProgressCompat(logMsg))
		}
	}

	stopStats := make(chan struct{})
	var wg sync.WaitGroup
	wg.Add(1)

	go func() {
		defer statusClosedRecover()
		defer wg.Done()
		ticker := time.NewTicker(syncStatusInterval)
		defer ticker.Stop()

		for {
			select {
			case <-ticker.C:
				if !isStatusClosed && statusChannel != nil {
					syncStatus := statusHandler.createSyncStatusFromStats()
					if jsonData, err := syncStatus.ToJSON(); err == nil {
						statusChannel <- jsonData
					}
				}
			case <-stopStats:
				// Restore the original logger
				slog.SetDefault(originalLogger)
				operations.SyncPrintf = oldSyncPrint
				return
			}
		}
	}()

	return func() {
		defer statusClosedRecover()
		close(stopStats)
		wg.Wait()
	}
}

// formatToProgressCompat prints the progress with an optional log (compatibility function)
func formatToProgressCompat(logMessage string) string {
	operations.StdoutMutex.Lock()
	defer operations.StdoutMutex.Unlock()

	stats := strings.TrimSpace(accounting.GlobalStats().String())
	logMessage = strings.TrimSpace(logMessage)

	var result strings.Builder
	if logMessage != "" {
		result.WriteString(logMessage + "\n")
	}

	if stats != "" {
		result.WriteString(stats)
	}

	return result.String()
}
