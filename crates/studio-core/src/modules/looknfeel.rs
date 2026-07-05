//! Look & Feel schema + model (spec 05 §2, roadmap 0.3.1).
//!
//! A small, curated schema of the Hyprland general/decoration settings people
//! actually reach for — gaps, borders, rounding, opacity, blur, shadow — each
//! with a human label, a value kind (for validation and the right widget), and
//! a category for grouping. The [`LookFeel`] model reads the *effective* value
//! of each setting (Omarchy default overlaid by the user's file) and writes
//! changes into a managed block in the user's `looknfeel.conf`, which Hyprland
//! sources last so those values win. Nothing outside the block is touched.

use std::collections::BTreeMap;
use std::path::PathBuf;

use crate::configfs::hyprlang::HyprDoc;
use crate::configfs::{atomic_write, CommentStyle, ManagedBlock};
use crate::error::{Result, StudioError};
use crate::omarchy::OmarchyPaths;

/// The kind of a setting's value — drives validation and the editor widget.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Kind {
    Int { min: i64, max: i64 },
    Float { min: f64, max: f64 },
    Bool,
    Enum(&'static [&'static str]),
}

/// One editable look & feel setting.
#[derive(Debug, Clone, Copy)]
pub struct Setting {
    /// Dotted key in looknfeel.conf, e.g. `decoration.blur.size`.
    pub key: &'static str,
    pub label: &'static str,
    /// UI grouping header.
    pub group: &'static str,
    pub kind: Kind,
    /// The Omarchy default, shown when the user hasn't overridden it.
    pub default: &'static str,
    pub help: &'static str,
}

use Kind::*;

/// The curated schema. Order is display order within each group.
pub const SETTINGS: &[Setting] = &[
    // Windows
    s(
        "general.gaps_in",
        "Inner gaps",
        "Windows",
        Int { min: 0, max: 60 },
        "5",
        "Space between tiled windows",
    ),
    s(
        "general.gaps_out",
        "Outer gaps",
        "Windows",
        Int { min: 0, max: 100 },
        "10",
        "Space between windows and the screen edge",
    ),
    s(
        "general.border_size",
        "Border width",
        "Windows",
        Int { min: 0, max: 12 },
        "2",
        "Thickness of the window border",
    ),
    s(
        "general.resize_on_border",
        "Resize on border",
        "Windows",
        Bool,
        "false",
        "Drag a window's border or gap to resize it",
    ),
    s(
        "general.layout",
        "Tiling layout",
        "Windows",
        Enum(&["dwindle", "master"]),
        "dwindle",
        "How new windows are arranged",
    ),
    // Corners & opacity
    s(
        "decoration.rounding",
        "Corner rounding",
        "Corners & opacity",
        Int { min: 0, max: 30 },
        "0",
        "Radius of rounded window corners",
    ),
    s(
        "decoration.active_opacity",
        "Active opacity",
        "Corners & opacity",
        Float { min: 0.0, max: 1.0 },
        "1.0",
        "Opacity of the focused window",
    ),
    s(
        "decoration.inactive_opacity",
        "Inactive opacity",
        "Corners & opacity",
        Float { min: 0.0, max: 1.0 },
        "1.0",
        "Opacity of unfocused windows",
    ),
    s(
        "decoration.dim_inactive",
        "Dim inactive",
        "Corners & opacity",
        Bool,
        "false",
        "Darken windows that aren't focused",
    ),
    s(
        "decoration.dim_strength",
        "Dim strength",
        "Corners & opacity",
        Float { min: 0.0, max: 1.0 },
        "0.5",
        "How strongly inactive windows are dimmed",
    ),
    // Blur
    s(
        "decoration.blur.enabled",
        "Blur",
        "Blur",
        Bool,
        "true",
        "Blur behind translucent windows",
    ),
    s(
        "decoration.blur.size",
        "Blur size",
        "Blur",
        Int { min: 1, max: 20 },
        "2",
        "Radius of the background blur",
    ),
    s(
        "decoration.blur.passes",
        "Blur passes",
        "Blur",
        Int { min: 1, max: 6 },
        "2",
        "More passes = smoother, heavier blur",
    ),
    // Shadow
    s(
        "decoration.shadow.enabled",
        "Shadow",
        "Shadow",
        Bool,
        "true",
        "Drop shadow under windows",
    ),
    s(
        "decoration.shadow.range",
        "Shadow size",
        "Shadow",
        Int { min: 0, max: 60 },
        "2",
        "How far the shadow spreads",
    ),
    s(
        "decoration.shadow.render_power",
        "Shadow softness",
        "Shadow",
        Int { min: 1, max: 4 },
        "3",
        "Falloff curve of the shadow",
    ),
];

