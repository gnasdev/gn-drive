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
