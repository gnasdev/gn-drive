//go:build darwin

package service

import (
	"errors"
	"fmt"
	"os"
	"path/filepath"
	"strings"
	"testing"
)

func TestRenderLaunchdPlist(t *testing.T) {
	home, _ := os.UserHomeDir()
	spec := Spec{
		Name:        "test-svc",
		ExecPath:    "/usr/local/bin/gn-drive",
		DisplayName: "Test Service",
		ConfigDir:   filepath.Join(home, ".config", "gn-drive"),
	}
	plist, err := renderLaunchdPlist(spec)
	if err != nil {
		t.Fatal(err)
	}
	// Label must be reverse-DNS, never Spec.Name ("gn-drive" fails bootstrap).
	if !strings.Contains(plist, defaultLabel) {
		t.Errorf("plist should contain reverse-DNS label %q", defaultLabel)
	}
	if strings.Contains(plist, "<string>test-svc</string>") {
		t.Error("plist Label must not use Spec.Name")
	}
	if !strings.Contains(plist, "/usr/local/bin/gn-drive") {
		t.Error("plist should contain ExecPath")
	}
	if !strings.Contains(plist, home) {
		t.Error("plist should contain HOME")
	}
	if !strings.Contains(plist, "<string>/usr/local/bin/gn-drive</string>") {
		t.Error("plist should contain binary path in <string>")
	}
	if !strings.Contains(plist, "<plist version=\"1.0\">") {
		t.Error("plist should have plist root")
	}
	if !strings.Contains(plist, "<key>RunAtLoad</key>") {
		t.Error("plist should have RunAtLoad key")
	}
	// WorkingDirectory should prefer ConfigDir, not binary dir.
	if !strings.Contains(plist, spec.ConfigDir) {
		t.Error("plist WorkingDirectory should use ConfigDir")
	}
}

func TestRenderLaunchdPlist_DefaultLabel(t *testing.T) {
	spec := Spec{ExecPath: "/bin/test", Name: "gn-drive"}
	plist, err := renderLaunchdPlist(spec)
	if err != nil {
		t.Fatal(err)
	}
	if !strings.Contains(plist, defaultLabel) {
		t.Errorf("plist should use default label %q", defaultLabel)
	}
	if strings.Contains(plist, "<string>gn-drive</string>") {
		t.Error("Label gn-drive is rejected by launchd bootstrap (exit 5)")
	}
}

func TestLaunchdManager_plistPath(t *testing.T) {
	m := &LaunchdManager{}
	home, _ := os.UserHomeDir()
	p, err := m.plistPath(Spec{Name: "x", Scope: ScopeUser})
	if err != nil {
		t.Fatal(err)
	}
	want := filepath.Join(home, "Library/LaunchAgents", defaultLabel+".plist")
	if p != want {
		t.Errorf("user plistPath = %q, want %q", p, want)
	}
	p, _ = m.plistPath(Spec{Name: "x", Scope: ScopeSystem})
	want = filepath.Join("/", "Library/LaunchDaemons", defaultLabel+".plist")
	if p != want {
		t.Errorf("system plistPath = %q, want %q", p, want)
	}
}

func TestLaunchdManager_label(t *testing.T) {
	m := &LaunchdManager{}
	// Spec.Name must not become the launchd Label.
	if got := m.label(Spec{Name: "gn-drive"}); got != defaultLabel {
		t.Errorf("label = %q, want %q", got, defaultLabel)
	}
	if got := m.label(Spec{}); got != defaultLabel {
		t.Errorf("default label = %q, want %q", got, defaultLabel)
	}
}

func TestLaunchdManager_IsInstalled(t *testing.T) {
	m := &LaunchdManager{}
	dir := t.TempDir()
	// Override HOME to use temp dir.
	t.Setenv("HOME", dir)
	// Plist does not exist.
	installed, err := m.IsInstalled(Spec{Name: "missing", Scope: ScopeUser})
	if err != nil {
		t.Fatal(err)
	}
	if installed {
		t.Error("expected not installed")
	}
	// Create the plist file at the fixed reverse-DNS path.
	plistDir := filepath.Join(dir, "Library/LaunchAgents")
	if err := os.MkdirAll(plistDir, 0o755); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(plistDir, defaultLabel+".plist"), []byte("<plist/>"), 0o644); err != nil {
		t.Fatal(err)
	}
	installed, err = m.IsInstalled(Spec{Name: "exists", Scope: ScopeUser})
	if err != nil {
		t.Fatal(err)
	}
	if !installed {
		t.Error("expected installed")
	}
}

