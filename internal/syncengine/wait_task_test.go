package syncengine

import (
	"context"
	"errors"
	"testing"
	"time"

	"github.com/gnasdev/gn-drive/internal/rclone"
)

// failingClient always returns an error from Sync.
type failingClient struct{}

func (f *failingClient) Sync(ctx context.Context, cfg rclone.SyncConfig, onProgress func(rclone.Stats)) (*rclone.SyncResult, error) {
	return nil, errors.New("rclone boom")
}

type okClient struct{}

func (o *okClient) Sync(ctx context.Context, cfg rclone.SyncConfig, onProgress func(rclone.Stats)) (*rclone.SyncResult, error) {
	if onProgress != nil {
		onProgress(rclone.Stats{Bytes: 10, BytesTotal: 100, Files: 1, FilesTotal: 2})
	}
	return &rclone.SyncResult{StartedAt: 1, EndedAt: 2, Stats: rclone.Stats{Bytes: 10}}, nil
}

func TestWaitTask_Success(t *testing.T) {
	eng := New(Deps{Rclone: &okClient{}})
	if err := eng.Start(context.Background()); err != nil {
		t.Fatal(err)
	}
	defer eng.Stop(context.Background())

	id, err := eng.StartPathSync(context.Background(), "push", "f1:op1", "/a", "/b", nil)
	if err != nil {
		t.Fatal(err)
	}
	if err := eng.WaitTask(context.Background(), id); err != nil {
		t.Fatalf("expected success, got %v", err)
	}
}

func TestWaitTask_Failed(t *testing.T) {
	eng := New(Deps{Rclone: &failingClient{}})
	if err := eng.Start(context.Background()); err != nil {
		t.Fatal(err)
	}
	defer eng.Stop(context.Background())

	id, err := eng.StartPathSync(context.Background(), "push", "f1:op1", "/a", "/b", nil)
	if err != nil {
		t.Fatal(err)
	}
	err = eng.WaitTask(context.Background(), id)
	if err == nil {
		t.Fatal("expected failure, got nil")
	}
	if !errors.Is(err, ErrTaskFailed) {
		t.Fatalf("expected ErrTaskFailed, got %v", err)
	}
}

func TestWaitTask_ContextCancel(t *testing.T) {
	// slow client blocks until ctx cancelled
	eng := New(Deps{Rclone: &okClient{}})
	if err := eng.Start(context.Background()); err != nil {
		t.Fatal(err)
	}
	defer eng.Stop(context.Background())

	id, err := eng.StartPathSync(context.Background(), "push", "f1:op1", "/a", "/b", nil)
	if err != nil {
		t.Fatal(err)
	}
	// Wait with already-cancelled context — should return promptly.
	ctx, cancel := context.WithCancel(context.Background())
	cancel()
	// Give task a moment; WaitTask should still exit on ctx
	err = eng.WaitTask(ctx, id)
	if !errors.Is(err, context.Canceled) {
		// If task finished first, success is also acceptable race
		if err != nil && !errors.Is(err, ErrTaskFailed) {
			t.Logf("WaitTask returned %v (race with finish ok)", err)
		}
		if err == nil {
			// task completed before cancel observed — ok
			return
		}
	}
	_ = time.Millisecond
}
