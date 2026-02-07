package services

import (
	"context"
	"desktop/backend/models"
	"fmt"
	"testing"
	"time"
)

func newTestHistoryService(t *testing.T) *HistoryService {
	t.Helper()
	// Clean DB table for test isolation
	db, _ := GetSharedDB()
	db.Exec("DELETE FROM history")
	return &HistoryService{
		initialized: true,
	}
}

func TestHistoryService_AddEntry(t *testing.T) {
	h := newTestHistoryService(t)
	ctx := context.Background()

	entry := models.HistoryEntry{
		Id:               "test-1",
		ProfileName:      "my-profile",
		Action:           "push",
		Status:           "completed",
		StartTime:        time.Now().Add(-5 * time.Minute),
		EndTime:          time.Now(),
		Duration:         "5m0s",
		FilesTransferred: 10,
		BytesTransferred: 1024,
	}

	err := h.AddEntry(ctx, entry)
	if err != nil {
		t.Fatalf("AddEntry failed: %v", err)
	}

	entries, err := h.GetHistory(ctx, 10, 0)
	if err != nil {
		t.Fatalf("GetHistory failed: %v", err)
	}
	if len(entries) != 1 {
		t.Fatalf("expected 1 entry, got %d", len(entries))
	}
	if entries[0].Id != "test-1" {
		t.Errorf("expected entry id 'test-1', got %q", entries[0].Id)
	}
}

func TestHistoryService_MaxEntries(t *testing.T) {
	h := newTestHistoryService(t)
	ctx := context.Background()

	// Add more than maxHistoryEntries
	for i := 0; i < maxHistoryEntries+10; i++ {
		entry := models.HistoryEntry{
			Id:          fmt.Sprintf("entry-%d", i),
			ProfileName: "test",
			Action:      "push",
			Status:      "completed",
			StartTime:   time.Now(),
			EndTime:     time.Now(),
			Duration:    "1s",
		}
		if err := h.AddEntry(ctx, entry); err != nil {
			t.Fatalf("AddEntry %d failed: %v", i, err)
		}
	}

	entries, err := h.GetHistory(ctx, maxHistoryEntries+100, 0)
	if err != nil {
		t.Fatalf("GetHistory failed: %v", err)
	}
	if len(entries) != maxHistoryEntries {
		t.Errorf("expected %d entries (capped), got %d", maxHistoryEntries, len(entries))
	}
}

func TestHistoryService_GetHistoryForProfile(t *testing.T) {
	h := newTestHistoryService(t)
	ctx := context.Background()

	profiles := []string{"profile-a", "profile-b", "profile-a"}
	for i, name := range profiles {
		entry := models.HistoryEntry{
			Id:          fmt.Sprintf("entry-%d", i),
			ProfileName: name,
			Action:      "push",
			Status:      "completed",
			StartTime:   time.Now(),
			EndTime:     time.Now(),
			Duration:    "1s",
		}
		if err := h.AddEntry(ctx, entry); err != nil {
			t.Fatalf("AddEntry failed: %v", err)
		}
	}

	entries, err := h.GetHistoryForProfile(ctx, "profile-a")
	if err != nil {
		t.Fatalf("GetHistoryForProfile failed: %v", err)
	}
	if len(entries) != 2 {
		t.Errorf("expected 2 entries for profile-a, got %d", len(entries))
	}
}

func TestHistoryService_ClearHistory(t *testing.T) {
	h := newTestHistoryService(t)
	ctx := context.Background()

	entry := models.HistoryEntry{
		Id:          "to-clear",
		ProfileName: "test",
		Action:      "push",
		Status:      "completed",
		StartTime:   time.Now(),
		EndTime:     time.Now(),
		Duration:    "1s",
	}
	if err := h.AddEntry(ctx, entry); err != nil {
		t.Fatalf("AddEntry failed: %v", err)
	}

	if err := h.ClearHistory(ctx); err != nil {
		t.Fatalf("ClearHistory failed: %v", err)
	}

	entries, err := h.GetHistory(ctx, 10, 0)
	if err != nil {
		t.Fatalf("GetHistory failed: %v", err)
	}
	if len(entries) != 0 {
		t.Errorf("expected 0 entries after clear, got %d", len(entries))
	}
}

