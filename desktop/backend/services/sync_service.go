package services

import (
	"context"
	beConfig "desktop/backend/config"
	"desktop/backend/events"
	"desktop/backend/models"
	"desktop/backend/rclone"
	"desktop/backend/utils"
	"fmt"
	"log"
	"sync"
	"time"

	"github.com/wailsapp/wails/v3/pkg/application"
)

// eventBusProvider is a shared EventBus instance for services
var sharedEventBus *events.WailsEventBus
var eventBusMutex sync.RWMutex

// GetSharedEventBus returns the shared EventBus instance
func GetSharedEventBus() *events.WailsEventBus {
	eventBusMutex.RLock()
	defer eventBusMutex.RUnlock()
	return sharedEventBus
}

// SetSharedEventBus sets the shared EventBus instance
func SetSharedEventBus(bus *events.WailsEventBus) {
	eventBusMutex.Lock()
	defer eventBusMutex.Unlock()
	sharedEventBus = bus
}

// SyncAction represents the type of sync operation
type SyncAction string

const (
	ActionPull     SyncAction = "pull"
	ActionPush     SyncAction = "push"
	ActionBi       SyncAction = "bi"
	ActionBiResync SyncAction = "bi-resync"
)

// SyncResult represents the result of a sync operation
type SyncResult struct {
	TaskId    int        `json:"taskId"`
	Action    string     `json:"action"`
	Status    string     `json:"status"`
	Message   string     `json:"message"`
	StartTime time.Time  `json:"startTime"`
	EndTime   *time.Time `json:"endTime,omitempty"`
}

// SyncService handles all sync operations using the rclone Go library
type SyncService struct {
	app         *application.App
	eventBus    *events.WailsEventBus
	activeTasks map[int]*SyncTask
	taskCounter int
	mutex       sync.RWMutex
	envConfig   beConfig.Config
}

// SyncTask represents an active sync task
type SyncTask struct {
	Id        int
	Action    SyncAction
	Profile   models.Profile
	TabId     string
	Cancel    context.CancelFunc
	StartTime time.Time
	EndTime   *time.Time
	Status    string
	Done      chan error // closed with result when task completes
}

// NewSyncService creates a new sync service
func NewSyncService(app *application.App) *SyncService {
	return &SyncService{
		app:         app,
		activeTasks: make(map[int]*SyncTask),
		taskCounter: 0,
	}
}

// SetApp sets the application reference for events
func (s *SyncService) SetApp(app *application.App) {
	s.app = app
	// Initialize EventBus with the app
	s.eventBus = events.NewEventBus(app)
	SetSharedEventBus(s.eventBus)
}

// SetEnvConfig sets the environment configuration (needed for rclone init)
func (s *SyncService) SetEnvConfig(config beConfig.Config) {
	s.envConfig = config
}

// ServiceName returns the name of the service
func (s *SyncService) ServiceName() string {
	return "SyncService"
}

// ServiceStartup is called when the service starts
func (s *SyncService) ServiceStartup(ctx context.Context, options application.ServiceOptions) error {
	log.Printf("SyncService starting up...")
	return nil
}

// ServiceShutdown is called when the service shuts down
func (s *SyncService) ServiceShutdown(ctx context.Context) error {
	log.Printf("SyncService shutting down...")

	// Cancel all active tasks
	s.mutex.Lock()
	defer s.mutex.Unlock()

	for _, task := range s.activeTasks {
		if task.Cancel != nil {
			task.Cancel()
		}
	}

	return nil
}

// StartSync starts a sync operation with context support
func (s *SyncService) StartSync(ctx context.Context, action string, profile models.Profile, tabId string) (*SyncResult, error) {
	s.mutex.Lock()
	defer s.mutex.Unlock()

	// Check if context is already cancelled
	select {
	case <-ctx.Done():
		return nil, ctx.Err()
	default:
	}

	// Create new task
	s.taskCounter++
	taskId := s.taskCounter

	// Create cancellable context for the task
	taskCtx, cancel := context.WithCancel(ctx)

	task := &SyncTask{
		Id:        taskId,
		Action:    SyncAction(action),
		Profile:   profile,
		TabId:     tabId,
		Cancel:    cancel,
		StartTime: time.Now(),
		Status:    "starting",
		Done:      make(chan error, 1),
	}

	s.activeTasks[taskId] = task

	// Emit sync started event
	s.emitSyncEvent(events.SyncStarted, tabId, action, "starting", "Sync operation started")

	// Start sync operation in goroutine
	go s.executeSyncTask(taskCtx, task)

	result := &SyncResult{
		TaskId:    taskId,
		Action:    action,
		Status:    "started",
		Message:   "Sync operation initiated",
		StartTime: task.StartTime,
	}

	return result, nil
}

// StopSync stops a running sync operation
func (s *SyncService) StopSync(ctx context.Context, taskId int) error {
	s.mutex.Lock()
	defer s.mutex.Unlock()

	task, exists := s.activeTasks[taskId]
	if !exists {
		return fmt.Errorf("task %d not found", taskId)
	}

	// Cancel the task context (this cancels the rclone Go library operation)
	if task.Cancel != nil {
		task.Cancel()
	}

	// Update task status
	task.Status = "cancelled"

	// Emit cancelled event
	s.emitSyncEvent(events.SyncCancelled, task.TabId, string(task.Action), "cancelled", "Sync operation cancelled")

	// Remove from active tasks
	delete(s.activeTasks, taskId)

	return nil
}

