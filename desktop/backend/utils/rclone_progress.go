package utils

import (
	"bytes"
	"fmt"
	"log/slog"
	"strings"
	"sync"
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
	isOutLogClosed := false
	outLogClosedRecover := func() {
		if r := recover(); r != nil {
			isOutLogClosed = true
		}
	}

	stopStats := make(chan struct{})
	oldSyncPrint := operations.SyncPrintf

	// Use rclone's OutputHandler.AddOutput to capture logs
	// This hooks into rclone's actual logging system (fs.Errorf, fs.Logf, etc.)
	fslog.Handler.AddOutput(false, func(level slog.Level, text string) {
		defer outLogClosedRecover()
		if isOutLogClosed {
			return
		}
		text = strings.TrimSpace(text)
		if text == "" || shouldSkipLogMessage(text) {
			return
		}
		outLog <- formatToProgress(text)
	})

	// Intercept output from functions such as HashLister to stdout
	operations.SyncPrintf = func(format string, a ...interface{}) {
		defer outLogClosedRecover()
		if !isOutLogClosed {
			outLog <- formatToProgress(fmt.Sprintf(format, a...))
		}
	}

	var wg sync.WaitGroup
	wg.Add(1)
	go func() {
		defer outLogClosedRecover()
		defer wg.Done()
		progressInterval := defaultProgressInterval
		if cmd.ShowStats() && statsInterval > 0 {
			progressInterval = statsInterval
		}
		ticker := time.NewTicker(progressInterval)
		for {
			select {
			case <-ticker.C:
				if !isOutLogClosed {
					outLog <- formatToProgress("")
				}
			case <-stopStats:
				ticker.Stop()
				return
			}
		}
	}()

	return func() {
		defer outLogClosedRecover()
		// CRITICAL ORDER:
		// 1. Set flag to stop any in-flight callbacks from sending
		isOutLogClosed = true
		// 2. Reset output handler to stop new callbacks
		fslog.Handler.ResetOutput()
		operations.SyncPrintf = oldSyncPrint
		// 3. Signal goroutine to stop
		close(stopStats)
		// 4. Wait for goroutine to finish
		wg.Wait()
		// 5. Finally close the channel (safe now since flag prevents sends)
		close(outLog)
	}
}

func startStats(outLog chan string) func() {
	isOutLogClosed := false
	outLogClosedRecover := func() {
		if r := recover(); r != nil {
			isOutLogClosed = true
		}
	}

	if statsInterval <= 0 {
		return func() {}
	}
	stopStats := make(chan struct{})
	var wg sync.WaitGroup
	wg.Add(1)
	go func() {
		defer outLogClosedRecover()
		defer wg.Done()
		ticker := time.NewTicker(statsInterval)
		for {
			select {
			case <-ticker.C:
				if !isOutLogClosed {
					outLog <- accounting.GlobalStats().String()
				}
			case <-stopStats:
				ticker.Stop()
				return
			}
		}
	}()
	return func() {
		defer outLogClosedRecover()
		close(stopStats)
		if !isOutLogClosed {
			close(outLog)
		}
		wg.Wait()
	}
}
