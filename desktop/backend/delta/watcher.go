package delta

import (
	"context"
	"log"
	"sync"
	"time"

	"github.com/rclone/rclone/fs"
)

// Watcher wraps a single remote's ChangeNotify to collect changes in the background.
type Watcher struct {
	remoteKey string
	remoteFs  fs.Fs
	pollCh    chan time.Duration
	changes   []FileChange
	mu        sync.Mutex
	running   bool
	ctx       context.Context
	cancel    context.CancelFunc
}

// NewWatcher creates a watcher for a remote filesystem.
func NewWatcher(remoteKey string, remoteFs fs.Fs) *Watcher {
	return &Watcher{
		remoteKey: remoteKey,
		remoteFs:  remoteFs,
	}
}

// Start begins ChangeNotify polling in a background goroutine.
func (w *Watcher) Start(parentCtx context.Context, pollInterval time.Duration) {
	w.mu.Lock()

	if w.running {
		w.mu.Unlock()
		return
	}

	features := w.remoteFs.Features()
	if features.ChangeNotify == nil {
		w.mu.Unlock()
		log.Printf("[delta-watcher] %s: ChangeNotify not supported, skipping", w.remoteKey)
		return
	}

	w.ctx, w.cancel = context.WithCancel(parentCtx)
	w.pollCh = make(chan time.Duration, 1)
	w.changes = nil
	w.running = true

	// Start ChangeNotify — it spawns its own goroutine internally
	features.ChangeNotify(w.ctx, w.notifyCallback, w.pollCh)

	// Release lock before channel send to avoid holding mutex during potential block
	pollCh := w.pollCh
	w.mu.Unlock()

	// Send the initial poll interval (outside mutex)
	pollCh <- pollInterval

	log.Printf("[delta-watcher] %s: started with poll interval %v", w.remoteKey, pollInterval)
}

// notifyCallback is called by ChangeNotify for each detected change.
func (w *Watcher) notifyCallback(path string, entryType fs.EntryType) {
	w.mu.Lock()
	defer w.mu.Unlock()

	w.changes = append(w.changes, FileChange{
		Path:       path,
		EntryType:  entryType,
		Type:       ChangeModified,
		DetectedAt: time.Now(),
	})
}

// HasChanges returns true if any changes have been collected since the last drain.
func (w *Watcher) HasChanges() bool {
	w.mu.Lock()
	defer w.mu.Unlock()
	return len(w.changes) > 0
}

// DrainChanges returns and clears all collected changes atomically.
func (w *Watcher) DrainChanges() []FileChange {
	w.mu.Lock()
	defer w.mu.Unlock()

	if len(w.changes) == 0 {
		return nil
	}

	changes := w.changes
	w.changes = nil
	return changes
}

// RestoreChanges prepends previously drained changes back into the buffer.
// Used when a scoped delta sync fails, so changes are not lost.
func (w *Watcher) RestoreChanges(changes []FileChange) {
	if len(changes) == 0 {
		return
	}

	w.mu.Lock()
	defer w.mu.Unlock()

	// Prepend restored changes before any new ones that arrived since the drain
	w.changes = append(changes, w.changes...)
}

// IsRunning returns whether the watcher is currently active.
func (w *Watcher) IsRunning() bool {
	w.mu.Lock()
	defer w.mu.Unlock()
	return w.running
}

// Stop closes the poll channel and cancels the context,
// which signals the ChangeNotify goroutine to exit.
func (w *Watcher) Stop() {
	w.mu.Lock()
	defer w.mu.Unlock()

	if !w.running {
		return
	}

	w.running = false

	// Close the poll channel to signal ChangeNotify to stop
	if w.pollCh != nil {
		close(w.pollCh)
		w.pollCh = nil
	}

	// Cancel the context
	if w.cancel != nil {
		w.cancel()
		w.cancel = nil
	}

	log.Printf("[delta-watcher] %s: stopped", w.remoteKey)
}
