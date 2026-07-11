// Package eventbus provides in-process typed event channels.
//
// Each Subscribe(topic, handler) gets its OWN buffered channel and goroutine,
// so Publish fans out (broadcasts) a copy of the event to every subscriber of
// the topic — not a single competing consumer. The returned cancel() stops
// that one subscription.
//
// Publish is non-blocking: if a subscriber's buffer is full it drops the
// OLDEST queued event to make room for the newest, so a slow subscriber never
// blocks the publisher and recent/terminal events are favoured over stale
// backlog.
//
// Close is concurrency-safe with Publish: it never closes the event channels
// publishers write to (it only signals reader goroutines to stop), so a
// concurrent Publish can never panic with "send on closed channel".
//
// Glob-free multi-topic subscription is supported via SubscribeAll, used by
// the SSE handler.
package eventbus

import (
	"context"
	"sync"
)

// subscriberBuffer is the per-subscriber channel capacity.
const subscriberBuffer = 64

// subscriber is a single subscription: its own buffered channel plus a done
// signal used to stop the reader goroutine exactly once.
type subscriber struct {
	ch        chan Event
	done      chan struct{}
	closeOnce sync.Once
}

// stop signals the reader goroutine to exit. Idempotent and safe to call from
// both cancel() and Bus.Close().
func (s *subscriber) stop() {
	s.closeOnce.Do(func() { close(s.done) })
}

// send delivers an event to the subscriber without blocking. On a full buffer
// it evicts the oldest event and enqueues the newest.
func (s *subscriber) send(ev Event) {
	select {
	case s.ch <- ev:
		return
	default:
	}
	// Buffer full: drop the oldest to favour the newest event.
	select {
	case <-s.ch:
	default:
	}
	select {
	case s.ch <- ev:
	default:
	}
}

// Bus is the in-process event bus.
type Bus struct {
	mu     sync.RWMutex
	topics map[string][]*subscriber
	closed bool
}

// NewBus creates a new in-process event bus.
func NewBus(_ context.Context) *Bus {
	return &Bus{topics: make(map[string][]*subscriber)}
}

// Subscribe registers a handler for the given topic and returns a cancel
// function that stops the subscription.
//
// Each subscription has its own goroutine and buffered channel; the handler is
// invoked serially per subscription, and every subscriber of a topic receives
// every published event (fan-out).
func (b *Bus) Subscribe(topic string, handler func(Event)) (cancel func()) {
	sub := &subscriber{
		ch:   make(chan Event, subscriberBuffer),
		done: make(chan struct{}),
	}

	b.mu.Lock()
	if b.closed {
		b.mu.Unlock()
		// Bus already closed: no goroutine, no-op cancel.
		return func() {}
	}
	b.topics[topic] = append(b.topics[topic], sub)
	b.mu.Unlock()

	go func() {
		for {
			select {
			case <-sub.done:
				return
			case ev := <-sub.ch:
				func() {
					defer func() { _ = recover() }() // handler panic doesn't kill the bus
					handler(ev)
				}()
			}
		}
	}()

	return func() {
		b.mu.Lock()
		if subs, ok := b.topics[topic]; ok {
			for i, s := range subs {
				if s == sub {
					b.topics[topic] = append(subs[:i], subs[i+1:]...)
					break
				}
			}
		}
		b.mu.Unlock()
		sub.stop()
	}
}

// SubscribeAll subscribes to multiple topics. Returns a single cancel that
// unsubscribes from all. Used by the SSE handler.
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

// Publish broadcasts an event to every subscriber of the given topic.
// Non-blocking: a full subscriber buffer drops its oldest event (see
// subscriber.send). No-op once the bus is closed.
func (b *Bus) Publish(topic string, event Event) {
	if b == nil {
		return
	}
	b.mu.RLock()
	defer b.mu.RUnlock()
	if b.closed {
		return
	}
	for _, sub := range b.topics[topic] {
		sub.send(event)
	}
}

// Close stops the bus and all subscriber goroutines. Pending buffered events
// are discarded. Safe to call concurrently with Publish and more than once.
func (b *Bus) Close() error {
	b.mu.Lock()
	if b.closed {
		b.mu.Unlock()
		return nil
	}
	b.closed = true
	subs := b.topics
	b.topics = nil
	b.mu.Unlock()

	// Stop reader goroutines. We close each subscriber's `done` channel — not
	// the event channels — so a concurrent Publish (which only sends to event
	// channels, under RLock with the closed-guard above) can never send on a
	// closed channel.
	for _, list := range subs {
		for _, s := range list {
			s.stop()
		}
	}
	return nil
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
	TopicFlowExecution  = "flow:execution"
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
		TopicFlowExecution,
	}
}
