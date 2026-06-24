package instance

import (
	"errors"
	"os"
	"path/filepath"
	"strconv"
	"sync"
	"testing"
)

// withMkdirAll swaps osMkdirAll for the duration of the test.
func withMkdirAll(t *testing.T, fn func(string, os.FileMode) error) {
	t.Helper()
	orig := osMkdirAll
	t.Cleanup(func() { osMkdirAll = orig })
	osMkdirAll = fn
}

// withWriteFile swaps osWriteFile for the duration of the test.
func withWriteFile(t *testing.T, fn func(string, []byte, os.FileMode) error) {
	t.Helper()
	orig := osWriteFile
	t.Cleanup(func() { osWriteFile = orig })
	osWriteFile = fn
}

// withGetpid swaps osGetpid for the duration of the test.
func withGetpid(t *testing.T, fn func() int) {
	t.Helper()
	orig := osGetpid
	t.Cleanup(func() { osGetpid = orig })
	osGetpid = fn
}

// stubFlock implements tryLocker for tests.
type stubFlock struct {
	tryLockFn func() (bool, error)
	unlockFn  func() error

	unlockCalls int
	mu          sync.Mutex
}

func (s *stubFlock) TryLock() (bool, error) { return s.tryLockFn() }

func (s *stubFlock) Unlock() error {
	s.mu.Lock()
	s.unlockCalls++
	s.mu.Unlock()
	if s.unlockFn != nil {
		return s.unlockFn()
	}
	return nil
}

func (s *stubFlock) unlockCallCount() int {
	s.mu.Lock()
	defer s.mu.Unlock()
	return s.unlockCalls
}

// withFlock swaps newFlock for the duration of the test.
func withFlock(t *testing.T, stub tryLocker) {
	t.Helper()
	orig := newFlock
	t.Cleanup(func() { newFlock = orig })
	newFlock = func(string) tryLocker { return stub }
}

func TestAcquire_FirstSucceeds(t *testing.T) {
	dir := t.TempDir()
	l, err := Acquire(dir)
	if err != nil {
		t.Fatalf("Acquire: %v", err)
	}
	if l.PID() != os.Getpid() {
		t.Errorf("PID = %d, want %d", l.PID(), os.Getpid())
	}
	defer l.Release()

	// Lock file and pid file should exist.
	lockPath := filepath.Join(dir, "gn-drive.lock")
	pidPath := filepath.Join(dir, "gn-drive.pid")
	if _, err := os.Stat(lockPath); err != nil {
		t.Errorf("lock file missing: %v", err)
	}
	data, err := os.ReadFile(pidPath)
	if err != nil {
		t.Fatalf("pid file missing: %v", err)
	}
	if string(data) != strconv.Itoa(os.Getpid()) {
		t.Errorf("pid file = %q, want %q", data, strconv.Itoa(os.Getpid()))
	}
}

func TestAcquire_SecondFailsWithErrAnotherInstance(t *testing.T) {
	dir := t.TempDir()
	l1, err := Acquire(dir)
	if err != nil {
		t.Fatal(err)
	}
	defer l1.Release()

	_, err = Acquire(dir)
	if err == nil {
		t.Fatal("expected error acquiring second lock")
	}
	if !errors.Is(err, ErrAnotherInstance) {
		t.Errorf("err = %v, want wrap of ErrAnotherInstance", err)
	}
	if !contains(err.Error(), strconv.Itoa(l1.PID())) {
		t.Errorf("err message %q should mention pid %d", err.Error(), l1.PID())
	}
}

func TestAcquire_MkdirConfigDir(t *testing.T) {
	base := t.TempDir()
	dir := filepath.Join(base, "nested", "gn-drive")
	l, err := Acquire(dir)
	if err != nil {
		t.Fatal(err)
	}
	defer l.Release()
	if _, err := os.Stat(dir); err != nil {
		t.Errorf("config dir not created: %v", err)
	}
}

