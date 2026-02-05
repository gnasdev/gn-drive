package services

import (
	"context"
	"desktop/backend/events"
	"log"
	"sync"
	"time"

	"github.com/wailsapp/wails/v3/pkg/application"
)

const (
	// DefaultLogBufferCapacity is the default number of log entries to store
	DefaultLogBufferCapacity = 5000
)

// LogService provides centralized logging with reliable delivery
type LogService struct {
	app      *application.App
	buffer   *LogBuffer
	eventBus *events.WailsEventBus
	mutex    sync.RWMutex
}

// NewLogService creates a new LogService
func NewLogService() *LogService {
	return &LogService{
		buffer: NewLogBuffer(DefaultLogBufferCapacity),
	}
}

// SetApp sets the application reference for events
func (s *LogService) SetApp(app *application.App) {
	s.mutex.Lock()
	defer s.mutex.Unlock()
	s.app = app
}

// SetEventBus sets the event bus for emitting events
func (s *LogService) SetEventBus(eventBus *events.WailsEventBus) {
	s.mutex.Lock()
	defer s.mutex.Unlock()
	s.eventBus = eventBus
}

// ServiceName returns the name of the service
func (s *LogService) ServiceName() string {
	return "LogService"
}

// ServiceStartup is called when the service starts
func (s *LogService) ServiceStartup(ctx context.Context, options application.ServiceOptions) error {
	log.Printf("LogService starting up...")
	return nil
}

// ServiceShutdown is called when the service shuts down
func (s *LogService) ServiceShutdown(ctx context.Context) error {
	log.Printf("LogService shutting down...")
	return nil
}

// Log adds a log entry and emits an event to the frontend
// This is the main method that other services should use
func (s *LogService) Log(tabId, message, level string) uint64 {
	// Store in buffer first (guaranteed)
	seqNo := s.buffer.Append(tabId, message, level)

	// Emit event (best-effort)
	s.emitLogEvent(tabId, message, level, seqNo)

	return seqNo
}

// LogSync logs a sync progress message
func (s *LogService) LogSync(tabId, action, status, message string) uint64 {
	seqNo := s.buffer.Append(tabId, message, "progress")

	// Emit sync event with sequence number
	s.emitSyncEventWithSeqNo(tabId, action, status, message, seqNo)

	return seqNo
}

// emitLogEvent emits a log event to the frontend
func (s *LogService) emitLogEvent(tabId, message, level string, seqNo uint64) {
	s.mutex.RLock()
	eventBus := s.eventBus
	s.mutex.RUnlock()

	if eventBus == nil {
		return
	}

	// Create a log event with sequence number
	event := &LogEvent{
		Type:      "log:entry",
		TabId:     tabId,
		Message:   message,
		Level:     level,
		SeqNo:     seqNo,
		Timestamp: time.Now().UnixMilli(),
	}

	if err := eventBus.Emit(event); err != nil {
		log.Printf("Failed to emit log event (seqNo=%d): %v", seqNo, err)
	}
}

// emitSyncEventWithSeqNo emits a sync event with sequence number
func (s *LogService) emitSyncEventWithSeqNo(tabId, action, status, message string, seqNo uint64) {
	s.mutex.RLock()
	eventBus := s.eventBus
	s.mutex.RUnlock()

	if eventBus == nil {
		return
	}

	event := &SyncEventWithSeqNo{
		Type:      events.SyncProgress,
		TabId:     tabId,
		Action:    action,
		Status:    status,
		Message:   message,
		SeqNo:     seqNo,
		Timestamp: time.Now().UnixMilli(),
	}

	if err := eventBus.Emit(event); err != nil {
		log.Printf("Failed to emit sync event (seqNo=%d): %v", seqNo, err)
	}
}

// GetLogsSince returns all logs after the given sequence number (exposed to frontend)
func (s *LogService) GetLogsSince(ctx context.Context, tabId string, afterSeqNo uint64) ([]LogEntry, error) {
	return s.buffer.GetSince(tabId, afterSeqNo), nil
}

// GetLatestLogs returns the N most recent logs (exposed to frontend)
func (s *LogService) GetLatestLogs(ctx context.Context, tabId string, count int) ([]LogEntry, error) {
	return s.buffer.GetLatest(tabId, count), nil
}

// GetCurrentSeqNo returns the current sequence number (exposed to frontend)
func (s *LogService) GetCurrentSeqNo(ctx context.Context) (uint64, error) {
	return s.buffer.GetCurrentSeqNo(), nil
}

// ClearLogs clears logs for a specific tab or all logs if tabId is empty (exposed to frontend)
func (s *LogService) ClearLogs(ctx context.Context, tabId string) error {
	s.buffer.Clear(tabId)
	return nil
}

// GetBufferSize returns the current number of entries in the buffer
func (s *LogService) GetBufferSize(ctx context.Context) (int, error) {
	return s.buffer.Size(), nil
}

// LogEvent represents a log entry event sent to frontend
type LogEvent struct {
	Type      string `json:"type"`
	TabId     string `json:"tabId,omitempty"`
	Message   string `json:"message"`
	Level     string `json:"level"`
	SeqNo     uint64 `json:"seqNo"`
	Timestamp int64  `json:"timestamp"`
}

// SyncEventWithSeqNo is a sync event that includes sequence number
type SyncEventWithSeqNo struct {
	Type      events.EventType `json:"type"`
	TabId     string           `json:"tabId,omitempty"`
	Action    string           `json:"action"`
	Status    string           `json:"status"`
	Message   string           `json:"message,omitempty"`
	SeqNo     uint64           `json:"seqNo"`
	Timestamp int64            `json:"timestamp"`
}
