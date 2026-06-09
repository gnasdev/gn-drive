// Package instance provides single-instance advisory locking.
//
// Phase 3: full implementation using github.com/gofrs/flock.
//
// Lock file: ~/.config/gn-drive/gn-drive.lock
// PID file:  ~/.config/gn-drive/gn-drive.pid
//
// On start, Acquire() tries to take the flock. If another process holds it,
// the existing PID is read from the pid file and returned via ErrAnotherInstance.
package instance

import (
	"errors"
	"fmt"
	"os"
	"path/filepath"
	"strconv"
	"sync"

	"github.com/gofrs/flock"
)

// ErrAnotherInstance is returned when a second gn-drive process tries to start
// while another is already running.
var ErrAnotherInstance = errors.New("another gn-drive instance is running")

// Locker represents an acquired advisory file lock.
type Locker struct {
	flock     *flock.Flock
	pidFile   string
	pid       int
	released  bool
	releaseMu sync.Mutex
}

// Acquire takes an exclusive advisory lock on the config directory.
// If another process holds the lock, returns ErrAnotherInstance with the
// existing PID embedded in the error message.
func Acquire(configDir string) (*Locker, error) {
	lockPath := filepath.Join(configDir, "gn-drive.lock")
	pidPath := filepath.Join(configDir, "gn-drive.pid")

	if err := os.MkdirAll(configDir, 0o700); err != nil {
		return nil, fmt.Errorf("instance: mkdir config dir: %w", err)
	}

	l := flock.New(lockPath)
	ok, err := l.TryLock()
	if err != nil {
		return nil, fmt.Errorf("instance: lock: %w", err)
	}
	if !ok {
		existingPID := readPID(pidPath)
		if existingPID > 0 {
			return nil, fmt.Errorf("%w (pid=%d). Run 'gn-drive service stop' to stop it, or kill the process manually", ErrAnotherInstance, existingPID)
		}
		return nil, fmt.Errorf("%w. Run 'gn-drive service stop' to stop it, or kill the process manually", ErrAnotherInstance)
	}

	pid := os.Getpid()
	if err := os.WriteFile(pidPath, []byte(strconv.Itoa(pid)), 0o644); err != nil {
		_ = l.Unlock()
		return nil, fmt.Errorf("instance: write pid file: %w", err)
	}

	return &Locker{flock: l, pidFile: pidPath, pid: pid}, nil
}

// Release releases the advisory lock and removes the pid file.
// Safe to call multiple times.
func (l *Locker) Release() error {
	if l == nil {
		return nil
	}
	l.releaseMu.Lock()
	defer l.releaseMu.Unlock()
	if l.released {
		return nil
	}
	l.released = true

	// Only remove pid file if it still contains our pid.
	if data, err := os.ReadFile(l.pidFile); err == nil {
		if pid, err := strconv.Atoi(string(data)); err == nil && pid == l.pid {
			_ = os.Remove(l.pidFile)
		}
	}
	return l.flock.Unlock()
}

// PID returns the PID of the process holding the lock.
func (l *Locker) PID() int { return l.pid }

func readPID(pidPath string) int {
	data, err := os.ReadFile(pidPath)
	if err != nil {
		return 0
	}
	pid, err := strconv.Atoi(string(data))
	if err != nil {
		return 0
	}
	return pid
}
