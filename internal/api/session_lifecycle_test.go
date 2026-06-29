package api

import (
	"net/http"
	"testing"
	"time"
)

// TestSessionStore_Expiry verifies tokens become invalid after sessionTTL and
// are removed lazily on lookup.
func TestSessionStore_Expiry(t *testing.T) {
	origNow, origTTL := sessionNow, sessionTTL
	defer func() { sessionNow, sessionTTL = origNow, origTTL }()

	base := time.Unix(1000, 0)
	sessionNow = func() time.Time { return base }
	sessionTTL = time.Minute

	s := NewSessionStore()
	s.Add("tok")
	if !s.Valid("tok") {
		t.Fatal("token should be valid before expiry")
	}

	// Advance past the TTL.
	sessionNow = func() time.Time { return base.Add(2 * time.Minute) }
	if s.Valid("tok") {
		t.Error("token should be invalid after expiry")
	}
	if s.Count() != 0 {
		t.Errorf("expired token should be removed lazily, Count = %d", s.Count())
	}
}

// TestSessionStore_Clear verifies Clear revokes every token.
func TestSessionStore_Clear(t *testing.T) {
	s := NewSessionStore()
	s.Add("a")
	s.Add("b")
	s.Add("c")
	if s.Count() != 3 {
		t.Fatalf("Count = %d, want 3", s.Count())
	}
	s.Clear()
	if s.Count() != 0 {
		t.Errorf("after Clear Count = %d, want 0", s.Count())
	}
	if s.Valid("a") {
		t.Error("token should be invalid after Clear")
	}
}

// TestHandleLock_RevokesAllSessions verifies that locking the app invalidates
// every outstanding session, not just the caller's cookie.
func TestHandleLock_RevokesAllSessions(t *testing.T) {
	srv, cleanup := newTestServer(t)
	defer cleanup()

	if rr := doRequest(srv, "POST", "/api/v1/auth/setup", map[string]any{"password": "testpass123"}, ""); rr.Code != http.StatusCreated {
		t.Fatalf("setup: %d %s", rr.Code, rr.Body.String())
	}

	// A second, independent session token.
	sessionAdd("other-session-token")
	if !sessionValid("other-session-token") {
		t.Fatal("precondition: extra token should be valid")
	}

	if rr := doRequest(srv, "POST", "/api/v1/auth/lock", nil, ""); rr.Code != http.StatusOK {
		t.Fatalf("lock: %d %s", rr.Code, rr.Body.String())
	}

	if sessionValid("other-session-token") {
		t.Error("lock must revoke ALL sessions, but the extra token is still valid")
	}
}
