// Package api provides the HTTP API server for the web UI.
package api

import (
	"net/http"

	"github.com/gnasdev/gn-drive/internal/eventbus"
)

// handleGetSettings returns app settings.
func (s *Server) handleGetSettings(w http.ResponseWriter, r *http.Request) {
	ctx := r.Context()
	settings := make(map[string]string)
	keys := []string{
		"theme", "notifications_enabled", "debug_mode",
		"minimize_to_tray", "start_at_login",
	}
	for _, key := range keys {
		val, err := s.app.Store.Settings().Get(ctx, key)
		if err == nil {
			settings[key] = val
		}
	}
	respondOK(w, settings)
}

// handleSetSettings saves app settings.
func (s *Server) handleSetSettings(w http.ResponseWriter, r *http.Request) {
	ctx := r.Context()
	var settings map[string]string
	if err := parseJSON(r, &settings); err != nil {
		respondError(w, http.StatusBadRequest, "bad_request", err.Error())
		return
	}
	for k, v := range settings {
		if err := s.app.Store.Settings().Set(ctx, k, v); err != nil {
			respondError(w, http.StatusInternalServerError, "save_error", err.Error())
			return
		}
	}
	s.app.Bus.Publish(eventbus.TopicStateChanged, eventbus.StateChangedEvent{})
	respondOK(w, map[string]bool{"ok": true})
}
