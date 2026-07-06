//! Waybar module catalog (spec 05 §4, roadmap 0.4.2).
//!
//! A catalogue of the standard Waybar modules, each with a human label, a
//! grouping category, a one-line description, any packages it needs, and a
//! sensible default config snippet to drop in when the user adds it. Unknown
//! ids (the user's own `custom/*`) are handled gracefully — the catalog is for
//! guidance, never a gate.

/// A grouping for the module picker.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Group {
    Workspaces,
    System,
    Hardware,
    Audio,
    Network,
    Clock,
    Tray,
    Custom,
}

impl Group {
    pub fn label(self) -> &'static str {
        match self {
            Group::Workspaces => "Workspaces & window",
            Group::System => "System",
            Group::Hardware => "Hardware",
            Group::Audio => "Audio",
            Group::Network => "Network",
            Group::Clock => "Clock",
            Group::Tray => "Tray",
            Group::Custom => "Custom",
        }
    }
}

/// One catalog entry.
#[derive(Debug, Clone, Copy)]
pub struct Module {
    pub id: &'static str,
    pub label: &'static str,
    pub group: Group,
    pub summary: &'static str,
    /// Package(s) this module needs to function, if any (dep-guidance).
    pub needs: &'static [&'static str],
    /// A default config object to insert with the module (JSONC, no key), or
    /// empty when the module needs no config.
    pub default_config: &'static str,
}

use Group::*;

pub const MODULES: &[Module] = &[
    m("hyprland/workspaces", "Workspaces", Workspaces, "The Hyprland workspace pills", &[], ""),
    m("hyprland/window", "Focused window", Workspaces, "Title of the active window", &[], ""),
    m("hyprland/submap", "Active submap", Workspaces, "Shows the current keybind submap", &[], ""),
    m("clock", "Clock", Clock, "Date and time", &[], "{\n  \"format\": \"{:%H:%M}\",\n  \"tooltip-format\": \"<big>{:%Y %B}</big>\\n<tt><small>{calendar}</small></tt>\"\n}"),
    m("battery", "Battery", Hardware, "Charge level and state", &[], "{\n  \"format\": \"{icon} {capacity}%\",\n  \"format-icons\": [\"\", \"\", \"\", \"\", \"\"]\n}"),
    m("cpu", "CPU", Hardware, "Processor load", &[], "{\n  \"format\": \" {usage}%\"\n}"),
    m("memory", "Memory", Hardware, "RAM usage", &[], "{\n  \"format\": \" {}%\"\n}"),
    m("temperature", "Temperature", Hardware, "CPU/GPU temperature", &["lm_sensors"], "{\n  \"critical-threshold\": 80,\n  \"format\": \" {temperatureC}°C\"\n}"),
    m("disk", "Disk", Hardware, "Filesystem usage", &[], "{\n  \"format\": \" {percentage_used}%\"\n}"),
    m("backlight", "Brightness", Hardware, "Screen backlight level", &[], "{\n  \"format\": \"{icon} {percent}%\",\n  \"format-icons\": [\"\", \"\"]\n}"),
    m("idle_inhibitor", "Keep awake", System, "Toggle to stop the screen sleeping", &[], "{\n  \"format\": \"{icon}\",\n  \"format-icons\": { \"activated\": \"\", \"deactivated\": \"\" }\n}"),
    m("power-profiles-daemon", "Power profile", System, "Balanced / performance / saver", &["power-profiles-daemon"], ""),
    m("pulseaudio", "Volume", Audio, "Output volume and device", &[], "{\n  \"format\": \"{icon} {volume}%\",\n  \"format-muted\": \" muted\",\n  \"on-click\": \"pavucontrol\"\n}"),
    m("wireplumber", "Volume (WirePlumber)", Audio, "Volume via WirePlumber", &["wireplumber"], "{\n  \"format\": \"{icon} {volume}%\"\n}"),
    m("mpris", "Now playing", Audio, "Current media track", &["playerctl"], "{\n  \"format\": \"{player_icon} {title}\"\n}"),
    m("network", "Network", Network, "Wi-Fi / ethernet status", &[], "{\n  \"format-wifi\": \" {signalStrength}%\",\n  \"format-ethernet\": \" wired\",\n  \"format-disconnected\": \"⚠ offline\"\n}"),
    m("bluetooth", "Bluetooth", Network, "Bluetooth status and devices", &[], "{\n  \"format\": \" {status}\"\n}"),
    m("tray", "System tray", Tray, "Status icons from running apps", &[], "{\n  \"spacing\": 10\n}"),
    m("custom/*", "Custom command", Custom, "A script-driven module you define", &[], "{\n  \"exec\": \"your-command\",\n  \"interval\": 5,\n  \"format\": \"{}\"\n}"),
    m("group/*", "Module group", Custom, "A collapsible group of modules", &[], "{\n  \"orientation\": \"horizontal\",\n  \"modules\": []\n}"),
];

