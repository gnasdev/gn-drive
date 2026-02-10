package dto

import (
	"encoding/json"
	"time"
)

// SyncStatusDTO represents the sync status information sent to frontend
type SyncStatusDTO struct {
	Command         string    `json:"command"`
	Id              *int      `json:"pid,omitempty"`
	TabId           *string   `json:"tab_id,omitempty"`
	Status          string    `json:"status"`           // "running", "completed", "error", "stopped"
	Progress        float64   `json:"progress"`         // 0-100 percentage
	Speed           string    `json:"speed"`            // e.g., "1.2 MB/s"
	ETA             string    `json:"eta"`              // estimated time remaining
	FilesTransferred int64    `json:"files_transferred"`
	TotalFiles      int64     `json:"total_files"`
	BytesTransferred int64    `json:"bytes_transferred"`
	TotalBytes      int64     `json:"total_bytes"`
	CurrentFile     string    `json:"current_file"`
	Errors          int       `json:"errors"`
	Checks          int64     `json:"checks"`
	TotalChecks     int64     `json:"total_checks"`
	Deletes         int64     `json:"deletes"`
	Renames         int64     `json:"renames"`
	Timestamp       time.Time `json:"timestamp"`
	ElapsedTime     string    `json:"elapsed_time"`
	Action          string             `json:"action"`                      // "pull", "push", "bi", "bi-resync"
	LogMessages     []string           `json:"log_messages,omitempty"`      // Captured rclone log messages since last emission
	Transfers       []FileTransferInfo `json:"transfers,omitempty"`         // Per-file transfer info
	DeltaMode       bool               `json:"delta_mode,omitempty"`        // true if using delta sync optimization
	DeltaSkipped    bool               `json:"delta_skipped,omitempty"`     // true if sync was skipped (no changes detected)
}

// FileTransferInfo represents a single file's transfer status
type FileTransferInfo struct {
	Name     string  `json:"name"`
	Size     int64   `json:"size"`
	Bytes    int64   `json:"bytes"`
	Progress float64 `json:"progress"`            // 0-100
	Status   string  `json:"status"`              // "transferring", "completed", "failed", "checking", "checked"
	Speed    float64 `json:"speed,omitempty"`     // bytes per second
	Error    string  `json:"error,omitempty"`
}

// ToJSON converts SyncStatusDTO to JSON bytes
func (s SyncStatusDTO) ToJSON() ([]byte, error) {
	jsonData, err := json.Marshal(s)
	if err != nil {
		return []byte{}, err
	}
	return jsonData, nil
}

// NewSyncStatusDTO creates a new SyncStatusDTO
func NewSyncStatusDTO(id int, action string) *SyncStatusDTO {
	return &SyncStatusDTO{
		Command:   SyncStatus.String(),
		Id:        &id,
		Status:    "running",
		Progress:  0.0,
		Action:    action,
		Timestamp: time.Now(),
	}
}

// NewSyncStatusDTOWithTab creates a new SyncStatusDTO with tab ID
func NewSyncStatusDTOWithTab(id int, action string, tabId string) *SyncStatusDTO {
	dto := NewSyncStatusDTO(id, action)
	dto.TabId = &tabId
	return dto
}
