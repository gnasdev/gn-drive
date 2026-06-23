// Package syncengine provides the sync orchestration engine.
package syncengine

import (
	"context"
	"sync"
	"time"

	"github.com/gnasdev/gn-drive/internal/rclone"
)

// Task represents a running or completed sync task.
type Task struct {
	ID       string
	Name     string // profile name
	Action   string
	Status   string // running | completed | failed | cancelled
	ctx      context.Context
	cancel   context.CancelFunc
	Mu       sync.Mutex
	Stats    rclone.Stats
	StartedAt time.Time
	EndedAt  time.Time
}

func (t *Task) Cancel() {
	if t.cancel != nil {
		t.cancel()
		t.Mu.Lock()
		t.Status = "cancelled"
		t.Mu.Unlock()
	}
}

// TaskSnapshot is an immutable view of a Task safe to copy by value and
// to send across goroutines. Use this in API responses, event payloads,
// and anywhere the receiver must not hold a pointer to live state.
type TaskSnapshot struct {
	ID        string      `json:"id"`
	Name      string      `json:"name"`
	Action    string      `json:"action"`
	Status    string      `json:"status"`
	Stats     rclone.Stats `json:"stats"`
	StartedAt time.Time   `json:"started_at"`
	EndedAt   time.Time   `json:"ended_at,omitempty"`
}

// Snapshot returns a copy of the task with the mutex-protected fields
// read under the lock. The returned TaskSnapshot is decoupled from the
// live Task; subsequent mutations to the source Task are not visible.
func (t *Task) Snapshot() TaskSnapshot {
	t.Mu.Lock()
	defer t.Mu.Unlock()
	return TaskSnapshot{
		ID:        t.ID,
		Name:      t.Name,
		Action:    t.Action,
		Status:    t.Status,
		Stats:     t.Stats,
		StartedAt: t.StartedAt,
		EndedAt:   t.EndedAt,
	}
}