const fn s(
    key: &'static str,
    label: &'static str,
    group: &'static str,
    kind: Kind,
    default: &'static str,
    help: &'static str,
) -> Setting {
    Setting {
        key,
        label,
        group,
        kind,
        default,
        help,
    }
}

pub fn lookup(key: &str) -> Option<&'static Setting> {
    SETTINGS.iter().find(|s| s.key == key)
}

/// Groups in display order (deduplicated, stable).
pub fn groups() -> Vec<&'static str> {
    let mut seen = Vec::new();
    for s in SETTINGS {
        if !seen.contains(&s.group) {
            seen.push(s.group);
        }
    }
    seen
}

impl Kind {
    /// Validate and normalize a candidate value, or return why it's rejected.
    pub fn validate(&self, raw: &str) -> std::result::Result<String, String> {
        let raw = raw.trim();
        match self {
            Kind::Int { min, max } => {
                let v: i64 = raw
                    .parse()
                    .map_err(|_| format!("“{raw}” isn't a whole number"))?;
                if v < *min || v > *max {
                    return Err(format!("must be between {min} and {max}"));
                }
                Ok(v.to_string())
            }
            Kind::Float { min, max } => {
                let v: f64 = raw.parse().map_err(|_| format!("“{raw}” isn't a number"))?;
                if v < *min || v > *max {
                    return Err(format!("must be between {min} and {max}"));
                }
                Ok(format!("{v}"))
            }
            Kind::Bool => match raw.to_ascii_lowercase().as_str() {
                "true" | "yes" | "on" | "1" => Ok("true".into()),
                "false" | "no" | "off" | "0" => Ok("false".into()),
                _ => Err("must be on or off".into()),
            },
            Kind::Enum(opts) => {
                if opts.contains(&raw) {
                    Ok(raw.to_string())
                } else {
                    Err(format!("must be one of: {}", opts.join(", ")))
                }
            }
        }
    }
}

/// The look & feel model: effective base values overlaid by Studio overrides.
pub struct LookFeel {
    /// Effective values from Omarchy default + the user's own (non-Studio) file.
    base: BTreeMap<String, String>,
    /// Studio's overrides, written to the managed block. Dotted key → value.
    overrides: BTreeMap<String, String>,
}

fn block() -> ManagedBlock {
    ManagedBlock::new("looknfeel", CommentStyle::Hash)
}

fn user_path(paths: &OmarchyPaths) -> PathBuf {
    paths.hypr_config().join("looknfeel.conf")
}

impl LookFeel {
    pub fn load(paths: &OmarchyPaths) -> Self {
        // Base: default looknfeel overlaid by the user's file with our managed
        // block stripped out (so we read the user's own values, not ours).
        let mut base = BTreeMap::new();
        let default = paths.system.join("default/hypr/looknfeel.conf");
        for path in [default, user_path(paths)] {
            if let Ok(text) = std::fs::read_to_string(&path) {
                let cleaned = block().remove(&text);
                let doc = HyprDoc::parse(&cleaned);
                for setting in SETTINGS {
                    if let Some(v) = doc.get(setting.key) {
                        base.insert(setting.key.to_string(), v);
                    }
                }
            }
        }

        // Overrides: whatever is in our managed block already.
        let mut overrides = BTreeMap::new();
        if let Ok(text) = std::fs::read_to_string(user_path(paths)) {
            if let Some(body) = block().extract(&text) {
                let doc = HyprDoc::parse(body);
                for e in doc.entries() {
                    overrides.insert(e.dotted(), e.value);
                }
            }
        }

        Self { base, overrides }
    }

