package delta

import (
	"context"
	"log"
	"sync"
	"time"

	"github.com/rclone/rclone/fs"
)

const (
	// MaxDeltaSyncsBeforeFullSync forces a full sync after this many consecutive delta syncs.
	MaxDeltaSyncsBeforeFullSync = 50

	// MaxTimeBetweenFullSyncs forces a full sync after this duration.
	MaxTimeBetweenFullSyncs = 24 * time.Hour

	// DefaultPollInterval is the default ChangeNotify poll interval.
	DefaultPollInterval = 1 * time.Minute

	// MaxChangesBeforeFallback triggers a full sync instead of filter-scoped delta.
	MaxChangesBeforeFallback = 5000
)

// DeltaService manages delta watchers for all configured remotes.
type DeltaService struct {
	store    *DeltaStore
	watchers map[string]*Watcher
	mu       sync.RWMutex
	ctx      context.Context
	cancel   context.CancelFunc
}

// NewDeltaService creates a new DeltaService.
func NewDeltaService(store *DeltaStore) *DeltaService {
	ctx, cancel := context.WithCancel(context.Background())
	return &DeltaService{
		store:    store,
		watchers: make(map[string]*Watcher),
		ctx:      ctx,
		cancel:   cancel,
	}
}

// getProviderType returns the provider type string for a filesystem,
// or "none" if it doesn't support ChangeNotify.
func getProviderType(remoteFs fs.Fs) string {
	if remoteFs.Features().ChangeNotify != nil {
		return remoteFs.Name()
	}
	return "none"
}

// EnsureWatcher starts a watcher for the remote if the backend supports ChangeNotify
// and no watcher is already running. Called after each successful full sync.
func (d *DeltaService) EnsureWatcher(remoteFs fs.Fs, remoteKey string) error {
	provider := getProviderType(remoteFs)
	if provider == "none" {
		return nil
	}

	d.mu.Lock()
	defer d.mu.Unlock()

	// Check if watcher already running
	if w, ok := d.watchers[remoteKey]; ok && w.IsRunning() {
		return nil
	}

	// Create and start watcher
	w := NewWatcher(remoteKey, remoteFs)
	w.Start(d.ctx, DefaultPollInterval)
	d.watchers[remoteKey] = w

	// Update DB
	if err := d.store.SetWatching(remoteKey, true); err != nil {
		log.Printf("[delta] Failed to update watching state for %s: %v", remoteKey, err)
	}

	return nil
}

// ShouldSkipSync returns true if the watcher for this remote reports 0 changes
// and conditions are met for a delta sync (watcher running, not too many
// consecutive deltas, not too long since last full sync).
// Returns false (meaning do full sync) if any condition is not met.
func (d *DeltaService) ShouldSkipSync(remoteKey string) bool {
	d.mu.RLock()
	w, exists := d.watchers[remoteKey]
	d.mu.RUnlock()

	// No watcher → can't determine, do full sync
	if !exists || !w.IsRunning() {
		return false
	}

	// Check state for periodic full sync requirements
	state, err := d.store.GetState(remoteKey)
	if err != nil || state == nil {
		return false
	}

	// Force full sync after too many consecutive delta syncs
	if state.DeltaCount >= MaxDeltaSyncsBeforeFullSync {
		log.Printf("[delta] %s: forcing full sync after %d consecutive delta syncs", remoteKey, state.DeltaCount)
		return false
	}

	// Force full sync after too long since last full sync
	if state.LastFullSync != nil && time.Since(*state.LastFullSync) > MaxTimeBetweenFullSyncs {
		log.Printf("[delta] %s: forcing full sync, last full sync was %v ago", remoteKey, time.Since(*state.LastFullSync))
		return false
	}

	// Watcher reports no changes → safe to skip
	return !w.HasChanges()
}

// GetChanges drains changes from the watcher for filter scoping.
// Returns nil if no watcher, no changes, or too many changes.
func (d *DeltaService) GetChanges(remoteKey string) *ChangeSet {
	d.mu.RLock()
	w, exists := d.watchers[remoteKey]
	d.mu.RUnlock()

	if !exists || !w.IsRunning() {
		return nil
	}

	changes := w.DrainChanges()
	if len(changes) == 0 {
		return &ChangeSet{
			RemoteKey:  remoteKey,
			HasChanges: false,
		}
	}

	return &ChangeSet{
		RemoteKey:  remoteKey,
		Changes:    changes,
		HasChanges: true,
	}
}

// RestoreChanges puts previously drained changes back into the watcher buffer.
// Call this when a scoped delta sync fails so the changes are not lost.
func (d *DeltaService) RestoreChanges(remoteKey string, changes []FileChange) {
	d.mu.RLock()
	w, exists := d.watchers[remoteKey]
	d.mu.RUnlock()

	if !exists || !w.IsRunning() {
		return
	}
	w.RestoreChanges(changes)
}

// CommitDelta records a successful delta sync (increments counter).
func (d *DeltaService) CommitDelta(remoteKey string) error {
	return d.store.IncrementDeltaCount(remoteKey)
}

// CommitFullSync records a full sync completion and ensures a watcher is running.
func (d *DeltaService) CommitFullSync(remoteFs fs.Fs, remoteKey string) error {
	provider := getProviderType(remoteFs)
	isWatching := false

	// Start watcher if provider supports it
	if provider != "none" {
		if err := d.EnsureWatcher(remoteFs, remoteKey); err != nil {
			log.Printf("[delta] Failed to start watcher for %s: %v", remoteKey, err)
		} else {
			isWatching = true
		}
	}

	return d.store.RecordFullSync(remoteKey, provider, isWatching)
}

// StopAll stops all watchers. Called on app shutdown.
func (d *DeltaService) StopAll() {
	d.mu.Lock()
	defer d.mu.Unlock()

	for key, w := range d.watchers {
		w.Stop()
		if err := d.store.SetWatching(key, false); err != nil {
			log.Printf("[delta] Failed to update watching state on stop for %s: %v", key, err)
		}
	}
	d.watchers = make(map[string]*Watcher)

	if d.cancel != nil {
		d.cancel()
	}

	log.Printf("[delta] All watchers stopped")
}
