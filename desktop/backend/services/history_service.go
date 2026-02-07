package services

import (
	"context"
	"desktop/backend/events"
	"desktop/backend/models"
	"fmt"
	"log"
	"sync"
	"time"

	"github.com/wailsapp/wails/v3/pkg/application"
)

const maxHistoryEntries = 1000

// HistoryService manages operation history with SQLite persistence
type HistoryService struct {
	app         *application.App
	eventBus    *events.WailsEventBus
	mutex       sync.RWMutex
	initialized bool
}

// NewHistoryService creates a new history service
func NewHistoryService(app *application.App) *HistoryService {
	return &HistoryService{
		app: app,
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

// ServiceStartup is called when the service starts.
// Initialization is deferred to first access to speed up app startup.
func (h *HistoryService) ServiceStartup(ctx context.Context, options application.ServiceOptions) error {
	log.Printf("HistoryService starting up (lazy init)...")
	return nil
}

// ensureInitialized lazily initializes the service on first access.
func (h *HistoryService) ensureInitialized() error {
	h.mutex.RLock()
	if h.initialized {
		h.mutex.RUnlock()
		return nil
	}
	h.mutex.RUnlock()
	return h.initialize()
}

// ServiceShutdown is called when the service shuts down
func (h *HistoryService) ServiceShutdown(ctx context.Context) error {
	log.Printf("HistoryService shutting down...")
	return nil
}

// initialize sets up the service
func (h *HistoryService) initialize() error {
	h.mutex.Lock()
	defer h.mutex.Unlock()

	if h.initialized {
		return nil
	}

	h.initialized = true
	log.Printf("HistoryService initialized")
	return nil
}

// AddEntry adds a new history entry (capped at maxHistoryEntries)
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

	if err := h.saveHistoryEntryToDB(entry); err != nil {
		return fmt.Errorf("failed to save history: %w", err)
	}

	// Enforce cap
	h.enforceHistoryCap()

	h.emitHistoryEvent(events.HistoryAdded, entry)
	return nil
}

// GetHistory returns history entries with pagination
func (h *HistoryService) GetHistory(ctx context.Context, limit, offset int) ([]models.HistoryEntry, error) {
	if err := h.ensureInitialized(); err != nil {
		return nil, err
	}

	db, err := GetSharedDB()
	if err != nil {
		return nil, err
	}

	rows, err := db.Query(`SELECT id, profile_name, action, status, start_time, end_time,
		duration, files_transferred, bytes_transferred, errors, error_message
		FROM history ORDER BY start_time DESC LIMIT ? OFFSET ?`, limit, offset)
	if err != nil {
		return nil, fmt.Errorf("failed to query history: %w", err)
	}
	defer rows.Close()

	entries, err := h.scanHistoryRows(rows)
	if err != nil {
		return nil, err
	}
	return entries, nil
}

// GetHistoryForProfile returns history entries for a specific profile
func (h *HistoryService) GetHistoryForProfile(ctx context.Context, profileName string) ([]models.HistoryEntry, error) {
	if err := h.ensureInitialized(); err != nil {
		return nil, err
	}

	db, err := GetSharedDB()
	if err != nil {
		return nil, err
	}

	rows, err := db.Query(`SELECT id, profile_name, action, status, start_time, end_time,
		duration, files_transferred, bytes_transferred, errors, error_message
		FROM history WHERE profile_name = ? ORDER BY start_time DESC`, profileName)
	if err != nil {
		return nil, fmt.Errorf("failed to query history for profile: %w", err)
	}
	defer rows.Close()

	entries, err := h.scanHistoryRows(rows)
	if err != nil {
		return nil, err
	}
	return entries, nil
}

// GetStats returns aggregate statistics across all history
func (h *HistoryService) GetStats(ctx context.Context) (*models.AggregateStats, error) {
	if err := h.ensureInitialized(); err != nil {
		return nil, err
	}

	db, err := GetSharedDB()
	if err != nil {
		return nil, err
	}

	stats := &models.AggregateStats{}

	// Get counts and totals via SQL aggregation
	err = db.QueryRow(`SELECT
		COUNT(*),
		COALESCE(SUM(CASE WHEN status = 'completed' THEN 1 ELSE 0 END), 0),
		COALESCE(SUM(CASE WHEN status = 'failed' THEN 1 ELSE 0 END), 0),
		COALESCE(SUM(CASE WHEN status = 'cancelled' THEN 1 ELSE 0 END), 0),
		COALESCE(SUM(bytes_transferred), 0),
		COALESCE(SUM(files_transferred), 0)
		FROM history`).Scan(
		&stats.TotalOperations,
		&stats.SuccessCount,
		&stats.FailureCount,
		&stats.CancelledCount,
		&stats.TotalBytes,
		&stats.TotalFiles,
	)
	if err != nil {
		return nil, fmt.Errorf("failed to get history stats: %w", err)
	}

	// Compute average duration from all duration strings
	if stats.TotalOperations > 0 {
		rows, err := db.Query("SELECT duration FROM history WHERE duration != ''")
		if err == nil {
			defer rows.Close()
			var totalDuration time.Duration
			var count int
			for rows.Next() {
				var dur string
				if err := rows.Scan(&dur); err == nil {
					if d, err := time.ParseDuration(dur); err == nil {
						totalDuration += d
						count++
					}
				}
			}
			if count > 0 {
				avg := totalDuration / time.Duration(count)
				stats.AverageDuration = avg.String()
			}
		}
	}

	return stats, nil
}

// ClearHistory removes all history entries
func (h *HistoryService) ClearHistory(ctx context.Context) error {
	if err := h.ensureInitialized(); err != nil {
		return err
	}

	db, err := GetSharedDB()
	if err != nil {
		return err
	}

	if _, err := db.Exec("DELETE FROM history"); err != nil {
		return fmt.Errorf("failed to clear history: %w", err)
	}

	h.emitHistoryEvent(events.HistoryCleared, nil)
	return nil
}

// saveHistoryEntryToDB inserts a single history entry into the database
func (h *HistoryService) saveHistoryEntryToDB(e models.HistoryEntry) error {
	db, err := GetSharedDB()
	if err != nil {
		return err
	}

	_, err = db.Exec(`INSERT OR REPLACE INTO history (id, profile_name, action, status, start_time, end_time,
		duration, files_transferred, bytes_transferred, errors, error_message)
		VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)`,
		e.Id, e.ProfileName, e.Action, e.Status,
		e.StartTime.UTC().Format(time.RFC3339), e.EndTime.UTC().Format(time.RFC3339),
		e.Duration, e.FilesTransferred, e.BytesTransferred, e.Errors, e.ErrorMessage)
	return err
}

// enforceHistoryCap deletes oldest entries exceeding the max count
func (h *HistoryService) enforceHistoryCap() {
	db, err := GetSharedDB()
	if err != nil {
		return
	}

	_, _ = db.Exec(`DELETE FROM history WHERE id NOT IN (
		SELECT id FROM history ORDER BY start_time DESC LIMIT ?
	)`, maxHistoryEntries)
}

// scanHistoryRows scans rows into HistoryEntry slice
func (h *HistoryService) scanHistoryRows(rows interface{ Next() bool; Scan(...interface{}) error; Err() error }) ([]models.HistoryEntry, error) {
	var entries []models.HistoryEntry
	for rows.Next() {
		var e models.HistoryEntry
		var startTime, endTime string
		if err := rows.Scan(&e.Id, &e.ProfileName, &e.Action, &e.Status, &startTime, &endTime,
			&e.Duration, &e.FilesTransferred, &e.BytesTransferred, &e.Errors, &e.ErrorMessage); err != nil {
			return nil, fmt.Errorf("failed to scan history entry: %w", err)
		}
		if t, err := time.Parse(time.RFC3339, startTime); err == nil {
			e.StartTime = t
		}
		if t, err := time.Parse(time.RFC3339, endTime); err == nil {
			e.EndTime = t
		}
		entries = append(entries, e)
	}
	if entries == nil {
		entries = []models.HistoryEntry{}
	}
	if err := rows.Err(); err != nil {
		return nil, err
	}
	return entries, nil
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
