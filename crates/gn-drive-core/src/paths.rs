//! Platform and path detection (port of `Paths.swift`).

use std::collections::HashMap;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppEnv {
    Development,
    Production,
}

#[derive(Debug, Clone)]
pub struct Paths {
    /// User's home directory (`~`).
    pub home_dir: String,
    /// `~/.config/gn-drive` (override: `GN_DRIVE_CONFIG_DIR`).
    pub config_dir: String,
    /// Log directory (== config_dir).
    pub log_dir: String,
    /// Directory containing the running binary.
    pub working_dir: String,
    pub env: AppEnv,
}

fn home_from(env: &HashMap<String, String>) -> String {
    if let Some(h) = env.get("HOME").filter(|h| !h.is_empty()) {
        return h.clone();
    }
    // SAFETY: getpwuid returns a pointer to static storage or null.
    unsafe {
        let pw = libc::getpwuid(libc::getuid());
        if !pw.is_null() && !(*pw).pw_dir.is_null() {
            let s = std::ffi::CStr::from_ptr((*pw).pw_dir)
                .to_string_lossy()
                .into_owned();
            if !s.is_empty() {
                return s;
            }
        }
    }
    "/tmp".to_string()
}

impl Paths {
    /// Detect from the process environment.
    pub fn detect() -> Paths {
        Self::detect_with(&std::env::vars().collect())
    }

    /// Detect from an explicit environment map (tests).
    pub fn detect_with(env: &HashMap<String, String>) -> Paths {
        let home = home_from(env);
        let mut cfg = Path::new(&home)
            .join(".config/gn-drive")
            .to_string_lossy()
            .into_owned();
        if let Some(o) = env.get("GN_DRIVE_CONFIG_DIR").filter(|o| !o.is_empty()) {
            cfg = o.clone();
        }
        let work_dir = std::env::current_exe()
            .ok()
            .and_then(|p| p.parent().map(|p| p.to_string_lossy().into_owned()))
            .unwrap_or_default();
        let app_env = if env.contains_key("GN_DRIVE_DEV")
            || work_dir.ends_with("/gn-drive/bin")
            || work_dir.contains("/desktop/bin")
        {
            AppEnv::Development
        } else {
            AppEnv::Production
        };
        Paths {
            home_dir: home,
            log_dir: cfg.clone(),
            config_dir: cfg,
            working_dir: work_dir,
            env: app_env,
        }
    }

    /// Create the config dir (mode 0700).
    pub fn ensure_config_dir(&self) -> std::io::Result<()> {
        use std::os::unix::fs::PermissionsExt;
        std::fs::create_dir_all(&self.config_dir)?;
        std::fs::set_permissions(&self.config_dir, std::fs::Permissions::from_mode(0o700))
    }

    pub fn config_path(&self, name: &str) -> PathBuf {
        Path::new(&self.config_dir).join(name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_dir_override() {
        let mut env = HashMap::new();
        env.insert("HOME".to_string(), "/home/x".to_string());
        assert_eq!(
            Paths::detect_with(&env).config_dir,
            "/home/x/.config/gn-drive"
        );
        env.insert("GN_DRIVE_CONFIG_DIR".to_string(), "/tmp/cfg".to_string());
        let p = Paths::detect_with(&env);
        assert_eq!(p.config_dir, "/tmp/cfg");
        assert_eq!(p.log_dir, "/tmp/cfg");
        env.insert("GN_DRIVE_CONFIG_DIR".to_string(), String::new());
        assert_eq!(
            Paths::detect_with(&env).config_dir,
            "/home/x/.config/gn-drive"
        );
    }

    #[test]
    fn dev_env_flag() {
        let mut env = HashMap::new();
        env.insert("HOME".to_string(), "/h".to_string());
        env.insert("GN_DRIVE_DEV".to_string(), "1".to_string());
        assert_eq!(Paths::detect_with(&env).env, AppEnv::Development);
    }

    #[test]
    fn ensure_creates_0700() {
        use std::os::unix::fs::PermissionsExt;
        let t = tempfile::tempdir().unwrap();
        let mut env = HashMap::new();
        env.insert(
            "GN_DRIVE_CONFIG_DIR".to_string(),
            t.path().join("a/b").to_string_lossy().into_owned(),
        );
        let p = Paths::detect_with(&env);
        p.ensure_config_dir().unwrap();
        let mode = std::fs::metadata(&p.config_dir)
            .unwrap()
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(mode, 0o700);
    }
}
