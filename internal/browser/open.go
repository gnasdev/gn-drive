// Package browser opens URLs in the system default browser.
package browser

import (
	"fmt"
	"os/exec"
	"runtime"
)

// Opener opens URLs in the system default browser.
type Opener struct {
	// Override for testing; if nil, uses runtime.GOOS.
	GOOS func() string
}

// New creates a new Opener.
func New() *Opener { return &Opener{} }

// Open opens url in the system default browser.
// Best-effort: errors are returned but not fatal — the user can always
// copy the URL from stdout.
func (o *Opener) Open(url string) error {
	goos := runtime.GOOS
	if o.GOOS != nil {
		goos = o.GOOS()
	}

	var cmd *exec.Cmd
	switch goos {
	case "darwin":
		cmd = exec.Command("open", url)
	case "linux":
		// Prefer xdg-open; fall back to gio, sensible-browser, wslview.
		if _, err := exec.LookPath("xdg-open"); err == nil {
			cmd = exec.Command("xdg-open", url)
		} else if _, err := exec.LookPath("gio"); err == nil {
			cmd = exec.Command("gio", "open", url)
		} else if _, err := exec.LookPath("sensible-browser"); err == nil {
			cmd = exec.Command("sensible-browser", url)
		} else {
			return fmt.Errorf("browser: no opener found (install xdg-open, gio, or sensible-browser)")
		}
	case "windows":
		cmd = exec.Command("rundll32", "url.dll,FileProtocolHandler", url)
	default:
		return fmt.Errorf("browser: unsupported platform %q", goos)
	}

	if err := cmd.Start(); err != nil {
		return fmt.Errorf("browser: start: %w", err)
	}
	// Don't wait — the child process is independent.
	go func() { _ = cmd.Wait() }()
	return nil
}
