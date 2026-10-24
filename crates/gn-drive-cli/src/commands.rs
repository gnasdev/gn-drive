//! Subcommand implementations — port of `Sources/gn-drive/Commands.swift`.

use std::io::Write;
use std::sync::Arc;

use clap_complete::Shell;
use gn_drive_core::container::{AppContainer, ContainerOptions};
use gn_drive_core::health::{HealthWriter, ServiceHealth};
use gn_drive_core::instance_lock::InstanceLocker;
use gn_drive_core::keychain::KeychainStore;
use gn_drive_core::launchd::{LaunchdService, ServiceScope, ServiceSpec};
use gn_drive_core::logger::LogMode;
use gn_drive_core::models::{Profile, profile_direction};
use gn_drive_core::paths::Paths;
use gn_drive_core::rclone::{RcloneAction, RcloneClient, SyncConfig, SyncStats};
use gn_drive_core::self_update::{self, SelfUpdateError, UpdateOptions};
use gn_drive_core::sync_engine::SyncEngine;
use parking_lot::Mutex;

use crate::{
    BoardArgs, CliResult, CompletionArgs, DoctorArgs, ProfileAddArgs, RemoteAddArgs, RunArgs,
    SelfUpdateArgs, ServiceArgs, SyncArgs, VERSION, app_err,
};

/// Shared helper: build the app container for CLI one-shot commands.
/// The password comes from `GN_DRIVE_PASSWORD` when set.
pub fn make_app(
    password: Option<&str>,
    config_dir: Option<&str>,
) -> Result<Arc<AppContainer>, crate::CliError> {
    let mut opts = ContainerOptions {
        log_mode: LogMode::Foreground,
        version: VERSION.to_string(),
        ..ContainerOptions::default()
    };
    match password {
        Some(p) => opts.unlock_password = p.to_string(),
        None => {
            if let Ok(p) = std::env::var("GN_DRIVE_PASSWORD") {
                opts.unlock_password = p;
            }
        }
    }
    if let Some(d) = config_dir {
        opts.config_dir = d.to_string();
    }
    Ok(AppContainer::new(opts)?)
}

/// Closes the container on every exit path (Swift `defer { app.close() }`).
struct App(Arc<AppContainer>);

impl App {
    fn new() -> Result<App, crate::CliError> {
        Ok(App(make_app(None, None)?))
    }
}

impl std::ops::Deref for App {
    type Target = AppContainer;
    fn deref(&self) -> &AppContainer {
        &self.0
    }
}

impl Drop for App {
    fn drop(&mut self) {
        self.0.close();
    }
}

// ---- run -------------------------------------------------------------------

pub fn run(a: RunArgs) -> CliResult {
    let mut opts = ContainerOptions {
        log_mode: if a.dev {
            LogMode::Foreground
        } else if a.service {
            LogMode::Service
        } else {
            LogMode::Foreground
        },
        version: VERSION.to_string(),
        unlock_password: a.password.clone(),
        config_dir: a.config_dir.clone(),
        portal_mode: false,
        ..ContainerOptions::default()
    };
    if !a.service {
        opts.key_store = Some(Arc::new(KeychainStore::new("gn-drive")));
    }

    let cfg = Paths::detect();
    let dir = if opts.config_dir.is_empty() {
        cfg.config_dir.clone()
    } else {
        opts.config_dir.clone()
    };
    let locker = InstanceLocker::acquire(&dir)?;

    let app = App(AppContainer::new(opts)?);
    app.sync_engine.start();

    if a.service {
        let health = HealthWriter::new(&app.config.config_dir);
        let _ = health.start();
        *app.health.lock() = Some(health);
        app.log.info(
            "gn-drive service started",
            &[("pid", &std::process::id().to_string())],
        );
    } else {
        println!("gn-drive ready (engine running, headless). Ctrl+C to stop.");
        println!("Use the GNDrive app for the UI.");
        let _ = std::io::stdout().flush();
    }

    // Wait for SIGINT/SIGTERM.
    let mut signals = signal_hook::iterator::Signals::new([
        signal_hook::consts::SIGINT,
        signal_hook::consts::SIGTERM,
    ])?;
    let _ = signals.forever().next();
    app.log.info("signal received, shutting down", &[]);

    app.sync_engine.stop();
    drop(app);
    locker.release();
    Ok(())
}

