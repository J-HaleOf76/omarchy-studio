//! Doctor screen (spec 07, roadmap 0.5.6): the health view. System facts,
//! capability probes, and the update-survival picture (hooks + drift) in one
//! place, with install/remove for the lifecycle hooks a keypress away.
//!
//! Read-only except the two hook actions, which the App performs (IO stays
//! out of screens); `reload` re-probes everything.

use std::path::PathBuf;

use ratatui::crossterm::event::{KeyCode, KeyEvent};
use ratatui::layout::Rect;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Wrap};
use ratatui::Frame;

use studio_core::cmd::CommandRunner;
use studio_core::hooks::{self, HookState};
use studio_core::omarchy::{Capabilities, OmarchyPaths};
use studio_core::snapshot::SnapshotStore;

use crate::tui::theme::Skin;

pub enum DoctorAction {
    None,
    /// Install (or repair) both lifecycle hooks.
    InstallHooks,
    /// Remove both lifecycle hooks.
    RemoveHooks,
    /// Re-run every probe.
    Refresh,
}

pub struct DoctorScreen {
    caps: Capabilities,
    theme: String,
    system_path: String,
    hooks: Vec<(&'static str, HookState)>,
    drift: Vec<PathBuf>,
    clobbers: Vec<PathBuf>,
    store_ok: bool,
}

impl DoctorScreen {
    pub fn load(paths: &OmarchyPaths, runner: &dyn CommandRunner) -> Self {
        let caps = Capabilities::probe(paths, runner);
        let theme = paths
            .current_theme_name()
            .unwrap_or_else(|_| "unknown".into());
        let hooks = hooks::status(paths);
        let (drift, clobbers, store_ok) = match SnapshotStore::open_or_init(
            studio_core::studio_state_dir().join("history"),
            Box::new(studio_core::cmd::RealRunner),
        ) {
            Ok(store) => {
                let r = hooks::update_report(&store);
                (r.drift, r.clobbers, true)
            }
            Err(_) => (Vec::new(), Vec::new(), false),
        };
        Self {
            caps,
            theme,
            system_path: paths.system.display().to_string(),
            hooks,
            drift,
            clobbers,
            store_ok,
        }
    }

    pub fn reload(&mut self, paths: &OmarchyPaths, runner: &dyn CommandRunner) {
        *self = Self::load(paths, runner);
    }

    pub fn handle(&mut self, key: KeyEvent) -> DoctorAction {
        match key.code {
            KeyCode::Char('i') => DoctorAction::InstallHooks,
            KeyCode::Char('x') if self.hooks.iter().any(|(_, s)| *s != HookState::Missing) => {
                DoctorAction::RemoveHooks
            }
            KeyCode::Char('r') => DoctorAction::Refresh,
            _ => DoctorAction::None,
        }
    }

    pub fn render(&self, f: &mut Frame, area: Rect, skin: &Skin) {
        let good = |b: bool| if b { "✓" } else { "✗" };
        let style_of = |b: bool, skin: &Skin| if b { skin.ok() } else { skin.error() };
        let mut lines: Vec<Line> = Vec::new();

        lines.push(Line::from(Span::styled("  System", skin.dim())));
        let fact = |label: &str, value: String, skin: &Skin| {
            Line::from(vec![
                Span::styled(format!("    {label:<14}"), skin.dim()),
                Span::styled(value, skin.body()),
            ])
        };
        lines.push(fact(
            "omarchy",
            format!("{} at {}", self.caps.omarchy_version, self.system_path),
            skin,
        ));
        let hypr = if self.caps.hyprland_version.is_empty() {
            "unavailable (not in a Hyprland session?)".into()
        } else {
            self.caps.hyprland_version.clone()
        };
        lines.push(fact("hyprland", hypr, skin));
        lines.push(fact("theme", self.theme.clone(), skin));
        lines.push(Line::from(""));

        lines.push(Line::from(Span::styled("  Capabilities", skin.dim())));
        let caps = [
            (
                self.caps.has_configerrors,
                "config verification (hyprctl configerrors)",
            ),
            (self.caps.has_templates, "theme template engine"),
            (self.caps.has_hooks, "lifecycle hooks (omarchy-hook)"),
            (self.caps.tui_float_rule, "stock floating-TUI window rule"),
            (self.caps.video_wallpapers, "video wallpapers (mpvpaper)"),
        ];
        for (ok, label) in caps {
            lines.push(Line::from(vec![
                Span::styled(format!("    {} ", good(ok)), style_of(ok, skin)),
                Span::styled(label, skin.body()),
            ]));
        }
        lines.push(Line::from(""));

        lines.push(Line::from(Span::styled("  Update survival", skin.dim())));
        for (event, state) in &self.hooks {
            let (mark, note, ok) = match state {
                HookState::Installed => ("✓", "installed".to_string(), true),
                HookState::Missing => ("✗", "not installed — i to install".to_string(), false),
                HookState::Modified => (
                    "✗",
                    "MODIFIED, not our script — i repairs".to_string(),
                    false,
                ),
            };
            lines.push(Line::from(vec![
                Span::styled(format!("    {mark} "), style_of(ok, skin)),
                Span::styled(format!("{event:<14} hook  "), skin.body()),
                Span::styled(note, if ok { skin.dim() } else { skin.warn() }),
            ]));
        }
        if !self.store_ok {
            lines.push(Line::from(Span::styled(
                "    ✗ snapshot store unavailable — drift unknown",
                skin.error(),
            )));
        } else if self.drift.is_empty() && self.clobbers.is_empty() {
            lines.push(Line::from(vec![
                Span::styled("    ✓ ", skin.ok()),
                Span::styled("tracked files match the last snapshot", skin.body()),
            ]));
        } else {
            for f in &self.drift {
                lines.push(Line::from(vec![
                    Span::styled("    ~ ", skin.warn()),
                    Span::styled(format!("{}", f.display()), skin.body()),
                    Span::styled("  hand-edited since our snapshot", skin.warn()),
                ]));
            }
            for f in &self.clobbers {
                lines.push(Line::from(vec![
                    Span::styled("    ! ", skin.error()),
                    Span::styled(format!("{}", f.display()), skin.body()),
                    Span::styled("  replaced by an update (.bak left)", skin.error()),
                ]));
            }
            lines.push(Line::from(Span::styled(
                "      review from Snapshots, or `omarchy-studio snapshot restore <id>`",
                skin.dim(),
            )));
        }

        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            "  i install/repair hooks   x remove hooks   r re-run checks",
            skin.dim(),
        )));

        f.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), area);
    }
}
