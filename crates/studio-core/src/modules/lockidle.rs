//! Lock & Idle — hypridle + hyprlock (spec 05 §7, roadmap 0.5.4).
//!
//! **hypridle** (`~/.config/hypr/hypridle.conf`) is a set of `listener {}` blocks,
//! each an idle timeout with an action (start the screensaver, lock, turn the
//! screen off, suspend). Studio models them as a *timeline* — sorted by timeout,
//! each classified into a friendly stage — and lets the user retime a stage.
//! Blocks it doesn't recognize are preserved untouched. Apply restarts hypridle.
//!
//! **hyprlock** (`~/.config/hypr/hyprlock.conf`) is edited through the hyprlang
//! line-CST so colours (sourced from the theme) stay put. Studio touches only
//! the avatar image, its size, and the background blur/dim. The first edit drops
//! a `lock-screen-backup.<epoch>/` copy — Omarchy's own convention — so the
//! stock lock screen is always recoverable.

use std::path::{Path, PathBuf};

use crate::configfs::atomic_write;
use crate::configfs::hyprlang::HyprDoc;
use crate::error::{Result, StudioError};
use crate::omarchy::{cmds, Component, OmarchyPaths};

fn hypridle_conf(paths: &OmarchyPaths) -> PathBuf {
    paths.hypr_config().join("hypridle.conf")
}

fn hyprlock_conf(paths: &OmarchyPaths) -> PathBuf {
    paths.hypr_config().join("hyprlock.conf")
}

// ── hypridle timeline ────────────────────────────────────────────────────────

/// The friendly stage an idle listener represents, inferred from its action.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stage {
    Screensaver,
    Lock,
    ScreenOff,
    Suspend,
    Other,
}

impl Stage {
    pub fn label(self) -> &'static str {
        match self {
            Stage::Screensaver => "Start screensaver",
            Stage::Lock => "Lock the screen",
            Stage::ScreenOff => "Turn the screen off",
            Stage::Suspend => "Suspend",
            Stage::Other => "Custom action",
        }
    }

    fn classify(on_timeout: &str) -> Stage {
        let a = on_timeout.to_ascii_lowercase();
        if a.contains("screensaver") {
            Stage::Screensaver
        } else if a.contains("dpms") || a.contains("screen off") {
            Stage::ScreenOff
        } else if a.contains("suspend") || a.contains("systemctl suspend") {
            Stage::Suspend
        } else if a.contains("lock") {
            Stage::Lock
        } else {
            Stage::Other
        }
    }
}

/// One idle listener: a timeout (seconds) plus what it triggers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Listener {
    pub timeout: i64,
    pub on_timeout: String,
    pub stage: Stage,
}

/// The hypridle configuration modelled as an ordered idle timeline.
pub struct Hypridle {
    path: PathBuf,
    src: String,
    /// Listeners in source order, with the byte span of each `timeout` value.
    listeners: Vec<(Listener, (usize, usize))>,
}

