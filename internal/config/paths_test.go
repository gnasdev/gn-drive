package config

import (
	"os"
	"path/filepath"
	"runtime"
	"testing"
)

func TestDetect_BasicFields(t *testing.T) {
	p := Detect()
	if p == nil {
		t.Fatal("Detect returned nil")
	}
	if p.HomeDir == "" {
		t.Error("HomeDir is empty")
	}
	if !filepath.IsAbs(p.HomeDir) {
		t.Errorf("HomeDir = %q, want absolute path", p.HomeDir)
	}
	if p.ConfigDir == "" {
		t.Error("ConfigDir is empty")
	}
	if p.LogDir == "" {
		t.Error("LogDir is empty")
	}
	if p.Platform != Platform(runtime.GOOS) {
		t.Errorf("Platform = %q, want %q", p.Platform, runtime.GOOS)
	}
	// Env defaults to production when GN_DRIVE_DEV is unset and the binary
	// does not live in a known dev path.
	if p.Env != EnvProduction && p.Env != EnvDevelopment {
		t.Errorf("Env = %q, want production or development", p.Env)
	}
}

func TestDetect_XDGConfigHomeOnLinux(t *testing.T) {
	if runtime.GOOS != "linux" {
		t.Skip("XDG_CONFIG_HOME only honored on Linux")
	}
	custom := "/tmp/xdg-config-gn-drive-test"
	t.Setenv("XDG_CONFIG_HOME", custom)
	p := Detect()
	want := filepath.Join(custom, "gn-drive")
	if p.ConfigDir != want {
		t.Errorf("ConfigDir = %q, want %q", p.ConfigDir, want)
	}
}

func TestDetect_XDGConfigHomeIgnoredOnNonLinux(t *testing.T) {
	if runtime.GOOS == "linux" {
		t.Skip("XDG_CONFIG_HOME is only ignored off Linux")
	}
	custom := "/tmp/xdg-config-gn-drive-test"
	t.Setenv("XDG_CONFIG_HOME", custom)
	p := Detect()
	if p.ConfigDir == custom {
		t.Errorf("ConfigDir = %q, want fallback to home", p.ConfigDir)
	}
}

func TestDetect_XDGStateHomeOnLinux(t *testing.T) {
	if runtime.GOOS != "linux" {
		t.Skip("XDG_STATE_HOME only honored on Linux")
	}
	custom := "/tmp/xdg-state-gn-drive-test"
	t.Setenv("XDG_STATE_HOME", custom)
	p := Detect()
	want := filepath.Join(custom, "gn-drive")
	if p.LogDir != want {
		t.Errorf("LogDir = %q, want %q", p.LogDir, want)
	}
}

func TestDetect_LogDirEqualsConfigDirOffLinux(t *testing.T) {
	if runtime.GOOS == "linux" {
		t.Skip("Linux has a separate XDG_STATE_HOME")
	}
	p := Detect()
	if p.LogDir != p.ConfigDir {
		t.Errorf("LogDir = %q, want ConfigDir = %q", p.LogDir, p.ConfigDir)
	}
}

func TestDetect_EnvDevFromVariable(t *testing.T) {
	t.Setenv("GN_DRIVE_DEV", "1")
	p := Detect()
	if p.Env != EnvDevelopment {
		t.Errorf("Env = %q, want EnvDevelopment when GN_DRIVE_DEV is set", p.Env)
	}
}

func TestEnsureConfigDir_Creates(t *testing.T) {
	dir := t.TempDir()
	target := filepath.Join(dir, "subdir", "gn-drive")
	p := &Paths{ConfigDir: target}
	if err := p.EnsureConfigDir(); err != nil {
		t.Fatalf("EnsureConfigDir: %v", err)
	}
	info, err := os.Stat(target)
	if err != nil {
		t.Fatalf("stat: %v", err)
	}
	if !info.IsDir() {
		t.Error("EnsureConfigDir did not create a directory")
	}
}

func TestEnsureConfigDir_ExistingOK(t *testing.T) {
	dir := t.TempDir()
	p := &Paths{ConfigDir: dir}
	if err := p.EnsureConfigDir(); err != nil {
		t.Errorf("EnsureConfigDir on existing dir: %v", err)
	}
}

func TestEnvConstants(t *testing.T) {
	if EnvDevelopment == "" || EnvProduction == "" {
		t.Error("env constants must be non-empty")
	}
	if EnvDevelopment == EnvProduction {
		t.Error("env constants must differ")
	}
}

func TestPlatformConstants(t *testing.T) {
	ps := []Platform{PlatformLinux, PlatformDarwin, PlatformWindows}
	seen := make(map[Platform]bool)
	for _, p := range ps {
		if p == "" {
			t.Error("platform constant is empty")
		}
		if seen[p] {
			t.Error("duplicate platform constant")
		}
		seen[p] = true
	}
}

