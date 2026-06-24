// Package api provides the HTTP API server for the web UI.
//
// Phase 3 wires a chi router with middleware (auth cookie, recover, request-id, slog)
// and registers all REST handlers + the SSE event stream.
package api

import (
	"context"
	"crypto/rand"
	"encoding/hex"
	"encoding/json"
	"log/slog"
	"net"
	"net/http"
	"sync"
	"time"

	"github.com/go-chi/chi/v5"
	"github.com/go-chi/chi/v5/middleware"

	"github.com/gnasdev/gn-drive/internal/auth"
	"github.com/gnasdev/gn-drive/internal/eventbus"
	"github.com/gnasdev/gn-drive/internal/rclone"
	"github.com/gnasdev/gn-drive/internal/service"
	"github.com/gnasdev/gn-drive/internal/store"
	"github.com/gnasdev/gn-drive/internal/syncengine"
)

// Server is the HTTP API server.
type Server struct {
	Addr   string
	router chi.Router
	log    *slog.Logger
	app    *AppDeps
}

// AppDeps holds the services the API needs. Passed in from app.App.
type AppDeps struct {
	Auth       *auth.Service
	Store      *store.Store
	Rclone     *rclone.Client
	SyncEngine *syncengine.Engine
	Bus        *eventbus.Bus
	WebUI      http.Handler
	Service    *service.Writer // non-nil in service mode
}

// New creates a new Server. The server is not started until Serve is called.
func New(deps *AppDeps, log *slog.Logger) *Server {
	if log == nil {
		log = slog.Default()
	}

	r := chi.NewMux()
	s := &Server{router: r, app: deps, log: log}

	// Global middleware
	r.Use(middleware.Recoverer)
	r.Use(middleware.RequestID)
	r.Use(slogMiddleware(log))
	r.Use(middleware.Compress(5, "text/html", "text/plain", "application/json", "text/event-stream"))
	r.Use(corsMiddleware)

	// Static files (SPA fallback to webui)
	if deps.WebUI != nil {
		r.Handle("/*", deps.WebUI)
	}

	// Mount subroutes
	r.Mount("/api/v1", s.apiRouter())

	return s
}

func (s *Server) apiRouter() chi.Router {
	r := chi.NewRouter()
	r.Use(authMiddleware(s.app.Auth))

	// Status + SSE bypass auth middleware
	r.Get("/status", s.handleStatus)
	r.Post("/auth/unlock", s.handleUnlock)
	r.Post("/auth/setup", s.handleSetup)
	r.Post("/auth/lock", s.handleLock)
	r.Post("/auth/change-password", s.handleChangePassword)
	r.Get("/events", s.handleSSE)

	// Settings
	r.Get("/settings", s.handleGetSettings)
	r.Post("/settings", s.handleSetSettings)

	// Profiles
	r.Get("/profiles", s.handleListProfiles)
	r.Post("/profiles", s.handleCreateProfile)
	r.Get("/profiles/{name}", s.handleGetProfile)
	r.Put("/profiles/{name}", s.handleUpdateProfile)
	r.Delete("/profiles/{name}", s.handleDeleteProfile)

	// Remotes
	r.Get("/remotes", s.handleListRemotes)
	r.Post("/remotes", s.handleCreateRemote)
	r.Delete("/remotes/{name}", s.handleDeleteRemote)
	r.Post("/remotes/{name}/test", s.handleTestRemote)

	// Sync
	r.Post("/sync", s.handleStartSync)
	r.Get("/sync/tasks", s.handleListTasks)
	r.Delete("/sync/tasks/{id}", s.handleStopTask)
	r.Get("/sync/tasks/{id}/logs", s.handleTaskLogs)

	// Schedules
	r.Get("/schedules", s.handleListSchedules)
	r.Post("/schedules", s.handleCreateSchedule)
	r.Put("/schedules/{id}", s.handleUpdateSchedule)
	r.Delete("/schedules/{id}", s.handleDeleteSchedule)
	r.Post("/schedules/{id}/enable", s.handleEnableSchedule)
	r.Post("/schedules/{id}/disable", s.handleDisableSchedule)

	// Boards
	r.Get("/boards", s.handleListBoards)
	r.Post("/boards", s.handleCreateBoard)
	r.Get("/boards/{id}", s.handleGetBoard)
	r.Put("/boards/{id}", s.handleUpdateBoard)
	r.Delete("/boards/{id}", s.handleDeleteBoard)
	r.Post("/boards/{id}/execute", s.handleExecuteBoard)
	r.Post("/boards/{id}/stop", s.handleStopBoard)

	// Flows
	r.Get("/flows", s.handleListFlows)
	r.Post("/flows", s.handleCreateFlow)
	r.Put("/flows/{id}", s.handleUpdateFlow)
	r.Delete("/flows/{id}", s.handleDeleteFlow)

	// History
	r.Get("/history", s.handleListHistory)
	r.Get("/history/stats", s.handleHistoryStats)

	// Operations
	r.Post("/operations", s.handleStartOperation)
	r.Get("/operations/fs", s.handleBrowseFS)

	// Service
	r.Get("/service/status", s.handleServiceStatus)
	r.Post("/service/install", s.handleServiceInstall)
	r.Post("/service/uninstall", s.handleServiceUninstall)
	r.Post("/service/start", s.handleServiceStart)
	r.Post("/service/stop", s.handleServiceStop)
	r.Post("/service/restart", s.handleServiceRestart)

	return r
}

