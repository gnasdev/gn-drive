// Package logging provides structured logging with slog.
//
// Phase 1: Logger struct wrapping slog.
// Two handlers based on mode:
//   - Foreground (run): text handler → stderr
//   - Service (run --service): JSON handler → stderr (journald picks this up)
//
// Log buffer (in-memory ring buffer for SSE forwarding) is added in phase 3.
package logging

import (
	"context"
	"log/slog"
	"os"
)

// Mode determines the log format and output.
type Mode string

const (
	ModeForeground Mode = "foreground" // human-readable text
	ModeService    Mode = "service"   // structured JSON (journald compatible)
)

// Logger wraps slog with a closeable handler.
type Logger struct {
	*slog.Logger
	mode   Mode
	cancel context.CancelFunc
}

// New creates a new Logger. Call Close() when done.
func New(mode Mode) *Logger {
	var handler slog.Handler
	opts := &slog.HandlerOptions{
		Level: slog.LevelDebug,
	}

	switch mode {
	case ModeService:
		handler = slog.NewJSONHandler(os.Stderr, opts)
	default: // foreground
		handler = slog.NewTextHandler(os.Stderr, opts)
	}

	l := slog.New(handler)
	return &Logger{Logger: l, mode: mode}
}

// WithContext returns a logger that includes context fields.
func (l *Logger) WithContext(ctx context.Context) *Logger {
	// Phase 1: no-op. Phase 3 adds trace IDs.
	return l
}

// Mode returns the logging mode.
func (l *Logger) Mode() Mode { return l.mode }

// Close releases logger resources.
func (l *Logger) Close() error {
	if l.cancel != nil {
		l.cancel()
	}
	return nil
}