const fn m(
    id: &'static str,
    label: &'static str,
    group: Group,
    summary: &'static str,
    needs: &'static [&'static str],
    default_config: &'static str,
) -> Module {
    Module {
        id,
        label,
        group,
        summary,
        needs,
        default_config,
    }
}

/// Look up a module by exact id, falling back to the `custom/*` and `group/*`
/// catalog entries for any `custom/…` / `group/…` id the user has invented.
pub fn lookup(id: &str) -> Option<&'static Module> {
    if let Some(exact) = MODULES.iter().find(|m| m.id == id) {
        return Some(exact);
    }
    if id.starts_with("custom/") {
        return MODULES.iter().find(|m| m.id == "custom/*");
    }
    if id.starts_with("group/") {
        return MODULES.iter().find(|m| m.id == "group/*");
    }
    None
}

/// The human label for a module id (falls back to the id itself).
pub fn label_for(id: &str) -> String {
    match lookup(id) {
        // for custom/group, show the user's own suffix so rows stay distinct
        Some(m) if m.id.ends_with("/*") => id.to_string(),
        Some(m) => m.label.to_string(),
        None => id.to_string(),
    }
}

pub fn in_group(g: Group) -> impl Iterator<Item = &'static Module> {
    MODULES.iter().filter(move |m| m.group == g)
}

// ── config model ─────────────────────────────────────────────────────────────

use std::path::PathBuf;

use crate::configfs::jsonc::JsoncDoc;
use crate::error::{Result, StudioError};
use crate::omarchy::{Component, OmarchyPaths};

/// The three Waybar module lanes, in bar order.
pub const LANES: [&str; 3] = ["modules-left", "modules-center", "modules-right"];

/// A live view over the user's `config.jsonc`, edited through the JSONC editor
/// so comments and formatting survive.
pub struct WaybarConfig {
    doc: JsoncDoc,
    path: PathBuf,
}

fn config_path(paths: &OmarchyPaths) -> PathBuf {
    paths
        .config
        .parent()
        .map(|c| c.join("waybar/config.jsonc"))
        .unwrap_or_else(|| PathBuf::from("waybar/config.jsonc"))
}

impl WaybarConfig {
    pub fn load(paths: &OmarchyPaths) -> Result<Self> {
        let path = config_path(paths);
        let src = std::fs::read_to_string(&path).map_err(|e| StudioError::External {
            cmd: format!("read {}", path.display()),
            detail: e.to_string(),
        })?;
        let doc = JsoncDoc::parse(&src).map_err(|e| StudioError::ParseFailed {
            file: path.clone(),
            line: None,
            hint: e,
        })?;
        Ok(Self { doc, path })
    }

    /// Module ids in a lane, unquoted for display (e.g. `clock`, `custom/x`).
    pub fn lane(&self, lane: &str) -> Vec<String> {
        self.doc
            .array_items(lane)
            .unwrap_or_default()
            .iter()
            .map(|s| s.trim_matches('"').to_string())
            .collect()
    }

    /// All three lanes in bar order.
    pub fn lanes(&self) -> Vec<(&'static str, Vec<String>)> {
        LANES.iter().map(|l| (*l, self.lane(l))).collect()
    }

    pub fn reorder(&mut self, lane: &str, ids: &[String]) -> Result<()> {
        let quoted: Vec<String> = ids.iter().map(|i| format!("\"{i}\"")).collect();
        self.doc.reorder(lane, &quoted).map_err(edit_err)
    }

    pub fn add(&mut self, lane: &str, id: &str) -> Result<()> {
        self.doc
            .add_item(lane, &format!("\"{id}\""))
            .map_err(edit_err)
    }

    pub fn remove(&mut self, lane: &str, id: &str) -> Result<()> {
        self.doc
            .remove_item(lane, &format!("\"{id}\""))
            .map_err(edit_err)
    }

    /// Move a module to `lane`, removing it from whichever lane holds it.
    pub fn move_to(&mut self, id: &str, lane: &str) -> Result<()> {
        for l in LANES {
            self.remove(l, id)?;
        }
        self.add(lane, id)
    }

    /// Set a scalar config value in place (e.g. `height` → `30`). `raw` is the
    /// literal JSON text (numbers bare, strings quoted).
    pub fn set_scalar(&mut self, path: &str, raw: &str) -> Result<()> {
        self.doc.set_scalar(path, raw).map_err(edit_err)
    }

