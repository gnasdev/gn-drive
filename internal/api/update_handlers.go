package api

import (
	"bytes"
	"errors"
	"fmt"
	"net/http"
	"time"

	"github.com/gnasdev/gn-drive/internal/selfupdate"
)

// handleSelfUpdate runs the GitHub Releases update pipeline (Settings UI).
// Response shape matches the frontend: { ok, output }.
func (s *Server) handleSelfUpdate(w http.ResponseWriter, r *http.Request) {
	ctx := r.Context()
	ver := s.app.Version
	if ver == "" {
		ver = "dev"
	}

	var buf bytes.Buffer
	res, err := selfupdate.Update(ctx, selfupdate.Options{
		CurrentVersion: ver,
		Stdout:         &buf,
	})
	if errors.Is(err, selfupdate.ErrAlreadyUpToDate) {
		respondOK(w, map[string]any{
			"ok":     true,
			"output": fmt.Sprintf("already on latest version (%s)", ver),
		})
		return
	}
	if err != nil {
		// Include any progress already written.
		msg := err.Error()
		if buf.Len() > 0 {
			msg = buf.String() + "\n" + msg
		}
		respondError(w, http.StatusInternalServerError, "update_failed", msg)
		return
	}

	out := buf.String()
	if res != nil {
		if out != "" && out[len(out)-1] != '\n' {
			out += "\n"
		}
		out += fmt.Sprintf("updated %s → %s\n", res.OldVersion, res.NewVersion)
		if res.BinaryPath != "" {
			out += fmt.Sprintf("binary: %s\n", res.BinaryPath)
		}
		if res.RestartHint != "" {
			out += res.RestartHint + "\n"
		}
		_ = res.ReleasedAt // keep field used if zero
	}
	if out == "" {
		out = "update completed"
	}
	respondOK(w, map[string]any{
		"ok":     true,
		"output": out,
		"at":     time.Now().UTC().Format(time.RFC3339),
	})
}
