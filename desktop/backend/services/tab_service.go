package services

import (
	"context"
	"desktop/backend/events"
	"desktop/backend/models"
	"fmt"
	"log"
	"sync"
	"time"

	"github.com/google/uuid"
	"github.com/wailsapp/wails/v3/pkg/application"
)

// TabState represents the state of a tab
type TabState string

const (
	TabStateIdle    TabState = "idle"
	TabStateRunning TabState = "running"
	TabStatePaused  TabState = "paused"
	TabStateError   TabState = "error"
)

// Tab represents a tab with its state and operations
type Tab struct {
	Id            string          `json:"id"`
	Name          string          `json:"name"`
	Profile       *models.Profile `json:"profile,omitempty"`
	State         TabState        `json:"state"`
	CurrentAction string          `json:"currentAction,omitempty"`
	TaskId        int             `json:"taskId,omitempty"`
	Output        []string        `json:"output"`
	CreatedAt     time.Time       `json:"createdAt"`
	UpdatedAt     time.Time       `json:"updatedAt"`
	LastError     string          `json:"lastError,omitempty"`
}

// TabService handles tab management and operations
type TabService struct {
	app   *application.App
	tabs  map[string]*Tab
	mutex sync.RWMutex
}

// NewTabService creates a new tab service
func NewTabService(app *application.App) *TabService {
	return &TabService{
		app:  app,
		tabs: make(map[string]*Tab),
	}
}

// SetApp sets the application reference for events
func (t *TabService) SetApp(app *application.App) {
	t.app = app
}

// ServiceName returns the name of the service
func (t *TabService) ServiceName() string {
	return "TabService"
}

// ServiceStartup is called when the service starts
func (t *TabService) ServiceStartup(ctx context.Context, options application.ServiceOptions) error {
	log.Printf("TabService starting up...")
	return nil
}

// ServiceShutdown is called when the service shuts down
func (t *TabService) ServiceShutdown(ctx context.Context) error {
	log.Printf("TabService shutting down...")

	// Clean up all tabs
	t.mutex.Lock()
	defer t.mutex.Unlock()

	for id := range t.tabs {
		delete(t.tabs, id)
	}

	return nil
}

// CreateTab creates a new tab
func (t *TabService) CreateTab(ctx context.Context, name string) (*Tab, error) {
	t.mutex.Lock()
	defer t.mutex.Unlock()

	// Validate input
	if name == "" {
		return nil, fmt.Errorf("tab name cannot be empty")
	}

	// Check for duplicate names
	for _, tab := range t.tabs {
		if tab.Name == name {
			return nil, fmt.Errorf("tab with name '%s' already exists", name)
		}
	}

	// Create new tab
	tab := &Tab{
		Id:        uuid.New().String(),
		Name:      name,
		State:     TabStateIdle,
		Output:    make([]string, 0),
		CreatedAt: time.Now(),
		UpdatedAt: time.Now(),
	}

	t.tabs[tab.Id] = tab

	// Emit tab created event
	t.emitTabEvent(events.TabCreated, tab.Id, tab.Name, tab)

	log.Printf("Tab '%s' (ID: %s) created successfully", name, tab.Id)
	return tab, nil
}

// GetTab returns a tab by ID
func (t *TabService) GetTab(ctx context.Context, tabId string) (*Tab, error) {
	t.mutex.RLock()
	defer t.mutex.RUnlock()

	tab, exists := t.tabs[tabId]
	if !exists {
		return nil, fmt.Errorf("tab with ID '%s' not found", tabId)
	}

	// Return a copy to prevent external modifications
	tabCopy := *tab
	tabCopy.Output = make([]string, len(tab.Output))
	copy(tabCopy.Output, tab.Output)

	return &tabCopy, nil
}

// GetAllTabs returns all tabs
func (t *TabService) GetAllTabs(ctx context.Context) (map[string]*Tab, error) {
	t.mutex.RLock()
	defer t.mutex.RUnlock()

	// Return copies to prevent external modifications
	tabs := make(map[string]*Tab)
	for id, tab := range t.tabs {
		tabCopy := *tab
		tabCopy.Output = make([]string, len(tab.Output))
		copy(tabCopy.Output, tab.Output)
		tabs[id] = &tabCopy
	}

	return tabs, nil
}

