package models

import "time"

// HistoryEntry represents a record of a completed sync/operation
type HistoryEntry struct {
	Id               string    `json:"id"`
	ProfileName      string    `json:"profile_name"`
	Action           string    `json:"action"`           // "pull", "push", "bi", "bi-resync", "copy", "move", etc.
	Status           string    `json:"status"`           // "completed", "failed", "cancelled"
	StartTime        time.Time `json:"start_time"`
	EndTime          time.Time `json:"end_time"`
	Duration         string    `json:"duration"`
	FilesTransferred int64     `json:"files_transferred"`
	BytesTransferred int64     `json:"bytes_transferred"`
	Errors           int       `json:"errors"`
	ErrorMessage     string    `json:"error_message,omitempty"`
}

// AggregateStats contains summary statistics across all history entries
type AggregateStats struct {
	TotalOperations int    `json:"total_operations"`
	SuccessCount    int    `json:"success_count"`
	FailureCount    int    `json:"failure_count"`
	CancelledCount  int    `json:"cancelled_count"`
	TotalBytes      int64  `json:"total_bytes"`
	TotalFiles      int64  `json:"total_files"`
	AverageDuration string `json:"average_duration"`
}
