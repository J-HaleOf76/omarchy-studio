//! Quick tweaks catalog (roadmap 0.8.5).
//!
//! a-la-carchy ships ~30 one-shot toggles that edit config in place with no way
//! back. Studio's tweaks are the same idea done reversibly: each one is an
//! idempotent on/off that writes a **uniquely-named managed block** into a
//! user-side file (or touches the filesystem), reports its own state, and
//! reverts cleanly — nothing is written into Omarchy's vendored tree, and each
//! tweak's block is independent of every other module's.
//!
//! The catalog is a registry of [`Tweak`] trait objects; new tweaks slot in
//! without touching the frontend. Every tweak routes through machinery that
//! already exists ([`ManagedBlock`], the filesystem) rather than a new writer.

use std::path::PathBuf;

use crate::configfs::{CommentStyle, ManagedBlock};
use crate::error::Result;
use crate::omarchy::OmarchyPaths;

/// A tweak's current state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum State {
    On,
    Off,
}

impl State {
    pub fn is_on(self) -> bool {
        self == State::On
    }
}

/// What a tweak needs to read/write. `home` is injected (not read from the
/// environment inside a tweak) so filesystem tweaks are testable.
pub struct Ctx {
    pub paths: OmarchyPaths,
    pub home: PathBuf,
}

impl Ctx {
    pub fn discover() -> Result<Self> {
        Ok(Self {
            paths: OmarchyPaths::discover()?,
            home: PathBuf::from(std::env::var_os("HOME").unwrap_or_default()),
        })
    }
}

/// A single reversible tweak.
pub trait Tweak: Sync {
    /// Stable id for the CLI (`tweak <id> on`).
    fn id(&self) -> &'static str;
    fn label(&self) -> &'static str;
    /// One-line plain-language description.
    fn detail(&self) -> &'static str;
    /// The Hyprland file the change lands in, if any (for a reload hint).
    fn touches_hypr(&self) -> bool {
        true
    }
    fn state(&self, ctx: &Ctx) -> State;
    /// Apply (`on`) or revert (`!on`). Idempotent. Returns the files written.
    fn set(&self, ctx: &Ctx, on: bool) -> Result<Vec<PathBuf>>;
}

/// The registry, in display order.
pub fn catalog() -> Vec<Box<dyn Tweak>> {
    vec![
        Box::new(CapsEscape),
        Box::new(InactiveTransparency),
        Box::new(MediaDirs),
    ]
}

pub fn find(id: &str) -> Option<Box<dyn Tweak>> {
    catalog().into_iter().find(|t| t.id() == id)
}

// ---------------------------------------------------------- managed-block help

/// Upsert (`on`) or remove (`!on`) a uniquely-named block carrying `body` in a
/// user-side hypr file. Returns the file path when it changed.
fn set_hypr_block(path: PathBuf, section: &str, body: &str, on: bool) -> Result<Vec<PathBuf>> {
    let block = ManagedBlock::new(section, CommentStyle::Hash);
    let existing = std::fs::read_to_string(&path).unwrap_or_default();
    let updated = if on {
        block.upsert(&existing, body)
    } else {
        block.remove(&existing)
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

fn hypr_block_present(path: &std::path::Path, section: &str) -> bool {
    std::fs::read_to_string(path)
        .map(|c| ManagedBlock::new(section, CommentStyle::Hash).contains(&c))
        .unwrap_or(false)
}

// ------------------------------------------------------------------ tweaks

/// Remap Caps Lock to Escape (Omarchy defaults it to Compose). Lands in a
/// `tweak-caps-escape` block in the user's `input.conf`, sourced after the
/// default so it wins.
struct CapsEscape;

impl CapsEscape {
    const SECTION: &'static str = "tweak-caps-escape";
    fn path(ctx: &Ctx) -> PathBuf {
        ctx.paths.hypr_config().join("input.conf")
    }
}

impl Tweak for CapsEscape {
    fn id(&self) -> &'static str {
        "caps-escape"
    }
    fn label(&self) -> &'static str {
        "Caps Lock → Escape"
    }
    fn detail(&self) -> &'static str {
        "Remap Caps Lock to Escape (overrides Omarchy's compose default)"
    }
    fn state(&self, ctx: &Ctx) -> State {
        if hypr_block_present(&Self::path(ctx), Self::SECTION) {
            State::On
        } else {
            State::Off
        }
    }
    fn set(&self, ctx: &Ctx, on: bool) -> Result<Vec<PathBuf>> {
        let body = "input {\n  kb_options = caps:escape\n}";
        set_hypr_block(Self::path(ctx), Self::SECTION, body, on)
    }
}

/// Subtle transparency on inactive windows (active stays fully opaque). Lands in
/// a `tweak-transparency` block in `looknfeel.conf`, matching Omarchy's own
/// `windowrule = opacity <active> <inactive>` syntax.
struct InactiveTransparency;

impl InactiveTransparency {
    const SECTION: &'static str = "tweak-transparency";
    fn path(ctx: &Ctx) -> PathBuf {
        ctx.paths.hypr_config().join("looknfeel.conf")
    }
}

impl Tweak for InactiveTransparency {
    fn id(&self) -> &'static str {
        "inactive-transparency"
    }
    fn label(&self) -> &'static str {
        "Inactive window transparency"
    }
    fn detail(&self) -> &'static str {
        "Dim unfocused windows to 95% opacity (active stays solid)"
    }
    fn state(&self, ctx: &Ctx) -> State {
        if hypr_block_present(&Self::path(ctx), Self::SECTION) {
            State::On
        } else {
            State::Off
        }
    }
    fn set(&self, ctx: &Ctx, on: bool) -> Result<Vec<PathBuf>> {
        let body = "windowrule = opacity 1.0 0.95, match:class .*";
        set_hypr_block(Self::path(ctx), Self::SECTION, body, on)
    }
}

