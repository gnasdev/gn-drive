package services

import (
	"context"
	"desktop/backend/events"
	"desktop/backend/models"
	"fmt"
	"log"
	"sync"
	"time"

	"github.com/robfig/cron/v3"
	"github.com/wailsapp/wails/v3/pkg/application"
)

// SchedulerService manages cron-based scheduled sync operations
type SchedulerService struct {
	app         *application.App
	eventBus    *events.WailsEventBus
	cron        *cron.Cron
	schedules   []models.ScheduleEntry
	cronEntries map[string]cron.EntryID // scheduleId -> cron entry ID
	mutex       sync.RWMutex
	initialized bool

	// Dependencies injected after creation
	syncService *SyncService
}

// NewSchedulerService creates a new scheduler service
func NewSchedulerService(app *application.App) *SchedulerService {
	return &SchedulerService{
		app:         app,
		schedules:   []models.ScheduleEntry{},
		cronEntries: make(map[string]cron.EntryID),
		cron:        cron.New(),
	}
}

// SetApp sets the application reference for events
func (s *SchedulerService) SetApp(app *application.App) {
	s.app = app
	if bus := GetSharedEventBus(); bus != nil {
		s.eventBus = bus
	} else {
		s.eventBus = events.NewEventBus(app)
	}
}

// SetSyncService sets the sync service dependency for triggering syncs
func (s *SchedulerService) SetSyncService(syncService *SyncService) {
	s.syncService = syncService
}

// ServiceName returns the name of the service
func (s *SchedulerService) ServiceName() string {
	return "SchedulerService"
}

// ServiceStartup is called when the service starts.
// Initialization runs in a goroutine to avoid blocking app startup.
// If DB is not available (auth enabled), the goroutine fails silently
// and ensureInitialized() retries on first access after unlock.
func (s *SchedulerService) ServiceStartup(ctx context.Context, options application.ServiceOptions) error {
	log.Printf("SchedulerService starting up (async)...")
	go func() {
		if err := s.initialize(); err != nil {
			log.Printf("SchedulerService init deferred (DB not ready): %v", err)
		}
	}()
	return nil
}

// ensureInitialized lazily initializes the service if not yet done.
func (s *SchedulerService) ensureInitialized() error {
	s.mutex.RLock()
	if s.initialized {
		s.mutex.RUnlock()
		return nil
	}
	s.mutex.RUnlock()
	return s.initialize()
}

// ServiceShutdown is called when the service shuts down
func (s *SchedulerService) ServiceShutdown(ctx context.Context) error {
	log.Printf("SchedulerService shutting down...")
	s.cron.Stop()
	return nil
}

// initialize loads existing schedules from SQLite and registers cron jobs.
// Returns error if DB is not available (e.g. auth enabled, files encrypted).
// In that case, initialized stays false so ensureInitialized() retries later.
func (s *SchedulerService) initialize() error {
	s.mutex.Lock()
	defer s.mutex.Unlock()

	if s.initialized {
		return nil
	}

	schedules, err := s.loadSchedulesFromDB()
	if err != nil {
		// Don't set initialized - ensureInitialized() will retry on next access
		return fmt.Errorf("could not load schedules: %w", err)
	}
	s.schedules = schedules

	// Register enabled schedules with cron
	for i := range s.schedules {
		if s.schedules[i].Enabled {
			if err := s.registerCronJob(&s.schedules[i]); err != nil {
				log.Printf("Warning: Failed to register schedule %s: %v", s.schedules[i].Id, err)
			}
		}
	}

	// Start cron scheduler (safe to call multiple times)
	s.cron.Start()

	s.initialized = true
	log.Printf("SchedulerService initialized with %d schedules", len(s.schedules))
	return nil
}

