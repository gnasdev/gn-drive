// Package api provides the HTTP API server for the web UI.
package api

import (
	"net/http"
)

// handleStartOperation starts a file operation (copy, move, etc.).
// Phase 3: returns not implemented.
func (s *Server) handleStartOperation(w http.ResponseWriter, r *http.Request) {
	respondError(w, http.StatusNotImplemented, "not_implemented", "file operations are implemented in phase 6")
}

// handleBrowseFS lists files at a remote path.
// Phase 3: returns not implemented.
func (s *Server) handleBrowseFS(w http.ResponseWriter, r *http.Request) {
	respondError(w, http.StatusNotImplemented, "not_implemented", "file browser is implemented in phase 6")
}
