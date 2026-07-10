// Package api provides the HTTP API server for the web UI.
package api

import (
	"net/http"
	"path"
	"strings"

	"github.com/gnasdev/gn-drive/internal/rclone"
)

// fsEntry is the JSON shape path browsers (RemotePathField) expect.
type fsEntry struct {
	Name    string `json:"name"`
	Path    string `json:"path"`
	Size    int64  `json:"size"`
	IsDir   bool   `json:"is_dir"`
	ModTime string `json:"mod_time"`
}

// handleStartOperation runs a one-shot file operation (copy/move/check/mkdir/purge/delete).
// Body:
//
//	{
//	  "op": "copy"|"move"|"check"|"mkdir"|"purge"|"delete",
//	  "source": "remote:path or /abs",   // required for copy/move/check
//	  "dest":   "remote:path or /abs",   // required for copy/move/check
//	  "path":   "remote:path or /abs"    // required for mkdir/purge/delete
//	}
func (s *Server) handleStartOperation(w http.ResponseWriter, r *http.Request) {
	if s.app.Rclone == nil {
		respondError(w, http.StatusServiceUnavailable, "rclone_unavailable", "rclone client not initialized")
		return
	}
	var req struct {
		Op     string `json:"op"`
		Source string `json:"source"`
		Dest   string `json:"dest"`
		Path   string `json:"path"`
	}
	if err := parseJSON(r, &req); err != nil {
		respondError(w, http.StatusBadRequest, "bad_request", err.Error())
		return
	}
	op := strings.ToLower(strings.TrimSpace(req.Op))
	if op == "" {
		respondError(w, http.StatusBadRequest, "missing_op", "op is required")
		return
	}

	ctx := r.Context()
	switch op {
	case "copy", "move", "check":
		if req.Source == "" || req.Dest == "" {
			respondError(w, http.StatusBadRequest, "missing_paths", "source and dest are required for "+op)
			return
		}
		action := rclone.ActionCopy
		if op == "move" {
			action = rclone.ActionMove
		} else if op == "check" {
			action = rclone.ActionCheck
		}
		_, err := s.app.Rclone.Sync(ctx, rclone.SyncConfig{
			Action: action,
			Source: req.Source,
			Dest:   req.Dest,
			Profile: &rclone.ProfileFlags{
				Transfers: 4,
			},
		}, nil)
		if err != nil {
			respondError(w, http.StatusBadRequest, "op_failed", err.Error())
			return
		}
		respondOK(w, map[string]any{"ok": true, "op": op, "source": req.Source, "dest": req.Dest})

	case "mkdir":
		p := req.Path
		if p == "" {
			p = req.Dest
		}
		if p == "" {
			respondError(w, http.StatusBadRequest, "missing_path", "path is required for mkdir")
			return
		}
		if err := s.app.Rclone.Mkdir(ctx, p); err != nil {
			respondError(w, http.StatusBadRequest, "op_failed", err.Error())
			return
		}
		respondOK(w, map[string]any{"ok": true, "op": op, "path": p})

	case "purge":
		p := req.Path
		if p == "" {
			p = req.Dest
		}
		if p == "" {
			respondError(w, http.StatusBadRequest, "missing_path", "path is required for purge")
			return
		}
		if err := s.app.Rclone.Purge(ctx, p); err != nil {
			respondError(w, http.StatusBadRequest, "op_failed", err.Error())
			return
		}
		respondOK(w, map[string]any{"ok": true, "op": op, "path": p})

	case "delete", "deletefile":
		p := req.Path
		if p == "" {
			p = req.Source
		}
		if p == "" {
			respondError(w, http.StatusBadRequest, "missing_path", "path is required for delete")
			return
		}
		if err := s.app.Rclone.DeleteFile(ctx, p); err != nil {
			respondError(w, http.StatusBadRequest, "op_failed", err.Error())
			return
		}
		respondOK(w, map[string]any{"ok": true, "op": "delete", "path": p})

	default:
		respondError(w, http.StatusBadRequest, "unknown_op", "supported ops: copy, move, check, mkdir, purge, delete")
	}
}

// handleBrowseFS lists files/dirs at a remote path via rclone lsjson.
// Query: ?remote=remote:path or absolute local path.
func (s *Server) handleBrowseFS(w http.ResponseWriter, r *http.Request) {
	remote := strings.TrimSpace(r.URL.Query().Get("remote"))
	if remote == "" {
		remote = strings.TrimSpace(r.URL.Query().Get("path"))
	}
	if remote == "" {
		respondError(w, http.StatusBadRequest, "missing_remote", "query param remote is required (e.g. gdrive:/ or /tmp)")
		return
	}
	if s.app.Rclone == nil {
		respondError(w, http.StatusServiceUnavailable, "rclone_unavailable", "rclone client not initialized")
		return
	}
	entries, err := s.app.Rclone.ListFiles(r.Context(), remote)
	if err != nil {
		respondError(w, http.StatusBadRequest, "browse_failed", err.Error())
		return
	}
	out := make([]fsEntry, 0, len(entries))
	for _, e := range entries {
		name := e.Name
		if name == "" {
			name = path.Base(e.Path)
		}
		p := e.Path
		if p == "" {
			p = name
		}
		out = append(out, fsEntry{
			Name:    name,
			Path:    p,
			Size:    e.Size,
			IsDir:   e.IsDir,
			ModTime: e.ModTime,
		})
	}
	respondOK(w, out)
}
