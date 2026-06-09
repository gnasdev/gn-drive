//go:build linux

// Package service provides cross-platform service management.
package service

import (
	"bytes"
	"fmt"
	"os"
	"os/exec"
	"path/filepath"
	"strconv"
	"strings"
	"time"
)

// SystemdManager implements Manager for systemd (Linux).
// Supports both user-level (~/.config/systemd/user/) and system-level
// (/etc/systemd/system/) installations.
type SystemdManager struct{}

// unitTemplate is the systemd unit file template.
const unitTemplate = `[Unit]
Description={{DESCRIPTION}}
After=network.target

[Service]
Type=simple
ExecStart={{EXEC}} run --service
Restart=on-failure
RestartSec=10s
Environment=GN_DRIVE_MODE=service
Environment=HOME={{HOME}}
WorkingDirectory={{WORKDIR}}
{{EXTRA_ENV}}

[Install]
WantedBy={{WANTED_BY}}
`

// userUnitPath returns the path to the user-level systemd unit.
func userUnitPath() (string, error) {
	home, err := os.UserHomeDir()
	if err != nil {
		return "", err
	}
	return filepath.Join(home, ".config", "systemd", "user", "gn-drive.service"), nil
}

// systemUnitPath returns the path to the system-level systemd unit.
func systemUnitPath() string {
	return "/etc/systemd/system/gn-drive.service"
}

func (m *SystemdManager) unitPath(spec Spec) string {
	if spec.Scope == ScopeSystem {
		return systemUnitPath()
	}
	p, _ := userUnitPath()
	return p
}

// IsInstalled returns true if the unit file exists.
func (m *SystemdManager) IsInstalled(spec Spec) (bool, error) {
	_, err := os.Stat(m.unitPath(spec))
	if err == nil {
		return true, nil
	}
	if os.IsNotExist(err) {
		return false, nil
	}
	return false, err
}

func (m *SystemdManager) Install(spec Spec) error {
	installed, err := m.IsInstalled(spec)
	if err != nil {
		return err
	}
	if installed {
		return fmt.Errorf("%w: %s", ErrAlreadyInstalled, m.unitPath(spec))
	}

	unit, err := renderSystemdUnit(spec)
	if err != nil {
		return err
	}

	// Ensure parent directory exists.
	if err := os.MkdirAll(filepath.Dir(m.unitPath(spec)), 0o755); err != nil {
		return fmt.Errorf("mkdir unit dir: %w", err)
	}

	// Write unit file.
	if err := os.WriteFile(m.unitPath(spec), []byte(unit), 0o644); err != nil {
		return fmt.Errorf("write unit: %w", err)
	}

	// Reload + enable.
	if err := m.runSystemctl(spec, "daemon-reload"); err != nil {
		return fmt.Errorf("daemon-reload: %w", err)
	}
	if err := m.runSystemctl(spec, "enable", "gn-drive.service"); err != nil {
		return fmt.Errorf("enable: %w", err)
	}

	return nil
}

func (m *SystemdManager) Uninstall(spec Spec) error {
	installed, err := m.IsInstalled(spec)
	if err != nil {
		return err
	}
	if !installed {
		return nil // idempotent
	}
	// Stop first.
	_ = m.runSystemctl(spec, "stop", "gn-drive.service")
	// Disable.
	_ = m.runSystemctl(spec, "disable", "gn-drive.service")
	// Remove unit file.
	if err := os.Remove(m.unitPath(spec)); err != nil {
		return fmt.Errorf("remove unit: %w", err)
	}
	// Reload.
	_ = m.runSystemctl(spec, "daemon-reload")
	return nil
}

func (m *SystemdManager) Start(spec Spec) error {
	return m.runSystemctl(spec, "start", "gn-drive.service")
}

func (m *SystemdManager) Stop(spec Spec) error {
	return m.runSystemctl(spec, "stop", "gn-drive.service")
}

func (m *SystemdManager) Restart(spec Spec) error {
	return m.runSystemctl(spec, "restart", "gn-drive.service")
}

func (m *SystemdManager) Status(spec Spec) (Status, error) {
	st := Status{Installed: true, Mode: "service", Scope: string(spec.Scope)}

	// Check if running via systemctl is-active.
	out, err := m.runSystemctlOutput(spec, "is-active", "gn-drive.service")
	if err == nil {
		st.Running = strings.TrimSpace(string(out)) == "active"
	}

	// Get PID.
	if st.Running {
		pidOut, err := m.runSystemctlOutput(spec, "show", "gn-drive.service", "--property=MainPID", "--value")
		if err == nil {
			pid, _ := strconv.Atoi(strings.TrimSpace(string(pidOut)))
			st.PID = pid
		}
	}

	return st, nil
}

// runSystemctl runs a systemctl command and returns its error.
func (m *SystemdManager) runSystemctl(spec Spec, args ...string) error {
	cmd := m.systemctlCmd(spec, args...)
	var stderr bytes.Buffer
	cmd.Stderr = &stderr
	if err := cmd.Run(); err != nil {
		return fmt.Errorf("systemctl %s: %w (%s)", strings.Join(args, " "), err, strings.TrimSpace(stderr.String()))
	}
	return nil
}

func (m *SystemdManager) runSystemctlOutput(spec Spec, args ...string) ([]byte, error) {
	cmd := m.systemctlCmd(spec, args...)
	var stderr bytes.Buffer
	cmd.Stderr = &stderr
	out, err := cmd.Output()
	if err != nil {
		return out, fmt.Errorf("systemctl %s: %w (%s)", strings.Join(args, " "), err, strings.TrimSpace(stderr.String()))
	}
	return out, nil
}

func (m *SystemdManager) systemctlCmd(spec Spec, args ...string) *exec.Cmd {
	if spec.Scope == ScopeSystem {
		return exec.Command("systemctl", args...)
	}
	// User-level: prepend --user.
	full := append([]string{"--user"}, args...)
	return exec.Command("systemctl", full...)
}

func renderSystemdUnit(spec Spec) (string, error) {
	home, err := os.UserHomeDir()
	if err != nil {
		return "", err
	}
	workdir := filepath.Dir(spec.ExecPath)

	var extraEnv strings.Builder
	for _, e := range spec.Env {
		if strings.HasPrefix(e, "GN_DRIVE_MODE=") || strings.HasPrefix(e, "HOME=") {
			continue
		}
		extraEnv.WriteString("Environment=")
		extraEnv.WriteString(e)
		extraEnv.WriteString("\n")
	}

	wantedBy := "default.target"
	if spec.Scope == ScopeSystem {
		wantedBy = "multi-user.target"
	}

	r := strings.NewReplacer(
		"{{DESCRIPTION}}", QuoteValue(spec.Description),
		"{{EXEC}}", QuoteValue(spec.ExecPath),
		"{{HOME}}", home,
		"{{WORKDIR}}", workdir,
		"{{EXTRA_ENV}}", extraEnv.String(),
		"{{WANTED_BY}}", wantedBy,
	)
	return r.Replace(unitTemplate), nil
}

var _ = time.Now