    /// The effective value shown to the user: override, else base, else the
    /// schema default.
    pub fn value(&self, key: &str) -> String {
        self.overrides
            .get(key)
            .or_else(|| self.base.get(key))
            .cloned()
            .unwrap_or_else(|| {
                lookup(key)
                    .map(|s| s.default.to_string())
                    .unwrap_or_default()
            })
    }

    pub fn is_overridden(&self, key: &str) -> bool {
        self.overrides.contains_key(key)
    }

    /// Set (and validate) an override. Returns the rejection reason on failure.
    pub fn set(&mut self, key: &str, raw: &str) -> std::result::Result<(), String> {
        let setting = lookup(key).ok_or_else(|| format!("unknown setting {key}"))?;
        let normalized = setting.kind.validate(raw)?;
        self.overrides.insert(key.to_string(), normalized);
        Ok(())
    }

    /// Drop an override, reverting to the base/default value.
    pub fn clear(&mut self, key: &str) {
        self.overrides.remove(key);
    }

    /// Render the managed block body as nested category blocks.
    pub fn render_block_body(&self) -> String {
        let entries: Vec<(Vec<String>, String)> = self
            .overrides
            .iter()
            .map(|(k, v)| (k.split('.').map(str::to_string).collect(), v.clone()))
            .collect();
        let mut out = String::from("# Managed by Omarchy Studio — your look & feel tweaks.\n");
        render_nested(&entries, 0, &mut out);
        out.trim_end().to_string()
    }

    /// Persist the managed block (empty overrides remove it). Returns the path.
    pub fn save(&self, paths: &OmarchyPaths) -> Result<PathBuf> {
        let path = user_path(paths);
        let existing = std::fs::read_to_string(&path).unwrap_or_default();
        let updated = if self.overrides.is_empty() {
            block().remove(&existing)
        } else {
            block().upsert(&existing, &self.render_block_body())
        };
        atomic_write(&path, &updated)?;
        Ok(path)
    }

    /// Save and live-reload Hyprland. Caller snapshots first for undo.
    pub fn apply(
        &self,
        paths: &OmarchyPaths,
        runner: &dyn crate::cmd::CommandRunner,
    ) -> Result<PathBuf> {
        let path = self.save(paths)?;
        let out = runner.run(&crate::omarchy::cmds::hypr_reload())?;
        if !out.ok() {
            return Err(StudioError::External {
                cmd: "hyprctl reload".into(),
                detail: out.stderr.trim().to_string(),
            });
        }
        Ok(path)
    }
}

