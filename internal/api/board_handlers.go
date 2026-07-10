// Package api provides the HTTP API server for the web UI.
package api

import (
	"context"
	"errors"
	"net/http"

	"github.com/go-chi/chi/v5"
	"github.com/google/uuid"

	"github.com/gnasdev/gn-drive/internal/boardengine"
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

// handleGetBoard returns a board with nodes+edges (full graph).
func (s *Server) handleGetBoard(w http.ResponseWriter, r *http.Request) {
	ctx := r.Context()
	id := chi.URLParam(r, "id")
	b, err := s.app.Store.Boards().LoadGraph(ctx, id)
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

// handleCreateBoard creates a new board (metadata + optional graph).
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
	// Always use SaveGraph so nodes/edges round-trip when provided.
	if err := s.app.Store.Boards().SaveGraph(ctx, &b); err != nil {
		// Fallback for empty graphs if SaveGraph is strict.
		if err2 := s.app.Store.Boards().Save(ctx, &b); err2 != nil {
			respondError(w, http.StatusInternalServerError, "save_error", err.Error())
			return
		}
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
	id := chi.URLParam(r, "id")
	if b.ID == "" {
		b.ID = id
	}
	if err := s.app.Store.Boards().SaveGraph(ctx, &b); err != nil {
		if err2 := s.app.Store.Boards().Save(ctx, &b); err2 != nil {
			respondError(w, http.StatusInternalServerError, "save_error", err.Error())
			return
		}
	}
	respondOK(w, b)
}

// handleDeleteBoard deletes a board.
func (s *Server) handleDeleteBoard(w http.ResponseWriter, r *http.Request) {
	ctx := r.Context()
	id := chi.URLParam(r, "id")
	if s.app.BoardEngine != nil {
		_ = s.app.BoardEngine.Stop(id) // best-effort cancel
	}
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

// handleExecuteBoard starts board DAG execution asynchronously.
func (s *Server) handleExecuteBoard(w http.ResponseWriter, r *http.Request) {
	if s.app.BoardEngine == nil {
		respondError(w, http.StatusServiceUnavailable, "unavailable", "board engine not configured")
		return
	}
	id := chi.URLParam(r, "id")
	var req struct {
		StopOnError *bool `json:"stop_on_error"`
	}
	// Body is optional.
	_ = parseJSON(r, &req)
	stopOnError := true
	if req.StopOnError != nil {
		stopOnError = *req.StopOnError
	}

	// Detach from request context — otherwise the run is cancelled when the
	// HTTP handler returns.
	runID, err := s.app.BoardEngine.Execute(context.Background(), id, stopOnError)
	if err != nil {
		if errors.Is(err, boardengine.ErrAlreadyRunning) {
			respondError(w, http.StatusConflict, "already_running", err.Error())
			return
		}
		if errors.Is(err, boardengine.ErrEmptyBoard) {
			respondError(w, http.StatusBadRequest, "empty_board", err.Error())
			return
		}
		if errors.Is(err, store.ErrNotFound) {
			respondError(w, http.StatusNotFound, "not_found", "board not found")
			return
		}
		respondError(w, http.StatusInternalServerError, "execute_failed", err.Error())
		return
	}
	respondOK(w, map[string]any{
		"run_id":   runID,
		"board_id": id,
		"status":   "running",
	})
}

// handleStopBoard cancels a running board execution.
func (s *Server) handleStopBoard(w http.ResponseWriter, r *http.Request) {
	if s.app.BoardEngine == nil {
		respondError(w, http.StatusServiceUnavailable, "unavailable", "board engine not configured")
		return
	}
	id := chi.URLParam(r, "id")
	if err := s.app.BoardEngine.Stop(id); err != nil {
		if errors.Is(err, boardengine.ErrNotRunning) {
			respondError(w, http.StatusConflict, "not_running", err.Error())
			return
		}
		respondError(w, http.StatusInternalServerError, "stop_failed", err.Error())
		return
	}
	respondOK(w, map[string]any{"ok": true, "board_id": id, "status": "cancelling"})
}
