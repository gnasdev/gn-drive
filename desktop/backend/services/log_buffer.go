package services

import (
	"sync"
	"sync/atomic"
	"time"
)

// LogEntry represents a single log entry with sequence number for tracking
type LogEntry struct {
	SeqNo     uint64    `json:"seqNo"`
	TabId     string    `json:"tabId"`
	Message   string    `json:"message"`
	Timestamp time.Time `json:"timestamp"`
	Level     string    `json:"level"` // info, error, progress
}

// LogBuffer is a thread-safe ring buffer for storing log entries
type LogBuffer struct {
	entries    []LogEntry
	capacity   int
	writeIndex int
	seqCounter uint64
	mutex      sync.RWMutex
}

// NewLogBuffer creates a new LogBuffer with the specified capacity
func NewLogBuffer(capacity int) *LogBuffer {
	if capacity <= 0 {
		capacity = 5000 // Default capacity
	}
	return &LogBuffer{
		entries:    make([]LogEntry, 0, capacity),
		capacity:   capacity,
		writeIndex: 0,
		seqCounter: 0,
	}
}

// Append adds a new log entry to the buffer and returns the assigned sequence number
func (b *LogBuffer) Append(tabId, message, level string) uint64 {
	b.mutex.Lock()
	defer b.mutex.Unlock()

	// Increment sequence counter atomically
	seqNo := atomic.AddUint64(&b.seqCounter, 1)

	entry := LogEntry{
		SeqNo:     seqNo,
		TabId:     tabId,
		Message:   message,
		Timestamp: time.Now(),
		Level:     level,
	}

	if len(b.entries) < b.capacity {
		// Buffer not full yet, just append
		b.entries = append(b.entries, entry)
	} else {
		// Buffer is full, overwrite at writeIndex (ring buffer)
		b.entries[b.writeIndex] = entry
		b.writeIndex = (b.writeIndex + 1) % b.capacity
	}

	return seqNo
}

// GetSince returns all log entries with sequence number greater than afterSeqNo
// If tabId is not empty, only returns entries for that tab
func (b *LogBuffer) GetSince(tabId string, afterSeqNo uint64) []LogEntry {
	b.mutex.RLock()
	defer b.mutex.RUnlock()

	result := make([]LogEntry, 0)

	for _, entry := range b.entries {
		if entry.SeqNo > afterSeqNo {
			if tabId == "" || entry.TabId == tabId {
				result = append(result, entry)
			}
		}
	}

	// Sort by sequence number to ensure correct order
	sortBySeqNo(result)

	return result
}

// GetLatest returns the N most recent log entries
// If tabId is not empty, only returns entries for that tab
func (b *LogBuffer) GetLatest(tabId string, count int) []LogEntry {
	b.mutex.RLock()
	defer b.mutex.RUnlock()

	if count <= 0 {
		return []LogEntry{}
	}

	// Collect matching entries
	matching := make([]LogEntry, 0)
	for _, entry := range b.entries {
		if tabId == "" || entry.TabId == tabId {
			matching = append(matching, entry)
		}
	}

	// Sort by sequence number
	sortBySeqNo(matching)

	// Return the last N entries
	if len(matching) <= count {
		return matching
	}
	return matching[len(matching)-count:]
}

// GetCurrentSeqNo returns the current sequence number counter
func (b *LogBuffer) GetCurrentSeqNo() uint64 {
	return atomic.LoadUint64(&b.seqCounter)
}

// Clear removes all entries for a specific tab, or all entries if tabId is empty
func (b *LogBuffer) Clear(tabId string) {
	b.mutex.Lock()
	defer b.mutex.Unlock()

	if tabId == "" {
		// Clear all entries
		b.entries = make([]LogEntry, 0, b.capacity)
		b.writeIndex = 0
	} else {
		// Remove entries for specific tab
		newEntries := make([]LogEntry, 0, len(b.entries))
		for _, entry := range b.entries {
			if entry.TabId != tabId {
				newEntries = append(newEntries, entry)
			}
		}
		b.entries = newEntries
		// Reset writeIndex if we cleared entries
		if len(b.entries) < b.capacity {
			b.writeIndex = 0
		}
	}
}

// Size returns the current number of entries in the buffer
func (b *LogBuffer) Size() int {
	b.mutex.RLock()
	defer b.mutex.RUnlock()
	return len(b.entries)
}

// sortBySeqNo sorts log entries by sequence number in ascending order
func sortBySeqNo(entries []LogEntry) {
	// Simple insertion sort - efficient for small to medium arrays
	for i := 1; i < len(entries); i++ {
		key := entries[i]
		j := i - 1
		for j >= 0 && entries[j].SeqNo > key.SeqNo {
			entries[j+1] = entries[j]
			j--
		}
		entries[j+1] = key
	}
}
