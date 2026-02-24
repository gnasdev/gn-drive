package services

import (
	"context"
	beConfig "desktop/backend/config"
	"desktop/backend/dto"
	"desktop/backend/events"
	"desktop/backend/models"
	"desktop/backend/rclone"
	"desktop/backend/utils"
	"fmt"
	"log"
	"strings"
	"sync"
	"time"

	"github.com/wailsapp/wails/v3/pkg/application"
)

// OperationTask represents an active non-sync operation
type OperationTask struct {
	Id        int
	Operation string // "copy", "move", "check", "dedupe"
	Profile   models.Profile
	TabId     string
	Cancel    context.CancelFunc
	StartTime time.Time
	EndTime   *time.Time
	Status    string
}

// OperationService handles non-sync rclone operations (copy, move, check, dedupe, file browser, etc.)
type OperationService struct {
	app         *application.App
	eventBus    *events.WailsEventBus
	activeTasks map[int]*OperationTask
	taskCounter int
	mutex       sync.RWMutex
	envConfig   beConfig.Config
}

// NewOperationService creates a new operation service
func NewOperationService(app *application.App) *OperationService {
	return &OperationService{
		app:         app,
		activeTasks: make(map[int]*OperationTask),
	}
}

// SetApp sets the application reference for events
func (o *OperationService) SetApp(app *application.App) {
	o.app = app
	if bus := GetSharedEventBus(); bus != nil {
		o.eventBus = bus
	} else {
		o.eventBus = events.NewEventBus(app)
	}
}

// SetEnvConfig sets the environment configuration
func (o *OperationService) SetEnvConfig(config beConfig.Config) {
	o.envConfig = config
	// Initialize rclone global state once
	if err := rclone.InitGlobal(config.DebugMode); err != nil {
		log.Printf("WARNING: Failed to initialize rclone: %v", err)
	}
}

// ServiceName returns the name of the service
func (o *OperationService) ServiceName() string {
	return "OperationService"
}

// ServiceStartup is called when the service starts
func (o *OperationService) ServiceStartup(ctx context.Context, options application.ServiceOptions) error {
	log.Printf("OperationService starting up...")
	return nil
}

// ServiceShutdown is called when the service shuts down
func (o *OperationService) ServiceShutdown(ctx context.Context) error {
	log.Printf("OperationService shutting down...")
	o.mutex.Lock()
	defer o.mutex.Unlock()
	for _, task := range o.activeTasks {
		if task.Cancel != nil {
			task.Cancel()
		}
	}
	return nil
}

// Copy starts a one-way copy operation
func (o *OperationService) Copy(ctx context.Context, profile models.Profile, tabId string) (int, error) {
	return o.startOperation(ctx, "copy", profile, tabId)
}

// Move starts a move operation (copy + delete source)
func (o *OperationService) Move(ctx context.Context, profile models.Profile, tabId string) (int, error) {
	return o.startOperation(ctx, "move", profile, tabId)
}

// Check starts a check/verify operation
func (o *OperationService) CheckFiles(ctx context.Context, profile models.Profile, tabId string) (int, error) {
	return o.startOperation(ctx, "check", profile, tabId)
}

// DryRun runs the specified action in dry-run mode (preview only)
func (o *OperationService) DryRun(ctx context.Context, action string, profile models.Profile, tabId string) (int, error) {
	return o.startOperation(ctx, "dryrun:"+action, profile, tabId)
}

// ListFiles lists files at the given remote path
func (o *OperationService) ListFiles(ctx context.Context, remotePath string, recursive bool) ([]models.FileEntry, error) {
	opCtx, err := rclone.SimpleContext(ctx)
	if err != nil {
		return nil, fmt.Errorf("failed to initialize rclone config: %w", err)
	}
	return rclone.ListFiles(opCtx, remotePath, recursive)
}

// DeleteFile deletes a single file at the given remote path
func (o *OperationService) DeleteFile(ctx context.Context, remotePath string) error {
	opCtx, err := rclone.SimpleContext(ctx)
	if err != nil {
		return fmt.Errorf("failed to initialize rclone config: %w", err)
	}
	return rclone.DeleteFile(opCtx, remotePath)
}

