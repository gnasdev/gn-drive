//! CLI integration tests. Every test uses a temp `GN_DRIVE_CONFIG_DIR` + `HOME`
//! and a fake `rclone` on PATH; nothing touches real config, keychain or launchd.

use assert_cmd::Command;
use predicates::prelude::*;
use std::io::{BufRead, BufReader, Write};
use std::os::unix::fs::PermissionsExt;
use std::process::{Child, Stdio};
use tempfile::TempDir;

struct Env {
    _tmp: TempDir,
    home: String,
    cfg: String,
    bin: String,
}

fn env() -> Env {
    let tmp = tempfile::tempdir().unwrap();
    let home = tmp.path().join("home");
    let cfg = tmp.path().join("cfg");
    let bin = tmp.path().join("bin");
    for d in [&home, &cfg, &bin] {
        std::fs::create_dir_all(d).unwrap();
    }
    let script = bin.join("rclone");
    std::fs::write(
        &script,
        r#"#!/bin/sh
case "$1" in
  version) echo "rclone v9.9.9-fake" ;;
  listremotes) echo "alpha:"; echo "beta:" ;;
  config) case "$2" in
      dump) echo '{"alpha":{"type":"drive"},"beta":{"type":"s3"}}' ;;
      *) : ;;
    esac ;;
  lsd) : ;;
  *) : ;;
esac
exit 0
"#,
    )
    .unwrap();
    std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
    Env {
        home: home.to_string_lossy().into(),
        cfg: cfg.to_string_lossy().into(),
        bin: bin.to_string_lossy().into(),
        _tmp: tmp,
    }
}

impl Env {
    fn cmd(&self) -> Command {
        let mut c = Command::cargo_bin("gn-drive").unwrap();
        c.env("HOME", &self.home)
            .env("GN_DRIVE_CONFIG_DIR", &self.cfg)
            .env("PATH", format!("{}:/usr/bin:/bin", self.bin))
            .env_remove("GN_DRIVE_PASSWORD");
        c
    }

    fn spawn_serve(&self, extra: &[&str]) -> Child {
        let exe = assert_cmd::cargo::cargo_bin("gn-drive");
        std::process::Command::new(exe)
            .args(["serve", "--stdio"])
            .args(extra)
            .env("HOME", &self.home)
            .env("GN_DRIVE_CONFIG_DIR", &self.cfg)
            .env("PATH", format!("{}:/usr/bin:/bin", self.bin))
            .env_remove("GN_DRIVE_PASSWORD")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .unwrap()
    }
}

#[test]
fn help_lists_subcommands() {
    let e = env();
    e.cmd().arg("--help").assert().success().stdout(
        predicate::str::contains("GN Drive")
            .and(predicate::str::contains("self-update"))
            .and(predicate::str::contains("completion"))
            .and(predicate::str::contains("doctor")),
    );
}

#[test]
fn no_args_prints_help_and_fails() {
    // clap writes the "missing subcommand" help to stderr.
    env()
        .cmd()
        .assert()
        .failure()
        .stderr(predicate::str::contains("Usage"));
}

#[test]
fn version_command_and_flag() {
    let e = env();
    e.cmd()
        .arg("version")
        .assert()
        .success()
        .stdout(predicate::str::is_match(r"^gn-drive \S+ \(commit=unknown, rust\)\n$").unwrap());
    e.cmd()
        .arg("--version")
        .assert()
        .success()
        .stdout(predicate::str::is_match(r"^\S+\n$").unwrap());
}

#[test]
fn completion_shells() {
    let e = env();
    for sh in ["bash", "zsh", "fish"] {
        e.cmd()
            .args(["completion", sh])
            .assert()
            .success()
            .stdout(predicate::str::contains("gn-drive"));
    }
    e.cmd()
        .args(["completion", "powershell"])
        .assert()
        .code(1)
        .stderr(predicate::str::contains(
            "unsupported shell: powershell (want bash|zsh|fish)",
        ));
}

#[test]
fn profile_crud_roundtrip() {
    let e = env();
    e.cmd()
        .args(["profile", "list"])
        .assert()
        .success()
        .stdout("No profiles configured. Use 'gn-drive profile add' or the app UI.\n");
    e.cmd()
        .args([
            "profile",
            "add",
            "--name",
            "p1",
            "--from",
            "/tmp/a",
            "--to",
            "alpha:b",
            "--bandwidth",
            "5",
            "--dry-run",
        ])
        .assert()
        .success()
        .stdout("✓ added profile \"p1\" (direction=push)\n");
    // default subcommand of `profile` is list
    e.cmd().arg("profile").assert().success().stdout(
        predicate::str::starts_with("NAME\tFROM\tTO\tPARALLEL\tBANDWIDTH\tDRY-RUN\n")
            .and(predicate::str::contains("p1\t/tmp/a\talpha:b\t4\t5M\ttrue")),
    );
    e.cmd()
        .args([
            "profile",
            "add",
            "--name",
            "p2",
            "--from",
            "x",
            "--to",
            "y",
            "--direction",
            "sideways",
        ])
        .assert()
        .code(1)
        .stderr(predicate::str::contains(
            "profile add: invalid --direction sideways (allowed: push, bi, bi-resync)",
        ));
    e.cmd()
        .args(["profile", "delete", "p1"])
        .assert()
        .success()
        .stdout("✓ deleted profile \"p1\"\n");
    e.cmd()
        .args(["profile", "list"])
        .assert()
        .success()
        .stdout(predicate::str::contains("No profiles"));
}

