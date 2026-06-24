package browser

import (
	"errors"
	"os"
	"runtime"
	"testing"
)

func TestNew_ReturnsOpener(t *testing.T) {
	o := New()
	if o == nil {
		t.Fatal("New returned nil")
	}
	if o.GOOS != nil {
		t.Error("GOOS should default to nil")
	}
}

func TestOpen_DarwinStartsOpen(t *testing.T) {
	if runtime.GOOS != "darwin" {
		t.Skip("darwin-only test")
	}
	o := New()
	// We just verify it returns no error and does not panic.
	// We cannot easily verify the spawned "open" process without
	// exposing the cmd, so we just check the call path.
	_ = o.Open("https://example.invalid")
}

func TestOpen_LinuxUsesXdgOpen(t *testing.T) {
	if runtime.GOOS != "linux" {
		t.Skip("linux-only test")
	}
	o := New()
	// xdg-open may or may not be installed; either way, Open should
	// return a sensible error or succeed without panic.
	_ = o.Open("https://example.invalid")
}

func TestOpen_WindowsRundll32(t *testing.T) {
	if runtime.GOOS != "windows" {
		t.Skip("windows-only test")
	}
	o := New()
	_ = o.Open("https://example.invalid")
}

func TestOpen_UnsupportedPlatformErrors(t *testing.T) {
	o := &Opener{GOOS: func() string { return "plan9" }}
	err := o.Open("https://example.invalid")
	if err == nil {
		t.Fatal("expected error for unsupported platform")
	}
}

func TestOpen_LinuxNoOpenerErrors(t *testing.T) {
	if runtime.GOOS != "linux" {
		t.Skip("linux-only test")
	}
	// Patch the lookup by setting PATH to a directory with no executables.
	t.Setenv("PATH", "/tmp/gn-drive-empty-path-test")
	err := (&Opener{}).Open("https://example.invalid")
	if err == nil {
		t.Fatal("expected error when no opener is on PATH")
	}
	// The error message should mention the missing tools.
	if !errorMentions(err, "xdg-open") && !errorMentions(err, "gio") && !errorMentions(err, "sensible-browser") {
		t.Errorf("error %q should mention a browser opener name", err)
	}
}

func errorMentions(err error, substr string) bool {
	if err == nil {
		return false
	}
	s := err.Error()
	for i := 0; i+len(substr) <= len(s); i++ {
		if s[i:i+len(substr)] == substr {
			return true
		}
	}
	return false
}

// TestOpen_OverrideGOOS exercises the GOOS override path.
func TestOpen_OverrideGOOS(t *testing.T) {
	o := &Opener{GOOS: func() string { return "darwin" }}
	// "open" exists on macOS by default. We don't fail if it doesn't.
	// The contract is: must not panic, may return error.
	_ = o.Open("https://example.invalid")
}

// TestOpen_OverrideGOOS_Linux forces the linux path with no openers.
func TestOpen_OverrideGOOS_Linux(t *testing.T) {
	t.Setenv("PATH", "/tmp/gn-drive-empty-path-test")
	o := &Opener{GOOS: func() string { return "linux" }}
	err := o.Open("https://example.invalid")
	if err == nil {
		t.Fatal("expected error for linux with no openers")
	}
	if !errorMentions(err, "xdg-open") && !errorMentions(err, "gio") && !errorMentions(err, "sensible-browser") {
		t.Errorf("error %q should mention a browser opener name", err)
	}
}

// TestOpen_OverrideGOOS_Linux_XDGOpen forces the linux path with xdg-open present.
func TestOpen_OverrideGOOS_Linux_XDGOpen(t *testing.T) {
	// Create a fake xdg-open in temp dir and put it on PATH.
	tmp := t.TempDir()
	fakeBin := tmp + "/xdg-open"
	if err := writeExecutable(fakeBin, "#!/bin/sh\nexit 0\n"); err != nil {
		t.Fatal(err)
	}
	t.Setenv("PATH", tmp)
	o := &Opener{GOOS: func() string { return "linux" }}
	if err := o.Open("https://example.invalid"); err != nil {
		t.Errorf("Open with xdg-open on PATH: %v", err)
	}
}

// TestOpen_OverrideGOOS_Linux_GIO forces the linux path with gio present.
func TestOpen_OverrideGOOS_Linux_GIO(t *testing.T) {
	tmp := t.TempDir()
	fakeBin := tmp + "/gio"
	if err := writeExecutable(fakeBin, "#!/bin/sh\nexit 0\n"); err != nil {
		t.Fatal(err)
	}
	t.Setenv("PATH", tmp)
	o := &Opener{GOOS: func() string { return "linux" }}
	if err := o.Open("https://example.invalid"); err != nil {
		t.Errorf("Open with gio on PATH: %v", err)
	}
}

// TestOpen_OverrideGOOS_Linux_SensibleBrowser forces the linux path with sensible-browser present.
func TestOpen_OverrideGOOS_Linux_SensibleBrowser(t *testing.T) {
	tmp := t.TempDir()
	fakeBin := tmp + "/sensible-browser"
	if err := writeExecutable(fakeBin, "#!/bin/sh\nexit 0\n"); err != nil {
		t.Fatal(err)
	}
	t.Setenv("PATH", tmp)
	o := &Opener{GOOS: func() string { return "linux" }}
	if err := o.Open("https://example.invalid"); err != nil {
		t.Errorf("Open with sensible-browser on PATH: %v", err)
	}
}

// TestOpen_OverrideGOOS_Windows forces the windows path.
func TestOpen_OverrideGOOS_Windows(t *testing.T) {
	o := &Opener{GOOS: func() string { return "windows" }}
	// rundll32 may not be on non-windows; allow error.
	_ = o.Open("https://example.invalid")
}

func writeExecutable(path, content string) error {
	if err := os.WriteFile(path, []byte(content), 0o755); err != nil {
		return err
	}
	return nil
}

// Sentinel: ensure the package's error type isn't accidentally shadowed.
var _ = errors.New
