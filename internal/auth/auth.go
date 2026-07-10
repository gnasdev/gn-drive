// Package auth provides master password authentication using Argon2id + AES-GCM.
//
// Phase 2: full implementation ported from desktop/backend/services/auth_service.go.
// Phase 2 keeps the same wire format (auth.json) so users can upgrade transparently.
//
// Wire format:
//
//	auth.json:
//	  {
//	    "enabled": true,
//	    "password_hash": "argon2id$v=19$m=65536,t=3,p=4$<salt_b64>$<hash_b64>",
//	    "failed_attempts": 0,
//	    "lockout_until": "",
//	    "app_settings": { ... }
//	  }
//
//	Encrypted file format (.enc):
//	  [12-byte nonce][AES-256-GCM(plaintext)]
package auth

import (
	"crypto/aes"
	"crypto/cipher"
	"crypto/rand"
	"crypto/subtle"
	"encoding/base64"
	"encoding/json"
	"errors"
	"fmt"
	"log/slog"
	"math"
	"os"
	"path/filepath"
	"strings"
	"sync"
	"time"

	"github.com/alexedwards/argon2id"
	"golang.org/x/crypto/argon2"
)

// Errors returned by Service.
var (
	ErrNotSetup        = errors.New("auth: master password not configured")
	ErrAlreadyUnlocked = errors.New("auth: app is already unlocked")
	ErrNotUnlocked     = errors.New("auth: app is locked")
	ErrInvalidPassword = errors.New("auth: invalid password")
	ErrLocked          = errors.New("auth: account locked, try again later")
	ErrAlreadySetup    = errors.New("auth: password already configured, use ChangePassword")
)

// Argon2id parameters — matches Wails format for cross-compat.
const (
	argon2Memory      = 64 * 1024 // 64MB
	argon2Iterations  = 3
	argon2Parallelism = 4
	argon2KeyLen      = 32
	argon2SaltLen     = 32
)

// argon2Params is used by alexedwards/argon2id for creating new hashes.
var argon2Params = &argon2id.Params{
	Memory:      argon2Memory,
	Iterations:  argon2Iterations,
	Parallelism: argon2Parallelism,
	SaltLength:  argon2SaltLen,
	KeyLength:   argon2KeyLen,
}

// Rate limit constants.
const (
	maxAttemptsBeforeDelay = 3
	maxAttemptsBeforeLock  = 10
	lockoutDuration        = 5 * time.Minute
)

// AppSettings holds user-facing settings persisted in auth.json.
type AppSettings struct {
	NotificationsEnabled bool `json:"notifications_enabled"`
	DebugMode            bool `json:"debug_mode"`
	MinimizeToTray       bool `json:"minimize_to_tray"`
	StartAtLogin         bool `json:"start_at_login"`
}

// AuthData is the on-disk format of auth.json.
type AuthData struct {
	Enabled        bool        `json:"enabled"`
	PasswordHash   string      `json:"password_hash"`
	FailedAttempts int         `json:"failed_attempts"`
	LockoutUntil   string      `json:"lockout_until"`
	AppSettings    AppSettings `json:"app_settings"`
}

// LockoutStatus is the public rate-limit state.
type LockoutStatus struct {
	FailedAttempts int    `json:"failed_attempts"`
	LockedUntil    string `json:"locked_until"`
	IsLocked       bool   `json:"is_locked"`
	RetryAfterSecs int    `json:"retry_after_secs"`
}

// Status is the public auth state.
type Status struct {
	Setup     bool      `json:"setup"`
	Unlocked  bool      `json:"unlocked"`
	LockedAt  time.Time `json:"locked_at"`
	Enabled   bool      `json:"enabled"`
	Lockout   LockoutStatus `json:"lockout"`
}

// Options configures the Service.
type Options struct {
	ConfigDir string
	Logger    *slog.Logger
}

// Service manages password-based unlock, encryption, and rate limiting.
type Service struct {
	mu          sync.RWMutex
	unlocked    bool
	encKey      []byte
	authData    *AuthData
	authFile    string
	configDir   string
	logger      *slog.Logger
	lockedAt    time.Time
	sleep       func(time.Duration) // injectable for tests
}

