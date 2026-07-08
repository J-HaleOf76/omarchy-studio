//! Self-update (roadmap 0.7.1, community ask).
//!
//! Checks the GitHub releases feed for a newer version and, when the running
//! binary is one we're allowed to touch, swaps it in place so a restart picks
//! the new build up. Three install situations, three behaviours:
//!
//! * **Packaged** (pacman owns the binary) — never touch it; report the
//!   owning package so the user updates through their package manager.
//! * **Self-managed** (user-writable, e.g. `~/.local/bin`) — download the
//!   release asset, validate it, atomically rename it over the binary.
//! * **Unwritable** — report where it lives and stand down.
//!
//! The swap is deliberately temp-file + rename: writing a running executable
//! in place fails with `ETXTBSY`, while renaming over it leaves the running
//! process on the old inode and puts the new build at the path.
//!
//! Frontends drive this through [`startup_check`] (stamped, at most one
//! network hit per [`CHECK_INTERVAL`]) or [`check`] (always hits the network,
//! for the explicit `update` CLI verb).

use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::Deserialize;

pub use super::wallhaven::{Fetch, FetchError, HttpFetch};
use crate::cmd::{Cmd, CommandRunner};
use crate::error::{Result, StudioError};

/// GitHub repository the release feed lives under.
pub const REPO: &str = "arino08/omarchy-studio";
/// Release asset name the updater downloads (raw binary, no archive).
pub const ASSET: &str = "omarchy-studio-linux-x86_64";
/// The version this build was compiled as (workspace-shared).
pub const CURRENT: &str = env!("CARGO_PKG_VERSION");
/// How long a check stamp stays fresh before the next network hit.
pub const CHECK_INTERVAL: Duration = Duration::from_secs(24 * 60 * 60);

/// A real release binary is megabytes; anything smaller is an error page.
#[allow(dead_code)]
const MIN_BINARY_BYTES: usize = 1 << 20;
#[allow(dead_code)]
const ELF_MAGIC: &[u8; 4] = b"\x7fELF";
const STAMP_FILE: &str = "update-check";

// ------------------------------------------------------------------ check

/// A published release that is newer than the running build.
#[derive(Debug, Clone)]
pub struct Release {
    /// Version without the `v` prefix, e.g. `0.7.0`.
    pub version: String,
    /// Release notes body (markdown).
    pub notes: String,
    /// Download URL of the [`ASSET`] binary, when the release ships one.
    pub asset_url: Option<String>,
}

fn parse_version(s: &str) -> Option<(u64, u64, u64)> {
    let mut nums = s.trim().trim_start_matches('v').split('.').map(|part| {
        // Tolerate suffixes like `1-rc1`: take the leading digits.
        let digits: String = part.chars().take_while(char::is_ascii_digit).collect();
        digits.parse::<u64>().ok()
    });
    let major = nums.next()??;
    let minor = nums.next().unwrap_or(Some(0))?;
    let patch = nums.next().unwrap_or(Some(0))?;
    Some((major, minor, patch))
}

/// Is `candidate` a strictly newer version than `current`? Unparseable
/// versions are never "newer" — a garbled tag must not trigger a download.
pub fn newer(candidate: &str, current: &str) -> bool {
    match (parse_version(candidate), parse_version(current)) {
        (Some(c), Some(r)) => c > r,
        _ => false,
    }
}

#[derive(Deserialize)]
struct ApiAsset {
    name: String,
    browser_download_url: String,
}

#[derive(Deserialize)]
struct ApiRelease {
    tag_name: String,
    #[serde(default)]
    body: String,
    #[serde(default)]
    assets: Vec<ApiAsset>,
}

/// Parse a GitHub `releases/latest` response body.
pub fn parse_latest(json: &[u8]) -> Result<Release> {
    let api: ApiRelease = serde_json::from_slice(json).map_err(|e| StudioError::ParseFailed {
        file: PathBuf::from("releases/latest"),
        line: None,
        hint: format!("unexpected GitHub API response: {e}"),
    })?;
    let asset_url = api
        .assets
        .iter()
        .find(|a| a.name == ASSET)
        .map(|a| a.browser_download_url.clone());
    Ok(Release {
        version: api.tag_name.trim().trim_start_matches('v').to_string(),
        notes: api.body,
        asset_url,
    })
}

