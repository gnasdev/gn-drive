package api

import (
	"net/http"
	"net/http/httptest"
	"testing"
)

// TestAuthMiddleware_PrefixBypassFixed verifies that the old fragile
// `path[:9] == "/api/v1/a"` check no longer makes arbitrary routes public.
// A non-auth route that merely starts with "/api/v1/a" (e.g. /api/v1/about)
// must be protected when the app is locked, while genuine /api/v1/auth/*
// routes remain public.
func TestAuthMiddleware_PrefixBypassFixed(t *testing.T) {
	srv, cleanup := newTestServer(t)
	defer cleanup()

	// Put the app into a setup (and locked) state.
	if rr := doRequest(srv, "POST", "/api/v1/auth/setup", map[string]any{"password": "testpass123"}, ""); rr.Code != http.StatusCreated {
		t.Fatalf("setup: %d %s", rr.Code, rr.Body.String())
	}

	reached := false
	h := authMiddleware(srv.app.Auth)(http.HandlerFunc(func(w http.ResponseWriter, _ *http.Request) {
		reached = true
		w.WriteHeader(http.StatusOK)
	}))

	// /api/v1/about starts with "/api/v1/a" but is NOT an auth route — it must
	// be blocked (not reach the handler) when the app is locked.
	req := httptest.NewRequest("GET", "/api/v1/about", nil)
	rr := httptest.NewRecorder()
	h.ServeHTTP(rr, req)
	if reached {
		t.Error("/api/v1/about reached the handler — auth was bypassed")
	}
	if rr.Code != http.StatusUnauthorized {
		t.Errorf("/api/v1/about: want 401, got %d", rr.Code)
	}

	// A genuine auth route must still bypass auth (be public).
	reached = false
	req2 := httptest.NewRequest("POST", "/api/v1/auth/unlock", nil)
	rr2 := httptest.NewRecorder()
	h.ServeHTTP(rr2, req2)
	if !reached {
		t.Error("/api/v1/auth/unlock did not reach the handler — should be public")
	}
}
