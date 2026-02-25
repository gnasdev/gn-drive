package services

import (
	"context"
	"crypto/aes"
	"crypto/cipher"
	"crypto/rand"
	"crypto/subtle"
	"encoding/base64"
	"encoding/json"
	"fmt"
	"log"
	"math"
	"os"
	"path/filepath"
	"strings"
	"sync"
	"time"

	"desktop/backend/events"

	"github.com/wailsapp/wails/v3/pkg/application"
	"golang.org/x/crypto/argon2"
)

// Auth event types
const (
	AuthLocked   events.EventType = "auth:locked"
	AuthUnlocked events.EventType = "auth:unlocked"
)

// Argon2id parameters
const (
	argon2Memory      = 64 * 1024 // 64MB
	argon2Iterations  = 3
	argon2Parallelism = 4
	argon2KeyLen      = 32
	argon2SaltLen     = 32
)

// Rate limit constants
const (
	maxAttemptsBeforeDelay = 3
	maxAttemptsBeforeLock  = 10
	lockoutDuration        = 5 * time.Minute
)

// AuthData holds persisted auth metadata (stored in auth.json)
type AuthData struct {
	Enabled        bool        `json:"enabled"`
	PasswordHash   string      `json:"password_hash"`
	FailedAttempts int         `json:"failed_attempts"`
	LockoutUntil   string      `json:"lockout_until"`
	AppSettings    AppSettings `json:"app_settings"`
}

// LockoutStatus represents the current rate limit state
type LockoutStatus struct {
	FailedAttempts int    `json:"failed_attempts"`
	LockedUntil    string `json:"locked_until"`
	IsLocked       bool   `json:"is_locked"`
	RetryAfterSecs int    `json:"retry_after_secs"`
}

// AuthEvent is emitted on auth state changes
type AuthEvent struct {
	events.BaseEvent
}

// AuthService manages password-based unlock, encryption, and rate limiting
type AuthService struct {
	app                 *application.App
	appService          interface{ CompleteInitialization(context.Context) error }
	notificationService *NotificationService
	mutex               sync.RWMutex
	unlocked            bool
	encKey              []byte // derived encryption key, zeroed on lock
	authData            *AuthData
	authFilePath        string
}

// NewAuthService creates a new AuthService
func NewAuthService(app *application.App) *AuthService {
	return &AuthService{
		app: app,
	}
}

// SetApp sets the application reference
func (a *AuthService) SetApp(app *application.App) {
	a.app = app
}

// SetAppService sets the App service reference for deferred initialization
func (a *AuthService) SetAppService(appService interface{ CompleteInitialization(context.Context) error }) {
	a.appService = appService
}

// SetNotificationService sets the notification service reference
func (a *AuthService) SetNotificationService(ns *NotificationService) {
	a.notificationService = ns
}

// ServiceName returns the service name
func (a *AuthService) ServiceName() string {
	return "AuthService"
}

// ServiceStartup is called when the service starts.
// It reads auth.json and either unlocks immediately (no auth) or waits for password.
func (a *AuthService) ServiceStartup(ctx context.Context, options application.ServiceOptions) error {
	log.Printf("AuthService starting up...")

	cfg := GetSharedConfig()
	if cfg == nil {
		return fmt.Errorf("shared config not set")
	}
	a.authFilePath = filepath.Join(cfg.ConfigDir, "auth.json")

	// Load auth data
	authData, err := a.loadAuthData()
	if err != nil {
		log.Printf("AuthService: No auth.json found or invalid, treating as no auth: %v", err)
		authData = &AuthData{Enabled: false}
	}
	a.authData = authData

	// Register settings syncer so tray/startup settings get saved to auth.json
	SetAuthSettingsSyncer(a.SyncAppSettings)

	// Crash recovery: clean up inconsistent state from interrupted encrypt/decrypt
	a.recoverFromCrash(cfg)

	if !authData.Enabled {
		// No auth - initialize everything immediately
		if err := a.initializeApp(ctx); err != nil {
			return fmt.Errorf("failed to initialize app: %w", err)
		}
		a.unlocked = true
		a.emitAuthEvent(AuthUnlocked)
		log.Printf("AuthService: No auth configured, app unlocked")
	} else {
		// Auth enabled - wait for unlock
		a.unlocked = false
		a.emitAuthEvent(AuthLocked)
		log.Printf("AuthService: Auth enabled, waiting for unlock")
	}

	return nil
}

