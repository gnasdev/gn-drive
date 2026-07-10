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
// Label must be reverse-DNS (e.g. com.gndrive.app). A bare name like "gn-drive"
// causes `launchctl bootstrap` to fail with exit status 5 (Input/output error)
// on modern macOS.
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
	// defaultLabel is the only Label we register with launchd.
	// Do not use Spec.Name ("gn-drive") — launchd rejects non reverse-DNS labels
	// with bootstrap exit 5.
	defaultLabel = "com.gndrive.app"
	// legacyAgentName is the broken agent filename/label from earlier builds.
	legacyAgentName = "gn-drive"
)

func (m *LaunchdManager) plistPath(spec Spec) (string, error) {
	label := m.label(spec)
	if spec.Scope == ScopeSystem {
		return filepath.Join("/", systemPlistDir, label+".plist"), nil
	}
	home, err := os.UserHomeDir()
	if err != nil {
		return "", err
	}
	return filepath.Join(home, userPlistDir, label+".plist"), nil
}

// label always returns a reverse-DNS launchd Label. Spec.Name is intentionally
// ignored for launchd (it is still used on other platforms as a display/service name).
func (m *LaunchdManager) label(spec Spec) string {
	_ = spec
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
	// Drop legacy agent written with Label "gn-drive" (bootstrap always failed).
	m.cleanupLegacyAgent(spec)

	installed, err := m.IsInstalled(spec)
	if err != nil {
		return err
	}
	if installed {
		// Allow reinstall after partial failure: unload then rewrite.
		_ = m.bootout(spec)
		plistPath, _ := m.plistPath(spec)
		_ = os.Remove(plistPath)
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

	domain := m.domainTarget(spec)
	if err := runLaunchctl("bootstrap", domain, plistPath); err != nil {
		// Stale job with same label can block bootstrap; bootout and retry once.
		_ = m.bootout(spec)
		if err2 := runLaunchctl("bootstrap", domain, plistPath); err2 != nil {
			return fmt.Errorf("launchctl bootstrap: %w\nplist written to: %s", err2, plistPath)
		}
	}

	// Enable so it auto-starts on next login/boot.
	_ = runLaunchctl("enable", domain+"/"+m.label(spec))

	return nil
}

func (m *LaunchdManager) Uninstall(spec Spec) error {
	m.cleanupLegacyAgent(spec)

	installed, err := m.IsInstalled(spec)
	if err != nil {
		return err
	}
	if !installed {
		return nil // idempotent
	}

	_ = m.bootout(spec)
	_ = runLaunchctl("disable", m.domainTarget(spec)+"/"+m.label(spec))

	plistPath, _ := m.plistPath(spec)
	if err := os.Remove(plistPath); err != nil && !os.IsNotExist(err) {
		return fmt.Errorf("remove plist: %w", err)
	}
	return nil
}

func (m *LaunchdManager) bootout(spec Spec) error {
	domain := m.domainTarget(spec)
	label := m.label(spec)
	// Prefer domain/label form; also try path form for stubborn agents.
	err1 := runLaunchctl("bootout", domain+"/"+label)
	if plistPath, err := m.plistPath(spec); err == nil {
		_ = runLaunchctl("bootout", domain, plistPath)
	}
	return err1
}

// cleanupLegacyAgent removes the broken ~/Library/LaunchAgents/gn-drive.plist
// that older builds wrote with Label "gn-drive" (rejected by launchd).
func (m *LaunchdManager) cleanupLegacyAgent(spec Spec) {
	if spec.Scope == ScopeSystem {
		return
	}
	home, err := os.UserHomeDir()
	if err != nil {
		return
	}
	legacyPath := filepath.Join(home, userPlistDir, legacyAgentName+".plist")
	if _, err := os.Stat(legacyPath); err != nil {
		return
	}
	domain := m.domainTarget(spec)
	_ = runLaunchctl("bootout", domain+"/"+legacyAgentName)
	_ = runLaunchctl("bootout", domain, legacyPath)
	_ = os.Remove(legacyPath)
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
	// Prefer config dir as WorkingDirectory — binary dir may be air's tmp/.
	workdir := home
	if spec.ConfigDir != "" {
		workdir = spec.ConfigDir
	}
	logDir := filepath.Join(home, "Library", "Logs", "GNDrive")
	if err := os.MkdirAll(logDir, 0o755); err != nil {
		return "", fmt.Errorf("mkdir log dir: %w", err)
	}

	execPath := spec.ExecPath
	if execPath == "" {
		execPath, _ = os.Executable()
	}
	// Resolve symlinks so launchd does not depend on a moving air/tmp path when possible.
	if resolved, err := filepath.EvalSymlinks(execPath); err == nil && resolved != "" {
		execPath = resolved
	}

	r := strings.NewReplacer(
		"{{LABEL}}", defaultLabel,
		"{{EXEC}}", execPath,
		"{{HOME}}", home,
		"{{WORKDIR}}", workdir,
		"{{LOG_DIR}}", logDir,
	)
	return r.Replace(plistTemplate), nil
}