    pub fn path(&self) -> &std::path::Path {
        &self.path
    }

    pub fn save(&self) -> Result<()> {
        crate::configfs::atomic_write(&self.path, &self.doc.to_string())
    }

    /// Save and restart Waybar so the change shows. Caller snapshots first.
    pub fn apply(&self, runner: &dyn crate::cmd::CommandRunner) -> Result<()> {
        self.save()?;
        let out = runner.run(&crate::omarchy::cmds::restart(Component::Waybar))?;
        if !out.ok() {
            return Err(StudioError::External {
                cmd: "omarchy-restart-waybar".into(),
                detail: out.stderr.trim().to_string(),
            });
        }
        Ok(())
    }
}

fn edit_err(e: String) -> StudioError {
    StudioError::External {
        cmd: "edit waybar config".into(),
        detail: e,
    }
}

// ── style.css geometry (roadmap 0.4.4) ───────────────────────────────────────

use crate::configfs::{CommentStyle, ManagedBlock};

/// A couple of safe geometry knobs written to a managed CSS block at the end of
/// the user's `style.css` — after the theme `@import`, so it wins. We never
/// touch the import or the user's own rules.
#[derive(Default)]
pub struct WaybarStyle {
    pub font_size: Option<u32>,
    pub radius: Option<u32>,
}

fn style_path(paths: &OmarchyPaths) -> PathBuf {
    paths
        .config
        .parent()
        .map(|c| c.join("waybar/style.css"))
        .unwrap_or_else(|| PathBuf::from("waybar/style.css"))
}

fn style_block() -> ManagedBlock {
    ManagedBlock::new("waybar-style", CommentStyle::CBlock)
}

impl WaybarStyle {
    pub fn load(paths: &OmarchyPaths) -> Self {
        let mut s = Self::default();
        if let Ok(text) = std::fs::read_to_string(style_path(paths)) {
            if let Some(body) = style_block().extract(&text) {
                s.font_size = grab_px(body, "font-size");
                s.radius = grab_px(body, "border-radius");
            }
        }
        s
    }

    fn body(&self) -> String {
        let mut decls = String::new();
        if let Some(fs) = self.font_size {
            decls.push_str(&format!("  font-size: {fs}px;\n"));
        }
        if let Some(r) = self.radius {
            decls.push_str(&format!("  border-radius: {r}px;\n"));
        }
        format!("* {{\n{decls}}}")
    }

    fn is_empty(&self) -> bool {
        self.font_size.is_none() && self.radius.is_none()
    }

    /// Write the managed block into style.css (removed when empty). The block
    /// always lands at the end, after any `@import`. Returns the path.
    pub fn save(&self, paths: &OmarchyPaths) -> Result<PathBuf> {
        let path = style_path(paths);
        let existing = std::fs::read_to_string(&path).unwrap_or_default();
        let updated = if self.is_empty() {
            style_block().remove(&existing)
        } else {
            style_block().upsert(&existing, &self.body())
        };
        crate::configfs::atomic_write(&path, &updated)?;
        Ok(path)
    }

    pub fn apply(
        &self,
        paths: &OmarchyPaths,
        runner: &dyn crate::cmd::CommandRunner,
    ) -> Result<PathBuf> {
        let path = self.save(paths)?;
        let out = runner.run(&crate::omarchy::cmds::restart(Component::Waybar))?;
        if !out.ok() {
            return Err(StudioError::External {
                cmd: "omarchy-restart-waybar".into(),
                detail: out.stderr.trim().to_string(),
            });
        }
        Ok(path)
    }
}