// New creates a new auth Service. It reads auth.json if present and
// determines whether the app is locked or unlocked.
func New(opts Options) (*Service, error) {
	if opts.ConfigDir == "" {
		return nil, errors.New("auth: ConfigDir required")
	}
	logger := opts.Logger
	if logger == nil {
		logger = slog.Default()
	}

	s := &Service{
		authFile:  filepath.Join(opts.ConfigDir, "auth.json"),
		configDir: opts.ConfigDir,
		logger:    logger,
		sleep:     time.Sleep,
	}

	authData, err := s.loadAuthData()
	if err != nil {
		if !os.IsNotExist(err) {
			logger.Warn("auth: failed to load auth.json, treating as no auth", "err", err)
		}
		authData = &AuthData{Enabled: false}
	}
	s.authData = authData

	// Crash recovery: clean up inconsistent state.
	s.recoverFromCrash()

	if !authData.Enabled {
		s.unlocked = true
		logger.Info("auth: not configured, app is open")
	} else {
		s.unlocked = false
		logger.Info("auth: configured, app is locked")
	}
	return s, nil
}

// IsSetup returns true if a master password has been configured.
func (s *Service) IsSetup() bool {
	s.mu.RLock()
	defer s.mu.RUnlock()
	return s.authData != nil && s.authData.Enabled
}

// IsUnlocked returns true if the app currently holds the decryption key.
func (s *Service) IsUnlocked() bool {
	s.mu.RLock()
	defer s.mu.RUnlock()
	return s.unlocked
}

// Status returns the public auth state.
func (s *Service) Status() Status {
	s.mu.RLock()
	defer s.mu.RUnlock()
	return Status{
		Setup:    s.authData != nil && s.authData.Enabled,
		Unlocked: s.unlocked,
		LockedAt: s.lockedAt,
		Enabled:  s.authData != nil && s.authData.Enabled,
		Lockout:  s.lockoutStatusLocked(),
	}
}

// LockoutStatus returns the current rate-limit state.
func (s *Service) LockoutStatus() LockoutStatus {
	s.mu.RLock()
	defer s.mu.RUnlock()
	return s.lockoutStatusLocked()
}

func (s *Service) lockoutStatusLocked() LockoutStatus {
	if s.authData == nil {
		return LockoutStatus{}
	}
	status := LockoutStatus{
		FailedAttempts: s.authData.FailedAttempts,
		LockedUntil:    s.authData.LockoutUntil,
	}
	if s.authData.LockoutUntil != "" {
		t, err := time.Parse(time.RFC3339, s.authData.LockoutUntil)
		if err == nil && time.Now().Before(t) {
			status.IsLocked = true
			status.RetryAfterSecs = int(math.Ceil(time.Until(t).Seconds()))
		}
	}
	if !status.IsLocked && s.authData.FailedAttempts >= maxAttemptsBeforeDelay {
		delay := int(math.Pow(2, float64(s.authData.FailedAttempts-maxAttemptsBeforeDelay)))
		status.RetryAfterSecs = delay
	}
	return status
}

// SetupPassword configures a new master password.
func (s *Service) SetupPassword(password string) error {
	s.mu.Lock()
	defer s.mu.Unlock()

	if s.authData != nil && s.authData.Enabled {
		return ErrAlreadySetup
	}
	if len(password) < 4 {
		return errors.New("auth: password must be at least 4 characters")
	}

	hash, err := createPasswordHash(password)
	if err != nil {
		return fmt.Errorf("auth: create hash: %w", err)
	}

	salt, err := extractSalt(hash)
	if err != nil {
		return fmt.Errorf("auth: extract salt: %w", err)
	}
	key := deriveKey(password, salt)

	s.authData = &AuthData{
		Enabled:        true,
		PasswordHash:   hash,
		FailedAttempts: 0,
		LockoutUntil:   "",
		AppSettings:    s.authData.AppSettings,
	}
	if err := s.saveAuthData(); err != nil {
		return fmt.Errorf("auth: save: %w", err)
	}
	s.encKey = key
	s.unlocked = true
	s.logger.Info("auth: password set up")
	return nil
}