func TestRelease_Idempotent(t *testing.T) {
	dir := t.TempDir()
	l, err := Acquire(dir)
	if err != nil {
		t.Fatal(err)
	}
	if err := l.Release(); err != nil {
		t.Errorf("first Release: %v", err)
	}
	// Second release must be no-op and not error.
	if err := l.Release(); err != nil {
		t.Errorf("second Release: %v", err)
	}
	// After release, the pid file should be gone.
	pidPath := filepath.Join(dir, "gn-drive.pid")
	if _, err := os.Stat(pidPath); !os.IsNotExist(err) {
		t.Errorf("pid file should be removed; stat err = %v", err)
	}
}

func TestRelease_NilSafe(t *testing.T) {
	var l *Locker
	if err := l.Release(); err != nil {
		t.Errorf("nil Release: %v", err)
	}
}

func TestRelease_AfterReacquireKeepsOtherPid(t *testing.T) {
	// If the pid file has been overwritten by another process (or someone
	// hand-edited it), Release should NOT remove it.
	dir := t.TempDir()
	l, err := Acquire(dir)
	if err != nil {
		t.Fatal(err)
	}
	pidPath := filepath.Join(dir, "gn-drive.pid")
	// Simulate foreign pid file.
	if err := os.WriteFile(pidPath, []byte("999999"), 0o644); err != nil {
		t.Fatal(err)
	}
	if err := l.Release(); err != nil {
		t.Fatal(err)
	}
	if _, err := os.Stat(pidPath); err != nil {
		t.Errorf("foreign pid file should be preserved; stat err = %v", err)
	}
}

func TestAcquire_CorruptPIDFile(t *testing.T) {
	// If the pid file is corrupt, the conflict message omits a PID
	// rather than crashing.
	dir := t.TempDir()
	// Pre-create a corrupt pid file.
	pidPath := filepath.Join(dir, "gn-drive.pid")
	if err := os.WriteFile(pidPath, []byte("not-a-number"), 0o644); err != nil {
		t.Fatal(err)
	}

	// The first Acquire should still succeed (the existing file is just
	// overwritten with our pid).
	l, err := Acquire(dir)
	if err != nil {
		t.Fatal(err)
	}
	defer l.Release()

	// Now the second one must conflict; we don't care if it has a PID or
	// not — just that it returns ErrAnotherInstance.
	_, err = Acquire(dir)
	if !errors.Is(err, ErrAnotherInstance) {
		t.Errorf("err = %v, want ErrAnotherInstance", err)
	}
}

func TestAcquireAfterReleaseReuses(t *testing.T) {
	dir := t.TempDir()
	l1, err := Acquire(dir)
	if err != nil {
		t.Fatal(err)
	}
	if err := l1.Release(); err != nil {
		t.Fatal(err)
	}
	// Re-acquire must succeed.
	l2, err := Acquire(dir)
	if err != nil {
		t.Fatalf("re-Acquire: %v", err)
	}
	defer l2.Release()
}

func TestAcquire_Concurrent(t *testing.T) {
	dir := t.TempDir()
	const N = 5
	var wg sync.WaitGroup
	successes := make(chan *Locker, N)
	for i := 0; i < N; i++ {
		wg.Add(1)
		go func() {
			defer wg.Done()
			if l, err := Acquire(dir); err == nil {
				successes <- l
			}
		}()
	}
	wg.Wait()
	close(successes)
	count := 0
	for l := range successes {
		count++
		l.Release()
	}
	if count != 1 {
		t.Errorf("expected exactly 1 successful acquire, got %d", count)
	}
}

func contains(s, substr string) bool {
	for i := 0; i+len(substr) <= len(s); i++ {
		if s[i:i+len(substr)] == substr {
			return true
		}
	}
	return false
}

func TestAcquire_MkdirAllError(t *testing.T) {
	// Point osMkdirAll at a path where mkdir will fail. We simulate by
	// returning an error directly from the override.
	withMkdirAll(t, func(string, os.FileMode) error {
		return errors.New("disk full")
	})

	_, err := Acquire(t.TempDir())
	if err == nil {
		t.Fatal("expected error from Acquire when osMkdirAll fails")
	}
	if !contains(err.Error(), "instance: mkdir config dir") {
		t.Errorf("err = %q, want wrap of 'instance: mkdir config dir'", err)
	}
	if !contains(err.Error(), "disk full") {
		t.Errorf("err = %q, want wrap of underlying 'disk full'", err)
	}
}

