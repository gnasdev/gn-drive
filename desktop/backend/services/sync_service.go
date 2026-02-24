package services

import (
	"context"
	beConfig "desktop/backend/config"
	"desktop/backend/delta"
	"desktop/backend/dto"
	"desktop/backend/events"
	"desktop/backend/models"
	"desktop/backend/rclone"
	"desktop/backend/utils"
	"fmt"
	"log"
	"os"
	"strings"
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

// SetSharedEventBusWindow sets the window on the shared EventBus for window-specific events
func SetSharedEventBusWindow(window *application.WebviewWindow) {
	eventBusMutex.RLock()
	defer eventBusMutex.RUnlock()
	if sharedEventBus != nil {
		sharedEventBus.SetWindow(window)
	}
}

// Board log buffer for polling (workaround for Wails v3 event issues)
var boardLogBuffer []string
var boardLogMutex sync.RWMutex
var boardLogBufferSize = 200

// AppendBoardLog adds a log entry for board execution
func AppendBoardLog(logEntry string) {
	boardLogMutex.Lock()
	defer boardLogMutex.Unlock()
	boardLogBuffer = append(boardLogBuffer, logEntry)
	if len(boardLogBuffer) > boardLogBufferSize {
		boardLogBuffer = boardLogBuffer[len(boardLogBuffer)-boardLogBufferSize:]
	}
}

// GetBoardLogs returns and clears board logs
func GetBoardLogs() []string {
	boardLogMutex.Lock()
	defer boardLogMutex.Unlock()
	logs := make([]string, len(boardLogBuffer))
	copy(logs, boardLogBuffer)
	boardLogBuffer = boardLogBuffer[:0]
	return logs
}

// ClearBoardLogs clears the board log buffer
func ClearBoardLogs() {
	boardLogMutex.Lock()
	defer boardLogMutex.Unlock()
	boardLogBuffer = boardLogBuffer[:0]
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
	app                 *application.App
	eventBus            *events.WailsEventBus
	logService          *LogService
	notificationService *NotificationService
	activeTasks         map[int]*SyncTask
	taskCounter         int
	mutex               sync.RWMutex
	envConfig           beConfig.Config
	deltaSvc            *delta.DeltaService
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
	// Initialize rclone global state once
	if err := rclone.InitGlobal(config.DebugMode); err != nil {
		log.Printf("WARNING: Failed to initialize rclone: %v", err)
	}
	// Initialize delta service for change-notification-based sync optimization
	store := delta.NewDeltaStore(GetSharedDB)
	s.deltaSvc = delta.NewDeltaService(store)
}

// SetLogService sets the log service for reliable log delivery
func (s *SyncService) SetLogService(logService *LogService) {
	s.logService = logService
}

// SetNotificationService sets the notification service for desktop notifications
func (s *SyncService) SetNotificationService(notificationService *NotificationService) {
	s.notificationService = notificationService
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

	// Stop all delta watchers
	if s.deltaSvc != nil {
		s.deltaSvc.StopAll()
	}

	return nil
}

// StartSync starts a sync operation with context support
func (s *SyncService) StartSync(ctx context.Context, action string, profile models.Profile, tabId string) (*SyncResult, error) {
	log.Printf("[SyncService] StartSync called: action=%s tabId=%s from=%s to=%s", action, tabId, profile.From, profile.To)

	s.mutex.Lock()
	defer s.mutex.Unlock()

	// Check if context is already cancelled
	select {
	case <-ctx.Done():
		log.Printf("[SyncService] StartSync: context already cancelled")
		return nil, ctx.Err()
	default:
	}

	// Create new task
	s.taskCounter++
	taskId := s.taskCounter
	log.Printf("[SyncService] StartSync: created taskId=%d", taskId)

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
	log.Printf("[SyncService] executeSyncTask started: taskId=%d action=%s tabId=%s from=%s to=%s", task.Id, task.Action, task.TabId, task.Profile.From, task.Profile.To)
	var taskErr error
	defer func() {
		log.Printf("[SyncService] executeSyncTask finished: taskId=%d err=%v", task.Id, taskErr)
		task.Done <- taskErr
		close(task.Done)
		s.mutex.Lock()
		delete(s.activeTasks, task.Id)
		s.mutex.Unlock()
	}()

	// Create isolated rclone context for this task
	ctx, err := rclone.NewTaskContext(ctx, task.Id)
	if err != nil {
		taskErr = fmt.Errorf("failed to create rclone task context: %w", err)
		s.handleSyncError(task, taskErr.Error())
		return
	}

	// Apply on-the-fly crypt wrapping if configured
	cryptCleanup, err := rclone.ApplyCryptWrapping(ctx, &task.Profile)
	if err != nil {
		taskErr = fmt.Errorf("failed to setup encryption: %w", err)
		s.handleSyncError(task, taskErr.Error())
		return
	}
	defer cryptCleanup()

	// Create structured status channel
	outStatus := make(chan *dto.SyncStatusDTO, 100)
	var outStatusClosed bool
	var outStatusMutex sync.Mutex
	closeOutStatus := func() {
		outStatusMutex.Lock()
		defer outStatusMutex.Unlock()
		if !outStatusClosed {
			outStatusClosed = true
			close(outStatus)
		}
	}

	// Register cancel function in the global command store
	utils.AddCmd(task.Id, func() {
		closeOutStatus()
		task.Cancel()
	})

	if task.TabId != "" {
		utils.AddTabMapping(task.Id, task.TabId)
	}

	// Goroutine to consume structured SyncStatusDTO and dispatch events + logs
	go func() {
		isBoardTask := strings.HasPrefix(task.TabId, "board-")
		for status := range outStatus {
			// Enrich DTO with task identity (library layer doesn't set these)
			status.Id = &task.Id
			status.TabId = &task.TabId
			status.Action = string(task.Action)

			// Dispatch LogMessages to board log buffer and log service.
			// NOTE: Do NOT re-log to stderr here — rclone's slog handler already
			// writes these messages to stderr. Re-logging via log.Printf would
			// create a feedback loop (Go 1.22+ redirects log.Printf through slog,
			// which rclone captures via AddOutput). Even fmt.Fprintf would cause
			// duplicate output since rclone already wrote the original message.
			for _, logMsg := range status.LogMessages {
				if isBoardTask {
					AppendBoardLog(logMsg)
				}
				if s.logService != nil {
					s.logService.LogSync(task.TabId, string(task.Action), status.Status, logMsg)
				}
			}

			// Emit structured SyncStatusDTO for frontend
			if s.eventBus != nil {
				if emitErr := s.eventBus.Emit(status); emitErr != nil {
					fmt.Fprintf(os.Stderr, "Failed to emit sync status: %v\n", emitErr)
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
		err = rclone.Sync(ctx, config, "pull", task.Profile, outStatus, s.deltaSvc)
	case ActionPush:
		err = rclone.Sync(ctx, config, "push", task.Profile, outStatus, s.deltaSvc)
	case ActionBi:
		err = rclone.BiSync(ctx, config, task.Profile, false, outStatus, s.deltaSvc)
	case ActionBiResync:
		err = rclone.BiSync(ctx, config, task.Profile, true, outStatus, s.deltaSvc)
	default:
		err = fmt.Errorf("unknown sync action: %s", task.Action)
	}

	// Close the outStatus channel to unblock the reader goroutine
	closeOutStatus()

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

		// Send notification for sync failure (only for non-board tasks)
		if !strings.HasPrefix(task.TabId, "board-") {
			s.sendSyncNotification(task, false, err.Error())
		}
		return
	}

	// Success
	task.Status = "completed"
	endTime := time.Now()
	task.EndTime = &endTime

	s.emitSyncEvent(events.SyncCompleted, task.TabId, string(task.Action), "completed", "Sync operation completed successfully")

	// Send notification for sync success (only for non-board tasks)
	if !strings.HasPrefix(task.TabId, "board-") {
		s.sendSyncNotification(task, true, "")
	}
}

// emitSyncEvent emits a sync event to the frontend via unified EventBus
func (s *SyncService) emitSyncEvent(eventType events.EventType, tabId, action, status, message string) {
	fmt.Fprintf(os.Stderr, "[sync:%s:%s] %s: %s\n", action, tabId, status, message)
	event := events.NewSyncEvent(eventType, tabId, action, status, message)
	if s.eventBus != nil {
		if err := s.eventBus.EmitSyncEvent(event); err != nil {
			fmt.Fprintf(os.Stderr, "Failed to emit sync event: %v\n", err)
		}
	} else if s.app != nil {
		// Fallback to direct emission if EventBus not initialized
		s.app.Event.Emit("tofe", event)
	}
}

// sendSyncNotification sends a desktop notification for sync completion/failure
func (s *SyncService) sendSyncNotification(task *SyncTask, success bool, errorMsg string) {
	if s.notificationService == nil {
		return
	}

	// Build notification title and body
	actionLabel := "Sync"
	switch task.Action {
	case ActionPull:
		actionLabel = "Pull"
	case ActionPush:
		actionLabel = "Push"
	case ActionBi:
		actionLabel = "Bi-directional Sync"
	case ActionBiResync:
		actionLabel = "Bi-directional Resync"
	}

	profileName := task.Profile.Name
	if profileName == "" {
		profileName = "Unnamed profile"
	}

	var title, body string
	if success {
		title = fmt.Sprintf("%s Completed", actionLabel)
		body = fmt.Sprintf("Profile \"%s\" synced successfully.", profileName)
	} else {
		title = fmt.Sprintf("%s Failed", actionLabel)
		// Truncate error message if too long
		errText := errorMsg
		if len(errText) > 100 {
			errText = errText[:97] + "..."
		}
		body = fmt.Sprintf("Profile \"%s\" failed: %s", profileName, errText)
	}

	// Send notification (context.Background() since task context may be cancelled)
	if err := s.notificationService.SendNotification(context.Background(), title, body); err != nil {
		log.Printf("Failed to send sync notification: %v", err)
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