// Serve starts the HTTP server on the given listener.
// It blocks until the listener is closed.
func (s *Server) Serve(ln net.Listener) error {
	s.Addr = ln.Addr().String()
	return http.Serve(ln, s.router)
}

// --- middleware ---------------------------------------------------------

func slogMiddleware(log *slog.Logger) func(http.Handler) http.Handler {
	return func(next http.Handler) http.Handler {
		return http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
			ww := middleware.NewWrapResponseWriter(w, r.ProtoMajor)
			start := time.Now()
			next.ServeHTTP(ww, r)
			log.Info("http",
				slog.String("method", r.Method),
				slog.String("path", r.URL.Path),
				slog.Int("status", ww.Status()),
				slog.Int("size", ww.BytesWritten()),
				slog.Duration("dur", time.Since(start)),
				slog.String("req_id", middleware.GetReqID(r.Context())),
			)
		})
	}
}

func corsMiddleware(next http.Handler) http.Handler {
	return http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		origin := r.Header.Get("Origin")
		if origin != "" {
			w.Header().Set("Access-Control-Allow-Origin", origin)
			w.Header().Set("Access-Control-Allow-Credentials", "true")
			w.Header().Set("Vary", "Origin")
		}
		if r.Method == "OPTIONS" {
			w.Header().Set("Access-Control-Allow-Methods", "GET, POST, PUT, DELETE, OPTIONS")
			w.Header().Set("Access-Control-Allow-Headers", "Content-Type, Authorization, X-Request-ID")
			w.WriteHeader(http.StatusNoContent)
			return
		}
		next.ServeHTTP(w, r)
	})
}

func authMiddleware(authSvc *auth.Service) func(http.Handler) http.Handler {
	return func(next http.Handler) http.Handler {
		return http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
			// Public paths bypass auth check entirely.
			path := r.URL.Path
			if path == "/api/v1/status" ||
				path == "/api/v1/events" ||
				len(path) >= 9 && path[:9] == "/api/v1/a" {
				// /api/v1/auth/* — public (unlock, setup, lock, change-password)
				next.ServeHTTP(w, r)
				return
			}

			if !authSvc.IsSetup() {
				next.ServeHTTP(w, r)
				return
			}
			if !authSvc.IsUnlocked() {
				http.Error(w, `{"error":"app is locked","code":"locked"}`, http.StatusUnauthorized)
				return
			}
			cookie, err := r.Cookie(SessionCookieName)
			if err != nil || cookie == nil || cookie.Value == "" {
				http.Error(w, `{"error":"session required","code":"unauthorized"}`, http.StatusUnauthorized)
				return
			}
			if !sessionValid(cookie.Value) {
				http.Error(w, `{"error":"invalid session","code":"unauthorized"}`, http.StatusUnauthorized)
				return
			}
			next.ServeHTTP(w, r)
		})
	}
}