// AddSchedule adds a new schedule entry
func (s *SchedulerService) AddSchedule(ctx context.Context, entry models.ScheduleEntry) error {
	if err := s.ensureInitialized(); err != nil {
		return err
	}
	s.mutex.Lock()
	defer s.mutex.Unlock()

	// Validate cron expression
	if _, err := cron.ParseStandard(entry.CronExpr); err != nil {
		return fmt.Errorf("invalid cron expression %q: %w", entry.CronExpr, err)
	}

	if entry.CreatedAt.IsZero() {
		entry.CreatedAt = time.Now()
	}

	s.schedules = append(s.schedules, entry)

	if entry.Enabled {
		if err := s.registerCronJob(&s.schedules[len(s.schedules)-1]); err != nil {
			s.schedules = s.schedules[:len(s.schedules)-1]
			return fmt.Errorf("failed to register cron job: %w", err)
		}
	}

	if err := s.saveScheduleToDB(s.schedules[len(s.schedules)-1]); err != nil {
		s.unregisterCronJob(entry.Id)
		s.schedules = s.schedules[:len(s.schedules)-1]
		return fmt.Errorf("failed to save schedule: %w", err)
	}

	s.emitScheduleEvent(events.ScheduleAdded, entry.Id, entry)
	log.Printf("Schedule '%s' added for profile '%s'", entry.Id, entry.ProfileName)
	return nil
}

// UpdateSchedule updates an existing schedule
func (s *SchedulerService) UpdateSchedule(ctx context.Context, entry models.ScheduleEntry) error {
	if err := s.ensureInitialized(); err != nil {
		return err
	}
	s.mutex.Lock()
	defer s.mutex.Unlock()

	// Validate cron expression
	if _, err := cron.ParseStandard(entry.CronExpr); err != nil {
		return fmt.Errorf("invalid cron expression %q: %w", entry.CronExpr, err)
	}

	found := false
	var oldEntry models.ScheduleEntry
	for i, existing := range s.schedules {
		if existing.Id == entry.Id {
			oldEntry = existing
			s.schedules[i] = entry
			found = true

			// Re-register cron job
			s.unregisterCronJob(entry.Id)
			if entry.Enabled {
				if err := s.registerCronJob(&s.schedules[i]); err != nil {
					s.schedules[i] = oldEntry
					return fmt.Errorf("failed to register cron job: %w", err)
				}
			}
			break
		}
	}

	if !found {
		return fmt.Errorf("schedule '%s' not found", entry.Id)
	}

	if err := s.saveScheduleToDB(entry); err != nil {
		// Rollback
		for i, existing := range s.schedules {
			if existing.Id == entry.Id {
				s.unregisterCronJob(entry.Id)
				s.schedules[i] = oldEntry
				if oldEntry.Enabled {
					_ = s.registerCronJob(&s.schedules[i])
				}
				break
			}
		}
		return fmt.Errorf("failed to save schedule: %w", err)
	}

	s.emitScheduleEvent(events.ScheduleUpdated, entry.Id, entry)
	return nil
}

// DeleteSchedule removes a schedule
func (s *SchedulerService) DeleteSchedule(ctx context.Context, scheduleId string) error {
	if err := s.ensureInitialized(); err != nil {
		return err
	}
	s.mutex.Lock()
	defer s.mutex.Unlock()

	found := false
	var deletedEntry models.ScheduleEntry
	for i, entry := range s.schedules {
		if entry.Id == scheduleId {
			deletedEntry = entry
			s.schedules = append(s.schedules[:i], s.schedules[i+1:]...)
			s.unregisterCronJob(scheduleId)
			found = true
			break
		}
	}

	if !found {
		return fmt.Errorf("schedule '%s' not found", scheduleId)
	}

	if err := s.deleteScheduleFromDB(scheduleId); err != nil {
		s.schedules = append(s.schedules, deletedEntry)
		return fmt.Errorf("failed to delete schedule: %w", err)
	}

	s.emitScheduleEvent(events.ScheduleDeleted, scheduleId, deletedEntry)
	return nil
}

// GetSchedules returns all schedules
func (s *SchedulerService) GetSchedules(ctx context.Context) ([]models.ScheduleEntry, error) {
	if err := s.ensureInitialized(); err != nil {
		return nil, err
	}
	s.mutex.RLock()
	defer s.mutex.RUnlock()

	result := make([]models.ScheduleEntry, len(s.schedules))
	copy(result, s.schedules)
	return result, nil
}

