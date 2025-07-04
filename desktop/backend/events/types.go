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
	ConfigUpdated EventType = "config:updated"
	ProfileAdded  EventType = "profile:added"
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
