// Package api provides the HTTP API server for the web UI.
package api

import (
	"net/http"

	"github.com/go-chi/chi/v5"
)

// handleStartSync starts a one-shot sync task.
func (s *Server) handleStartSync(w http.ResponseWriter, r *http.Request) {
	ctx := r.Context()
	var req struct {
		ProfileName string `json:"profile_name"`
		Action      string `json:"action"`
	}
	if err := parseJSON(r, &req); err != nil {
		respondError(w, http.StatusBadRequest, "bad_request", err.Error())
		return
	}
	if req.ProfileName == "" {
		respondError(w, http.StatusBadRequest, "missing_profile", "profile_name is required")
		return
	}
	taskID, err := s.app.SyncEngine.StartSync(ctx, req.Action, req.ProfileName)
	if err != nil {
		respondError(w, http.StatusInternalServerError, "sync_error", err.Error())
		return
	}
	respondCreated(w, map[string]string{"task_id": taskID})
}

// handleListTasks returns all active tasks.
func (s *Server) handleListTasks(w http.ResponseWriter, r *http.Request) {
	ctx := r.Context()
	tasks, err := s.app.SyncEngine.ActiveTasks(ctx)
	if err != nil {
		respondError(w, http.StatusInternalServerError, "internal", err.Error())
		return
	}
	respondOK(w, tasks)
}

// handleStopTask cancels an active sync task.
func (s *Server) handleStopTask(w http.ResponseWriter, r *http.Request) {
	ctx := r.Context()
	taskID := chi.URLParam(r, "id")
	if err := s.app.SyncEngine.StopSync(ctx, taskID); err != nil {
		respondError(w, http.StatusInternalServerError, "stop_error", err.Error())
		return
	}
	respondOK(w, map[string]bool{"ok": true})
}

// handleTaskLogs returns log lines since a sequence number.
// Not implemented in Phase 3 — returns empty.
func (s *Server) handleTaskLogs(w http.ResponseWriter, r *http.Request) {
	respondOK(w, []any{})
}