// EnableSchedule enables a schedule
func (s *SchedulerService) EnableSchedule(ctx context.Context, scheduleId string) error {
	if err := s.ensureInitialized(); err != nil {
		return err
	}
	s.mutex.Lock()
	defer s.mutex.Unlock()

	for i, entry := range s.schedules {
		if entry.Id == scheduleId {
			s.schedules[i].Enabled = true
			if err := s.registerCronJob(&s.schedules[i]); err != nil {
				return fmt.Errorf("failed to register cron job: %w", err)
			}
			if err := s.saveScheduleToDB(s.schedules[i]); err != nil {
				s.unregisterCronJob(scheduleId)
				s.schedules[i].Enabled = false
				return fmt.Errorf("failed to save schedules: %w", err)
			}
			s.emitScheduleEvent(events.ScheduleUpdated, scheduleId, s.schedules[i])
			return nil
		}
	}
	return fmt.Errorf("schedule '%s' not found", scheduleId)
}

// DisableSchedule disables a schedule
func (s *SchedulerService) DisableSchedule(ctx context.Context, scheduleId string) error {
	if err := s.ensureInitialized(); err != nil {
		return err
	}
	s.mutex.Lock()
	defer s.mutex.Unlock()

	for i, entry := range s.schedules {
		if entry.Id == scheduleId {
			s.schedules[i].Enabled = false
			s.unregisterCronJob(scheduleId)
			if err := s.saveScheduleToDB(s.schedules[i]); err != nil {
				s.schedules[i].Enabled = true
				return fmt.Errorf("failed to save schedules: %w", err)
			}
			s.emitScheduleEvent(events.ScheduleUpdated, scheduleId, s.schedules[i])
			return nil
		}
	}
	return fmt.Errorf("schedule '%s' not found", scheduleId)
}

// registerCronJob registers a cron job for a schedule entry
func (s *SchedulerService) registerCronJob(entry *models.ScheduleEntry) error {
	scheduleId := entry.Id
	profileName := entry.ProfileName
	action := entry.Action

	entryId, err := s.cron.AddFunc(entry.CronExpr, func() {
		s.triggerSchedule(scheduleId, profileName, action)
	})
	if err != nil {
		return err
	}

	s.cronEntries[scheduleId] = entryId

	// Update next run time
	cronEntry := s.cron.Entry(entryId)
	nextRun := cronEntry.Next
	entry.NextRun = &nextRun

	return nil
}

// unregisterCronJob removes a cron job
func (s *SchedulerService) unregisterCronJob(scheduleId string) {
	if entryId, exists := s.cronEntries[scheduleId]; exists {
		s.cron.Remove(entryId)
		delete(s.cronEntries, scheduleId)
	}
}

// triggerSchedule is called by cron to execute a scheduled sync
func (s *SchedulerService) triggerSchedule(scheduleId, profileName, action string) {
	log.Printf("Schedule '%s' triggered: profile=%s action=%s", scheduleId, profileName, action)

	s.emitScheduleEvent(events.ScheduleTriggered, scheduleId, map[string]string{
		"profile_name": profileName,
		"action":       action,
	})

	// Update last run time
	s.mutex.Lock()
	now := time.Now()
	for i, entry := range s.schedules {
		if entry.Id == scheduleId {
			s.schedules[i].LastRun = &now

			// Update next run from cron
			if entryId, exists := s.cronEntries[scheduleId]; exists {
				cronEntry := s.cron.Entry(entryId)
				nextRun := cronEntry.Next
				s.schedules[i].NextRun = &nextRun
			}

			// Trigger sync via SyncService if available
			if s.syncService != nil {
				var syncAction SyncAction
				switch action {
				case "pull":
					syncAction = ActionPull
				case "push":
					syncAction = ActionPush
				case "bi":
					syncAction = ActionBi
				case "bi-resync":
					syncAction = ActionBiResync
				default:
					s.schedules[i].LastResult = "failed"
					log.Printf("Unknown action '%s' for schedule '%s'", action, scheduleId)
					_ = s.saveScheduleToDB(s.schedules[i])
					s.mutex.Unlock()
					return
				}

				// We need the profile from ConfigService, but for now we use profile name
				// The caller should have the profile data when creating the schedule
				s.mutex.Unlock()

				// Start sync (will run asynchronously)
				_, err := s.syncService.StartSync(context.Background(), string(syncAction), models.Profile{Name: profileName}, "")
				s.mutex.Lock()

				if err != nil {
					for j, e := range s.schedules {
						if e.Id == scheduleId {
							s.schedules[j].LastResult = "failed"
							break
						}
					}
					log.Printf("Failed to trigger sync for schedule '%s': %v", scheduleId, err)
				} else {
					for j, e := range s.schedules {
						if e.Id == scheduleId {
							s.schedules[j].LastResult = "success"
							break
						}
					}
				}
			}

			_ = s.saveScheduleToDB(s.schedules[i])
			break
		}
	}
	s.mutex.Unlock()
}

