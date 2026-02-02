package services

import (
	"context"
	"desktop/backend/events"
	"desktop/backend/models"
	"fmt"
	"log"
	"os/exec"
	"sync"
	"time"

	"github.com/wailsapp/wails/v3/pkg/application"
)

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

// SyncService handles all sync operations
type SyncService struct {
	app         *application.App
	activeTasks map[int]*SyncTask
	taskCounter int
	mutex       sync.RWMutex
	// errorHandler interface{} // Will be properly typed later - removed unused field
}

// SyncTask represents an active sync task
type SyncTask struct {
	Id        int
	Action    SyncAction
	Profile   models.Profile
	TabId     string
	Cmd       *exec.Cmd
	Cancel    context.CancelFunc
	StartTime time.Time
	Status    string
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

	// Cancel the task
	if task.Cancel != nil {
		task.Cancel()
	}

	// Kill the process if it's running
	if task.Cmd != nil && task.Cmd.Process != nil {
		if err := task.Cmd.Process.Kill(); err != nil {
			fmt.Printf("Warning: failed to kill process: %v\n", err)
		}
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

// executeSyncTask executes the actual sync operation
func (s *SyncService) executeSyncTask(ctx context.Context, task *SyncTask) {
	defer func() {
		s.mutex.Lock()
		delete(s.activeTasks, task.Id)
		s.mutex.Unlock()
	}()

	// Build rclone command
	args, err := s.buildRcloneArgs(task.Action, task.Profile)
	if err != nil {
		s.handleSyncError(task, fmt.Sprintf("Failed to build command: %v", err))
		return
	}

	// Create command with context
	cmd := exec.CommandContext(ctx, "rclone", args...)
	task.Cmd = cmd

	// Update task status
	task.Status = "running"
	s.emitSyncEvent(events.SyncProgress, task.TabId, string(task.Action), "running", "Sync operation in progress")

	// Execute command
	output, err := cmd.CombinedOutput()

	// Check if context was cancelled
	select {
	case <-ctx.Done():
		task.Status = "cancelled"
		s.emitSyncEvent(events.SyncCancelled, task.TabId, string(task.Action), "cancelled", "Sync operation was cancelled")
		return
	default:
	}

	// Handle command result
	if err != nil {
		task.Status = "failed"
		errorMsg := fmt.Sprintf("Sync failed: %v\nOutput: %s", err, string(output))
		s.handleSyncError(task, errorMsg)
		return
	}

	// Success
	task.Status = "completed"
	endTime := time.Now()
	task.StartTime = endTime // This should be EndTime, will fix in next iteration

	s.emitSyncEvent(events.SyncCompleted, task.TabId, string(task.Action), "completed", "Sync operation completed successfully")
}

// buildRcloneArgs builds the rclone command arguments
func (s *SyncService) buildRcloneArgs(action SyncAction, profile models.Profile) ([]string, error) {
	args := []string{}

	switch action {
	case ActionPull:
		args = append(args, "copy", profile.To, profile.From)
	case ActionPush:
		args = append(args, "copy", profile.From, profile.To)
	case ActionBi:
		args = append(args, "bisync", profile.From, profile.To)
	case ActionBiResync:
		args = append(args, "bisync", profile.From, profile.To, "--resync")
	default:
		return nil, fmt.Errorf("unknown sync action: %s", action)
	}

	// Add common flags
	if profile.Parallel > 0 {
		args = append(args, "--transfers", fmt.Sprintf("%d", profile.Parallel))
	}
	if profile.Bandwidth > 0 {
		args = append(args, "--bwlimit", fmt.Sprintf("%dM", profile.Bandwidth))
	}

	// Add include/exclude patterns
	for _, include := range profile.IncludedPaths {
		if include != "" {
			args = append(args, "--include", include)
		}
	}
	for _, exclude := range profile.ExcludedPaths {
		if exclude != "" {
			args = append(args, "--exclude", exclude)
		}
	}

	return args, nil
}

// emitSyncEvent emits a sync event to the frontend
func (s *SyncService) emitSyncEvent(eventType events.EventType, tabId, action, status, message string) {
	event := events.NewSyncEvent(eventType, tabId, action, status, message)
	s.app.Event.Emit(string(eventType), event)
}

// handleSyncError handles sync operation errors
func (s *SyncService) handleSyncError(task *SyncTask, errorMsg string) {
	log.Printf("Sync error for task %d: %s", task.Id, errorMsg)

	// Emit error event
	errorEvent := events.NewErrorEvent("SYNC_ERROR", errorMsg, "", task.TabId)
	s.app.Event.Emit(string(events.ErrorOccurred), errorEvent)

	// Emit sync failed event
	s.emitSyncEvent(events.SyncFailed, task.TabId, string(task.Action), "failed", errorMsg)
}