func TestAcquire_TryLockError(t *testing.T) {
	// Stub flock that returns an error from TryLock.
	stub := &stubFlock{
		tryLockFn: func() (bool, error) { return false, errors.New("fs corrupt") },
	}
	withFlock(t, stub)

	_, err := Acquire(t.TempDir())
	if err == nil {
		t.Fatal("expected error from Acquire when TryLock errors")
	}
	if !contains(err.Error(), "instance: lock") {
		t.Errorf("err = %q, want wrap of 'instance: lock'", err)
	}
	if !contains(err.Error(), "fs corrupt") {
		t.Errorf("err = %q, want wrap of 'fs corrupt'", err)
	}
}

func TestAcquire_ConflictNoPIDFile(t *testing.T) {
	// The first Acquire succeeds; we then remove the pid file (simulating
	// a crash leaving the flock held but pid file gone). A second Acquire
	// must return ErrAnotherInstance *without* mentioning a pid — this
	// exercises the readPID == 0 branch.
	stub := &stubFlock{
		tryLockFn: func() (bool, error) { return false, nil },
	}
	withFlock(t, stub)

	dir := t.TempDir()
	// No pid file exists, so readPID returns 0 → conflict without pid.

	_, err := Acquire(dir)
	if err == nil {
		t.Fatal("expected ErrAnotherInstance when flock is held but no pid file")
	}
	if !errors.Is(err, ErrAnotherInstance) {
		t.Errorf("err = %v, want wrap of ErrAnotherInstance", err)
	}
	if contains(err.Error(), "pid=") {
		t.Errorf("err = %q, must NOT contain 'pid=' when no pid file exists", err)
	}
}

func TestAcquire_WritePIDError(t *testing.T) {
	// TryLock succeeds, but WriteFile fails. Acquire must call Unlock()
	// to roll back the lock and return the wrapped error.
	stub := &stubFlock{
		tryLockFn: func() (bool, error) { return true, nil },
	}
	withFlock(t, stub)

	withWriteFile(t, func(string, []byte, os.FileMode) error {
		return errors.New("read-only fs")
	})

	_, err := Acquire(t.TempDir())
	if err == nil {
		t.Fatal("expected error when osWriteFile fails")
	}
	if !contains(err.Error(), "instance: write pid file") {
		t.Errorf("err = %q, want wrap of 'instance: write pid file'", err)
	}
	if !contains(err.Error(), "read-only fs") {
		t.Errorf("err = %q, want wrap of 'read-only fs'", err)
	}
	if stub.unlockCallCount() != 1 {
		t.Errorf("Unlock called %d times, want exactly 1 (rollback)", stub.unlockCallCount())
	}
}

func TestReadPID_FileMissing(t *testing.T) {
	// readPID on a nonexistent file should return 0.
	dir := t.TempDir()
	missing := filepath.Join(dir, "no-such-file.pid")
	if got := readPID(missing); got != 0 {
		t.Errorf("readPID(missing) = %d, want 0", got)
	}
}

func TestReadPID_NonNumeric(t *testing.T) {
	// readPID on a non-numeric file should return 0.
	dir := t.TempDir()
	path := filepath.Join(dir, "gn-drive.pid")
	if err := os.WriteFile(path, []byte("not-a-number"), 0o644); err != nil {
		t.Fatal(err)
	}
	if got := readPID(path); got != 0 {
		t.Errorf("readPID(non-numeric) = %d, want 0", got)
	}
}

func TestReadPID_Empty(t *testing.T) {
	// readPID on an empty file should return 0 (Atoi fails on empty).
	dir := t.TempDir()
	path := filepath.Join(dir, "gn-drive.pid")
	if err := os.WriteFile(path, nil, 0o644); err != nil {
		t.Fatal(err)
	}
	if got := readPID(path); got != 0 {
		t.Errorf("readPID(empty) = %d, want 0", got)
	}
}
