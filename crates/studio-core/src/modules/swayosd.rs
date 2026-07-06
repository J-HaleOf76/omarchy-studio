//! On-screen display — SwayOSD (spec 05 §6, roadmap 0.5.3).
//!
//! SwayOSD shows the volume/brightness popups. Two files drive it, both real
//! files the user owns (no theme symlink):
//!
//!   * `~/.config/swayosd/config.toml` — behavior (`[server]`: show_percentage,
//!     max_volume, top_margin), edited with `toml_edit` so formatting survives.
//!   * `~/.config/swayosd/style.css` — appearance; it `@import`s the theme's
//!     colours, so Studio only owns a managed geometry block appended after the
//!     import (radius + size), exactly like the Waybar style block.
//!
//! Apply restarts the server through `omarchy-restart-swayosd` (the systemd
//! user-unit dance lives in that bin — never reimplemented). A self-test fires
//! a no-op `raise 0` so the OSD flashes and the user can see their changes.

use std::path::{Path, PathBuf};

use crate::configfs::{atomic_write, CommentStyle, ManagedBlock};
use crate::error::{Result, StudioError};
use crate::omarchy::{cmds, Component, OmarchyPaths};

fn config_dir(paths: &OmarchyPaths) -> PathBuf {
    paths
        .config
        .parent()
        .map(|c| c.join("swayosd"))
        .unwrap_or_else(|| PathBuf::from("swayosd"))
}

fn config_toml(paths: &OmarchyPaths) -> PathBuf {
    config_dir(paths).join("config.toml")
}

pub(crate) fn style_css(paths: &OmarchyPaths) -> PathBuf {
    config_dir(paths).join("style.css")
}

pub(crate) fn style_block() -> ManagedBlock {
    ManagedBlock::new("swayosd-style", CommentStyle::CBlock)
}

/// The editable SwayOSD settings — behavior (config.toml) + geometry (style.css).
#[derive(Debug, Clone, PartialEq)]
pub struct Swayosd {
    config_toml: PathBuf,
    style_css: PathBuf,
    /// `[server] show_percentage`.
    pub show_percentage: bool,
    /// `[server] max_volume` (percent).
    pub max_volume: i64,
    /// `[server] top_margin` (0.0–1.0 of screen height); None = unset (default).
    pub top_margin: Option<f64>,
    /// Managed style: corner radius in px (None = leave the stock value).
    pub radius: Option<i64>,
    /// Managed style: label font size in pt (None = leave the stock value).
    pub font_pt: Option<i64>,
}

/// Defaults matching Omarchy's stock `config.toml`.
const DEFAULT_MAX_VOLUME: i64 = 100;

impl Swayosd {
    pub fn load(paths: &OmarchyPaths) -> Self {
        let config_toml = config_toml(paths);
        let style_css = style_css(paths);

        let (show_percentage, max_volume, top_margin) = read_config(&config_toml);
        let (radius, font_pt) = read_style(&style_css);

        Self {
            config_toml,
            style_css,
            show_percentage,
            max_volume,
            top_margin,
            radius,
            font_pt,
        }
    }

    pub fn config_path(&self) -> &Path {
        &self.config_toml
    }

    pub fn style_path(&self) -> &Path {
        &self.style_css
    }

    /// Render the config.toml with the current values, preserving the file's
    /// existing formatting/comments where present.
    fn render_config(&self) -> Result<String> {
        let existing = std::fs::read_to_string(&self.config_toml).unwrap_or_default();
        let mut doc =
            existing
                .parse::<toml_edit::DocumentMut>()
                .map_err(|e| StudioError::ParseFailed {
                    file: self.config_toml.clone(),
                    line: None,
                    hint: e.to_string(),
                })?;
        let server = doc["server"].or_insert(toml_edit::table());
        server["show_percentage"] = toml_edit::value(self.show_percentage);
        server["max_volume"] = toml_edit::value(self.max_volume);
        match self.top_margin {
            Some(m) => server["top_margin"] = toml_edit::value(m),
            None => {
                if let Some(t) = server.as_table_like_mut() {
                    t.remove("top_margin");
                }
            }
        }
        Ok(doc.to_string())
    }