// Unlock verifies the password, derives the AES key, and decrypts config files.
func (s *Service) Unlock(password string) error {
	s.mu.Lock()
	defer s.mu.Unlock()

	if s.authData == nil || !s.authData.Enabled {
		return ErrNotSetup
	}
	if s.unlocked {
		return nil
	}

	// Check lockout.
	if s.authData.LockoutUntil != "" {
		t, err := time.Parse(time.RFC3339, s.authData.LockoutUntil)
		if err == nil && time.Now().Before(t) {
			remaining := int(math.Ceil(time.Until(t).Seconds()))
			return fmt.Errorf("%w: %d seconds", ErrLocked, remaining)
		}
		s.authData.LockoutUntil = ""
	}

	// Enforce rate-limit delay (3..9 failed attempts: 2^attempts seconds).
	if s.authData.FailedAttempts >= maxAttemptsBeforeDelay && s.authData.FailedAttempts < maxAttemptsBeforeLock {
		delay := int(math.Pow(2, float64(s.authData.FailedAttempts-maxAttemptsBeforeDelay)))
		s.mu.Unlock()
		s.sleep(time.Duration(delay) * time.Second)
		s.mu.Lock()
		if s.unlocked {
			return nil
		}
	}

	if !verifyPasswordHash(password, s.authData.PasswordHash) {
		s.authData.FailedAttempts++
		if s.authData.FailedAttempts >= maxAttemptsBeforeLock {
			s.authData.LockoutUntil = time.Now().Add(lockoutDuration).Format(time.RFC3339)
			s.authData.FailedAttempts = 0
			s.logger.Warn("auth: too many failed attempts, locked", "duration", lockoutDuration)
		}
		_ = s.saveAuthData()
		return ErrInvalidPassword
	}

	salt, err := extractSaltFn(s.authData.PasswordHash)
	if err != nil {
		return fmt.Errorf("auth: extract salt: %w", err)
	}
	key := deriveKey(password, salt)

	if err := s.decryptConfigFiles(key); err != nil {
		return fmt.Errorf("auth: decrypt: %w", err)
	}

	s.authData.FailedAttempts = 0
	s.authData.LockoutUntil = ""
	_ = s.saveAuthData()

	s.encKey = key
	s.unlocked = true
	s.lockedAt = time.Time{}
	s.logger.Info("auth: unlocked")
	return nil
}

// Lock re-encrypts files and zeros the key.
func (s *Service) Lock() error {
	s.mu.Lock()
	defer s.mu.Unlock()

	if !s.unlocked {
		return nil
	}
	s.lockInternal()
	return nil
}

func (s *Service) lockInternal() {
	if !s.unlocked || s.encKey == nil {
		return
	}
	if err := s.encryptConfigFiles(s.encKey); err != nil {
		s.logger.Error("auth: encrypt on lock failed", "err", err)
	}
	zeroBytes(s.encKey)
	s.encKey = nil
	s.unlocked = false
	s.lockedAt = time.Now()
}

// ChangePassword re-keys the encryption.
func (s *Service) ChangePassword(oldPwd, newPwd string) error {
	s.mu.Lock()
	defer s.mu.Unlock()

	if s.authData == nil || !s.authData.Enabled {
		return ErrNotSetup
	}
	if !s.unlocked {
		return ErrNotUnlocked
	}
	if !verifyPasswordHash(oldPwd, s.authData.PasswordHash) {
		return ErrInvalidPassword
	}
	if len(newPwd) < 4 {
		return errors.New("auth: new password must be at least 4 characters")
	}

	newHash, err := createPasswordHash(newPwd)
	if err != nil {
		return fmt.Errorf("auth: create hash: %w", err)
	}
	newSalt, err := extractSalt(newHash)
	if err != nil {
		return fmt.Errorf("auth: extract salt: %w", err)
	}
	newKey := deriveKey(newPwd, newSalt)

	if err := s.encryptConfigFiles(newKey); err != nil {
		// Try to recover: decrypt with new key.
		if decErr := s.decryptConfigFiles(newKey); decErr != nil {
			zeroBytes(s.encKey)
			s.encKey = nil
			s.unlocked = false
			return fmt.Errorf("auth: re-encrypt failed and recovery failed: %w", err)
		}
		return fmt.Errorf("auth: re-encrypt: %w", err)
	}

	oldHash := s.authData.PasswordHash
	s.authData.PasswordHash = newHash
	if err := s.saveAuthData(); err != nil {
		s.authData.PasswordHash = oldHash
		_ = s.decryptConfigFiles(newKey)
		return fmt.Errorf("auth: save: %w", err)
	}

	if err := s.decryptConfigFiles(newKey); err != nil {
		zeroBytes(s.encKey)
		s.encKey = nil
		s.unlocked = false
		return fmt.Errorf("auth: re-decrypt: %w", err)
	}

	zeroBytes(s.encKey)
	s.encKey = newKey
	s.logger.Info("auth: password changed")
	return nil
}

// RemovePassword removes master-password protection and decrypts files permanently.
func (s *Service) RemovePassword(password string) error {
	s.mu.Lock()
	defer s.mu.Unlock()

	if s.authData == nil || !s.authData.Enabled {
		return ErrNotSetup
	}
	if !s.unlocked {
		return ErrNotUnlocked
	}
	if !verifyPasswordHash(password, s.authData.PasswordHash) {
		return ErrInvalidPassword
	}
	s.cleanupEncryptedFiles()
	_ = os.Remove(s.authFile)
	zeroBytes(s.encKey)
	s.encKey = nil
	s.authData = &AuthData{Enabled: false}
	s.unlocked = true
	s.logger.Info("auth: password removed")
	return nil
}

