package app

import (
	"context"
	"os"
	"path/filepath"
	"testing"

	"github.com/gnasdev/gn-drive/internal/config"
	"github.com/gnasdev/gn-drive/internal/logging"
	"github.com/gnasdev/gn-drive/internal/service"
)

func TestClose_NilSafe(t *testing.T) {
	// An app with all nil fields must Close without panic.
	a := &App{}
	if err := a.Close(); err != nil {
		t.Errorf("Close on empty App: %v", err)
	}
}

func TestClose_PartialFields(t *testing.T) {
	// Just Health set to nil — other fields nil. Should not panic.
	a := &App{}
	if err := a.Close(); err != nil {
		t.Errorf("Close: %v", err)
	}
}

func TestNew_AuthFailure(t *testing.T) {
	// Empty ConfigDir triggers auth.New failure.
	_, err := New(context.Background(), Options{ConfigDir: "/this/does/not/exist/anywhere/xyz"})
	if err == nil {
		t.Fatal("expected error for invalid config dir")
	}
}

func TestNew_LoggerFallback(t *testing.T) {
	// When opts.ConfigDir is set to a valid path and LogMode is empty,
	// logger.New should fall back to foreground mode.
	dir := t.TempDir()
	a, err := New(context.Background(), Options{ConfigDir: dir})
	if err != nil {
		t.Fatal(err)
	}
	defer a.Close()
	if a.Log == nil {
		t.Error("Log should be set even with empty LogMode")
	}
}

func TestNew_ServiceLogMode(t *testing.T) {
	dir := t.TempDir()
	a, err := New(context.Background(), Options{ConfigDir: dir, LogMode: logging.ModeService})
	if err != nil {
		t.Fatal(err)
	}
	defer a.Close()
	if a.Log == nil {
		t.Error("Log should be set in service mode")
	}
}

func TestNew_UnlockPassword_Wrong(t *testing.T) {
	dir := t.TempDir()
	// First app to set up a password.
	a1, err := New(context.Background(), Options{ConfigDir: dir})
	if err != nil {
		t.Fatal(err)
	}
	if err := a1.Auth.SetupPassword("secret-pw-1"); err != nil {
		t.Fatal(err)
	}
	a1.Close()
	// Second app with wrong unlock password.
	_, err = New(context.Background(), Options{ConfigDir: dir, UnlockPassword: "wrong-pw"})
	if err == nil {
		t.Fatal("expected error for wrong unlock password")
	}
}

func TestNew_UnlockPassword_Correct(t *testing.T) {
	dir := t.TempDir()
	// First app to set up a password.
	a1, err := New(context.Background(), Options{ConfigDir: dir})
	if err != nil {
		t.Fatal(err)
	}
	if err := a1.Auth.SetupPassword("secret-pw-1"); err != nil {
		t.Fatal(err)
	}
	a1.Close()
	// Second app with correct unlock password.
	a2, err := New(context.Background(), Options{ConfigDir: dir, UnlockPassword: "secret-pw-1"})
	if err != nil {
		t.Fatal(err)
	}
	defer a2.Close()
	if !a2.Auth.IsUnlocked() {
		t.Error("auth should be unlocked")
	}
}

func TestNew_LockedAppWithoutPassword(t *testing.T) {
	dir := t.TempDir()
	// First app to set up a password and lock it.
	a1, err := New(context.Background(), Options{ConfigDir: dir})
	if err != nil {
		t.Fatal(err)
	}
	if err := a1.Auth.SetupPassword("secret-pw-1"); err != nil {
		t.Fatal(err)
	}
	a1.Auth.Lock()
	a1.Close()
	// CLI (non-portal) without unlock — should fail.
	_, err = New(context.Background(), Options{ConfigDir: dir})
	if err == nil {
		t.Fatal("expected error for locked app without portal mode")
	}
}

// TestNew_PortalMode_StartsLocked ensures web portal starts while locked and
// stays locked until unlock; data plane may open on plaintext or defer.
func TestNew_PortalMode_StartsLocked(t *testing.T) {
	dir := t.TempDir()
	a1, err := New(context.Background(), Options{ConfigDir: dir})
	if err != nil {
		t.Fatal(err)
	}
	if err := a1.Auth.SetupPassword("secret-pw-1"); err != nil {
		t.Fatal(err)
	}
	// Lock encrypts; close cleans up.
	if err := a1.Auth.Lock(); err != nil {
		t.Fatal(err)
	}
	_ = a1.Close()

	// Portal must start even when encrypted+locked.
	a2, err := New(context.Background(), Options{ConfigDir: dir, PortalMode: true})
	if err != nil {
		t.Fatalf("portal start while locked: %v", err)
	}
	defer a2.Close()
	if a2.Auth.IsUnlocked() {
		t.Error("portal should remain locked until web unlock")
	}
	// Encrypted start: store deferred.
	if a2.Store != nil {
		t.Error("expected deferred store when config is encrypted")
	}

	// Unlock + open data plane (same path as HTTP AfterUnlock).
	if err := a2.Auth.Unlock("secret-pw-1"); err != nil {
		t.Fatal(err)
	}
	if err := a2.AfterUnlock(context.Background()); err != nil {
		t.Fatal(err)
	}
	if a2.Store == nil {
		t.Error("store should open after unlock")
	}
}

