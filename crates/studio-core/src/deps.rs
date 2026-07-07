//! Dependency & tool registry (spec 06). Data lives in `data/integrations.toml`
//! (embedded at build time) merged with `~/.config/omarchy-studio/integrations.toml`
//! (user overlay, wins by id).
//!
//! Product principle 7: a feature with a missing dependency stays VISIBLE but
//! disabled, with the reason and the exact install command — never a silent
//! failure, never a stack trace.

use crate::cmd::{find_in_path, Cmd, CommandRunner};
use crate::error::{Result, StudioError};

/// Registry id, e.g. `mpvpaper-rs`, `terminal-graphics`.
pub type DepId = String;

/// How a dependency is detected (spec 06 §2).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DepKind {
    Binary,
    Package,
    TerminalGraphics,
    Network,
    Service,
}

impl DepKind {
    fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "binary" => DepKind::Binary,
            "package" => DepKind::Package,
            "terminal-graphics" => DepKind::TerminalGraphics,
            "network" => DepKind::Network,
            "service" => DepKind::Service,
            _ => return None,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DepStatus {
    Present,
    Missing,
    /// Usable but limited; carries a user-language reason.
    Degraded(String),
    /// Only checkable in context (terminal graphics at TUI start, network on
    /// first request) — treated as available until that check runs.
    Deferred,
}

/// One `[deps.*]` entry.
#[derive(Debug, Clone)]
pub struct DepSpec {
    pub id: DepId,
    pub kind: DepKind,
    /// What to look for (binary name / package name / user unit).
    pub probe: Option<String>,
    /// Alternates that also satisfy the dep (e.g. paru for yay).
    pub probe_alt: Vec<String>,
    pub package: Option<String>,
    /// `official` | `aur` — picks pacman vs AUR-helper guidance.
    pub repo: Option<String>,
    /// User-language purpose ("Plays video wallpapers behind your desktop").
    pub reason: String,
    /// Feature ids gated on this dep.
    pub unlocks: Vec<String>,
    /// What the user gets while it's missing.
    pub fallback: String,
}

/// One `[tools.*]` companion entry (detect + interop, never required).
#[derive(Debug, Clone)]
pub struct ToolSpec {
    pub id: String,
    pub probe: Option<String>,
    pub homepage: Option<String>,
    pub install: Option<String>,
    pub license: Option<String>,
    pub interop: Option<String>,
    pub actions: Vec<ToolAction>,
}

#[derive(Debug, Clone)]
pub struct ToolAction {
    pub id: String,
    pub label: String,
    pub exec: String,
    /// Screen where the action surfaces contextually (None = Integrations only).
    pub context: Option<String>,
    /// The tool is a TUI — launch it in a floating terminal, not detached.
    pub terminal: bool,
}

/// Everything a frontend needs to render the guidance banner (spec 06 §3).
#[derive(Debug, Clone)]
pub struct DepReport {
    pub id: DepId,
    pub kind: DepKind,
    pub status: DepStatus,
    pub reason: String,
    /// Exact install command to show/run, e.g. `yay -S mpvpaper-rs`.
    /// `None` for kinds that aren't installable (network, graphics).
    pub install: Option<String>,
    pub fallback: String,
    pub unlocks: Vec<String>,
}

#[derive(Debug, Default)]
pub struct Registry {
    pub deps: Vec<DepSpec>,
    pub tools: Vec<ToolSpec>,
}

impl Registry {
    /// The registry shipped inside the binary (`data/integrations.toml`).
    pub fn builtin() -> Result<Self> {
        Self::parse(include_str!("../data/integrations.toml"))
    }

