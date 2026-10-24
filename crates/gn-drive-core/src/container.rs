//! Application wiring — port of `AppContainer.swift` / `internal/app`.
//!
//! Engines are built before the data plane. Store and rclone attach on
//! construction when the config is readable, and again after unlock when the
//! portal started locked.

use crate::auth::{AuthError, AuthService};
use crate::board_engine::BoardEngine;
use crate::event_bus::EventBus;
use crate::flow_engine::FlowEngine;
use crate::health::HealthWriter;
use crate::keychain::SecureStore;
use crate::logger::{LogMode, Logger};
use crate::paths::Paths;
use crate::rclone::{RcloneClient, RcloneError, RcloneRunner, SyncClient};
use crate::runtime_hub::RuntimeHub;
use crate::store::{Store, StoreError};
use crate::sync_engine::SyncEngine;
use parking_lot::Mutex;
use std::path::Path;
use std::sync::Arc;

/// Display texts match Swift `AppError.errorDescription`.
#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("auth: app is locked — provide --password")]
    Locked,
    #[error("init rclone: {0}")]
    RcloneMissing(RcloneError),
    #[error(transparent)]
    Auth(#[from] AuthError),
    #[error(transparent)]
    Store(#[from] StoreError),
    #[error("{0}")]
    Io(String),
}

impl From<std::io::Error> for AppError {
    fn from(e: std::io::Error) -> Self {
        AppError::Io(e.to_string())
    }
}

/// Options for [`AppContainer::new`].
pub struct ContainerOptions {
    pub config_dir: String,
    pub log_mode: LogMode,
    pub rclone_binary: String,
    pub unlock_password: String,
    /// Start even while locked (the UI shows unlock; the data plane waits).
    pub portal_mode: bool,
    pub version: String,
    pub key_store: Option<Arc<dyn SecureStore>>,
    /// When set, [`RcloneClient::with_runner`] is used and binary resolution is skipped.
    pub rclone_runner: Option<Arc<dyn RcloneRunner>>,
    /// Inject a logger (tests pass `Logger::quiet()`).
    pub logger: Option<Logger>,
}

impl Default for ContainerOptions {
    fn default() -> Self {
        Self {
            config_dir: String::new(),
            log_mode: LogMode::Foreground,
            rclone_binary: String::new(),
            unlock_password: String::new(),
            portal_mode: false,
            version: "dev".into(),
            key_store: None,
            rclone_runner: None,
            logger: None,
        }
    }
}

/// Process-wide wiring of auth, engines, and the deferred data plane.
pub struct AppContainer {
    pub config: Paths,
    pub bus: EventBus,
    pub log: Logger,
    pub auth: Arc<AuthService>,
    pub sync_engine: Arc<SyncEngine>,
    pub board_engine: Arc<BoardEngine>,
    pub flow_engine: Arc<FlowEngine>,
    pub runtime: Arc<RuntimeHub>,
    pub version: String,
    pub health: Mutex<Option<HealthWriter>>,
    store: Mutex<Option<Arc<Store>>>,
    rclone: Mutex<Option<Arc<RcloneClient>>>,
    rclone_binary: Option<String>,
    rclone_runner: Option<Arc<dyn RcloneRunner>>,
    mu: Mutex<()>,
}

impl AppContainer {
    pub fn new(opts: ContainerOptions) -> Result<Arc<AppContainer>, AppError> {
        let mut cfg = Paths::detect();
        if !opts.config_dir.is_empty() {
            cfg.config_dir = opts.config_dir.clone();
            cfg.log_dir = opts.config_dir;
        }
        cfg.ensure_config_dir()?;

        let log = match opts.logger {
            Some(l) => l,
            None => {
                let log = Logger::new(opts.log_mode);
                if opts.log_mode == LogMode::Service {
                    log.with("service", "gn-drive")
                } else {
                    log
                }
            }
        };

        let auth = Arc::new(AuthService::new(
            &cfg.config_dir,
            &log,
            opts.key_store.clone(),
        )?);
        if !opts.unlock_password.is_empty() {
            auth.unlock(&opts.unlock_password)?;
        }
        if !opts.portal_mode && auth.is_setup() && !auth.is_unlocked() {
            return Err(AppError::Locked);
        }
        if opts.portal_mode && auth.is_setup() && !auth.is_unlocked() {
            log.info("portal: starting locked — unlock via UI", &[]);
        }

        let version = if opts.version.is_empty() {
            "dev".to_string()
        } else {
            opts.version
        };
        let bus = EventBus::new();
        let sync_engine = SyncEngine::new(log.clone(), bus.clone());
        let board_engine = BoardEngine::new(log.clone(), bus.clone());
        let flow_engine = FlowEngine::new(log.clone(), bus.clone());
        sync_engine.set_flow_executor(flow_engine.clone());
        let runtime = RuntimeHub::new(&bus, Some(flow_engine.clone()), Some(sync_engine.clone()));

        let app = Arc::new(AppContainer {
            config: cfg,
            bus,
            log,
            auth,
            sync_engine,
            board_engine,
            flow_engine,
            runtime,
            version,
            health: Mutex::new(None),
            store: Mutex::new(None),
            rclone: Mutex::new(None),
            rclone_binary: if opts.rclone_binary.is_empty() {
                None
            } else {
                Some(opts.rclone_binary)
            },
            rclone_runner: opts.rclone_runner,
            mu: Mutex::new(()),
        });

        if Self::can_open_data_plane(&app.auth) {
            if let Err(e) = app.open_data_plane() {
                app.runtime.close();
                app.bus.close();
                return Err(e);
            }
        } else {
            app.log.info(
                "portal: data plane deferred until unlock (encrypted config)",
                &[],
            );
        }
        Ok(app)
    }

    /// Unlocked, never-setup, or locked-with-plaintext can read sqlite/rclone now.
    fn can_open_data_plane(auth: &AuthService) -> bool {
        if !auth.is_setup() || auth.is_unlocked() {
            return true;
        }
        !auth.has_encrypted_config()
    }

    pub fn store(&self) -> Option<Arc<Store>> {
        self.store.lock().clone()
    }

    pub fn rclone(&self) -> Option<Arc<RcloneClient>> {
        self.rclone.lock().clone()
    }

    /// Open store + rclone and attach them to the engines.
    pub fn open_data_plane(&self) -> Result<(), AppError> {
        let _guard = self.mu.lock();
        if self.store.lock().is_some() {
            return Ok(());
        }

        let db_path = child(&self.config.config_dir, "gn-drive.db");
        let st = Arc::new(Store::open(&db_path, &self.log)?);
        let rclone_cfg = child(&self.config.config_dir, "rclone.conf");
        let rc = match self.open_rclone(&rclone_cfg) {
            Ok(rc) => rc,
            Err(e) => {
                st.close();
                return Err(AppError::RcloneMissing(e));
            }
        };

        *self.store.lock() = Some(st.clone());
        *self.rclone.lock() = Some(rc.clone());
        let sync_client: Arc<dyn SyncClient> = rc.clone();
        self.sync_engine.attach(st.clone(), sync_client.clone());
        self.board_engine.attach(st.clone(), sync_client);
        self.flow_engine.attach(st, self.sync_engine.clone());
        self.log.info("data plane ready", &[("db", &db_path)]);
        Ok(())
    }

    fn open_rclone(&self, config_path: &str) -> Result<Arc<RcloneClient>, RcloneError> {
        if let Some(runner) = &self.rclone_runner {
            return Ok(Arc::new(RcloneClient::with_runner(
                runner.clone(),
                config_path,
                &self.log,
            )));
        }
        RcloneClient::new(self.rclone_binary.as_deref(), config_path, &self.log).map(Arc::new)
    }

    /// Called after auth unlock or setup — opens a deferred data plane.
    pub fn after_unlock(&self) -> Result<(), AppError> {
        self.open_data_plane()
    }

    /// Close the data plane so auth can re-encrypt config safely.
    /// The sync engine stays "running" so schedules reload on the next attach.
    pub fn before_lock(&self) {
        let _guard = self.mu.lock();
        self.sync_engine.detach();
        self.board_engine.detach();
        self.flow_engine.detach();
        self.runtime.reset();
        if let Some(st) = self.store.lock().take() {
            st.close();
        }
        *self.rclone.lock() = None;
    }

    pub fn close(&self) {
        if let Some(h) = self.health.lock().take() {
            h.stop();
        }
        // Stop in-flight syncs before the hub and the database go away.
        self.sync_engine.stop();
        self.runtime.close();
        self.bus.close();
        self.before_lock();
        self.auth.suspend();
    }
}

impl Drop for AppContainer {
    fn drop(&mut self) {
        self.close();
    }
}

fn child(dir: &str, name: &str) -> String {
    Path::new(dir).join(name).to_string_lossy().into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::keychain::MemorySecureStore;
    use crate::rclone::testing::FakeRunner;

    fn opts(dir: &std::path::Path, runner: Arc<FakeRunner>) -> ContainerOptions {
        ContainerOptions {
            config_dir: dir.to_string_lossy().into_owned(),
            portal_mode: true,
            version: "test".into(),
            key_store: Some(Arc::new(MemorySecureStore::new())),
            rclone_runner: Some(runner),
            logger: Some(Logger::quiet()),
            ..ContainerOptions::default()
        }
    }

    #[test]
    fn fake_runner_opens_data_plane_without_a_real_binary() {
        let dir = tempfile::tempdir().unwrap();
        let runner = Arc::new(FakeRunner::new());
        let app = AppContainer::new(opts(dir.path(), runner)).unwrap();
        assert!(app.store().is_some());
        assert!(app.rclone().is_some());
        assert!(!app.sync_engine.is_running(), "start is explicit");
        assert_eq!(app.version, "test");
        assert!(dir.path().join("gn-drive.db").exists());
        app.close();
    }

    #[test]
    fn missing_binary_is_rclone_missing() {
        let dir = tempfile::tempdir().unwrap();
        let app = AppContainer::new(ContainerOptions {
            config_dir: dir.path().to_string_lossy().into_owned(),
            rclone_binary: "/definitely/not/a/rclone-binary".into(),
            logger: Some(Logger::quiet()),
            ..ContainerOptions::default()
        });
        match app {
            Err(AppError::RcloneMissing(e)) => {
                let msg = AppError::RcloneMissing(e).to_string();
                assert!(msg.starts_with("init rclone:"), "{msg}");
            }
            Ok(_) => panic!("expected rclone missing"),
            Err(other) => panic!("unexpected {other}"),
        }
    }

    #[test]
    fn portal_defers_when_locked_and_reopens_after_unlock() {
        let dir = tempfile::tempdir().unwrap();
        let runner = Arc::new(FakeRunner::new());
        let app = AppContainer::new(opts(dir.path(), runner.clone())).unwrap();
        app.auth.setup_password("test-pass-1").unwrap();
        assert!(app.store().is_some());
        app.close();

        // Fresh key store: the remembered key must not resume the session.
        let locked = AppContainer::new(ContainerOptions {
            config_dir: dir.path().to_string_lossy().into_owned(),
            portal_mode: true,
            rclone_runner: Some(runner.clone()),
            key_store: Some(Arc::new(MemorySecureStore::new())),
            logger: Some(Logger::quiet()),
            ..ContainerOptions::default()
        })
        .unwrap();
        assert!(locked.auth.is_setup());
        assert!(!locked.auth.is_unlocked());
        assert!(
            locked.store().is_none(),
            "encrypted config defers the data plane"
        );
        locked.close();

        let refused = AppContainer::new(ContainerOptions {
            config_dir: dir.path().to_string_lossy().into_owned(),
            portal_mode: false,
            rclone_runner: Some(runner.clone()),
            logger: Some(Logger::quiet()),
            ..ContainerOptions::default()
        });
        assert!(matches!(refused, Err(AppError::Locked)));
        assert_eq!(
            AppError::Locked.to_string(),
            "auth: app is locked — provide --password"
        );

        let opened = AppContainer::new(ContainerOptions {
            config_dir: dir.path().to_string_lossy().into_owned(),
            portal_mode: false,
            unlock_password: "test-pass-1".into(),
            rclone_runner: Some(runner),
            logger: Some(Logger::quiet()),
            ..ContainerOptions::default()
        })
        .unwrap();
        assert!(opened.auth.is_unlocked());
        assert!(opened.store().is_some());
        opened.before_lock();
        assert!(opened.store().is_none());
        opened.auth.unlock("test-pass-1").unwrap();
        opened.after_unlock().unwrap();
        assert!(opened.store().is_some());
        opened.close();
    }
}
