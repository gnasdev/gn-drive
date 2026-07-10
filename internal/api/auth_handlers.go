// Package api provides the HTTP API server for the web UI.
package api

import (
	"net/http"

	"github.com/gnasdev/gn-drive/internal/auth"
	"github.com/gnasdev/gn-drive/internal/eventbus"
)

// handleUnlock verifies password, derives AES key, decrypts config files,
// opens the data plane if deferred, and sets a session cookie.
func (s *Server) handleUnlock(w http.ResponseWriter, r *http.Request) {
	var req struct {
		Password string `json:"password"`
	}
	if err := parseJSON(r, &req); err != nil {
		respondError(w, http.StatusBadRequest, "bad_request", err.Error())
		return
	}
	if req.Password == "" {
		respondError(w, http.StatusBadRequest, "missing_password", "password is required")
		return
	}

	if err := s.app.Auth.Unlock(req.Password); err != nil {
		respondError(w, http.StatusUnauthorized, "unlock_failed", err.Error())
		return
	}

	if s.app.AfterUnlock != nil {
		if err := s.app.AfterUnlock(r.Context()); err != nil {
			// Roll back unlock so the user can retry cleanly.
			_ = authLockFn(s.app.Auth)
			respondError(w, http.StatusInternalServerError, "data_plane", err.Error())
			return
		}
	}

	token, err := generateToken()
	if err != nil {
		respondError(w, http.StatusInternalServerError, "internal", err.Error())
		return
	}
	sessionAdd(token)
	setSessionCookie(w, token)

	s.app.Bus.Publish(eventbus.TopicAuthUnlocked, eventbus.AuthUnlockedEvent{})

	respondOK(w, map[string]string{"token": token})
}

// handleSetup configures a new master password for the first time.
func (s *Server) handleSetup(w http.ResponseWriter, r *http.Request) {
	var req struct {
		Password string `json:"password"`
	}
	if err := parseJSON(r, &req); err != nil {
		respondError(w, http.StatusBadRequest, "bad_request", err.Error())
		return
	}
	if req.Password == "" || len(req.Password) < 4 {
		respondError(w, http.StatusBadRequest, "weak_password", "password must be at least 4 characters")
		return
	}

	if err := s.app.Auth.SetupPassword(req.Password); err != nil {
		respondError(w, http.StatusInternalServerError, "setup_failed", err.Error())
		return
	}

	if s.app.AfterUnlock != nil {
		if err := s.app.AfterUnlock(r.Context()); err != nil {
			respondError(w, http.StatusInternalServerError, "data_plane", err.Error())
			return
		}
	}

	token, err := generateToken()
	if err != nil {
		respondError(w, http.StatusInternalServerError, "internal", err.Error())
		return
	}
	sessionAdd(token)
	setSessionCookie(w, token)

	s.app.Bus.Publish(eventbus.TopicAuthUnlocked, eventbus.AuthUnlockedEvent{})

	respondCreated(w, map[string]string{"token": token})
}

// authLockFn is overridable for tests; defaults to auth.Service.Lock.
var authLockFn = func(a *auth.Service) error {
	return a.Lock()
}

// handleLock encrypts config files and clears the session.
func (s *Server) handleLock(w http.ResponseWriter, r *http.Request) {
	// Find our session token.
	cookie, _ := r.Cookie(SessionCookieName)
	if cookie != nil && cookie.Value != "" {
		clearSessionCookie(w)
	}
	// Revoke ALL sessions, not just the caller's: locking the app must
	// invalidate every outstanding session.
	sessionClearAll()

	// Close sqlite/rclone before re-encrypt so file handles are released.
	if s.app.BeforeLock != nil {
		if err := s.app.BeforeLock(); err != nil {
			respondError(w, http.StatusInternalServerError, "lock_failed", err.Error())
			return
		}
	}

	if err := authLockFn(s.app.Auth); err != nil {
		respondError(w, http.StatusInternalServerError, "lock_failed", err.Error())
		return
	}
	s.app.Bus.Publish(eventbus.TopicAuthLocked, eventbus.AuthLockedEvent{})

	respondOK(w, map[string]bool{"ok": true})
}

// handleChangePassword changes the master password.
func (s *Server) handleChangePassword(w http.ResponseWriter, r *http.Request) {
	var req struct {
		OldPassword string `json:"old_password"`
		NewPassword string `json:"new_password"`
	}
	if err := parseJSON(r, &req); err != nil {
		respondError(w, http.StatusBadRequest, "bad_request", err.Error())
		return
	}
	if req.NewPassword == "" || len(req.NewPassword) < 4 {
		respondError(w, http.StatusBadRequest, "weak_password", "new password must be at least 4 characters")
		return
	}

	if err := s.app.Auth.ChangePassword(req.OldPassword, req.NewPassword); err != nil {
		respondError(w, http.StatusForbidden, "change_failed", err.Error())
		return
	}
	// Invalidate all outstanding sessions (including the caller's) so a
	// password change forces re-authentication everywhere.
	sessionClearAll()
	clearSessionCookie(w)
	respondOK(w, map[string]bool{"ok": true})
}