    pub fn parse(content: &str) -> Result<Self> {
        let doc =
            content
                .parse::<toml_edit::DocumentMut>()
                .map_err(|e| StudioError::ParseFailed {
                    file: std::path::PathBuf::from("integrations.toml"),
                    line: None,
                    hint: format!("not valid TOML: {}", e.message()),
                })?;

        let str_of = |t: &toml_edit::Table, k: &str| -> Option<String> {
            t.get(k).and_then(|v| v.as_str()).map(str::to_string)
        };
        let list_of = |t: &toml_edit::Table, k: &str| -> Vec<String> {
            t.get(k)
                .and_then(|v| v.as_array())
                .map(|a| {
                    a.iter()
                        .filter_map(|v| v.as_str().map(str::to_string))
                        .collect()
                })
                .unwrap_or_default()
        };

        let mut reg = Registry::default();

        if let Some(deps) = doc.get("deps").and_then(|i| i.as_table()) {
            for (id, item) in deps {
                let Some(t) = item.as_table() else { continue };
                let kind = str_of(t, "kind")
                    .and_then(|k| DepKind::parse(&k))
                    .ok_or_else(|| StudioError::ParseFailed {
                        file: std::path::PathBuf::from("integrations.toml"),
                        line: None,
                        hint: format!("dep `{id}` has a missing or unknown `kind`"),
                    })?;
                reg.deps.push(DepSpec {
                    id: id.to_string(),
                    kind,
                    probe: str_of(t, "probe"),
                    probe_alt: list_of(t, "probe-alt"),
                    package: str_of(t, "package"),
                    repo: str_of(t, "repo"),
                    reason: str_of(t, "reason").unwrap_or_default(),
                    unlocks: list_of(t, "unlocks"),
                    fallback: str_of(t, "fallback").unwrap_or_default(),
                });
            }
        }

        if let Some(tools) = doc.get("tools").and_then(|i| i.as_table()) {
            for (id, item) in tools {
                let Some(t) = item.as_table() else { continue };
                let actions = t
                    .get("actions")
                    .and_then(|v| v.as_array())
                    .map(|a| {
                        a.iter()
                            .filter_map(|v| v.as_inline_table())
                            .filter_map(|it| {
                                Some(ToolAction {
                                    id: it.get("id")?.as_str()?.to_string(),
                                    label: it.get("label")?.as_str()?.to_string(),
                                    exec: it.get("exec")?.as_str()?.to_string(),
                                    context: it
                                        .get("context")
                                        .and_then(|c| c.as_str())
                                        .map(str::to_string),
                                    terminal: it
                                        .get("terminal")
                                        .and_then(|b| b.as_bool())
                                        .unwrap_or(false),
                                })
                            })
                            .collect()
                    })
                    .unwrap_or_default();
                reg.tools.push(ToolSpec {
                    id: id.to_string(),
                    probe: str_of(t, "probe"),
                    homepage: str_of(t, "homepage"),
                    install: str_of(t, "install"),
                    license: str_of(t, "license"),
                    interop: str_of(t, "interop"),
                    actions,
                });
            }
        }

        Ok(reg)
    }

    /// Overlay `other` (user file): entries replace built-ins with the same id.
    pub fn merge(mut self, other: Registry) -> Self {
        for dep in other.deps {
            self.deps.retain(|d| d.id != dep.id);
            self.deps.push(dep);
        }
        for tool in other.tools {
            self.tools.retain(|t| t.id != tool.id);
            self.tools.push(tool);
        }
        self
    }

    pub fn dep(&self, id: &str) -> Option<&DepSpec> {
        self.deps.iter().find(|d| d.id == id)
    }