// ServiceShutdown is called when the service shuts down.
// If auth is enabled, re-encrypt files and zero the key.
func (a *AuthService) ServiceShutdown(ctx context.Context) error {
	log.Printf("AuthService shutting down...")
	a.mutex.Lock()
	defer a.mutex.Unlock()

	if a.authData != nil && a.authData.Enabled && a.unlocked {
		a.lockInternal()
	}

	return nil
}

// IsAuthEnabled returns whether password auth is configured
func (a *AuthService) IsAuthEnabled(ctx context.Context) bool {
	a.mutex.RLock()
	defer a.mutex.RUnlock()
	return a.authData != nil && a.authData.Enabled
}

// IsUnlocked returns whether the app is currently unlocked
func (a *AuthService) IsUnlocked(ctx context.Context) bool {
	a.mutex.RLock()
	defer a.mutex.RUnlock()
	return a.unlocked
}

// GetLockoutStatus returns the current rate limiting status
func (a *AuthService) GetLockoutStatus(ctx context.Context) LockoutStatus {
	a.mutex.RLock()
	defer a.mutex.RUnlock()

	if a.authData == nil {
		return LockoutStatus{}
	}

	status := LockoutStatus{
		FailedAttempts: a.authData.FailedAttempts,
		LockedUntil:    a.authData.LockoutUntil,
	}

	if a.authData.LockoutUntil != "" {
		lockoutTime, err := time.Parse(time.RFC3339, a.authData.LockoutUntil)
		if err == nil && time.Now().Before(lockoutTime) {
			status.IsLocked = true
			status.RetryAfterSecs = int(math.Ceil(time.Until(lockoutTime).Seconds()))
		}
	}

	// Calculate delay for attempts 4-9
	if !status.IsLocked && a.authData.FailedAttempts >= maxAttemptsBeforeDelay {
		delaySecs := int(math.Pow(2, float64(a.authData.FailedAttempts-maxAttemptsBeforeDelay)))
		status.RetryAfterSecs = delaySecs
	}

	return status
}

// GetPreUnlockSettings returns app settings from auth.json (available before unlock)
func (a *AuthService) GetPreUnlockSettings() AppSettings {
	a.mutex.RLock()
	defer a.mutex.RUnlock()
	if a.authData != nil {
		return a.authData.AppSettings
	}
	return AppSettings{}
}

// SetupPassword sets up password authentication for the first time.
// Encrypts all sensitive files and creates auth.json.
func (a *AuthService) SetupPassword(ctx context.Context, password string) error {
	a.mutex.Lock()
	defer a.mutex.Unlock()

	if a.authData != nil && a.authData.Enabled {
		return fmt.Errorf("password already configured, use ChangePassword instead")
	}

	if len(password) < 4 {
		return fmt.Errorf("password must be at least 4 characters")
	}

	// Generate salt and derive key
	salt := make([]byte, argon2SaltLen)
	if _, err := rand.Read(salt); err != nil {
		return fmt.Errorf("failed to generate salt: %w", err)
	}

	key := deriveKey(password, salt)
	hash := encodePasswordHash(password, salt)

	// Get current app settings from notification service (if DB is available)
	var appSettings AppSettings
	if a.notificationService != nil {
		appSettings = a.notificationService.GetSettings(ctx)
	}

	// NOTE: We do NOT encrypt files here. The DB connection is still open and
	// deleting the file would cause data loss. Files stay as plaintext for the
	// current session. lockInternal() (called on Lock/Shutdown) will properly
	// close DB first, then encrypt.

	// Delete legacy JSON files (already migrated to DB)
	cfg := GetSharedConfig()
	a.deleteLegacyFiles(cfg)

	// Save auth data
	a.authData = &AuthData{
		Enabled:        true,
		PasswordHash:   hash,
		FailedAttempts: 0,
		LockoutUntil:   "",
		AppSettings:    appSettings,
	}
	if err := a.saveAuthData(); err != nil {
		return fmt.Errorf("failed to save auth data: %w", err)
	}

	a.encKey = key
	log.Printf("AuthService: Password set up successfully")
	return nil
}

