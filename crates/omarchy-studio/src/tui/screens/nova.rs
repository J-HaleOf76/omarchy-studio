//! Nova screen — configure the Nova launcher (visual mode, backdrop,
//! animation toggles), install/remove its launch keybind, and launch it.
//!
//! Every edit is local until `s` applies (snapshot → atomic save). Nova reads
//! the file at launch, so a save simply takes effect the next time it opens.
//! Providers are CLI-only (`omarchy-studio nova providers …`) — reordering a
//! search blend is not a ←/→ kind of edit.

use ratatui::crossterm::event::{KeyCode, KeyEvent};
use ratatui::layout::Rect;
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::Frame;

use studio_core::modules::keybinds::render_chord;
use studio_core::modules::nova::{self, Nova, MODES};
use studio_core::omarchy::OmarchyPaths;

use crate::tui::theme::Skin;

pub enum NovaAction {
    None,
    /// Snapshot + save nova.json.
    Apply,
    /// Install the launch keybind if absent, remove it if present.
    ToggleBind,
    /// Spawn Nova now (detached).
    Launch,
    Refresh,
}

/// The editable rows, top to bottom.
const ROWS: usize = 9;

pub struct NovaScreen {
    model: Nova,
    /// Installed launch bind, pre-rendered ("SUPER+SPACE"), if any.
    bind: Option<String>,
    launch: Option<String>,
    row: usize,
    dirty: bool,
}

impl NovaScreen {
    pub fn load(paths: &OmarchyPaths) -> Self {
        Self {
            model: Nova::load(paths),
            bind: nova::keybind(paths).map(|b| render_chord(b.modmask, &b.key)),
            launch: nova::launch_command(paths),
            row: 0,
            dirty: false,
        }
    }

    pub fn reload(&mut self, paths: &OmarchyPaths) {
        let row = self.row;
        *self = Self::load(paths);
        self.row = row.min(ROWS - 1);
    }

    pub fn model(&self) -> &Nova {
        &self.model
    }

    /// Called by the app after a successful save.
    pub fn saved(&mut self) {
        self.dirty = false;
    }

    pub fn launch_exec(&self) -> Option<&str> {
        self.launch.as_deref()
    }

    /// Step the value under the cursor. `dir` is -1 (←) or +1 (→/space).
    fn step(&mut self, dir: i64) {
        let cfg = &mut self.model.cfg;
        match self.row {
            0 => {
                let cur = MODES.iter().position(|m| *m == cfg.mode).unwrap_or(0) as i64;
                let next = (cur + dir).rem_euclid(MODES.len() as i64) as usize;
                cfg.mode = MODES[next].into();
            }
            1 => cfg.blur = !cfg.blur,
            2 => {
                cfg.backdrop_opacity = (cfg.backdrop_opacity + dir as f64 * 0.05).clamp(0.5, 1.0);
                // keep the JSON tidy (0.8500000000000001 → 0.85)
                cfg.backdrop_opacity = (cfg.backdrop_opacity * 1000.0).round() / 1000.0;
            }
            3 => cfg.limit = (cfg.limit + dir * 5).clamp(10, 100),
            4 => cfg.anim.stagger = !cfg.anim.stagger,
            5 => cfg.anim.stagger_ms = (cfg.anim.stagger_ms + dir * 4).clamp(0, 120),
            6 => cfg.anim.living_background = !cfg.anim.living_background,
            7 => cfg.anim.spring = !cfg.anim.spring,
            8 => cfg.anim.micro = !cfg.anim.micro,
            _ => return,
        }
        self.dirty = true;
    }

    pub fn handle(&mut self, key: KeyEvent) -> NovaAction {
        match key.code {
            KeyCode::Up => self.row = self.row.saturating_sub(1),
            KeyCode::Down => self.row = (self.row + 1).min(ROWS - 1),
            KeyCode::Left => self.step(-1),
            KeyCode::Right | KeyCode::Char(' ') => self.step(1),
            KeyCode::Char('s') | KeyCode::Enter => return NovaAction::Apply,
            KeyCode::Char('b') => return NovaAction::ToggleBind,
            KeyCode::Char('l') => return NovaAction::Launch,
            KeyCode::Char('r') => return NovaAction::Refresh,
            _ => {}
        }
        NovaAction::None
    }