// ---- service ---------------------------------------------------------------

pub fn service(a: ServiceArgs) -> CliResult {
    let mgr = LaunchdService::new();
    let exec = std::env::current_exe()
        .map(|p| p.to_string_lossy().into_owned())
        .or_else(|_| std::env::args().next().ok_or(()))
        .unwrap_or_default();
    let spec = ServiceSpec {
        scope: if a.system {
            ServiceScope::System
        } else {
            ServiceScope::User
        },
        config_dir: Paths::detect().config_dir,
        exec_path: exec,
        ..ServiceSpec::default()
    };
    let scope = spec.scope.as_str();

    match a.action.as_str() {
        "install" => {
            if spec.scope == ServiceScope::System {
                println!("Note: system-level install requires elevated privileges (sudo).");
            }
            println!("Installing gn-drive service (darwin, {scope})...");
            mgr.install(&spec)?;
            println!("✓ installed.");
            println!();
            println!("Status:");
            print_status(&mgr, &spec);
        }
        "uninstall" => {
            println!("Uninstalling gn-drive service (darwin, {scope})...");
            mgr.uninstall(&spec)?;
            println!("✓ uninstalled.");
        }
        "start" => {
            println!("Starting gn-drive service...");
            mgr.start(&spec)?;
            println!("✓ started.");
        }
        "stop" => {
            println!("Stopping gn-drive service...");
            mgr.stop(&spec)?;
            println!("✓ stopped.");
        }
        "status" => print_status(&mgr, &spec),
        "restart" => {
            println!("Restarting gn-drive service...");
            mgr.restart(&spec)?;
            println!("✓ restarted.");
        }
        other => {
            return Err(app_err(format!(
                "unknown service action: {other} (want install|uninstall|start|stop|status|restart)"
            )));
        }
    }
    Ok(())
}

fn print_status(mgr: &LaunchdService, spec: &ServiceSpec) {
    if !mgr.is_installed(spec) {
        println!("Service: not installed.");
        println!();
        println!("To install:");
        println!(
            "  gn-drive service install{}",
            if spec.scope == ServiceScope::System {
                " --system"
            } else {
                ""
            }
        );
        return;
    }
    match mgr.status(spec) {
        Ok(st) => {
            println!("Service:");
            println!("  Mode:     {}", st.mode);
            println!("  Scope:    {}", st.scope);
            println!("  Platform: darwin");
            if st.running {
                println!("  Running:  yes (pid {})", st.pid);
            } else {
                println!("  Running:  no");
            }
        }
        Err(e) => println!("Service: installed (status check failed: {e})"),
    }

    if let Some(h) = ServiceHealth::read(&spec.config_dir) {
        println!();
        println!("Health:");
        println!("  Started:        {}", h.started_at.to_rfc3339());
        println!("  Last heartbeat: {}", h.last_heartbeat.to_rfc3339());
        if h.is_stale() {
            println!("  ⚠ heartbeat stale (>60s old) — service may be unresponsive");
        }
        if h.web_port > 0 {
            println!("  Web port:       {}", h.web_port);
        }
        println!("  Uptime:         {}s", h.uptime_secs().round());
        if !h.last_error.is_empty() {
            println!("  Last error:     {}", h.last_error);
        }
        if let Some(d) = h.last_sync_at {
            println!("  Last sync:      {}", d.to_rfc3339());
        }
        if let Some(d) = h.next_schedule_at {
            println!("  Next schedule:  {}", d.to_rfc3339());
        }
        if !h.active_tasks.is_empty() {
            println!("  Active tasks:   {}", h.active_tasks.join(", "));
        }
    }
}

// ---- sync ------------------------------------------------------------------