    /// Render the managed geometry body for style.css, or None if nothing is set.
    fn style_body(&self) -> Option<String> {
        let mut decls = String::new();
        if let Some(r) = self.radius {
            decls.push_str(&format!("  border-radius: {r}px;\n"));
        }
        if let Some(pt) = self.font_pt {
            decls.push_str(&format!("  font-size: {pt}pt;\n"));
        }
        if decls.is_empty() {
            None
        } else {
            Some(format!("window, progressbar, progress {{\n{decls}}}"))
        }
    }

    fn render_style(&self) -> Option<String> {
        let existing = std::fs::read_to_string(&self.style_css).unwrap_or_default();
        match self.style_body() {
            Some(body) => Some(style_block().upsert(&existing, &body)),
            None if style_block().contains(&existing) => Some(style_block().remove(&existing)),
            None => None,
        }
    }

    /// Write config.toml (always) and style.css (only if geometry changed).
    pub fn save(&self) -> Result<()> {
        atomic_write(&self.config_toml, &self.render_config()?)?;
        if let Some(css) = self.render_style() {
            atomic_write(&self.style_css, &css)?;
        }
        Ok(())
    }

    /// Save + restart the SwayOSD server. Caller snapshots first.
    pub fn apply(&self, runner: &dyn crate::cmd::CommandRunner) -> Result<()> {
        self.save()?;
        let out = runner.run(&cmds::restart(Component::Swayosd))?;
        if !out.ok() {
            return Err(StudioError::External {
                cmd: "omarchy-restart-swayosd".into(),
                detail: out.stderr.trim().to_string(),
            });
        }
        Ok(())
    }
}

/// Flash the OSD without changing anything, so the user can preview their look.
pub fn self_test(runner: &dyn crate::cmd::CommandRunner) -> Result<()> {
    let out = runner.run(&cmds::swayosd_selftest())?;
    if !out.ok() {
        return Err(StudioError::External {
            cmd: "swayosd-client".into(),
            detail: out.stderr.trim().to_string(),
        });
    }
    Ok(())
}

/// Read `[server]` values, falling back to Omarchy's stock defaults.
fn read_config(path: &Path) -> (bool, i64, Option<f64>) {
    let text = std::fs::read_to_string(path).unwrap_or_default();
    let doc = text.parse::<toml_edit::DocumentMut>().ok();
    let server = doc.as_ref().and_then(|d| d.get("server"));
    let show = server
        .and_then(|s| s.get("show_percentage"))
        .and_then(|v| v.as_bool())
        .unwrap_or(true);
    let maxv = server
        .and_then(|s| s.get("max_volume"))
        .and_then(|v| v.as_integer())
        .unwrap_or(DEFAULT_MAX_VOLUME);
    let margin = server
        .and_then(|s| s.get("top_margin"))
        .and_then(|v| v.as_float());
    (show, maxv, margin)
}

/// Read the managed style block's radius/font, if present.
fn read_style(path: &Path) -> (Option<i64>, Option<i64>) {
    let text = std::fs::read_to_string(path).unwrap_or_default();
    let Some(body) = style_block().extract(&text) else {
        return (None, None);
    };
    (
        grab_unit(body, "border-radius"),
        grab_unit(body, "font-size"),
    )
}