func TestLaunchdManager_IsInstalled_PlistPathError(t *testing.T) {
	m := &LaunchdManager{}
	// System scope uses absolute path /Library/LaunchDaemons — stat will
	// either succeed (root) or return not-exist (other). Either way, the
	// function should not panic. We can't easily inject a real error
	// without a bad HOME, so this is a smoke test.
	_, err := m.IsInstalled(Spec{Name: "x", Scope: ScopeSystem})
	if err != nil {
		t.Logf("IsInstalled(system): %v", err)
	}
}

func TestLaunchdManager_domainTarget(t *testing.T) {
	m := &LaunchdManager{}
	user := m.domainTarget(Spec{Scope: ScopeUser})
	if !strings.HasPrefix(user, "gui/") {
		t.Errorf("user domain = %q, want gui/UID", user)
	}
	if m.domainTarget(Spec{Scope: ScopeSystem}) != "system" {
		t.Errorf("system domain = %q, want system", m.domainTarget(Spec{Scope: ScopeSystem}))
	}
}

func TestLaunchdManager_Install_ReplacesExisting(t *testing.T) {
	// Reinstall is allowed: bootout + rewrite + bootstrap (idempotent upgrade path).
	orig := runLaunchctl
	defer func() { runLaunchctl = orig }()
	runLaunchctl = func(args ...string) error { return nil }

	m := &LaunchdManager{}
	dir := t.TempDir()
	t.Setenv("HOME", dir)
	plistDir := filepath.Join(dir, "Library/LaunchAgents")
	if err := os.MkdirAll(plistDir, 0o755); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(plistDir, defaultLabel+".plist"), []byte("<plist/>"), 0o644); err != nil {
		t.Fatal(err)
	}
	if err := m.Install(Spec{Name: "x", ExecPath: "/bin/true", Scope: ScopeUser}); err != nil {
		t.Fatalf("reinstall should succeed: %v", err)
	}
}

func TestLaunchdManager_Uninstall_NotInstalled(t *testing.T) {
	m := &LaunchdManager{}
	dir := t.TempDir()
	t.Setenv("HOME", dir)
	// Should be idempotent (no-op when not installed).
	if err := m.Uninstall(Spec{Name: "missing", Scope: ScopeUser}); err != nil {
		t.Errorf("Uninstall when not installed: %v", err)
	}
}

func TestRenderLaunchdPlist_LogDirCreate(t *testing.T) {
	// The renderLaunchdPlist function creates ~/Library/Logs/GNDrive.
	// We just verify it runs without error and produces a valid plist.
	spec := Spec{Name: "x", ExecPath: "/bin/true"}
	plist, err := renderLaunchdPlist(spec)
	if err != nil {
		t.Fatal(err)
	}
	if !strings.Contains(plist, "<?xml") {
		t.Error("plist should be valid XML")
	}
	if !strings.Contains(plist, "<key>Label</key>") {
		t.Error("plist should have Label key")
	}
}

// --- Tests using mocked runLaunchctl / runLaunchctlOutput ---

func TestLaunchdManager_Start_Mocked(t *testing.T) {
	orig := runLaunchctl
	defer func() { runLaunchctl = orig }()
	var got []string
	runLaunchctl = func(args ...string) error {
		got = args
		return nil
	}
	m := &LaunchdManager{}
	if err := m.Start(Spec{Name: "x", Scope: ScopeUser}); err != nil {
		t.Fatal(err)
	}
	if len(got) < 2 || got[0] != "kickstart" {
		t.Errorf("expected kickstart command, got %v", got)
	}
}

func TestLaunchdManager_Stop_Mocked(t *testing.T) {
	orig := runLaunchctl
	defer func() { runLaunchctl = orig }()
	var got []string
	runLaunchctl = func(args ...string) error {
		got = args
		return nil
	}
	m := &LaunchdManager{}
	if err := m.Stop(Spec{Name: "x", Scope: ScopeUser}); err != nil {
		t.Fatal(err)
	}
	if got[0] != "kill" {
		t.Errorf("expected kill command, got %v", got)
	}
}

func TestLaunchdManager_Restart_Mocked(t *testing.T) {
	orig := runLaunchctl
	defer func() { runLaunchctl = orig }()
	calls := 0
	runLaunchctl = func(args ...string) error {
		calls++
		return nil
	}
	m := &LaunchdManager{}
	if err := m.Restart(Spec{Name: "x", Scope: ScopeUser}); err != nil {
		t.Fatal(err)
	}
	// Restart = Stop + Start
	if calls != 2 {
		t.Errorf("expected 2 launchctl calls (stop+start), got %d", calls)
	}
}

