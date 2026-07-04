//! Omarchy adapter (spec 02). The ONLY module allowed to know Omarchy paths
//! and command names — CI greps enforce this (spec 02 §6).

use std::path::PathBuf;

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
        let home = std::env::var_os("HOME")
            .map(PathBuf::from)
            .ok_or_else(|| crate::StudioError::External {
                cmd: "env".into(),
                detail: "HOME is not set".into(),
            })?;
        Ok(Self {
            system: std::env::var_os("OMARCHY_PATH")
                .map(PathBuf::from)
                .unwrap_or_else(|| home.join(".local/share/omarchy")),
            config: home.join(".config/omarchy"),
            state: home.join(".local/state/omarchy"),
        })
    }

    /// Slug of the active theme (`current/theme.name`) — spec 02 §1.
    pub fn current_theme_name(&self) -> crate::error::Result<String> {
        let p = self.config.join("current/theme.name");
        Ok(std::fs::read_to_string(p)?.trim().to_string())
    }
}

/// Capability probe result (spec 02 §5). Cached in probe-cache.toml keyed on
/// omarchy + hyprland versions; `graphics` is re-detected every TUI start.
#[derive(Debug, Default)]
pub struct Capabilities {
    pub omarchy_version: String,
    pub hyprland_version: String,
    pub has_configerrors: bool,
    pub has_templates: bool,
    pub has_hooks: bool,
    pub tui_float_rule: bool,
    pub video_wallpapers: bool,
    pub omarchy_dirty: bool,
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