impl Hypridle {
    pub fn load(paths: &OmarchyPaths) -> Result<Self> {
        let path = hypridle_conf(paths);
        let src = std::fs::read_to_string(&path).map_err(|e| StudioError::External {
            cmd: format!("read {}", path.display()),
            detail: e.to_string(),
        })?;
        let listeners = parse_listeners(&src);
        Ok(Self {
            path,
            src,
            listeners,
        })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// The idle timeline, sorted by timeout ascending (as the user experiences it).
    pub fn timeline(&self) -> Vec<Listener> {
        let mut v: Vec<Listener> = self.listeners.iter().map(|(l, _)| l.clone()).collect();
        v.sort_by_key(|l| l.timeout);
        v
    }

    /// Retime the listener at source `index` (as in [`timeline`] order) to
    /// `seconds`, splicing only its `timeout` value.
    pub fn set_timeout(&mut self, listener: &Listener, seconds: i64) -> Result<()> {
        // match by (timeout, on_timeout) identity — timeline order may differ
        let Some((_, span)) = self
            .listeners
            .iter()
            .find(|(l, _)| l.timeout == listener.timeout && l.on_timeout == listener.on_timeout)
        else {
            return Err(StudioError::External {
                cmd: "hypridle".into(),
                detail: "listener no longer present".into(),
            });
        };
        let span = *span;
        let mut s = String::with_capacity(self.src.len());
        s.push_str(&self.src[..span.0]);
        s.push_str(&seconds.to_string());
        s.push_str(&self.src[span.1..]);
        self.src = s;
        self.listeners = parse_listeners(&self.src);
        Ok(())
    }

    pub fn rendered(&self) -> &str {
        &self.src
    }

    /// Write + restart hypridle. Caller snapshots first.
    pub fn apply(&self, runner: &dyn crate::cmd::CommandRunner) -> Result<()> {
        atomic_write(&self.path, &self.src)?;
        let out = runner.run(&cmds::restart(Component::Hypridle))?;
        if !out.ok() {
            return Err(StudioError::External {
                cmd: "omarchy-restart-hypridle".into(),
                detail: out.stderr.trim().to_string(),
            });
        }
        Ok(())
    }
}

/// Parse `listener {}` blocks, capturing each timeout value's byte span so it
/// can be edited in place without disturbing the rest of the file.
fn parse_listeners(src: &str) -> Vec<(Listener, (usize, usize))> {
    let b = src.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while let Some(rel) = src[i..].find("listener") {
        let kw = i + rel;
        // must be followed (after ws) by '{'
        let mut j = kw + "listener".len();
        while j < b.len() && (b[j] == b' ' || b[j] == b'\t' || b[j] == b'\n' || b[j] == b'\r') {
            j += 1;
        }
        if j >= b.len() || b[j] != b'{' {
            i = kw + "listener".len();
            continue;
        }
        // find matching close brace
        let open = j;
        let mut depth = 1;
        let mut k = open + 1;
        while k < b.len() && depth > 0 {
            match b[k] {
                b'{' => depth += 1,
                b'}' => depth -= 1,
                _ => {}
            }
            k += 1;
        }
        let block = &src[open + 1..k - 1];
        let block_off = open + 1;
        if let Some((timeout, tspan)) = field_int(block, "timeout", block_off) {
            let on_timeout = field_str(block, "on-timeout").unwrap_or_default();
            let stage = Stage::classify(&on_timeout);
            out.push((
                Listener {
                    timeout,
                    on_timeout,
                    stage,
                },
                tspan,
            ));
        }
        i = k;
    }
    out
}

/// Find `key = <int>` in a block, returning the value and the byte span of its
/// digits (offset by `base` into the whole source).
fn field_int(block: &str, key: &str, base: usize) -> Option<(i64, (usize, usize))> {
    let mut off = 0usize;
    for line in block.split_inclusive('\n') {
        let raw = line.trim_end_matches('\n');
        let trimmed = raw.trim_start();
        let is_key = trimmed
            .strip_prefix(key)
            .map(|r| r.trim_start().starts_with('='))
            .unwrap_or(false);
        if is_key {
            if let Some(eq) = raw.find('=') {
                let after = &raw[eq + 1..];
                let lead = after.len() - after.trim_start().len();
                let val_start = eq + 1 + lead;
                let digits: String = raw[val_start..]
                    .chars()
                    .take_while(|c| c.is_ascii_digit() || *c == '-')
                    .collect();
                if let Ok(v) = digits.parse::<i64>() {
                    let start = base + off + val_start;
                    return Some((v, (start, start + digits.len())));
                }
            }
        }
        off += line.len();
    }
    None
}

/// Find `key = <rest of line>` in a block (trimmed).
fn field_str(block: &str, key: &str) -> Option<String> {
    for line in block.lines() {
        let trimmed = line.trim_start();
        if let Some(rest) = trimmed.strip_prefix(key) {
            let rest = rest.trim_start();
            if let Some(after_eq) = rest.strip_prefix('=') {
                // drop a trailing comment
                let val = after_eq.split('#').next().unwrap_or("").trim();
                return Some(val.to_string());
            }
        }
    }
    None
}

// ── hyprlock form ────────────────────────────────────────────────────────────

/// The subset of hyprlock Studio edits: the avatar and the background feel.
/// Colours stay theme-owned (the `source =` line is untouched).
pub struct Hyprlock {
    path: PathBuf,
    doc: HyprDoc,
}

impl Hyprlock {
    pub fn load(paths: &OmarchyPaths) -> Result<Self> {
        let path = hyprlock_conf(paths);
        let src = std::fs::read_to_string(&path).map_err(|e| StudioError::External {
            cmd: format!("read {}", path.display()),
            detail: e.to_string(),
        })?;
        Ok(Self {
            path,
            doc: HyprDoc::parse(&src),
        })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn avatar_path(&self) -> Option<String> {
        self.doc.get("image.path")
    }
    pub fn avatar_size(&self) -> Option<String> {
        self.doc.get("image.size")
    }
    pub fn blur_passes(&self) -> Option<String> {
        self.doc.get("background.blur_passes")
    }
    pub fn dim(&self) -> Option<String> {
        self.doc.get("background.brightness")
    }

    pub fn set_avatar(&mut self, path: &str) -> bool {
        self.doc.set("image.path", path)
    }
    pub fn set_avatar_size(&mut self, px: i64) -> bool {
        self.doc.set("image.size", &px.to_string())
    }
    pub fn set_blur_passes(&mut self, n: i64) -> bool {
        self.doc.set("background.blur_passes", &n.to_string())
    }
    /// Background brightness (the "dim" knob), 0.0–1.0.
    pub fn set_dim(&mut self, level: f64) -> bool {
        self.doc
            .set("background.brightness", &format!("{level:.2}"))
    }

    /// Avatar images Omarchy ships/keeps for the picker.
    pub fn avatar_choices(paths: &OmarchyPaths) -> Vec<PathBuf> {
        let dir = paths.config.join("profile_images");
        let mut out = Vec::new();
        if let Ok(rd) = std::fs::read_dir(&dir) {
            for e in rd.flatten() {
                let p = e.path();
                if p.extension()
                    .map(|x| matches!(x.to_str(), Some("png" | "jpg" | "jpeg" | "webp")))
                    .unwrap_or(false)
                {
                    out.push(p);
                }
            }
        }
        out.sort();
        out
    }

    /// Save the edits, first dropping a one-time backup honoring Omarchy's
    /// `lock-screen-backup.<epoch>/` convention if none exists yet. Returns the
    /// backup directory when one was created. hyprlock reads its config at
    /// launch, so no restart is needed.
    pub fn save(&self) -> Result<Option<PathBuf>> {
        let backup = self.ensure_backup()?;
        atomic_write(&self.path, &self.doc.to_string())?;
        Ok(backup)
    }

    fn ensure_backup(&self) -> Result<Option<PathBuf>> {
        let parent = self.path.parent().unwrap_or_else(|| Path::new("."));
        // already backed up once?
        if let Ok(rd) = std::fs::read_dir(parent) {
            for e in rd.flatten() {
                if e.file_name()
                    .to_string_lossy()
                    .starts_with("lock-screen-backup.")
                {
                    return Ok(None);
                }
            }
        }
        let epoch = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        let dir = parent.join(format!("lock-screen-backup.{epoch}"));
        std::fs::create_dir_all(&dir).map_err(|e| StudioError::External {
            cmd: format!("mkdir {}", dir.display()),
            detail: e.to_string(),
        })?;
        let dest = dir.join("hyprlock.conf");
        std::fs::copy(&self.path, &dest).map_err(|e| StudioError::External {
            cmd: format!("backup {}", self.path.display()),
            detail: e.to_string(),
        })?;
        Ok(Some(dir))
    }
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
            "omarchy-studio-lock-{tag}-{}-{nanos}",
            std::process::id()
        ));
        std::fs::create_dir_all(root.join(".config/hypr")).unwrap();
        OmarchyPaths {
            system: root.join("sys/omarchy"),
            config: root.join(".config/omarchy"),
            state: root.join(".local/state/omarchy"),
        }
    }

    const HYPRIDLE: &str = "\
general {
    lock_cmd = omarchy-system-lock
}

