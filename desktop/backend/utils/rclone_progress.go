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
	// maximum log messages per status DTO to prevent unbounded growth
	maxLogMessagesPerStatus = 50
)

// extractLogContent strips rclone log prefixes (stats group + timestamp + level)
// and returns the actual message content. For example:
//
//	"[task-1] 2026/02/10 20:18:46 NOTICE: actual message" → "actual message"
//	"[task-1] 2026/02/10 20:18:46 NOTICE:" → ""
//	"plain message" → "plain message"
func extractLogContent(message string) string {
	// Strip ALL [...] prefixes (there may be multiple if a message was
	// re-captured through Go's standard logger → slog → callback loop)
	msg := message
	for strings.HasPrefix(msg, "[") {
		if idx := strings.Index(msg, "] "); idx != -1 {
			msg = msg[idx+2:]
		} else {
			break
		}
	}

	// Strip known rclone log level tags and everything before them
	for _, tag := range []string{"NOTICE:", "INFO  :", "DEBUG :", "ERROR :", "WARNING:"} {
		if idx := strings.LastIndex(msg, tag); idx != -1 {
			msg = msg[idx+len(tag):]
			break
		}
	}

	return strings.TrimSpace(msg)
}

// shouldSkipLogMessage returns true for messages that should be filtered out of log output.
// This includes internal backend debug messages and rclone's periodic stats output
// (which we already capture via RemoteStats).
func shouldSkipLogMessage(message string) bool {
	// Internal backend debug messages
	if strings.Contains(message, "Emitting event to frontend") ||
		strings.Contains(message, "Event emitted successfully") ||
		strings.Contains(message, "Event channel") ||
		strings.Contains(message, "SetApp called") ||
		strings.Contains(message, "SyncWithTab called") ||
		strings.Contains(message, "Generated task ID") ||
		strings.Contains(message, "Sending command") {
		return true
	}

	// Extract actual content after stripping rclone prefix/timestamp/level
	content := extractLogContent(message)

	// Empty messages (e.g., "[sync:push:board-...] 2026/... NOTICE:" with no content)
	if content == "" || content == "-" {
		return true
	}

	// rclone periodic stats output (redundant — we capture stats via RemoteStats)
	if strings.HasPrefix(content, "Transferred:") ||
		strings.HasPrefix(content, "Checks:") ||
		strings.HasPrefix(content, "Elapsed time:") ||
		strings.HasPrefix(content, "Transferring:") ||
		strings.HasPrefix(content, " *") {
		return true
	}

	return false
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
// Returns the status DTO and the set of currently-checking file names (for accumulation tracking).
func createStatusFromStats(ctx context.Context, startTime time.Time, logMessages []string) (*dto.SyncStatusDTO, map[string]struct{}) {
	stats := accounting.Stats(ctx)

	syncStatus := &dto.SyncStatusDTO{
		Command:   dto.SyncStatus.String(),
		Status:    "running",
		Timestamp: time.Now(),
	}

	// Track currently-checking file names for the caller's accumulation logic
	currentlyChecking := make(map[string]struct{})

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
		if v, ok := remoteStats["totalChecks"]; ok {
			if n, ok := v.(int64); ok {
				syncStatus.TotalChecks = n
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

		// In-progress checks — also collect names for accumulation tracking
		if v, ok := remoteStats["checking"]; ok && v != nil {
			if checkList, ok := v.([]string); ok {
				for _, name := range checkList {
					currentlyChecking[name] = struct{}{}
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
			if tr.Error != nil {
				fi.Status = "failed"
				fi.Error = tr.Error.Error()
			} else if tr.Checked {
				fi.Status = "checked"
				fi.Progress = 100
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
	} else if syncStatus.TotalChecks > 0 {
		// During check-only phase (no transfers yet), show check progress
		syncStatus.Progress = float64(syncStatus.Checks) / float64(syncStatus.TotalChecks) * 100
	}

	// Determine status
	if stats.GetErrors() > 0 {
		syncStatus.Status = "error"
	} else if syncStatus.Progress >= 100.0 {
		syncStatus.Status = "completed"
	} else {
		syncStatus.Status = "running"
	}

	// Derive error/check counts from the actual transfer list so displayed
	// counts match the items shown (rclone's raw counters can diverge due to
	// retries, transient errors, and internal list caps).
	if len(syncStatus.Transfers) > 0 {
		var errCount int
		var checkCount int64
		for _, ft := range syncStatus.Transfers {
			switch ft.Status {
			case "failed":
				errCount++
			case "checked", "checking":
				checkCount++
			}
		}
		syncStatus.Errors = errCount
		syncStatus.Checks = checkCount
	}

	// Attach accumulated log messages (capped to prevent unbounded growth)
	if len(logMessages) > maxLogMessagesPerStatus {
		logMessages = logMessages[len(logMessages)-maxLogMessagesPerStatus:]
	}
	if len(logMessages) > 0 {
		syncStatus.LogMessages = logMessages
	}

	return syncStatus, currentlyChecking
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
		// Store cleaned content (strip rclone prefix/timestamp/level)
		content := extractLogContent(text)
		if content == "" {
			return
		}
		appendLog(content)
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

		// Accumulate completed checks by tracking the checking set across ticks.
		// Files that leave remoteStats["checking"] between ticks are "done checking".
		const maxCompletedChecks = 100
		prevChecking := make(map[string]struct{})
		var completedChecks []dto.FileTransferInfo

		for {
			select {
			case <-ticker.C:
				if !isClosed.Load() {
					msgs := drainLogs()
					status, curChecking := createStatusFromStats(ctx, startTime, msgs)

					// Detect files that left the checking set → completed check
					for name := range prevChecking {
						if _, ok := curChecking[name]; !ok {
							completedChecks = append(completedChecks, dto.FileTransferInfo{
								Name:     name,
								Status:   "checked",
								Progress: 100,
							})
						}
					}
					prevChecking = curChecking

					// Bound the accumulator
					if len(completedChecks) > maxCompletedChecks {
						completedChecks = completedChecks[len(completedChecks)-maxCompletedChecks:]
					}

					// Inject accumulated completed checks (skip duplicates already in list)
					existingNames := make(map[string]struct{}, len(status.Transfers))
					for _, ft := range status.Transfers {
						existingNames[ft.Name] = struct{}{}
					}
					for _, cc := range completedChecks {
						if _, exists := existingNames[cc.Name]; !exists {
							status.Transfers = append(status.Transfers, cc)
						}
					}

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
			finalStatus, _ := createStatusFromStats(ctx, startTime, remaining)
			// Direct send (non-blocking) — channel might be full
			select {
			case outStatus <- finalStatus:
			default:
			}
		}
		// NOTE: Do NOT close outStatus here — the caller is responsible
	}
}
