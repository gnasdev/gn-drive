package services

import (
	"sync"
	"testing"
)

func TestNewLogBuffer(t *testing.T) {
	tests := []struct {
		name     string
		capacity int
		expected int
	}{
		{"default capacity for zero", 0, 5000},
		{"default capacity for negative", -1, 5000},
		{"custom capacity", 100, 100},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			buf := NewLogBuffer(tt.capacity)
			if buf.capacity != tt.expected {
				t.Errorf("expected capacity %d, got %d", tt.expected, buf.capacity)
			}
			if buf.Size() != 0 {
				t.Errorf("expected size 0, got %d", buf.Size())
			}
		})
	}
}

func TestLogBufferAppend(t *testing.T) {
	buf := NewLogBuffer(10)

	// Test basic append
	seqNo1 := buf.Append("tab1", "message1", "info")
	if seqNo1 != 1 {
		t.Errorf("expected seqNo 1, got %d", seqNo1)
	}

	seqNo2 := buf.Append("tab1", "message2", "info")
	if seqNo2 != 2 {
		t.Errorf("expected seqNo 2, got %d", seqNo2)
	}

	if buf.Size() != 2 {
		t.Errorf("expected size 2, got %d", buf.Size())
	}
}

func TestLogBufferAppendRingBuffer(t *testing.T) {
	buf := NewLogBuffer(3)

	// Fill buffer
	buf.Append("tab1", "msg1", "info")
	buf.Append("tab1", "msg2", "info")
	buf.Append("tab1", "msg3", "info")

	if buf.Size() != 3 {
		t.Errorf("expected size 3, got %d", buf.Size())
	}

	// Overflow - should replace oldest
	seqNo := buf.Append("tab1", "msg4", "info")
	if seqNo != 4 {
		t.Errorf("expected seqNo 4, got %d", seqNo)
	}

	if buf.Size() != 3 {
		t.Errorf("expected size 3 after overflow, got %d", buf.Size())
	}

	// Verify oldest entry was replaced
	entries := buf.GetSince("", 0)
	if len(entries) != 3 {
		t.Errorf("expected 3 entries, got %d", len(entries))
	}

	// Check that msg1 is gone and we have msg2, msg3, msg4
	messages := make(map[string]bool)
	for _, e := range entries {
		messages[e.Message] = true
	}
	if messages["msg1"] {
		t.Error("msg1 should have been overwritten")
	}
	if !messages["msg2"] || !messages["msg3"] || !messages["msg4"] {
		t.Error("expected msg2, msg3, msg4 to be present")
	}
}

func TestLogBufferGetSince(t *testing.T) {
	buf := NewLogBuffer(100)

	buf.Append("tab1", "msg1", "info")
	buf.Append("tab2", "msg2", "info")
	buf.Append("tab1", "msg3", "info")
	buf.Append("tab2", "msg4", "info")
	buf.Append("tab1", "msg5", "info")

	// Get all entries
	all := buf.GetSince("", 0)
	if len(all) != 5 {
		t.Errorf("expected 5 entries, got %d", len(all))
	}

	// Get entries after seqNo 2
	afterTwo := buf.GetSince("", 2)
	if len(afterTwo) != 3 {
		t.Errorf("expected 3 entries after seqNo 2, got %d", len(afterTwo))
	}

	// Get entries for tab1 only
	tab1Entries := buf.GetSince("tab1", 0)
	if len(tab1Entries) != 3 {
		t.Errorf("expected 3 entries for tab1, got %d", len(tab1Entries))
	}

	// Get entries for tab1 after seqNo 1
	tab1AfterOne := buf.GetSince("tab1", 1)
	if len(tab1AfterOne) != 2 {
		t.Errorf("expected 2 entries for tab1 after seqNo 1, got %d", len(tab1AfterOne))
	}

	// Verify ordering (should be sorted by seqNo)
	for i := 1; i < len(all); i++ {
		if all[i].SeqNo <= all[i-1].SeqNo {
			t.Errorf("entries not sorted: seqNo %d should be > %d", all[i].SeqNo, all[i-1].SeqNo)
		}
	}
}

func TestLogBufferGetLatest(t *testing.T) {
	buf := NewLogBuffer(100)

	for i := 1; i <= 10; i++ {
		buf.Append("tab1", "msg", "info")
	}

	// Get latest 3
	latest3 := buf.GetLatest("", 3)
	if len(latest3) != 3 {
		t.Errorf("expected 3 entries, got %d", len(latest3))
	}

	// Verify they are the last 3 (seqNo 8, 9, 10)
	expectedSeqNos := []uint64{8, 9, 10}
	for i, e := range latest3 {
		if e.SeqNo != expectedSeqNos[i] {
			t.Errorf("expected seqNo %d, got %d", expectedSeqNos[i], e.SeqNo)
		}
	}

	// Get more than available
	all := buf.GetLatest("", 100)
	if len(all) != 10 {
		t.Errorf("expected 10 entries, got %d", len(all))
	}

	// Get 0 or negative
	zero := buf.GetLatest("", 0)
	if len(zero) != 0 {
		t.Errorf("expected 0 entries, got %d", len(zero))
	}

	negative := buf.GetLatest("", -5)
	if len(negative) != 0 {
		t.Errorf("expected 0 entries for negative count, got %d", len(negative))
	}
}

