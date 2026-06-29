package eventbus

import (
	"context"
	"sync"
	"testing"
	"time"
)

// TestBus_ConcurrentPublishAndCloseNoPanic verifies that closing the bus while
// other goroutines are publishing never panics with "send on closed channel".
// Run under -race to also catch data races on the topic map / closed flag.
func TestBus_ConcurrentPublishAndCloseNoPanic(t *testing.T) {
	b := NewBus(context.Background())

	// A subscriber whose buffer fills (it reads slowly) so Publish exercises
	// the drop-oldest path concurrently with Close.
	b.Subscribe("race:topic", func(ev Event) {
		time.Sleep(time.Millisecond)
	})

	var wg sync.WaitGroup
	for g := 0; g < 8; g++ {
		wg.Add(1)
		go func() {
			defer wg.Done()
			defer func() {
				if r := recover(); r != nil {
					t.Errorf("Publish panicked: %v", r)
				}
			}()
			for i := 0; i < 500; i++ {
				b.Publish("race:topic", newTestEvent("e"))
			}
		}()
	}

	// Close while publishers are still running.
	time.Sleep(2 * time.Millisecond)
	if err := b.Close(); err != nil {
		t.Fatalf("Close: %v", err)
	}
	// Double close must be safe.
	if err := b.Close(); err != nil {
		t.Fatalf("second Close: %v", err)
	}

	wg.Wait()

	// Publishing after close is a safe no-op.
	b.Publish("race:topic", newTestEvent("after-close"))
}

// TestBus_DropOldestDeliversNewest verifies that when a subscriber's buffer is
// saturated, a later (e.g. terminal) event still gets enqueued by evicting the
// oldest, rather than the newest being dropped.
func TestBus_DropOldestDeliversNewest(t *testing.T) {
	b := NewBus(context.Background())
	defer b.Close()

	release := make(chan struct{})
	got := make(chan string, subscriberBuffer*4)
	first := make(chan struct{}, 1)

	b.Subscribe("dq", func(ev Event) {
		// Block on the very first event so the buffer fills behind it.
		if te, ok := ev.(TestEvent); ok && te.Payload == "block" {
			select {
			case first <- struct{}{}:
			default:
			}
			<-release
		}
		got <- ev.(TestEvent).Payload
	})

	// Kick off the blocking event and wait until the handler is parked.
	b.Publish("dq", newTestEvent("block"))
	select {
	case <-first:
	case <-time.After(2 * time.Second):
		t.Fatal("handler never started")
	}

	// Saturate the buffer with filler, then publish the important terminal
	// event last. With drop-oldest, the terminal event evicts an old filler
	// and survives in the buffer.
	for i := 0; i < subscriberBuffer*3; i++ {
		b.Publish("dq", newTestEvent("filler"))
	}
	b.Publish("dq", newTestEvent("TERMINAL"))

	// Let the handler drain.
	close(release)

	deadline := time.After(2 * time.Second)
	for {
		select {
		case p := <-got:
			if p == "TERMINAL" {
				return // success: newest event was delivered
			}
		case <-deadline:
			t.Fatal("TERMINAL event was dropped — drop-oldest did not preserve newest")
		}
	}
}
