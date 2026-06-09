// Package api provides the HTTP API server for the web UI.
package api

import (
	"net/http"
	"os/exec"
	"time"

	"github.com/gnasdev/gn-drive/internal/service"
)

// serviceSpec returns the default service.Spec for the current user.
// Service is always user-level from the web UI; system-level requires sudo
// and is not exposed here.
func (s *Server) serviceSpec() service.Spec {
	return service.DefaultSpec(service.ScopeUser)
}

// runServiceCLI runs `gn-drive service <action>` and returns the combined output.
func (s *Server) runServiceCLI(action string) (string, error) {
	exe, err := ownExecutable()
	if err != nil {
		return "", err
	}
	out, err := exec.Command(exe, "service", action).CombinedOutput()
	return string(out), err
}

// handleServiceStatus returns the current service state and health.
func (s *Server) handleServiceStatus(w http.ResponseWriter, r *http.Request) {
	spec := s.serviceSpec()
	mgr, err := service.NewManager()
	if err != nil {
		respondError(w, http.StatusInternalServerError, "platform_error", err.Error())
		return
	}

	installed, _ := mgr.IsInstalled(spec)
	resp := map[string]any{
		"platform":     service.Platform(),
		"scope":        string(spec.Scope),
		"installed":    installed,
		"running":      false,
		"pid":          0,
		"web_port":     0,
		"uptime_secs":  0,
		"started_at":   "",
		"last_heartbeat": "",
		"last_error":   "",
		"active_tasks": []string{},
	}

	if installed {
		st, _ := mgr.Status(spec)
		resp["running"] = st.Running
		resp["pid"] = st.PID
	}

	// Read health file (process-local, not service-level).
	if h, err := service.ReadHealth(spec.ConfigDir); err == nil {
		resp["web_port"] = h.WebPort
		resp["active_tasks"] = h.ActiveTasks
		resp["started_at"] = h.StartedAt.Format(time.RFC3339)
		resp["last_heartbeat"] = h.LastHeartbeat.Format(time.RFC3339)
		resp["last_error"] = h.LastError
		if !h.StartedAt.IsZero() {
			resp["uptime_secs"] = int(h.Uptime().Seconds())
		}
		if h.IsStale(60 * time.Second) {
			resp["heartbeat_stale"] = true
		}
	}
	respondOK(w, resp)
}

// handleServiceInstall installs the service.
func (s *Server) handleServiceInstall(w http.ResponseWriter, r *http.Request) {
	out, err := s.runServiceCLI("install")
	if err != nil {
		respondError(w, http.StatusInternalServerError, "install_failed", err.Error()+": "+out)
		return
	}
	respondOK(w, map[string]any{"ok": true, "output": out})
}

// handleServiceUninstall uninstalls the service.
func (s *Server) handleServiceUninstall(w http.ResponseWriter, r *http.Request) {
	out, err := s.runServiceCLI("uninstall")
	if err != nil {
		respondError(w, http.StatusInternalServerError, "uninstall_failed", err.Error()+": "+out)
		return
	}
	respondOK(w, map[string]any{"ok": true, "output": out})
}

// handleServiceStart starts the service.
func (s *Server) handleServiceStart(w http.ResponseWriter, r *http.Request) {
	out, err := s.runServiceCLI("start")
	if err != nil {
		respondError(w, http.StatusInternalServerError, "start_failed", err.Error()+": "+out)
		return
	}
	respondOK(w, map[string]any{"ok": true, "output": out})
}

// handleServiceStop stops the service.
func (s *Server) handleServiceStop(w http.ResponseWriter, r *http.Request) {
	out, err := s.runServiceCLI("stop")
	if err != nil {
		respondError(w, http.StatusInternalServerError, "stop_failed", err.Error()+": "+out)
		return
	}
	respondOK(w, map[string]any{"ok": true, "output": out})
}

// handleServiceRestart restarts the service.
func (s *Server) handleServiceRestart(w http.ResponseWriter, r *http.Request) {
	out, err := s.runServiceCLI("restart")
	if err != nil {
		respondError(w, http.StatusInternalServerError, "restart_failed", err.Error()+": "+out)
		return
	}
	respondOK(w, map[string]any{"ok": true, "output": out})
}

// ownExecutable returns the absolute path to the running binary.
func ownExecutable() (string, error) {
	exe, err := ownExe()
	if err != nil {
		return "", err
	}
	return exe, nil
}