// loadSchedulesFromDB loads all schedules from SQLite
func (s *SchedulerService) loadSchedulesFromDB() ([]models.ScheduleEntry, error) {
	db, err := GetSharedDB()
	if err != nil {
		return nil, err
	}

	rows, err := db.Query("SELECT id, profile_name, action, cron_expr, enabled, last_run, next_run, last_result, created_at FROM schedules")
	if err != nil {
		return nil, err
	}
	defer rows.Close()

	var schedules []models.ScheduleEntry
	for rows.Next() {
		var e models.ScheduleEntry
		var enabled int
		var lastRun, nextRun *string
		var createdAt string
		if err := rows.Scan(&e.Id, &e.ProfileName, &e.Action, &e.CronExpr, &enabled, &lastRun, &nextRun, &e.LastResult, &createdAt); err != nil {
			return nil, fmt.Errorf("failed to scan schedule: %w", err)
		}
		e.Enabled = enabled != 0
		if lastRun != nil {
			if t, err := time.Parse(time.RFC3339, *lastRun); err == nil {
				e.LastRun = &t
			}
		}
		if nextRun != nil {
			if t, err := time.Parse(time.RFC3339, *nextRun); err == nil {
				e.NextRun = &t
			}
		}
		if t, err := time.Parse(time.RFC3339, createdAt); err == nil {
			e.CreatedAt = t
		}
		schedules = append(schedules, e)
	}
	if schedules == nil {
		schedules = []models.ScheduleEntry{}
	}
	return schedules, rows.Err()
}

// saveScheduleToDB saves a single schedule to the database
func (s *SchedulerService) saveScheduleToDB(e models.ScheduleEntry) error {
	db, err := GetSharedDB()
	if err != nil {
		return err
	}
	_, err = db.Exec(`INSERT OR REPLACE INTO schedules (id, profile_name, action, cron_expr, enabled, last_run, next_run, last_result, created_at)
		VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)`,
		e.Id, e.ProfileName, e.Action, e.CronExpr, boolToInt(e.Enabled),
		timePtrToNullable(e.LastRun), timePtrToNullable(e.NextRun),
		e.LastResult, e.CreatedAt.UTC().Format(time.RFC3339))
	return err
}

// deleteScheduleFromDB removes a schedule from the database
func (s *SchedulerService) deleteScheduleFromDB(id string) error {
	db, err := GetSharedDB()
	if err != nil {
		return err
	}
	_, err = db.Exec("DELETE FROM schedules WHERE id = ?", id)
	return err
}

// emitScheduleEvent emits a schedule event
func (s *SchedulerService) emitScheduleEvent(eventType events.EventType, scheduleId string, data interface{}) {
	event := events.NewScheduleEvent(eventType, scheduleId, data)
	if s.eventBus != nil {
		if err := s.eventBus.EmitScheduleEvent(event); err != nil {
			log.Printf("Failed to emit schedule event: %v", err)
		}
	} else if s.app != nil {
		s.app.Event.Emit("tofe", event)
	}
}