/// Pull an integer px value for a CSS property out of a block body.
fn grab_px(body: &str, prop: &str) -> Option<u32> {
    let needle = format!("{prop}:");
    let start = body.find(&needle)? + needle.len();
    let rest = &body[start..];
    let digits: String = rest
        .trim_start()
        .chars()
        .take_while(|c| c.is_ascii_digit())
        .collect();
    digits.parse().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalog_is_unique_and_snippets_are_valid_jsonc() {
        use crate::configfs::jsonc::JsoncDoc;
        let mut ids: Vec<_> = MODULES.iter().map(|m| m.id).collect();
        let n = ids.len();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), n, "duplicate module id");

        for m in MODULES {
            if !m.default_config.is_empty() {
                // each snippet must parse as a JSONC object
                let wrapped = format!(
                    "{{ \"{}\": {} }}",
                    m.id.trim_end_matches("/*"),
                    m.default_config
                );
                JsoncDoc::parse(&wrapped)
                    .unwrap_or_else(|e| panic!("bad snippet for {}: {e}", m.id));
            }
        }
    }

    #[test]
    fn resolves_standard_and_custom_ids() {
        assert_eq!(lookup("clock").unwrap().group, Group::Clock);
        assert_eq!(label_for("clock"), "Clock");
        // the user's real modules resolve (custom/* + group/* fallbacks)
        for id in [
            "hyprland/workspaces",
            "battery",
            "network",
            "pulseaudio",
            "tray",
        ] {
            assert!(lookup(id).is_some(), "missing {id}");
        }
        assert!(lookup("custom/voxtype").is_some());
        assert_eq!(label_for("custom/voxtype"), "custom/voxtype");
        assert!(lookup("group/tray-expander").is_some());
        assert!(lookup("nonexistent-core-module").is_none());
    }

    #[test]
    fn deps_are_surfaced() {
        assert_eq!(lookup("temperature").unwrap().needs, &["lm_sensors"]);
        assert!(lookup("clock").unwrap().needs.is_empty());
    }

    fn fake_paths(tag: &str) -> OmarchyPaths {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .subsec_nanos();
        let root = std::env::temp_dir().join(format!(
            "omarchy-studio-wb-{tag}-{}-{nanos}",
            std::process::id()
        ));
        std::fs::create_dir_all(root.join(".config/waybar")).unwrap();
        OmarchyPaths {
            system: root.join("sys/omarchy"),
            config: root.join(".config/omarchy"),
            state: root.join(".local/state/omarchy"),
        }
    }

    #[test]
    fn config_model_reads_and_edits_lanes() {
        let paths = fake_paths("model");
        std::fs::write(
            config_path(&paths),
            "{\n  // bar\n  \"modules-left\": [\"clock\", \"cpu\"],\n  \"modules-center\": [],\n  \"modules-right\": [\"battery\", \"tray\"]\n}\n",
        )
        .unwrap();

        let mut wb = WaybarConfig::load(&paths).unwrap();
        assert_eq!(wb.lane("modules-left"), vec!["clock", "cpu"]);
        assert_eq!(wb.lanes().len(), 3);

        // reorder, add, remove — all through the comment-preserving editor
        wb.reorder("modules-left", &["cpu".into(), "clock".into()])
            .unwrap();
        assert_eq!(wb.lane("modules-left"), vec!["cpu", "clock"]);
        wb.add("modules-center", "network").unwrap();
        assert_eq!(wb.lane("modules-center"), vec!["network"]);
        wb.remove("modules-right", "battery").unwrap();
        assert_eq!(wb.lane("modules-right"), vec!["tray"]);

        wb.save().unwrap();
        let on_disk = std::fs::read_to_string(config_path(&paths)).unwrap();
        assert!(on_disk.contains("// bar")); // comment preserved through edits
                                             // reloads to the same state
        let wb2 = WaybarConfig::load(&paths).unwrap();
        assert_eq!(wb2.lane("modules-left"), vec!["cpu", "clock"]);
    }

    #[test]
    fn style_geometry_block_after_import_and_round_trips() {
        let paths = fake_paths("style");
        let css = "@import \"theme.css\";\n\n* {\n  font-size: 12px;\n}\n";
        std::fs::write(style_path(&paths), css).unwrap();

        let mut st = WaybarStyle::load(&paths);
        assert!(st.font_size.is_none()); // nothing managed yet
        st.font_size = Some(14);
        st.radius = Some(6);
        st.save(&paths).unwrap();

        let out = std::fs::read_to_string(style_path(&paths)).unwrap();
        // the @import stays first; our block is appended after the user's rules
        assert!(out.starts_with("@import"));
        let import_pos = out.find("@import").unwrap();
        let block_pos = out.find("omarchy-studio:waybar-style").unwrap();
        assert!(import_pos < block_pos, "managed block must follow @import");
        assert!(out.contains("font-size: 14px"));
        assert!(out.contains("border-radius: 6px"));
        // the user's own rule is untouched
        assert!(out.contains("font-size: 12px"));

        // reload reads the managed values back
        let re = WaybarStyle::load(&paths);
        assert_eq!(re.font_size, Some(14));
        assert_eq!(re.radius, Some(6));

        // clearing removes the block, leaving the original css intact
        let empty = WaybarStyle::default();
        empty.save(&paths).unwrap();
        let cleared = std::fs::read_to_string(style_path(&paths)).unwrap();
        assert!(!cleared.contains("omarchy-studio:waybar-style"));
        assert_eq!(cleared, css);
    }
}