// --- Internal helpers -----------------------------------------------------

func (s *Service) loadAuthData() (*AuthData, error) {
	data, err := os.ReadFile(s.authFile)
	if err != nil {
		return nil, err
	}
	var d AuthData
	if err := json.Unmarshal(data, &d); err != nil {
		return nil, fmt.Errorf("parse auth.json: %w", err)
	}
	return &d, nil
}

// marshalAuthData is overridable for tests; defaults to json.MarshalIndent.
var marshalAuthData = json.MarshalIndent

func (s *Service) saveAuthData() error {
	data, err := marshalAuthData(s.authData, "", "  ")
	if err != nil {
		return fmt.Errorf("marshal: %w", err)
	}
	if err := os.MkdirAll(filepath.Dir(s.authFile), 0o700); err != nil {
		return fmt.Errorf("mkdir: %w", err)
	}
	return os.WriteFile(s.authFile, data, 0o600)
}

// recoverFromCrash cleans up inconsistent encrypt/decrypt state.
// If both plaintext and .enc exist for a file:
//   - Auth enabled: keep .enc (encrypted is authoritative), remove plaintext
//   - Auth disabled: keep plaintext, remove stale .enc
func (s *Service) recoverFromCrash() {
	files := []string{"rclone.conf", "gn-drive.db"}
	for _, name := range files {
		base := filepath.Join(s.configDir, name)
		enc := base + ".enc"
		plainExists := fileExists(base)
		encExists := fileExists(enc)
		if !(plainExists && encExists) {
			continue
		}
		if s.authData != nil && s.authData.Enabled {
			_ = os.Remove(base)
			_ = os.Remove(base + "-wal")
			_ = os.Remove(base + "-shm")
			s.logger.Warn("auth: crash recovery - removed plaintext", "file", name)
		} else {
			_ = os.Remove(enc)
			s.logger.Warn("auth: crash recovery - removed .enc", "file", name)
		}
	}
}

// encryptFilesFn is overridable for tests; defaults to encryptFile.
// Tests can swap it to inject errors into encryptConfigFiles.
var encryptFilesFn = encryptFile

// decryptFilesFn is overridable for tests; defaults to decryptFile.
// Tests can swap it to inject errors into decryptConfigFiles.
var decryptFilesFn = decryptFile

func (s *Service) encryptConfigFiles(key []byte) error {
	for _, name := range []string{"rclone.conf", "gn-drive.db"} {
		src := filepath.Join(s.configDir, name)
		if _, err := os.Stat(src); os.IsNotExist(err) {
			continue
		}
		if err := encryptFilesFn(src, src+".enc", key); err != nil {
			return fmt.Errorf("encrypt %s: %w", name, err)
		}
		_ = os.Remove(src)
		_ = os.Remove(src + "-wal")
		_ = os.Remove(src + "-shm")
	}
	return nil
}

func (s *Service) decryptConfigFiles(key []byte) error {
	for _, name := range []string{"rclone.conf", "gn-drive.db"} {
		base := filepath.Join(s.configDir, name)
		enc := base + ".enc"
		if _, err := os.Stat(enc); os.IsNotExist(err) {
			continue
		}
		if err := decryptFilesFn(enc, base, key); err != nil {
			return fmt.Errorf("decrypt %s: %w", name, err)
		}
		_ = os.Remove(enc)
	}
	return nil
}

func (s *Service) cleanupEncryptedFiles() {
	for _, name := range []string{"rclone.conf.enc", "gn-drive.db.enc"} {
		_ = os.Remove(filepath.Join(s.configDir, name))
	}
}

func fileExists(p string) bool {
	_, err := os.Stat(p)
	return err == nil
}

// --- Crypto functions -----------------------------------------------------

func deriveKey(password string, salt []byte) []byte {
	return argon2.IDKey([]byte(password), salt, argon2Iterations, argon2Memory, argon2Parallelism, argon2KeyLen)
}

func createPasswordHash(password string) (string, error) {
	return argon2id.CreateHash(password, argon2Params)
}

func verifyPasswordHash(password, encoded string) bool {
	if strings.HasPrefix(encoded, "$argon2id$") {
		match, err := argon2id.ComparePasswordAndHash(password, encoded)
		return err == nil && match
	}
	return verifyLegacyHash(password, encoded)
}

