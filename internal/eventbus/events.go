// Package eventbus provides in-process typed event channels.
// See bus.go for the Bus interface.
package eventbus

import "time"

// Event is the base interface for all bus events.
type Event interface {
	eventMarker() // prevents accidental interface{} use
}

type eventBase struct {
	Type      string    `json:"type"`
	Timestamp time.Time `json:"timestamp"`
}

func (eventBase) eventMarker() {}

// --- Sync events -----------------------------------------------------------

// SyncProgressEvent is emitted periodically during a sync task.
type SyncProgressEvent struct {
	eventBase
	TaskID          string  `json:"task_id"`
	ProfileID       string  `json:"profile_id"`
	Action          string  `json:"action"`
	State           string  `json:"state"` // running, completed, failed, cancelled
	Transferred     int64   `json:"transferred"`
	Total           int64   `json:"total"`
	BytesPerSec     float64 `json:"bytes_per_sec"`
	ETA             int64   `json:"eta_secs"`
	Errors          int     `json:"errors"`
	CurrentFile     string  `json:"current_file"`
	FilesTransferred int    `json:"files_transferred"`
	TotalFiles      int     `json:"total_files"`
	// ErrorMessage is set on failed sync events so the UI can surface the
	// reason (omitted for running/completed events).
	ErrorMessage string `json:"error_message,omitempty"`
}

// SyncStartedEvent is emitted when a sync task begins.
type SyncStartedEvent struct {
	eventBase
	TaskID    string `json:"task_id"`
	ProfileID string `json:"profile_id"`
	Action    string `json:"action"`
}

// SyncCompletedEvent is emitted when a sync task finishes.
type SyncCompletedEvent struct {
	eventBase
	TaskID    string `json:"task_id"`
	ProfileID string `json:"profile_id"`
	Action    string `json:"action"`
	Duration  int64  `json:"duration_secs"`
	Bytes     int64  `json:"bytes"`
	Errors    int    `json:"errors"`
}

// --- Auth events -----------------------------------------------------------

// AuthUnlockedEvent is emitted after successful master password unlock.
type AuthUnlockedEvent struct {
	eventBase
}

// AuthLockedEvent is emitted after lock.
type AuthLockedEvent struct {
	eventBase
}

// --- Service events --------------------------------------------------------

// ServiceStatusEvent is emitted on service state changes.
type ServiceStatusEvent struct {
	eventBase
	Running  bool `json:"running"`
	WebPort  int  `json:"web_port"`
	UptimeSecs int `json:"uptime_secs"`
}

// --- Schedule events -------------------------------------------------------

// ScheduleTriggeredEvent is emitted when a cron schedule fires.
type ScheduleTriggeredEvent struct {
	eventBase
	ScheduleID string `json:"schedule_id"`
	ProfileID  string `json:"profile_id"`
	Action     string `json:"action"`
}

// --- Board events ----------------------------------------------------------

// BoardExecutionEvent is emitted during board DAG execution.
type BoardExecutionEvent struct {
	eventBase
	BoardID   string `json:"board_id"`
	NodeID    string `json:"node_id,omitempty"`
	EdgeID    string `json:"edge_id,omitempty"`
	Status    string `json:"status"` // running, completed, failed
	ProfileID string `json:"profile_id,omitempty"`
	Action    string `json:"action,omitempty"`
}

// --- State events ----------------------------------------------------------

// StateChangedEvent is emitted when app-wide state (config, remotes) changes.
type StateChangedEvent struct {
	eventBase
	Domain string `json:"domain"` // config, remotes, profiles
}