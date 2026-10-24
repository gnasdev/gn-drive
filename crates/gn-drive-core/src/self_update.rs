//! Self-update via GitHub Releases (port of `SelfUpdate.swift`).
//!
//! Pipeline: fetch latest release → pick the macOS CLI tarball + `.sha256`
//! sidecar → download → verify SHA256 → extract → atomic swap (keeps `<bin>.bak`).
//!
//! # Asset names (UC-156)
//! Swift only knew `gn-drive-darwin-<arm64|amd64>.tar.gz`, while
//! `.github/workflows/release.yml` publishes
//! `gn-drive-<VERSION>-macos-arm64.tar.gz(.sha256)` (tarball containing `gn-drive`
//! and `rclone`) and `GNDrive-<VERSION>-macos.zip(.sha256)` (the `.app`, NOT used
//! here: replacing a nested binary would break the bundle signature). This port
//! accepts both the legacy names and the release-workflow names for the CLI
//! tarball; see [`candidate_asset_names`]. CI only builds arm64, so on x86_64 the
//! updater reports `no matching asset` until an x86_64 asset is published.

use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;
use std::time::Duration;

pub const DEFAULT_REPO_OWNER: &str = "gnasdev";
pub const DEFAULT_REPO_NAME: &str = "gn-drive";
pub const API_BASE: &str = "https://api.github.com";
pub const HTTP_TIMEOUT: Duration = Duration::from_secs(60);
pub const DOWNLOAD_TIMEOUT: Duration = Duration::from_secs(300);
pub const RESTART_HINT: &str = "Restart gn-drive to use the new binary. Foreground: Ctrl+C and re-run. Service: 'gn-drive service restart'.";

#[derive(Debug, thiserror::Error)]
pub enum SelfUpdateError {
    #[error("selfupdate: no release found")]
    NoRelease,
    #[error("selfupdate: no asset matches current platform: {0}")]
    NoMatchingAsset(String),
    #[error("selfupdate: missing checksum asset {0}")]
    MissingChecksum(String),
    #[error("selfupdate: SHA256 mismatch")]
    ChecksumMismatch,
    #[error("selfupdate: already on latest version")]
    AlreadyUpToDate,
    #[error("selfupdate: HTTP {0}: {1}")]
    Http(i32, String),
    #[error("selfupdate: extract: {0}")]
    ExtractFailed(String),
    /// Transport / filesystem failure (no Swift equivalent: Swift surfaced the raw Foundation error).
    #[error("selfupdate: {0}")]
    Io(String),
}

