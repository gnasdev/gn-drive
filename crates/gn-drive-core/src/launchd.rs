//! launchd service management (port of `Launchd.swift`).
//!
//! User: `~/Library/LaunchAgents/com.gndrive.app.plist` (`gui/<uid>` domain).
//! System: `/Library/LaunchDaemons/com.gndrive.app.plist`.

use std::fs;
use std::path::Path;
use std::process::Command;
use std::sync::Arc;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ServiceScope {
    User,
    System,
}

impl ServiceScope {
    pub fn as_str(self) -> &'static str {
        match self {
            ServiceScope::User => "user",
            ServiceScope::System => "system",
        }
    }
    pub fn parse(s: &str) -> Option<ServiceScope> {
        match s {
            "user" => Some(ServiceScope::User),
            "system" => Some(ServiceScope::System),
            _ => None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct ServiceSpec {
    pub name: String,
    pub display_name: String,
    pub description: String,
    pub exec_path: String,
    pub config_dir: String,
    pub scope: ServiceScope,
    pub env: Vec<String>,
}

impl Default for ServiceSpec {
    fn default() -> Self {
        ServiceSpec {
            name: "gn-drive".into(),
            display_name: "GN Drive".into(),
            description: String::new(),
            exec_path: String::new(),
            config_dir: String::new(),
            scope: ServiceScope::User,
            env: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServiceStatus {
    pub installed: bool,
    pub running: bool,
    pub pid: i64,
    pub mode: String,
    pub scope: String,
}

impl Default for ServiceStatus {
    fn default() -> Self {
        ServiceStatus {
            installed: false,
            running: false,
            pid: 0,
            mode: "service".into(),
            scope: String::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ServiceError {
    #[error("service: not installed")]
    NotInstalled,
    #[error("service: {0}")]
    Launchctl(String),
}

/// Runs `launchctl <args>`; `Ok(stdout)` on exit 0, otherwise `Err(stderr or stdout)`.
pub trait LaunchctlRunner: Send + Sync {
    fn run(&self, args: &[String]) -> Result<String, String>;
}

/// Real implementation: `/bin/launchctl`.
pub struct SystemLaunchctl;

impl LaunchctlRunner for SystemLaunchctl {
    fn run(&self, args: &[String]) -> Result<String, String> {
        let out = Command::new("/bin/launchctl")
            .args(args)
            .output()
            .map_err(|e| e.to_string())?;
        let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
        let stderr = String::from_utf8_lossy(&out.stderr).into_owned();
        if !out.status.success() {
            return Err(if stderr.is_empty() { stdout } else { stderr });
        }
        Ok(stdout)
    }
}

pub struct LaunchdService {
    home: String,
    uid: u32,
    /// Executable used when `ServiceSpec.exec_path` is empty.
    default_exec: String,
    runner: Arc<dyn LaunchctlRunner>,
}

const USER_PLIST_DIR: &str = "Library/LaunchAgents";
const SYSTEM_PLIST_DIR: &str = "Library/LaunchDaemons";
const LEGACY_AGENT_NAME: &str = "gn-drive";

impl LaunchdService {
    pub const LABEL: &'static str = "com.gndrive.app";

    /// Real home dir / uid / `/bin/launchctl`.
    pub fn new() -> LaunchdService {
        let home = std::env::var("HOME")
            .ok()
            .filter(|h| !h.is_empty())
            .unwrap_or_else(|| "/tmp".into());
        // SAFETY: getuid never fails.
        let uid = unsafe { libc::getuid() };
        Self::with_parts(&home, uid, Arc::new(SystemLaunchctl))
    }

    /// Injectable home dir, uid and launchctl runner (tests).
    pub fn with_parts(home: &str, uid: u32, runner: Arc<dyn LaunchctlRunner>) -> LaunchdService {
        let default_exec = std::env::current_exe()
            .ok()
            .map(|p| p.to_string_lossy().into_owned())
            .or_else(|| std::env::args().next())
            .unwrap_or_default();
        LaunchdService {
            home: home.to_string(),
            uid,
            default_exec,
            runner,
        }
    }

    /// Override the executable path used when the spec has none.
    pub fn with_default_exec(mut self, exec: &str) -> LaunchdService {
        self.default_exec = exec.to_string();
        self
    }

    pub fn plist_path(&self, spec: &ServiceSpec) -> String {
        if spec.scope == ServiceScope::System {
            return format!("/{SYSTEM_PLIST_DIR}/{}.plist", Self::LABEL);
        }
        format!("{}/{USER_PLIST_DIR}/{}.plist", self.home, Self::LABEL)
    }

    fn domain(&self, spec: &ServiceSpec) -> String {
        if spec.scope == ServiceScope::System {
            "system".into()
        } else {
            format!("gui/{}", self.uid)
        }
    }

    pub fn is_installed(&self, spec: &ServiceSpec) -> bool {
        Path::new(&self.plist_path(spec)).exists()
    }

    pub fn install(&self, spec: &ServiceSpec) -> Result<(), ServiceError> {
        self.cleanup_legacy_agent(spec);
        // Remove stale registration before writing a fresh plist.
        if self.is_installed(spec) {
            let _ = self.bootout(spec);
            let _ = fs::remove_file(self.plist_path(spec));
        }
        let plist = self.render_plist(spec);
        let path = self.plist_path(spec);
        if let Some(parent) = Path::new(&path).parent() {
            fs::create_dir_all(parent).map_err(io_err)?;
        }
        write_atomic(&path, plist.as_bytes()).map_err(io_err)?;
        let args = vec!["bootstrap".to_string(), self.domain(spec), path.clone()];
        if self.run(&args).is_err() {
            // One retry (matches Go behaviour for transient bootstrap errors).
            if let Err(e) = self.run(&args) {
                return Err(ServiceError::Launchctl(format!(
                    "bootstrap failed; plist written to: {path}\n{e}"
                )));
            }
        }
        Ok(())
    }

    pub fn uninstall(&self, spec: &ServiceSpec) -> Result<(), ServiceError> {
        if self.is_installed(spec) {
            let _ = self.bootout(spec);
            fs::remove_file(self.plist_path(spec)).map_err(io_err)?;
        }
        self.cleanup_legacy_agent(spec);
        Ok(())
    }

    pub fn start(&self, spec: &ServiceSpec) -> Result<(), ServiceError> {
        if !self.is_installed(spec) {
            return Err(ServiceError::NotInstalled);
        }
        self.run(&[
            "kickstart".into(),
            "-k".into(),
            format!("{}/{}", self.domain(spec), Self::LABEL),
        ])
        .map(|_| ())
    }

    pub fn stop(&self, spec: &ServiceSpec) -> Result<(), ServiceError> {
        if !self.is_installed(spec) {
            return Err(ServiceError::NotInstalled);
        }
        self.run(&[
            "bootout".into(),
            format!("{}/{}", self.domain(spec), Self::LABEL),
        ])
        .map(|_| ())
    }

    pub fn restart(&self, spec: &ServiceSpec) -> Result<(), ServiceError> {
        self.stop(spec)?;
        self.start(spec)
    }

    fn bootout(&self, spec: &ServiceSpec) -> Result<String, ServiceError> {
        self.run(&["bootout".into(), self.domain(spec), self.plist_path(spec)])
    }

    pub fn status(&self, spec: &ServiceSpec) -> Result<ServiceStatus, ServiceError> {
        let mut st = ServiceStatus {
            scope: spec.scope.as_str().into(),
            ..Default::default()
        };
        st.installed = self.is_installed(spec);
        if !st.installed {
            return Err(ServiceError::NotInstalled);
        }
        // launchctl print gui/$UID/label — look for "state = running" / pid.
        let out = self
            .run(&[
                "print".into(),
                format!("{}/{}", self.domain(spec), Self::LABEL),
            ])
            .unwrap_or_default();
        if let Some(line) = out.split('\n').find(|l| l.contains("pid = ")) {
            let digits = line.split('=').next_back().map(str::trim).unwrap_or("");
            if let Ok(pid) = digits.parse::<i64>()
                && pid > 0
            {
                st.pid = pid;
                st.running = true;
            }
        } else if out.contains("state = running") {
            st.running = true;
        }
        Ok(st)
    }

    /// Render the launchd plist (byte-identical to the Swift template).
    pub fn render_plist(&self, spec: &ServiceSpec) -> String {
        let exec = if spec.exec_path.is_empty() {
            self.default_exec.as_str()
        } else {
            spec.exec_path.as_str()
        };
        let home = &self.home;
        let work_dir = delete_last_component(exec);
        let log_dir = if spec.config_dir.is_empty() {
            format!("{home}/.config/gn-drive")
        } else {
            spec.config_dir.clone()
        };
        let label = Self::LABEL;
        format!(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>Label</key>
  <string>{label}</string>
  <key>ProgramArguments</key>
  <array>
    <string>{exec}</string>
    <string>run</string>
    <string>--service</string>
  </array>
  <key>RunAtLoad</key>
  <true/>
  <key>KeepAlive</key>
  <dict>
    <key>SuccessfulExit</key>
    <false/>
    <key>Crashed</key>
    <true/>
  </dict>
  <key>ThrottleInterval</key>
  <integer>10</integer>
  <key>EnvironmentVariables</key>
  <dict>
    <key>GN_DRIVE_MODE</key>
    <string>service</string>
    <key>HOME</key>
    <string>{home}</string>
  </dict>
  <key>WorkingDirectory</key>
  <string>{work_dir}</string>
  <key>StandardOutPath</key>
  <string>{log_dir}/gn-drive.out.log</string>
  <key>StandardErrorPath</key>
  <string>{log_dir}/gn-drive.err.log</string>
</dict>
</plist>"#
        )
    }

    /// Drop legacy `~/Library/LaunchAgents/gn-drive.plist` (non reverse-DNS label).
    fn cleanup_legacy_agent(&self, spec: &ServiceSpec) {
        if spec.scope != ServiceScope::User {
            return;
        }
        let legacy = format!("{}/{USER_PLIST_DIR}/{LEGACY_AGENT_NAME}.plist", self.home);
        if Path::new(&legacy).exists() {
            let _ = self.run(&["bootout".into(), self.domain(spec), legacy.clone()]);
            let _ = fs::remove_file(&legacy);
        }
    }

    fn run(&self, args: &[String]) -> Result<String, ServiceError> {
        self.runner.run(args).map_err(ServiceError::Launchctl)
    }
}

impl Default for LaunchdService {
    fn default() -> Self {
        Self::new()
    }
}

fn io_err(e: std::io::Error) -> ServiceError {
    ServiceError::Launchctl(e.to_string())
}

fn write_atomic(path: &str, data: &[u8]) -> std::io::Result<()> {
    let tmp = format!("{path}.tmp");
    fs::write(&tmp, data)?;
    fs::rename(&tmp, path)
}

/// `NSString.deletingLastPathComponent` for the common cases.
fn delete_last_component(p: &str) -> String {
    let trimmed = p.trim_end_matches('/');
    match trimmed.rfind('/') {
        None => String::new(),
        Some(0) => "/".into(),
        Some(i) => trimmed[..i].to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use parking_lot::Mutex;

    #[derive(Default)]
    struct Fake {
        calls: Mutex<Vec<Vec<String>>>,
        /// (args prefix command, result) overrides; first match wins.
        responses: Mutex<Vec<(String, Result<String, String>)>>,
        fail_bootstrap: Mutex<usize>,
    }

    impl LaunchctlRunner for Fake {
        fn run(&self, args: &[String]) -> Result<String, String> {
            self.calls.lock().push(args.to_vec());
            if args[0] == "bootstrap" {
                let mut n = self.fail_bootstrap.lock();
                if *n > 0 {
                    *n -= 1;
                    return Err("Bootstrap failed: 5".into());
                }
            }
            for (cmd, r) in self.responses.lock().iter() {
                if *cmd == args[0] {
                    return r.clone();
                }
            }
            Ok(String::new())
        }
    }

    fn setup() -> (tempfile::TempDir, Arc<Fake>, LaunchdService, ServiceSpec) {
        let d = tempfile::tempdir().unwrap();
        let fake = Arc::new(Fake::default());
        let svc = LaunchdService::with_parts(d.path().to_str().unwrap(), 501, fake.clone())
            .with_default_exec("/opt/gn/bin/gn-drive");
        let spec = ServiceSpec {
            exec_path: "/opt/gn/bin/gn-drive".into(),
            ..Default::default()
        };
        (d, fake, svc, spec)
    }

    fn calls(f: &Fake) -> Vec<String> {
        f.calls.lock().iter().map(|c| c.join(" ")).collect()
    }

    #[test]
    fn paths_and_domains() {
        let (d, _f, svc, spec) = setup();
        let home = d.path().to_str().unwrap();
        assert_eq!(
            svc.plist_path(&spec),
            format!("{home}/Library/LaunchAgents/com.gndrive.app.plist")
        );
        let sys = ServiceSpec {
            scope: ServiceScope::System,
            ..spec.clone()
        };
        assert_eq!(
            svc.plist_path(&sys),
            "/Library/LaunchDaemons/com.gndrive.app.plist"
        );
        assert_eq!(svc.domain(&spec), "gui/501");
        assert_eq!(svc.domain(&sys), "system");
    }

    #[test]
    fn plist_matches_swift_template() {
        let (_d, _f, svc, spec) = setup();
        let spec = ServiceSpec {
            config_dir: "/cfg".into(),
            ..spec
        };
        let home = &svc.home;
        let expected = format!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
<!DOCTYPE plist PUBLIC \"-//Apple//DTD PLIST 1.0//EN\" \"http://www.apple.com/DTDs/PropertyList-1.0.dtd\">\n\
<plist version=\"1.0\">\n<dict>\n  <key>Label</key>\n  <string>com.gndrive.app</string>\n  <key>ProgramArguments</key>\n  <array>\n    <string>/opt/gn/bin/gn-drive</string>\n    <string>run</string>\n    <string>--service</string>\n  </array>\n  <key>RunAtLoad</key>\n  <true/>\n  <key>KeepAlive</key>\n  <dict>\n    <key>SuccessfulExit</key>\n    <false/>\n    <key>Crashed</key>\n    <true/>\n  </dict>\n  <key>ThrottleInterval</key>\n  <integer>10</integer>\n  <key>EnvironmentVariables</key>\n  <dict>\n    <key>GN_DRIVE_MODE</key>\n    <string>service</string>\n    <key>HOME</key>\n    <string>{home}</string>\n  </dict>\n  <key>WorkingDirectory</key>\n  <string>/opt/gn/bin</string>\n  <key>StandardOutPath</key>\n  <string>/cfg/gn-drive.out.log</string>\n  <key>StandardErrorPath</key>\n  <string>/cfg/gn-drive.err.log</string>\n</dict>\n</plist>"
        );
        assert_eq!(svc.render_plist(&spec), expected);
    }

    #[test]
    fn plist_defaults_log_dir_and_exec() {
        let (_d, _f, svc, _spec) = setup();
        let p = svc.render_plist(&ServiceSpec::default());
        assert!(p.contains("<string>/opt/gn/bin/gn-drive</string>"));
        assert!(p.contains(&format!(
            "<string>{}/.config/gn-drive/gn-drive.out.log</string>",
            svc.home
        )));
    }

    #[test]
    fn install_writes_plist_and_bootstraps() {
        let (_d, f, svc, spec) = setup();
        svc.install(&spec).unwrap();
        let path = svc.plist_path(&spec);
        assert_eq!(fs::read_to_string(&path).unwrap(), svc.render_plist(&spec));
        assert_eq!(calls(&f), vec![format!("bootstrap gui/501 {path}")]);
        assert!(svc.is_installed(&spec));
    }

    #[test]
    fn install_reinstall_boots_out_first_and_cleans_legacy() {
        let (_d, f, svc, spec) = setup();
        let legacy = format!("{}/Library/LaunchAgents/gn-drive.plist", svc.home);
        fs::create_dir_all(Path::new(&legacy).parent().unwrap()).unwrap();
        fs::write(&legacy, "x").unwrap();
        svc.install(&spec).unwrap();
        svc.install(&spec).unwrap();
        let path = svc.plist_path(&spec);
        assert!(!Path::new(&legacy).exists());
        assert_eq!(
            calls(&f),
            vec![
                format!("bootout gui/501 {legacy}"),
                format!("bootstrap gui/501 {path}"),
                format!("bootout gui/501 {path}"),
                format!("bootstrap gui/501 {path}"),
            ]
        );
    }

    #[test]
    fn install_retries_once_then_reports_error() {
        let (_d, f, svc, spec) = setup();
        *f.fail_bootstrap.lock() = 1;
        svc.install(&spec).unwrap();
        assert_eq!(calls(&f).len(), 2);

        let (_d2, f2, svc2, spec2) = setup();
        *f2.fail_bootstrap.lock() = 2;
        let err = svc2.install(&spec2).unwrap_err();
        let path = svc2.plist_path(&spec2);
        assert_eq!(
            err.to_string(),
            format!(
                "service: bootstrap failed; plist written to: {path}\nservice: Bootstrap failed: 5"
            )
        );
        assert!(Path::new(&path).exists());
    }

    #[test]
    fn start_stop_restart_require_install() {
        let (_d, f, svc, spec) = setup();
        assert_eq!(svc.start(&spec), Err(ServiceError::NotInstalled));
        assert_eq!(svc.stop(&spec), Err(ServiceError::NotInstalled));
        assert_eq!(svc.restart(&spec), Err(ServiceError::NotInstalled));
        assert_eq!(
            ServiceError::NotInstalled.to_string(),
            "service: not installed"
        );
        assert!(f.calls.lock().is_empty());
        svc.install(&spec).unwrap();
        f.calls.lock().clear();
        svc.restart(&spec).unwrap();
        assert_eq!(
            calls(&f),
            vec![
                "bootout gui/501/com.gndrive.app",
                "kickstart -k gui/501/com.gndrive.app"
            ]
        );
    }

    #[test]
    fn stop_error_propagates() {
        let (_d, f, svc, spec) = setup();
        svc.install(&spec).unwrap();
        f.responses
            .lock()
            .push(("bootout".into(), Err("nope".into())));
        assert_eq!(svc.stop(&spec).unwrap_err().to_string(), "service: nope");
    }

    #[test]
    fn uninstall_removes_plist() {
        let (_d, f, svc, spec) = setup();
        svc.uninstall(&spec).unwrap(); // not installed: no-op
        assert!(f.calls.lock().is_empty());
        svc.install(&spec).unwrap();
        f.calls.lock().clear();
        svc.uninstall(&spec).unwrap();
        assert!(!svc.is_installed(&spec));
        assert_eq!(
            calls(&f),
            vec![format!("bootout gui/501 {}", svc.plist_path(&spec))]
        );
    }

    #[test]
    fn status_parses_launchctl_print() {
        let (_d, f, svc, spec) = setup();
        assert_eq!(svc.status(&spec), Err(ServiceError::NotInstalled));
        svc.install(&spec).unwrap();

        let st = svc.status(&spec).unwrap();
        assert!(st.installed && !st.running && st.pid == 0);
        assert_eq!((st.mode.as_str(), st.scope.as_str()), ("service", "user"));

        f.responses.lock().push((
            "print".into(),
            Ok("com.gndrive.app = {\n\tstate = running\n\tpid = 4242\n}\n".into()),
        ));
        let st = svc.status(&spec).unwrap();
        assert!(st.running);
        assert_eq!(st.pid, 4242);
        assert_eq!(
            f.calls.lock().last().unwrap().join(" "),
            "print gui/501/com.gndrive.app"
        );

        f.responses.lock().clear();
        f.responses
            .lock()
            .push(("print".into(), Ok("\tstate = running\n".into())));
        let st = svc.status(&spec).unwrap();
        assert!(st.running && st.pid == 0);

        f.responses.lock().clear();
        f.responses
            .lock()
            .push(("print".into(), Err("Could not find service".into())));
        let st = svc.status(&spec).unwrap();
        assert!(st.installed && !st.running);
    }

    #[test]
    fn delete_last_component_cases() {
        assert_eq!(delete_last_component("/a/b/c"), "/a/b");
        assert_eq!(delete_last_component("/a"), "/");
        assert_eq!(delete_last_component("a"), "");
    }
}