func TestLaunchdManager_Status_MockedRunning(t *testing.T) {
	orig := runLaunchctlOutput
	defer func() { runLaunchctlOutput = orig }()
	runLaunchctlOutput = func(args ...string) ([]byte, error) {
		return []byte("state = running\n\tpid = 1234\n"), nil
	}
	m := &LaunchdManager{}
	st, err := m.Status(Spec{Name: "x", Scope: ScopeUser})
	if err != nil {
		t.Fatal(err)
	}
	if !st.Running {
		t.Error("expected Running=true")
	}
	if st.PID != 1234 {
		t.Errorf("PID = %d, want 1234", st.PID)
	}
}

func TestLaunchdManager_Status_MockedNotRunning(t *testing.T) {
	orig := runLaunchctlOutput
	defer func() { runLaunchctlOutput = orig }()
	runLaunchctlOutput = func(args ...string) ([]byte, error) {
		return []byte("state = waiting\n"), nil
	}
	m := &LaunchdManager{}
	st, err := m.Status(Spec{Name: "x", Scope: ScopeUser})
	if err != nil {
		t.Fatal(err)
	}
	if st.Running {
		t.Error("expected Running=false")
	}
}

func TestLaunchdManager_Status_MockedError(t *testing.T) {
	orig := runLaunchctlOutput
	defer func() { runLaunchctlOutput = orig }()
	runLaunchctlOutput = func(args ...string) ([]byte, error) {
		return nil, fmt.Errorf("not loaded")
	}
	m := &LaunchdManager{}
	st, err := m.Status(Spec{Name: "x", Scope: ScopeUser})
	if err != nil {
		t.Fatal(err)
	}
	// Errors from launchctl print are non-fatal for Status; we just return
	// the default (Installed=true, Running=false, PID=0).
	if st.PID != 0 {
		t.Errorf("PID = %d, want 0", st.PID)
	}
}

func TestLaunchdManager_Install_Mocked(t *testing.T) {
	orig := runLaunchctl
	defer func() { runLaunchctl = orig }()
	calls := 0
	runLaunchctl = func(args ...string) error {
		calls++
		return nil
	}
	m := &LaunchdManager{}
	dir := t.TempDir()
	t.Setenv("HOME", dir)
	if err := m.Install(Spec{Name: "fresh", Scope: ScopeUser}); err != nil {
		t.Fatal(err)
	}
	// bootstrap + enable
	if calls != 2 {
		t.Errorf("expected 2 launchctl calls, got %d", calls)
	}
}

func TestLaunchdManager_Install_MockedBootstrapFails(t *testing.T) {
	orig := runLaunchctl
	defer func() { runLaunchctl = orig }()
	runLaunchctl = func(args ...string) error {
		return fmt.Errorf("permission denied")
	}
	m := &LaunchdManager{}
	dir := t.TempDir()
	t.Setenv("HOME", dir)
	err := m.Install(Spec{Name: "fresh", Scope: ScopeUser})
	if err == nil {
		t.Error("expected error from bootstrap failure")
	}
	if !strings.Contains(err.Error(), "bootstrap") {
		t.Errorf("expected bootstrap error, got %v", err)
	}
}

func TestLaunchdManager_Uninstall_Mocked(t *testing.T) {
	orig := runLaunchctl
	defer func() { runLaunchctl = orig }()
	calls := 0
	runLaunchctl = func(args ...string) error {
		calls++
		return nil
	}
	m := &LaunchdManager{}
	dir := t.TempDir()
	t.Setenv("HOME", dir)
	plistDir := filepath.Join(dir, "Library/LaunchAgents")
	if err := os.MkdirAll(plistDir, 0o755); err != nil {
		t.Fatal(err)
	}
	plistPath := filepath.Join(plistDir, defaultLabel+".plist")
	if err := os.WriteFile(plistPath, []byte("<plist/>"), 0o644); err != nil {
		t.Fatal(err)
	}
	if err := m.Uninstall(Spec{Name: "x", Scope: ScopeUser}); err != nil {
		t.Fatal(err)
	}
	// bootout (domain/label + domain path) + disable = at least 2
	if calls < 2 {
		t.Errorf("expected >=2 launchctl calls, got %d", calls)
	}
	if _, err := os.Stat(plistPath); !os.IsNotExist(err) {
		t.Error("plist should be removed after uninstall")
	}
}

