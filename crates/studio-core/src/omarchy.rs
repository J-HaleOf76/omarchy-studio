//! Omarchy adapter (spec 02). The ONLY module allowed to know Omarchy paths
//! and command names — CI greps enforce this (spec 02 §6).

use std::path::PathBuf;

use crate::cmd::{find_in_path, Cmd, CommandRunner};

/// Resolved Omarchy locations (spec 02 §1).
pub struct OmarchyPaths {
    /// `~/.local/share/omarchy` — `$OMARCHY_PATH`. READ-ONLY, always.
    pub system: PathBuf,
    /// `~/.config/omarchy`
    pub config: PathBuf,
    /// `~/.local/state/omarchy`
    pub state: PathBuf,
}

impl OmarchyPaths {
    pub fn discover() -> crate::error::Result<Self> {
        let home = std::env::var_os("HOME").map(PathBuf::from).ok_or_else(|| {
            crate::StudioError::External {
                cmd: "env".into(),
                detail: "HOME is not set".into(),
            }
        })?;
        Ok(Self {
            system: std::env::var_os("OMARCHY_PATH")
                .map(PathBuf::from)
                .unwrap_or_else(|| home.join(".local/share/omarchy")),
            config: home.join(".config/omarchy"),
            state: home.join(".local/state/omarchy"),
        })
    }

    /// Is there an Omarchy install here at all? (Gates everything else.)
    pub fn installed(&self) -> bool {
        self.system.join("version").is_file()
    }

    /// Slug of the active theme (`current/theme.name`) — spec 02 §1.
    pub fn current_theme_name(&self) -> crate::error::Result<String> {
        let p = self.config.join("current/theme.name");
        Ok(std::fs::read_to_string(p)?.trim().to_string())
    }

    /// `~/.config/hypr` — the user's own Hyprland config, sourced last and thus
    /// the right place for Studio's keybind/looknfeel overrides. Derived as a
    /// sibling of `config` (`~/.config/omarchy` → `~/.config/hypr`).
    pub fn hypr_config(&self) -> PathBuf {
        self.config
            .parent()
            .map(|p| p.join("hypr"))
            .unwrap_or_else(|| PathBuf::from("hypr"))
    }
}

/// Capability probe result (spec 02 §5).
///
/// Probing is cheap (< 20 ms: two fast execs + a few stats), so v0.1 probes on
/// every startup; the on-disk cache keyed by versions arrives with `toml_edit`
/// (roadmap 0.1.5) if probing ever shows up in a profile.
#[derive(Debug, Default, Clone)]
pub struct Capabilities {
    /// Contents of `$OMARCHY_PATH/version`, e.g. `3.8.2`. Empty = not found.
    pub omarchy_version: String,
    /// Parsed from `hyprctl version`, e.g. `0.55.2`. Empty = hyprctl absent
    /// or not in a Hyprland session (e.g. SSH).
    pub hyprland_version: String,
    /// `hyprctl configerrors` supported — enables post-apply verification. [gate]
    pub has_configerrors: bool,
    /// `default/themed/` template engine present (Omarchy ≥ 3.x theme model).
    pub has_templates: bool,
    /// `bin/omarchy-hook` present — lifecycle hooks usable.
    pub has_hooks: bool,
    /// Stock `TUI.float` window rule present — our floating launch needs no
    /// rule installation (spec 02 §3.3).
    pub tui_float_rule: bool,
    /// `mpvpaper-rs` or `mpvpaper` in PATH — video wallpapers playable.
    pub video_wallpapers: bool,
    /// `$OMARCHY_PATH` git checkout has local modifications — will conflict
    /// on `omarchy-update` (doctor warns; the reference machine's video-
    /// wallpaper patch is the canonical case).
    pub omarchy_dirty: bool,
}

