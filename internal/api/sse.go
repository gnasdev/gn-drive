// Package api provides the HTTP API server for the web UI.
package api

import (
	"encoding/json"
	"fmt"
	"io"
	"log/slog"
	"net/http"
	"sync"
	"time"

	"github.com/gnasdev/gn-drive/internal/eventbus"
)

// sseHeartbeatInterval is overridable for tests to speed up the
// heartbeat.
var sseHeartbeatInterval = 25 * time.Second

// sseNewTickerFn is overridable for tests; defaults to time.NewTicker.
var sseNewTickerFn = func(d time.Duration) tickerIface {
	return tickerAdapter{time.NewTicker(d)}
}

type tickerIface interface {
	C() <-chan time.Time
	Stop()
}

type tickerAdapter struct {
	*time.Ticker
}

func (t tickerAdapter) C() <-chan time.Time { return t.Ticker.C }
func (t tickerAdapter) Stop()               { t.Ticker.Stop() }

// handleSSE streams events from the eventbus as Server-Sent Events.
func (s *Server) handleSSE(w http.ResponseWriter, r *http.Request) {
	flusher, ok := w.(http.Flusher)
	if !ok {
		http.Error(w, "streaming not supported", http.StatusInternalServerError)
		return
	}

	w.Header().Set("Content-Type", "text/event-stream; charset=utf-8")
	w.Header().Set("Cache-Control", "no-cache, no-store")
	w.Header().Set("Connection", "keep-alive")
	w.Header().Set("X-Accel-Buffering", "no")
	// Disable proxy/transform buffering that can hold SSE frames.
	w.Header().Set("Content-Encoding", "identity")
	w.WriteHeader(http.StatusOK)
	flusher.Flush()

	// Serialize concurrent bus → ResponseWriter writes (one sub per topic).
	var writeMu sync.Mutex

	topics := eventbus.AllTopics()
	subs := make([]func(), 0, len(topics))
	ctx := r.Context()

	for _, t := range topics {
		topic := t
		cancel := s.app.Bus.Subscribe(topic, makeSSEHandlerFn(w, flusher, topic, s.log, &writeMu))
		subs = append(subs, cancel)
	}
	defer func() {
		for _, c := range subs {
			c()
		}
	}()

	heartbeat := sseNewTickerFn(sseHeartbeatInterval)
	defer heartbeat.Stop()

	for {
		select {
		case <-ctx.Done():
			return
		case <-heartbeat.C():
			writeMu.Lock()
			_, _ = io.WriteString(w, ": heartbeat\n\n")
			flusher.Flush()
			writeMu.Unlock()
		}
	}
}

func makeSSEHandler(w http.ResponseWriter, flusher http.Flusher, topic string, log *slog.Logger) func(eventbus.Event) {
	var mu sync.Mutex
	return makeSSEHandlerFn(w, flusher, topic, log, &mu)
}

// makeSSEHandlerFn is overridable for tests.
var makeSSEHandlerFn = func(w http.ResponseWriter, flusher http.Flusher, topic string, log *slog.Logger, writeMu *sync.Mutex) func(eventbus.Event) {
	return func(ev eventbus.Event) {
		data, err := json.Marshal(ev)
		if err != nil {
			log.Warn("sse: marshal event", "topic", topic, "err", err)
			return
		}
		if writeMu != nil {
			writeMu.Lock()
			defer writeMu.Unlock()
		}
		_, _ = fmt.Fprintf(w, "event: %s\n", topic)
		_, _ = fmt.Fprintf(w, "data: %s\n\n", data)
		flusher.Flush()
	}
}

// handleStatus returns the current app status (auth state + version).
// Registered as a public route (no auth required).
//
// Web session model:
//   - Unlock/setup mint an HttpOnly session cookie (see handleUnlock).
//   - On SPA reload the process may still be unlocked while the browser
//     needs a valid cookie for protected APIs. If crypto is unlocked but
//     the cookie is missing/expired, mint a fresh session here so reload
//     does not force the user through the password form again.
//   - When the process is locked, unlocked stays false (show unlock UI).
func (s *Server) handleStatus(w http.ResponseWriter, r *http.Request) {
	status := s.app.Auth.Status()
	ver := s.app.Version
	if ver == "" {
		ver = "dev"
	}

	sessionOK := false
	if cookie, err := r.Cookie(SessionCookieName); err == nil && cookie != nil && cookie.Value != "" {
		sessionOK = sessionValid(cookie.Value)
	}
	// Process already unlocked (prior unlock in this process) but browser
	// has no live session — re-issue cookie for this client.
	if status.Unlocked && !sessionOK {
		if token, err := generateToken(); err == nil {
			sessionAdd(token)
			setSessionCookie(w, token)
			sessionOK = true
		}
	}

	// For the SPA gate: "unlocked" means the user may enter the app.
	// Require a web session once setup is complete.
	webUnlocked := status.Unlocked
	if status.Setup {
		webUnlocked = status.Unlocked && sessionOK
	}

	respondJSON(w, http.StatusOK, map[string]any{
		"setup":    status.Setup,
		"unlocked": webUnlocked,
		"session":  sessionOK,
		"lockout":  status.Lockout,
		"version":  ver,
	})
}