pub fn sync(a: SyncArgs) -> CliResult {
    let act = if a.dry_run {
        "dry-run".to_string()
    } else {
        a.action.clone()
    };
    let Some(action) = RcloneAction::parse(&act) else {
        return Err(app_err(format!("sync: unknown action {act}")));
    };
    let app = App::new()?;
    let store = app.store().ok_or_else(|| app_err("store not ready"))?;
    let p = store.get_profile(&a.profile)?;
    println!("→ {} sync: {} → {} (action={act})", p.name, p.from, p.to);
    if p.dry_run {
        println!("  [profile has dry_run=true, no actual changes will be made]");
    }
    let rclone = app
        .rclone()
        .ok_or_else(|| app_err("rclone not available"))?;

    let last = Mutex::new(SyncStats::default());
    let mut flags = SyncEngine::profile_flags(&p);
    flags.dry_run = p.dry_run;
    let cfg = SyncConfig {
        action,
        source: p.from.clone(),
        dest: p.to.clone(),
        profile: Some(flags),
        ..SyncConfig::default()
    };
    let on_progress = |s: SyncStats| {
        let mut l = last.lock();
        if s.bytes != l.bytes || s.files != l.files {
            print!(
                "\r  {} / {} | {} files | {} errors   ",
                human_bytes(s.bytes),
                human_bytes(s.bytes_total),
                s.files,
                s.errors
            );
            let _ = std::io::stdout().flush();
            *l = s;
        }
    };
    match rclone.sync(&cfg, Some(&on_progress), None) {
        Ok(res) => {
            println!();
            println!(
                "✓ sync completed in {}s — {} transferred, {} errors",
                res.ended_at - res.started_at,
                human_bytes(res.stats.bytes),
                res.stats.errors
            );
            Ok(())
        }
        Err(e) => {
            println!();
            println!("✗ sync failed: {e}");
            Err(e.into())
        }
    }
}

// ---- board -----------------------------------------------------------------

pub fn board(a: BoardArgs) -> CliResult {
    let stop_on_error = !a.no_stop_on_error;
    let app = App::new()?;
    let store = app.store().ok_or_else(|| app_err("store not ready"))?;
    let rclone: Arc<RcloneClient> = app
        .rclone()
        .ok_or_else(|| app_err("rclone not available"))?;
    let eng = gn_drive_core::board_engine::BoardEngine::new(app.log.clone(), app.bus.clone());
    eng.attach(store.clone(), rclone);

    let b = store.load_board_graph(&a.board_id)?;
    if b.nodes.is_empty() {
        return Err(app_err(format!("board {} has no nodes", a.board_id)));
    }
    if b.edges.is_empty() {
        return Err(app_err(format!("board {} has no edges", a.board_id)));
    }
    println!(
        "▶ Board: {} ({}) — {} nodes, {} edges",
        b.name,
        b.id,
        b.nodes.len(),
        b.edges.len()
    );

    let out = Mutex::new(());
    eng.execute_sync(
        &a.board_id,
        stop_on_error,
        &|layer, idx, total, edge, src, dst, err| {
            let _g = out.lock();
            println!(
                "  │ [{idx}/{total}] layer={} {} — {} : {} → {}",
                layer + 1,
                edge.id,
                edge.action,
                src.label,
                dst.label
            );
            match err {
                Some(e) => println!("  │   ✗ edge {} failed: {e}", edge.id),
                None => println!("  │   ✓ edge {} ok", edge.id),
            }
        },
    )?;
    println!("✓ board {} executed successfully", b.id);
    Ok(())
}

// ---- profile ---------------------------------------------------------------

pub fn profile_list() -> CliResult {
    let app = App::new()?;
    let store = app.store().ok_or_else(|| app_err("store not ready"))?;
    let profiles = store.list_profiles()?;
    if profiles.is_empty() {
        println!("No profiles configured. Use 'gn-drive profile add' or the app UI.");
        return Ok(());
    }
    println!("NAME\tFROM\tTO\tPARALLEL\tBANDWIDTH\tDRY-RUN");
    for p in profiles {
        let bw = if p.bandwidth > 0 {
            format!("{}M", p.bandwidth)
        } else {
            String::new()
        };
        println!(
            "{}\t{}\t{}\t{}\t{}\t{}",
            p.name,
            trunc(&p.from, 40),
            trunc(&p.to, 40),
            p.parallel,
            bw,
            p.dry_run
        );
    }
    Ok(())
}

pub fn profile_add(a: ProfileAddArgs) -> CliResult {
    if a.name.is_empty() || a.from.is_empty() || a.to.is_empty() {
        return Err(app_err("profile add: --name, --from, --to are required"));
    }
    if !profile_direction::is_valid(&a.direction) {
        return Err(app_err(format!(
            "profile add: invalid --direction {} (allowed: push, bi, bi-resync)",
            a.direction
        )));
    }
    let app = App::new()?;
    let store = app.store().ok_or_else(|| app_err("store not ready"))?;
    let mut p = Profile {
        name: a.name.clone(),
        from: a.from,
        to: a.to,
        direction: a.direction.clone(),
        parallel: a.parallel,
        bandwidth: a.bandwidth,
        dry_run: a.dry_run,
        ..Profile::default()
    };
    store.save_profile(&mut p)?;
    println!("✓ added profile \"{}\" (direction={})", a.name, a.direction);
    Ok(())
}

