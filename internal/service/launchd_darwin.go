//go:build darwin

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
)

// LaunchdManager implements Manager for launchd (macOS).
// User-level: ~/Library/LaunchAgents/<label>.plist (loaded by launchctl bootstrap gui/UID).
// System-level: /Library/LaunchDaemons/<label>.plist (loaded by launchctl bootstrap system).
type LaunchdManager struct{}

// plistTemplate is the launchd plist template.
const plistTemplate = `<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>Label</key>
  <string>{{LABEL}}</string>
  <key>ProgramArguments</key>
  <array>
    <string>{{EXEC}}</string>
    <string>run</string>
    <string>--service</string>
  </array>
  <key>RunAtLoad</key>
  <true/>
  <key>KeepAlive</key>
  <dict>
    <key>SuccessfulExit</key>
    <false/>
    <key>Crashed</key>
    <true/>
  </dict>
  <key>ThrottleInterval</key>
  <integer>10</integer>
  <key>EnvironmentVariables</key>
  <dict>
    <key>GN_DRIVE_MODE</key>
    <string>service</string>
    <key>HOME</key>
    <string>{{HOME}}</string>
  </dict>
  <key>WorkingDirectory</key>
  <string>{{WORKDIR}}</string>
  <key>StandardOutPath</key>
  <string>{{LOG_DIR}}/gn-drive.out.log</string>
  <key>StandardErrorPath</key>
  <string>{{LOG_DIR}}/gn-drive.err.log</string>
</dict>
</plist>
`

const (
	userPlistDir   = "Library/LaunchAgents"
	systemPlistDir = "Library/LaunchDaemons"
	defaultLabel   = "com.gndrive.app"
)

func (m *LaunchdManager) plistPath(spec Spec) (string, error) {
	label := defaultLabel
	if spec.Name != "" {
		label = spec.Name
	}
	if spec.Scope == ScopeSystem {
		return filepath.Join("/", systemPlistDir, label+".plist"), nil
	}
	home, err := os.UserHomeDir()
	if err != nil {
		return "", err
	}
	return filepath.Join(home, userPlistDir, label+".plist"), nil
}

func (m *LaunchdManager) label(spec Spec) string {
	if spec.Name != "" {
		return spec.Name
	}
	// Fall back to a stable label derived from the binary basename.
	return defaultLabel
}

func (m *LaunchdManager) IsInstalled(spec Spec) (bool, error) {
	p, err := m.plistPath(spec)
	if err != nil {
		return false, err
	}
	_, err = os.Stat(p)
	if err == nil {
		return true, nil
	}
	if os.IsNotExist(err) {
		return false, nil
	}
	return false, err
}

// installPlistPathFn is overridable for tests; defaults to (*LaunchdManager).plistPath.
var installPlistPathFn = func(m *LaunchdManager, spec Spec) (string, error) {
	return m.plistPath(spec)
}

// installMkdirAllFn is overridable for tests; defaults to os.MkdirAll.
var installMkdirAllFn = os.MkdirAll

func (m *LaunchdManager) Install(spec Spec) error {
	installed, err := m.IsInstalled(spec)
	if err != nil {
		return err
	}
	if installed {
		return fmt.Errorf("%w: %s", ErrAlreadyInstalled, m.plistPathFor(spec))
	}

	plist, err := renderLaunchdPlist(spec)
	if err != nil {
		return err
	}

	plistPath, err := installPlistPathFn(m, spec)
	if err != nil {
		return err
	}

	if err := installMkdirAllFn(filepath.Dir(plistPath), 0o755); err != nil {
		return fmt.Errorf("mkdir plist dir: %w", err)
	}

	if err := os.WriteFile(plistPath, []byte(plist), 0o644); err != nil {
		return fmt.Errorf("write plist: %w", err)
	}

	// Bootstrap the service. System-level requires sudo (caller's responsibility).
	domain := m.domainTarget(spec)
	if err := runLaunchctl("bootstrap", domain, plistPath); err != nil {
		// Plist was written; bootstrap failed. Don't remove the plist — the user
		// can run `launchctl bootstrap` manually or fix permissions and retry.
		return fmt.Errorf("launchctl bootstrap: %w\nplist written to: %s", err, plistPath)
	}

	// Enable so it auto-starts on next login/boot.
	_ = runLaunchctl("enable", domain+"/"+m.label(spec))

	return nil
}