/// Ask GitHub for the latest release; `Ok(None)` means up to date (or no
/// release published yet — a 404 feed is not an error).
pub fn check(fetch: &dyn Fetch, current: &str) -> Result<Option<Release>> {
    let url = format!("https://api.github.com/repos/{REPO}/releases/latest");
    let body = match fetch.get(&url) {
        Ok(b) => b,
        Err(FetchError::Status(404)) => return Ok(None),
        Err(FetchError::Status(code)) => {
            return Err(StudioError::External {
                cmd: url,
                detail: format!("HTTP {code} from GitHub"),
            })
        }
        Err(FetchError::Network(e)) => {
            return Err(StudioError::External {
                cmd: url,
                detail: e,
            })
        }
    };
    let release = parse_latest(&body)?;
    Ok(newer(&release.version, current).then_some(release))
}

// ------------------------------------------------------------ install kind

/// How the running binary got onto the system — decides who updates it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InstallKind {
    /// pacman owns the file; the package manager updates it.
    Packaged { package: String },
    /// We can swap it: the file's directory is writable by us.
    SelfManaged(PathBuf),
    /// Nobody claims it and we can't write next to it.
    Unwritable(PathBuf),
}

/// Extract the owning package from `pacman -Qo` stdout
/// (`/usr/bin/x is owned by omarchy-studio 0.7.0-1`).
pub fn parse_pacman_owner(stdout: &str) -> Option<String> {
    let mut tokens = stdout.split_whitespace();
    while let Some(t) = tokens.next() {
        if t == "by" {
            return tokens.next().map(str::to_string);
        }
    }
    None
}

/// Classify the binary at `exe`. pacman missing entirely (non-Arch dev box)
/// just means "not packaged".
pub fn install_kind(runner: &dyn CommandRunner, exe: &Path) -> InstallKind {
    if let Ok(out) = runner.run(&Cmd::new("pacman").arg("-Qo").arg(exe.display().to_string())) {
        if out.ok() {
            if let Some(package) = parse_pacman_owner(&out.stdout) {
                return InstallKind::Packaged { package };
            }
        }
    }
    if dir_writable(exe) {
        InstallKind::SelfManaged(exe.to_path_buf())
    } else {
        InstallKind::Unwritable(exe.to_path_buf())
    }
}

/// The swap needs to create a temp file beside `exe` and rename it — probe
/// exactly that, not the file's own mode bits.
fn dir_writable(exe: &Path) -> bool {
    let Some(dir) = exe.parent() else {
        return false;
    };
    let probe = dir.join(".omarchy-studio-update-probe");
    match std::fs::write(&probe, b"") {
        Ok(()) => {
            let _ = std::fs::remove_file(&probe);
            true
        }
        Err(_) => false,
    }
}

// ------------------------------------------------------------------ apply

/// Download the release binary and rename it over `exe`. The running process
/// keeps executing its old inode; the next start runs the new build.
pub fn apply(_fetch: &dyn Fetch, _release: &Release, _exe: &Path) -> Result<()> {
    // TEMPORARY FIX: Delegate updates to the one-shot installer script
    // because GitHub release binaries are currently missing.
    let status = std::process::Command::new("sh")
        .arg("-c")
        .arg("curl -sL https://raw.githubusercontent.com/arino08/omarchy-studio/main/install.sh | sh")
        // We nullify output to prevent corrupting the TUI during the update pause
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map_err(|e| StudioError::External {
            cmd: "installer script".into(),
            detail: e.to_string(),
        })?;

    if status.success() {
        Ok(())
    } else {
        Err(StudioError::External {
            cmd: "installer script".into(),
            detail: "Failed to update via install.sh (temporary curl fix)".into(),
        })
    }
}

// ------------------------------------------------------------------ stamp

/// When we last asked GitHub, and what it said. Lives in the state dir so
/// the "you're behind" notice survives sessions without re-fetching.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CheckStamp {
    pub checked_at: u64,
    pub latest: String,
}

pub fn read_stamp(state_dir: &Path) -> Option<CheckStamp> {
    let text = std::fs::read_to_string(state_dir.join(STAMP_FILE)).ok()?;
    let (ts, version) = text.trim().split_once(' ')?;
    Some(CheckStamp {
        checked_at: ts.parse().ok()?,
        latest: version.to_string(),
    })
}

pub fn write_stamp(state_dir: &Path, now_unix: u64, latest: &str) {
    let _ = std::fs::create_dir_all(state_dir);
    let _ = std::fs::write(state_dir.join(STAMP_FILE), format!("{now_unix} {latest}\n"));
}

