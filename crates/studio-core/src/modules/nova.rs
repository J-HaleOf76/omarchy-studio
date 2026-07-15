//! Nova launcher — the Quickshell app-search overlay (github arino08/nova).
//!
//! Nova is spawned per-invocation from a keybind and reads its whole config at
//! launch from `~/.config/nova/nova.json`, so "apply" here is just an atomic
//! save — there is no daemon to restart and the next launch picks it up.
//! Theme-sync is free: Nova reads the active Omarchy `colors.toml` itself.
//!
//! Studio owns three things:
//!   * the JSON config (visual mode, providers, backdrop, animation toggles) —
//!     unknown keys round-trip untouched so a newer Nova never loses fields;
//!   * an optional launch keybind, written as a `bindd` line through the
//!     keybinds override block (same managed block, same undo story);
//!   * launch detection — the run script of a checkout at `~/Projects/nova`.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::configfs::atomic_write;
use crate::error::{Result, StudioError};
use crate::modules::keybinds::{self, ConfigBind, Override};
use crate::omarchy::OmarchyPaths;

/// The four visual modes, in Nova's own cycle order.
pub const MODES: [&str; 4] = ["constellation", "spotlight", "orbital", "grid"];

/// Elephant providers Nova's default config searches. `symbols`/`unicode`
/// exist but are excluded from Nova's defaults (they bury real hits).
pub const KNOWN_PROVIDERS: [&str; 10] = [
    "desktopapplications",
    "calc",
    "files",
    "clipboard",
    "websearch",
    "menus",
    "runner",
    "todo",
    "symbols",
    "unicode",
];

/// Marker description on the managed keybind line (how we find ours again).
pub const BIND_DESC: &str = "Nova launcher";

fn config_json(paths: &OmarchyPaths) -> PathBuf {
    paths
        .config
        .parent()
        .map(|c| c.join("nova/nova.json"))
        .unwrap_or_else(|| PathBuf::from("nova/nova.json"))
}

/// The `anim` block — every toggle Nova's modes read.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct AnimCfg {
    pub stagger: bool,
    pub stagger_ms: i64,
    pub living_background: bool,
    pub spring: bool,
    pub micro: bool,
    /// Keys a newer Nova knows and this Studio doesn't — preserved verbatim.
    #[serde(flatten)]
    pub extra: serde_json::Map<String, serde_json::Value>,
}

impl Default for AnimCfg {
    fn default() -> Self {
        Self {
            stagger: true,
            stagger_ms: 26,
            living_background: true,
            spring: true,
            micro: true,
            extra: serde_json::Map::new(),
        }
    }
}

/// `nova.json`, defaults matching Nova's bundled `config/nova.json`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct NovaConfig {
    pub mode: String,
    pub providers: Vec<String>,
    pub limit: i64,
    pub backdrop_opacity: f64,
    pub blur: bool,
    pub accent_glow: bool,
    pub theme: String,
    pub anim: AnimCfg,
    #[serde(flatten)]
    pub extra: serde_json::Map<String, serde_json::Value>,
}

impl Default for NovaConfig {
    fn default() -> Self {
        Self {
            mode: "constellation".into(),
            providers: [
                "desktopapplications",
                "calc",
                "files",
                "clipboard",
                "websearch",
                "menus",
                "runner",
                "todo",
            ]
            .iter()
            .map(|s| s.to_string())
            .collect(),
            limit: 40,
            backdrop_opacity: 0.985,
            blur: true,
            accent_glow: true,
            theme: "auto".into(),
            anim: AnimCfg::default(),
            extra: serde_json::Map::new(),
        }
    }
}

/// The Nova config file, loaded (or defaulted) and editable in place.
#[derive(Debug, Clone, PartialEq)]
pub struct Nova {
    path: PathBuf,
    /// Whether nova.json existed on load (absent file → defaults, no error).
    pub existed: bool,
    pub cfg: NovaConfig,
}

impl Nova {
    pub fn load(paths: &OmarchyPaths) -> Self {
        let path = config_json(paths);
        let text = std::fs::read_to_string(&path).unwrap_or_default();
        let existed = !text.is_empty();
        let cfg = serde_json::from_str(&text).unwrap_or_default();
        Self { path, existed, cfg }
    }

    pub fn config_path(&self) -> &Path {
        &self.path
    }

    /// Write nova.json (pretty, trailing newline). Takes effect on Nova's next
    /// launch — nothing to restart. Caller snapshots first.
    pub fn save(&self) -> Result<()> {
        let json =
            serde_json::to_string_pretty(&self.cfg).map_err(|e| StudioError::ParseFailed {
                file: self.path.clone(),
                line: None,
                hint: e.to_string(),
            })?;
        atomic_write(&self.path, &format!("{json}\n"))
    }
}

/// The command that launches Nova: the run script of a checkout at
/// `~/Projects/nova`, when present. None → nothing to bind or launch.
pub fn launch_command(paths: &OmarchyPaths) -> Option<String> {
    let home = paths.config.parent()?.parent()?.to_path_buf();
    let script = home.join("Projects/nova/nova");
    if script.is_file() {
        return Some(script.display().to_string());
    }
    let shell = home.join("Projects/nova/shell.qml");
    if shell.is_file() {
        return Some(format!("qs -n -p {}", home.join("Projects/nova").display()));
    }
    None
}

fn is_nova_bind(o: &Override) -> bool {
    matches!(o, Override::Set(cb) if cb.description.as_deref() == Some(BIND_DESC))
}

/// Studio's Nova bind from the override block, if installed.
pub fn keybind(paths: &OmarchyPaths) -> Option<ConfigBind> {
    keybinds::read_overrides(paths)
        .into_iter()
        .find_map(|o| match o {
            Override::Set(cb) if cb.description.as_deref() == Some(BIND_DESC) => Some(cb),
            _ => None,
        })
}