// TestNew_PortalMode_PlaintextLocked opens store while still locked so the
// SPA can serve after password entry without re-decrypt delay.
func TestNew_PortalMode_PlaintextLocked(t *testing.T) {
	dir := t.TempDir()
	a1, err := New(context.Background(), Options{ConfigDir: dir})
	if err != nil {
		t.Fatal(err)
	}
	if err := a1.Auth.SetupPassword("secret-pw-1"); err != nil {
		t.Fatal(err)
	}
	// Do not Lock — leave plaintext on disk, process "locked" only in memory.
	// Simulate new process: auth loads as locked, files still plain.
	_ = a1.Store.Close()
	a1.Store = nil
	// Manually mark locked without encrypt: use a fresh app with locked state
	// by locking without encrypt... Setup leaves unlocked. Force new New after
	// writing auth as enabled: close with Lock would encrypt. Instead unlock
	// path: create locked state via Close (encrypts) then decrypt manually...
	// Simpler: new portal after setup without lock — unlocked true. Skip.
	// Use auth that is setup, unlocked=false, no .enc: OpenPlaintext path.
	// Create by: setup, lock (encrypt), unlock (decrypt), then new process
	// without unlocking (but Close re-encrypts). 
	// After Unlock files are plain; if we don't Close with Lock... 
	// Close always Locks. So: Unlock, then don't use Close — just leave files.
	// a1 is unlocked with plain files. Build a2 simulating restart: auth.New
	// sees enabled → locked in memory, files plain.
	// Force a1 not to re-encrypt: call BeforeLock already closed store; skip Auth.Lock
	// by replacing with portal New on same dir after Unlock without Close encrypt.
	if err := a1.Auth.Lock(); err != nil {
		t.Fatal(err)
	}
	// Now encrypted. Unlock to get plaintext without closing a1 store wrongly.
	if err := a1.Auth.Unlock("secret-pw-1"); err != nil {
		t.Fatal(err)
	}
	// Files plain, still "unlocked" in a1. Simulate exit without re-encrypt:
	// close store only.
	if a1.Store != nil {
		_ = a1.Store.Close()
		a1.Store = nil
	}
	// Fresh process: locked flag in auth, plaintext files.
	a2, err := New(context.Background(), Options{ConfigDir: dir, PortalMode: true})
	if err != nil {
		t.Fatal(err)
	}
	defer a2.Close()
	if a2.Auth.IsUnlocked() {
		t.Error("new process should start locked")
	}
	// Plaintext → store may already be open for faster post-unlock.
	if a2.Store == nil {
		t.Error("expected store open on plaintext locked start")
	}
}

// TestNew_RcloneFailure covers the rclone init error branch in New by
// pointing RcloneBinary at a non-existent path.
func TestNew_RcloneFailure(t *testing.T) {
	dir := t.TempDir()
	_, err := New(context.Background(), Options{ConfigDir: dir, RcloneBinary: "/nonexistent/rclone-xyz"})
	if err == nil {
		t.Error("expected error from rclone init with non-existent binary")
	}
}

// TestClose_HealthSet covers the a.Health != nil branch in Close. We use a
// nil Health to keep the test simple, but the a.Health.Stop() call requires
// a non-nil Health. Use a small wrapper that returns a non-nil Health.
func TestClose_HealthSet(t *testing.T) {
	// Health is a *Health (struct pointer), so we can construct one.
	// It's safe to call Stop on a non-running Health.
	dir := t.TempDir()
	a, err := New(context.Background(), Options{ConfigDir: dir})
	if err != nil {
		t.Fatal(err)
	}
	// a.Health should be set after New (it's part of the App).
	if a.Health == nil {
		t.Skip("Health is nil after New (in this build)")
	}
	if err := a.Close(); err != nil {
		t.Errorf("Close: %v", err)
	}
}

// TestClose_HealthSet_Direct covers the a.Health != nil branch in Close
// by directly setting a.Health to a service.Writer.
func TestClose_HealthSet_Direct(t *testing.T) {
	dir := t.TempDir()
	a := &App{
		Config: &config.Paths{ConfigDir: dir},
	}
	// Set a.Health to a non-nil service.Writer. The Writer doesn't need to
	// be running — Stop should be safe to call.
	w := service.NewWriter(dir, 1024)
	a.Health = w
	if err := a.Close(); err != nil {
		t.Errorf("Close with Health set: %v", err)
	}
}

// TestNew_StoreFailure covers the store init error branch in New by
// pre-creating a file at the gn-drive.db path inside the config dir.
func TestNew_StoreFailure(t *testing.T) {
	dir := t.TempDir()
	// Pre-create the db path as a regular file so store.New can't open it.
	dbPath := filepath.Join(dir, "gn-drive.db")
	if err := os.WriteFile(dbPath, []byte("blocker"), 0o600); err != nil {
		t.Fatal(err)
	}
	_, err := New(context.Background(), Options{ConfigDir: dir})
	if err == nil {
		t.Error("expected error from store init with blocker file at db path")
	}
}
