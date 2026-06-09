// Package api provides the HTTP API server for the web UI.
package api

import (
	"encoding/json"
	"fmt"
	"io"
	"log/slog"
	"net/http"
	"time"

	"github.com/gnasdev/gn-drive/internal/eventbus"
)

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
	w.WriteHeader(http.StatusOK)
	flusher.Flush()

	topics := eventbus.AllTopics()
	subs := make([]func(), 0, len(topics))
	ctx := r.Context()

	for _, t := range topics {
		topic := t
		cancel := s.app.Bus.Subscribe(topic, makeSSEHandler(w, flusher, topic, s.log))
		subs = append(subs, cancel)
	}
	defer func() {
		for _, c := range subs {
			c()
		}
	}()

	heartbeat := time.NewTicker(25 * time.Second)
	defer heartbeat.Stop()

	for {
		select {
		case <-ctx.Done():
			return
		case <-heartbeat.C:
			io.WriteString(w, ": heartbeat\n\n")
			flusher.Flush()
		}
	}
}

func makeSSEHandler(w http.ResponseWriter, flusher http.Flusher, topic string, log *slog.Logger) func(eventbus.Event) {
	return func(ev eventbus.Event) {
		data, err := json.Marshal(ev)
		if err != nil {
			log.Warn("sse: marshal event", "topic", topic, "err", err)
			return
		}
		fmt.Fprintf(w, "event: %s\n", topic)
		fmt.Fprintf(w, "data: %s\n\n", data)
		flusher.Flush()
	}
}

// handleStatus returns the current app status (auth state + version).
// Registered as a public route (no auth required).
func (s *Server) handleStatus(w http.ResponseWriter, r *http.Request) {
	status := s.app.Auth.Status()
	respondJSON(w, http.StatusOK, map[string]any{
		"setup":    status.Setup,
		"unlocked": status.Unlocked,
		"lockout":  status.Lockout,
		"version":  "dev",
	})
}