impl Capabilities {
    pub fn probe(paths: &OmarchyPaths, runner: &dyn CommandRunner) -> Self {
        let omarchy_version = std::fs::read_to_string(paths.system.join("version"))
            .map(|s| s.trim().to_string())
            .unwrap_or_default();

        let hyprland_version = runner
            .run(&Cmd::new("hyprctl").arg("version"))
            .ok()
            .filter(|o| o.ok())
            .and_then(|o| parse_hyprland_version(&o.stdout))
            .unwrap_or_default();

        let has_configerrors = runner
            .run(&Cmd::new("hyprctl").arg("configerrors"))
            .map(|o| o.ok())
            .unwrap_or(false);

        let omarchy_dirty = runner
            .run(
                &Cmd::new("git")
                    .args(["-C".to_string(), paths.system.display().to_string()])
                    .args(["status", "--porcelain"]),
            )
            .map(|o| o.ok() && !o.stdout.trim().is_empty())
            .unwrap_or(false);

        Self {
            omarchy_version,
            hyprland_version,
            has_configerrors,
            has_templates: paths.system.join("default/themed").is_dir(),
            has_hooks: paths.system.join("bin/omarchy-hook").is_file(),
            tui_float_rule: std::fs::read_to_string(
                paths.system.join("default/hypr/apps/system.conf"),
            )
            .map(|s| s.contains("TUI.float"))
            .unwrap_or(false),
            video_wallpapers: find_in_path("mpvpaper-rs").is_some()
                || find_in_path("mpvpaper").is_some(),
            omarchy_dirty,
        }
    }
}

/// `hyprctl version` first line: `Hyprland 0.55.2 built from branch …`.
/// Tagged builds may render as `Hyprland v0.55.2 …`; tolerate the `v`.
fn parse_hyprland_version(stdout: &str) -> Option<String> {
    let first = stdout.lines().next()?;
    let word = first.split_whitespace().nth(1)?;
    let version = word.trim_start_matches('v');
    if version.chars().next()?.is_ascii_digit() {
        Some(version.to_string())
    } else {
        None
    }
}

/// Components with an `omarchy-restart-*` primitive (spec 02 §2).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Component {
    Waybar,
    Mako,
    Swayosd,
    Hypridle,
    Walker,
    Terminal,
    Btop,
}

impl Component {
    /// The matching `omarchy-restart-*` bin — always wrapped, never reimplemented.
    pub fn restart_bin(self) -> &'static str {
        match self {
            Component::Waybar => "omarchy-restart-waybar",
            Component::Mako => "omarchy-restart-mako",
            Component::Swayosd => "omarchy-restart-swayosd",
            Component::Hypridle => "omarchy-restart-hypridle",
            Component::Walker => "omarchy-restart-walker",
            Component::Terminal => "omarchy-restart-terminal",
            Component::Btop => "omarchy-restart-btop",
        }
    }

    /// Process name for the post-restart alive check, where one applies.
    pub fn process_name(self) -> Option<&'static str> {
        match self {
            Component::Waybar => Some("waybar"),
            Component::Mako => Some("mako"),
            _ => None,
        }
    }
}

/// Typed constructors for the reload primitives (spec 02 §2). Keeping the
/// command names here — not in `engine` — preserves the "only this module
/// knows Omarchy commands" rule.
pub mod cmds {
    use super::Component;
    use crate::cmd::Cmd;

    pub fn restart(c: Component) -> Cmd {
        Cmd::new(c.restart_bin())
    }

    pub fn hypr_reload() -> Cmd {
        Cmd::new("hyprctl").arg("reload")
    }

    pub fn hypr_configerrors() -> Cmd {
        Cmd::new("hyprctl").arg("configerrors")
    }

    pub fn theme_refresh() -> Cmd {
        Cmd::new("omarchy-theme-refresh")
    }

    /// Idempotent full theme apply — Omarchy's own pipeline, never ours.
    pub fn theme_set(slug: &str) -> Cmd {
        Cmd::new("omarchy-theme-set").arg(slug)
    }

    /// Re-apply current theme without touching the wallpaper (env guard).
    pub fn theme_set_keep_bg(slug: &str) -> Cmd {
        Cmd::new("omarchy-theme-set")
            .arg(slug)
            .env("OMARCHY_THEME_SKIP_BACKGROUND", "1")
    }

    pub fn makoctl_reload() -> Cmd {
        Cmd::new("makoctl").arg("reload")
    }