// Unlock verifies the password and decrypts all files
func (a *AuthService) Unlock(ctx context.Context, password string) error {
	a.mutex.Lock()
	defer a.mutex.Unlock()

	if a.authData == nil || !a.authData.Enabled {
		return fmt.Errorf("auth not enabled")
	}

	if a.unlocked {
		return nil // Already unlocked
	}

	// Check lockout
	if a.authData.LockoutUntil != "" {
		lockoutTime, err := time.Parse(time.RFC3339, a.authData.LockoutUntil)
		if err == nil && time.Now().Before(lockoutTime) {
			remaining := int(math.Ceil(time.Until(lockoutTime).Seconds()))
			return fmt.Errorf("account locked, try again in %d seconds", remaining)
		}
		// Lockout expired, clear it
		a.authData.LockoutUntil = ""
	}

	// Enforce rate limit delay server-side (prevents brute-force bypassing UI)
	if a.authData.FailedAttempts >= maxAttemptsBeforeDelay && a.authData.FailedAttempts < maxAttemptsBeforeLock {
		delaySecs := int(math.Pow(2, float64(a.authData.FailedAttempts-maxAttemptsBeforeDelay)))
		// Release lock during sleep so other operations aren't blocked
		a.mutex.Unlock()
		time.Sleep(time.Duration(delaySecs) * time.Second)
		a.mutex.Lock()
		// Re-check state after reacquiring lock (another goroutine may have unlocked)
		if a.unlocked {
			return nil
		}
	}

	// Verify password
	if !verifyPasswordHash(password, a.authData.PasswordHash) {
		a.authData.FailedAttempts++

		if a.authData.FailedAttempts >= maxAttemptsBeforeLock {
			a.authData.LockoutUntil = time.Now().Add(lockoutDuration).Format(time.RFC3339)
			a.authData.FailedAttempts = 0
			log.Printf("AuthService: Too many failed attempts, locked for %v", lockoutDuration)
		}

		a.saveAuthData()
		return fmt.Errorf("incorrect password")
	}

	// Password correct - derive key and decrypt
	salt, err := extractSalt(a.authData.PasswordHash)
	if err != nil {
		return fmt.Errorf("failed to extract salt: %w", err)
	}

	key := deriveKey(password, salt)
	cfg := GetSharedConfig()

	if err := a.decryptConfigFiles(cfg, key); err != nil {
		return fmt.Errorf("failed to decrypt files: %w", err)
	}

	// Reset failed attempts
	a.authData.FailedAttempts = 0
	a.authData.LockoutUntil = ""
	a.saveAuthData()

	// Initialize the app (DB, rclone, etc.)
	if err := a.initializeApp(ctx); err != nil {
		// Re-encrypt on failure to leave files in secure state
		a.encryptConfigFiles(cfg, key)
		return fmt.Errorf("failed to initialize app after unlock: %w", err)
	}

	a.encKey = key
	a.unlocked = true
	a.emitAuthEvent(AuthUnlocked)

	log.Printf("AuthService: Unlocked successfully")
	return nil
}

// Lock re-encrypts all files and clears the key
func (a *AuthService) Lock(ctx context.Context) error {
	a.mutex.Lock()
	defer a.mutex.Unlock()

	if !a.unlocked {
		return nil // Already locked
	}

	a.lockInternal()
	a.emitAuthEvent(AuthLocked)

	log.Printf("AuthService: Locked successfully")
	return nil
}