/// Bind `mods+key` to launch Nova (replacing any previous Nova bind), through
/// the shared keybinds override block — sourced last, so it wins over the
/// Omarchy default on the same chord (e.g. SUPER+SPACE's Walker). Reloads
/// Hyprland. Caller snapshots `keybinds::user_bindings_path` first.
pub fn install_keybind(
    paths: &OmarchyPaths,
    mods: &str,
    key: &str,
    exec: &str,
    runner: &dyn crate::cmd::CommandRunner,
) -> Result<ConfigBind> {
    let bind = ConfigBind {
        flags: "bindd".into(),
        modmask: keybinds::mods_to_mask(mods),
        key: key.to_string(),
        description: Some(BIND_DESC.into()),
        dispatcher: "exec".into(),
        arg: exec.to_string(),
    };
    let mut overrides: Vec<Override> = keybinds::read_overrides(paths)
        .into_iter()
        .filter(|o| !is_nova_bind(o))
        .collect();
    overrides.push(Override::Set(bind.clone()));
    keybinds::apply_overrides(paths, &overrides, runner)?;
    Ok(bind)
}

/// Remove the Nova bind from the override block (other overrides survive).
/// Returns false when none was installed. Caller snapshots first.
pub fn remove_keybind(
    paths: &OmarchyPaths,
    runner: &dyn crate::cmd::CommandRunner,
) -> Result<bool> {
    let all = keybinds::read_overrides(paths);
    let kept: Vec<Override> = all.iter().filter(|o| !is_nova_bind(o)).cloned().collect();
    if kept.len() == all.len() {
        return Ok(false);
    }
    keybinds::apply_overrides(paths, &kept, runner)?;
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cmd::StubRunner;

    fn fake_paths(tag: &str) -> OmarchyPaths {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .subsec_nanos();
        let root = std::env::temp_dir().join(format!(
            "omarchy-studio-nova-{tag}-{}-{nanos}",
            std::process::id()
        ));
        std::fs::create_dir_all(root.join(".config/omarchy")).unwrap();
        std::fs::create_dir_all(root.join(".config/hypr")).unwrap();
        OmarchyPaths {
            system: root.join("sys/omarchy"),
            config: root.join(".config/omarchy"),
            state: root.join(".local/state/omarchy"),
        }
    }

    #[test]
    fn missing_file_loads_bundled_defaults() {
        let paths = fake_paths("defaults");
        let nova = Nova::load(&paths);
        assert!(!nova.existed);
        assert_eq!(nova.cfg.mode, "constellation");
        assert_eq!(nova.cfg.limit, 40);
        assert!(nova.cfg.anim.spring);
        assert_eq!(nova.cfg.providers.len(), 8);
    }

    #[test]
    fn save_round_trips_and_preserves_unknown_keys() {
        let paths = fake_paths("roundtrip");
        std::fs::create_dir_all(config_json(&paths).parent().unwrap()).unwrap();
        std::fs::write(
            config_json(&paths),
            r#"{ "mode": "grid", "futureKnob": 7, "anim": { "spring": false, "wobble": true } }"#,
        )
        .unwrap();

        let mut nova = Nova::load(&paths);
        assert!(nova.existed);
        assert_eq!(nova.cfg.mode, "grid");
        assert!(!nova.cfg.anim.spring);
        // unspecified fields fall back to defaults
        assert_eq!(nova.cfg.limit, 40);

        nova.cfg.mode = "orbital".into();
        nova.cfg.backdrop_opacity = 0.9;
        nova.save().unwrap();

        let text = std::fs::read_to_string(config_json(&paths)).unwrap();
        assert!(text.contains("\"orbital\""));
        // unknown keys survive the rewrite
        assert!(text.contains("futureKnob"));
        assert!(text.contains("wobble"));
        // camelCase field names on disk (what Nova's QML reads)
        assert!(text.contains("backdropOpacity"));
        assert!(text.contains("livingBackground"));

        let re = Nova::load(&paths);
        assert_eq!(re.cfg.mode, "orbital");
        assert_eq!(re.cfg.backdrop_opacity, 0.9);
    }

    #[test]
    fn save_creates_the_config_dir_first_run() {
        let paths = fake_paths("firstrun");
        let nova = Nova::load(&paths);
        assert!(!nova.existed);
        nova.save().unwrap();
        assert!(config_json(&paths).is_file());
    }

    #[test]
    fn keybind_installs_updates_and_removes() {
        let paths = fake_paths("bind");
        let runner = StubRunner::default().with_ok("hyprctl reload", "");

        assert!(keybind(&paths).is_none());
        let bind = install_keybind(&paths, "SUPER", "SPACE", "/x/nova", &runner).unwrap();
        assert_eq!(
            bind.render_line(),
            "bindd = SUPER, SPACE, Nova launcher, exec, /x/nova"
        );

        let found = keybind(&paths).expect("bind installed");
        assert_eq!(found.arg, "/x/nova");

        // reinstall with a new chord replaces, not duplicates
        install_keybind(&paths, "SUPER SHIFT", "N", "/x/nova", &runner).unwrap();
        let binds: Vec<_> = keybinds::read_overrides(&paths);
        assert_eq!(binds.iter().filter(|o| is_nova_bind(o)).count(), 1);
        assert_eq!(keybind(&paths).unwrap().key, "N");

        assert!(remove_keybind(&paths, &runner).unwrap());
        assert!(keybind(&paths).is_none());
        assert!(!remove_keybind(&paths, &runner).unwrap());
    }
}