// TestRenderLaunchdPlist_HomeError covers the os.UserHomeDir error branch
// in renderLaunchdPlist by unsetting HOME.
func TestRenderLaunchdPlist_HomeError(t *testing.T) {
	t.Setenv("HOME", "")
	_, err := renderLaunchdPlist(Spec{Name: "x", ExecPath: "/bin/true"})
	if err == nil {
		t.Error("expected error when HOME is unset")
	}
}

// TestRenderLaunchdPlist_LogDirError covers the MkdirAll logDir error branch
// in renderLaunchdPlist by setting HOME to a path under a regular file.
func TestRenderLaunchdPlist_LogDirError(t *testing.T) {
	dir := t.TempDir()
	blocker := filepath.Join(dir, "blocker")
	if err := os.WriteFile(blocker, []byte("hi"), 0o600); err != nil {
		t.Fatal(err)
	}
	t.Setenv("HOME", blocker)
	_, err := renderLaunchdPlist(Spec{Name: "x", ExecPath: "/bin/true"})
	if err == nil {
		t.Error("expected error from MkdirAll when HOME is under a file")
	}
}

// TestPlistPath_HomeError covers the os.UserHomeDir error branch in
// plistPath by unsetting HOME.
func TestPlistPath_HomeError(t *testing.T) {
	m := &LaunchdManager{}
	t.Setenv("HOME", "")
	_, err := m.plistPath(Spec{Scope: ScopeUser})
	if err == nil {
		t.Error("expected error when HOME is unset")
	}
}

// TestInstall_PlistPathError covers the plistPath error branch in Install
// by unsetting HOME.
func TestInstall_PlistPathError(t *testing.T) {
	m := &LaunchdManager{}
	t.Setenv("HOME", "")
	err := m.Install(Spec{Scope: ScopeUser})
	if err == nil {
		t.Error("expected error from Install when plistPath fails")
	}
}

// TestInstall_MkdirPlistError is not constructible: IsInstalled and
// MkdirAll both traverse the same path. If MkdirAll fails (parent is a
// file), IsInstalled's os.Stat also fails with non-ENOENT, returning
// (false, err) before Install reaches MkdirAll. So this branch is
// untestable in this combination.
func TestInstall_MkdirPlistError(t *testing.T) {
	t.Skip("IsInstalled fails before MkdirAll when parent is a file")
}

// TestIsInstalled_OsStatError covers the os.Stat error branch in IsInstalled
// by setting HOME to a path that makes the plist path under a non-existent
// dir. The os.Stat returns ENOENT, which is handled, but we want a non-ENOENT
// error. Use a path with a parent that is a file.
func TestIsInstalled_OsStatError(t *testing.T) {
	dir := t.TempDir()
	blocker := filepath.Join(dir, "blocker")
	if err := os.WriteFile(blocker, []byte("hi"), 0o600); err != nil {
		t.Fatal(err)
	}
	m := &LaunchdManager{}
	t.Setenv("HOME", filepath.Join(blocker, "Library")) // Library under a file
	_, err := m.IsInstalled(Spec{Scope: ScopeUser})
	if err == nil {
		t.Error("expected error from IsInstalled when plist path is under a file")
	}
}

// TestUninstall_RemoveError covers the os.Remove error branch in Uninstall.
func TestUninstall_RemoveError(t *testing.T) {
	dir := t.TempDir()
	// Create a directory (with contents) where Uninstall will try to remove a
	// regular file. os.Remove fails on non-empty directories.
	plistPath := filepath.Join(dir, "Library", "LaunchAgents", defaultLabel+".plist")
	if err := os.MkdirAll(plistPath, 0o755); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(plistPath, "child"), []byte("hi"), 0o600); err != nil {
		t.Fatal(err)
	}
	m := &LaunchdManager{}
	t.Setenv("HOME", dir)
	err := m.Uninstall(Spec{Scope: ScopeUser, Name: "x"})
	if err == nil {
		t.Error("expected error from Uninstall when Remove fails (non-empty dir at plist path)")
	}
}

// TestUninstall_IsInstalledError covers the IsInstalled error branch in
// Uninstall by unsetting HOME so plistPath fails.
func TestUninstall_IsInstalledError(t *testing.T) {
	m := &LaunchdManager{}
	t.Setenv("HOME", "")
	err := m.Uninstall(Spec{Scope: ScopeUser})
	if err == nil {
		t.Error("expected error from Uninstall when IsInstalled fails")
	}
}