    /// Built-in registry with the user's overlay applied, when one exists
    /// (`~/.config/omarchy-studio/integrations.toml`, entries win by id).
    pub fn load() -> crate::error::Result<Self> {
        let builtin = Self::builtin()?;
        let user_file = crate::studio_config_dir().join("integrations.toml");
        match std::fs::read_to_string(&user_file) {
            Ok(text) => Ok(builtin.merge(Self::parse(&text)?)),
            Err(_) => Ok(builtin),
        }
    }
}

/// A companion tool with its detection result (spec 06 §5).
#[derive(Debug, Clone)]
pub struct ToolReport {
    pub spec: ToolSpec,
    pub present: bool,
}

/// Detect every companion tool — the Integrations screen's second half.
pub fn probe_tools(reg: &Registry) -> Vec<ToolReport> {
    reg.tools
        .iter()
        .map(|t| ToolReport {
            present: t
                .probe
                .as_deref()
                .is_some_and(|p| find_in_path(p).is_some()),
            spec: t.clone(),
        })
        .collect()
}

/// Probe one dependency (spec 06 §2). Graphics/network are `Deferred` —
/// they can only be checked in context (TUI start / first request).
pub fn probe_dep(spec: &DepSpec, runner: &dyn CommandRunner) -> DepStatus {
    match spec.kind {
        DepKind::Binary => {
            let names = std::iter::once(spec.probe.as_deref())
                .flatten()
                .chain(spec.probe_alt.iter().map(String::as_str));
            for name in names {
                if find_in_path(name).is_some() {
                    return DepStatus::Present;
                }
            }
            DepStatus::Missing
        }
        DepKind::Package => match &spec.probe {
            Some(pkg) => runner
                .run(&Cmd::new("pacman").args(["-Qq", pkg]))
                .map(|o| {
                    if o.ok() {
                        DepStatus::Present
                    } else {
                        DepStatus::Missing
                    }
                })
                .unwrap_or(DepStatus::Missing),
            None => DepStatus::Missing,
        },
        DepKind::Service => match &spec.probe {
            Some(unit) => runner
                .run(&Cmd::new("systemctl").args(["--user", "is-active", unit]))
                .map(|o| {
                    if o.ok() {
                        DepStatus::Present
                    } else {
                        DepStatus::Missing
                    }
                })
                .unwrap_or(DepStatus::Missing),
            None => DepStatus::Missing,
        },
        DepKind::TerminalGraphics | DepKind::Network => DepStatus::Deferred,
    }
}

/// Build the user-facing report, deriving the exact install command
/// (spec 06 §3): official → pacman; AUR → yay/paru if present, else the
/// manual steps note (FR11.7).
pub fn report(spec: &DepSpec, status: DepStatus) -> DepReport {
    let install = spec.package.as_ref().map(|pkg| {
        if spec.repo.as_deref() == Some("aur") {
            if find_in_path("yay").is_some() {
                format!("yay -S {pkg}")
            } else if find_in_path("paru").is_some() {
                format!("paru -S {pkg}")
            } else {
                format!("git clone https://aur.archlinux.org/{pkg}.git && cd {pkg} && makepkg -si")
            }
        } else {
            format!("sudo pacman -S {pkg}")
        }
    });
    DepReport {
        id: spec.id.clone(),
        kind: spec.kind,
        status,
        reason: spec.reason.clone(),
        install,
        fallback: spec.fallback.clone(),
        unlocks: spec.unlocks.clone(),
    }
}

/// Probe everything — the `doctor --deps` / first-run-wizard table.
pub fn probe_all(reg: &Registry, runner: &dyn CommandRunner) -> Vec<DepReport> {
    reg.deps
        .iter()
        .map(|d| report(d, probe_dep(d, runner)))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cmd::StubRunner;

    #[test]
    fn builtin_registry_parses_and_is_complete() {
        let reg = Registry::builtin().unwrap();
        // The shipped data file is itself under test: every dep must carry
        // user-language copy, per principle 7.
        assert!(reg.deps.len() >= 6, "expected the seed deps");
        for d in &reg.deps {
            assert!(!d.reason.is_empty(), "dep `{}` needs a reason", d.id);
            assert!(!d.fallback.is_empty(), "dep `{}` needs a fallback", d.id);
        }
        let aether = reg.tools.iter().find(|t| t.id == "aether").unwrap();
        assert_eq!(aether.interop.as_deref(), Some("themes"));
        assert!(!aether.actions.is_empty());
        assert_eq!(aether.actions[0].context.as_deref(), Some("wallpapers"));
    }

    #[test]
    fn binary_probe_uses_alternates() {
        let spec = DepSpec {
            id: "aur-helper".into(),
            kind: DepKind::Binary,
            probe: Some("definitely-not-a-real-binary".into()),
            probe_alt: vec!["sh".into()], // always present
            package: None,
            repo: None,
            reason: "x".into(),
            unlocks: vec![],
            fallback: "x".into(),
        };
        assert_eq!(probe_dep(&spec, &StubRunner::default()), DepStatus::Present);
    }

    #[test]
    fn package_probe_asks_pacman() {
        let spec = DepSpec {
            id: "wl-copy".into(),
            kind: DepKind::Package,
            probe: Some("wl-clipboard".into()),
            probe_alt: vec![],
            package: Some("wl-clipboard".into()),
            repo: Some("official".into()),
            reason: "x".into(),
            unlocks: vec![],
            fallback: "x".into(),
        };
        let stub = StubRunner::default().with_ok("pacman -Qq wl-clipboard", "wl-clipboard\n");
        assert_eq!(probe_dep(&spec, &stub), DepStatus::Present);
        let stub = StubRunner::default().with_fail("pacman -Qq wl-clipboard");
        assert_eq!(probe_dep(&spec, &stub), DepStatus::Missing);
    }

    #[test]
    fn deferred_kinds_do_not_probe() {
        let spec = DepSpec {
            id: "network".into(),
            kind: DepKind::Network,
            probe: None,
            probe_alt: vec![],
            package: None,
            repo: None,
            reason: "x".into(),
            unlocks: vec![],
            fallback: "x".into(),
        };
        // StubRunner would error on any call — Deferred proves none happened.
        assert_eq!(
            probe_dep(&spec, &StubRunner::default()),
            DepStatus::Deferred
        );
    }

    #[test]
    fn install_command_derivation() {
        let mut spec = DepSpec {
            id: "x".into(),
            kind: DepKind::Binary,
            probe: None,
            probe_alt: vec![],
            package: Some("swww".into()),
            repo: Some("official".into()),
            reason: "r".into(),
            unlocks: vec![],
            fallback: "f".into(),
        };
        let r = report(&spec, DepStatus::Missing);
        assert_eq!(r.install.as_deref(), Some("sudo pacman -S swww"));
        spec.repo = Some("aur".into());
        let r = report(&spec, DepStatus::Missing);
        let cmd = r.install.unwrap();
        assert!(
            cmd.contains("-S swww") || cmd.contains("makepkg -si"),
            "AUR guidance should use a helper or manual steps: {cmd}"
        );
    }

    #[test]
    fn user_overlay_wins_merge() {
        let base = Registry::builtin().unwrap();
        let overlay = Registry::parse(
            "[deps.git]\nkind = \"binary\"\nprobe = \"git\"\nreason = \"changed\"\nfallback = \"f\"\n",
        )
        .unwrap();
        let merged = base.merge(overlay);
        assert_eq!(merged.dep("git").unwrap().reason, "changed");
        assert_eq!(
            merged.deps.iter().filter(|d| d.id == "git").count(),
            1,
            "no duplicates after merge"
        );
    }
}
