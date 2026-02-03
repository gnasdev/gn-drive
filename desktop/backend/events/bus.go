package events

import (
	"encoding/json"
	"log"
	"sync"

	"github.com/wailsapp/wails/v3/pkg/application"
)

// EventBus provides a unified interface for event emission
// All services should use this to emit events to the frontend
type EventBus interface {
	// Emit sends an event to the frontend via the unified "tofe" channel
	Emit(event interface{}) error
	// EmitWithType sends an event with explicit type to the frontend
	EmitWithType(eventType EventType, event interface{}) error
}

// WailsEventBus implements EventBus using Wails event system
type WailsEventBus struct {
	app   *application.App
	mutex sync.RWMutex
}

// NewEventBus creates a new WailsEventBus
func NewEventBus(app *application.App) *WailsEventBus {
	return &WailsEventBus{
		app: app,
	}
}

// SetApp sets the application reference
func (b *WailsEventBus) SetApp(app *application.App) {
	b.mutex.Lock()
	defer b.mutex.Unlock()
	b.app = app
}

// Emit sends an event to the frontend via the unified "tofe" channel
// The event should have a Type field that identifies the event type
func (b *WailsEventBus) Emit(event interface{}) error {
	b.mutex.RLock()
	defer b.mutex.RUnlock()

	if b.app == nil {
		log.Printf("EventBus: app is nil, cannot emit event")
		return nil
	}

	// Serialize event to JSON
	jsonData, err := json.Marshal(event)
	if err != nil {
		log.Printf("EventBus: failed to marshal event: %v", err)
		return err
	}

	// Emit through unified "tofe" channel
	b.app.Event.Emit("tofe", string(jsonData))
	return nil
}

// EmitWithType wraps data in a typed event envelope and emits
func (b *WailsEventBus) EmitWithType(eventType EventType, data interface{}) error {
	envelope := struct {
		Type      EventType   `json:"type"`
		Timestamp int64       `json:"timestamp"`
		Data      interface{} `json:"data"`
	}{
		Type: eventType,
		Data: data,
	}
	return b.Emit(envelope)
}

// EmitSyncEvent is a convenience method for sync events
func (b *WailsEventBus) EmitSyncEvent(event *SyncEvent) error {
	return b.Emit(event)
}

// EmitConfigEvent is a convenience method for config events
func (b *WailsEventBus) EmitConfigEvent(event *ConfigEvent) error {
	return b.Emit(event)
}

// EmitRemoteEvent is a convenience method for remote events
func (b *WailsEventBus) EmitRemoteEvent(event *RemoteEvent) error {
	return b.Emit(event)
}

// EmitTabEvent is a convenience method for tab events
func (b *WailsEventBus) EmitTabEvent(event *TabEvent) error {
	return b.Emit(event)
}

// EmitErrorEvent is a convenience method for error events
func (b *WailsEventBus) EmitErrorEvent(event *ErrorEvent) error {
	return b.Emit(event)
}

// EmitOperationEvent is a convenience method for operation events
func (b *WailsEventBus) EmitOperationEvent(event *OperationEvent) error {
	return b.Emit(event)
}

// EmitScheduleEvent is a convenience method for schedule events
func (b *WailsEventBus) EmitScheduleEvent(event *ScheduleEvent) error {
	return b.Emit(event)
}

// EmitHistoryEvent is a convenience method for history events
func (b *WailsEventBus) EmitHistoryEvent(event *HistoryEvent) error {
	return b.Emit(event)
}

// EmitCryptEvent is a convenience method for crypt events
func (b *WailsEventBus) EmitCryptEvent(event *CryptEvent) error {
	return b.Emit(event)
}