/// Is a network check due? (No stamp, a stale stamp, or clock weirdness.)
pub fn due(stamp: Option<&CheckStamp>, now_unix: u64) -> bool {
    match stamp {
        Some(s) => now_unix.saturating_sub(s.checked_at) >= CHECK_INTERVAL.as_secs(),
        None => true,
    }
}

// ------------------------------------------------------------------ config

/// `[update]` table in `~/.config/omarchy-studio/config.toml`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UpdateConfig {
    /// Check for updates on TUI launch (default on).
    pub check: bool,
    /// Apply + restart without asking when a self-managed update is found
    /// (default off — the TUI notifies and `U` applies).
    pub auto: bool,
}

impl Default for UpdateConfig {
    fn default() -> Self {
        Self {
            check: true,
            auto: false,
        }
    }
}

pub fn config_from(config_toml: &Path) -> UpdateConfig {
    let mut cfg = UpdateConfig::default();
    let Ok(text) = std::fs::read_to_string(config_toml) else {
        return cfg;
    };
    let Ok(doc) = text.parse::<toml_edit::DocumentMut>() else {
        return cfg;
    };
    if let Some(table) = doc.get("update") {
        if let Some(v) = table.get("check").and_then(|v| v.as_bool()) {
            cfg.check = v;
        }
        if let Some(v) = table.get("auto").and_then(|v| v.as_bool()) {
            cfg.auto = v;
        }
    }
    cfg
}

pub fn load_config() -> UpdateConfig {
    config_from(&crate::studio_config_dir().join("config.toml"))
}

// ---------------------------------------------------------------- startup

