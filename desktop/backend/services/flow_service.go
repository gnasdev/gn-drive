package services

import (
	"context"
	"desktop/backend/models"
	"encoding/json"
	"fmt"
	"log"
	"sync"
	"time"

	"github.com/wailsapp/wails/v3/pkg/application"
)

// FlowService manages flow persistence using the shared SQLite database
type FlowService struct {
	app         *application.App
	mutex       sync.RWMutex
	initialized bool
}

// Singleton instance for cross-service access
var flowServiceInstance *FlowService
var flowServiceOnce sync.Once

// GetFlowService returns the singleton FlowService instance
func GetFlowService() *FlowService {
	return flowServiceInstance
}

// SetFlowServiceInstance sets the singleton instance (called from main.go)
func SetFlowServiceInstance(fs *FlowService) {
	flowServiceOnce.Do(func() {
		flowServiceInstance = fs
	})
}

// NewFlowService creates a new flow service
func NewFlowService(app *application.App) *FlowService {
	return &FlowService{
		app: app,
	}
}

// SetApp sets the application reference
func (s *FlowService) SetApp(app *application.App) {
	s.app = app
}

// ServiceName returns the name of the service
func (s *FlowService) ServiceName() string {
	return "FlowService"
}

// ServiceStartup is called when the service starts
func (s *FlowService) ServiceStartup(ctx context.Context, options application.ServiceOptions) error {
	log.Printf("FlowService starting up (async)...")
	go func() {
		if err := s.initialize(); err != nil {
			log.Printf("FlowService init error: %v", err)
		}
	}()
	return nil
}

// ServiceShutdown is called when the service shuts down
func (s *FlowService) ServiceShutdown(ctx context.Context) error {
	log.Printf("FlowService shutting down...")
	return nil
}

// ensureInitialized lazily initializes the service if not yet done
func (s *FlowService) ensureInitialized() error {
	s.mutex.RLock()
	if s.initialized {
		s.mutex.RUnlock()
		return nil
	}
	s.mutex.RUnlock()
	return s.initialize()
}

// initialize sets up the service
func (s *FlowService) initialize() error {
	s.mutex.Lock()
	defer s.mutex.Unlock()

	if s.initialized {
		return nil
	}

	s.initialized = true
	log.Printf("FlowService initialized")
	return nil
}

// ============ Public API ============

// GetFlows returns all flows with their operations, ordered by sort_order
func (s *FlowService) GetFlows(ctx context.Context) ([]models.Flow, error) {
	if err := s.ensureInitialized(); err != nil {
		return nil, err
	}
	s.mutex.RLock()
	defer s.mutex.RUnlock()

	db, err := GetSharedDB()
	if err != nil {
		return nil, err
	}

	// Query flows
	rows, err := db.Query(`
		SELECT id, name, is_collapsed, schedule_enabled, cron_expr, sort_order, created_at, updated_at
		FROM flows ORDER BY sort_order
	`)
	if err != nil {
		return nil, fmt.Errorf("failed to query flows: %w", err)
	}
	defer rows.Close()

	var flows []models.Flow
	for rows.Next() {
		var f models.Flow
		var isCollapsed, scheduleEnabled int
		if err := rows.Scan(&f.Id, &f.Name, &isCollapsed, &scheduleEnabled, &f.CronExpr, &f.SortOrder, &f.CreatedAt, &f.UpdatedAt); err != nil {
			return nil, fmt.Errorf("failed to scan flow: %w", err)
		}
		f.IsCollapsed = isCollapsed != 0
		f.ScheduleEnabled = scheduleEnabled != 0
		f.Operations = []models.Operation{}
		flows = append(flows, f)
	}
	if err := rows.Err(); err != nil {
		return nil, fmt.Errorf("error iterating flows: %w", err)
	}

	// Query operations for each flow
	for i := range flows {
		ops, err := s.getOperationsForFlow(flows[i].Id)
		if err != nil {
			return nil, err
		}
		flows[i].Operations = ops
	}

	if flows == nil {
		flows = []models.Flow{}
	}
	return flows, nil
}

