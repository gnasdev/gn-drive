// Package service provides cross-platform service management.
//
// health.go: service.health file writer. Read by `gn-drive service status`
// and the web UI's /api/v1/service/status endpoint.
package service

import (
	"encoding/json"
	"fmt"
	"os"
	"path/filepath"
	"sync"
	"time"
)

// HealthPath returns the path to the service.health file in the given config dir.
func HealthPath(configDir string) string {
	return filepath.Join(configDir, "service.health")
}

// Health is the JSON structure written to service.health.
type Health struct {
	PID            int       `json:"pid"`
	ServiceName    string    `json:"service_name"`
	Mode           string    `json:"mode"` // always "service" when written
	StartedAt      time.Time `json:"started_at"`
	LastHeartbeat  time.Time `json:"last_heartbeat"`
	WebPort        int       `json:"web_port"`
	LastSyncAt     time.Time `json:"last_sync_at,omitempty"`
	NextScheduleAt time.Time `json:"next_schedule_at,omitempty"`
	LastError      string    `json:"last_error,omitempty"`
	ActiveTasks    []string  `json:"active_tasks"`
}

// Writer periodically writes the Health struct to disk.
type Writer struct {
	mu     sync.Mutex
	path   string
	health Health
	stop   chan struct{}
	period time.Duration
}

// NewWriter creates a new health writer that writes to the given config dir.
// The initial write is performed synchronously; subsequent writes happen
// every `period` (default 5s) until Stop is called.
func NewWriter(configDir string, period time.Duration) *Writer {
	if period <= 0 {
		period = 5 * time.Second
	}
	return &Writer{
		path:   HealthPath(configDir),
		health: Health{},
		stop:   make(chan struct{}),
		period: period,
	}
}

// Start launches the background goroutine. The first heartbeat is written
// synchronously.
func (w *Writer) Start() error {
	w.mu.Lock()
	w.health.PID = os.Getpid()
	w.health.ServiceName = "gn-drive"
	w.health.Mode = "service"
	w.health.StartedAt = time.Now().UTC()
	w.health.LastHeartbeat = time.Now().UTC()
	w.health.ActiveTasks = []string{}
	if err := w.writeLocked(); err != nil {
		w.mu.Unlock()
		return err
	}
	w.mu.Unlock()

	go w.loop()
	return nil
}

func (w *Writer) loop() {
	t := time.NewTicker(w.period)
	defer t.Stop()
	for {
		select {
		case <-w.stop:
			return
		case <-t.C:
			w.mu.Lock()
			w.health.LastHeartbeat = time.Now().UTC()
			_ = w.writeLocked()
			w.mu.Unlock()
		}
	}
}

// SetWebPort updates the web port.
func (w *Writer) SetWebPort(port int) {
	w.mu.Lock()
	defer w.mu.Unlock()
	w.health.WebPort = port
}

// SetLastError records the last error.
func (w *Writer) SetLastError(msg string) {
	w.mu.Lock()
	defer w.mu.Unlock()
	w.health.LastError = msg
}

// SetActiveTasks records the list of active task IDs.
func (w *Writer) SetActiveTasks(taskIDs []string) {
	w.mu.Lock()
	defer w.mu.Unlock()
	if taskIDs == nil {
		taskIDs = []string{}
	}
	w.health.ActiveTasks = taskIDs
}

// SetLastSyncAt records the last sync completion time.
func (w *Writer) SetLastSyncAt(t time.Time) {
	w.mu.Lock()
	defer w.mu.Unlock()
	w.health.LastSyncAt = t.UTC()
}

// SetNextScheduleAt records the next scheduled sync time.
func (w *Writer) SetNextScheduleAt(t time.Time) {
	w.mu.Lock()
	defer w.mu.Unlock()
	w.health.NextScheduleAt = t.UTC()
}

// Snapshot returns a copy of the current health.
func (w *Writer) Snapshot() Health {
	w.mu.Lock()
	defer w.mu.Unlock()
	return w.health
}

// Stop stops the writer.
func (w *Writer) Stop() {
	select {
	case <-w.stop:
		// already closed
	default:
		close(w.stop)
	}
}

func (w *Writer) writeLocked() error {
	data, err := marshalHealth(w.health)
	if err != nil {
		return fmt.Errorf("marshal health: %w", err)
	}
	return os.WriteFile(w.path, data, 0o644)
}

// marshalHealth is overridable for tests.
var marshalHealth = func(h Health) ([]byte, error) {
	return json.MarshalIndent(h, "", "  ")
}

// ReadHealth reads and parses the service.health file.
// Returns ErrNotInstalled if the file does not exist.
func ReadHealth(configDir string) (Health, error) {
	var h Health
	data, err := os.ReadFile(HealthPath(configDir))
	if err != nil {
		if os.IsNotExist(err) {
			return h, ErrNotInstalled
		}
		return h, fmt.Errorf("read health: %w", err)
	}
	if err := json.Unmarshal(data, &h); err != nil {
		return h, fmt.Errorf("parse health: %w", err)
	}
	return h, nil
}

// IsStale returns true if the health's LastHeartbeat is older than maxAge.
func (h Health) IsStale(maxAge time.Duration) bool {
	if h.LastHeartbeat.IsZero() {
		return true
	}
	return time.Since(h.LastHeartbeat) > maxAge
}

// Uptime returns the duration since the service started.
func (h Health) Uptime() time.Duration {
	if h.StartedAt.IsZero() {
		return 0
	}
	return time.Since(h.StartedAt)
}