// PurgeDir removes the directory and all its contents
func (o *OperationService) PurgeDir(ctx context.Context, remotePath string) error {
	opCtx, err := rclone.SimpleContext(ctx)
	if err != nil {
		return fmt.Errorf("failed to initialize rclone config: %w", err)
	}
	return rclone.Purge(opCtx, remotePath)
}

// MakeDir creates a directory at the given remote path
func (o *OperationService) MakeDir(ctx context.Context, remotePath string) error {
	opCtx, err := rclone.SimpleContext(ctx)
	if err != nil {
		return fmt.Errorf("failed to initialize rclone config: %w", err)
	}
	return rclone.Mkdir(opCtx, remotePath)
}

// GetAbout returns quota information for the given remote
func (o *OperationService) GetAbout(ctx context.Context, remoteName string) (*models.QuotaInfo, error) {
	opCtx, err := rclone.SimpleContext(ctx)
	if err != nil {
		return nil, fmt.Errorf("failed to initialize rclone config: %w", err)
	}
	return rclone.About(opCtx, remoteName)
}

// GetSize returns the total objects and size at the given remote path
func (o *OperationService) GetSize(ctx context.Context, remotePath string) (int64, int64, error) {
	opCtx, err := rclone.SimpleContext(ctx)
	if err != nil {
		return 0, 0, fmt.Errorf("failed to initialize rclone config: %w", err)
	}
	return rclone.GetSize(opCtx, remotePath)
}

// StopOperation stops a running operation by task ID
func (o *OperationService) StopOperation(ctx context.Context, taskId int) error {
	o.mutex.Lock()
	defer o.mutex.Unlock()

	task, exists := o.activeTasks[taskId]
	if !exists {
		return fmt.Errorf("task %d not found", taskId)
	}

	if task.Cancel != nil {
		task.Cancel()
	}

	task.Status = "cancelled"
	o.emitOperationEvent(events.OperationFailed, task.TabId, task.Operation, "cancelled", "Operation cancelled")
	delete(o.activeTasks, taskId)
	return nil
}

// GetActiveTasks returns a copy of active operation tasks
func (o *OperationService) GetActiveTasks(ctx context.Context) (map[int]*OperationTask, error) {
	o.mutex.RLock()
	defer o.mutex.RUnlock()

	tasks := make(map[int]*OperationTask)
	for id, task := range o.activeTasks {
		tasks[id] = task
	}
	return tasks, nil
}

// startOperation starts an async operation
func (o *OperationService) startOperation(ctx context.Context, operation string, profile models.Profile, tabId string) (int, error) {
	o.mutex.Lock()
	o.taskCounter++
	taskId := o.taskCounter
	taskCtx, cancel := context.WithCancel(ctx)

	task := &OperationTask{
		Id:        taskId,
		Operation: operation,
		Profile:   profile,
		TabId:     tabId,
		Cancel:    cancel,
		StartTime: time.Now(),
		Status:    "starting",
	}

	o.activeTasks[taskId] = task
	o.mutex.Unlock()

	o.emitOperationEvent(events.OperationStarted, tabId, operation, "starting", fmt.Sprintf("Starting %s operation", operation))

	go o.executeOperation(taskCtx, task)
	return taskId, nil
}

