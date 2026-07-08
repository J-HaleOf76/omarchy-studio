//! Power profiles & AC/battery auto-switch (roadmap 0.8.6).
//!
//! Wraps `powerprofilesctl` (power-profiles-daemon) to read and switch the
//! active profile, persist a choice across logins via an `exec-once` managed
//! block in the user's `autostart.conf`, and render the udev rule that flips
//! the profile automatically on AC plug/unplug. Root writes (the udev rule)
//! are always rendered for preview first — the frontend shows the exact text
//! and command before anything runs; this module never shells out to `sudo`.

use std::path::PathBuf;

use crate::cmd::{find_in_path, Cmd, CommandRunner};
use crate::configfs::{CommentStyle, ManagedBlock};
use crate::error::{Result, StudioError};
use crate::omarchy::OmarchyPaths;

/// A power profile as reported by `powerprofilesctl`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Profile {
    pub name: String,
    pub active: bool,
}

/// Is power-profiles-daemon's CLI available? Gates the whole feature.
pub fn available() -> bool {
    find_in_path("powerprofilesctl").is_some()
}

/// Parse `powerprofilesctl list`. The active profile's line is prefixed with
/// `* `; each profile header is `name:` (indented details follow, ignored).
pub fn parse_list(stdout: &str) -> Vec<Profile> {
    let mut out = Vec::new();
    for line in stdout.lines() {
        let active = line.trim_start().starts_with('*');
        let stripped = line.trim_start().trim_start_matches('*').trim();
        // A profile header is a bare `name:` — details are `key: value`.
        if let Some(name) = stripped.strip_suffix(':') {
            if !name.is_empty() && !name.contains(char::is_whitespace) {
                out.push(Profile {
                    name: name.to_string(),
                    active,
                });
            }
        }
    }
    out
}

/// Query the available profiles and which is active.
pub fn list(runner: &dyn CommandRunner) -> Result<Vec<Profile>> {
    let out = runner.run(&Cmd::new("powerprofilesctl").arg("list"))?;
    if !out.ok() {
        return Err(StudioError::External {
            cmd: "powerprofilesctl list".into(),
            detail: out.stderr.trim().to_string(),
        });
    }
    Ok(parse_list(&out.stdout))
}

/// The active profile name, if any.
pub fn active(runner: &dyn CommandRunner) -> Option<String> {
    let out = runner.run(&Cmd::new("powerprofilesctl").arg("get")).ok()?;
    let name = out.stdout.trim();
    (out.ok() && !name.is_empty()).then(|| name.to_string())
}

/// The command that switches the active profile (polkit-authorized, no sudo).
pub fn set_cmd(name: &str) -> Cmd {
    Cmd::new("powerprofilesctl").args(["set", name])
}

// ------------------------------------------------------------ login persistence

const AUTOSTART_SECTION: &str = "power-profile";

/// The user's `autostart.conf` — Hyprland sources it last, so an `exec-once`
/// here runs on login.
pub fn autostart_path(paths: &OmarchyPaths) -> PathBuf {
    paths.hypr_config().join("autostart.conf")
}

fn autostart_block() -> ManagedBlock {
    ManagedBlock::new(AUTOSTART_SECTION, CommentStyle::Hash)
}

/// Set (`Some(name)`) or clear (`None`) the login-persisted profile: an
/// `exec-once = powerprofilesctl set <name>` in a managed block. Returns the
/// file when it changed.
pub fn set_persist(paths: &OmarchyPaths, name: Option<&str>) -> Result<Vec<PathBuf>> {
    let path = autostart_path(paths);
    let existing = std::fs::read_to_string(&path).unwrap_or_default();
    let block = autostart_block();
    let updated = match name {
        Some(n) => block.upsert(&existing, &format!("exec-once = powerprofilesctl set {n}")),
        None => block.remove(&existing),
    };
    if updated == existing {
        return Ok(Vec::new());
    }
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    crate::configfs::atomic_write(&path, &updated)?;
    Ok(vec![path])
}

/// The login-persisted profile name, if one is set.
pub fn persisted(paths: &OmarchyPaths) -> Option<String> {
    let text = std::fs::read_to_string(autostart_path(paths)).ok()?;
    let body = autostart_block().extract(&text)?;
    body.split_whitespace().last().map(str::to_string)
}

// ------------------------------------------------------------ AC/battery udev

