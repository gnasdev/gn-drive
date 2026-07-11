// Package api provides the HTTP API server for the web UI.
package api

import (
	"fmt"
	"net/http"
	"strings"

	"github.com/go-chi/chi/v5"

	"github.com/gnasdev/gn-drive/internal/eventbus"
	"github.com/gnasdev/gn-drive/internal/store"
)

// handleListProfiles returns all profiles.
func (s *Server) handleListProfiles(w http.ResponseWriter, r *http.Request) {
	ctx := r.Context()
	profiles, err := s.app.Store.Profiles().List(ctx)
	if err != nil {
		respondError(w, http.StatusInternalServerError, "db_error", err.Error())
		return
	}
	respondOK(w, profiles)
}

// handleGetProfile returns a single profile by name.
func (s *Server) handleGetProfile(w http.ResponseWriter, r *http.Request) {
	ctx := r.Context()
	name := chi.URLParam(r, "name")
	p, err := s.app.Store.Profiles().Get(ctx, name)
	if err != nil {
		if err == store.ErrNotFound {
			respondError(w, http.StatusNotFound, "not_found", "profile not found")
			return
		}
		respondError(w, http.StatusInternalServerError, "db_error", err.Error())
		return
	}
	respondOK(w, p)
}

// handleCreateProfile creates a new profile.
func (s *Server) handleCreateProfile(w http.ResponseWriter, r *http.Request) {
	ctx := r.Context()
	var p store.Profile
	if err := parseJSON(r, &p); err != nil {
		respondError(w, http.StatusBadRequest, "bad_request", err.Error())
		return
	}
	if p.Name == "" {
		respondError(w, http.StatusBadRequest, "missing_name", "profile name is required")
		return
	}
	if err := validateProfileDirection(&p); err != nil {
		respondError(w, http.StatusBadRequest, "invalid_direction", err.Error())
		return
	}
	if err := s.app.Store.Profiles().Save(ctx, &p); err != nil {
		respondError(w, http.StatusInternalServerError, "save_error", err.Error())
		return
	}
	s.app.Bus.Publish(eventbus.TopicStateChanged, eventbus.StateChangedEvent{})
	respondCreated(w, p)
}

// handleUpdateProfile updates an existing profile.
func (s *Server) handleUpdateProfile(w http.ResponseWriter, r *http.Request) {
	ctx := r.Context()
	var p store.Profile
	if err := parseJSON(r, &p); err != nil {
		respondError(w, http.StatusBadRequest, "bad_request", err.Error())
		return
	}
	if err := validateProfileDirection(&p); err != nil {
		respondError(w, http.StatusBadRequest, "invalid_direction", err.Error())
		return
	}
	if err := s.app.Store.Profiles().Save(ctx, &p); err != nil {
		respondError(w, http.StatusInternalServerError, "save_error", err.Error())
		return
	}
	s.app.Bus.Publish(eventbus.TopicStateChanged, eventbus.StateChangedEvent{})
	respondOK(w, p)
}

// validateProfileDirection enforces push | bi | bi-resync (empty → push).
func validateProfileDirection(p *store.Profile) error {
	if p == nil {
		return nil
	}
	dir := strings.TrimSpace(p.Direction)
	if dir == "" {
		p.Direction = store.ProfileDirectionPush
		return nil
	}
	if !store.IsValidProfileDirection(dir) {
		return fmt.Errorf("direction must be one of: push, bi, bi-resync")
	}
	p.Direction = dir
	return nil
}

// handleDeleteProfile deletes a profile.
func (s *Server) handleDeleteProfile(w http.ResponseWriter, r *http.Request) {
	ctx := r.Context()
	name := chi.URLParam(r, "name")
	if err := s.app.Store.Profiles().Delete(ctx, name); err != nil {
		if err == store.ErrNotFound {
			respondError(w, http.StatusNotFound, "not_found", "profile not found")
			return
		}
		respondError(w, http.StatusInternalServerError, "delete_error", err.Error())
		return
	}
	s.app.Bus.Publish(eventbus.TopicStateChanged, eventbus.StateChangedEvent{})
	respondOK(w, map[string]bool{"ok": true})
}
