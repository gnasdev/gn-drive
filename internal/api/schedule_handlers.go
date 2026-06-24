// Package api provides the HTTP API server for the web UI.
package api

import (
	"context"
	"net/http"

	"github.com/go-chi/chi/v5"
	"github.com/google/uuid"
	"github.com/gnasdev/gn-drive/internal/store"
)

// handleListSchedules returns all schedules.
func (s *Server) handleListSchedules(w http.ResponseWriter, r *http.Request) {
	ctx := r.Context()
	schedules, err := s.app.Store.Schedules().List(ctx)
	if err != nil {
		respondError(w, http.StatusInternalServerError, "db_error", err.Error())
		return
	}
	respondOK(w, schedules)
}

// handleCreateSchedule creates a new schedule.
func (s *Server) handleCreateSchedule(w http.ResponseWriter, r *http.Request) {
	ctx := r.Context()
	var s2 store.Schedule
	if err := parseJSON(r, &s2); err != nil {
		respondError(w, http.StatusBadRequest, "bad_request", err.Error())
		return
	}
	if s2.ID == "" {
		s2.ID = uuid.New().String()
	}
	if err := s.app.Store.Schedules().Save(ctx, &s2); err != nil {
		respondError(w, http.StatusInternalServerError, "save_error", err.Error())
		return
	}
	s.app.SyncEngine.RegisterSchedule(ctx, &s2)
	respondCreated(w, s2)
}

// handleUpdateSchedule updates a schedule.
func (s *Server) handleUpdateSchedule(w http.ResponseWriter, r *http.Request) {
	ctx := r.Context()
	var s2 store.Schedule
	if err := parseJSON(r, &s2); err != nil {
		respondError(w, http.StatusBadRequest, "bad_request", err.Error())
		return
	}
	if err := s.app.Store.Schedules().Save(ctx, &s2); err != nil {
		respondError(w, http.StatusInternalServerError, "save_error", err.Error())
		return
	}
	s.app.SyncEngine.RegisterSchedule(ctx, &s2)
	respondOK(w, s2)
}

// handleDeleteSchedule deletes a schedule.
func (s *Server) handleDeleteSchedule(w http.ResponseWriter, r *http.Request) {
	ctx := r.Context()
	id := chi.URLParam(r, "id")
	s.app.SyncEngine.UnregisterSchedule(id)
	if err := s.app.Store.Schedules().Delete(ctx, id); err != nil {
		if err == store.ErrNotFound {
			respondError(w, http.StatusNotFound, "not_found", "schedule not found")
			return
		}
		respondError(w, http.StatusInternalServerError, "delete_error", err.Error())
		return
	}
	respondOK(w, map[string]bool{"ok": true})
}

func (s *Server) handleEnableSchedule(w http.ResponseWriter, r *http.Request) {
	ctx := r.Context()
	id := chi.URLParam(r, "id")
	s2, err := schedulesGetFn(ctx, s.app.Store.Schedules(), id)
	if err != nil {
		respondError(w, http.StatusNotFound, "not_found", err.Error())
		return
	}
	s2.Enabled = true
	if err := schedulesSaveFn(ctx, s.app.Store.Schedules(), s2); err != nil {
		respondError(w, http.StatusInternalServerError, "save_error", err.Error())
		return
	}
	s.app.SyncEngine.RegisterSchedule(ctx, s2)
	respondOK(w, s2)
}

func (s *Server) handleDisableSchedule(w http.ResponseWriter, r *http.Request) {
	ctx := r.Context()
	id := chi.URLParam(r, "id")
	s.app.SyncEngine.UnregisterSchedule(id)
	s2, err := schedulesGetFn(ctx, s.app.Store.Schedules(), id)
	if err != nil {
		respondError(w, http.StatusNotFound, "not_found", err.Error())
		return
	}
	s2.Enabled = false
	if err := schedulesSaveFn(ctx, s.app.Store.Schedules(), s2); err != nil {
		respondError(w, http.StatusInternalServerError, "save_error", err.Error())
		return
	}
	respondOK(w, s2)
}

// schedulesGetFn and schedulesSaveFn are overridable for tests.
var (
	schedulesGetFn = func(ctx context.Context, r store.ScheduleRepo, id string) (*store.Schedule, error) {
		return r.Get(ctx, id)
	}
	schedulesSaveFn = func(ctx context.Context, r store.ScheduleRepo, s *store.Schedule) error {
		return r.Save(ctx, s)
	}
)
