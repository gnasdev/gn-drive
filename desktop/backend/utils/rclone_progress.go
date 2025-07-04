package utils

import (
	"bytes"
	"context"
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
	// time format for logging
	logTimeFormat = "2006/01/02 15:04:05"
)

// Custom slog handler that intercepts logs and forwards them to our progress channel
type progressLogHandler struct {
	originalHandler     slog.Handler
	outLog              chan string
	isOutLogClosed      *bool
	outLogClosedRecover func()
}

func (h *progressLogHandler) Enabled(ctx context.Context, level slog.Level) bool {
	return h.originalHandler.Enabled(ctx, level)
}

func (h *progressLogHandler) Handle(ctx context.Context, r slog.Record) error {
	// Filter out backend debug messages to prevent recursive logging
	message := r.Message
	if strings.Contains(message, "Emitting event to frontend") ||
		strings.Contains(message, "Event emitted successfully") ||
		strings.Contains(message, "Event channel") ||
		strings.Contains(message, "SetApp called") ||
		strings.Contains(message, "SyncWithTab called") ||
		strings.Contains(message, "Generated task ID") ||
		strings.Contains(message, "Sending command") {
		// Skip these backend debug messages
		return h.originalHandler.Handle(ctx, r)
	}

	// Format the log message similar to the original format
	defer h.outLogClosedRecover()
	if !*h.isOutLogClosed {
		logMsg := fmt.Sprintf("%s %-6s: %s", r.Time.Format(logTimeFormat), r.Level.String(), r.Message)
		h.outLog <- formatToProgress(logMsg)
	}

	// Also call the original handler
	return h.originalHandler.Handle(ctx, r)
}

func (h *progressLogHandler) WithAttrs(attrs []slog.Attr) slog.Handler {
	return &progressLogHandler{
		originalHandler:     h.originalHandler.WithAttrs(attrs),
		outLog:              h.outLog,
		isOutLogClosed:      h.isOutLogClosed,
		outLogClosedRecover: h.outLogClosedRecover,
	}
}

func (h *progressLogHandler) WithGroup(name string) slog.Handler {
	return &progressLogHandler{
		originalHandler:     h.originalHandler.WithGroup(name),
		outLog:              h.outLog,
		isOutLogClosed:      h.isOutLogClosed,
		outLogClosedRecover: h.outLogClosedRecover,
	}
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
	var originalLogger *slog.Logger
	var customHandler *progressLogHandler

	if !fslog.Redirected() {
		// Intercept the log calls using a custom slog handler
		originalLogger = slog.Default()
		customHandler = &progressLogHandler{
			originalHandler:     originalLogger.Handler(),
			outLog:              outLog,
			isOutLogClosed:      &isOutLogClosed,
			outLogClosedRecover: outLogClosedRecover,
		}

		// Set the custom handler as the default logger
		newLogger := slog.New(customHandler)
		slog.SetDefault(newLogger)
	}

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
				// Restore the original logger
				if originalLogger != nil {
					slog.SetDefault(originalLogger)
				}
				operations.SyncPrintf = oldSyncPrint
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
