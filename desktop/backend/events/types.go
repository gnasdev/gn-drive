package events

import "time"

// EventType defines the type of event
type EventType string

const (
	// Sync Events
	SyncStarted   EventType = "sync:started"
	SyncProgress  EventType = "sync:progress"
	SyncCompleted EventType = "sync:completed"
	SyncFailed    EventType = "sync:failed"
	SyncCancelled EventType = "sync:cancelled"

	// Config Events
	ConfigUpdated  EventType = "config:updated"
	ProfileAdded   EventType = "profile:added"
	ProfileUpdated EventType = "profile:updated"
	ProfileDeleted EventType = "profile:deleted"

	// Remote Events
	RemoteAdded   EventType = "remote:added"
	RemoteUpdated EventType = "remote:updated"
	RemoteDeleted EventType = "remote:deleted"
	RemotesList   EventType = "remotes:list"

	// Tab Events
	TabCreated EventType = "tab:created"
	TabUpdated EventType = "tab:updated"
	TabDeleted EventType = "tab:deleted"
	TabOutput  EventType = "tab:output"

	// Error Events
	ErrorOccurred EventType = "error:occurred"

	// Operation Events (non-sync operations: copy, move, check, dedupe, etc.)
	OperationStarted   EventType = "operation:started"
	OperationProgress  EventType = "operation:progress"
	OperationCompleted EventType = "operation:completed"
	OperationFailed    EventType = "operation:failed"

	// File Browser Events
	FileBrowserResult EventType = "filebrowser:result"

	// Schedule Events
	ScheduleAdded     EventType = "schedule:added"
	ScheduleUpdated   EventType = "schedule:updated"
	ScheduleDeleted   EventType = "schedule:deleted"
	ScheduleTriggered EventType = "schedule:triggered"

	// History Events
	HistoryAdded   EventType = "history:added"
	HistoryCleared EventType = "history:cleared"

	// Crypt Events
	CryptRemoteCreated EventType = "crypt:created"
	CryptRemoteDeleted EventType = "crypt:deleted"

	// Board Events
	BoardUpdated            EventType = "board:updated"
	BoardExecutionStarted   EventType = "board:execution:started"
	BoardExecutionProgress  EventType = "board:execution:progress"
	BoardExecutionCompleted EventType = "board:execution:completed"
	BoardExecutionFailed    EventType = "board:execution:failed"
	BoardExecutionCancelled EventType = "board:execution:cancelled"
)

// BaseEvent represents the base structure for all events
type BaseEvent struct {
	Type      EventType   `json:"type"`
	Timestamp time.Time   `json:"timestamp"`
	Data      interface{} `json:"data"`
}

// SyncEvent represents sync operation events
type SyncEvent struct {
	BaseEvent
	TabId    string `json:"tabId,omitempty"`
	Action   string `json:"action"`
	Progress int    `json:"progress,omitempty"`
	Status   string `json:"status"`
	Message  string `json:"message,omitempty"`
	SeqNo    uint64 `json:"seqNo,omitempty"`
}

// SyncProgressData represents detailed sync progress information
type SyncProgressData struct {
	TransferredBytes int64  `json:"transferredBytes"`
	TotalBytes       int64  `json:"totalBytes"`
	TransferredFiles int    `json:"transferredFiles"`
	TotalFiles       int    `json:"totalFiles"`
	CurrentFile      string `json:"currentFile"`
	Speed            string `json:"speed"`
	ETA              string `json:"eta"`
	Errors           int    `json:"errors"`
	Checks           int    `json:"checks"`
	Deletes          int    `json:"deletes"`
	Renames          int    `json:"renames"`
	ElapsedTime      string `json:"elapsedTime"`
}

// ConfigEvent represents configuration events
type ConfigEvent struct {
	BaseEvent
	ProfileId string `json:"profileId,omitempty"`
}

// RemoteEvent represents remote storage events
type RemoteEvent struct {
	BaseEvent
	RemoteName string `json:"remoteName,omitempty"`
}

// TabEvent represents tab operation events
type TabEvent struct {
	BaseEvent
	TabId   string `json:"tabId"`
	TabName string `json:"tabName,omitempty"`
}

// ErrorEvent represents error events
type ErrorEvent struct {
	BaseEvent
	Code    string `json:"code"`
	Message string `json:"message"`
	Details string `json:"details,omitempty"`
	TabId   string `json:"tabId,omitempty"`
}

// NewSyncEvent creates a new sync event
func NewSyncEvent(eventType EventType, tabId, action, status, message string) *SyncEvent {
	return &SyncEvent{
		BaseEvent: BaseEvent{
			Type:      eventType,
			Timestamp: time.Now(),
		},
		TabId:   tabId,
		Action:  action,
		Status:  status,
		Message: message,
	}
}

// NewSyncEventWithSeqNo creates a new sync event with sequence number
func NewSyncEventWithSeqNo(eventType EventType, tabId, action, status, message string, seqNo uint64) *SyncEvent {
	return &SyncEvent{
		BaseEvent: BaseEvent{
			Type:      eventType,
			Timestamp: time.Now(),
		},
		TabId:   tabId,
		Action:  action,
		Status:  status,
		Message: message,
		SeqNo:   seqNo,
	}
}