func (m *LaunchdManager) Uninstall(spec Spec) error {
	installed, err := m.IsInstalled(spec)
	if err != nil {
		return err
	}
	if !installed {
		return nil // idempotent
	}

	domain := m.domainTarget(spec)
	label := m.label(spec)

	// Bootout (unload) the service.
	_ = runLaunchctl("bootout", domain+"/"+label)
	// Disable.
	_ = runLaunchctl("disable", domain+"/"+label)

	plistPath, _ := m.plistPath(spec)
	if err := os.Remove(plistPath); err != nil {
		return fmt.Errorf("remove plist: %w", err)
	}
	return nil
}

func (m *LaunchdManager) Start(spec Spec) error {
	domain := m.domainTarget(spec)
	return runLaunchctl("kickstart", "-k", domain+"/"+m.label(spec))
}

func (m *LaunchdManager) Stop(spec Spec) error {
	domain := m.domainTarget(spec)
	return runLaunchctl("kill", "SIGTERM", domain+"/"+m.label(spec))
}

func (m *LaunchdManager) Restart(spec Spec) error {
	if err := m.Stop(spec); err != nil {
		// Ignore: service may not be running.
	}
	return m.Start(spec)
}

func (m *LaunchdManager) Status(spec Spec) (Status, error) {
	st := Status{Installed: true, Mode: "service", Scope: string(spec.Scope)}

	domain := m.domainTarget(spec)
	label := m.label(spec)

	// Try `launchctl print` to get detailed info.
	out, err := runLaunchctlOutput("print", domain+"/"+label)
	if err == nil {
		st.Running = strings.Contains(string(out), "state = running")
		// Extract PID.
		for _, line := range strings.Split(string(out), "\n") {
			if strings.HasPrefix(line, "\tpid = ") {
				pid, _ := strconv.Atoi(strings.TrimPrefix(line, "\tpid = "))
				st.PID = pid
				break
			}
		}
	}
	return st, nil
}

// domainTarget returns the launchd domain for the scope.
func (m *LaunchdManager) domainTarget(spec Spec) string {
	if spec.Scope == ScopeSystem {
		return "system"
	}
	// User-level: gui/UID
	uid := os.Getuid()
	return fmt.Sprintf("gui/%d", uid)
}

func (m *LaunchdManager) plistPathFor(spec Spec) string {
	p, _ := m.plistPath(spec)
	return p
}

// runLaunchctl is the testable inner of runLaunchctl. It allows tests to
// override the actual launchctl invocation.
var runLaunchctl = func(args ...string) error {
	cmd := exec.Command("launchctl", args...)
	var stderr bytes.Buffer
	cmd.Stderr = &stderr
	if err := cmd.Run(); err != nil {
		return fmt.Errorf("launchctl %s: %w (%s)", strings.Join(args, " "), err, strings.TrimSpace(stderr.String()))
	}
	return nil
}

// runLaunchctlOutput is the testable inner of runLaunchctlOutput.
var runLaunchctlOutput = func(args ...string) ([]byte, error) {
	cmd := exec.Command("launchctl", args...)
	var stderr bytes.Buffer
	cmd.Stderr = &stderr
	out, err := cmd.Output()
	if err != nil {
		return out, fmt.Errorf("launchctl %s: %w (%s)", strings.Join(args, " "), err, strings.TrimSpace(stderr.String()))
	}
	return out, nil
}

func renderLaunchdPlist(spec Spec) (string, error) {
	home, err := os.UserHomeDir()
	if err != nil {
		return "", err
	}
	workdir := filepath.Dir(spec.ExecPath)
	logDir := filepath.Join(home, "Library", "Logs", "GNDrive")
	if err := os.MkdirAll(logDir, 0o755); err != nil {
		return "", fmt.Errorf("mkdir log dir: %w", err)
	}

	label := defaultLabel
	if spec.Name != "" {
		label = spec.Name
	}

	r := strings.NewReplacer(
		"{{LABEL}}", label,
		"{{EXEC}}", spec.ExecPath,
		"{{HOME}}", home,
		"{{WORKDIR}}", workdir,
		"{{LOG_DIR}}", logDir,
	)
	return r.Replace(plistTemplate), nil
}