// SaveFlows replaces all flows and operations atomically
func (s *FlowService) SaveFlows(ctx context.Context, flows []models.Flow) error {
	if err := s.ensureInitialized(); err != nil {
		return err
	}
	s.mutex.Lock()
	defer s.mutex.Unlock()

	db, err := GetSharedDB()
	if err != nil {
		return err
	}

	tx, err := db.Begin()
	if err != nil {
		return fmt.Errorf("failed to begin transaction: %w", err)
	}
	defer tx.Rollback()

	// Clear existing data
	if _, err := tx.Exec("DELETE FROM operations"); err != nil {
		return fmt.Errorf("failed to clear operations: %w", err)
	}
	if _, err := tx.Exec("DELETE FROM flows"); err != nil {
		return fmt.Errorf("failed to clear flows: %w", err)
	}

	now := time.Now().UTC().Format("2006-01-02 15:04:05")

	// Insert flows
	flowStmt, err := tx.Prepare(`
		INSERT INTO flows (id, name, is_collapsed, schedule_enabled, cron_expr, sort_order, created_at, updated_at)
		VALUES (?, ?, ?, ?, ?, ?, ?, ?)
	`)
	if err != nil {
		return fmt.Errorf("failed to prepare flow insert: %w", err)
	}
	defer flowStmt.Close()

	// Insert operations
	opStmt, err := tx.Prepare(`
		INSERT INTO operations (id, flow_id, source_remote, source_path, target_remote, target_path, action, sync_config, is_expanded, sort_order)
		VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
	`)
	if err != nil {
		return fmt.Errorf("failed to prepare operation insert: %w", err)
	}
	defer opStmt.Close()

	for i, f := range flows {
		isCollapsed := 0
		if f.IsCollapsed {
			isCollapsed = 1
		}
		scheduleEnabled := 0
		if f.ScheduleEnabled {
			scheduleEnabled = 1
		}
		createdAt := f.CreatedAt
		if createdAt == "" {
			createdAt = now
		}

		if _, err := flowStmt.Exec(f.Id, f.Name, isCollapsed, scheduleEnabled, f.CronExpr, i, createdAt, now); err != nil {
			return fmt.Errorf("failed to insert flow %s: %w", f.Id, err)
		}

		for j, op := range f.Operations {
			// Strip encryption passwords before persisting
			op.SyncConfig.StripEncryptPasswords()
			syncConfigJSON, err := json.Marshal(op.SyncConfig)
			if err != nil {
				return fmt.Errorf("failed to marshal sync_config for operation %s: %w", op.Id, err)
			}
			isExpanded := 0
			if op.IsExpanded {
				isExpanded = 1
			}

			if _, err := opStmt.Exec(op.Id, f.Id, op.SourceRemote, op.SourcePath, op.TargetRemote, op.TargetPath, op.Action, string(syncConfigJSON), isExpanded, j); err != nil {
				return fmt.Errorf("failed to insert operation %s: %w", op.Id, err)
			}
		}
	}

	if err := tx.Commit(); err != nil {
		return err
	}

	// Refresh tray menu to reflect flow changes
	if ts := GetTrayService(); ts != nil {
		go ts.RefreshMenu()
	}

	return nil
}

// OnRemoteDeleted cleans up operations referencing a deleted remote
func (s *FlowService) OnRemoteDeleted(ctx context.Context, remoteName string) error {
	if err := s.ensureInitialized(); err != nil {
		return err
	}
	s.mutex.Lock()
	defer s.mutex.Unlock()

	db, err := GetSharedDB()
	if err != nil {
		return err
	}

	// Clear source/target remote references (set to empty string)
	if _, err := db.Exec(
		"UPDATE operations SET source_remote = '' WHERE source_remote = ?", remoteName,
	); err != nil {
		return fmt.Errorf("failed to clear source remote: %w", err)
	}
	if _, err := db.Exec(
		"UPDATE operations SET target_remote = '' WHERE target_remote = ?", remoteName,
	); err != nil {
		return fmt.Errorf("failed to clear target remote: %w", err)
	}

	log.Printf("FlowService: cleared references to deleted remote '%s'", remoteName)
	return nil
}

// ============ Private Helpers ============

func (s *FlowService) getOperationsForFlow(flowId string) ([]models.Operation, error) {
	db, err := GetSharedDB()
	if err != nil {
		return nil, err
	}

	rows, err := db.Query(`
		SELECT id, flow_id, source_remote, source_path, target_remote, target_path, action,
		       sync_config, is_expanded, sort_order
		FROM operations WHERE flow_id = ? ORDER BY sort_order
	`, flowId)
	if err != nil {
		return nil, fmt.Errorf("failed to query operations: %w", err)
	}
	defer rows.Close()

	var ops []models.Operation
	for rows.Next() {
		var op models.Operation
		var syncConfigJSON string
		var isExpanded int
		if err := rows.Scan(&op.Id, &op.FlowId, &op.SourceRemote, &op.SourcePath, &op.TargetRemote, &op.TargetPath, &op.Action,
			&syncConfigJSON, &isExpanded, &op.SortOrder); err != nil {
			return nil, fmt.Errorf("failed to scan operation: %w", err)
		}
		if syncConfigJSON != "" && syncConfigJSON != "{}" {
			if err := json.Unmarshal([]byte(syncConfigJSON), &op.SyncConfig); err != nil {
				log.Printf("warning: failed to unmarshal sync_config for operation %s: %v", op.Id, err)
			}
		}
		op.IsExpanded = isExpanded != 0
		ops = append(ops, op)
	}
	if err := rows.Err(); err != nil {
		return nil, fmt.Errorf("error iterating operations: %w", err)
	}

	if ops == nil {
		ops = []models.Operation{}
	}
	return ops, nil
}

// ============ JSON Helpers ============

func marshalStringSlice(slice []string) string {
	if len(slice) == 0 {
		return "[]"
	}
	data, err := json.Marshal(slice)
	if err != nil {
		return "[]"
	}
	return string(data)
}

func unmarshalStringSlice(data string) []string {
	if data == "" || data == "[]" {
		return nil
	}
	var result []string
	if err := json.Unmarshal([]byte(data), &result); err != nil {
		return nil
	}
	return result
}