// NewConfigEvent creates a new config event
func NewConfigEvent(eventType EventType, profileId string, data interface{}) *ConfigEvent {
	return &ConfigEvent{
		BaseEvent: BaseEvent{
			Type:      eventType,
			Timestamp: time.Now(),
			Data:      data,
		},
		ProfileId: profileId,
	}
}

// NewRemoteEvent creates a new remote event
func NewRemoteEvent(eventType EventType, remoteName string, data interface{}) *RemoteEvent {
	return &RemoteEvent{
		BaseEvent: BaseEvent{
			Type:      eventType,
			Timestamp: time.Now(),
			Data:      data,
		},
		RemoteName: remoteName,
	}
}

// NewTabEvent creates a new tab event
func NewTabEvent(eventType EventType, tabId, tabName string, data interface{}) *TabEvent {
	return &TabEvent{
		BaseEvent: BaseEvent{
			Type:      eventType,
			Timestamp: time.Now(),
			Data:      data,
		},
		TabId:   tabId,
		TabName: tabName,
	}
}

// NewErrorEvent creates a new error event
func NewErrorEvent(code, message, details, tabId string) *ErrorEvent {
	return &ErrorEvent{
		BaseEvent: BaseEvent{
			Type:      ErrorOccurred,
			Timestamp: time.Now(),
		},
		Code:    code,
		Message: message,
		Details: details,
		TabId:   tabId,
	}
}

// OperationEvent represents non-sync operation events (copy, move, check, etc.)
type OperationEvent struct {
	BaseEvent
	TabId     string `json:"tabId,omitempty"`
	Operation string `json:"operation"` // "copy", "move", "check", "dedupe", "delete", "purge"
	Status    string `json:"status"`
	Message   string `json:"message,omitempty"`
}

// NewOperationEvent creates a new operation event
func NewOperationEvent(eventType EventType, tabId, operation, status, message string) *OperationEvent {
	return &OperationEvent{
		BaseEvent: BaseEvent{
			Type:      eventType,
			Timestamp: time.Now(),
		},
		TabId:     tabId,
		Operation: operation,
		Status:    status,
		Message:   message,
	}
}

// DryRunResult contains preview of what a sync operation would do
type DryRunResult struct {
	FilesToTransfer []FileChange `json:"files_to_transfer"`
	FilesToDelete   []FileChange `json:"files_to_delete"`
	TotalBytes      int64        `json:"total_bytes"`
	TotalFiles      int          `json:"total_files"`
}

// FileChange represents a single file change in a dry-run preview
type FileChange struct {
	Path    string `json:"path"`
	Size    int64  `json:"size"`
	Action  string `json:"action"` // "copy", "delete", "move", "update"
	ModTime string `json:"mod_time"`
}

// CheckResult contains the result of a check/verify operation
type CheckResult struct {
	Matched     int        `json:"matched"`
	Differences []FileDiff `json:"differences"`
	Missing     []string   `json:"missing"`
	Errors      int        `json:"errors"`
}

// FileDiff represents a difference between source and destination files
type FileDiff struct {
	Path       string `json:"path"`
	SrcSize    int64  `json:"src_size"`
	DstSize    int64  `json:"dst_size"`
	SrcModTime string `json:"src_mod_time"`
	DstModTime string `json:"dst_mod_time"`
}

// Note: QuotaInfo, FileEntry, and ListOptions are defined in models/file_entry.go
// to avoid circular dependencies between events and models packages.

// ScheduleEvent represents schedule-related events
type ScheduleEvent struct {
	BaseEvent
	ScheduleId string `json:"scheduleId"`
}

// NewScheduleEvent creates a new schedule event
func NewScheduleEvent(eventType EventType, scheduleId string, data interface{}) *ScheduleEvent {
	return &ScheduleEvent{
		BaseEvent: BaseEvent{
			Type:      eventType,
			Timestamp: time.Now(),
			Data:      data,
		},
		ScheduleId: scheduleId,
	}
}

// HistoryEvent represents history-related events
type HistoryEvent struct {
	BaseEvent
}

// NewHistoryEvent creates a new history event
func NewHistoryEvent(eventType EventType, data interface{}) *HistoryEvent {
	return &HistoryEvent{
		BaseEvent: BaseEvent{
			Type:      eventType,
			Timestamp: time.Now(),
			Data:      data,
		},
	}
}

// BoardEvent represents board flow events
type BoardEvent struct {
	BaseEvent
	BoardId string `json:"boardId"`
	EdgeId  string `json:"edgeId,omitempty"`
	Status  string `json:"status"`
	Message string `json:"message,omitempty"`
}

// NewBoardEvent creates a new board event
func NewBoardEvent(eventType EventType, boardId, edgeId, status, message string) *BoardEvent {
	return &BoardEvent{
		BaseEvent: BaseEvent{
			Type:      eventType,
			Timestamp: time.Now(),
		},
		BoardId: boardId,
		EdgeId:  edgeId,
		Status:  status,
		Message: message,
	}
}

// CryptEvent represents encryption-related events
type CryptEvent struct {
	BaseEvent
	RemoteName string `json:"remoteName"`
}

// NewCryptEvent creates a new crypt event
func NewCryptEvent(eventType EventType, remoteName string, data interface{}) *CryptEvent {
	return &CryptEvent{
		BaseEvent: BaseEvent{
			Type:      eventType,
			Timestamp: time.Now(),
			Data:      data,
		},
		RemoteName: remoteName,
	}
}
