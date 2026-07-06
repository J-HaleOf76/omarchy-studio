//! Wallpaper browser (spec 04 §5, roadmap 0.6.1).
//!
//! Lists the same four sources `omarchy-theme-bg-next` merges, in Omarchy's
//! priority order, and applies through Omarchy's own scripts — Studio never
//! spawns swaybg/swww/mpvpaper itself:
//!
//! 1. `~/.config/omarchy/backgrounds/<theme>/`         (user adds, removable)
//! 2. `~/.config/omarchy/current/theme/backgrounds/`   (the theme's own)
//! 3. `~/.config/omarchy/themes/<theme>/backgrounds/`  (user-installed theme)
//! 4. `~/.config/omarchy/themes/<theme>/videos/`       (video wallpapers)
//!
//! Sources 2 and 3 are often the same directory through the `current/theme`
//! symlink, so entries deduplicate by canonical path (first source wins).
//! Kind detection mirrors `omarchy-theme-bg-set`'s extension lists; the
//! frontends gate video entries on the `video_wallpapers` capability.

use std::path::{Path, PathBuf};

use crate::cmd::CommandRunner;
use crate::error::Result;
use crate::omarchy::{cmds, OmarchyPaths};
use crate::StudioError;

/// Extension lists from `omarchy-theme-bg-set` — keep in sync.
const VIDEO_EXTENSIONS: [&str; 8] = ["mp4", "mkv", "webm", "avi", "mov", "wmv", "flv", "ogm"];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Image,
    Gif,
    /// Needs mpvpaper (`video_wallpapers` capability) to actually play.
    Video,
}

impl Kind {
    pub fn of(path: &Path) -> Kind {
        let ext = path
            .extension()
            .map(|e| e.to_string_lossy().to_lowercase())
            .unwrap_or_default();
        if VIDEO_EXTENSIONS.contains(&ext.as_str()) {
            Kind::Video
        } else if ext == "gif" {
            Kind::Gif
        } else {
            Kind::Image
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Kind::Image => "image",
            Kind::Gif => "gif",
            Kind::Video => "video",
        }
    }
}

/// Where an entry came from, in Omarchy's merge-priority order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Source {
    /// `backgrounds/<theme>/` — the only source Studio may delete from.
    UserBackgrounds,
    /// The active theme's own `backgrounds/`.
    Theme,
    /// A user-installed theme's `backgrounds/`.
    UserTheme,
    /// A user-installed theme's `videos/`.
    Videos,
}

impl Source {
    pub fn label(self) -> &'static str {
        match self {
            Source::UserBackgrounds => "yours",
            Source::Theme => "theme",
            Source::UserTheme => "theme",
            Source::Videos => "videos",
        }
    }
}

#[derive(Debug, Clone)]
pub struct Entry {
    pub path: PathBuf,
    pub kind: Kind,
    pub source: Source,
}

impl Entry {
    pub fn name(&self) -> String {
        self.path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default()
    }

    /// Only files the user added through source 1 are Studio-removable.
    pub fn removable(&self) -> bool {
        self.source == Source::UserBackgrounds
    }
}

/// The merged, deduplicated wallpaper list plus the active background.
#[derive(Debug, Clone)]
pub struct Wallpapers {
    pub entries: Vec<Entry>,
    /// Target of `current/background`, if the link exists.
    pub current: Option<PathBuf>,
    pub theme: String,
}

impl Wallpapers {
    pub fn load(paths: &OmarchyPaths) -> Self {
        let theme = paths.current_theme_name().unwrap_or_default();
        let sources = [
            (
                Source::UserBackgrounds,
                paths.config.join(format!("backgrounds/{theme}")),
            ),
            (
                Source::Theme,
                paths.config.join("current/theme/backgrounds"),
            ),
            (
                Source::UserTheme,
                paths.config.join(format!("themes/{theme}/backgrounds")),
            ),
            (
                Source::Videos,
                paths.config.join(format!("themes/{theme}/videos")),
            ),
        ];

        let mut entries: Vec<Entry> = Vec::new();
        let mut seen: Vec<PathBuf> = Vec::new();
        for (source, dir) in sources {
            let Ok(read) = std::fs::read_dir(&dir) else {
                continue;
            };
            let mut files: Vec<PathBuf> = read
                .flatten()
                .map(|e| e.path())
                // follow symlinks like `find -L`: keep anything that is a file
                .filter(|p| p.is_file())
                .collect();
            files.sort();
            for path in files {
                let canon = path.canonicalize().unwrap_or_else(|_| path.clone());
                if seen.contains(&canon) {
                    continue; // current/theme often aliases the theme dir
                }
                seen.push(canon);
                entries.push(Entry {
                    kind: Kind::of(&path),
                    path,
                    source,
                });
            }
        }

        let current = std::fs::read_link(paths.config.join("current/background")).ok();
        Self {
            entries,
            current,
            theme,
        }
    }

    /// Is this entry what's on screen right now?
    pub fn is_current(&self, e: &Entry) -> bool {
        let Some(cur) = &self.current else {
            return false;
        };
        e.path == *cur
            || e.path.canonicalize().ok() == cur.canonicalize().ok()
                && e.path.canonicalize().is_ok()
    }

    /// Apply through Omarchy's own script (image/gif/video dispatch included).
    pub fn set(runner: &dyn CommandRunner, path: &Path) -> Result<()> {
        let out = runner.run(&cmds::theme_bg_set(path))?;
        if out.ok() {
            Ok(())
        } else {
            Err(StudioError::External {
                cmd: "omarchy-theme-bg-set".into(),
                detail: out.stderr.trim().to_string(),
            })
        }
    }

