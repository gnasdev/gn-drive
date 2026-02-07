package services

import (
	"context"
	"desktop/backend/models"
	"testing"
	"time"

	"github.com/robfig/cron/v3"
)

func newTestSchedulerService(t *testing.T) *SchedulerService {
	t.Helper()
	// Clean DB table for test isolation
	db, _ := GetSharedDB()
	db.Exec("DELETE FROM schedules")
	return &SchedulerService{
		schedules:   []models.ScheduleEntry{},
		cronEntries: make(map[string]cron.EntryID),
		cron:        cron.New(),
		initialized: true,
	}
}

func TestSchedulerService_AddSchedule(t *testing.T) {
	s := newTestSchedulerService(t)
	s.cron.Start()
	defer s.cron.Stop()
	ctx := context.Background()

	entry := models.ScheduleEntry{
		Id:          "sched-1",
		ProfileName: "test-profile",
		Action:      "push",
		CronExpr:    "0 */6 * * *",
		Enabled:     true,
		CreatedAt:   time.Now(),
	}

	err := s.AddSchedule(ctx, entry)
	if err != nil {
		t.Fatalf("AddSchedule failed: %v", err)
	}

	schedules, err := s.GetSchedules(ctx)
	if err != nil {
		t.Fatalf("GetSchedules failed: %v", err)
	}
	if len(schedules) != 1 {
		t.Fatalf("expected 1 schedule, got %d", len(schedules))
	}
	if schedules[0].Id != "sched-1" {
		t.Errorf("expected schedule id 'sched-1', got %q", schedules[0].Id)
	}
}

func TestSchedulerService_AddSchedule_InvalidCron(t *testing.T) {
	s := newTestSchedulerService(t)
	ctx := context.Background()

	entry := models.ScheduleEntry{
		Id:          "sched-bad",
		ProfileName: "test",
		Action:      "push",
		CronExpr:    "not-a-cron",
		Enabled:     true,
	}

	err := s.AddSchedule(ctx, entry)
	if err == nil {
		t.Error("expected error for invalid cron expression")
	}
}

func TestSchedulerService_DeleteSchedule(t *testing.T) {
	s := newTestSchedulerService(t)
	s.cron.Start()
	defer s.cron.Stop()
	ctx := context.Background()

	entry := models.ScheduleEntry{
		Id:          "sched-del",
		ProfileName: "test",
		Action:      "push",
		CronExpr:    "0 0 * * *",
		Enabled:     true,
		CreatedAt:   time.Now(),
	}

	if err := s.AddSchedule(ctx, entry); err != nil {
		t.Fatalf("AddSchedule failed: %v", err)
	}

	if err := s.DeleteSchedule(ctx, "sched-del"); err != nil {
		t.Fatalf("DeleteSchedule failed: %v", err)
	}

	schedules, _ := s.GetSchedules(ctx)
	if len(schedules) != 0 {
		t.Errorf("expected 0 schedules after delete, got %d", len(schedules))
	}
}

func TestSchedulerService_DeleteSchedule_NotFound(t *testing.T) {
	s := newTestSchedulerService(t)
	ctx := context.Background()

	err := s.DeleteSchedule(ctx, "nonexistent")
	if err == nil {
		t.Error("expected error for nonexistent schedule")
	}
}

func TestSchedulerService_EnableDisable(t *testing.T) {
	s := newTestSchedulerService(t)
	s.cron.Start()
	defer s.cron.Stop()
	ctx := context.Background()

	entry := models.ScheduleEntry{
		Id:          "sched-toggle",
		ProfileName: "test",
		Action:      "push",
		CronExpr:    "0 0 * * *",
		Enabled:     false,
		CreatedAt:   time.Now(),
	}

	if err := s.AddSchedule(ctx, entry); err != nil {
		t.Fatalf("AddSchedule failed: %v", err)
	}

	// Enable
	if err := s.EnableSchedule(ctx, "sched-toggle"); err != nil {
		t.Fatalf("EnableSchedule failed: %v", err)
	}
	schedules, _ := s.GetSchedules(ctx)
	if !schedules[0].Enabled {
		t.Error("expected schedule to be enabled")
	}

	// Disable
	if err := s.DisableSchedule(ctx, "sched-toggle"); err != nil {
		t.Fatalf("DisableSchedule failed: %v", err)
	}
	schedules, _ = s.GetSchedules(ctx)
	if schedules[0].Enabled {
		t.Error("expected schedule to be disabled")
	}
}

func TestSchedulerService_UpdateSchedule(t *testing.T) {
	s := newTestSchedulerService(t)
	s.cron.Start()
	defer s.cron.Stop()
	ctx := context.Background()

	entry := models.ScheduleEntry{
		Id:          "sched-update",
		ProfileName: "test",
		Action:      "push",
		CronExpr:    "0 0 * * *",
		Enabled:     true,
		CreatedAt:   time.Now(),
	}

	if err := s.AddSchedule(ctx, entry); err != nil {
		t.Fatalf("AddSchedule failed: %v", err)
	}

	// Update with new cron expression
	updated := entry
	updated.CronExpr = "0 */12 * * *"
	updated.Action = "bi"

	if err := s.UpdateSchedule(ctx, updated); err != nil {
		t.Fatalf("UpdateSchedule failed: %v", err)
	}

	schedules, _ := s.GetSchedules(ctx)
	if schedules[0].CronExpr != "0 */12 * * *" {
		t.Errorf("expected updated cron '0 */12 * * *', got %q", schedules[0].CronExpr)
	}
	if schedules[0].Action != "bi" {
		t.Errorf("expected updated action 'bi', got %q", schedules[0].Action)
	}
}

func TestSchedulerService_Persistence(t *testing.T) {
	ctx := context.Background()

	// Create first service instance and add a schedule
	s1 := &SchedulerService{
		schedules:   []models.ScheduleEntry{},
		cronEntries: make(map[string]cron.EntryID),
		cron:        cron.New(),
		initialized: true,
	}
	s1.cron.Start()

	entry := models.ScheduleEntry{
		Id:          "persist-sched",
		ProfileName: "test",
		Action:      "push",
		CronExpr:    "0 0 * * *",
		Enabled:     true,
		CreatedAt:   time.Now(),
	}
	if err := s1.AddSchedule(ctx, entry); err != nil {
		t.Fatalf("AddSchedule failed: %v", err)
	}
	s1.cron.Stop()

	// Create second service instance and load from DB
	s2 := &SchedulerService{
		schedules:   []models.ScheduleEntry{},
		cronEntries: make(map[string]cron.EntryID),
		cron:        cron.New(),
	}
	if err := s2.initialize(); err != nil {
		t.Fatalf("initialize failed: %v", err)
	}

	found := false
	for _, s := range s2.schedules {
		if s.Id == "persist-sched" {
			found = true
			break
		}
	}
	if !found {
		t.Error("expected to find 'persist-sched' after loading from DB")
	}
}
