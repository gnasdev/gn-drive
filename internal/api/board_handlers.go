// Package api provides the HTTP API server for the web UI.
package api

import (
	"net/http"

	"github.com/go-chi/chi/v5"
	"github.com/google/uuid"

	"github.com/gnasdev/gn-drive/internal/store"
)

// handleListBoards returns all boards.
func (s *Server) handleListBoards(w http.ResponseWriter, r *http.Request) {
	ctx := r.Context()
	boards, err := s.app.Store.Boards().List(ctx)
	if err != nil {
		respondError(w, http.StatusInternalServerError, "db_error", err.Error())
		return
	}
	respondOK(w, boards)
}

// handleGetBoard returns a board with nodes+edges.
func (s *Server) handleGetBoard(w http.ResponseWriter, r *http.Request) {
	ctx := r.Context()
	id := chi.URLParam(r, "id")
	b, err := s.app.Store.Boards().Get(ctx, id)
	if err != nil {
		if err == store.ErrNotFound {
			respondError(w, http.StatusNotFound, "not_found", "board not found")
			return
		}
		respondError(w, http.StatusInternalServerError, "db_error", err.Error())
		return
	}
	respondOK(w, b)
}

// handleCreateBoard creates a new board.
func (s *Server) handleCreateBoard(w http.ResponseWriter, r *http.Request) {
	ctx := r.Context()
	var b store.Board
	if err := parseJSON(r, &b); err != nil {
		respondError(w, http.StatusBadRequest, "bad_request", err.Error())
		return
	}
	if b.ID == "" {
		b.ID = uuid.New().String()
	}
	if err := s.app.Store.Boards().Save(ctx, &b); err != nil {
		respondError(w, http.StatusInternalServerError, "save_error", err.Error())
		return
	}
	respondCreated(w, b)
}

// handleUpdateBoard updates a board.
func (s *Server) handleUpdateBoard(w http.ResponseWriter, r *http.Request) {
	ctx := r.Context()
	var b store.Board
	if err := parseJSON(r, &b); err != nil {
		respondError(w, http.StatusBadRequest, "bad_request", err.Error())
		return
	}
	if err := s.app.Store.Boards().Save(ctx, &b); err != nil {
		respondError(w, http.StatusInternalServerError, "save_error", err.Error())
		return
	}
	respondOK(w, b)
}

// handleDeleteBoard deletes a board.
func (s *Server) handleDeleteBoard(w http.ResponseWriter, r *http.Request) {
	ctx := r.Context()
	id := chi.URLParam(r, "id")
	if err := s.app.Store.Boards().Delete(ctx, id); err != nil {
		if err == store.ErrNotFound {
			respondError(w, http.StatusNotFound, "not_found", "board not found")
			return
		}
		respondError(w, http.StatusInternalServerError, "delete_error", err.Error())
		return
	}
	respondOK(w, map[string]bool{"ok": true})
}

// handleExecuteBoard starts board DAG execution.
func (s *Server) handleExecuteBoard(w http.ResponseWriter, r *http.Request) {
	// Phase 3: board execution is a stub.
	respondError(w, http.StatusNotImplemented, "not_implemented", "board execution is implemented in phase 6")
}

// handleStopBoard stops a running board execution.
func (s *Server) handleStopBoard(w http.ResponseWriter, r *http.Request) {
	respondError(w, http.StatusNotImplemented, "not_implemented", "board execution is implemented in phase 6")
}