func TestHistoryService_GetStats(t *testing.T) {
	h := newTestHistoryService(t)
	ctx := context.Background()

	entries := []models.HistoryEntry{
		{Id: "1", Status: "completed", Duration: "10s", FilesTransferred: 5, BytesTransferred: 100},
		{Id: "2", Status: "failed", Duration: "5s", FilesTransferred: 0, BytesTransferred: 0, ErrorMessage: "err"},
		{Id: "3", Status: "cancelled", Duration: "2s", FilesTransferred: 2, BytesTransferred: 50},
		{Id: "4", Status: "completed", Duration: "8s", FilesTransferred: 3, BytesTransferred: 200},
	}

	for _, e := range entries {
		e.ProfileName = "test"
		e.Action = "push"
		e.StartTime = time.Now()
		e.EndTime = time.Now()
		if err := h.AddEntry(ctx, e); err != nil {
			t.Fatalf("AddEntry failed: %v", err)
		}
	}

	stats, err := h.GetStats(ctx)
	if err != nil {
		t.Fatalf("GetStats failed: %v", err)
	}
	if stats.TotalOperations != 4 {
		t.Errorf("expected 4 total ops, got %d", stats.TotalOperations)
	}
	if stats.SuccessCount != 2 {
		t.Errorf("expected 2 successes, got %d", stats.SuccessCount)
	}
	if stats.FailureCount != 1 {
		t.Errorf("expected 1 failure, got %d", stats.FailureCount)
	}
	if stats.CancelledCount != 1 {
		t.Errorf("expected 1 cancelled, got %d", stats.CancelledCount)
	}
}

func TestHistoryService_Persistence(t *testing.T) {
	ctx := context.Background()

	// Create first service instance and add an entry
	h1 := &HistoryService{initialized: true}
	entry := models.HistoryEntry{
		Id:          "persist-hist-1",
		ProfileName: "test",
		Action:      "push",
		Status:      "completed",
		StartTime:   time.Now(),
		EndTime:     time.Now(),
		Duration:    "1s",
	}
	if err := h1.AddEntry(ctx, entry); err != nil {
		t.Fatalf("AddEntry failed: %v", err)
	}

	// Create second service instance and query DB
	h2 := &HistoryService{initialized: true}
	entries, err := h2.GetHistory(ctx, 100, 0)
	if err != nil {
		t.Fatalf("GetHistory failed: %v", err)
	}

	found := false
	for _, e := range entries {
		if e.Id == "persist-hist-1" {
			found = true
			break
		}
	}
	if !found {
		t.Error("expected to find 'persist-hist-1' in DB after persistence")
	}
}

func TestHistoryService_Pagination(t *testing.T) {
	h := newTestHistoryService(t)
	ctx := context.Background()

	for i := 0; i < 5; i++ {
		entry := models.HistoryEntry{
			Id:          fmt.Sprintf("page-%d", i),
			ProfileName: "test",
			Action:      "push",
			Status:      "completed",
			StartTime:   time.Now(),
			EndTime:     time.Now(),
			Duration:    "1s",
		}
		if err := h.AddEntry(ctx, entry); err != nil {
			t.Fatalf("AddEntry failed: %v", err)
		}
	}

	// Test pagination
	page1, _ := h.GetHistory(ctx, 2, 0)
	if len(page1) != 2 {
		t.Errorf("expected 2 entries on page 1, got %d", len(page1))
	}

	page2, _ := h.GetHistory(ctx, 2, 2)
	if len(page2) != 2 {
		t.Errorf("expected 2 entries on page 2, got %d", len(page2))
	}

	page3, _ := h.GetHistory(ctx, 2, 4)
	if len(page3) != 1 {
		t.Errorf("expected 1 entry on page 3, got %d", len(page3))
	}

	// Out of range
	page4, _ := h.GetHistory(ctx, 2, 10)
	if len(page4) != 0 {
		t.Errorf("expected 0 entries for out-of-range offset, got %d", len(page4))
	}
}

