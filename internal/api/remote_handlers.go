// Package api provides the HTTP API server for the web UI.
package api

import (
	"net/http"

	"github.com/go-chi/chi/v5"

	"github.com/gnasdev/gn-drive/internal/eventbus"
)

// handleListRemotes returns all rclone remotes.
func (s *Server) handleListRemotes(w http.ResponseWriter, r *http.Request) {
	ctx := r.Context()
	remotes, err := s.app.Rclone.ListRemotes(ctx)
	if err != nil {
		respondError(w, http.StatusInternalServerError, "rclone_error", err.Error())
		return
	}
	respondOK(w, remotes)
}

// handleCreateRemote creates a new rclone remote.
func (s *Server) handleCreateRemote(w http.ResponseWriter, r *http.Request) {
	ctx := r.Context()
	var req struct {
		Name    string   `json:"name"`
		Type    string   `json:"type"`
		Config  []string `json:"config"` // ["key=value", ...]
	}
	if err := parseJSON(r, &req); err != nil {
		respondError(w, http.StatusBadRequest, "bad_request", err.Error())
		return
	}
	if req.Name == "" || req.Type == "" {
		respondError(w, http.StatusBadRequest, "missing_fields", "name and type are required")
		return
	}
	if err := s.app.Rclone.CreateRemote(ctx, req.Name, req.Type, req.Config); err != nil {
		respondError(w, http.StatusInternalServerError, "create_error", err.Error())
		return
	}
	s.app.Bus.Publish(eventbus.TopicStateChanged, eventbus.StateChangedEvent{})
	respondCreated(w, map[string]any{"name": req.Name, "type": req.Type})
}

// handleDeleteRemote deletes a remote.
func (s *Server) handleDeleteRemote(w http.ResponseWriter, r *http.Request) {
	ctx := r.Context()
	name := chi.URLParam(r, "name")
	if err := s.app.Rclone.DeleteRemote(ctx, name); err != nil {
		respondError(w, http.StatusInternalServerError, "delete_error", err.Error())
		return
	}
	s.app.Bus.Publish(eventbus.TopicStateChanged, eventbus.StateChangedEvent{})
	respondOK(w, map[string]bool{"ok": true})
}

// handleTestRemote tests connectivity to a remote.
func (s *Server) handleTestRemote(w http.ResponseWriter, r *http.Request) {
	ctx := r.Context()
	name := chi.URLParam(r, "name")
	if err := s.app.Rclone.TestRemote(ctx, name); err != nil {
		respondError(w, http.StatusServiceUnavailable, "test_failed", err.Error())
		return
	}
	respondOK(w, map[string]bool{"ok": true})
}
