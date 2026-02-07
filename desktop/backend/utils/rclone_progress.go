package utils

import (
	"context"
	"desktop/backend/dto"
	"fmt"
	"log/slog"
	"strings"
	"sync"
	"sync/atomic"
	"time"

	"github.com/rclone/rclone/fs/accounting"
	fslog "github.com/rclone/rclone/fs/log"
	"github.com/rclone/rclone/fs/operations"
	"github.com/rclone/rclone/fs/rc"
)

const (
	// interval between progress status emissions
	defaultProgressInterval = 500 * time.Millisecond
)

// shouldSkipLogMessage returns true for backend debug messages that should be filtered out
func shouldSkipLogMessage(message string) bool {
	return strings.Contains(message, "Emitting event to frontend") ||
		strings.Contains(message, "Event emitted successfully") ||
		strings.Contains(message, "Event channel") ||
		strings.Contains(message, "SetApp called") ||
		strings.Contains(message, "SyncWithTab called") ||
		strings.Contains(message, "Generated task ID") ||
		strings.Contains(message, "Sending command")
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

// createStatusFromStats builds a SyncStatusDTO from rclone accounting stats.
// Identity fields (Id, TabId, Action) are NOT set — the service layer sets them.
func createStatusFromStats(ctx context.Context, startTime time.Time, logMessages []string) *dto.SyncStatusDTO {
	stats := accounting.Stats(ctx)

	syncStatus := &dto.SyncStatusDTO{
		Command:   dto.SyncStatus.String(),
		Status:    "running",
		Timestamp: time.Now(),
	}

	// Use RemoteStats for accurate totals, speed, ETA, and per-file transfer info
	remoteStats, err := stats.RemoteStats(false)
	if err == nil {
		// Totals from calculateTransferStats
		if v, ok := remoteStats["totalTransfers"]; ok {
			if n, ok := v.(int64); ok {
				syncStatus.TotalFiles = n
			}
		}
		if v, ok := remoteStats["totalBytes"]; ok {
			if n, ok := v.(int64); ok {
				syncStatus.TotalBytes = n
			}
		}

		// Completed counts
		if v, ok := remoteStats["transfers"]; ok {
			if n, ok := v.(int64); ok {
				syncStatus.FilesTransferred = n
			}
		}
		if v, ok := remoteStats["bytes"]; ok {
			if n, ok := v.(int64); ok {
				syncStatus.BytesTransferred = n
			}
		}
		if v, ok := remoteStats["errors"]; ok {
			if n, ok := v.(int64); ok {
				syncStatus.Errors = int(n)
			}
		}
		if v, ok := remoteStats["checks"]; ok {
			if n, ok := v.(int64); ok {
				syncStatus.Checks = n
			}
		}
		if v, ok := remoteStats["deletes"]; ok {
			if n, ok := v.(int64); ok {
				syncStatus.Deletes = n
			}
		}
		if v, ok := remoteStats["renames"]; ok {
			if n, ok := v.(int64); ok {
				syncStatus.Renames = n
			}
		}

		// Speed from rclone's own calculation
		if v, ok := remoteStats["speed"]; ok {
			if speed, ok := v.(float64); ok {
				syncStatus.Speed = formatSpeed(speed)
			}
		}

		// ETA from rclone
		if v, ok := remoteStats["eta"]; ok && v != nil {
			if etaSec, ok := v.(float64); ok {
				syncStatus.ETA = formatDuration(time.Duration(etaSec) * time.Second)
			}
		}

		// Build per-file transfer list
		var transfers []dto.FileTransferInfo

		// In-progress transfers
		if v, ok := remoteStats["transferring"]; ok && v != nil {
			if trList, ok := v.([]rc.Params); ok {
				for _, tr := range trList {
					fi := dto.FileTransferInfo{Status: "transferring"}
					if name, ok := tr["name"].(string); ok {
						fi.Name = name
					}
					if size, ok := tr["size"].(int64); ok {
						fi.Size = size
					}
					if bytes, ok := tr["bytes"].(int64); ok {
						fi.Bytes = bytes
					}
					if pct, ok := tr["percentage"].(int); ok {
						fi.Progress = float64(pct)
					}
					if speed, ok := tr["speed"].(float64); ok {
						fi.Speed = speed
					}
					transfers = append(transfers, fi)
				}
			}
		}

		// In-progress checks
		if v, ok := remoteStats["checking"]; ok && v != nil {
			if checkList, ok := v.([]string); ok {
				for _, name := range checkList {
					transfers = append(transfers, dto.FileTransferInfo{
						Name:   name,
						Status: "checking",
					})
				}
			}
		}

		// Completed/failed transfers
		for _, tr := range stats.Transferred() {
			fi := dto.FileTransferInfo{
				Name:  tr.Name,
				Size:  tr.Size,
				Bytes: tr.Bytes,
			}
			if tr.Checked {
				fi.Status = "checking"
				fi.Progress = 100
			} else if tr.Error != nil {
				fi.Status = "failed"
				fi.Error = tr.Error.Error()
			} else {
				fi.Status = "completed"
				fi.Progress = 100
			}
			transfers = append(transfers, fi)
		}

		syncStatus.Transfers = transfers
	} else {
		// Fallback to basic stats if RemoteStats fails
		syncStatus.FilesTransferred = stats.GetTransfers()
		syncStatus.BytesTransferred = stats.GetBytes()
		syncStatus.Errors = int(stats.GetErrors())
		syncStatus.Checks = stats.GetChecks()
		syncStatus.Deletes = stats.GetDeletes()
	}

	// Elapsed time
	elapsed := time.Since(startTime)
	syncStatus.ElapsedTime = formatDuration(elapsed)

	// Fallback speed if not set from RemoteStats
	if syncStatus.Speed == "" && elapsed.Seconds() > 0 {
		bytesPerSecond := float64(syncStatus.BytesTransferred) / elapsed.Seconds()
		syncStatus.Speed = formatSpeed(bytesPerSecond)
	}

	// Progress percentage
	if syncStatus.TotalBytes > 0 {
		syncStatus.Progress = float64(syncStatus.BytesTransferred) / float64(syncStatus.TotalBytes) * 100
	} else if syncStatus.TotalFiles > 0 {
		syncStatus.Progress = float64(syncStatus.FilesTransferred) / float64(syncStatus.TotalFiles) * 100
	}

	// Determine status
	if stats.GetErrors() > 0 {
		syncStatus.Status = "error"
	} else if syncStatus.Progress >= 100.0 {
		syncStatus.Status = "completed"
	} else {
		syncStatus.Status = "running"
	}

	// Attach accumulated log messages
	if len(logMessages) > 0 {
		syncStatus.LogMessages = logMessages
	}

	return syncStatus
}

// startProgress starts capturing rclone logs and producing structured SyncStatusDTO
// objects on the outStatus channel. It merges log capture hooks with periodic stats
// extraction into a single unified output stream.
//
// Returns a cleanup function that must be called when the operation completes.
func startProgress(ctx context.Context, outStatus chan *dto.SyncStatusDTO) func() {
	var isClosed atomic.Bool
	startTime := time.Now()

	// Log message accumulator — protected by mutex
	var logMu sync.Mutex
	var logAccum []string

	appendLog := func(msg string) {
		logMu.Lock()
		logAccum = append(logAccum, msg)
		logMu.Unlock()
	}

	drainLogs := func() []string {
		logMu.Lock()
		msgs := logAccum
		logAccum = nil
		logMu.Unlock()
		return msgs
	}

	// Safe send helper — non-blocking, returns false if channel full or closed
	safeSend := func(status *dto.SyncStatusDTO) bool {
		if isClosed.Load() {
			return false
		}
		select {
		case outStatus <- status:
			return true
		default:
			return false
		}
	}

	stopCh := make(chan struct{})
	oldSyncPrint := operations.SyncPrintf

	// Hook into rclone's logging system (fs.Errorf, fs.Logf, etc.)
	fslog.Handler.AddOutput(false, func(level slog.Level, text string) {
		if isClosed.Load() {
			return
		}
		text = strings.TrimSpace(text)
		if text == "" || shouldSkipLogMessage(text) {
			return
		}
		appendLog(text)
	})

	// Intercept output from functions such as HashLister to stdout
	operations.SyncPrintf = func(format string, a ...interface{}) {
		if isClosed.Load() {
			return
		}
		msg := strings.TrimSpace(fmt.Sprintf(format, a...))
		if msg != "" {
			appendLog(msg)
		}
	}

	var wg sync.WaitGroup
	wg.Add(1)
	go func() {
		defer wg.Done()
		ticker := time.NewTicker(defaultProgressInterval)
		defer ticker.Stop()

		for {
			select {
			case <-ticker.C:
				if !isClosed.Load() {
					msgs := drainLogs()
					status := createStatusFromStats(ctx, startTime, msgs)
					safeSend(status)
				}
			case <-stopCh:
				return
			}
		}
	}()

	return func() {
		// CRITICAL ORDER:
		// 1. Set flag to stop in-flight callbacks from accumulating
		isClosed.Store(true)
		// 2. Reset output handler to stop new callbacks
		fslog.Handler.ResetOutput()
		operations.SyncPrintf = oldSyncPrint
		// 3. Signal goroutine to stop
		close(stopCh)
		// 4. Wait for goroutine to finish
		wg.Wait()
		// 5. Emit one final DTO with any remaining accumulated messages
		remaining := drainLogs()
		if len(remaining) > 0 {
			finalStatus := createStatusFromStats(ctx, startTime, remaining)
			// Direct send (non-blocking) — channel might be full
			select {
			case outStatus <- finalStatus:
			default:
			}
		}
		// NOTE: Do NOT close outStatus here — the caller is responsible
	}
}
