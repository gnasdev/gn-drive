// Package api provides the HTTP API server for the web UI.
package api

import (
	"net/http"

	"github.com/go-chi/chi/v5"

	"github.com/gnasdev/gn-drive/internal/store"
)

// handleListFlows returns all flows.
func (s *Server) handleListFlows(w http.ResponseWriter, r *http.Request) {
	ctx := r.Context()
	flows, err := s.app.Store.Flows().List(ctx)
	if err != nil {
		respondError(w, http.StatusInternalServerError, "db_error", err.Error())
		return
	}
	respondOK(w, flows)
}

// handleCreateFlow creates a new flow.
func (s *Server) handleCreateFlow(w http.ResponseWriter, r *http.Request) {
	ctx := r.Context()
	var f store.Flow
	if err := parseJSON(r, &f); err != nil {
		respondError(w, http.StatusBadRequest, "bad_request", err.Error())
		return
	}
	if err := s.app.Store.Flows().Save(ctx, &f); err != nil {
		respondError(w, http.StatusInternalServerError, "save_error", err.Error())
		return
	}
	respondCreated(w, f)
}

// handleUpdateFlow updates a flow.
func (s *Server) handleUpdateFlow(w http.ResponseWriter, r *http.Request) {
	ctx := r.Context()
	var f store.Flow
	if err := parseJSON(r, &f); err != nil {
		respondError(w, http.StatusBadRequest, "bad_request", err.Error())
		return
	}
	if err := s.app.Store.Flows().Save(ctx, &f); err != nil {
		respondError(w, http.StatusInternalServerError, "save_error", err.Error())
		return
	}
	respondOK(w, f)
}

// handleDeleteFlow deletes a flow.
func (s *Server) handleDeleteFlow(w http.ResponseWriter, r *http.Request) {
	ctx := r.Context()
	id := chi.URLParam(r, "id")
	if err := s.app.Store.Flows().Delete(ctx, id); err != nil {
		if err == store.ErrNotFound {
			respondError(w, http.StatusNotFound, "not_found", "flow not found")
			return
		}
		respondError(w, http.StatusInternalServerError, "delete_error", err.Error())
		return
	}
	respondOK(w, map[string]bool{"ok": true})
}
