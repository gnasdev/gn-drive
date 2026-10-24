//! Single-instance advisory locking (port of `InstanceLock.swift`).
//!
//! Lock file: `<configDir>/gn-drive.lock` (flock); PID file: `gn-drive.pid`.

use parking_lot::Mutex;
use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::os::fd::AsRawFd;
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};

#[derive(Debug, thiserror::Error)]
pub enum InstanceError {
    #[error("{}", another_instance_text(*.pid))]
    AnotherInstance { pid: Option<i32> },
    #[error("{0}")]
    Io(String),
}

fn another_instance_text(pid: Option<i32>) -> String {
    match pid {
        Some(pid) => format!(
            "another gn-drive instance is running (pid={pid}). Run 'gn-drive service stop' to stop it, or kill the process manually"
        ),
        None => "another gn-drive instance is running. Run 'gn-drive service stop' to stop it, or kill the process manually".to_string(),
    }
}

impl From<std::io::Error> for InstanceError {
    fn from(e: std::io::Error) -> Self {
        InstanceError::Io(e.to_string())
    }
}

pub struct InstanceLocker {
    /// `None` once released.
    file: Mutex<Option<File>>,
    pid_file: PathBuf,
    pid: i32,
}

impl InstanceLocker {
    /// Take an exclusive non-blocking advisory lock on the config dir.
    pub fn acquire(config_dir: &str) -> Result<InstanceLocker, InstanceError> {
        fs::create_dir_all(config_dir)?;
        let lock_path = Path::new(config_dir).join("gn-drive.lock");
        let pid_path = Path::new(config_dir).join("gn-drive.pid");

        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .mode(0o600)
            .open(&lock_path)?;
        // SAFETY: valid open fd owned by `file`.
        if unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } != 0 {
            drop(file);
            let existing = read_pid(&pid_path);
            return Err(InstanceError::AnotherInstance {
                pid: (existing > 0).then_some(existing),
            });
        }
        let pid = std::process::id() as i32;
        let tmp = pid_path.with_extension("pid.tmp");
        {
            let mut f = File::create(&tmp)?;
            f.write_all(pid.to_string().as_bytes())?;
        }
        let _ = fs::set_permissions(&tmp, fs::Permissions::from_mode(0o644));
        fs::rename(&tmp, &pid_path)?;
        Ok(InstanceLocker {
            file: Mutex::new(Some(file)),
            pid_file: pid_path,
            pid,
        })
    }

    pub fn pid_value(&self) -> i32 {
        self.pid
    }

    /// Release the lock and remove the pid file if it still names this process. Idempotent.
    pub fn release(&self) {
        let mut guard = self.file.lock();
        let Some(file) = guard.take() else { return };
        if read_pid(&self.pid_file) == self.pid {
            let _ = fs::remove_file(&self.pid_file);
        }
        // SAFETY: valid fd; closing happens when `file` drops.
        unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_UN) };
    }
}

impl Drop for InstanceLocker {
    fn drop(&mut self) {
        self.release();
    }
}

fn read_pid(path: &Path) -> i32 {
    fs::read_to_string(path)
        .ok()
        .and_then(|s| s.trim().parse::<i32>().ok())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dir() -> tempfile::TempDir {
        tempfile::tempdir().unwrap()
    }

    #[test]
    fn acquire_writes_pid_and_release_removes_it() {
        let d = dir();
        let cfg = d.path().to_str().unwrap();
        let l = InstanceLocker::acquire(cfg).unwrap();
        assert_eq!(l.pid_value(), std::process::id() as i32);
        let pid_path = d.path().join("gn-drive.pid");
        assert_eq!(
            fs::read_to_string(&pid_path).unwrap(),
            std::process::id().to_string()
        );
        assert_eq!(
            fs::metadata(&pid_path).unwrap().permissions().mode() & 0o777,
            0o644
        );
        assert_eq!(
            fs::metadata(d.path().join("gn-drive.lock"))
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o600
        );
        l.release();
        l.release(); // idempotent
        assert!(!pid_path.exists());
    }

    #[test]
    fn second_locker_conflicts_and_reports_holder_pid() {
        let d = dir();
        let cfg = d.path().to_str().unwrap();
        let first = InstanceLocker::acquire(cfg).unwrap();
        let err = InstanceLocker::acquire(cfg).err().expect("must conflict");
        match &err {
            InstanceError::AnotherInstance { pid } => assert_eq!(*pid, Some(first.pid_value())),
            e => panic!("unexpected {e:?}"),
        }
        assert!(
            err.to_string()
                .contains(&format!("(pid={})", first.pid_value()))
        );
        // failed attempt must not have removed the holder's pid file
        assert!(d.path().join("gn-drive.pid").exists());
        first.release();
        let again = InstanceLocker::acquire(cfg).unwrap();
        drop(again);
    }

    #[test]
    fn drop_releases_lock() {
        let d = dir();
        let cfg = d.path().to_str().unwrap();
        drop(InstanceLocker::acquire(cfg).unwrap());
        assert!(InstanceLocker::acquire(cfg).is_ok());
    }

    #[test]
    fn stale_pid_file_is_overwritten() {
        let d = dir();
        fs::write(d.path().join("gn-drive.pid"), "99999999\n").unwrap();
        let l = InstanceLocker::acquire(d.path().to_str().unwrap()).unwrap();
        assert_eq!(
            fs::read_to_string(d.path().join("gn-drive.pid")).unwrap(),
            std::process::id().to_string()
        );
        drop(l);
    }

    #[test]
    fn conflict_with_garbage_pid_file_reports_no_pid() {
        let d = dir();
        let cfg = d.path().to_str().unwrap();
        let first = InstanceLocker::acquire(cfg).unwrap();
        fs::write(d.path().join("gn-drive.pid"), "not-a-number").unwrap();
        let err = InstanceLocker::acquire(cfg).err().unwrap();
        assert!(matches!(err, InstanceError::AnotherInstance { pid: None }));
        assert_eq!(
            err.to_string(),
            "another gn-drive instance is running. Run 'gn-drive service stop' to stop it, or kill the process manually"
        );
        drop(first);
    }

    #[test]
    fn release_keeps_pid_file_owned_by_someone_else() {
        let d = dir();
        let l = InstanceLocker::acquire(d.path().to_str().unwrap()).unwrap();
        fs::write(d.path().join("gn-drive.pid"), "1").unwrap();
        l.release();
        assert!(d.path().join("gn-drive.pid").exists());
    }

    #[test]
    fn creates_missing_config_dir() {
        let d = dir();
        let nested = d.path().join("a/b");
        let l = InstanceLocker::acquire(nested.to_str().unwrap()).unwrap();
        assert!(nested.join("gn-drive.lock").exists());
        drop(l);
    }
}