#[test]
fn remote_list_and_mutations_via_fake_rclone() {
    let e = env();
    e.cmd()
        .args(["remote", "list"])
        .assert()
        .success()
        .stdout("NAME\tTYPE\nalpha\tdrive\nbeta\ts3\n");
    e.cmd()
        .args([
            "remote", "add", "--name", "n1", "--type", "local", "--config", "a=b", "c=d",
        ])
        .assert()
        .success()
        .stdout("✓ added remote \"n1\" (type=local)\n");
    e.cmd()
        .args(["remote", "test", "alpha"])
        .assert()
        .success()
        .stdout("Testing remote \"alpha\"... ✓ OK\n");
    e.cmd()
        .args(["remote", "delete", "alpha"])
        .assert()
        .success()
        .stdout("✓ deleted remote \"alpha\"\n");
}

#[test]
fn sync_rejects_unknown_action() {
    let e = env();
    e.cmd()
        .args(["sync", "bogus", "--profile", "p"])
        .assert()
        .code(1)
        .stderr(predicate::str::contains("sync: unknown action bogus"));
}

#[test]
fn doctor_output_shape() {
    let e = env();
    e.cmd()
        .args(["doctor", "--data"])
        .assert()
        .success()
        .stdout(
            predicate::str::contains("=== gn-drive doctor ===")
                .and(predicate::str::contains("rclone v9.9.9-fake"))
                .and(predicate::str::contains(format!("Config dir:  {}", e.cfg)))
                .and(predicate::str::contains("configured: no  [OK]"))
                .and(predicate::str::contains("Remotes:      2 configured"))
                .and(predicate::str::contains("  - alpha (drive)"))
                .and(predicate::str::contains("Profiles:     0 configured"))
                .and(predicate::str::contains("--- data directory contents ---"))
                .and(predicate::str::ends_with(
                    "All checks passed. gn-drive is ready to run.\n",
                )),
        );
}

#[test]
fn service_status_with_temp_home() {
    let e = env();
    e.cmd()
        .args(["service", "status"])
        .assert()
        .success()
        .stdout("Service: not installed.\n\nTo install:\n  gn-drive service install\n");
    e.cmd()
        .args(["service", "frobnicate"])
        .assert()
        .code(1)
        .stderr(predicate::str::contains(
            "unknown service action: frobnicate (want install|uninstall|start|stop|status|restart)",
        ));
}

#[test]
fn serve_without_stdio_exits_2() {
    let e = env();
    e.cmd()
        .arg("serve")
        .assert()
        .code(2)
        .stderr(predicate::str::contains(
            "gn-drive serve: --stdio is required in protocol v1",
        ));
}

#[test]
fn serve_unsupported_protocol_exits_2() {
    let e = env();
    e.cmd()
        .args(["serve", "--stdio", "--protocol", "7"])
        .assert()
        .code(2)
        .stdout(predicate::str::contains("unsupported_protocol"));
}

fn read_line(r: &mut BufReader<std::process::ChildStdout>) -> serde_json::Value {
    let mut s = String::new();
    r.read_line(&mut s).unwrap();
    serde_json::from_str(s.trim()).unwrap_or_else(|_| panic!("bad frame: {s:?}"))
}

#[test]
fn serve_stdio_smoke_and_instance_busy() {
    let e = env();
    let mut child = e.spawn_serve(&[]);
    let mut stdin = child.stdin.take().unwrap();
    let mut out = BufReader::new(child.stdout.take().unwrap());

    let hello = read_line(&mut out);
    assert!(hello.to_string().contains("hello"), "{hello}");

    // Second instance on the same config dir → fatal instance_busy, exit 3.
    e.cmd()
        .args(["serve", "--stdio"])
        .assert()
        .code(3)
        .stdout(predicate::str::contains("instance_busy"));

    writeln!(stdin, r#"{{"id":1,"method":"ping","params":{{}}}}"#).unwrap();
    stdin.flush().unwrap();
    loop {
        let v = read_line(&mut out);
        if v.get("id").and_then(|i| i.as_i64()) == Some(1) {
            assert!(v.get("result").is_some(), "{v}");
            break;
        }
    }
    writeln!(stdin, r#"{{"id":2,"method":"app.quit","params":{{}}}}"#).unwrap();
    stdin.flush().unwrap();
    drop(stdin);
    let status = child.wait().unwrap();
    assert!(status.success(), "{status:?}");
}
