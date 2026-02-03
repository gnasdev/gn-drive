package services

import (
	"context"
	"desktop/backend/events"
	"desktop/backend/models"
	"encoding/json"
	"fmt"
	"log"
	"os"
	"path/filepath"
	"sync"
	"time"

	"github.com/wailsapp/wails/v3/pkg/application"
)

const maxHistoryEntries = 1000

// HistoryService manages operation history with JSON persistence
type HistoryService struct {
	app         *application.App
	eventBus    *events.WailsEventBus
	entries     []models.HistoryEntry
	filePath    string
	mutex       sync.RWMutex
	initialized bool
}

// NewHistoryService creates a new history service
func NewHistoryService(app *application.App) *HistoryService {
	return &HistoryService{
		app:     app,
		entries: []models.HistoryEntry{},
	}
}

// SetApp sets the application reference for events
func (h *HistoryService) SetApp(app *application.App) {
	h.app = app
	if bus := GetSharedEventBus(); bus != nil {
		h.eventBus = bus
	} else {
		h.eventBus = events.NewEventBus(app)
	}
}

// ServiceName returns the name of the service
func (h *HistoryService) ServiceName() string {
	return "HistoryService"
}

// ServiceStartup is called when the service starts
func (h *HistoryService) ServiceStartup(ctx context.Context, options application.ServiceOptions) error {
	log.Printf("HistoryService starting up...")
	return h.initialize()
}

// ServiceShutdown is called when the service shuts down
func (h *HistoryService) ServiceShutdown(ctx context.Context) error {
	log.Printf("HistoryService shutting down...")
	return nil
}

// initialize sets up the file path and loads existing history
func (h *HistoryService) initialize() error {
	h.mutex.Lock()
	defer h.mutex.Unlock()

	if h.initialized {
		return nil
	}

	homeDir, err := os.UserHomeDir()
	if err != nil {
		homeDir = "."
	}

	h.filePath = filepath.Join(homeDir, ".config", "ns-drive", "history.json")

	// Ensure directory exists
	if err := os.MkdirAll(filepath.Dir(h.filePath), 0755); err != nil {
		return fmt.Errorf("failed to create history directory: %w", err)
	}

	// Load existing history
	if err := h.loadFromFile(); err != nil {
		log.Printf("Warning: Could not load history: %v", err)
		h.entries = []models.HistoryEntry{}
	}

	h.initialized = true
	log.Printf("HistoryService initialized with %d entries", len(h.entries))
	return nil
}

// AddEntry adds a new history entry (FIFO capped at maxHistoryEntries)
func (h *HistoryService) AddEntry(ctx context.Context, entry models.HistoryEntry) error {
	h.mutex.Lock()
	defer h.mutex.Unlock()

	if !h.initialized {
		h.mutex.Unlock()
		if err := h.initialize(); err != nil {
			return err
		}
		h.mutex.Lock()
	}

	// Prepend (newest first)
	h.entries = append([]models.HistoryEntry{entry}, h.entries...)

	// Cap at max entries
	if len(h.entries) > maxHistoryEntries {
		h.entries = h.entries[:maxHistoryEntries]
	}

	if err := h.saveToFile(); err != nil {
		// Rollback
		h.entries = h.entries[1:]
		return fmt.Errorf("failed to save history: %w", err)
	}

	h.emitHistoryEvent(events.HistoryAdded, entry)
	return nil
}

// GetHistory returns history entries with pagination
func (h *HistoryService) GetHistory(ctx context.Context, limit, offset int) ([]models.HistoryEntry, error) {
	h.mutex.RLock()
	defer h.mutex.RUnlock()

	if offset >= len(h.entries) {
		return []models.HistoryEntry{}, nil
	}

	end := offset + limit
	if end > len(h.entries) {
		end = len(h.entries)
	}

	result := make([]models.HistoryEntry, end-offset)
	copy(result, h.entries[offset:end])
	return result, nil
}

// GetHistoryForProfile returns history entries for a specific profile
func (h *HistoryService) GetHistoryForProfile(ctx context.Context, profileName string) ([]models.HistoryEntry, error) {
	h.mutex.RLock()
	defer h.mutex.RUnlock()

	var result []models.HistoryEntry
	for _, entry := range h.entries {
		if entry.ProfileName == profileName {
			result = append(result, entry)
		}
	}
	return result, nil
}

// GetStats returns aggregate statistics across all history
func (h *HistoryService) GetStats(ctx context.Context) (*models.AggregateStats, error) {
	h.mutex.RLock()
	defer h.mutex.RUnlock()

	stats := &models.AggregateStats{
		TotalOperations: len(h.entries),
	}

	var totalDuration time.Duration
	for _, entry := range h.entries {
		switch entry.Status {
		case "completed":
			stats.SuccessCount++
		case "failed":
			stats.FailureCount++
		case "cancelled":
			stats.CancelledCount++
		}
		stats.TotalBytes += entry.BytesTransferred
		stats.TotalFiles += entry.FilesTransferred

		if d, err := time.ParseDuration(entry.Duration); err == nil {
			totalDuration += d
		}
	}

	if stats.TotalOperations > 0 {
		avg := totalDuration / time.Duration(stats.TotalOperations)
		stats.AverageDuration = avg.String()
	}

	return stats, nil
}

// ClearHistory removes all history entries
func (h *HistoryService) ClearHistory(ctx context.Context) error {
	h.mutex.Lock()
	defer h.mutex.Unlock()

	oldEntries := h.entries
	h.entries = []models.HistoryEntry{}

	if err := h.saveToFile(); err != nil {
		h.entries = oldEntries
		return fmt.Errorf("failed to clear history: %w", err)
	}

	h.emitHistoryEvent(events.HistoryCleared, nil)
	return nil
}

// loadFromFile loads history from the JSON file
func (h *HistoryService) loadFromFile() error {
	data, err := os.ReadFile(h.filePath)
	if err != nil {
		if os.IsNotExist(err) {
			return nil
		}
		return err
	}

	if len(data) == 0 {
		return nil
	}

	return json.Unmarshal(data, &h.entries)
}

// saveToFile saves history to the JSON file
func (h *HistoryService) saveToFile() error {
	data, err := json.MarshalIndent(h.entries, "", "  ")
	if err != nil {
		return err
	}
	return os.WriteFile(h.filePath, data, 0644)
}

// emitHistoryEvent emits a history event
func (h *HistoryService) emitHistoryEvent(eventType events.EventType, data interface{}) {
	event := events.NewHistoryEvent(eventType, data)
	if h.eventBus != nil {
		if err := h.eventBus.EmitHistoryEvent(event); err != nil {
			log.Printf("Failed to emit history event: %v", err)
		}
	} else if h.app != nil {
		h.app.Event.Emit("tofe", event)
	}
}
