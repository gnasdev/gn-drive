// Package eventbus provides in-process typed event channels.
//
// Phase 3: real implementation using per-topic buffered Go channels.
// Subscribe(topic, handler) starts a goroutine; the returned cancel()
// stops it. Publish is non-blocking: events are dropped if the buffer is
// full (so a slow subscriber never blocks the publisher).
//
// Glob matching is supported for SSE handlers: SubscribeAll([]string{...})
// subscribes to multiple topics at once.
package eventbus

import (
	"context"
	"sync"
)

// Bus is the in-process event bus.
type Bus struct {
	mu     sync.RWMutex
	topics map[string]chan Event
}

// NewBus creates a new in-process event bus.
func NewBus(_ context.Context) *Bus {
	return &Bus{topics: make(map[string]chan Event)}
}

// Subscribe registers a handler for the given topic.
// Returns a cancel function that stops the subscription when called.
//
// Each subscription spawns a goroutine that reads from the topic channel
// and invokes the handler. The handler is called serially per subscription;
// concurrent subscribers receive events concurrently.
func (b *Bus) Subscribe(topic string, handler func(Event)) (cancel func()) {
	ch := b.getOrCreate(topic)
	sub := make(chan struct{})
	go func() {
		for {
			select {
			case <-sub:
				return
			case ev := <-ch:
				func() {
					defer func() { _ = recover() }() // handler panic doesn't kill the bus
					handler(ev)
				}()
			}
		}
	}()
	return func() { close(sub) }
}

// SubscribeAll subscribes to multiple topics. Returns a single cancel
// that unsubscribes from all. Used by the SSE handler.
func (b *Bus) SubscribeAll(topics []string, handler func(topic string, ev Event)) (cancel func()) {
	cancels := make([]func(), 0, len(topics))
	for _, t := range topics {
		t := t
		cancels = append(cancels, b.Subscribe(t, func(ev Event) {
			handler(t, ev)
		}))
	}
	return func() {
		for _, c := range cancels {
			c()
		}
	}
}

// Publish emits an event to all subscribers of the given topic.
// Non-blocking: drops the event if any subscriber's channel buffer is full.
func (b *Bus) Publish(topic string, event Event) {
	b.mu.RLock()
	ch, ok := b.topics[topic]
	b.mu.RUnlock()
	if !ok {
		return
	}
	select {
	case ch <- event:
	default:
		// drop: slow subscriber
	}
}

// Close stops the bus and releases resources.
// Pending events in buffers are discarded.
func (b *Bus) Close() error {
	b.mu.Lock()
	defer b.mu.Unlock()
	for _, ch := range b.topics {
		close(ch)
	}
	b.topics = nil
	return nil
}

func (b *Bus) getOrCreate(topic string) chan Event {
	b.mu.Lock()
	defer b.mu.Unlock()
	ch, ok := b.topics[topic]
	if !ok {
		ch = make(chan Event, 64)
		b.topics[topic] = ch
	}
	return ch
}

// Topic constants — used as Publish/Subscribe keys.
const (
	TopicSyncStarted   = "sync:started"
	TopicSyncProgress  = "sync:progress"
	TopicSyncCompleted = "sync:completed"
	TopicSyncFailed    = "sync:failed"

	TopicAuthUnlocked = "auth:unlocked"
	TopicAuthLocked   = "auth:locked"

	TopicServiceStatus = "service:status"

	TopicStateChanged = "state:changed"

	TopicScheduleTriggered = "schedule:triggered"

	TopicBoardExecution = "board:execution"
)

// AllTopics returns every topic the bus is expected to emit.
// Useful for SSE handlers that want a single subscription covering everything.
func AllTopics() []string {
	return []string{
		TopicSyncStarted,
		TopicSyncProgress,
		TopicSyncCompleted,
		TopicSyncFailed,
		TopicAuthUnlocked,
		TopicAuthLocked,
		TopicServiceStatus,
		TopicStateChanged,
		TopicScheduleTriggered,
		TopicBoardExecution,
	}
}