// ChangePassword changes the master password
func (a *AuthService) ChangePassword(ctx context.Context, oldPassword, newPassword string) error {
	a.mutex.Lock()
	defer a.mutex.Unlock()

	if a.authData == nil || !a.authData.Enabled {
		return fmt.Errorf("auth not enabled")
	}

	if !a.unlocked {
		return fmt.Errorf("app must be unlocked to change password")
	}

	// Verify old password
	if !verifyPasswordHash(oldPassword, a.authData.PasswordHash) {
		return fmt.Errorf("incorrect current password")
	}

	if len(newPassword) < 4 {
		return fmt.Errorf("new password must be at least 4 characters")
	}

	// Generate new salt and key
	newSalt := make([]byte, argon2SaltLen)
	if _, err := rand.Read(newSalt); err != nil {
		return fmt.Errorf("failed to generate salt: %w", err)
	}

	newKey := deriveKey(newPassword, newSalt)
	newHash := encodePasswordHash(newPassword, newSalt)

	// Close DB before re-encrypting
	CloseDatabase()
	ResetSharedDB()

	cfg := GetSharedConfig()
	oldHash := a.authData.PasswordHash

	// Encrypt with new key
	if err := a.encryptConfigFiles(cfg, newKey); err != nil {
		// Try to recover: decrypt with new key (files that were encrypted) and reopen DB
		if decErr := a.decryptConfigFiles(cfg, newKey); decErr != nil {
			// Recovery failed - files are in mixed state. Lock app so user re-unlocks with new password.
			zeroBytes(a.encKey)
			a.encKey = nil
			a.unlocked = false
			a.emitAuthEvent(AuthLocked)
			return fmt.Errorf("failed to re-encrypt files and recovery failed, please restart and re-unlock: %w", err)
		}
		InitDatabase()
		return fmt.Errorf("failed to re-encrypt files: %w", err)
	}

	// Update auth data
	a.authData.PasswordHash = newHash
	if err := a.saveAuthData(); err != nil {
		// Revert hash on failure
		a.authData.PasswordHash = oldHash
		if decErr := a.decryptConfigFiles(cfg, newKey); decErr != nil {
			// Can't decrypt back — files encrypted with newKey, auth.json has oldHash.
			// Force new hash into auth.json so user can re-unlock with new password.
			a.authData.PasswordHash = newHash
			a.saveAuthData() // best-effort
			zeroBytes(a.encKey)
			a.encKey = nil
			a.unlocked = false
			a.emitAuthEvent(AuthLocked)
			return fmt.Errorf("failed to save auth data and recovery failed, please re-unlock with new password: %w", err)
		}
		InitDatabase()
		return fmt.Errorf("failed to save auth data: %w", err)
	}

	// Decrypt with new key to restore working state
	if err := a.decryptConfigFiles(cfg, newKey); err != nil {
		// Files are encrypted with new key and auth.json has new hash.
		// Mark as locked so user must re-unlock with new password.
		zeroBytes(a.encKey)
		a.encKey = nil
		a.unlocked = false
		a.emitAuthEvent(AuthLocked)
		return fmt.Errorf("failed to decrypt files after password change, please re-unlock: %w", err)
	}

	// Re-init DB
	if err := InitDatabase(); err != nil {
		// DB init failed but files are decrypted and auth.json has new hash.
		// Mark as locked so user must re-unlock to get a clean state.
		zeroBytes(a.encKey)
		a.encKey = nil
		a.unlocked = false
		a.emitAuthEvent(AuthLocked)
		return fmt.Errorf("failed to re-initialize database after password change, please re-unlock: %w", err)
	}
	if a.notificationService != nil {
		a.notificationService.LoadSettings()
	}

	// Zero old key, set new
	zeroBytes(a.encKey)
	a.encKey = newKey

	log.Printf("AuthService: Password changed successfully")
	return nil
}

// RemovePassword removes password protection and decrypts files permanently
func (a *AuthService) RemovePassword(ctx context.Context, password string) error {
	a.mutex.Lock()
	defer a.mutex.Unlock()

	if a.authData == nil || !a.authData.Enabled {
		return fmt.Errorf("auth not enabled")
	}

	if !a.unlocked {
		return fmt.Errorf("app must be unlocked to remove password")
	}

	// Verify password
	if !verifyPasswordHash(password, a.authData.PasswordHash) {
		return fmt.Errorf("incorrect password")
	}

	// Files are already decrypted (we're unlocked), just clean up .enc files
	cfg := GetSharedConfig()
	a.cleanupEncryptedFiles(cfg)

	// Remove auth.json
	os.Remove(a.authFilePath)

	// Zero key
	zeroBytes(a.encKey)
	a.encKey = nil

	a.authData = &AuthData{Enabled: false}
	a.unlocked = true

	log.Printf("AuthService: Password removed, auth disabled")
	return nil
}

// SyncAppSettings updates app_settings in auth.json (called when settings change)
func (a *AuthService) SyncAppSettings(settings AppSettings) {
	a.mutex.Lock()
	defer a.mutex.Unlock()

	if a.authData == nil || !a.authData.Enabled {
		return
	}

	a.authData.AppSettings = settings
	a.saveAuthData()
}

// --- Internal methods ---