// UpdateTab updates a tab's properties
func (t *TabService) UpdateTab(ctx context.Context, tabId string, updates map[string]interface{}) error {
	t.mutex.Lock()
	defer t.mutex.Unlock()

	tab, exists := t.tabs[tabId]
	if !exists {
		return fmt.Errorf("tab with ID '%s' not found", tabId)
	}

	// Store old values for rollback
	oldName := tab.Name
	oldState := tab.State
	oldProfile := tab.Profile
	oldCurrentAction := tab.CurrentAction
	oldTaskId := tab.TaskId
	oldLastError := tab.LastError

	// Apply updates
	if name, ok := updates["name"].(string); ok && name != "" {
		// Check for duplicate names
		for id, existingTab := range t.tabs {
			if id != tabId && existingTab.Name == name {
				return fmt.Errorf("tab with name '%s' already exists", name)
			}
		}
		tab.Name = name
	}

	if state, ok := updates["state"].(TabState); ok {
		tab.State = state
	}

	if profile, ok := updates["profile"].(*models.Profile); ok {
		tab.Profile = profile
	}

	if action, ok := updates["currentAction"].(string); ok {
		tab.CurrentAction = action
	}

	if taskId, ok := updates["taskId"].(int); ok {
		tab.TaskId = taskId
	}

	if output, ok := updates["output"].([]string); ok {
		tab.Output = output
	}

	if lastError, ok := updates["lastError"].(string); ok {
		tab.LastError = lastError
	}

	tab.UpdatedAt = time.Now()

	// Emit tab updated event
	t.emitTabEvent(events.TabUpdated, tab.Id, tab.Name, tab)

	log.Printf("Tab '%s' (ID: %s) updated successfully", tab.Name, tab.Id)

	// Note: In a real implementation, you might want to add rollback logic
	// if the event emission fails or other post-update operations fail
	_ = oldName
	_ = oldState
	_ = oldProfile
	_ = oldCurrentAction
	_ = oldTaskId
	_ = oldLastError

	return nil
}

// DeleteTab deletes a tab
func (t *TabService) DeleteTab(ctx context.Context, tabId string) error {
	t.mutex.Lock()
	defer t.mutex.Unlock()

	tab, exists := t.tabs[tabId]
	if !exists {
		return fmt.Errorf("tab with ID '%s' not found", tabId)
	}

	// Store tab info for event
	tabName := tab.Name
	tabCopy := *tab

	// Delete the tab
	delete(t.tabs, tabId)

	// Emit tab deleted event
	t.emitTabEvent(events.TabDeleted, tabId, tabName, &tabCopy)

	log.Printf("Tab '%s' (ID: %s) deleted successfully", tabName, tabId)
	return nil
}

// RenameTab renames a tab
func (t *TabService) RenameTab(ctx context.Context, tabId, newName string) error {
	if newName == "" {
		return fmt.Errorf("tab name cannot be empty")
	}

	updates := map[string]interface{}{
		"name": newName,
	}

	return t.UpdateTab(ctx, tabId, updates)
}

// SetTabProfile sets the profile for a tab
func (t *TabService) SetTabProfile(ctx context.Context, tabId string, profile *models.Profile) error {
	updates := map[string]interface{}{
		"profile": profile,
	}

	return t.UpdateTab(ctx, tabId, updates)
}

// AddTabOutput adds output to a tab
func (t *TabService) AddTabOutput(ctx context.Context, tabId string, output string) error {
	t.mutex.Lock()
	defer t.mutex.Unlock()

	tab, exists := t.tabs[tabId]
	if !exists {
		return fmt.Errorf("tab with ID '%s' not found", tabId)
	}

	// Add output
	tab.Output = append(tab.Output, output)
	tab.UpdatedAt = time.Now()

	// Emit tab output event
	outputData := map[string]interface{}{
		"output": output,
		"total":  len(tab.Output),
	}
	t.emitTabEvent(events.TabOutput, tab.Id, tab.Name, outputData)

	return nil
}

// ClearTabOutput clears all output from a tab
func (t *TabService) ClearTabOutput(ctx context.Context, tabId string) error {
	t.mutex.Lock()
	defer t.mutex.Unlock()

	tab, exists := t.tabs[tabId]
	if !exists {
		return fmt.Errorf("tab with ID '%s' not found", tabId)
	}

	// Clear output
	tab.Output = make([]string, 0)
	tab.UpdatedAt = time.Now()

	// Emit tab updated event
	t.emitTabEvent(events.TabUpdated, tab.Id, tab.Name, tab)

	log.Printf("Output cleared for tab '%s' (ID: %s)", tab.Name, tab.Id)
	return nil
}

// SetTabState sets the state of a tab
func (t *TabService) SetTabState(ctx context.Context, tabId string, state TabState) error {
	updates := map[string]interface{}{
		"state": state,
	}

	return t.UpdateTab(ctx, tabId, updates)
}

// SetTabError sets an error for a tab
func (t *TabService) SetTabError(ctx context.Context, tabId string, errorMsg string) error {
	updates := map[string]interface{}{
		"state":     TabStateError,
		"lastError": errorMsg,
	}

	return t.UpdateTab(ctx, tabId, updates)
}

// emitTabEvent emits a tab event
func (t *TabService) emitTabEvent(eventType events.EventType, tabId, tabName string, data interface{}) {
	event := events.NewTabEvent(eventType, tabId, tabName, data)
	t.app.Event.Emit(string(eventType), event)
}