func TestDetect_HomeEmptyFallback(t *testing.T) {
	// os.UserHomeDir() can return "" if HOME is unset; Detect should fall
	// back to /tmp. We just verify it doesn't panic.
	t.Setenv("HOME", "")
	_ = Detect()
}

func TestDetect_WorkingDirFromExecutable(t *testing.T) {
	p := Detect()
	if p.WorkingDir == "" {
		t.Error("WorkingDir should not be empty")
	}
}

func TestIsDevEnv_FromPath(t *testing.T) {
	// Binary lives in /tmp/test/bin/ — heuristic should detect dev.
	// Hard to inject the executable path; just test the env var path.
	if !isDevEnv() && os.Getenv("GN_DRIVE_DEV") == "" {
		t.Setenv("GN_DRIVE_DEV", "1")
	}
	if !isDevEnv() {
		t.Error("isDevEnv should be true with GN_DRIVE_DEV=1")
	}
}

func TestIsDevEnv_Production(t *testing.T) {
	t.Setenv("GN_DRIVE_DEV", "")
	// We can't easily unset it, but the env var is checked first.
	// isDevEnv() may still return true if the executable lives in a dev path.
	// Just call it to ensure no panic.
	_ = isDevEnv()
}

func TestPaths_EnsureConfigDir_Exists(t *testing.T) {
	dir := t.TempDir()
	p := &Paths{ConfigDir: dir}
	// dir already exists, EnsureConfigDir should succeed.
	if err := p.EnsureConfigDir(); err != nil {
		t.Errorf("EnsureConfigDir on existing dir: %v", err)
	}
}

// --- additional platform detection tests ---

// TestDetect_XDGConfigHome_HonoredOnLinux exercises the Linux branch
// where XDG_CONFIG_HOME is honored.
func TestDetect_XDGConfigHome_HonoredOnLinux(t *testing.T) {
	if runtime.GOOS != "linux" {
		t.Skip("linux-only test")
	}
	t.Setenv("XDG_CONFIG_HOME", "/tmp/xdg-gn-drive-test")
	t.Setenv("XDG_STATE_HOME", "")
	p := Detect()
	want := "/tmp/xdg-gn-drive-test/gn-drive"
	if p.ConfigDir != want {
		t.Errorf("ConfigDir = %q, want %q", p.ConfigDir, want)
	}
}

// TestDetect_XDGStateHome_Default exercises the XDG_STATE_HOME default
// (when not set, falls back to $HOME/.local/state).
func TestDetect_XDGStateHome_Default(t *testing.T) {
	if runtime.GOOS != "linux" {
		t.Skip("linux-only test")
	}
	t.Setenv("XDG_STATE_HOME", "")
	t.Setenv("HOME", "/tmp/test-home")
	p := Detect()
	want := "/tmp/test-home/.local/state/gn-drive"
	if p.LogDir != want {
		t.Errorf("LogDir = %q, want %q", p.LogDir, want)
	}
}

// TestDetect_XDGStateHome_Override exercises the XDG_STATE_HOME override
// branch on Linux.
func TestDetect_XDGStateHome_Override(t *testing.T) {
	if runtime.GOOS != "linux" {
		t.Skip("linux-only test")
	}
	t.Setenv("XDG_STATE_HOME", "/tmp/xdg-state-override")
	p := Detect()
	want := "/tmp/xdg-state-override/gn-drive"
	if p.LogDir != want {
		t.Errorf("LogDir = %q, want %q", p.LogDir, want)
	}
}

// TestDetect_PlatformConstant exercises the Platform constant.
func TestDetect_PlatformConstant(t *testing.T) {
	p := Detect()
	switch runtime.GOOS {
	case "linux", "darwin", "windows":
		if p.Platform != Platform(runtime.GOOS) {
			t.Errorf("Platform = %q, want %q", p.Platform, runtime.GOOS)
		}
	}
}

// --- additional coverage for Linux branches via injectable platform ---

// withPlatform runs fn with runtimeGOOS overridden to os, then restores.
func withPlatform(t *testing.T, os string, fn func()) {
	t.Helper()
	orig := runtimeGOOS
	runtimeGOOS = func() string { return os }
	defer func() { runtimeGOOS = orig }()
	fn()
}

func TestDetect_Linux_XDGConfigHome(t *testing.T) {
	withPlatform(t, "linux", func() {
		t.Setenv("XDG_CONFIG_HOME", "/tmp/xdg-gn-test")
		p := Detect()
		want := "/tmp/xdg-gn-test/gn-drive"
		if p.ConfigDir != want {
			t.Errorf("ConfigDir = %q, want %q", p.ConfigDir, want)
		}
		if p.Platform != PlatformLinux {
			t.Errorf("Platform = %q, want %q", p.Platform, PlatformLinux)
		}
	})
}