// GetActiveTasks returns all currently active sync tasks
func (s *SyncService) GetActiveTasks(ctx context.Context) (map[int]*SyncTask, error) {
	s.mutex.RLock()
	defer s.mutex.RUnlock()

	// Create a copy to avoid race conditions
	tasks := make(map[int]*SyncTask)
	for id, task := range s.activeTasks {
		tasks[id] = task
	}

	return tasks, nil
}

// WaitForTask blocks until the given task completes and returns its error (nil on success)
func (s *SyncService) WaitForTask(ctx context.Context, taskId int) error {
	s.mutex.RLock()
	task, exists := s.activeTasks[taskId]
	s.mutex.RUnlock()

	if !exists {
		return fmt.Errorf("task %d not found", taskId)
	}

	select {
	case err := <-task.Done:
		return err
	case <-ctx.Done():
		return ctx.Err()
	}
}

// executeSyncTask executes the sync operation using the rclone Go library
func (s *SyncService) executeSyncTask(ctx context.Context, task *SyncTask) {
	var taskErr error
	defer func() {
		task.Done <- taskErr
		close(task.Done)
		s.mutex.Lock()
		delete(s.activeTasks, task.Id)
		s.mutex.Unlock()
	}()

	// Initialize rclone config
	ctx, err := rclone.InitConfig(ctx, s.envConfig.DebugMode)
	if err != nil {
		taskErr = fmt.Errorf("failed to initialize rclone config: %w", err)
		s.handleSyncError(task, taskErr.Error())
		return
	}

	// Create output log channel for progress reporting
	outLog := make(chan string, 100)

	// Start sync status reporting
	stopSyncStatus := utils.StartSyncStatusReporting(nil, task.Id, string(task.Action), task.TabId)

	// Register cancel function in the global command store
	utils.AddCmd(task.Id, func() {
		stopSyncStatus()
		close(outLog)
		task.Cancel()
	})

	if task.TabId != "" {
		utils.AddTabMapping(task.Id, task.TabId)
	}

	// Goroutine to read output logs and emit progress events
	go func() {
		for logEntry := range outLog {
			if s.eventBus != nil {
				event := events.NewSyncEvent(events.SyncProgress, task.TabId, string(task.Action), "running", logEntry)
				if emitErr := s.eventBus.EmitSyncEvent(event); emitErr != nil {
					log.Printf("Failed to emit progress event: %v", emitErr)
				}
			}
		}
	}()

	// Update task status
	task.Status = "running"
	s.emitSyncEvent(events.SyncProgress, task.TabId, string(task.Action), "running", "Sync operation in progress")

	// Execute the sync operation using rclone Go library
	config := s.envConfig
	switch task.Action {
	case ActionPull:
		err = rclone.Sync(ctx, config, "pull", task.Profile, outLog)
	case ActionPush:
		err = rclone.Sync(ctx, config, "push", task.Profile, outLog)
	case ActionBi:
		err = rclone.BiSync(ctx, config, task.Profile, false, outLog)
	case ActionBiResync:
		err = rclone.BiSync(ctx, config, task.Profile, true, outLog)
	default:
		err = fmt.Errorf("unknown sync action: %s", task.Action)
	}

	// Clean up
	stopSyncStatus()
	if task.TabId != "" {
		utils.RemoveTabMapping(task.Id)
	}

	// Check if context was cancelled
	select {
	case <-ctx.Done():
		task.Status = "cancelled"
		taskErr = ctx.Err()
		s.emitSyncEvent(events.SyncCancelled, task.TabId, string(task.Action), "cancelled", "Sync operation was cancelled")
		return
	default:
	}

	// Handle result
	if err != nil {
		task.Status = "failed"
		taskErr = fmt.Errorf("sync failed: %w", err)
		s.handleSyncError(task, taskErr.Error())
		return
	}

	// Success
	task.Status = "completed"
	endTime := time.Now()
	task.EndTime = &endTime

	s.emitSyncEvent(events.SyncCompleted, task.TabId, string(task.Action), "completed", "Sync operation completed successfully")
}

// emitSyncEvent emits a sync event to the frontend via unified EventBus
func (s *SyncService) emitSyncEvent(eventType events.EventType, tabId, action, status, message string) {
	event := events.NewSyncEvent(eventType, tabId, action, status, message)
	if s.eventBus != nil {
		if err := s.eventBus.EmitSyncEvent(event); err != nil {
			log.Printf("Failed to emit sync event: %v", err)
		}
	} else if s.app != nil {
		// Fallback to direct emission if EventBus not initialized
		s.app.Event.Emit("tofe", event)
	}
}

// handleSyncError handles sync operation errors
func (s *SyncService) handleSyncError(task *SyncTask, errorMsg string) {
	log.Printf("Sync error for task %d: %s", task.Id, errorMsg)

	// Emit error event via unified EventBus
	errorEvent := events.NewErrorEvent("SYNC_ERROR", errorMsg, "", task.TabId)
	if s.eventBus != nil {
		if err := s.eventBus.EmitErrorEvent(errorEvent); err != nil {
			log.Printf("Failed to emit error event: %v", err)
		}
	}

	// Emit sync failed event
	s.emitSyncEvent(events.SyncFailed, task.TabId, string(task.Action), "failed", errorMsg)
}