/// Grab the integer part of a `prop: <n><unit>;` declaration.
fn grab_unit(body: &str, prop: &str) -> Option<i64> {
    let needle = format!("{prop}:");
    let start = body.find(&needle)? + needle.len();
    let digits: String = body[start..]
        .trim_start()
        .chars()
        .take_while(|c| c.is_ascii_digit())
        .collect();
    digits.parse().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fake_paths(tag: &str) -> OmarchyPaths {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .subsec_nanos();
        let root = std::env::temp_dir().join(format!(
            "omarchy-studio-osd-{tag}-{}-{nanos}",
            std::process::id()
        ));
        std::fs::create_dir_all(root.join(".config/swayosd")).unwrap();
        OmarchyPaths {
            system: root.join("sys/omarchy"),
            config: root.join(".config/omarchy"),
            state: root.join(".local/state/omarchy"),
        }
    }

    #[test]
    fn reads_defaults_and_writes_config() {
        let paths = fake_paths("config");
        std::fs::write(
            config_toml(&paths),
            "[server]\nshow_percentage = true\nmax_volume = 100\nstyle = \"~/.config/swayosd/style.css\"\n",
        )
        .unwrap();

        let mut osd = Swayosd::load(&paths);
        assert!(osd.show_percentage);
        assert_eq!(osd.max_volume, 100);
        assert_eq!(osd.top_margin, None);

        osd.max_volume = 150;
        osd.show_percentage = false;
        osd.top_margin = Some(0.3);
        osd.save().unwrap();

        let out = std::fs::read_to_string(config_toml(&paths)).unwrap();
        assert!(out.contains("max_volume = 150"));
        assert!(out.contains("show_percentage = false"));
        assert!(out.contains("top_margin = 0.3"));
        // untouched key preserved by toml_edit
        assert!(out.contains("style = \"~/.config/swayosd/style.css\""));

        let re = Swayosd::load(&paths);
        assert_eq!(re.max_volume, 150);
        assert_eq!(re.top_margin, Some(0.3));
    }

    #[test]
    fn style_geometry_block_after_import() {
        let paths = fake_paths("style");
        let css = "@import \"../omarchy/current/theme/swayosd.css\";\n\nwindow {\n  border-radius: 0;\n}\n";
        std::fs::write(style_css(&paths), css).unwrap();
        // config.toml so load doesn't choke
        std::fs::write(config_toml(&paths), "[server]\n").unwrap();

        let mut osd = Swayosd::load(&paths);
        assert_eq!(osd.radius, None);
        osd.radius = Some(12);
        osd.font_pt = Some(13);
        osd.save().unwrap();

        let out = std::fs::read_to_string(style_css(&paths)).unwrap();
        assert!(out.starts_with("@import"));
        let import = out.find("@import").unwrap();
        let block = out.find("omarchy-studio:swayosd-style").unwrap();
        assert!(import < block, "managed block must follow the import");
        assert!(out.contains("border-radius: 12px"));
        assert!(out.contains("font-size: 13pt"));

        let re = Swayosd::load(&paths);
        assert_eq!(re.radius, Some(12));
        assert_eq!(re.font_pt, Some(13));

        // clearing geometry removes the block, leaving the file otherwise intact
        let mut osd = Swayosd::load(&paths);
        osd.radius = None;
        osd.font_pt = None;
        osd.save().unwrap();
        let cleared = std::fs::read_to_string(style_css(&paths)).unwrap();
        assert!(!cleared.contains("omarchy-studio:swayosd-style"));
        assert!(cleared.contains("@import"));
    }

    #[test]
    fn apply_restarts_and_selftest_bumps() {
        use crate::cmd::StubRunner;
        let paths = fake_paths("apply");
        std::fs::write(config_toml(&paths), "[server]\nmax_volume = 100\n").unwrap();

        let osd = Swayosd::load(&paths);
        let runner = StubRunner::default().with_ok("omarchy-restart-swayosd", "");
        assert!(osd.apply(&runner).is_ok());
        assert!(runner
            .calls()
            .iter()
            .any(|c| c == "omarchy-restart-swayosd"));

        let r = StubRunner::default().with_ok("swayosd-client --output-volume raise 0", "");
        assert!(self_test(&r).is_ok());
    }
}
