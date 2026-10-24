//! `gn-drive serve --stdio` — port of `Sources/gn-drive/ServeCommand.swift`.

use std::io::Write;
use std::sync::Arc;

use gn_drive_core::container::{AppContainer, AppError, ContainerOptions};
use gn_drive_core::instance_lock::{InstanceError, InstanceLocker};
use gn_drive_core::keychain::KeychainStore;
use gn_drive_core::logger::{LogLevel, LogMode};
use gn_drive_core::paths::Paths;
use gn_drive_core::rpc::stdio::RpcService;
use gn_drive_core::rpc::{Dispatcher, Options};

use crate::{CliResult, ServeArgs, VERSION};

fn exit(code: i32) -> ! {
    let _ = std::io::stdout().flush();
    std::process::exit(code)
}

pub fn serve(a: ServeArgs) -> CliResult {
    // SAFETY: setting a signal disposition has no memory-safety preconditions.
    unsafe { libc::signal(libc::SIGPIPE, libc::SIG_IGN) };

    if !a.stdio {
        eprintln!("gn-drive serve: --stdio is required in protocol v1");
        exit(2);
    }
    if a.protocol_version != 1 {
        RpcService::emit_fatal(
            "unsupported_protocol",
            &format!(
                "unsupported protocol version {} (want 1)",
                a.protocol_version
            ),
            None,
            None,
        );
        exit(2);
    }

    let level = LogLevel::from_label(&a.log_level).unwrap_or(LogLevel::Info);

    if RpcService::is_app_running() {
        RpcService::emit_fatal(
            "instance_busy",
            "the GN Drive app is running. Close it before starting another window.",
            None,
            Some("app"),
        );
        exit(3);
    }

    let cfg_dir = if a.config_dir.is_empty() {
        Paths::detect().config_dir
    } else {
        a.config_dir.clone()
    };
    let locker = match InstanceLocker::acquire(&cfg_dir) {
        Ok(l) => l,
        Err(InstanceError::AnotherInstance { pid }) => {
            let holder = pid
                .map(RpcService::classify_holder)
                .unwrap_or_else(|| "cli".to_string());
            let mut message = "another gn-drive instance is running".to_string();
            if let Some(pid) = pid {
                message.push_str(&format!(" (pid={pid}, holder={holder})"));
            }
            message.push_str(". Stop it with 'gn-drive service stop' or close the other window.");
            RpcService::emit_fatal("instance_busy", &message, pid, Some(&holder));
            exit(3);
        }
        Err(e) => return Err(e.into()),
    };

    let opts = ContainerOptions {
        portal_mode: true,
        key_store: Some(Arc::new(KeychainStore::new("gn-drive"))),
        version: VERSION.to_string(),
        log_mode: LogMode::Foreground,
        config_dir: a.config_dir.clone(),
        rclone_binary: a.rclone.clone(),
        unlock_password: a.password.clone(),
        ..ContainerOptions::default()
    };

    let container: Arc<AppContainer> = match AppContainer::new(opts) {
        Ok(c) => c,
        Err(AppError::RcloneMissing(e)) => {
            RpcService::emit_fatal(
                "rclone_missing",
                &format!("rclone unavailable: {e}"),
                None,
                None,
            );
            exit(2);
        }
        Err(AppError::Locked) => {
            RpcService::emit_fatal("locked", "auth: app is locked", None, None);
            exit(2);
        }
        Err(e) => {
            RpcService::emit_fatal(
                "init_failed",
                &format!("initialization failed: {e}"),
                None,
                None,
            );
            exit(2);
        }
    };

    let svc_opts = Options {
        log_level: level,
        log_buffer_capacity: a.log_buffer.max(1) as usize,
        ..Options::default()
    };
    let dispatcher = Dispatcher::start(container.clone(), &a.rclone, svc_opts.clone());
    let lock_path = std::path::Path::new(&cfg_dir)
        .join("gn-drive.lock")
        .to_string_lossy()
        .into_owned();
    let service = Arc::new(RpcService::new(
        dispatcher,
        &lock_path,
        Some(locker),
        svc_opts,
    ));

    let mut signals = signal_hook::iterator::Signals::new([
        signal_hook::consts::SIGINT,
        signal_hook::consts::SIGTERM,
    ])?;
    {
        let service = service.clone();
        std::thread::Builder::new()
            .name("gn-drive.rpc.signal".into())
            .spawn(move || {
                for _ in signals.forever() {
                    service.begin_shutdown("signal");
                }
            })?;
    }

    container.sync_engine.start();
    let _ = service.run();
    Ok(())
}