// recoverFromCrash handles inconsistent state from interrupted encrypt/decrypt.
// If both plaintext and .enc exist for a file:
//   - Auth enabled: keep .enc (encrypted is authoritative), remove plaintext
//   - Auth disabled: keep plaintext, remove .enc
func (a *AuthService) recoverFromCrash(cfg *SharedConfig) {
	files := []string{
		filepath.Join(cfg.ConfigDir, "rclone.conf"),
		filepath.Join(cfg.ConfigDir, "ng-drive.db"),
	}

	for _, basePath := range files {
		encPath := basePath + ".enc"
		plainExists := fileExists(basePath)
		encExists := fileExists(encPath)

		if plainExists && encExists {
			if a.authData != nil && a.authData.Enabled {
				// Auth enabled: encrypted is authoritative, remove partial plaintext
				os.Remove(basePath)
				os.Remove(basePath + "-wal")
				os.Remove(basePath + "-shm")
				log.Printf("AuthService: Crash recovery - removed plaintext %s (keeping .enc)", filepath.Base(basePath))
			} else {
				// Auth disabled: plaintext is authoritative, remove stale .enc
				os.Remove(encPath)
				log.Printf("AuthService: Crash recovery - removed .enc for %s (keeping plaintext)", filepath.Base(basePath))
			}
		}
	}
}

func fileExists(path string) bool {
	_, err := os.Stat(path)
	return err == nil
}

// initializeApp initializes the database, loads settings, and loads rclone config
func (a *AuthService) initializeApp(ctx context.Context) error {
	// Reset shared DB to ensure fresh connection (another service may have
	// opened an empty DB before encrypted files were decrypted)
	ResetSharedDB()

	// Initialize shared SQLite database
	if err := InitDatabase(); err != nil {
		return fmt.Errorf("failed to initialize database: %w", err)
	}

	// Load settings from DB
	if a.notificationService != nil {
		a.notificationService.LoadSettings()
	}

	// Complete App service initialization (rclone config, etc.)
	if a.appService != nil {
		if err := a.appService.CompleteInitialization(ctx); err != nil {
			return fmt.Errorf("failed to complete app initialization: %w", err)
		}
	}

	return nil
}

// lockInternal performs lock without acquiring mutex (caller must hold lock)
func (a *AuthService) lockInternal() {
	if !a.unlocked || a.encKey == nil {
		return
	}

	cfg := GetSharedConfig()

	// Close DB before encrypting
	CloseDatabase()
	ResetSharedDB()

	// Re-encrypt files
	if err := a.encryptConfigFiles(cfg, a.encKey); err != nil {
		log.Printf("AuthService: WARNING - Failed to encrypt files on lock: %v", err)
	}

	// Zero key
	zeroBytes(a.encKey)
	a.encKey = nil
	a.unlocked = false
}

// encryptConfigFiles encrypts rclone.conf and ng-drive.db
func (a *AuthService) encryptConfigFiles(cfg *SharedConfig, key []byte) error {
	filesToEncrypt := []string{
		filepath.Join(cfg.ConfigDir, "rclone.conf"),
		filepath.Join(cfg.ConfigDir, "ng-drive.db"),
	}

	for _, srcPath := range filesToEncrypt {
		if _, err := os.Stat(srcPath); os.IsNotExist(err) {
			continue // Skip if file doesn't exist
		}

		dstPath := srcPath + ".enc"
		if err := encryptFile(srcPath, dstPath, key); err != nil {
			return fmt.Errorf("failed to encrypt %s: %w", filepath.Base(srcPath), err)
		}

		// Remove plaintext
		os.Remove(srcPath)
		// Also remove WAL and SHM files for SQLite
		os.Remove(srcPath + "-wal")
		os.Remove(srcPath + "-shm")
	}

	return nil
}

// decryptConfigFiles decrypts rclone.conf.enc and ng-drive.db.enc
func (a *AuthService) decryptConfigFiles(cfg *SharedConfig, key []byte) error {
	filesToDecrypt := []string{
		filepath.Join(cfg.ConfigDir, "rclone.conf"),
		filepath.Join(cfg.ConfigDir, "ng-drive.db"),
	}

	for _, basePath := range filesToDecrypt {
		encPath := basePath + ".enc"
		if _, err := os.Stat(encPath); os.IsNotExist(err) {
			continue // Skip if encrypted file doesn't exist
		}

		if err := decryptFile(encPath, basePath, key); err != nil {
			return fmt.Errorf("failed to decrypt %s: %w", filepath.Base(encPath), err)
		}

		// Remove encrypted file after successful decryption
		os.Remove(encPath)
	}

	return nil
}