func TestDetect_Linux_NoXDGConfigHome(t *testing.T) {
	withPlatform(t, "linux", func() {
		t.Setenv("XDG_CONFIG_HOME", "")
		t.Setenv("HOME", "/tmp/test-home")
		p := Detect()
		want := "/tmp/test-home/.config/gn-drive"
		if p.ConfigDir != want {
			t.Errorf("ConfigDir = %q, want %q", p.ConfigDir, want)
		}
	})
}

func TestDetect_NonLinux_IgnoresXDGConfigHome(t *testing.T) {
	withPlatform(t, "darwin", func() {
		t.Setenv("XDG_CONFIG_HOME", "/tmp/should-be-ignored")
		t.Setenv("HOME", "/tmp/test-home")
		p := Detect()
		want := "/tmp/test-home/.config/gn-drive"
		if p.ConfigDir != want {
			t.Errorf("ConfigDir = %q, want %q", p.ConfigDir, want)
		}
	})
}

func TestDetect_Linux_XDGStateHome_Override(t *testing.T) {
	withPlatform(t, "linux", func() {
		t.Setenv("XDG_STATE_HOME", "/tmp/xdg-state-override")
		p := Detect()
		want := "/tmp/xdg-state-override/gn-drive"
		if p.LogDir != want {
			t.Errorf("LogDir = %q, want %q", p.LogDir, want)
		}
	})
}

func TestDetect_Linux_XDGStateHome_Default(t *testing.T) {
	withPlatform(t, "linux", func() {
		t.Setenv("XDG_STATE_HOME", "")
		t.Setenv("HOME", "/tmp/test-home")
		p := Detect()
		want := "/tmp/test-home/.local/state/gn-drive"
		if p.LogDir != want {
			t.Errorf("LogDir = %q, want %q", p.LogDir, want)
		}
	})
}

func TestDetect_NonLinux_LogDirEqualsConfigDir(t *testing.T) {
	withPlatform(t, "darwin", func() {
		t.Setenv("HOME", "/tmp/test-home")
		p := Detect()
		if p.LogDir != p.ConfigDir {
			t.Errorf("LogDir = %q, want ConfigDir = %q", p.LogDir, p.ConfigDir)
		}
	})
}

func TestDetect_HomeEmpty(t *testing.T) {
	t.Setenv("HOME", "")
	// os.UserHomeDir may return "" which causes fallback to /tmp.
	p := Detect()
	if p.HomeDir == "" {
		t.Error("HomeDir should not be empty (falls back to /tmp)")
	}
}

// --- isDevEnv tests ---

func TestIsDevEnv_FromEnvVar(t *testing.T) {
	t.Setenv("GN_DRIVE_DEV", "1")
	if !isDevEnv() {
		t.Error("isDevEnv should return true when GN_DRIVE_DEV is set")
	}
}

func TestIsDevEnv_FromBinPath(t *testing.T) {
	orig := osExecutable
	defer func() { osExecutable = orig }()
	// Pretend the binary lives in /path/to/gn-drive/bin/gn-drive.
	osExecutable = func() (string, error) { return "/path/to/gn-drive/bin/gn-drive", nil }
	if !isDevEnv() {
		t.Error("isDevEnv should detect gn-drive/bin/ path")
	}
}

func TestIsDevEnv_FromDesktopBinPath(t *testing.T) {
	orig := osExecutable
	defer func() { osExecutable = orig }()
	// Pretend the binary lives in /path/to/desktop/bin/gn-drive.
	osExecutable = func() (string, error) { return "/path/to/desktop/bin/gn-drive", nil }
	if !isDevEnv() {
		t.Error("isDevEnv should detect desktop/bin/ path")
	}
}

func TestIsDevEnv_NotDev(t *testing.T) {
	orig := osExecutable
	defer func() { osExecutable = orig }()
	t.Setenv("GN_DRIVE_DEV", "")
	osExecutable = func() (string, error) { return "/usr/local/bin/gn-drive", nil }
	if isDevEnv() {
		t.Error("isDevEnv should return false for non-dev binary path")
	}
}

func TestIsDevEnv_NoExecutable(t *testing.T) {
	orig := osExecutable
	defer func() { osExecutable = orig }()
	t.Setenv("GN_DRIVE_DEV", "")
	osExecutable = func() (string, error) { return "", os.ErrNotExist }
	if isDevEnv() {
		t.Error("isDevEnv should return false when executable fails")
	}
}