# Start screensaver after 2.5 minutes
listener {
    timeout = 150
    on-timeout = pidof hyprlock || omarchy-launch-screensaver
}

listener {
    timeout = 152
    on-timeout = omarchy-system-lock
    on-resume = omarchy-system-wake
}

listener {
    timeout = 600
    on-timeout = hyprctl dispatch dpms off
    on-resume = hyprctl dispatch dpms on
}
";

    #[test]
    fn parses_and_classifies_the_idle_timeline() {
        let paths = fake_paths("timeline");
        std::fs::write(hypridle_conf(&paths), HYPRIDLE).unwrap();
        let idle = Hypridle::load(&paths).unwrap();
        let tl = idle.timeline();
        assert_eq!(tl.len(), 3);
        assert_eq!(tl[0].timeout, 150);
        assert_eq!(tl[0].stage, Stage::Screensaver);
        assert_eq!(tl[1].stage, Stage::Lock);
        assert_eq!(tl[2].stage, Stage::ScreenOff);
    }

    #[test]
    fn retimes_one_listener_in_place() {
        let paths = fake_paths("retime");
        std::fs::write(hypridle_conf(&paths), HYPRIDLE).unwrap();
        let mut idle = Hypridle::load(&paths).unwrap();
        let lock = idle
            .timeline()
            .into_iter()
            .find(|l| l.stage == Stage::Lock)
            .unwrap();
        idle.set_timeout(&lock, 300).unwrap();
        let out = idle.rendered();
        // only the lock listener's timeout changed; the screensaver stays 150
        assert!(out.contains("timeout = 300"));
        assert!(out.contains("timeout = 150"));
        assert!(!out.contains("timeout = 152"));
        // everything else preserved
        assert!(out.contains("lock_cmd = omarchy-system-lock"));
        assert!(out.contains("dpms off"));
        // reloads clean
        let idle2 = Hypridle::load(&paths).unwrap();
        let _ = idle2;
        assert_eq!(
            Hypridle {
                path: hypridle_conf(&paths),
                src: out.to_string(),
                listeners: parse_listeners(out),
            }
            .timeline()
            .iter()
            .find(|l| l.stage == Stage::Lock)
            .unwrap()
            .timeout,
            300
        );
    }

    const HYPRLOCK: &str = "\
source = ~/.config/omarchy/current/theme/hyprlock.conf