/// Where the auto-switch rule lives (root-owned).
pub const UDEV_PATH: &str = "/etc/udev/rules.d/99-power-profile.rules";

/// Render the udev rule that switches profiles on AC plug/unplug. `on_ac` runs
/// when mains power appears, `on_battery` when it's removed.
pub fn udev_rule(on_ac: &str, on_battery: &str) -> String {
    format!(
        "# Managed by omarchy-studio — AC/battery power-profile auto-switch\n\
         SUBSYSTEM==\"power_supply\", ATTR{{online}}==\"1\", \
         RUN+=\"/usr/bin/powerprofilesctl set {on_ac}\"\n\
         SUBSYSTEM==\"power_supply\", ATTR{{online}}==\"0\", \
         RUN+=\"/usr/bin/powerprofilesctl set {on_battery}\"\n"
    )
}

/// The exact command that installs the rule (shown before it runs). Uses
/// `printf '%b'` so the escaped newlines in the debug-quoted rule expand back
/// to real ones when written.
pub fn install_rule_cmd(rule: &str) -> String {
    format!("printf '%b' {:?} | sudo tee {UDEV_PATH} >/dev/null", rule)
}

/// The command that removes the rule.
pub fn remove_rule_cmd() -> String {
    format!("sudo rm -f {UDEV_PATH}")
}

#[cfg(test)]
mod tests {
    use super::*;

    const LIST: &str = "* performance:\n    CpuDriver:\tamd_pstate\n\n  balanced:\n    CpuDriver:\tamd_pstate\n\n  power-saver:\n    CpuDriver:\tamd_pstate\n";

    fn fake_paths(tag: &str) -> (OmarchyPaths, PathBuf) {
        let root = std::env::temp_dir().join(format!(
            "omarchy-studio-power-{tag}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&root).unwrap();
        let paths = OmarchyPaths {
            system: root.join("sys"),
            config: root.join("cfg/omarchy"),
            state: root.join("cfg/state"),
        };
        (paths, root)
    }

    #[test]
    fn parse_list_reads_profiles_and_active() {
        let ps = parse_list(LIST);
        assert_eq!(ps.len(), 3);
        assert_eq!(ps[0].name, "performance");
        assert!(ps[0].active);
        assert_eq!(ps[1].name, "balanced");
        assert!(!ps[1].active);
        assert_eq!(ps[2].name, "power-saver");
    }

    #[test]
    fn set_cmd_shape() {
        assert_eq!(
            set_cmd("balanced").display(),
            "powerprofilesctl set balanced"
        );
    }

    #[test]
    fn persist_round_trips() {
        let (paths, _root) = fake_paths("persist");
        assert!(persisted(&paths).is_none());
        let files = set_persist(&paths, Some("power-saver")).unwrap();
        assert_eq!(files.len(), 1);
        assert_eq!(persisted(&paths).as_deref(), Some("power-saver"));
        let text = std::fs::read_to_string(&files[0]).unwrap();
        assert!(text.contains("exec-once = powerprofilesctl set power-saver"));
        // idempotent
        assert!(set_persist(&paths, Some("power-saver")).unwrap().is_empty());
        // clear
        set_persist(&paths, None).unwrap();
        assert!(persisted(&paths).is_none());
    }

    #[test]
    fn persist_preserves_user_autostart_lines() {
        let (paths, _root) = fake_paths("coexist");
        let path = autostart_path(&paths);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, "exec-once = my-own-thing\n").unwrap();
        set_persist(&paths, Some("balanced")).unwrap();
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(text.contains("exec-once = my-own-thing"));
        assert!(text.contains("powerprofilesctl set balanced"));
        set_persist(&paths, None).unwrap();
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(text.contains("exec-once = my-own-thing"));
        assert!(!text.contains("powerprofilesctl"));
    }

    #[test]
    fn udev_rule_golden() {
        let rule = udev_rule("performance", "balanced");
        assert!(rule.contains("ATTR{online}==\"1\""));
        assert!(rule.contains("powerprofilesctl set performance"));
        assert!(rule.contains("ATTR{online}==\"0\""));
        assert!(rule.contains("powerprofilesctl set balanced"));
        assert!(rule.starts_with("# Managed by omarchy-studio"));
    }

    #[test]
    fn install_cmd_targets_udev_path() {
        let cmd = install_rule_cmd(&udev_rule("performance", "balanced"));
        assert!(cmd.contains("sudo tee /etc/udev/rules.d/99-power-profile.rules"));
    }
}
