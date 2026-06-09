//go:build windows

// Package service provides cross-platform service management.
package service

import (
	"bytes"
	"fmt"
	"os/exec"
	"strings"
)

// SCMManager implements Manager for Windows Service Control Manager (SCM).
// Uses `sc.exe` to create/delete/start/stop/query the service.
//
// Note: Windows service install typically requires elevated (admin) privileges.
// The binary is registered with binPath pointing to "gn-drive.exe run --service".
type SCMManager struct{}

const scmServiceName = "gn-drive"

func (m *SCMManager) IsInstalled(spec Spec) (bool, error) {
	cmd := exec.Command("sc", "query", scmServiceName)
	var stderr bytes.Buffer
	cmd.Stderr = &stderr
	err := cmd.Run()
	if err == nil {
		return true, nil
	}
	// sc query returns non-zero when service not found.
	if strings.Contains(stderr.String(), "does not exist") || strings.Contains(stderr.String(), "1060") {
		return false, nil
	}
	return false, fmt.Errorf("sc query: %w (%s)", err, strings.TrimSpace(stderr.String()))
}

func (m *SCMManager) Install(spec Spec) error {
	installed, err := m.IsInstalled(spec)
	if err != nil {
		return err
	}
	if installed {
		return fmt.Errorf("%w: %s", ErrAlreadyInstalled, scmServiceName)
	}

	binPath := fmt.Sprintf(`"%s" run --service`, spec.ExecPath)
	cmd := exec.Command("sc", "create", scmServiceName,
		"binPath=", binPath,
		"start=", "auto",
		"displayname=", spec.DisplayName)
	var stderr bytes.Buffer
	cmd.Stderr = &stderr
	if err := cmd.Run(); err != nil {
		return fmt.Errorf("sc create: %w (%s)", err, strings.TrimSpace(stderr.String()))
	}
	// Add description.
	_ = exec.Command("sc", "description", scmServiceName, spec.Description).Run()
	return nil
}

func (m *SCMManager) Uninstall(spec Spec) error {
	installed, err := m.IsInstalled(spec)
	if err != nil {
		return err
	}
	if !installed {
		return nil // idempotent
	}
	_ = m.Stop(spec)
	cmd := exec.Command("sc", "delete", scmServiceName)
	var stderr bytes.Buffer
	cmd.Stderr = &stderr
	if err := cmd.Run(); err != nil {
		return fmt.Errorf("sc delete: %w (%s)", err, strings.TrimSpace(stderr.String()))
	}
	return nil
}

func (m *SCMManager) Start(spec Spec) error {
	cmd := exec.Command("sc", "start", scmServiceName)
	var stderr bytes.Buffer
	cmd.Stderr = &stderr
	if err := cmd.Run(); err != nil {
		return fmt.Errorf("sc start: %w (%s)", err, strings.TrimSpace(stderr.String()))
	}
	return nil
}

func (m *SCMManager) Stop(spec Spec) error {
	cmd := exec.Command("sc", "stop", scmServiceName)
	var stderr bytes.Buffer
	cmd.Stderr = &stderr
	if err := cmd.Run(); err != nil {
		return fmt.Errorf("sc stop: %w (%s)", err, strings.TrimSpace(stderr.String()))
	}
	return nil
}

func (m *SCMManager) Restart(spec Spec) error {
	if err := m.Stop(spec); err != nil {
		// ignore
	}
	return m.Start(spec)
}

func (m *SCMManager) Status(spec Spec) (Status, error) {
	st := Status{Installed: true, Mode: "service", Scope: string(spec.Scope)}
	out, err := exec.Command("sc", "query", scmServiceName).Output()
	if err != nil {
		return st, fmt.Errorf("sc query: %w", err)
	}
	for _, line := range strings.Split(string(out), "\n") {
		line = strings.TrimSpace(line)
		if strings.HasPrefix(line, "STATE") {
			st.Running = strings.Contains(line, "RUNNING")
		}
		if strings.HasPrefix(line, "PID") {
			parts := strings.Fields(line)
			if len(parts) >= 3 {
				_, _ = fmt.Sscanf(parts[2], "%d", &st.PID)
			}
		}
	}
	return st, nil
}