func verifyLegacyHash(password, encoded string) bool {
	parts := strings.Split(encoded, "$")
	if len(parts) != 5 {
		return false
	}
	salt, err := base64.RawStdEncoding.DecodeString(parts[3])
	if err != nil {
		return false
	}
	storedHash, err := base64.RawStdEncoding.DecodeString(parts[4])
	if err != nil {
		return false
	}
	computed := argon2.IDKey([]byte(password), salt, argon2Iterations, argon2Memory, argon2Parallelism, argon2KeyLen)
	return subtle.ConstantTimeCompare(storedHash, computed) == 1
}

// extractSaltFn is overridable for tests; defaults to extractSalt.
var extractSaltFn = extractSalt

func extractSalt(encoded string) ([]byte, error) {
	// Handle new format ($argon2id$v=19$m=...$salt$hash) — 6 parts split by $
	if strings.HasPrefix(encoded, "$argon2id$") {
		parts := strings.Split(encoded, "$")
		if len(parts) != 6 {
			return nil, errors.New("invalid hash format")
		}
		return base64.RawStdEncoding.DecodeString(parts[4])
	}
	// Legacy format (argon2id$v=19$m=...$salt$hash) — 5 parts
	parts := strings.Split(encoded, "$")
	if len(parts) != 5 {
		return nil, errors.New("invalid hash format")
	}
	return base64.RawStdEncoding.DecodeString(parts[3])
}

// newCipher is overridable for tests; defaults to aes.NewCipher.
var newCipher = aes.NewCipher

// newGCM is overridable for tests; defaults to cipher.NewGCM.
var newGCM = cipher.NewGCM

func encryptFile(srcPath, dstPath string, key []byte) error {
	plaintext, err := os.ReadFile(srcPath)
	if err != nil {
		return fmt.Errorf("read: %w", err)
	}
	block, err := newCipher(key)
	if err != nil {
		return fmt.Errorf("cipher: %w", err)
	}
	gcm, err := newGCM(block)
	if err != nil {
		return fmt.Errorf("gcm: %w", err)
	}
	nonce := make([]byte, gcm.NonceSize())
	if _, err := cryptoRandRead(nonce); err != nil {
		return fmt.Errorf("nonce: %w", err)
	}
	out := gcm.Seal(nonce, nonce, plaintext, nil)
	return os.WriteFile(dstPath, out, 0o600)
}

func decryptFile(srcPath, dstPath string, key []byte) error {
	ciphertext, err := os.ReadFile(srcPath)
	if err != nil {
		return fmt.Errorf("read: %w", err)
	}
	block, err := newCipher(key)
	if err != nil {
		return fmt.Errorf("cipher: %w", err)
	}
	gcm, err := newGCM(block)
	if err != nil {
		return fmt.Errorf("gcm: %w", err)
	}
	ns := gcm.NonceSize()
	if len(ciphertext) < ns {
		return errors.New("ciphertext too short")
	}
	nonce, body := ciphertext[:ns], ciphertext[ns:]
	plaintext, err := gcm.Open(nil, nonce, body, nil)
	if err != nil {
		return errors.New("decryption failed (wrong password or corrupted data)")
	}
	return os.WriteFile(dstPath, plaintext, 0o600)
}

// EncryptData encrypts raw data using AES-256-GCM (used for export).
func EncryptData(data, key []byte) ([]byte, error) {
	block, err := newCipher(key)
	if err != nil {
		return nil, err
	}
	gcm, err := newGCM(block)
	if err != nil {
		return nil, err
	}
	nonce := make([]byte, gcm.NonceSize())
	if _, err := cryptoRandRead(nonce); err != nil {
		return nil, err
	}
	return gcm.Seal(nonce, nonce, data, nil), nil
}

// DecryptData decrypts AES-256-GCM data (used for export).
func DecryptData(ciphertext, key []byte) ([]byte, error) {
	block, err := newCipher(key)
	if err != nil {
		return nil, err
	}
	gcm, err := newGCM(block)
	if err != nil {
		return nil, err
	}
	ns := gcm.NonceSize()
	if len(ciphertext) < ns {
		return nil, errors.New("ciphertext too short")
	}
	return gcm.Open(nil, ciphertext[:ns], ciphertext[ns:], nil)
}

// DeriveExportKey derives an encryption key from a password (used for export).
func DeriveExportKey(password string) (key, salt []byte) {
	salt = make([]byte, 16)
	_, _ = rand.Read(salt)
	key = deriveKey(password, salt)
	return
}

func zeroBytes(b []byte) {
	for i := range b {
		b[i] = 0
	}
}

// cryptoRandRead is the testable inner of crypto/rand.Read. It allows
// tests to inject a failing read.
var cryptoRandRead = rand.Read