/// Emit `path…value` entries as nested `category { … }` blocks. Entries whose
/// remaining path is a single segment are leaves (`key = value`); longer paths
/// nest under their first segment.
fn render_nested(entries: &[(Vec<String>, String)], depth: usize, out: &mut String) {
    let indent = "    ".repeat(depth);
    // leaves at this level
    for (path, value) in entries.iter().filter(|(p, _)| p.len() == 1) {
        out.push_str(&format!("{indent}{} = {value}\n", path[0]));
    }
    // group the deeper entries by their first segment, preserving first-seen order
    let mut order: Vec<String> = Vec::new();
    for (path, _) in entries.iter().filter(|(p, _)| p.len() > 1) {
        if !order.contains(&path[0]) {
            order.push(path[0].clone());
        }
    }
    for cat in order {
        out.push_str(&format!("{indent}{cat} {{\n"));
        let children: Vec<(Vec<String>, String)> = entries
            .iter()
            .filter(|(p, _)| p.len() > 1 && p[0] == cat)
            .map(|(p, v)| (p[1..].to_vec(), v.clone()))
            .collect();
        render_nested(&children, depth + 1, out);
        out.push_str(&format!("{indent}}}\n"));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn schema_is_coherent() {
        for s in SETTINGS {
            // defaults must validate against their own kind
            assert!(
                s.kind.validate(s.default).is_ok(),
                "default {:?} invalid for {}",
                s.default,
                s.key
            );
        }
        assert!(lookup("decoration.blur.size").is_some());
        assert!(groups().contains(&"Blur"));
    }

    #[test]
    fn validation_enforces_kind_and_range() {
        let int = Kind::Int { min: 0, max: 10 };
        assert_eq!(int.validate("7").unwrap(), "7");
        assert!(int.validate("20").is_err());
        assert!(int.validate("x").is_err());
        assert_eq!(Kind::Bool.validate("YES").unwrap(), "true");
        assert_eq!(Kind::Bool.validate("off").unwrap(), "false");
        let en = Kind::Enum(&["dwindle", "master"]);
        assert!(en.validate("master").is_ok());
        assert!(en.validate("floating").is_err());
    }

    fn fake_paths(tag: &str) -> OmarchyPaths {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .subsec_nanos();
        let root = std::env::temp_dir().join(format!(
            "omarchy-studio-lnf-{tag}-{}-{nanos}",
            std::process::id()
        ));
        std::fs::create_dir_all(root.join("sys/omarchy/default/hypr")).unwrap();
        std::fs::create_dir_all(root.join(".config/hypr")).unwrap();
        OmarchyPaths {
            system: root.join("sys/omarchy"),
            config: root.join(".config/omarchy"),
            state: root.join(".local/state/omarchy"),
        }
    }

    #[test]
    fn effective_value_overlays_default_user_and_override() {
        let paths = fake_paths("effective");
        std::fs::write(
            paths.system.join("default/hypr/looknfeel.conf"),
            "general {\n    gaps_in = 5\n    border_size = 2\n}\n",
        )
        .unwrap();
        // user's own file bumps border_size; no Studio block yet
        std::fs::write(user_path(&paths), "general {\n    border_size = 4\n}\n").unwrap();

        let mut lf = LookFeel::load(&paths);
        assert_eq!(lf.value("general.gaps_in"), "5"); // from default
        assert_eq!(lf.value("general.border_size"), "4"); // user overrides default
        assert_eq!(lf.value("decoration.rounding"), "0"); // schema default (absent)
        assert!(!lf.is_overridden("general.gaps_in"));

        // set an override, save, and confirm it round-trips + wins
        lf.set("general.gaps_in", "12").unwrap();
        assert!(lf.set("general.gaps_in", "999").is_err()); // out of range, unchanged
        lf.save(&paths).unwrap();

        let reloaded = LookFeel::load(&paths);
        assert_eq!(reloaded.value("general.gaps_in"), "12");
        assert!(reloaded.is_overridden("general.gaps_in"));
        assert_eq!(reloaded.value("general.border_size"), "4"); // user's own preserved
        let on_disk = std::fs::read_to_string(user_path(&paths)).unwrap();
        assert!(on_disk.contains("border_size = 4")); // untouched
        assert!(on_disk.contains("omarchy-studio:looknfeel"));

        // clearing all overrides removes the block, restoring the user's file
        let mut r2 = LookFeel::load(&paths);
        r2.clear("general.gaps_in");
        r2.save(&paths).unwrap();
        let after = std::fs::read_to_string(user_path(&paths)).unwrap();
        assert!(!after.contains("omarchy-studio:looknfeel"));
        assert!(after.contains("border_size = 4"));
    }

    #[test]
    fn nested_block_renders_categories() {
        let entries = vec![
            (vec!["general".into(), "gaps_in".into()], "8".to_string()),
            (
                vec!["decoration".into(), "rounding".into()],
                "12".to_string(),
            ),
            (
                vec!["decoration".into(), "blur".into(), "size".into()],
                "4".to_string(),
            ),
        ];
        let mut out = String::new();
        render_nested(&entries, 0, &mut out);
        assert!(out.contains("general {\n    gaps_in = 8\n}"));
        assert!(out.contains("decoration {\n    rounding = 12\n"));
        assert!(out.contains("    blur {\n        size = 4\n    }"));
        // and it parses back as valid hyprlang with the right dotted keys
        let doc = HyprDoc::parse(&out);
        assert_eq!(doc.get("decoration.blur.size").as_deref(), Some("4"));
        assert_eq!(doc.get("general.gaps_in").as_deref(), Some("8"));
    }
}
