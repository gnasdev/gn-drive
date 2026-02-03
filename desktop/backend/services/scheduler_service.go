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
	filePath    string
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

// ServiceStartup is called when the service starts
func (s *SchedulerService) ServiceStartup(ctx context.Context, options application.ServiceOptions) error {
	log.Printf("SchedulerService starting up...")
	if err := s.initialize(); err != nil {
		return err
	}
	s.cron.Start()
	return nil
}

// ServiceShutdown is called when the service shuts down
func (s *SchedulerService) ServiceShutdown(ctx context.Context) error {
	log.Printf("SchedulerService shutting down...")
	s.cron.Stop()
	return nil
}

// initialize sets up the file path and loads existing schedules
func (s *SchedulerService) initialize() error {
	s.mutex.Lock()
	defer s.mutex.Unlock()

	if s.initialized {
		return nil
	}

	homeDir, err := os.UserHomeDir()
	if err != nil {
		homeDir = "."
	}

	s.filePath = filepath.Join(homeDir, ".config", "ns-drive", "schedules.json")

	if err := os.MkdirAll(filepath.Dir(s.filePath), 0755); err != nil {
		return fmt.Errorf("failed to create schedules directory: %w", err)
	}

	if err := s.loadFromFile(); err != nil {
		log.Printf("Warning: Could not load schedules: %v", err)
		s.schedules = []models.ScheduleEntry{}
	}

	// Register enabled schedules with cron
	for i := range s.schedules {
		if s.schedules[i].Enabled {
			if err := s.registerCronJob(&s.schedules[i]); err != nil {
				log.Printf("Warning: Failed to register schedule %s: %v", s.schedules[i].Id, err)
			}
		}
	}

	s.initialized = true
	log.Printf("SchedulerService initialized with %d schedules", len(s.schedules))
	return nil
}

// AddSchedule adds a new schedule entry
func (s *SchedulerService) AddSchedule(ctx context.Context, entry models.ScheduleEntry) error {
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

	if err := s.saveToFile(); err != nil {
		s.unregisterCronJob(entry.Id)
		s.schedules = s.schedules[:len(s.schedules)-1]
		return fmt.Errorf("failed to save schedules: %w", err)
	}

	s.emitScheduleEvent(events.ScheduleAdded, entry.Id, entry)
	log.Printf("Schedule '%s' added for profile '%s'", entry.Id, entry.ProfileName)
	return nil
}

// UpdateSchedule updates an existing schedule
func (s *SchedulerService) UpdateSchedule(ctx context.Context, entry models.ScheduleEntry) error {
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

	if err := s.saveToFile(); err != nil {
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
		return fmt.Errorf("failed to save schedules: %w", err)
	}

	s.emitScheduleEvent(events.ScheduleUpdated, entry.Id, entry)
	return nil
}

// DeleteSchedule removes a schedule
func (s *SchedulerService) DeleteSchedule(ctx context.Context, scheduleId string) error {
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

	if err := s.saveToFile(); err != nil {
		s.schedules = append(s.schedules, deletedEntry)
		return fmt.Errorf("failed to save schedules: %w", err)
	}

	s.emitScheduleEvent(events.ScheduleDeleted, scheduleId, deletedEntry)
	return nil
}

// GetSchedules returns all schedules
func (s *SchedulerService) GetSchedules(ctx context.Context) ([]models.ScheduleEntry, error) {
	s.mutex.RLock()
	defer s.mutex.RUnlock()

	result := make([]models.ScheduleEntry, len(s.schedules))
	copy(result, s.schedules)
	return result, nil
}

// EnableSchedule enables a schedule
func (s *SchedulerService) EnableSchedule(ctx context.Context, scheduleId string) error {
	s.mutex.Lock()
	defer s.mutex.Unlock()

	for i, entry := range s.schedules {
		if entry.Id == scheduleId {
			s.schedules[i].Enabled = true
			if err := s.registerCronJob(&s.schedules[i]); err != nil {
				return fmt.Errorf("failed to register cron job: %w", err)
			}
			if err := s.saveToFile(); err != nil {
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
	s.mutex.Lock()
	defer s.mutex.Unlock()

	for i, entry := range s.schedules {
		if entry.Id == scheduleId {
			s.schedules[i].Enabled = false
			s.unregisterCronJob(scheduleId)
			if err := s.saveToFile(); err != nil {
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
					_ = s.saveToFile()
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

			_ = s.saveToFile()
			break
		}
	}
	s.mutex.Unlock()
}

// loadFromFile loads schedules from the JSON file
func (s *SchedulerService) loadFromFile() error {
	data, err := os.ReadFile(s.filePath)
	if err != nil {
		if os.IsNotExist(err) {
			return nil
		}
		return err
	}

	if len(data) == 0 {
		return nil
	}

	return json.Unmarshal(data, &s.schedules)
}

// saveToFile saves schedules to the JSON file
func (s *SchedulerService) saveToFile() error {
	data, err := json.MarshalIndent(s.schedules, "", "  ")
	if err != nil {
		return err
	}
	return os.WriteFile(s.filePath, data, 0644)
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