    pub fn process_alive(name: &str) -> Cmd {
        Cmd::new("pgrep").arg("-x").arg(name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cmd::StubRunner;

    /// Unique throwaway dir; std-only stand-in for tempfile (the scaffold
    /// stays dependency-free, spec 01 §6).
    fn scratch_dir(tag: &str) -> PathBuf {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .subsec_nanos();
        let dir = std::env::temp_dir().join(format!(
            "omarchy-studio-test-{tag}-{}-{nanos}",
            std::process::id()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn fake_omarchy(tag: &str) -> OmarchyPaths {
        let root = scratch_dir(tag);
        let system = root.join("share/omarchy");
        std::fs::create_dir_all(system.join("default/themed")).unwrap();
        std::fs::create_dir_all(system.join("default/hypr/apps")).unwrap();
        std::fs::create_dir_all(system.join("bin")).unwrap();
        std::fs::write(system.join("version"), "3.8.2\n").unwrap();
        std::fs::write(system.join("bin/omarchy-hook"), "#!/bin/bash\n").unwrap();
        std::fs::write(
            system.join("default/hypr/apps/system.conf"),
            "windowrule = tag +floating-window, match:class (org.omarchy.btop|TUI.float)\n",
        )
        .unwrap();
        OmarchyPaths {
            system,
            config: root.join("config/omarchy"),
            state: root.join("state/omarchy"),
        }
    }

    fn git_status_display(paths: &OmarchyPaths) -> String {
        format!("git -C {} status --porcelain", paths.system.display())
    }

    #[test]
    fn probes_a_healthy_install() {
        let paths = fake_omarchy("healthy");
        let stub = StubRunner::default()
            .with_ok(
                "hyprctl version",
                "Hyprland 0.55.2 built from branch v0.55.2 at commit abc\n",
            )
            .with_ok("hyprctl configerrors", "no errors\n")
            .with_ok(git_status_display(&paths), "");

        let caps = Capabilities::probe(&paths, &stub);
        assert_eq!(caps.omarchy_version, "3.8.2");
        assert_eq!(caps.hyprland_version, "0.55.2");
        assert!(caps.has_configerrors);
        assert!(caps.has_templates);
        assert!(caps.has_hooks);
        assert!(caps.tui_float_rule);
        assert!(!caps.omarchy_dirty);
    }

    #[test]
    fn flags_dirty_checkout_and_survives_missing_hyprctl() {
        let paths = fake_omarchy("dirty");
        // hyprctl unscripted → StubRunner errors → probe degrades gracefully.
        let stub = StubRunner::default()
            .with_ok(git_status_display(&paths), " M bin/omarchy-theme-bg-next\n");

        let caps = Capabilities::probe(&paths, &stub);
        assert_eq!(caps.hyprland_version, "");
        assert!(!caps.has_configerrors);
        assert!(caps.omarchy_dirty);
    }

    #[test]
    fn absent_install_probes_empty_not_panicky() {
        let root = scratch_dir("absent");
        let paths = OmarchyPaths {
            system: root.join("nope"),
            config: root.join("config"),
            state: root.join("state"),
        };
        let stub = StubRunner::default();
        let caps = Capabilities::probe(&paths, &stub);
        assert!(!paths.installed());
        assert_eq!(caps.omarchy_version, "");
        assert!(!caps.has_templates && !caps.has_hooks && !caps.tui_float_rule);
    }

    #[test]
    fn parses_hyprland_version_variants() {
        assert_eq!(
            parse_hyprland_version("Hyprland 0.55.2 built from branch\nDate: x").as_deref(),
            Some("0.55.2")
        );
        assert_eq!(
            parse_hyprland_version("Hyprland v0.55.2 built…").as_deref(),
            Some("0.55.2")
        );
        assert_eq!(parse_hyprland_version("garbage"), None);
        assert_eq!(parse_hyprland_version(""), None);
    }

    #[test]
    fn reads_current_theme_name() {
        let paths = fake_omarchy("theme");
        std::fs::create_dir_all(paths.config.join("current")).unwrap();
        std::fs::write(paths.config.join("current/theme.name"), "osaka-jade\n").unwrap();
        assert_eq!(paths.current_theme_name().unwrap(), "osaka-jade");
    }
}
