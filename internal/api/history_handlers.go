// Package api provides the HTTP API server for the web UI.
package api

import (
	"net/http"
	"strconv"

	"github.com/gnasdev/gn-drive/internal/store"
)

// handleListHistory returns paginated history entries.
func (s *Server) handleListHistory(w http.ResponseWriter, r *http.Request) {
	ctx := r.Context()
	limit, _ := strconv.Atoi(r.URL.Query().Get("limit"))
	offset, _ := strconv.Atoi(r.URL.Query().Get("offset"))
	if limit <= 0 || limit > 1000 {
		limit = 100
	}
	if offset < 0 {
		offset = 0
	}
	profile := r.URL.Query().Get("profile")

	var entries []store.HistoryEntry
	var err error
	if profile != "" {
		entries, err = s.app.Store.History().ListByProfile(ctx, profile, limit, offset)
	} else {
		entries, err = s.app.Store.History().List(ctx, limit, offset)
	}
	if err != nil {
		respondError(w, http.StatusInternalServerError, "db_error", err.Error())
		return
	}
	respondOK(w, entries)
}

// handleHistoryStats returns aggregate stats.
func (s *Server) handleHistoryStats(w http.ResponseWriter, r *http.Request) {
	ctx := r.Context()
	stats, err := s.app.Store.History().Stats(ctx)
	if err != nil {
		respondError(w, http.StatusInternalServerError, "db_error", err.Error())
		return
	}
	respondOK(w, stats)
}