    /// Cycle to the next background (Omarchy's own rotation).
    pub fn next(runner: &dyn CommandRunner) -> Result<()> {
        let out = runner.run(&cmds::theme_bg_next())?;
        if out.ok() {
            Ok(())
        } else {
            Err(StudioError::External {
                cmd: "omarchy-theme-bg-next".into(),
                detail: out.stderr.trim().to_string(),
            })
        }
    }

    /// Copy a file into the user backgrounds dir for the current theme.
    /// Returns the destination. Refuses to overwrite an existing name.
    pub fn add(&self, paths: &OmarchyPaths, file: &Path) -> Result<PathBuf> {
        if !file.is_file() {
            return Err(StudioError::External {
                cmd: "wallpaper add".into(),
                detail: format!("{} is not a file", file.display()),
            });
        }
        let dir = paths.config.join(format!("backgrounds/{}", self.theme));
        std::fs::create_dir_all(&dir)?;
        let name = file.file_name().ok_or_else(|| StudioError::External {
            cmd: "wallpaper add".into(),
            detail: "path has no file name".into(),
        })?;
        let dest = dir.join(name);
        if dest.exists() {
            return Err(StudioError::External {
                cmd: "wallpaper add".into(),
                detail: format!("{} already exists — remove it first", dest.display()),
            });
        }
        std::fs::copy(file, &dest)?;
        Ok(dest)
    }

    /// Delete a user-added wallpaper. Theme-owned files are refused — they
    /// belong to the theme, not to us (spec 04 §5: "remove (user files only)").
    pub fn remove(entry: &Entry) -> Result<()> {
        if !entry.removable() {
            return Err(StudioError::External {
                cmd: "wallpaper remove".into(),
                detail: format!(
                    "{} belongs to the theme — only files under backgrounds/<theme>/ can be removed",
                    entry.name()
                ),
            });
        }
        std::fs::remove_file(&entry.path)?;
        Ok(())
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
            "omarchy-studio-wp-{tag}-{}-{nanos}",
            std::process::id()
        ));
        std::fs::create_dir_all(root.join("config/omarchy/current")).unwrap();
        std::fs::write(root.join("config/omarchy/current/theme.name"), "osaka\n").unwrap();
        OmarchyPaths {
            system: root.join("sys/omarchy"),
            config: root.join("config/omarchy"),
            state: root.join("state/omarchy"),
        }
    }

    #[test]
    fn kind_detection_matches_omarchy_lists() {
        assert_eq!(Kind::of(Path::new("a.jpg")), Kind::Image);
        assert_eq!(Kind::of(Path::new("a.PNG")), Kind::Image);
        assert_eq!(Kind::of(Path::new("a.gif")), Kind::Gif);
        assert_eq!(Kind::of(Path::new("a.mp4")), Kind::Video);
        assert_eq!(Kind::of(Path::new("a.WebM")), Kind::Video);
        assert_eq!(Kind::of(Path::new("noext")), Kind::Image);
    }

    #[test]
    fn merges_sources_in_priority_order_and_dedups_via_symlink() {
        let paths = fake_paths("merge");
        let cfg = &paths.config;
        // theme dir with two backgrounds + a video
        std::fs::create_dir_all(cfg.join("themes/osaka/backgrounds")).unwrap();
        std::fs::create_dir_all(cfg.join("themes/osaka/videos")).unwrap();
        std::fs::write(cfg.join("themes/osaka/backgrounds/b.jpg"), "b").unwrap();
        std::fs::write(cfg.join("themes/osaka/backgrounds/a.jpg"), "a").unwrap();
        std::fs::write(cfg.join("themes/osaka/videos/loop.mp4"), "v").unwrap();
        // current/theme -> the user theme dir (the usual aliasing)
        std::os::unix::fs::symlink(cfg.join("themes/osaka"), cfg.join("current/theme")).unwrap();
        // one user-added wallpaper
        std::fs::create_dir_all(cfg.join("backgrounds/osaka")).unwrap();
        std::fs::write(cfg.join("backgrounds/osaka/mine.png"), "m").unwrap();
        // current background link
        std::os::unix::fs::symlink(
            cfg.join("backgrounds/osaka/mine.png"),
            cfg.join("current/background"),
        )
        .unwrap();

        let w = Wallpapers::load(&paths);
        let names: Vec<(String, Source)> = w.entries.iter().map(|e| (e.name(), e.source)).collect();
        assert_eq!(
            names,
            vec![
                ("mine.png".into(), Source::UserBackgrounds),
                ("a.jpg".into(), Source::Theme),
                ("b.jpg".into(), Source::Theme),
                ("loop.mp4".into(), Source::Videos),
            ],
            "priority order, sorted within source, aliased dirs deduplicated"
        );
        assert_eq!(w.entries[3].kind, Kind::Video);

        // current detection + removability rules
        assert!(w.is_current(&w.entries[0]));
        assert!(!w.is_current(&w.entries[1]));
        assert!(w.entries[0].removable());
        assert!(!w.entries[1].removable());
        assert!(Wallpapers::remove(&w.entries[1]).is_err());
        Wallpapers::remove(&w.entries[0]).unwrap();
        assert!(!cfg.join("backgrounds/osaka/mine.png").exists());
    }

    #[test]
    fn add_copies_into_user_dir_and_refuses_overwrite() {
        let paths = fake_paths("add");
        let w = Wallpapers::load(&paths);
        let src = paths.config.parent().unwrap().join("pic.jpg");
        std::fs::write(&src, "img").unwrap();

        let dest = w.add(&paths, &src).unwrap();
        assert_eq!(dest, paths.config.join("backgrounds/osaka/pic.jpg"));
        assert!(dest.is_file());
        // same name again is refused, not clobbered
        assert!(w.add(&paths, &src).is_err());
        // and non-files are rejected
        assert!(w.add(&paths, &paths.config).is_err());
    }
}