// TestInstall_WriteFileError covers the WriteFile error branch in Install
// by making the LaunchAgents directory read-only. IsInstalled returns
// (false, nil) because no plist exists; MkdirAll succeeds (dir exists);
// WriteFile then fails on the read-only directory.
func TestInstall_WriteFileError(t *testing.T) {
	dir := t.TempDir()
	plistDir := filepath.Join(dir, "Library", "LaunchAgents")
	if err := os.MkdirAll(plistDir, 0o755); err != nil {
		t.Fatal(err)
	}
	// Make the LaunchAgents dir read-only so WriteFile fails.
	if err := os.Chmod(plistDir, 0o500); err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() { _ = os.Chmod(plistDir, 0o755) })

	m := &LaunchdManager{}
	t.Setenv("HOME", dir)
	err := m.Install(Spec{Scope: ScopeUser, Name: "y"})
	if err == nil {
		t.Error("expected error from Install when WriteFile fails (read-only dir)")
	}
}

// TestInstall_RenderPlistError covers the renderLaunchdPlist error branch
// in Install. We make HOME/Library/LaunchAgents a real directory so
// IsInstalled succeeds, but block the log dir creation inside
// renderLaunchdPlist by making HOME/Library a directory but with Logs as
// a regular file.
func TestInstall_RenderPlistError(t *testing.T) {
	dir := t.TempDir()
	t.Setenv("HOME", dir)
	// Create HOME/Library/LaunchAgents so plistPath's parent is valid
	// (IsInstalled then returns (false, nil)).
	if err := os.MkdirAll(filepath.Join(dir, "Library", "LaunchAgents"), 0o755); err != nil {
		t.Fatal(err)
	}
	// Block HOME/Library/Logs by making it a regular file.
	if err := os.WriteFile(filepath.Join(dir, "Library", "Logs"), []byte("not a dir"), 0o644); err != nil {
		t.Fatal(err)
	}

	m := &LaunchdManager{}
	err := m.Install(Spec{Name: "x", ExecPath: "/bin/true", Scope: ScopeUser})
	if err == nil {
		t.Error("expected error from Install when renderLaunchdPlist fails")
	}
}

// TestInstall_PlistPathError_Extra covers the plistPath error branch in
// Install when HOME is empty.
func TestInstall_PlistPathError_Extra(t *testing.T) {
	t.Setenv("HOME", "")
	m := &LaunchdManager{}
	err := m.Install(Spec{Name: "x", ExecPath: "/bin/true", Scope: ScopeUser})
	if err == nil {
		t.Error("expected error from Install when plistPath fails")
	}
}

// TestInstall_PlistPathError_InInstall covers the plistPath error branch
// in Install (line 121) by overriding installPlistPathFn to return an
// error.
func TestInstall_PlistPathError_InInstall(t *testing.T) {
	dir := t.TempDir()
	t.Setenv("HOME", dir)
	if err := os.MkdirAll(filepath.Join(dir, "Library", "LaunchAgents"), 0o755); err != nil {
		t.Fatal(err)
	}

	orig := installPlistPathFn
	t.Cleanup(func() { installPlistPathFn = orig })
	installPlistPathFn = func(m *LaunchdManager, spec Spec) (string, error) {
		return "", errors.New("simulated plistPath failure")
	}

	m := &LaunchdManager{}
	err := m.Install(Spec{Name: "x", ExecPath: "/bin/true", Scope: ScopeUser})
	if err == nil {
		t.Error("expected error from Install when plistPath fails")
	}
}

// TestInstall_MkdirPlistError_Inject covers the MkdirAll error branch in
// Install by overriding installMkdirAllFn.
func TestInstall_MkdirPlistError_Inject(t *testing.T) {
	dir := t.TempDir()
	t.Setenv("HOME", dir)
	if err := os.MkdirAll(filepath.Join(dir, "Library", "LaunchAgents"), 0o755); err != nil {
		t.Fatal(err)
	}

	orig := installMkdirAllFn
	t.Cleanup(func() { installMkdirAllFn = orig })
	installMkdirAllFn = func(path string, perm os.FileMode) error {
		return errors.New("simulated mkdir failure")
	}

	m := &LaunchdManager{}
	err := m.Install(Spec{Name: "x", ExecPath: "/bin/true", Scope: ScopeUser})
	if err == nil {
		t.Error("expected error from Install when MkdirAll fails")
	}
}