// cleanupEncryptedFiles removes .enc files
func (a *AuthService) cleanupEncryptedFiles(cfg *SharedConfig) {
	encFiles := []string{
		filepath.Join(cfg.ConfigDir, "rclone.conf.enc"),
		filepath.Join(cfg.ConfigDir, "ng-drive.db.enc"),
	}
	for _, f := range encFiles {
		os.Remove(f)
	}
}

// deleteLegacyFiles removes legacy JSON config files
func (a *AuthService) deleteLegacyFiles(cfg *SharedConfig) {
	legacyFiles := []string{
		filepath.Join(cfg.ConfigDir, "profiles.json"),
		filepath.Join(cfg.ConfigDir, "schedules.json"),
		filepath.Join(cfg.ConfigDir, "boards.json"),
		filepath.Join(cfg.ConfigDir, "history.json"),
		filepath.Join(cfg.ConfigDir, "app_settings.json"),
	}
	for _, f := range legacyFiles {
		if _, err := os.Stat(f); err == nil {
			os.Remove(f)
			log.Printf("AuthService: Removed legacy file %s", filepath.Base(f))
		}
	}
}

// loadAuthData reads and parses auth.json
func (a *AuthService) loadAuthData() (*AuthData, error) {
	data, err := os.ReadFile(a.authFilePath)
	if err != nil {
		return nil, err
	}

	var authData AuthData
	if err := json.Unmarshal(data, &authData); err != nil {
		return nil, fmt.Errorf("failed to parse auth.json: %w", err)
	}

	return &authData, nil
}

// saveAuthData writes auth.json
func (a *AuthService) saveAuthData() error {
	data, err := json.MarshalIndent(a.authData, "", "  ")
	if err != nil {
		return fmt.Errorf("failed to marshal auth data: %w", err)
	}

	if err := os.MkdirAll(filepath.Dir(a.authFilePath), 0700); err != nil {
		return fmt.Errorf("failed to create config dir: %w", err)
	}

	if err := os.WriteFile(a.authFilePath, data, 0600); err != nil {
		return fmt.Errorf("failed to write auth.json: %w", err)
	}

	return nil
}

// emitAuthEvent sends an auth event to the frontend
func (a *AuthService) emitAuthEvent(eventType events.EventType) {
	if bus := GetSharedEventBus(); bus != nil {
		bus.Emit(&AuthEvent{
			BaseEvent: events.BaseEvent{
				Type:      eventType,
				Timestamp: time.Now(),
			},
		})
	}
}

// --- Crypto functions ---

// deriveKey derives a 32-byte encryption key from password and salt using Argon2id
func deriveKey(password string, salt []byte) []byte {
	return argon2.IDKey([]byte(password), salt, argon2Iterations, argon2Memory, argon2Parallelism, argon2KeyLen)
}

// encodePasswordHash creates an encoded hash string: argon2id$v=19$m=65536,t=3,p=4$<salt_b64>$<hash_b64>
func encodePasswordHash(password string, salt []byte) string {
	hash := argon2.IDKey([]byte(password), salt, argon2Iterations, argon2Memory, argon2Parallelism, argon2KeyLen)
	saltB64 := base64.RawStdEncoding.EncodeToString(salt)
	hashB64 := base64.RawStdEncoding.EncodeToString(hash)
	return fmt.Sprintf("argon2id$v=19$m=%d,t=%d,p=%d$%s$%s",
		argon2Memory, argon2Iterations, argon2Parallelism, saltB64, hashB64)
}

// verifyPasswordHash verifies a password against an encoded hash
func verifyPasswordHash(password, encoded string) bool {
	salt, err := extractSalt(encoded)
	if err != nil {
		return false
	}

	// Extract the stored hash
	parts := strings.Split(encoded, "$")
	if len(parts) != 5 {
		return false
	}
	storedHash, err := base64.RawStdEncoding.DecodeString(parts[4])
	if err != nil {
		return false
	}

	// Recompute hash
	computedHash := argon2.IDKey([]byte(password), salt, argon2Iterations, argon2Memory, argon2Parallelism, argon2KeyLen)

	return subtle.ConstantTimeCompare(storedHash, computedHash) == 1
}