    pub fn hint(&self) -> String {
        let bind = if self.bind.is_some() {
            "b unbind"
        } else {
            "b keybind"
        };
        format!("↑↓ row · ←→ adjust · s save · {bind} · l launch · r re-read")
    }

    pub fn render(&self, f: &mut Frame, area: Rect, skin: &Skin) {
        let cfg = &self.model.cfg;
        let onoff = |b: bool| if b { "on" } else { "off" }.to_string();
        let rows: [(&str, String); ROWS] = [
            ("visual mode", cfg.mode.clone()),
            ("backdrop blur", onoff(cfg.blur)),
            ("backdrop opacity", format!("{:.3}", cfg.backdrop_opacity)),
            ("result limit", cfg.limit.to_string()),
            ("anim · stagger", onoff(cfg.anim.stagger)),
            ("anim · stagger ms", cfg.anim.stagger_ms.to_string()),
            (
                "anim · living background",
                onoff(cfg.anim.living_background),
            ),
            ("anim · spring physics", onoff(cfg.anim.spring)),
            ("anim · micro (hover/tilt)", onoff(cfg.anim.micro)),
        ];

        let mut lines: Vec<Line> = Vec::new();
        let state = if !self.model.existed && !self.dirty {
            "using Nova's bundled defaults (no nova.json yet — s creates it)"
        } else if self.dirty {
            "edited — s saves (Nova reads it on next launch)"
        } else {
            "saved"
        };
        lines.push(Line::from(vec![
            Span::styled("  Nova launcher", skin.dim()),
            Span::styled(
                format!("   {state}"),
                if self.dirty { skin.warn() } else { skin.dim() },
            ),
        ]));
        lines.push(Line::from(""));

        for (i, (label, value)) in rows.iter().enumerate() {
            let on = i == self.row;
            lines.push(Line::from(vec![
                Span::styled(if on { "  ▸ " } else { "    " }, skin.accent_bold()),
                Span::styled(format!("{label:<26}"), skin.body()),
                Span::styled("‹ ", skin.accent_bold()),
                Span::styled(
                    format!("{value:>13}"),
                    if on { skin.accent_bold() } else { skin.body() },
                ),
                Span::styled(" ›", skin.accent_bold()),
            ]));
        }

        lines.push(Line::from(""));
        lines.push(Line::from(vec![
            Span::styled("    providers                 ", skin.body()),
            Span::styled(cfg.providers.join(", "), skin.dim()),
        ]));
        lines.push(Line::from(Span::styled(
            "    (edit with: omarchy-studio nova providers add|remove <name>)",
            skin.dim(),
        )));
        lines.push(Line::from(""));

        match &self.bind {
            Some(chord) => lines.push(Line::from(vec![
                Span::styled("  keybind   ", skin.dim()),
                Span::styled(chord.clone(), skin.accent_bold()),
                Span::styled("  launches Nova (b removes it)", skin.dim()),
            ])),
            None => lines.push(Line::from(vec![
                Span::styled("  keybind   ", skin.dim()),
                Span::styled("none", skin.body()),
                Span::styled(
                    "  — b binds SUPER+SPACE (overrides Walker there)",
                    skin.dim(),
                ),
            ])),
        }
        match &self.launch {
            Some(cmd) => lines.push(Line::from(vec![
                Span::styled("  launch    ", skin.dim()),
                Span::styled(cmd.clone(), skin.body()),
            ])),
            None => lines.push(Line::from(Span::styled(
                "  launch    no Nova checkout found at ~/Projects/nova",
                skin.warn(),
            ))),
        }
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            "  Theme follows the active Omarchy palette automatically.",
            skin.dim(),
        )));

        f.render_widget(Paragraph::new(lines), area);
    }
}