func TestLogBufferGetCurrentSeqNo(t *testing.T) {
	buf := NewLogBuffer(100)

	if buf.GetCurrentSeqNo() != 0 {
		t.Errorf("expected initial seqNo 0, got %d", buf.GetCurrentSeqNo())
	}

	buf.Append("tab1", "msg1", "info")
	if buf.GetCurrentSeqNo() != 1 {
		t.Errorf("expected seqNo 1, got %d", buf.GetCurrentSeqNo())
	}

	buf.Append("tab1", "msg2", "info")
	buf.Append("tab1", "msg3", "info")
	if buf.GetCurrentSeqNo() != 3 {
		t.Errorf("expected seqNo 3, got %d", buf.GetCurrentSeqNo())
	}
}

func TestLogBufferClear(t *testing.T) {
	buf := NewLogBuffer(100)

	buf.Append("tab1", "msg1", "info")
	buf.Append("tab2", "msg2", "info")
	buf.Append("tab1", "msg3", "info")

	// Clear specific tab
	buf.Clear("tab1")
	remaining := buf.GetSince("", 0)
	if len(remaining) != 1 {
		t.Errorf("expected 1 entry after clearing tab1, got %d", len(remaining))
	}
	if remaining[0].TabId != "tab2" {
		t.Errorf("expected remaining entry to be tab2, got %s", remaining[0].TabId)
	}

	// Clear all
	buf.Append("tab1", "msg4", "info")
	buf.Clear("")
	if buf.Size() != 0 {
		t.Errorf("expected size 0 after clear all, got %d", buf.Size())
	}
}

func TestLogBufferConcurrency(t *testing.T) {
	buf := NewLogBuffer(1000)
	var wg sync.WaitGroup

	// Concurrent writes
	numWriters := 10
	writesPerWriter := 100

	for i := 0; i < numWriters; i++ {
		wg.Add(1)
		go func(writerID int) {
			defer wg.Done()
			for j := 0; j < writesPerWriter; j++ {
				buf.Append("tab1", "message", "info")
			}
		}(i)
	}

	// Concurrent reads
	numReaders := 5
	for i := 0; i < numReaders; i++ {
		wg.Add(1)
		go func() {
			defer wg.Done()
			for j := 0; j < 50; j++ {
				buf.GetSince("", 0)
				buf.GetLatest("", 10)
				buf.GetCurrentSeqNo()
			}
		}()
	}

	wg.Wait()

	// Verify total writes
	expectedSeqNo := uint64(numWriters * writesPerWriter)
	if buf.GetCurrentSeqNo() != expectedSeqNo {
		t.Errorf("expected seqNo %d, got %d", expectedSeqNo, buf.GetCurrentSeqNo())
	}
}

func TestLogBufferSequenceNumberMonotonicity(t *testing.T) {
	buf := NewLogBuffer(100)

	var seqNos []uint64
	for i := 0; i < 50; i++ {
		seqNo := buf.Append("tab1", "msg", "info")
		seqNos = append(seqNos, seqNo)
	}

	// Verify strictly increasing
	for i := 1; i < len(seqNos); i++ {
		if seqNos[i] != seqNos[i-1]+1 {
			t.Errorf("sequence numbers not monotonically increasing: %d -> %d", seqNos[i-1], seqNos[i])
		}
	}
}

func TestLogEntryFields(t *testing.T) {
	buf := NewLogBuffer(100)

	seqNo := buf.Append("test-tab", "test message", "error")

	entries := buf.GetSince("", 0)
	if len(entries) != 1 {
		t.Fatalf("expected 1 entry, got %d", len(entries))
	}

	entry := entries[0]
	if entry.SeqNo != seqNo {
		t.Errorf("expected seqNo %d, got %d", seqNo, entry.SeqNo)
	}
	if entry.TabId != "test-tab" {
		t.Errorf("expected tabId 'test-tab', got '%s'", entry.TabId)
	}
	if entry.Message != "test message" {
		t.Errorf("expected message 'test message', got '%s'", entry.Message)
	}
	if entry.Level != "error" {
		t.Errorf("expected level 'error', got '%s'", entry.Level)
	}
	if entry.Timestamp.IsZero() {
		t.Error("expected non-zero timestamp")
	}
}