// executeOperation runs the operation asynchronously
func (o *OperationService) executeOperation(ctx context.Context, task *OperationTask) {
	defer func() {
		o.mutex.Lock()
		delete(o.activeTasks, task.Id)
		o.mutex.Unlock()
	}()

	// Initialize rclone config with isolated context
	ctx, err := rclone.NewTaskContext(ctx, task.Id)
	if err != nil {
		o.handleOperationError(task, fmt.Sprintf("Failed to initialize rclone config: %v", err))
		return
	}

	// Apply on-the-fly crypt wrapping if configured
	cryptCleanup, err := rclone.ApplyCryptWrapping(ctx, &task.Profile)
	if err != nil {
		o.handleOperationError(task, fmt.Sprintf("Failed to setup encryption: %v", err))
		return
	}
	defer cryptCleanup()

	outStatus := make(chan *dto.SyncStatusDTO, 100)

	utils.AddCmd(task.Id, func() {
		close(outStatus)
		task.Cancel()
	})

	if task.TabId != "" {
		utils.AddTabMapping(task.Id, task.TabId)
	}

	// Consume structured SyncStatusDTO and dispatch events
	go func() {
		for status := range outStatus {
			// Enrich DTO with task identity
			status.Id = &task.Id
			status.TabId = &task.TabId
			status.Action = task.Operation

			// Extract LogMessages for text-based event consumers
			for _, logMsg := range status.LogMessages {
				if o.eventBus != nil {
					event := events.NewOperationEvent(events.OperationProgress, task.TabId, task.Operation, "running", logMsg)
					if emitErr := o.eventBus.EmitOperationEvent(event); emitErr != nil {
						log.Printf("Failed to emit operation progress event: %v", emitErr)
					}
				}
			}

			// Emit structured SyncStatusDTO for frontend
			if o.eventBus != nil {
				if emitErr := o.eventBus.Emit(status); emitErr != nil {
					log.Printf("Failed to emit operation status: %v", emitErr)
				}
			}
		}
	}()

	task.Status = "running"
	o.emitOperationEvent(events.OperationProgress, task.TabId, task.Operation, "running", "Operation in progress")

	config := o.envConfig

	// Handle dry-run prefix: "dryrun:copy" -> set DryRun flag and run "copy"
	operation := task.Operation
	if strings.HasPrefix(operation, "dryrun:") {
		operation = strings.TrimPrefix(operation, "dryrun:")
		task.Profile.DryRun = true
	}

	switch operation {
	case "copy":
		err = rclone.Copy(ctx, config, task.Profile, outStatus)
	case "move":
		err = rclone.Move(ctx, config, task.Profile, outStatus)
	case "check":
		err = rclone.Check(ctx, config, task.Profile, outStatus)
	default:
		err = fmt.Errorf("unknown operation: %s", operation)
	}
	if task.TabId != "" {
		utils.RemoveTabMapping(task.Id)
	}

	// Check if cancelled
	select {
	case <-ctx.Done():
		task.Status = "cancelled"
		o.emitOperationEvent(events.OperationFailed, task.TabId, task.Operation, "cancelled", "Operation was cancelled")
		return
	default:
	}

	if err != nil {
		task.Status = "failed"
		o.handleOperationError(task, fmt.Sprintf("Operation failed: %v", err))
		return
	}

	task.Status = "completed"
	endTime := time.Now()
	task.EndTime = &endTime
	o.emitOperationEvent(events.OperationCompleted, task.TabId, task.Operation, "completed", "Operation completed successfully")
}

// emitOperationEvent emits an operation event
func (o *OperationService) emitOperationEvent(eventType events.EventType, tabId, operation, status, message string) {
	event := events.NewOperationEvent(eventType, tabId, operation, status, message)
	if o.eventBus != nil {
		if err := o.eventBus.EmitOperationEvent(event); err != nil {
			log.Printf("Failed to emit operation event: %v", err)
		}
	} else if o.app != nil {
		o.app.Event.Emit("tofe", event)
	}
}

// handleOperationError handles operation errors
func (o *OperationService) handleOperationError(task *OperationTask, errorMsg string) {
	log.Printf("Operation error (task %d): %s", task.Id, errorMsg)

	errorEvent := events.NewErrorEvent("OPERATION_ERROR", errorMsg, "", task.TabId)
	if o.eventBus != nil {
		if err := o.eventBus.EmitErrorEvent(errorEvent); err != nil {
			log.Printf("Failed to emit error event: %v", err)
		}
	}

	o.emitOperationEvent(events.OperationFailed, task.TabId, task.Operation, "failed", errorMsg)
}