// extractSalt extracts the salt from an encoded password hash
func extractSalt(encoded string) ([]byte, error) {
	parts := strings.Split(encoded, "$")
	if len(parts) != 5 {
		return nil, fmt.Errorf("invalid hash format")
	}
	return base64.RawStdEncoding.DecodeString(parts[3])
}

// encryptFile encrypts a file using AES-256-GCM
// Output format: [12-byte nonce][ciphertext+GCM tag]
func encryptFile(srcPath, dstPath string, key []byte) error {
	plaintext, err := os.ReadFile(srcPath)
	if err != nil {
		return fmt.Errorf("failed to read file: %w", err)
	}

	block, err := aes.NewCipher(key)
	if err != nil {
		return fmt.Errorf("failed to create cipher: %w", err)
	}

	gcm, err := cipher.NewGCM(block)
	if err != nil {
		return fmt.Errorf("failed to create GCM: %w", err)
	}

	nonce := make([]byte, gcm.NonceSize())
	if _, err := rand.Read(nonce); err != nil {
		return fmt.Errorf("failed to generate nonce: %w", err)
	}

	ciphertext := gcm.Seal(nonce, nonce, plaintext, nil)

	if err := os.WriteFile(dstPath, ciphertext, 0600); err != nil {
		return fmt.Errorf("failed to write encrypted file: %w", err)
	}

	return nil
}

// decryptFile decrypts an AES-256-GCM encrypted file
func decryptFile(srcPath, dstPath string, key []byte) error {
	ciphertext, err := os.ReadFile(srcPath)
	if err != nil {
		return fmt.Errorf("failed to read file: %w", err)
	}

	block, err := aes.NewCipher(key)
	if err != nil {
		return fmt.Errorf("failed to create cipher: %w", err)
	}

	gcm, err := cipher.NewGCM(block)
	if err != nil {
		return fmt.Errorf("failed to create GCM: %w", err)
	}

	nonceSize := gcm.NonceSize()
	if len(ciphertext) < nonceSize {
		return fmt.Errorf("ciphertext too short")
	}

	nonce, ciphertext := ciphertext[:nonceSize], ciphertext[nonceSize:]
	plaintext, err := gcm.Open(nil, nonce, ciphertext, nil)
	if err != nil {
		return fmt.Errorf("decryption failed (wrong password or corrupted data)")
	}

	if err := os.WriteFile(dstPath, plaintext, 0600); err != nil {
		return fmt.Errorf("failed to write decrypted file: %w", err)
	}

	return nil
}

// EncryptData encrypts raw data using AES-256-GCM (used for export encryption)
func EncryptData(data []byte, key []byte) ([]byte, error) {
	block, err := aes.NewCipher(key)
	if err != nil {
		return nil, err
	}

	gcm, err := cipher.NewGCM(block)
	if err != nil {
		return nil, err
	}

	nonce := make([]byte, gcm.NonceSize())
	if _, err := rand.Read(nonce); err != nil {
		return nil, err
	}

	return gcm.Seal(nonce, nonce, data, nil), nil
}

// DecryptData decrypts AES-256-GCM encrypted data (used for export decryption)
func DecryptData(ciphertext []byte, key []byte) ([]byte, error) {
	block, err := aes.NewCipher(key)
	if err != nil {
		return nil, err
	}

	gcm, err := cipher.NewGCM(block)
	if err != nil {
		return nil, err
	}

	nonceSize := gcm.NonceSize()
	if len(ciphertext) < nonceSize {
		return nil, fmt.Errorf("ciphertext too short")
	}

	nonce, ciphertext := ciphertext[:nonceSize], ciphertext[nonceSize:]
	return gcm.Open(nil, nonce, ciphertext, nil)
}

// DeriveExportKey derives an encryption key from an export password.
// Uses 16-byte salt (fits in export header reserved space).
func DeriveExportKey(password string) ([]byte, []byte) {
	salt := make([]byte, 16) // 16 bytes to fit in export header reserved space
	rand.Read(salt)
	key := deriveKey(password, salt)
	return key, salt
}

// DeriveExportKeyWithSalt derives an encryption key from a password and existing salt
func DeriveExportKeyWithSalt(password string, salt []byte) []byte {
	return deriveKey(password, salt)
}

// zeroBytes securely zeros a byte slice
func zeroBytes(b []byte) {
	for i := range b {
		b[i] = 0
	}
}

