package utils

import (
	"bytes"
	"fmt"
	"log/slog"
	"strings"
	"sync"
	"sync/atomic"
	"time"

	"github.com/rclone/rclone/cmd"
	"github.com/rclone/rclone/fs/accounting"
	fslog "github.com/rclone/rclone/fs/log"
	"github.com/rclone/rclone/fs/operations"
)

var (
	// state for the progress printing
	nlines = 0 // number of lines in the previous stats block
)

const (
	statsInterval = time.Minute * 1
	// interval between progress prints
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

// formatToProgress prints the progress with an optional log
func formatToProgress(logMessage string) string {
	operations.StdoutMutex.Lock()
	defer operations.StdoutMutex.Unlock()

	var buf bytes.Buffer
	// w, _ := terminal.GetSize()
	stats := strings.TrimSpace(accounting.GlobalStats().String())
	logMessage = strings.TrimSpace(logMessage)

	out := func(s string) {
		buf.WriteString(s)
	}

	// if logMessage != "" {
	// 	out("\n")
	// 	out(terminal.MoveUp)
	// }
	// // Move to the start of the block we wrote erasing all the previous lines
	// for i := 0; i < nlines-1; i++ {
	// 	out(terminal.EraseLine)
	// 	out(terminal.MoveUp)
	// }
	// out(terminal.EraseLine)
	// out(terminal.MoveToStartOfLine)

	if logMessage != "" {
		// out(terminal.EraseLine)
		out(logMessage + "\n")
	}

	fixedLines := strings.Split(stats, "\n")
	nlines = len(fixedLines)
	for i, line := range fixedLines {
		// if len(line) > w {
		// 	line = line[:w]
		// }
		out(line)
		if i != nlines-1 {
			out("\n")
		}
	}
	// terminal.Write(buf.Bytes())

	return buf.String()
}

// startProgress starts the progress bar printing
//
// It returns a func which should be called to stop the stats.
func startProgress(outLog chan string) func() {
	// Use atomic.Bool for thread-safe flag access (fixes race condition)
	var isOutLogClosed atomic.Bool

	// Safe send helper - returns false if channel is closed
	safeSend := func(msg string) bool {
		if isOutLogClosed.Load() {
			return false
		}
		// Use select with default to avoid blocking if channel is full
		select {
		case outLog <- msg:
			return true
		default:
			// Channel full, skip this message but don't panic
			return false
		}
	}

	stopStats := make(chan struct{})
	oldSyncPrint := operations.SyncPrintf

	// Use rclone's OutputHandler.AddOutput to capture logs
	// This hooks into rclone's actual logging system (fs.Errorf, fs.Logf, etc.)
	fslog.Handler.AddOutput(false, func(level slog.Level, text string) {
		if isOutLogClosed.Load() {
			return
		}
		text = strings.TrimSpace(text)
		if text == "" || shouldSkipLogMessage(text) {
			return
		}
		safeSend(formatToProgress(text))
	})

	// Intercept output from functions such as HashLister to stdout
	operations.SyncPrintf = func(format string, a ...interface{}) {
		if !isOutLogClosed.Load() {
			safeSend(formatToProgress(fmt.Sprintf(format, a...)))
		}
	}

	var wg sync.WaitGroup
	wg.Add(1)
	go func() {
		defer wg.Done()
		progressInterval := defaultProgressInterval
		if cmd.ShowStats() && statsInterval > 0 {
			progressInterval = statsInterval
		}
		ticker := time.NewTicker(progressInterval)
		for {
			select {
			case <-ticker.C:
				if !isOutLogClosed.Load() {
					safeSend(formatToProgress(""))
				}
			case <-stopStats:
				ticker.Stop()
				return
			}
		}
	}()

	return func() {
		// CRITICAL ORDER:
		// 1. Set flag atomically to stop any in-flight callbacks from sending
		isOutLogClosed.Store(true)
		// 2. Reset output handler to stop new callbacks
		fslog.Handler.ResetOutput()
		operations.SyncPrintf = oldSyncPrint
		// 3. Signal goroutine to stop
		close(stopStats)
		// 4. Wait for goroutine to finish
		wg.Wait()
		// NOTE: Do NOT close outLog here - the caller (sync_service.go) is responsible for closing it
	}
}

func startStats(outLog chan string) func() {
	// Use atomic.Bool for thread-safe flag access (fixes race condition)
	var isOutLogClosed atomic.Bool

	// Safe send helper
	safeSend := func(msg string) bool {
		if isOutLogClosed.Load() {
			return false
		}
		select {
		case outLog <- msg:
			return true
		default:
			return false
		}
	}

	if statsInterval <= 0 {
		return func() {}
	}
	stopStats := make(chan struct{})
	var wg sync.WaitGroup
	wg.Add(1)
	go func() {
		defer wg.Done()
		ticker := time.NewTicker(statsInterval)
		for {
			select {
			case <-ticker.C:
				if !isOutLogClosed.Load() {
					safeSend(accounting.GlobalStats().String())
				}
			case <-stopStats:
				ticker.Stop()
				return
			}
		}
	}()
	return func() {
		isOutLogClosed.Store(true)
		close(stopStats)
		wg.Wait()
		// NOTE: Do NOT close outLog here - the caller (sync_service.go) is responsible for closing it
	}
}
