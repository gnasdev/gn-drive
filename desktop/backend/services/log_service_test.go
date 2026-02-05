package services

import (
	"context"
	"testing"
)

func TestNewLogService(t *testing.T) {
	svc := NewLogService()

	if svc == nil {
		t.Fatal("expected non-nil service")
	}
	if svc.buffer == nil {
		t.Fatal("expected non-nil buffer")
	}
}

func TestLogServiceLog(t *testing.T) {
	svc := NewLogService()

	seqNo1 := svc.Log("tab1", "message1", "info")
	if seqNo1 != 1 {
		t.Errorf("expected seqNo 1, got %d", seqNo1)
	}

	seqNo2 := svc.Log("tab1", "message2", "error")
	if seqNo2 != 2 {
		t.Errorf("expected seqNo 2, got %d", seqNo2)
	}

	// Verify stored in buffer
	ctx := context.Background()
	logs, err := svc.GetLogsSince(ctx, "", 0)
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if len(logs) != 2 {
		t.Errorf("expected 2 logs, got %d", len(logs))
	}
}

func TestLogServiceLogSync(t *testing.T) {
	svc := NewLogService()

	seqNo := svc.LogSync("tab1", "pull", "running", "Syncing file.txt")
	if seqNo != 1 {
		t.Errorf("expected seqNo 1, got %d", seqNo)
	}

	// Verify stored with correct level
	ctx := context.Background()
	logs, err := svc.GetLogsSince(ctx, "tab1", 0)
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if len(logs) != 1 {
		t.Fatalf("expected 1 log, got %d", len(logs))
	}
	if logs[0].Level != "progress" {
		t.Errorf("expected level 'progress', got '%s'", logs[0].Level)
	}
	if logs[0].Message != "Syncing file.txt" {
		t.Errorf("expected message 'Syncing file.txt', got '%s'", logs[0].Message)
	}
}

func TestLogServiceGetLogsSince(t *testing.T) {
	svc := NewLogService()
	ctx := context.Background()

	svc.Log("tab1", "msg1", "info")
	svc.Log("tab2", "msg2", "info")
	svc.Log("tab1", "msg3", "info")

	// Get all
	all, err := svc.GetLogsSince(ctx, "", 0)
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if len(all) != 3 {
		t.Errorf("expected 3 logs, got %d", len(all))
	}

	// Get for specific tab
	tab1Logs, err := svc.GetLogsSince(ctx, "tab1", 0)
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if len(tab1Logs) != 2 {
		t.Errorf("expected 2 logs for tab1, got %d", len(tab1Logs))
	}

	// Get after specific seqNo
	afterOne, err := svc.GetLogsSince(ctx, "", 1)
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if len(afterOne) != 2 {
		t.Errorf("expected 2 logs after seqNo 1, got %d", len(afterOne))
	}
}

func TestLogServiceGetLatestLogs(t *testing.T) {
	svc := NewLogService()
	ctx := context.Background()

	for i := 0; i < 10; i++ {
		svc.Log("tab1", "msg", "info")
	}

	// Get latest 3
	latest, err := svc.GetLatestLogs(ctx, "", 3)
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if len(latest) != 3 {
		t.Errorf("expected 3 logs, got %d", len(latest))
	}

	// Verify they are the last 3
	expectedSeqNos := []uint64{8, 9, 10}
	for i, log := range latest {
		if log.SeqNo != expectedSeqNos[i] {
			t.Errorf("expected seqNo %d, got %d", expectedSeqNos[i], log.SeqNo)
		}
	}
}

func TestLogServiceGetCurrentSeqNo(t *testing.T) {
	svc := NewLogService()
	ctx := context.Background()

	seqNo, err := svc.GetCurrentSeqNo(ctx)
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if seqNo != 0 {
		t.Errorf("expected initial seqNo 0, got %d", seqNo)
	}

	svc.Log("tab1", "msg1", "info")
	svc.Log("tab1", "msg2", "info")

	seqNo, err = svc.GetCurrentSeqNo(ctx)
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if seqNo != 2 {
		t.Errorf("expected seqNo 2, got %d", seqNo)
	}
}

func TestLogServiceClearLogs(t *testing.T) {
	svc := NewLogService()
	ctx := context.Background()

	svc.Log("tab1", "msg1", "info")
	svc.Log("tab2", "msg2", "info")
	svc.Log("tab1", "msg3", "info")

	// Clear tab1
	err := svc.ClearLogs(ctx, "tab1")
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}

	remaining, _ := svc.GetLogsSince(ctx, "", 0)
	if len(remaining) != 1 {
		t.Errorf("expected 1 log after clearing tab1, got %d", len(remaining))
	}

	// Clear all
	svc.Log("tab1", "msg4", "info")
	err = svc.ClearLogs(ctx, "")
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}

	size, _ := svc.GetBufferSize(ctx)
	if size != 0 {
		t.Errorf("expected 0 logs after clear all, got %d", size)
	}
}

func TestLogServiceGetBufferSize(t *testing.T) {
	svc := NewLogService()
	ctx := context.Background()

	size, err := svc.GetBufferSize(ctx)
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if size != 0 {
		t.Errorf("expected initial size 0, got %d", size)
	}

	svc.Log("tab1", "msg1", "info")
	svc.Log("tab1", "msg2", "info")
	svc.Log("tab1", "msg3", "info")

	size, err = svc.GetBufferSize(ctx)
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if size != 3 {
		t.Errorf("expected size 3, got %d", size)
	}
}

func TestLogServiceServiceName(t *testing.T) {
	svc := NewLogService()
	if svc.ServiceName() != "LogService" {
		t.Errorf("expected service name 'LogService', got '%s'", svc.ServiceName())
	}
}

func TestLogServiceWithoutEventBus(t *testing.T) {
	svc := NewLogService()
	// Without EventBus set, Log should still work (store in buffer, skip emit)

	seqNo := svc.Log("tab1", "message", "info")
	if seqNo != 1 {
		t.Errorf("expected seqNo 1, got %d", seqNo)
	}

	seqNo = svc.LogSync("tab1", "pull", "running", "message")
	if seqNo != 2 {
		t.Errorf("expected seqNo 2, got %d", seqNo)
	}

	// Verify logs are in buffer
	ctx := context.Background()
	logs, _ := svc.GetLogsSince(ctx, "", 0)
	if len(logs) != 2 {
		t.Errorf("expected 2 logs in buffer, got %d", len(logs))
	}
}