impl From<std::io::Error> for SelfUpdateError {
    fn from(e: std::io::Error) -> Self {
        SelfUpdateError::Io(e.to_string())
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct GitHubAsset {
    pub name: String,
    pub browser_download_url: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct GitHubRelease {
    pub tag_name: String,
    pub assets: Vec<GitHubAsset>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct UpdateResult {
    pub updated: bool,
    pub old_version: String,
    pub new_version: String,
    pub binary_path: String,
    pub restart_hint: String,
}

pub struct HttpResponse {
    pub status: i32,
    pub body: Vec<u8>,
}

/// HTTP GET seam. `headers` are `(name, value)` pairs.
pub trait HttpFetcher: Send + Sync {
    fn get(
        &self,
        url: &str,
        headers: &[(String, String)],
        timeout: Duration,
    ) -> Result<HttpResponse, String>;
}

/// Real fetcher backed by `ureq` (rustls). Follows redirects manually and drops
/// the `Authorization` header when the redirect crosses hosts (GitHub → S3/CDN).
pub struct UreqFetcher;

fn host_of(url: &str) -> &str {
    url.split("://")
        .nth(1)
        .unwrap_or(url)
        .split('/')
        .next()
        .unwrap_or("")
}

impl HttpFetcher for UreqFetcher {
    fn get(
        &self,
        url: &str,
        headers: &[(String, String)],
        timeout: Duration,
    ) -> Result<HttpResponse, String> {
        let agent: ureq::Agent = ureq::Agent::config_builder()
            .http_status_as_error(false)
            .max_redirects(0)
            .max_redirects_will_error(false)
            .timeout_global(Some(timeout))
            .build()
            .into();
        let mut cur = url.to_string();
        let mut hdrs: Vec<(String, String)> = headers.to_vec();
        for _ in 0..10 {
            let mut req = agent.get(&cur);
            for (k, v) in &hdrs {
                req = req.header(k, v);
            }
            let mut resp = req.call().map_err(|e| e.to_string())?;
            let status = resp.status().as_u16() as i32;
            if matches!(status, 301 | 302 | 303 | 307 | 308) {
                let loc = resp
                    .headers()
                    .get("location")
                    .and_then(|v| v.to_str().ok())
                    .ok_or_else(|| "redirect without Location".to_string())?
                    .to_string();
                let next = if loc.starts_with("http://") || loc.starts_with("https://") {
                    loc
                } else if loc.starts_with('/') {
                    format!(
                        "{}://{}{}",
                        cur.split("://").next().unwrap_or("https"),
                        host_of(&cur),
                        loc
                    )
                } else {
                    return Err(format!("unsupported redirect target {loc}"));
                };
                if host_of(&next) != host_of(&cur) {
                    hdrs.retain(|(k, _)| !k.eq_ignore_ascii_case("authorization"));
                }
                cur = next;
                continue;
            }
            let body = resp
                .body_mut()
                .with_config()
                .limit(1 << 30)
                .read_to_vec()
                .map_err(|e| e.to_string())?;
            return Ok(HttpResponse { status, body });
        }
        Err("too many redirects".into())
    }
}

#[derive(Clone)]
pub struct UpdateOptions {
    pub repo_owner: String,
    pub repo_name: String,
    pub current_version: String,
    pub force: bool,
    pub token: Option<String>,
    pub log: Arc<dyn Fn(&str) + Send + Sync>,
    pub fetcher: Arc<dyn HttpFetcher>,
    /// API base URL (tests / GitHub Enterprise).
    pub api_base: String,
    /// CPU arch in Go naming (`arm64` | `amd64`).
    pub arch: String,
    /// Binary to replace; `None` = the running executable.
    pub target_binary: Option<String>,
}

impl Default for UpdateOptions {
    fn default() -> Self {
        UpdateOptions {
            repo_owner: DEFAULT_REPO_OWNER.into(),
            repo_name: DEFAULT_REPO_NAME.into(),
            current_version: String::new(),
            force: false,
            token: std::env::var("GITHUB_TOKEN").ok(),
            log: Arc::new(|s| println!("{s}")),
            fetcher: Arc::new(UreqFetcher),
            api_base: API_BASE.into(),
            arch: host_arch().into(),
            target_binary: None,
        }
    }
}

fn host_arch() -> &'static str {
    if cfg!(target_arch = "aarch64") {
        "arm64"
    } else {
        "amd64"
    }
}

fn trim_v(tag: &str) -> &str {
    tag.strip_prefix('v').unwrap_or(tag)
}

/// Check for a newer release without downloading. Returns `(current, latest)`.
pub fn check(opts: &UpdateOptions) -> Result<(String, String), SelfUpdateError> {
    let rel = fetch_release(opts)?;
    Ok((
        opts.current_version.clone(),
        trim_v(&rel.tag_name).to_string(),
    ))
}

/// Numeric dotted-version comparison (`1.10.0 > 1.9.2`; leading `v` and any
/// `-pre`/`+build` suffix ignored). Non-numeric segments compare as 0.
pub fn is_newer(latest: &str, current: &str) -> bool {
    fn parts(v: &str) -> Vec<u64> {
        let core = trim_v(v.trim()).split(['-', '+']).next().unwrap_or("");
        core.split('.')
            .map(|p| p.parse::<u64>().unwrap_or(0))
            .collect()
    }
    let (a, b) = (parts(latest), parts(current));
    for i in 0..a.len().max(b.len()) {
        let (x, y) = (
            a.get(i).copied().unwrap_or(0),
            b.get(i).copied().unwrap_or(0),
        );
        if x != y {
            return x > y;
        }
    }
    false
}

/// Full pipeline. `Err(AlreadyUpToDate)` when versions match (unless `force`).
pub fn update(opts: &UpdateOptions) -> Result<UpdateResult, SelfUpdateError> {
    let rel = fetch_release(opts)?;
    let new_version = trim_v(&rel.tag_name).to_string();

    if !opts.force && !opts.current_version.is_empty() && new_version == opts.current_version {
        return Err(SelfUpdateError::AlreadyUpToDate);
    }

    let (asset, sum_asset) = pick_assets(&rel, &new_version, &opts.arch)?;
    let stage = StageDir::create("gn-drive-update-")?;

    let archive_path = stage.path.join(&asset.name);
    (opts.log)(&format!("↓ downloading {}", asset.name));
    let body = download(opts, &asset.browser_download_url, DOWNLOAD_TIMEOUT)?;
    fs::write(&archive_path, &body)?;

    let expected = fetch_checksum(opts, &sum_asset)?;
    verify_digest(&body, &expected)?;

    let binary = extract_binary(&archive_path, &stage.path)?;

    let current = match &opts.target_binary {
        Some(p) => p.clone(),
        None => std::env::current_exe()?.to_string_lossy().into_owned(),
    };
    let current = fs::canonicalize(&current).unwrap_or_else(|_| PathBuf::from(&current));

    fs::set_permissions(&binary, fs::Permissions::from_mode(0o755))?;
    atomic_swap(&binary, &current)?;

    Ok(UpdateResult {
        updated: true,
        old_version: opts.current_version.clone(),
        new_version,
        binary_path: current.to_string_lossy().into_owned(),
        restart_hint: RESTART_HINT.into(),
    })
}

fn auth_headers(opts: &UpdateOptions, accept: Option<&str>) -> Vec<(String, String)> {
    let mut h = Vec::new();
    if let Some(a) = accept {
        h.push(("Accept".to_string(), a.to_string()));
    }
    if let Some(t) = opts.token.as_deref().filter(|t| !t.is_empty()) {
        h.push(("Authorization".to_string(), format!("Bearer {t}")));
    }
    h
}

fn fetch_release(opts: &UpdateOptions) -> Result<GitHubRelease, SelfUpdateError> {
    let url = format!(
        "{}/repos/{}/{}/releases/latest",
        opts.api_base, opts.repo_owner, opts.repo_name
    );
    let resp = opts
        .fetcher
        .get(
            &url,
            &auth_headers(opts, Some("application/vnd.github+json")),
            HTTP_TIMEOUT,
        )
        .map_err(SelfUpdateError::Io)?;
    if resp.status == 404 {
        return Err(SelfUpdateError::NoRelease);
    }
    if resp.status != 200 {
        let n = resp.body.len().min(1024);
        return Err(SelfUpdateError::Http(
            resp.status,
            String::from_utf8_lossy(&resp.body[..n]).into_owned(),
        ));
    }
    match serde_json::from_slice::<GitHubRelease>(&resp.body) {
        Ok(r) if !r.tag_name.is_empty() => Ok(r),
        _ => Err(SelfUpdateError::NoRelease),
    }
}

/// CLI tarball names for this platform, preferred first: legacy Swift/Go name,
/// then the names published by `release.yml`. `arch` is `arm64` or `amd64`.
pub fn candidate_asset_names(version: &str, arch: &str) -> Vec<String> {
    let mut v = vec![format!("gn-drive-darwin-{arch}.tar.gz")];
    v.push(format!("gn-drive-{version}-macos-{arch}.tar.gz"));
    if arch == "amd64" {
        v.push(format!("gn-drive-{version}-macos-x86_64.tar.gz"));
    }
    v
}

fn pick_assets(
    r: &GitHubRelease,
    version: &str,
    arch: &str,
) -> Result<(GitHubAsset, GitHubAsset), SelfUpdateError> {
    let names = candidate_asset_names(version, arch);
    let Some(bin) = names
        .iter()
        .find_map(|n| r.assets.iter().find(|a| &a.name == n))
    else {
        return Err(SelfUpdateError::NoMatchingAsset(names[0].clone()));
    };
    let sum_name = format!("{}.sha256", bin.name);
    let Some(sum) = r.assets.iter().find(|a| a.name == sum_name) else {
        return Err(SelfUpdateError::MissingChecksum(sum_name));
    };
    Ok((bin.clone(), sum.clone()))
}

fn download(
    opts: &UpdateOptions,
    url: &str,
    timeout: Duration,
) -> Result<Vec<u8>, SelfUpdateError> {
    let resp = opts
        .fetcher
        .get(url, &auth_headers(opts, None), timeout)
        .map_err(SelfUpdateError::Io)?;
    if resp.status != 200 {
        return Err(SelfUpdateError::Http(resp.status, String::new()));
    }
    Ok(resp.body)
}

/// Sidecar format: `<sha256hex>  <filename>`.
fn fetch_checksum(opts: &UpdateOptions, asset: &GitHubAsset) -> Result<String, SelfUpdateError> {
    let body = download(opts, &asset.browser_download_url, HTTP_TIMEOUT)?;
    Ok(String::from_utf8_lossy(&body)
        .split_whitespace()
        .next()
        .unwrap_or("")
        .to_string())
}

fn verify_digest(data: &[u8], expected_hex: &str) -> Result<(), SelfUpdateError> {
    let digest = hex::encode(Sha256::digest(data));
    if digest.eq_ignore_ascii_case(expected_hex.trim()) {
        Ok(())
    } else {
        Err(SelfUpdateError::ChecksumMismatch)
    }
}

/// Extract the `gn-drive` binary from a `.tar.gz`/`.tgz` (system `tar`) or `.zip` (system `unzip`).
fn extract_binary(archive: &Path, dest: &Path) -> Result<PathBuf, SelfUpdateError> {
    let name = archive.to_string_lossy().into_owned();
    let dest_s = dest.to_string_lossy().into_owned();
    let (tool, args): (&str, Vec<&str>) = if name.ends_with(".tar.gz") || name.ends_with(".tgz") {
        ("/usr/bin/tar", vec!["-xzf", &name, "-C", &dest_s])
    } else if name.ends_with(".zip") {
        ("/usr/bin/unzip", vec!["-o", &name, "-d", &dest_s])
    } else {
        return Err(SelfUpdateError::ExtractFailed(format!(
            "unsupported archive: {name}"
        )));
    };
    let status = Command::new(tool)
        .args(&args)
        .status()
        .map_err(|e| SelfUpdateError::ExtractFailed(e.to_string()))?;
    if !status.success() {
        let base = tool.rsplit('/').next().unwrap_or(tool);
        return Err(SelfUpdateError::ExtractFailed(format!(
            "{base} exit {}",
            status.code().unwrap_or(-1)
        )));
    }
    let direct = dest.join("gn-drive");
    if direct.is_file() {
        return Ok(direct);
    }
    // Fall back: any executable regular file that is not the archive itself.
    let mut entries: Vec<PathBuf> = fs::read_dir(dest)?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .collect();
    entries.sort();
    for p in entries {
        let n = p
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        if p.is_file()
            && !n.ends_with(".tar.gz")
            && fs::metadata(&p)
                .map(|m| m.permissions().mode() & 0o111 != 0)
                .unwrap_or(false)
        {
            return Ok(p);
        }
    }
    Err(SelfUpdateError::ExtractFailed(
        "no binary found in archive".into(),
    ))
}

/// Atomic binary replacement (POSIX rename; keeps `<bin>.bak`). The new binary is
/// first copied next to the target so the final rename never crosses filesystems.
fn atomic_swap(new_bin: &Path, current: &Path) -> Result<(), SelfUpdateError> {
    let bak = with_suffix(current, ".bak");
    let staged = with_suffix(current, ".new");
    let _ = fs::remove_file(&staged);
    fs::copy(new_bin, &staged)?;
    fs::set_permissions(&staged, fs::Permissions::from_mode(0o755))?;
    let _ = fs::remove_file(&bak);
    if let Err(e) = fs::rename(current, &bak) {
        let _ = fs::remove_file(&staged);
        return Err(e.into());
    }
    if let Err(e) = fs::rename(&staged, current) {
        let _ = fs::rename(&bak, current);
        let _ = fs::remove_file(&staged);
        return Err(e.into());
    }
    Ok(())
}

fn with_suffix(p: &Path, suffix: &str) -> PathBuf {
    let mut s = p.as_os_str().to_owned();
    s.push(suffix);
    PathBuf::from(s)
}

struct StageDir {
    path: PathBuf,
}

impl StageDir {
    fn create(prefix: &str) -> std::io::Result<StageDir> {
        let path = std::env::temp_dir().join(format!("{prefix}{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&path)?;
        Ok(StageDir { path })
    }
}

impl Drop for StageDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use parking_lot::Mutex;
    use std::collections::HashMap;

    type Request = (String, Vec<(String, String)>);

    #[derive(Default)]
    struct FakeHttp {
        routes: Mutex<HashMap<String, (i32, Vec<u8>)>>,
        requests: Mutex<Vec<Request>>,
    }

    impl FakeHttp {
        fn route(&self, url: &str, status: i32, body: impl Into<Vec<u8>>) {
            self.routes.lock().insert(url.into(), (status, body.into()));
        }
    }

    impl HttpFetcher for FakeHttp {
        fn get(
            &self,
            url: &str,
            headers: &[(String, String)],
            _t: Duration,
        ) -> Result<HttpResponse, String> {
            self.requests.lock().push((url.into(), headers.to_vec()));
            match self.routes.lock().get(url) {
                Some((s, b)) => Ok(HttpResponse {
                    status: *s,
                    body: b.clone(),
                }),
                None => Err(format!("no route {url}")),
            }
        }
    }

    const REL_URL: &str = "https://api.github.com/repos/gnasdev/gn-drive/releases/latest";

    fn release_json(tag: &str, names: &[&str]) -> String {
        let assets: Vec<_> = names
            .iter()
            .map(|n| serde_json::json!({"name": n, "browser_download_url": format!("https://dl.test/{n}")}))
            .collect();
        serde_json::json!({"tag_name": tag, "assets": assets}).to_string()
    }

    /// Build a tar.gz containing `gn-drive` (script) and `rclone`.
    fn make_tarball(dir: &Path, name: &str, payload: &str) -> Vec<u8> {
        let pkg = dir.join("pkg");
        fs::create_dir_all(&pkg).unwrap();
        fs::write(pkg.join("gn-drive"), payload).unwrap();
        fs::write(pkg.join("rclone"), "rclone").unwrap();
        let out = dir.join(name);
        let st = Command::new("/usr/bin/tar")
            .args([
                "czf",
                out.to_str().unwrap(),
                "-C",
                pkg.to_str().unwrap(),
                "gn-drive",
                "rclone",
            ])
            .status()
            .unwrap();
        assert!(st.success());
        fs::read(out).unwrap()
    }

    struct Env {
        _tmp: tempfile::TempDir,
        target: PathBuf,
        http: Arc<FakeHttp>,
        opts: UpdateOptions,
        logs: Arc<Mutex<Vec<String>>>,
    }

    fn env(asset_name: &str, tag: &str, payload: &str, sum_override: Option<&str>) -> Env {
        let tmp = tempfile::tempdir().unwrap();
        let target = tmp.path().join("gn-drive-installed");
        fs::write(&target, "OLD").unwrap();
        let tgz = make_tarball(tmp.path(), "build.tar.gz", payload);
        let sum = sum_override
            .map(String::from)
            .unwrap_or_else(|| hex::encode(Sha256::digest(&tgz)));
        let http = Arc::new(FakeHttp::default());
        let sum_name = format!("{asset_name}.sha256");
        http.route(
            REL_URL,
            200,
            release_json(tag, &[asset_name, &sum_name, "GNDrive-1.2.3-macos.zip"]),
        );
        http.route(&format!("https://dl.test/{asset_name}"), 200, tgz);
        http.route(
            &format!("https://dl.test/{sum_name}"),
            200,
            format!("{sum}  {asset_name}\n"),
        );
        let logs = Arc::new(Mutex::new(Vec::new()));
        let l = logs.clone();
        let opts = UpdateOptions {
            current_version: "1.0.0".into(),
            token: None,
            log: Arc::new(move |s| l.lock().push(s.to_string())),
            fetcher: http.clone(),
            arch: "arm64".into(),
            target_binary: Some(target.to_string_lossy().into_owned()),
            ..Default::default()
        };
        Env {
            _tmp: tmp,
            target,
            http,
            opts,
            logs,
        }
    }

    #[test]
    fn version_compare() {
        assert!(is_newer("1.10.0", "1.9.2"));
        assert!(is_newer("v0.5.0", "0.4.0"));
        assert!(!is_newer("0.4.0", "0.4.0"));
        assert!(!is_newer("0.4", "0.4.0"));
        assert!(!is_newer("0.3.9", "0.4.0"));
        assert!(is_newer("1.0.1-rc1", "1.0.0"));
    }

    #[test]
    fn check_strips_v_prefix() {
        let e = env("gn-drive-darwin-arm64.tar.gz", "v2.0.0", "x", None);
        assert_eq!(
            check(&e.opts).unwrap(),
            ("1.0.0".to_string(), "2.0.0".to_string())
        );
    }

    #[test]
    fn check_errors() {
        let http = Arc::new(FakeHttp::default());
        let opts = UpdateOptions {
            fetcher: http.clone(),
            token: None,
            ..Default::default()
        };
        http.route(REL_URL, 404, "");
        assert!(matches!(check(&opts), Err(SelfUpdateError::NoRelease)));
        http.route(REL_URL, 500, "kaboom");
        let e = check(&opts).unwrap_err();
        assert_eq!(e.to_string(), "selfupdate: HTTP 500: kaboom");
        http.route(REL_URL, 200, "not json");
        assert!(matches!(check(&opts), Err(SelfUpdateError::NoRelease)));
        http.route(REL_URL, 200, r#"{"tag_name":"","assets":[]}"#);
        assert!(matches!(check(&opts), Err(SelfUpdateError::NoRelease)));
    }

    #[test]
    fn token_sent_as_bearer_only_when_set() {
        let mut e = env("gn-drive-darwin-arm64.tar.gz", "v2.0.0", "x", None);
        e.opts.token = Some("tok".into());
        check(&e.opts).unwrap();
        let reqs = e.http.requests.lock();
        assert!(
            reqs[0]
                .1
                .contains(&("Authorization".into(), "Bearer tok".into()))
        );
        assert!(
            reqs[0]
                .1
                .contains(&("Accept".into(), "application/vnd.github+json".into()))
        );
        drop(reqs);
        e.http.requests.lock().clear();
        e.opts.token = Some(String::new());
        check(&e.opts).unwrap();
        assert!(
            e.http.requests.lock()[0]
                .1
                .iter()
                .all(|(k, _)| k != "Authorization")
        );
    }

    #[test]
    fn update_with_legacy_asset_name() {
        let e = env(
            "gn-drive-darwin-arm64.tar.gz",
            "v2.0.0",
            "#!/bin/sh\necho NEW\n",
            None,
        );
        let r = update(&e.opts).unwrap();
        assert!(r.updated);
        assert_eq!(
            (r.old_version.as_str(), r.new_version.as_str()),
            ("1.0.0", "2.0.0")
        );
        assert_eq!(r.restart_hint, RESTART_HINT);
        assert_eq!(
            fs::read_to_string(&e.target).unwrap(),
            "#!/bin/sh\necho NEW\n"
        );
        assert_eq!(
            fs::read_to_string(with_suffix(&e.target, ".bak")).unwrap(),
            "OLD"
        );
        assert_eq!(
            fs::metadata(&e.target).unwrap().permissions().mode() & 0o777,
            0o755
        );
        assert!(!with_suffix(&e.target, ".new").exists());
        assert_eq!(
            e.logs.lock().as_slice(),
            ["↓ downloading gn-drive-darwin-arm64.tar.gz"]
        );
    }

    #[test]
    fn update_with_release_workflow_asset_name() {
        let e = env(
            "gn-drive-2.0.0-macos-arm64.tar.gz",
            "v2.0.0",
            "NEWBIN",
            None,
        );
        let r = update(&e.opts).unwrap();
        assert_eq!(r.new_version, "2.0.0");
        assert_eq!(fs::read_to_string(&e.target).unwrap(), "NEWBIN");
    }

    #[test]
    fn update_checksum_mismatch_leaves_binary_untouched() {
        let e = env(
            "gn-drive-darwin-arm64.tar.gz",
            "v2.0.0",
            "NEW",
            Some(&"0".repeat(64)),
        );
        assert!(matches!(
            update(&e.opts),
            Err(SelfUpdateError::ChecksumMismatch)
        ));
        assert_eq!(fs::read_to_string(&e.target).unwrap(), "OLD");
        assert!(!with_suffix(&e.target, ".bak").exists());
    }

    #[test]
    fn already_up_to_date_unless_forced() {
        let mut e = env("gn-drive-darwin-arm64.tar.gz", "v1.0.0", "NEW", None);
        assert!(matches!(
            update(&e.opts),
            Err(SelfUpdateError::AlreadyUpToDate)
        ));
        assert_eq!(fs::read_to_string(&e.target).unwrap(), "OLD");
        e.opts.force = true;
        assert!(update(&e.opts).unwrap().updated);
        // empty current version never short-circuits
        let mut e2 = env("gn-drive-darwin-arm64.tar.gz", "v1.0.0", "NEW", None);
        e2.opts.current_version = String::new();
        assert!(update(&e2.opts).unwrap().updated);
    }

    #[test]
    fn no_matching_asset_and_missing_checksum() {
        let http = Arc::new(FakeHttp::default());
        let opts = UpdateOptions {
            fetcher: http.clone(),
            token: None,
            arch: "arm64".into(),
            ..Default::default()
        };
        http.route(
            REL_URL,
            200,
            release_json(
                "v2.0.0",
                &[
                    "GNDrive-2.0.0-macos.zip",
                    "gn-drive-2.0.0-linux-amd64.tar.gz",
                ],
            ),
        );
        let e = update(&opts).unwrap_err();
        assert_eq!(
            e.to_string(),
            "selfupdate: no asset matches current platform: gn-drive-darwin-arm64.tar.gz"
        );
        http.route(
            REL_URL,
            200,
            release_json("v2.0.0", &["gn-drive-2.0.0-macos-arm64.tar.gz"]),
        );
        let e = update(&opts).unwrap_err();
        assert_eq!(
            e.to_string(),
            "selfupdate: missing checksum asset gn-drive-2.0.0-macos-arm64.tar.gz.sha256"
        );
    }

    #[test]
    fn candidate_names() {
        assert_eq!(
            candidate_asset_names("0.5.0", "arm64"),
            [
                "gn-drive-darwin-arm64.tar.gz",
                "gn-drive-0.5.0-macos-arm64.tar.gz"
            ]
        );
        assert!(
            candidate_asset_names("0.5.0", "amd64")
                .contains(&"gn-drive-0.5.0-macos-x86_64.tar.gz".to_string())
        );
    }

    #[test]
    fn download_http_error() {
        let e = env("gn-drive-darwin-arm64.tar.gz", "v2.0.0", "NEW", None);
        e.http
            .route("https://dl.test/gn-drive-darwin-arm64.tar.gz", 403, "");
        assert_eq!(
            update(&e.opts).unwrap_err().to_string(),
            "selfupdate: HTTP 403: "
        );
        assert_eq!(fs::read_to_string(&e.target).unwrap(), "OLD");
    }

    #[test]
    fn extract_rejects_unsupported_and_corrupt() {
        let d = tempfile::tempdir().unwrap();
        let rar = d.path().join("a.rar");
        fs::write(&rar, "x").unwrap();
        assert!(matches!(
            extract_binary(&rar, d.path()),
            Err(SelfUpdateError::ExtractFailed(_))
        ));
        let bad = d.path().join("bad.tar.gz");
        fs::write(&bad, "not gzip").unwrap();
        assert!(matches!(
            extract_binary(&bad, d.path()),
            Err(SelfUpdateError::ExtractFailed(_))
        ));
    }

    #[test]
    fn checksum_parsing_tolerates_whitespace_and_case() {
        assert!(verify_digest(b"abc", &hex::encode(Sha256::digest(b"abc")).to_uppercase()).is_ok());
        assert!(verify_digest(b"abc", "  ").is_err());
    }
}