background {
    color = $color
    path = ~/.config/omarchy/current/background
    blur_passes = 3
    brightness = 0.55
}

image {
    path = ~/.face
    size = 200
}

label {
    text = $TIME
    color = $font_color
}
";

    #[test]
    fn hyprlock_edits_avatar_blur_dim_and_keeps_theme_source() {
        let paths = fake_paths("hyprlock");
        std::fs::write(hyprlock_conf(&paths), HYPRLOCK).unwrap();
        let mut lock = Hyprlock::load(&paths).unwrap();
        assert_eq!(lock.avatar_path().as_deref(), Some("~/.face"));
        assert_eq!(lock.blur_passes().as_deref(), Some("3"));
        assert_eq!(lock.dim().as_deref(), Some("0.55"));

        assert!(lock.set_avatar("~/.config/omarchy/profile_images/me.png"));
        assert!(lock.set_blur_passes(1));
        assert!(lock.set_dim(0.8));
        let backup = lock.save().unwrap();
        assert!(backup.is_some(), "first save creates a backup");

        let out = std::fs::read_to_string(hyprlock_conf(&paths)).unwrap();
        assert!(out.contains("path = ~/.config/omarchy/profile_images/me.png"));
        assert!(out.contains("blur_passes = 1"));
        assert!(out.contains("brightness = 0.80"));
        // theme source + the $TIME label are untouched
        assert!(out.contains("source = ~/.config/omarchy/current/theme/hyprlock.conf"));
        assert!(out.contains("text = $TIME"));
        // the backup holds the original
        let bdir = backup.unwrap();
        let backed = std::fs::read_to_string(bdir.join("hyprlock.conf")).unwrap();
        assert!(backed.contains("path = ~/.face"));

        // a second save doesn't make another backup
        let lock2 = Hyprlock::load(&paths).unwrap();
        assert!(lock2.save().unwrap().is_none());
    }
}
