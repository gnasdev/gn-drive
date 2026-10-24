//! Master password auth: Argon2id + AES-256-GCM — port of `AuthService.swift`
//! (and `internal/auth/auth.go`). Keeps the same `auth.json` wire format.

use crate::argon2id::{self, Argon2Error};
use crate::crypto::{self, CryptoError};
use crate::keychain::{SecureStore, SecureStoreError};
use crate::logger::Logger;
use chrono::{DateTime, SecondsFormat, Utc};
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

const MAX_ATTEMPTS_BEFORE_DELAY: i64 = 3;
const MAX_ATTEMPTS_BEFORE_LOCK: i64 = 10;
const LOCKOUT_DURATION_SECS: i64 = 5 * 60;

/// Plain config files that get encrypted to `<name>.enc` while locked.
const CONFIG_FILES: [&str; 2] = ["rclone.conf", "gn-drive.db"];
const ENC_FILES: [&str; 2] = ["rclone.conf.enc", "gn-drive.db.enc"];

/// Display texts equal the RPC error messages (`rpcMapError` in Swift).
#[derive(Debug, thiserror::Error)]
pub enum AuthError {
    #[error("auth: no master password is configured")]
    NotSetup,
    #[error("auth: already unlocked")]
    AlreadyUnlocked,
    #[error("auth: app is locked")]
    NotUnlocked,
    #[error("auth: invalid password")]
    InvalidPassword,
    #[error("auth: too many attempts — retry in {retry_after_secs}s")]
    Locked { retry_after_secs: i64 },
    #[error("auth: a master password is already configured")]
    AlreadySetup,
    #[error("auth: password must be at least 4 characters")]
    PasswordTooShort,
    #[error("{0}")]
    Crypto(#[from] CryptoError),
    #[error("{0}")]
    Argon2(#[from] Argon2Error),
    #[error("{0}")]
    KeyStore(#[from] SecureStoreError),
    #[error("{0}")]
    Io(#[from] std::io::Error),
    #[error("{0}")]
    Json(#[from] serde_json::Error),
}

impl AuthError {
    /// Equivalent of Swift `error == AuthError.x` for the payload-free cases.
    pub fn is_invalid_password(&self) -> bool {
        matches!(self, AuthError::InvalidPassword)
    }
}

// Field order is alphabetical so serde_json output has sorted keys like
// Swift's `.sortedKeys`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct AppSettings {
    pub debug_mode: bool,
    pub minimize_to_tray: bool,
    pub notifications_enabled: bool,
    pub start_at_login: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct AuthData {
    pub app_settings: AppSettings,
    pub enabled: bool,
    pub failed_attempts: i64,
    pub lockout_until: String,
    pub password_hash: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LockoutStatus {
    pub failed_attempts: i64,
    pub locked_until: String,
    pub is_locked: bool,
    pub retry_after_secs: i64,
}

#[derive(Debug, Clone, Default)]
pub struct AuthStatus {
    pub setup: bool,
    pub unlocked: bool,
    pub locked_at: Option<DateTime<Utc>>,
    pub enabled: bool,
    pub lockout: LockoutStatus,
}

struct State {
    unlocked: bool,
    enc_key: Option<Vec<u8>>,
    auth_data: AuthData,
    locked_at: Option<DateTime<Utc>>,
}

type SleepFn = Arc<dyn Fn(Duration) + Send + Sync>;

/// Manages password-based unlock, config encryption, and rate limiting.
pub struct AuthService {
    state: Mutex<State>,
    auth_file: PathBuf,
    config_dir: PathBuf,
    logger: Logger,
    key_store: Option<Arc<dyn SecureStore>>,
    key_account: String,
    sleep: Mutex<SleepFn>,
}

impl AuthService {
    pub fn new(
        config_dir: &str,
        logger: &Logger,
        key_store: Option<Arc<dyn SecureStore>>,
    ) -> Result<AuthService, AuthError> {
        if config_dir.is_empty() {
            return Err(AuthError::NotSetup);
        }
        let config_dir_p = PathBuf::from(config_dir);
        let auth_file = config_dir_p.join("auth.json");
        let auth_data = fs::read(&auth_file)
            .ok()
            .and_then(|b| serde_json::from_slice::<AuthData>(&b).ok())
            .unwrap_or_default();

        let svc = AuthService {
            state: Mutex::new(State {
                unlocked: false,
                enc_key: None,
                auth_data,
                locked_at: None,
            }),
            auth_file,
            config_dir: config_dir_p,
            logger: logger.clone(),
            key_store,
            key_account: remembered_key_account(config_dir),
            sleep: Mutex::new(Arc::new(std::thread::sleep)),
        };

        {
            let mut st = svc.state.lock();
            svc.recover_from_crash(&st);
            if !st.auth_data.enabled {
                st.unlocked = true;
                svc.logger.info("auth: not configured, app is open", &[]);
            } else {
                st.unlocked = false;
                svc.resume_remembered_key(&mut st);
                svc.logger.info(
                    if st.unlocked {
                        "auth: resumed from OS keychain"
                    } else {
                        "auth: configured, app is locked"
                    },
                    &[],
                );
            }
        }
        Ok(svc)
    }

    /// Replace the delay sleeper (tests).
    pub fn set_sleep(&self, f: impl Fn(Duration) + Send + Sync + 'static) {
        *self.sleep.lock() = Arc::new(f);
    }

    /// Account name under which the remembered key is stored.
    pub fn key_account(&self) -> &str {
        &self.key_account
    }

    pub fn is_setup(&self) -> bool {
        self.state.lock().auth_data.enabled
    }

    pub fn is_unlocked(&self) -> bool {
        self.state.lock().unlocked
    }

    /// Any config files still encrypted on disk (.enc)?
    pub fn has_encrypted_config(&self) -> bool {
        ENC_FILES.iter().any(|n| self.config_dir.join(n).exists())
    }

    /// Marks the app unlocked without a decryption key. Only valid when no
    /// .enc files exist (config already plaintext — dev restart / crash path).
    pub fn open_plaintext_session(&self) -> Result<(), AuthError> {
        let mut st = self.state.lock();
        if !st.auth_data.enabled || st.unlocked {
            st.unlocked = true;
            return Ok(());
        }
        if self.has_encrypted_config() {
            return Err(AuthError::NotUnlocked);
        }
        st.unlocked = true;
        self.logger
            .info("auth: opened plaintext session (no re-encrypt key)", &[]);
        Ok(())
    }

    pub fn status(&self) -> AuthStatus {
        let st = self.state.lock();
        AuthStatus {
            setup: st.auth_data.enabled,
            unlocked: st.unlocked,
            locked_at: st.locked_at,
            enabled: st.auth_data.enabled,
            lockout: lockout_status_locked(&st.auth_data),
        }
    }

    pub fn lockout_status(&self) -> LockoutStatus {
        lockout_status_locked(&self.state.lock().auth_data)
    }

    pub fn setup_password(&self, password: &str) -> Result<(), AuthError> {
        let mut st = self.state.lock();
        if st.auth_data.enabled {
            return Err(AuthError::AlreadySetup);
        }
        if password.chars().count() < 4 {
            return Err(AuthError::PasswordTooShort);
        }

        let hash = argon2id::create_hash(password)?;
        let salt = argon2id::extract_salt(&hash)?;
        let key = argon2id::derive_key(password, &salt)?;

        st.auth_data = AuthData {
            enabled: true,
            password_hash: hash,
            failed_attempts: 0,
            lockout_until: String::new(),
            app_settings: st.auth_data.app_settings.clone(),
        };
        self.save_auth_data(&st.auth_data)?;
        self.remember_key(&key);
        st.enc_key = Some(key);
        st.unlocked = true;
        self.logger.info("auth: password set up", &[]);
        Ok(())
    }

    pub fn unlock(&self, password: &str) -> Result<(), AuthError> {
        let mut st = self.state.lock();
        if !st.auth_data.enabled {
            return Err(AuthError::NotSetup);
        }
        if st.unlocked {
            return Ok(());
        }

        if let Some(remaining) = active_lockout_remaining(&st.auth_data) {
            return Err(AuthError::Locked {
                retry_after_secs: remaining,
            });
        }
        st.auth_data.lockout_until.clear();

        if st.auth_data.failed_attempts >= MAX_ATTEMPTS_BEFORE_DELAY
            && st.auth_data.failed_attempts < MAX_ATTEMPTS_BEFORE_LOCK
        {
            let delay = 2u64.pow((st.auth_data.failed_attempts - MAX_ATTEMPTS_BEFORE_DELAY) as u32);
            let sleep = self.sleep.lock().clone();
            drop(st);
            sleep(Duration::from_secs(delay));
            st = self.state.lock();
            if st.unlocked {
                return Ok(());
            }
        }

        if !argon2id::verify(password, &st.auth_data.password_hash) {
            st.auth_data.failed_attempts += 1;
            if st.auth_data.failed_attempts >= MAX_ATTEMPTS_BEFORE_LOCK {
                st.auth_data.lockout_until =
                    rfc3339(Utc::now() + chrono::Duration::seconds(LOCKOUT_DURATION_SECS));
                st.auth_data.failed_attempts = 0;
                self.logger
                    .warn("auth: too many failed attempts, locked", &[]);
            }
            let _ = self.save_auth_data(&st.auth_data);
            return Err(AuthError::InvalidPassword);
        }

        let salt = argon2id::extract_salt(&st.auth_data.password_hash)?;
        let key = argon2id::derive_key(password, &salt)?;
        self.decrypt_config_files(&key)?;

        st.auth_data.failed_attempts = 0;
        st.auth_data.lockout_until.clear();
        let _ = self.save_auth_data(&st.auth_data);
        self.remember_key(&key);
        st.enc_key = Some(key);
        st.unlocked = true;
        st.locked_at = None;
        self.logger.info("auth: unlocked", &[]);
        Ok(())
    }

    /// Re-encrypt files, remove remembered key, zero the key.
    pub fn lock(&self) -> Result<(), AuthError> {
        {
            let mut st = self.state.lock();
            if st.unlocked {
                self.lock_internal(&mut st);
            }
        }
        self.forget_remembered_key()
    }

    /// Re-encrypt files before shutdown but keep the remembered key so a later
    /// process owned by the same OS user can resume.
    pub fn suspend(&self) {
        let mut st = self.state.lock();
        if st.unlocked {
            self.lock_internal(&mut st);
        }
    }

    fn lock_internal(&self, st: &mut State) {
        let Some(mut key) = st.enc_key.take() else {
            return;
        };
        if let Err(e) = self.encrypt_config_files(&key) {
            self.logger
                .error_err("auth: encrypt on lock failed", &e, &[]);
        }
        crypto::zero_bytes(&mut key);
        st.unlocked = false;
        st.locked_at = Some(Utc::now());
    }

    pub fn change_password(&self, old: &str, new: &str) -> Result<(), AuthError> {
        let mut st = self.state.lock();
        if !st.auth_data.enabled {
            return Err(AuthError::NotSetup);
        }
        if !st.unlocked {
            return Err(AuthError::NotUnlocked);
        }
        if !argon2id::verify(old, &st.auth_data.password_hash) {
            return Err(AuthError::InvalidPassword);
        }
        if new.chars().count() < 4 {
            return Err(AuthError::PasswordTooShort);
        }
        self.forget_remembered_key()?;

        let new_hash = argon2id::create_hash(new)?;
        let new_salt = argon2id::extract_salt(&new_hash)?;
        let new_key = argon2id::derive_key(new, &new_salt)?;

        if let Err(e) = self.encrypt_config_files(&new_key) {
            // Recover: decrypt with new key.
            if self.decrypt_config_files(&new_key).is_err() {
                if let Some(mut k) = st.enc_key.take() {
                    crypto::zero_bytes(&mut k);
                }
                st.unlocked = false;
            }
            return Err(e);
        }

        let old_hash = std::mem::replace(&mut st.auth_data.password_hash, new_hash);
        if let Err(e) = self.save_auth_data(&st.auth_data) {
            st.auth_data.password_hash = old_hash;
            let _ = self.decrypt_config_files(&new_key);
            return Err(e);
        }

        self.decrypt_config_files(&new_key)?;
        if let Some(mut old_k) = st.enc_key.replace(new_key) {
            crypto::zero_bytes(&mut old_k);
        }
        self.logger.info("auth: password changed", &[]);
        Ok(())
    }

    pub fn remove_password(&self, password: &str) -> Result<(), AuthError> {
        let mut st = self.state.lock();
        if !st.auth_data.enabled {
            return Err(AuthError::NotSetup);
        }
        if !st.unlocked {
            return Err(AuthError::NotUnlocked);
        }
        if !argon2id::verify(password, &st.auth_data.password_hash) {
            return Err(AuthError::InvalidPassword);
        }
        self.cleanup_encrypted_files();
        let _ = fs::remove_file(&self.auth_file);
        if let Some(mut k) = st.enc_key.take() {
            crypto::zero_bytes(&mut k);
        }
        st.auth_data = AuthData::default();
        st.unlocked = true;
        self.logger.info("auth: password removed", &[]);
        Ok(())
    }

    /// Read user-facing settings persisted in auth.json.
    pub fn app_settings(&self) -> AppSettings {
        self.state.lock().auth_data.app_settings.clone()
    }

    pub fn set_app_settings(&self, s: AppSettings) -> Result<(), AuthError> {
        let mut st = self.state.lock();
        st.auth_data.app_settings = s;
        self.save_auth_data(&st.auth_data)
    }

    // --- Internal ----------------------------------------------------------

    fn save_auth_data(&self, data: &AuthData) -> Result<(), AuthError> {
        let json = serde_json::to_vec_pretty(data)?;
        if let Some(dir) = self.auth_file.parent() {
            fs::create_dir_all(dir)?;
        }
        crypto::write_atomic_0600(&self.auth_file, &json)?;
        Ok(())
    }

    /// If both plaintext and .enc exist: auth enabled → keep .enc; disabled →
    /// keep plaintext.
    fn recover_from_crash(&self, st: &State) {
        for name in CONFIG_FILES {
            let base = self.config_dir.join(name);
            let enc = with_suffix(&base, ".enc");
            if !(base.exists() && enc.exists()) {
                continue;
            }
            if st.auth_data.enabled {
                let _ = fs::remove_file(&base);
                let _ = fs::remove_file(with_suffix(&base, "-wal"));
                let _ = fs::remove_file(with_suffix(&base, "-shm"));
                self.logger.warn(
                    "auth: crash recovery - removed plaintext",
                    &[("file", name)],
                );
            } else {
                let _ = fs::remove_file(&enc);
                self.logger
                    .warn("auth: crash recovery - removed .enc", &[("file", name)]);
            }
        }
    }

    fn encrypt_config_files(&self, key: &[u8]) -> Result<(), AuthError> {
        for name in CONFIG_FILES {
            let src = self.config_dir.join(name);
            if !src.exists() {
                continue;
            }
            let enc = with_suffix(&src, ".enc");
            crypto::encrypt_file(&path_str(&src), &path_str(&enc), key)?;
            let _ = fs::remove_file(&src);
            let _ = fs::remove_file(with_suffix(&src, "-wal"));
            let _ = fs::remove_file(with_suffix(&src, "-shm"));
        }
        Ok(())
    }

    fn decrypt_config_files(&self, key: &[u8]) -> Result<(), AuthError> {
        for name in CONFIG_FILES {
            let base = self.config_dir.join(name);
            let enc = with_suffix(&base, ".enc");
            if !enc.exists() {
                continue;
            }
            crypto::decrypt_file(&path_str(&enc), &path_str(&base), key)?;
            let _ = fs::remove_file(&enc);
        }
        Ok(())
    }

    fn cleanup_encrypted_files(&self) {
        for name in ENC_FILES {
            let _ = fs::remove_file(self.config_dir.join(name));
        }
    }

    fn resume_remembered_key(&self, st: &mut State) {
        let Some(ks) = &self.key_store else {
            return;
        };
        if !self.has_encrypted_config() {
            return;
        }
        let Ok(mut key) = ks.get(&self.key_account) else {
            return;
        };
        if key.len() != argon2id::KEY_LENGTH {
            self.logger
                .warn("auth: remembered key has invalid length", &[]);
            let _ = ks.delete(&self.key_account);
            return;
        }
        if let Err(e) = self.decrypt_config_files(&key) {
            crypto::zero_bytes(&mut key);
            self.logger
                .warn_err("auth: remembered key could not decrypt config", &e, &[]);
            let _ = ks.delete(&self.key_account);
            return;
        }
        st.enc_key = Some(key);
        st.unlocked = true;
        st.locked_at = None;
    }

    fn remember_key(&self, key: &[u8]) {
        let Some(ks) = &self.key_store else {
            return;
        };
        if let Err(e) = ks.set(&self.key_account, key) {
            self.logger.warn_err("auth: store remembered key", &e, &[]);
        }
    }

    fn forget_remembered_key(&self) -> Result<(), AuthError> {
        if let Some(ks) = &self.key_store {
            ks.delete(&self.key_account)?;
        }
        Ok(())
    }
}

fn with_suffix(p: &Path, suffix: &str) -> PathBuf {
    let mut s = p.as_os_str().to_os_string();
    s.push(suffix);
    PathBuf::from(s)
}

fn path_str(p: &Path) -> String {
    p.to_string_lossy().into_owned()
}

fn lockout_status_locked(d: &AuthData) -> LockoutStatus {
    let mut status = LockoutStatus {
        failed_attempts: d.failed_attempts,
        locked_until: d.lockout_until.clone(),
        ..Default::default()
    };
    if let Some(remaining) = active_lockout_remaining(d) {
        status.is_locked = true;
        status.retry_after_secs = remaining;
    }
    if !status.is_locked && d.failed_attempts >= MAX_ATTEMPTS_BEFORE_DELAY {
        status.retry_after_secs = 2i64.pow((d.failed_attempts - MAX_ATTEMPTS_BEFORE_DELAY) as u32);
    }
    status
}

/// `Some(ceil seconds remaining)` if `lockout_until` parses and is in the future.
fn active_lockout_remaining(d: &AuthData) -> Option<i64> {
    if d.lockout_until.is_empty() {
        return None;
    }
    let t = parse_rfc3339(&d.lockout_until)?;
    let ms = (t - Utc::now()).num_milliseconds();
    if ms <= 0 {
        return None;
    }
    Some((ms + 999) / 1000)
}

/// RFC3339, second precision, `Z` suffix (ISO8601DateFormatter internet date-time).
pub fn rfc3339(d: DateTime<Utc>) -> String {
    d.to_rfc3339_opts(SecondsFormat::Secs, true)
}

pub fn parse_rfc3339(s: &str) -> Option<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(s)
        .ok()
        .map(|d| d.with_timezone(&Utc))
}

/// Port of `NSString.standardizingPath` for the cases that matter here:
/// expands a leading `~`, collapses `//` and `.`, resolves `..` in absolute
/// paths, strips the trailing slash, and on macOS strips a leading `/private`
/// when the remainder starts with `/var`, `/tmp` or `/etc` and exists.
pub fn standardizing_path(path: &str) -> String {
    if path.is_empty() {
        return String::new();
    }
    let mut p = path.to_string();
    if p == "~" || p.starts_with("~/") {
        let home = std::env::var("HOME").unwrap_or_default();
        p = format!("{home}{}", &p[1..]);
    }
    let absolute = p.starts_with('/');
    let mut out: Vec<&str> = Vec::new();
    for c in p.split('/') {
        match c {
            "" | "." => {}
            ".." if absolute => {
                out.pop();
            }
            other => out.push(other),
        }
    }
    let mut s = if absolute {
        format!("/{}", out.join("/"))
    } else {
        out.join("/")
    };
    if cfg!(target_os = "macos")
        && let Some(rest) = s.strip_prefix("/private")
        && ["/var", "/tmp", "/etc"]
            .iter()
            .any(|p| rest == *p || rest.starts_with(&format!("{p}/")))
        && Path::new(rest).exists()
    {
        s = rest.to_string();
    }
    s
}

/// `remember-unlock/v1/` + sha256 hex of the standardized config dir.
pub fn remembered_key_account(config_dir: &str) -> String {
    let canonical = standardizing_path(config_dir);
    format!(
        "remember-unlock/v1/{}",
        hex::encode(Sha256::digest(canonical.as_bytes()))
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::keychain::MemorySecureStore;
    use std::sync::OnceLock;

    fn log() -> Logger {
        Logger::quiet()
    }

    fn dir() -> tempfile::TempDir {
        tempfile::tempdir().unwrap()
    }

    fn d(t: &tempfile::TempDir) -> String {
        t.path().to_string_lossy().into_owned()
    }

    fn mem() -> Arc<MemorySecureStore> {
        Arc::new(MemorySecureStore::new())
    }

    fn new_svc(t: &tempfile::TempDir, ks: Option<Arc<MemorySecureStore>>) -> AuthService {
        let svc = AuthService::new(&d(t), &log(), ks.map(|k| k as Arc<dyn SecureStore>)).unwrap();
        svc.set_sleep(|_| {});
        svc
    }

    /// One shared real hash of "secret" (hashing is ~100ms each).
    fn shared_hash() -> &'static str {
        static H: OnceLock<String> = OnceLock::new();
        H.get_or_init(|| argon2id::create_hash("secret").unwrap())
    }

    fn write_auth(t: &tempfile::TempDir, data: &AuthData) {
        fs::write(
            t.path().join("auth.json"),
            serde_json::to_vec_pretty(data).unwrap(),
        )
        .unwrap();
    }

    fn seeded(failed: i64, lockout_until: &str) -> AuthData {
        AuthData {
            enabled: true,
            password_hash: shared_hash().to_string(),
            failed_attempts: failed,
            lockout_until: lockout_until.to_string(),
            ..Default::default()
        }
    }

    fn read(t: &tempfile::TempDir, n: &str) -> String {
        fs::read_to_string(t.path().join(n)).unwrap()
    }

    fn exists(t: &tempfile::TempDir, n: &str) -> bool {
        t.path().join(n).exists()
    }

    const RC: &str = "[gdrive]\ntype = drive\n";

    #[test]
    fn requires_config_dir() {
        assert!(matches!(
            AuthService::new("", &log(), None),
            Err(AuthError::NotSetup)
        ));
    }

    #[test]
    fn first_run_is_open() {
        let t = dir();
        let s = new_svc(&t, None);
        assert!(!s.is_setup());
        assert!(s.is_unlocked());
        let st = s.status();
        assert!(!st.setup && st.unlocked && !st.enabled && st.locked_at.is_none());
        assert_eq!(s.lockout_status(), LockoutStatus::default());
    }

    #[test]
    fn error_messages_match_rpc_texts() {
        assert_eq!(
            AuthError::NotSetup.to_string(),
            "auth: no master password is configured"
        );
        assert_eq!(
            AuthError::AlreadySetup.to_string(),
            "auth: a master password is already configured"
        );
        assert_eq!(AuthError::NotUnlocked.to_string(), "auth: app is locked");
        assert_eq!(
            AuthError::InvalidPassword.to_string(),
            "auth: invalid password"
        );
        assert_eq!(
            AuthError::AlreadyUnlocked.to_string(),
            "auth: already unlocked"
        );
        assert_eq!(
            AuthError::PasswordTooShort.to_string(),
            "auth: password must be at least 4 characters"
        );
        assert_eq!(
            AuthError::Locked {
                retry_after_secs: 42
            }
            .to_string(),
            "auth: too many attempts — retry in 42s"
        );
    }

    #[test]
    fn setup_lock_unlock_change_remove_flow() {
        let t = dir();
        fs::write(t.path().join("rclone.conf"), RC).unwrap();
        fs::write(t.path().join("gn-drive.db"), b"dbbytes").unwrap();
        fs::write(t.path().join("gn-drive.db-wal"), b"wal").unwrap();
        fs::write(t.path().join("gn-drive.db-shm"), b"shm").unwrap();
        let ks = mem();
        let s = new_svc(&t, Some(ks.clone()));

        assert!(matches!(
            s.setup_password("abc"),
            Err(AuthError::PasswordTooShort)
        ));
        s.setup_password("testpass").unwrap();
        assert!(s.is_setup() && s.is_unlocked());
        assert!(matches!(
            s.setup_password("another"),
            Err(AuthError::AlreadySetup)
        ));
        // key remembered, raw 32 bytes
        assert_eq!(ks.get(s.key_account()).unwrap().len(), 32);
        // auth.json 0600
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let m = fs::metadata(t.path().join("auth.json"))
                .unwrap()
                .permissions()
                .mode();
            assert_eq!(m & 0o777, 0o600);
        }

        s.lock().unwrap();
        assert!(!s.is_unlocked());
        assert!(s.status().locked_at.is_some());
        assert!(exists(&t, "rclone.conf.enc") && exists(&t, "gn-drive.db.enc"));
        assert!(!exists(&t, "rclone.conf") && !exists(&t, "gn-drive.db"));
        assert!(!exists(&t, "gn-drive.db-wal") && !exists(&t, "gn-drive.db-shm"));
        assert!(s.has_encrypted_config());
        // lock() forgets remembered key
        assert_eq!(ks.get(s.key_account()), Err(SecureStoreError::NotFound));

        assert!(matches!(s.unlock("nope"), Err(AuthError::InvalidPassword)));
        assert_eq!(s.lockout_status().failed_attempts, 1);
        s.unlock("testpass").unwrap();
        assert!(s.is_unlocked() && s.status().locked_at.is_none());
        assert_eq!(read(&t, "rclone.conf"), RC);
        assert_eq!(read(&t, "gn-drive.db"), "dbbytes");
        assert!(!s.has_encrypted_config());
        assert_eq!(s.lockout_status().failed_attempts, 0);
        s.unlock("whatever").unwrap(); // already unlocked → noop

        // change password
        assert!(matches!(
            s.change_password("bad", "newpass"),
            Err(AuthError::InvalidPassword)
        ));
        assert!(matches!(
            s.change_password("testpass", "ab"),
            Err(AuthError::PasswordTooShort)
        ));
        s.change_password("testpass", "newpass").unwrap();
        assert!(s.is_unlocked());
        s.lock().unwrap();
        assert!(matches!(
            s.unlock("testpass"),
            Err(AuthError::InvalidPassword)
        ));
        s.unlock("newpass").unwrap();
        assert_eq!(read(&t, "rclone.conf"), RC);

        // remove password
        assert!(matches!(
            s.remove_password("bad"),
            Err(AuthError::InvalidPassword)
        ));
        s.remove_password("newpass").unwrap();
        assert!(!s.is_setup() && s.is_unlocked());
        assert!(!exists(&t, "auth.json"));
        assert!(!s.has_encrypted_config());
        assert_eq!(read(&t, "rclone.conf"), RC);
    }

    #[test]
    fn state_guards_not_setup_and_locked() {
        let t = dir();
        let s = new_svc(&t, None);
        assert!(matches!(s.unlock("x"), Err(AuthError::NotSetup)));
        assert!(matches!(
            s.change_password("a", "bbbb"),
            Err(AuthError::NotSetup)
        ));
        assert!(matches!(s.remove_password("a"), Err(AuthError::NotSetup)));
        s.lock().unwrap(); // noop when not set up

        write_auth(&t, &seeded(0, ""));
        let s = new_svc(&t, None);
        assert!(!s.is_unlocked());
        assert!(matches!(
            s.change_password("secret", "bbbb"),
            Err(AuthError::NotUnlocked)
        ));
        assert!(matches!(
            s.remove_password("secret"),
            Err(AuthError::NotUnlocked)
        ));
        s.lock().unwrap(); // noop when locked
    }

    #[test]
    fn lockout_counting_with_fake_sleep() {
        let t = dir();
        write_auth(&t, &seeded(0, ""));
        let s = AuthService::new(&d(&t), &log(), None).unwrap();
        let sleeps = Arc::new(Mutex::new(Vec::<u64>::new()));
        let sl = sleeps.clone();
        s.set_sleep(move |dur| sl.lock().push(dur.as_secs()));

        // Attempts 1..=3: no delay. Attempt n+1 (n>=3) sleeps 2^(n-3).
        for i in 1..=3 {
            assert!(matches!(s.unlock("wrong"), Err(AuthError::InvalidPassword)));
            assert_eq!(s.lockout_status().failed_attempts, i);
        }
        assert!(sleeps.lock().is_empty());
        assert_eq!(s.lockout_status().retry_after_secs, 1);
        for _ in 4..=10 {
            assert!(matches!(s.unlock("wrong"), Err(AuthError::InvalidPassword)));
        }
        assert_eq!(*sleeps.lock(), vec![1, 2, 4, 8, 16, 32, 64]);

        // 10th failure → lockout, counter reset.
        let ls = s.lockout_status();
        assert!(ls.is_locked);
        assert_eq!(ls.failed_attempts, 0);
        assert!(
            ls.retry_after_secs > 290 && ls.retry_after_secs <= 300,
            "{ls:?}"
        );
        assert!(!ls.locked_until.is_empty());
        assert!(s.status().lockout.is_locked);
        // Persisted
        let disk: AuthData = serde_json::from_str(&read(&t, "auth.json")).unwrap();
        assert_eq!(disk.failed_attempts, 0);
        assert_eq!(disk.lockout_until, ls.locked_until);

        // Even the right password is refused while locked, with no extra sleep.
        match s.unlock("secret") {
            Err(AuthError::Locked { retry_after_secs }) => assert!(retry_after_secs > 290),
            other => panic!("expected Locked, got {other:?}"),
        }
        assert_eq!(sleeps.lock().len(), 7);
    }

    #[test]
    fn expired_lockout_is_cleared_and_bad_format_ignored() {
        let t = dir();
        let past = rfc3339(Utc::now() - chrono::Duration::seconds(5));
        write_auth(&t, &seeded(0, &past));
        let s = new_svc(&t, None);
        assert!(!s.lockout_status().is_locked);
        s.unlock("secret").unwrap();
        assert!(s.is_unlocked());
        let disk: AuthData = serde_json::from_str(&read(&t, "auth.json")).unwrap();
        assert_eq!(disk.lockout_until, "");

        let t2 = dir();
        write_auth(&t2, &seeded(0, "not-a-date"));
        let s2 = new_svc(&t2, None);
        let ls = s2.lockout_status();
        assert!(!ls.is_locked);
        assert_eq!(ls.locked_until, "not-a-date");
        s2.unlock("secret").unwrap();
    }

    #[test]
    fn delay_status_and_abort_if_unlocked_during_sleep() {
        let t = dir();
        write_auth(&t, &seeded(5, ""));
        let s = Arc::new(AuthService::new(&d(&t), &log(), None).unwrap());
        assert_eq!(s.lockout_status().retry_after_secs, 4);
        // During the (fake) sleep, someone else unlocks.
        let s2 = s.clone();
        s.set_sleep(move |dur| {
            assert_eq!(dur.as_secs(), 4);
            s2.open_plaintext_session().unwrap();
        });
        s.unlock("wrong-but-never-checked").unwrap();
        assert!(s.is_unlocked());
        assert_eq!(s.lockout_status().failed_attempts, 5);
    }

    #[test]
    fn active_lockout_future_rfc3339_with_offset() {
        let t = dir();
        let fut = (Utc::now() + chrono::Duration::seconds(7200))
            .with_timezone(&chrono::FixedOffset::east_opt(7 * 3600).unwrap())
            .to_rfc3339_opts(SecondsFormat::Secs, false);
        write_auth(&t, &seeded(0, &fut));
        let s = new_svc(&t, None);
        let ls = s.lockout_status();
        assert!(ls.is_locked && ls.retry_after_secs > 3600 && ls.retry_after_secs <= 7200);
    }

    #[test]
    fn swift_written_auth_json_decodes() {
        // Exact shape produced by Swift `saveAuthData` (.prettyPrinted + .sortedKeys).
        let json = r#"{
  "app_settings" : {
    "debug_mode" : true,
    "minimize_to_tray" : false,
    "notifications_enabled" : true,
    "start_at_login" : true
  },
  "enabled" : true,
  "failed_attempts" : 2,
  "lockout_until" : "2026-01-02T03:04:05Z",
  "password_hash" : "$argon2id$v=19$m=65536,t=3,p=4$MDEyMzQ1Njc4OWFiY2RlZjAxMjM0NTY3ODlhYmNkZWY$8NzLqIEkiqq9xKPWRlUxhDrI+gvfaXSzhbPDZPwSu70"
}"#;
        let t = dir();
        fs::write(t.path().join("auth.json"), json).unwrap();
        let s = new_svc(&t, None);
        assert!(s.is_setup() && !s.is_unlocked());
        let a = s.app_settings();
        assert_eq!(
            a,
            AppSettings {
                notifications_enabled: true,
                debug_mode: true,
                minimize_to_tray: false,
                start_at_login: true
            }
        );
        let ls = s.lockout_status();
        assert_eq!(ls.failed_attempts, 2);
        assert_eq!(ls.locked_until, "2026-01-02T03:04:05Z");
        assert!(!ls.is_locked); // in the past
        // The vector hash is verifiable cross-implementation.
        s.unlock("correct horse battery").unwrap();
        assert!(s.is_unlocked());
    }

    #[test]
    fn auth_json_written_sorted_pretty_with_exact_keys() {
        let t = dir();
        let s = new_svc(&t, None);
        s.setup_password("pw-1234").unwrap();
        s.set_app_settings(AppSettings {
            debug_mode: true,
            ..Default::default()
        })
        .unwrap();
        let text = read(&t, "auth.json");
        let v: serde_json::Value = serde_json::from_str(&text).unwrap();
        let keys: Vec<&String> = v.as_object().unwrap().keys().collect();
        assert_eq!(
            keys,
            [
                "app_settings",
                "enabled",
                "failed_attempts",
                "lockout_until",
                "password_hash"
            ]
        );
        let akeys: Vec<&String> = v["app_settings"].as_object().unwrap().keys().collect();
        assert_eq!(
            akeys,
            [
                "debug_mode",
                "minimize_to_tray",
                "notifications_enabled",
                "start_at_login"
            ]
        );
        assert!(text.contains("\n  \"enabled\": true"));
        assert_eq!(v["app_settings"]["debug_mode"], true);
        assert!(
            v["password_hash"]
                .as_str()
                .unwrap()
                .starts_with("$argon2id$v=19$")
        );
    }

    #[test]
    fn invalid_json_means_open() {
        let t = dir();
        fs::write(t.path().join("auth.json"), "{not json").unwrap();
        let s = new_svc(&t, None);
        assert!(!s.is_setup() && s.is_unlocked());
    }

    #[test]
    fn app_settings_preserved_on_setup() {
        let t = dir();
        let s = new_svc(&t, None);
        s.set_app_settings(AppSettings {
            start_at_login: true,
            ..Default::default()
        })
        .unwrap();
        s.setup_password("abcd").unwrap();
        assert!(s.app_settings().start_at_login);
        let s2 = new_svc(&t, None);
        assert!(s2.app_settings().start_at_login);
    }

    #[test]
    fn crash_recovery_enabled_keeps_enc() {
        let t = dir();
        write_auth(&t, &seeded(0, ""));
        for (n, c) in [
            ("rclone.conf", "plain"),
            ("rclone.conf.enc", "enc"),
            ("gn-drive.db", "p"),
            ("gn-drive.db.enc", "e"),
            ("gn-drive.db-wal", "w"),
            ("gn-drive.db-shm", "s"),
        ] {
            fs::write(t.path().join(n), c).unwrap();
        }
        let s = new_svc(&t, None);
        assert!(!s.is_unlocked());
        assert!(!exists(&t, "rclone.conf") && !exists(&t, "gn-drive.db"));
        assert!(!exists(&t, "gn-drive.db-wal") && !exists(&t, "gn-drive.db-shm"));
        assert_eq!(read(&t, "rclone.conf.enc"), "enc");
        assert_eq!(read(&t, "gn-drive.db.enc"), "e");
    }

    #[test]
    fn crash_recovery_disabled_keeps_plaintext() {
        let t = dir();
        for (n, c) in [
            ("rclone.conf", "plain"),
            ("rclone.conf.enc", "enc"),
            ("gn-drive.db", "p"),
            ("gn-drive.db.enc", "e"),
        ] {
            fs::write(t.path().join(n), c).unwrap();
        }
        let s = new_svc(&t, None);
        assert!(s.is_unlocked());
        assert_eq!(read(&t, "rclone.conf"), "plain");
        assert_eq!(read(&t, "gn-drive.db"), "p");
        assert!(!exists(&t, "rclone.conf.enc") && !exists(&t, "gn-drive.db.enc"));
    }

    #[test]
    fn remembered_key_resumes_after_suspend() {
        let t = dir();
        fs::write(t.path().join("rclone.conf"), RC).unwrap();
        let ks = mem();
        {
            let s = new_svc(&t, Some(ks.clone()));
            s.setup_password("testpass").unwrap();
            s.suspend();
            assert!(!s.is_unlocked());
            assert!(exists(&t, "rclone.conf.enc") && !exists(&t, "rclone.conf"));
            // key still remembered after suspend
            assert_eq!(ks.get(s.key_account()).unwrap().len(), 32);
        }
        let s2 = new_svc(&t, Some(ks.clone()));
        assert!(s2.is_unlocked());
        assert_eq!(read(&t, "rclone.conf"), RC);
        assert!(!exists(&t, "rclone.conf.enc"));
        // Resumed service can lock again with the resumed key.
        s2.lock().unwrap();
        assert!(exists(&t, "rclone.conf.enc"));
        assert_eq!(ks.get(s2.key_account()), Err(SecureStoreError::NotFound));
        let s3 = new_svc(&t, Some(ks));
        assert!(!s3.is_unlocked());
    }

    #[test]
    fn remembered_key_not_used_without_encrypted_config() {
        let t = dir();
        write_auth(&t, &seeded(0, ""));
        let ks = mem();
        ks.set(&remembered_key_account(&d(&t)), &[7u8; 32]).unwrap();
        let s = new_svc(&t, Some(ks.clone()));
        assert!(!s.is_unlocked());
        // key untouched
        assert!(ks.get(s.key_account()).is_ok());
    }

    #[test]
    fn remembered_key_bad_length_is_deleted() {
        let t = dir();
        write_auth(&t, &seeded(0, ""));
        fs::write(t.path().join("rclone.conf.enc"), vec![0u8; 64]).unwrap();
        let ks = mem();
        ks.set(&remembered_key_account(&d(&t)), &[1u8; 5]).unwrap();
        let s = new_svc(&t, Some(ks.clone()));
        assert!(!s.is_unlocked());
        assert_eq!(ks.get(s.key_account()), Err(SecureStoreError::NotFound));
    }

    #[test]
    fn remembered_key_wrong_key_is_deleted() {
        let t = dir();
        write_auth(&t, &seeded(0, ""));
        // Encrypt with a different key.
        let other = [9u8; 32];
        fs::write(t.path().join("tmp"), "x").unwrap();
        crypto::encrypt_file(
            &format!("{}/tmp", d(&t)),
            &format!("{}/rclone.conf.enc", d(&t)),
            &other,
        )
        .unwrap();
        let ks = mem();
        ks.set(&remembered_key_account(&d(&t)), &[1u8; 32]).unwrap();
        let s = new_svc(&t, Some(ks.clone()));
        assert!(!s.is_unlocked());
        assert_eq!(ks.get(s.key_account()), Err(SecureStoreError::NotFound));
        assert!(exists(&t, "rclone.conf.enc"));
    }

    #[test]
    fn open_plaintext_session_rules() {
        let t = dir();
        write_auth(&t, &seeded(0, ""));
        let s = new_svc(&t, None);
        assert!(!s.is_unlocked());
        s.open_plaintext_session().unwrap();
        assert!(s.is_unlocked());

        let t2 = dir();
        write_auth(&t2, &seeded(0, ""));
        fs::write(t2.path().join("gn-drive.db.enc"), b"x").unwrap();
        let s2 = new_svc(&t2, None);
        assert!(matches!(
            s2.open_plaintext_session(),
            Err(AuthError::NotUnlocked)
        ));
        assert!(!s2.is_unlocked());
    }

    #[test]
    fn unlock_with_corrupt_enc_fails_and_stays_locked() {
        let t = dir();
        write_auth(&t, &seeded(0, ""));
        fs::write(t.path().join("rclone.conf.enc"), vec![1u8; 64]).unwrap();
        let s = new_svc(&t, None);
        assert!(matches!(
            s.unlock("secret"),
            Err(AuthError::Crypto(CryptoError::DecryptionFailed))
        ));
        assert!(!s.is_unlocked());
        assert!(exists(&t, "rclone.conf.enc"));
    }

    #[test]
    fn standardizing_path_cases() {
        assert_eq!(standardizing_path("/tmp/a/../b//c/"), "/tmp/b/c");
        assert_eq!(standardizing_path("/a/./b"), "/a/b");
        assert_eq!(standardizing_path("/../a"), "/a");
        assert_eq!(standardizing_path("/"), "/");
        assert_eq!(standardizing_path(""), "");
        assert_eq!(standardizing_path("a/./b"), "a/b");
        assert_eq!(standardizing_path("rel/../../x"), "rel/../../x");
        assert_eq!(standardizing_path("/private/foo"), "/private/foo");
        assert_eq!(standardizing_path("/private/varx"), "/private/varx");
        assert_eq!(
            standardizing_path("/private/nonexistent-zz"),
            "/private/nonexistent-zz"
        );
        if let Ok(home) = std::env::var("HOME") {
            assert_eq!(standardizing_path("~/x"), format!("{home}/x"));
            assert_eq!(standardizing_path("~"), home);
        }
        if cfg!(target_os = "macos") {
            // Verified against NSString.standardizingPath on macOS.
            assert_eq!(standardizing_path("/private/var/folders"), "/var/folders");
            assert_eq!(standardizing_path("/private/var"), "/var");
            assert_eq!(standardizing_path("/private/etc/hosts"), "/etc/hosts");
            assert_eq!(standardizing_path("/private/tmp"), "/tmp");
            assert_eq!(
                standardizing_path("/private/tmp/gnd-nonexistent-zz"),
                "/private/tmp/gnd-nonexistent-zz"
            );
        }
    }

    #[test]
    fn remembered_key_account_format() {
        let a = remembered_key_account("/tmp/x/../y/");
        let expect = format!(
            "remember-unlock/v1/{}",
            hex::encode(Sha256::digest(b"/tmp/y"))
        );
        assert_eq!(a, expect);
        assert_eq!(a.len(), "remember-unlock/v1/".len() + 64);
        assert_eq!(remembered_key_account("/tmp/y//"), a);
    }

    #[test]
    fn rfc3339_roundtrip_second_precision() {
        let now = Utc::now();
        let s = rfc3339(now);
        assert!(s.ends_with('Z') && s.len() == 20, "{s}");
        assert_eq!(parse_rfc3339(&s).unwrap().timestamp(), now.timestamp());
        assert!(parse_rfc3339("2026-01-02T03:04:05+07:00").is_some());
        assert!(parse_rfc3339("garbage").is_none());
    }
}
