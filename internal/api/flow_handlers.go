// Package api provides the HTTP API server for the web UI.
package api

import (
	"context"
	"errors"
	"net/http"

	"github.com/go-chi/chi/v5"

	"github.com/gnasdev/gn-drive/internal/flowengine"
	"github.com/gnasdev/gn-drive/internal/store"
)

// handleListFlows returns all flows with nested operations (Wails GetFlows).
func (s *Server) handleListFlows(w http.ResponseWriter, r *http.Request) {
	if s.app.Store == nil {
		respondError(w, http.StatusServiceUnavailable, "locked", "data plane not ready")
		return
	}
	ctx := r.Context()
	flows, err := s.app.Store.Flows().List(ctx)
	if err != nil {
		respondError(w, http.StatusInternalServerError, "db_error", err.Error())
		return
	}
	// Attach runtime status when flow engine is present.
	if s.app.FlowEngine != nil {
		out := make([]map[string]any, 0, len(flows))
		for _, f := range flows {
			out = append(out, flowWithStatus(f, s.app.FlowEngine.Status(f.ID)))
		}
		respondOK(w, out)
		return
	}
	respondOK(w, flows)
}

func flowWithStatus(f store.Flow, status string) map[string]any {
	return map[string]any{
		"id":               f.ID,
		"name":             f.Name,
		"is_collapsed":     f.IsCollapsed,
		"schedule_enabled": f.ScheduleEnabled,
		"enabled":          f.Enabled,
		"schedule_cron":    f.ScheduleCron,
		"cron_expr":        f.CronExpr,
		"sort_order":       f.SortOrder,
		"operations":       f.Operations,
		"created_at":       f.CreatedAt,
		"updated_at":       f.UpdatedAt,
		"status":           status,
	}
}

// handleGetFlow returns one flow with operations.
func (s *Server) handleGetFlow(w http.ResponseWriter, r *http.Request) {
	if s.app.Store == nil {
		respondError(w, http.StatusServiceUnavailable, "locked", "data plane not ready")
		return
	}
	id := chi.URLParam(r, "id")
	f, err := s.app.Store.Flows().Get(r.Context(), id)
	if err != nil {
		if errors.Is(err, store.ErrNotFound) {
			respondError(w, http.StatusNotFound, "not_found", "flow not found")
			return
		}
		respondError(w, http.StatusInternalServerError, "db_error", err.Error())
		return
	}
	status := "idle"
	if s.app.FlowEngine != nil {
		status = s.app.FlowEngine.Status(f.ID)
	}
	respondOK(w, flowWithStatus(*f, status))
}

// handleCreateFlow creates a new flow (optionally with operations).
func (s *Server) handleCreateFlow(w http.ResponseWriter, r *http.Request) {
	if s.app.Store == nil {
		respondError(w, http.StatusServiceUnavailable, "locked", "data plane not ready")
		return
	}
	ctx := r.Context()
	var f store.Flow
	if err := parseJSON(r, &f); err != nil {
		respondError(w, http.StatusBadRequest, "bad_request", err.Error())
		return
	}
	if f.ID == "" {
		respondError(w, http.StatusBadRequest, "missing_id", "flow id is required")
		return
	}
	if err := s.app.Store.Flows().Save(ctx, &f); err != nil {
		respondError(w, http.StatusInternalServerError, "save_error", err.Error())
		return
	}
	got, _ := s.app.Store.Flows().Get(ctx, f.ID)
	if got != nil {
		respondCreated(w, got)
		return
	}
	respondCreated(w, f)
}

// handleUpdateFlow updates a flow and replaces its operations.
func (s *Server) handleUpdateFlow(w http.ResponseWriter, r *http.Request) {
	if s.app.Store == nil {
		respondError(w, http.StatusServiceUnavailable, "locked", "data plane not ready")
		return
	}
	ctx := r.Context()
	id := chi.URLParam(r, "id")
	var f store.Flow
	if err := parseJSON(r, &f); err != nil {
		respondError(w, http.StatusBadRequest, "bad_request", err.Error())
		return
	}
	if f.ID == "" {
		f.ID = id
	}
	if s.app.FlowEngine != nil && s.app.FlowEngine.IsRunning(f.ID) {
		respondError(w, http.StatusConflict, "busy", "cannot update a running flow")
		return
	}
	if err := s.app.Store.Flows().Save(ctx, &f); err != nil {
		respondError(w, http.StatusInternalServerError, "save_error", err.Error())
		return
	}
	got, err := s.app.Store.Flows().Get(ctx, f.ID)
	if err != nil {
		respondOK(w, f)
		return
	}
	respondOK(w, got)
}

// handleDeleteFlow deletes a flow and its operations.
func (s *Server) handleDeleteFlow(w http.ResponseWriter, r *http.Request) {
	if s.app.Store == nil {
		respondError(w, http.StatusServiceUnavailable, "locked", "data plane not ready")
		return
	}
	ctx := r.Context()
	id := chi.URLParam(r, "id")
	if s.app.FlowEngine != nil && s.app.FlowEngine.IsRunning(id) {
		respondError(w, http.StatusConflict, "busy", "cannot delete a running flow")
		return
	}
	if err := s.app.Store.Flows().Delete(ctx, id); err != nil {
		if errors.Is(err, store.ErrNotFound) {
			respondError(w, http.StatusNotFound, "not_found", "flow not found")
			return
		}
		respondError(w, http.StatusInternalServerError, "delete_error", err.Error())
		return
	}
	respondOK(w, map[string]bool{"ok": true})
}

// handleExecuteFlow starts sequential execution of a flow's operations.
func (s *Server) handleExecuteFlow(w http.ResponseWriter, r *http.Request) {
	if s.app.FlowEngine == nil {
		respondError(w, http.StatusServiceUnavailable, "unavailable", "flow engine not ready")
		return
	}
	id := chi.URLParam(r, "id")
	// Detach from the HTTP request context: once we respond 200 the request
	// context is cancelled, which would abort the background run immediately.
	if err := s.app.FlowEngine.Execute(context.Background(), id); err != nil {
		switch {
		case errors.Is(err, flowengine.ErrAlreadyRunning):
			respondError(w, http.StatusConflict, "busy", err.Error())
		case errors.Is(err, flowengine.ErrEmptyFlow):
			respondError(w, http.StatusBadRequest, "empty", err.Error())
		case errors.Is(err, store.ErrNotFound):
			respondError(w, http.StatusNotFound, "not_found", "flow not found")
		default:
			respondError(w, http.StatusInternalServerError, "execute_error", err.Error())
		}
		return
	}
	respondOK(w, map[string]string{"status": "running", "flow_id": id})
}

// handleStopFlow cancels an in-flight flow.
func (s *Server) handleStopFlow(w http.ResponseWriter, r *http.Request) {
	if s.app.FlowEngine == nil {
		respondError(w, http.StatusServiceUnavailable, "unavailable", "flow engine not ready")
		return
	}
	id := chi.URLParam(r, "id")
	if err := s.app.FlowEngine.Stop(id); err != nil {
		if errors.Is(err, flowengine.ErrNotRunning) {
			respondError(w, http.StatusNotFound, "not_running", err.Error())
			return
		}
		respondError(w, http.StatusInternalServerError, "stop_error", err.Error())
		return
	}
	respondOK(w, map[string]bool{"ok": true})
}
