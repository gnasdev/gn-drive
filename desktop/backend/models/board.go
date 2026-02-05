package models

import "time"

// BoardNode represents a remote storage endpoint on the board canvas
type BoardNode struct {
	Id         string  `json:"id"`
	RemoteName string  `json:"remote_name"` // rclone remote name or "local"
	Path       string  `json:"path"`        // base path on this remote
	Label      string  `json:"label"`
	X          float64 `json:"x"`
	Y          float64 `json:"y"`
}

// BoardEdge represents a sync connection between two nodes
type BoardEdge struct {
	Id         string  `json:"id"`
	SourceId   string  `json:"source_id"`
	TargetId   string  `json:"target_id"`
	Action     string  `json:"action"` // "pull","push","bi","bi-resync"
	SyncConfig Profile `json:"sync_config"`
}

// Board represents a complete flow definition
type Board struct {
	Id          string      `json:"id"`
	Name        string      `json:"name"`
	Description string      `json:"description,omitempty"`
	Nodes       []BoardNode `json:"nodes"`
	Edges       []BoardEdge `json:"edges"`
	CreatedAt   time.Time   `json:"created_at"`
	UpdatedAt   time.Time   `json:"updated_at"`
}

// BoardExecutionStatus represents the status of a running board flow
type BoardExecutionStatus struct {
	BoardId      string                `json:"board_id"`
	Status       string                `json:"status"` // "running","completed","failed","cancelled"
	EdgeStatuses []EdgeExecutionStatus `json:"edge_statuses"`
	StartTime    time.Time             `json:"start_time"`
	EndTime      *time.Time            `json:"end_time,omitempty"`
}

// EdgeExecutionStatus represents the status of a single edge execution
type EdgeExecutionStatus struct {
	EdgeId    string     `json:"edge_id"`
	Status    string     `json:"status"` // "pending","running","completed","failed","skipped"
	TaskId    int        `json:"task_id,omitempty"`
	Message   string     `json:"message,omitempty"`
	StartTime *time.Time `json:"start_time,omitempty"`
	EndTime   *time.Time `json:"end_time,omitempty"`
}