pub fn profile_delete(name: &str) -> CliResult {
    let app = App::new()?;
    let store = app.store().ok_or_else(|| app_err("store not ready"))?;
    store.delete_profile(name)?;
    println!("✓ deleted profile \"{name}\"");
    Ok(())
}

// ---- remote ----------------------------------------------------------------

fn rclone_of(app: &App) -> Result<Arc<RcloneClient>, crate::CliError> {
    app.rclone().ok_or_else(|| app_err("rclone not available"))
}

pub fn remote_list() -> CliResult {
    let app = App::new()?;
    let remotes = rclone_of(&app)?.list_remotes()?;
    if remotes.is_empty() {
        println!("No remotes configured. Use 'gn-drive remote add' or the app UI.");
        return Ok(());
    }
    println!("NAME\tTYPE");
    for r in remotes {
        println!("{}\t{}", r.name, r.r#type);
    }
    Ok(())
}

pub fn remote_add(a: RemoteAddArgs) -> CliResult {
    if a.name.is_empty() || a.r#type.is_empty() {
        return Err(app_err("remote add: --name and --type are required"));
    }
    let app = App::new()?;
    rclone_of(&app)?.create_remote_verified(&a.name, &a.r#type, &a.config)?;
    println!("✓ added remote \"{}\" (type={})", a.name, a.r#type);
    Ok(())
}

pub fn remote_test(name: &str) -> CliResult {
    let app = App::new()?;
    let rc = rclone_of(&app)?;
    print!("Testing remote \"{name}\"... ");
    let _ = std::io::stdout().flush();
    match rc.test_remote(name) {
        Ok(()) => {
            println!("✓ OK");
            Ok(())
        }
        Err(e) => {
            println!("✗ FAILED");
            Err(e.into())
        }
    }
}

pub fn remote_delete(name: &str) -> CliResult {
    let app = App::new()?;
    rclone_of(&app)?.delete_remote(name)?;
    println!("✓ deleted remote \"{name}\"");
    Ok(())
}

// ---- self-update -----------------------------------------------------------

pub fn self_update(a: SelfUpdateArgs) -> CliResult {
    let mut opts = UpdateOptions::default();
    if !a.repo_owner.is_empty() {
        opts.repo_owner = a.repo_owner;
    }
    if !a.repo.is_empty() {
        opts.repo_name = a.repo;
    }
    opts.current_version = VERSION.to_string();
    opts.force = a.force;

    if a.check_only {
        let (cur, latest) = self_update::check(&opts)?;
        println!("current={cur} latest={latest}");
        if cur == latest {
            println!("✓ already on latest version");
        } else {
            println!("↑ update available — run 'gn-drive self-update' to apply");
        }
        return Ok(());
    }

    println!("gn-drive self-update (current={VERSION})");
    match self_update::update(&opts) {
        Ok(res) => {
            println!("✓ updated {} → {}", res.old_version, res.new_version);
            println!("  binary: {}", res.binary_path);
            if !res.restart_hint.is_empty() {
                println!("  next:   {}", res.restart_hint);
            }
            Ok(())
        }
        Err(SelfUpdateError::AlreadyUpToDate) => {
            println!("✓ already on latest version");
            Ok(())
        }
        Err(e) => Err(e.into()),
    }
}

// ---- doctor ----------------------------------------------------------------

pub fn doctor(a: DoctorArgs) -> CliResult {
    let mut out = std::io::stdout().lock();
    macro_rules! p {
        ($($t:tt)*) => {{ let _ = writeln!(out, $($t)*); }};
    }
    p!("=== gn-drive doctor ===");
    p!("");

    match RcloneClient::resolve_binary(None) {
        Ok(bin) => {
            p!("rclone:       {bin}");
            let v = std::process::Command::new(&bin)
                .arg("version")
                .output()
                .map(|o| {
                    let mut s = String::from_utf8_lossy(&o.stdout).into_owned();
                    s.push_str(&String::from_utf8_lossy(&o.stderr));
                    s
                })
                .unwrap_or_default();
            if let Some(first) = v.lines().find(|l| !l.is_empty()) {
                p!("  version: {first}");
            }
            p!("  [OK]");
        }
        Err(_) => {
            p!("rclone:       NOT FOUND in PATH");
            p!("  [ERROR] rclone is required. Install: https://rclone.org/install/");
        }
    }

    let cfg = Paths::detect();
    p!("");
    p!("Config dir:  {}", cfg.config_dir);
    if std::path::Path::new(&cfg.config_dir).exists() {
        p!("  [OK]");
    } else {
        p!("  [WARN] does not exist");
    }

    let db_path = std::path::Path::new(&cfg.config_dir).join("gn-drive.db");
    p!("Database:     {}", db_path.display());
    if db_path.exists() {
        p!("  [OK]");
    } else {
        p!("  [INFO] Database not yet created (first run)");
    }

    let app = App::new()?;
    let status = app.auth.status();
    p!("Auth config: {}/auth.json", cfg.config_dir);
    if status.setup {
        if status.unlocked {
            p!("  configured: yes, unlocked: yes  [OK]");
        } else {
            p!("  configured: yes, unlocked: no  [LOCKED]");
        }
    } else {
        p!("  configured: no  [OK]");
    }

    if !status.setup || status.unlocked {
        if let Some(rc) = app.rclone()
            && let Ok(remotes) = rc.list_remotes()
        {
            p!("");
            p!("Remotes:      {} configured", remotes.len());
            for r in &remotes {
                p!("  - {} ({})", r.name, r.r#type);
            }
        }
        if let Some(st) = app.store() {
            if let Ok(profiles) = st.list_profiles() {
                p!("");
                p!("Profiles:     {} configured", profiles.len());
                for pr in &profiles {
                    p!("  - {}", pr.name);
                }
            }
            if let Ok(history) = st.list_history(5, 0) {
                p!("");
                p!("History:      {} recent entries", history.len());
            }
        }
    }

    p!("");
    p!("Platform:     darwin ({})", arch_name());

    if a.data {
        p!("");
        p!("--- data directory contents ---");
        if let Ok(rd) = std::fs::read_dir(&cfg.config_dir) {
            for e in rd.flatten() {
                p!("  {}", e.file_name().to_string_lossy());
            }
        }
    }
    p!("");
    p!("All checks passed. gn-drive is ready to run.");
    Ok(())
}

// ---- completion ------------------------------------------------------------

pub fn completion(a: CompletionArgs) -> CliResult {
    let shell = match a.shell.as_str() {
        "bash" => Shell::Bash,
        "zsh" => Shell::Zsh,
        "fish" => Shell::Fish,
        other => {
            return Err(app_err(format!(
                "unsupported shell: {other} (want bash|zsh|fish)"
            )));
        }
    };
    let mut cmd = crate::command();
    clap_complete::generate(shell, &mut cmd, "gn-drive", &mut std::io::stdout());
    Ok(())
}

// ---- helpers ---------------------------------------------------------------

pub fn human_bytes(n: i64) -> String {
    let k = 1024.0_f64;
    let f = n as f64;
    if f < k {
        format!("{n}B")
    } else if f < k * k {
        format!("{:.1}K", f / k)
    } else if f < k * k * k {
        format!("{:.1}M", f / (k * k))
    } else {
        format!("{:.2}G", f / (k * k * k))
    }
}

pub fn trunc(s: &str, n: usize) -> String {
    if s.chars().count() <= n {
        s.to_string()
    } else {
        let mut t: String = s.chars().take(n - 1).collect();
        t.push('…');
        t
    }
}

fn arch_name() -> &'static str {
    if cfg!(target_arch = "aarch64") {
        "arm64"
    } else {
        "x86_64"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn human_bytes_formats() {
        assert_eq!(human_bytes(10), "10B");
        assert_eq!(human_bytes(1536), "1.5K");
        assert_eq!(human_bytes(5 * 1024 * 1024), "5.0M");
        assert_eq!(human_bytes(3 * 1024 * 1024 * 1024), "3.00G");
    }

    #[test]
    fn trunc_counts_chars() {
        assert_eq!(trunc("abc", 5), "abc");
        assert_eq!(trunc("abcdef", 4), "abc…");
        assert_eq!(trunc("ééééé", 3), "éé…");
    }
}