/// Create the standard screenshot/screencast directories Omarchy expects, so
/// captures have a home. Pure filesystem; revert removes them only if empty.
struct MediaDirs;

impl MediaDirs {
    fn dirs(ctx: &Ctx) -> [PathBuf; 2] {
        [
            ctx.home.join("Pictures/Screenshots"),
            ctx.home.join("Videos/Screencasts"),
        ]
    }
}

impl Tweak for MediaDirs {
    fn id(&self) -> &'static str {
        "media-dirs"
    }
    fn label(&self) -> &'static str {
        "Screenshot & screencast folders"
    }
    fn detail(&self) -> &'static str {
        "Create ~/Pictures/Screenshots and ~/Videos/Screencasts"
    }
    fn touches_hypr(&self) -> bool {
        false
    }
    fn state(&self, ctx: &Ctx) -> State {
        if Self::dirs(ctx).iter().all(|d| d.is_dir()) {
            State::On
        } else {
            State::Off
        }
    }
    fn set(&self, ctx: &Ctx, on: bool) -> Result<Vec<PathBuf>> {
        let mut changed = Vec::new();
        for d in Self::dirs(ctx) {
            if on {
                if !d.is_dir() {
                    std::fs::create_dir_all(&d)?;
                    changed.push(d);
                }
            } else if d.is_dir() {
                // Only remove an empty dir — never delete captured media.
                if std::fs::read_dir(&d)
                    .map(|mut e| e.next().is_none())
                    .unwrap_or(false)
                {
                    std::fs::remove_dir(&d)?;
                    changed.push(d);
                }
            }
        }
        Ok(changed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ctx(tag: &str) -> Ctx {
        let root = std::env::temp_dir().join(format!(
            "omarchy-studio-tweaks-{tag}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&root).unwrap();
        // hypr_config() = config.parent()/hypr, so config must have a parent.
        let paths = OmarchyPaths {
            system: root.join("sys"),
            config: root.join("cfg/omarchy"),
            state: root.join("cfg/state"),
        };
        Ctx { paths, home: root }
    }

    #[test]
    fn catalog_ids_unique() {
        let cat = catalog();
        let mut seen = std::collections::HashSet::new();
        for t in &cat {
            assert!(seen.insert(t.id()), "dup id {}", t.id());
        }
        assert!(find("caps-escape").is_some());
        assert!(find("nope").is_none());
    }

    #[test]
    fn caps_escape_round_trips() {
        let c = ctx("caps");
        let t = CapsEscape;
        assert_eq!(t.state(&c), State::Off);
        let files = t.set(&c, true).unwrap();
        assert_eq!(files.len(), 1);
        assert_eq!(t.state(&c), State::On);
        let text = std::fs::read_to_string(&files[0]).unwrap();
        assert!(text.contains("kb_options = caps:escape"));
        assert!(text.contains("omarchy-studio:tweak-caps-escape"));
        // idempotent apply
        assert!(t.set(&c, true).unwrap().is_empty());
        // revert
        t.set(&c, false).unwrap();
        assert_eq!(t.state(&c), State::Off);
    }

    #[test]
    fn tweak_block_leaves_other_content_intact() {
        let c = ctx("coexist");
        let path = c.paths.hypr_config().join("input.conf");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        // Pretend the looknfeel module already owns a block here.
        std::fs::write(
            &path,
            "# >>> omarchy-studio:input — generated; edit outside this block only >>>\ninput {\n  repeat_rate = 40\n}\n# <<< omarchy-studio:input <<<\n",
        )
        .unwrap();
        CapsEscape.set(&c, true).unwrap();
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(text.contains("repeat_rate = 40")); // untouched
        assert!(text.contains("caps:escape")); // added
        CapsEscape.set(&c, false).unwrap();
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(text.contains("repeat_rate = 40")); // still there
        assert!(!text.contains("caps:escape")); // removed
    }

    #[test]
    fn transparency_uses_omarchy_opacity_syntax() {
        let c = ctx("trans");
        let t = InactiveTransparency;
        let files = t.set(&c, true).unwrap();
        let text = std::fs::read_to_string(&files[0]).unwrap();
        assert!(text.contains("windowrule = opacity 1.0 0.95, match:class .*"));
        assert_eq!(t.state(&c), State::On);
    }

    #[test]
    fn media_dirs_create_and_revert_when_empty() {
        let c = ctx("media");
        let t = MediaDirs;
        assert_eq!(t.state(&c), State::Off);
        t.set(&c, true).unwrap();
        assert_eq!(t.state(&c), State::On);
        assert!(c.home.join("Pictures/Screenshots").is_dir());
        // revert removes the empty dirs
        t.set(&c, false).unwrap();
        assert_eq!(t.state(&c), State::Off);
    }

    #[test]
    fn media_dirs_revert_keeps_nonempty() {
        let c = ctx("media2");
        let t = MediaDirs;
        t.set(&c, true).unwrap();
        let shot = c.home.join("Pictures/Screenshots/keep.png");
        std::fs::write(&shot, "x").unwrap();
        // revert must not delete a dir with captures in it
        t.set(&c, false).unwrap();
        assert!(shot.is_file());
    }
}