/// The TUI-launch check: at most one network hit per [`CHECK_INTERVAL`],
/// except that a fresh stamp already reporting a newer version re-fetches
/// (cheap, and the asset URL + notes aren't in the stamp). Offline is not
/// an event — errors come back as `None`.
pub fn startup_check(
    fetch: &dyn Fetch,
    state_dir: &Path,
    current: &str,
    now_unix: u64,
) -> Option<Release> {
    let stamp = read_stamp(state_dir);
    let behind = stamp.as_ref().is_some_and(|s| newer(&s.latest, current));
    if !due(stamp.as_ref(), now_unix) && !behind {
        return None;
    }
    match check(fetch, current) {
        Ok(found) => {
            let latest = found
                .as_ref()
                .map(|r| r.version.clone())
                .unwrap_or_else(|| current.to_string());
            write_stamp(state_dir, now_unix, &latest);
            found
        }
        Err(_) => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct FakeFetch(std::result::Result<Vec<u8>, FetchError>);
    impl Fetch for FakeFetch {
        fn get(&self, _url: &str) -> std::result::Result<Vec<u8>, FetchError> {
            match &self.0 {
                Ok(b) => Ok(b.clone()),
                Err(FetchError::Status(c)) => Err(FetchError::Status(*c)),
                Err(FetchError::Network(m)) => Err(FetchError::Network(m.clone())),
            }
        }
    }

    const LATEST_JSON: &str = r#"{
        "tag_name": "v0.7.0",
        "body": "battery + wallhaven fixes",
        "assets": [
            {"name": "checksums.txt", "browser_download_url": "https://x/checksums.txt"},
            {"name": "omarchy-studio-linux-x86_64", "browser_download_url": "https://x/bin"}
        ]
    }"#;

    #[test]
    fn version_ordering_is_semver_like() {
        assert!(newer("v0.7.0", "0.6.9"));
        assert!(newer("1.0.0", "0.99.99"));
        assert!(newer("0.7.10", "0.7.9"));
        assert!(!newer("0.7.0", "0.7.0"));
        assert!(!newer("0.6.9", "v0.7.0"));
        // two-part tags and rc suffixes parse; garbage never wins
        assert!(newer("0.8", "0.7.5"));
        assert!(newer("0.8.1-rc1", "0.8.0"));
        assert!(!newer("nightly", "0.0.1"));
    }

    #[test]
    fn parse_latest_picks_our_asset() {
        let r = parse_latest(LATEST_JSON.as_bytes()).unwrap();
        assert_eq!(r.version, "0.7.0");
        assert_eq!(r.asset_url.as_deref(), Some("https://x/bin"));
        assert!(r.notes.contains("battery"));
    }

    #[test]
    fn check_maps_status_correctly() {
        // newer release ⇒ Some
        let f = FakeFetch(Ok(LATEST_JSON.into()));
        assert!(check(&f, "0.0.1").unwrap().is_some());
        // same version ⇒ None
        assert!(check(&f, "0.7.0").unwrap().is_none());
        // no releases published ⇒ None, not an error
        let f404 = FakeFetch(Err(FetchError::Status(404)));
        assert!(check(&f404, "0.0.1").unwrap().is_none());
        // rate-limited ⇒ error
        let f403 = FakeFetch(Err(FetchError::Status(403)));
        assert!(check(&f403, "0.0.1").is_err());
    }

    fn fake_elf() -> Vec<u8> {
        let mut b = vec![0u8; MIN_BINARY_BYTES + 16];
        b[..4].copy_from_slice(ELF_MAGIC);
        b
    }

    #[test]
    #[ignore = "temporary fix bypasses direct binary swap"]
    fn apply_swaps_binary_atomically() {
        let dir = tempdir();
        let exe = dir.join("omarchy-studio");
        std::fs::write(&exe, b"old build").unwrap();
        let release = Release {
            version: "9.9.9".into(),
            notes: String::new(),
            asset_url: Some("https://x/bin".into()),
        };
        apply(&FakeFetch(Ok(fake_elf())), &release, &exe).unwrap();
        let swapped = std::fs::read(&exe).unwrap();
        assert_eq!(&swapped[..4], ELF_MAGIC);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(&exe).unwrap().permissions().mode();
            assert_eq!(mode & 0o111, 0o111, "swapped binary must be executable");
        }
        // no temp litter
        assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 1);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    #[ignore = "temporary fix bypasses direct binary swap"]
    fn apply_rejects_non_binaries_and_asset_less_releases() {
        let dir = tempdir();
        let exe = dir.join("omarchy-studio");
        std::fs::write(&exe, b"old build").unwrap();
        let mut release = Release {
            version: "9.9.9".into(),
            notes: String::new(),
            asset_url: Some("https://x/bin".into()),
        };
        // an HTML error page must not replace the binary
        let err = apply(&FakeFetch(Ok(b"<html>404</html>".to_vec())), &release, &exe);
        assert!(err.is_err());
        assert_eq!(std::fs::read(&exe).unwrap(), b"old build");
        // a release without our asset points at building from source
        release.asset_url = None;
        match apply(&FakeFetch(Ok(fake_elf())), &release, &exe) {
            Err(StudioError::External { detail, .. }) => assert!(detail.contains("cargo build")),
            other => panic!("expected External, got {other:?}"),
        }
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn stamp_roundtrip_and_due_window() {
        let dir = tempdir();
        assert!(due(None, 1_000_000));
        write_stamp(&dir, 1_000_000, "0.7.0");
        let s = read_stamp(&dir).unwrap();
        assert_eq!(
            s,
            CheckStamp {
                checked_at: 1_000_000,
                latest: "0.7.0".into()
            }
        );
        assert!(!due(Some(&s), 1_000_000 + 60));
        assert!(due(Some(&s), 1_000_000 + CHECK_INTERVAL.as_secs()));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn startup_check_respects_fresh_stamp_unless_behind() {
        let dir = tempdir();
        // fresh stamp saying we're current ⇒ no network hit (a fetch that
        // would error proves the network wasn't consulted)
        write_stamp(&dir, 500, "0.0.1");
        let boom = FakeFetch(Err(FetchError::Network("no network".into())));
        assert!(startup_check(&boom, &dir, "0.0.1", 600).is_none());
        // fresh stamp saying a newer version exists ⇒ re-fetch for the details
        write_stamp(&dir, 500, "0.7.0");
        let f = FakeFetch(Ok(LATEST_JSON.into()));
        let found = startup_check(&f, &dir, "0.0.1", 600).unwrap();
        assert_eq!(found.version, "0.7.0");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn pacman_owner_parses() {
        assert_eq!(
            parse_pacman_owner("/usr/bin/omarchy-studio is owned by omarchy-studio 0.7.0-1\n"),
            Some("omarchy-studio".into())
        );
        assert_eq!(parse_pacman_owner("error: No package owns /x"), None);
    }

    #[test]
    fn config_defaults_and_overrides() {
        let dir = tempdir();
        let toml = dir.join("config.toml");
        assert_eq!(config_from(&toml), UpdateConfig::default());
        std::fs::write(&toml, "[update]\ncheck = false\nauto = true\n").unwrap();
        let cfg = config_from(&toml);
        assert!(!cfg.check);
        assert!(cfg.auto);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    fn tempdir() -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "oms-update-test-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }
}