// --- helpers -----------------------------------------------------------

func respondJSON(w http.ResponseWriter, status int, v any) {
	w.Header().Set("Content-Type", "application/json")
	w.WriteHeader(status)
	_ = json.NewEncoder(w).Encode(v)
}

func respondError(w http.ResponseWriter, status int, code, msg string) {
	respondJSON(w, status, map[string]string{"error": msg, "code": code})
}

func respondOK(w http.ResponseWriter, v any)         { respondJSON(w, http.StatusOK, v) }
func respondCreated(w http.ResponseWriter, v any)    { respondJSON(w, http.StatusCreated, v) }
func respondNoContent(w http.ResponseWriter)         { w.WriteHeader(http.StatusNoContent) }

func parseJSON(r *http.Request, v any) error {
	return json.NewDecoder(r.Body).Decode(v)
}

// generateTokenRand is overridable for tests; defaults to rand.Read.
var generateTokenRand = rand.Read

func generateToken() (string, error) {
	b := make([]byte, 32)
	if _, err := generateTokenRand(b); err != nil {
		return "", err
	}
	return hex.EncodeToString(b), nil
}

const SessionCookieName = "gn-drive-session"

func setSessionCookie(w http.ResponseWriter, token string) {
	http.SetCookie(w, &http.Cookie{
		Name:     SessionCookieName,
		Value:    token,
		Path:     "/",
		HttpOnly: true,
		Secure:   false, // local-only, no HTTPS
		SameSite: http.SameSiteStrictMode,
		MaxAge:   86400,
	})
}

func clearSessionCookie(w http.ResponseWriter) {
	http.SetCookie(w, &http.Cookie{
		Name:     SessionCookieName,
		Value:    "",
		Path:     "/",
		HttpOnly: true,
		Secure:   false,
		SameSite: http.SameSiteStrictMode,
		MaxAge:   -1,
		Expires:  time.Unix(0, 0),
	})
}

// SessionStore is an in-memory, process-local session token registry.
// Valid for the lifetime of the process; cleared on shutdown. Token format
// is opaque to the caller — produced by crypto/rand in generateToken.
type SessionStore struct {
	mu     sync.RWMutex
	tokens map[string]struct{}
}

// NewSessionStore creates a new empty SessionStore.
func NewSessionStore() *SessionStore {
	return &SessionStore{tokens: make(map[string]struct{})}
}

// Add registers a token. Duplicate adds are idempotent.
func (s *SessionStore) Add(token string) {
	s.mu.Lock()
	s.tokens[token] = struct{}{}
	s.mu.Unlock()
}

// Valid reports whether the token is currently registered.
func (s *SessionStore) Valid(token string) bool {
	s.mu.RLock()
	_, ok := s.tokens[token]
	s.mu.RUnlock()
	return ok
}

// Delete removes a token. Missing-token deletes are no-ops.
func (s *SessionStore) Delete(token string) {
	s.mu.Lock()
	delete(s.tokens, token)
	s.mu.Unlock()
}

// Count returns the number of registered tokens (for tests/diagnostics).
func (s *SessionStore) Count() int {
	s.mu.RLock()
	defer s.mu.RUnlock()
	return len(s.tokens)
}

// sessionStore is the process-wide SessionStore used by the HTTP handlers.
// It is replaced with a fresh instance in tests; production code should
// always go through this var so cookies minted by one handler are visible
// to another.
var sessionStore = NewSessionStore()

func sessionValid(t string) bool   { return sessionStore.Valid(t) }
func sessionAdd(t string)          { sessionStore.Add(t) }
func sessionDelete(t string)       { sessionStore.Delete(t) }

// silence unused
var _ = context.Background
