package delta

import (
	"time"

	"github.com/rclone/rclone/fs"
)

// ChangeType represents the kind of change detected by a watcher.
type ChangeType int

const (
	// ChangeModified indicates a file/dir was added or modified.
	// ChangeNotify does not distinguish between add and modify.
	ChangeModified ChangeType = iota
	// ChangeDeleted indicates a file/dir was removed.
	ChangeDeleted
)

// FileChange represents a single detected change on a remote.
type FileChange struct {
	Path       string       // Relative path from the remote root
	EntryType  fs.EntryType // fs.EntryDirectory or fs.EntryObject
	Type       ChangeType
	DetectedAt time.Time
}

// ChangeSet holds the accumulated changes from a watcher drain.
type ChangeSet struct {
	RemoteKey  string
	Changes    []FileChange
	HasChanges bool
}

// DeltaState represents persisted state for a sync endpoint in SQLite.
type DeltaState struct {
	RemoteKey    string
	Provider     string // "drive", "onedrive", "none"
	IsWatching   bool
	LastFullSync *time.Time
	DeltaCount   int
}
