//! Integrations screen (spec 06 §5, roadmap 0.6.6).
//!
//! Two honest tables from the same registry that powers `doctor --deps`:
//!
//! - **Dependencies** — what Studio itself leans on: status, plain-language
//!   purpose, and the exact install command when missing (never a silent
//!   failure, spec 06 §3).
//! - **Companion tools** — detected neighbors (Aether, Omarchist, matugen,
//!   hyprmon…): homepage, license, and launchable actions. Studio never
//!   *requires* these; `interop = "themes"` tools already show up in the
//!   Themes browser for free because their output is standard theme dirs.
//!
//! Enter launches the selected tool (detached); missing entries show how to
//! install instead — the M11 "we don't do it, but you're two keys away".

use ratatui::crossterm::event::{KeyCode, KeyEvent};
use ratatui::layout::Rect;
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::Frame;

use studio_core::cmd::CommandRunner;
use studio_core::deps::{probe_all, probe_tools, DepReport, DepStatus, Registry, ToolReport};

use crate::tui::theme::Skin;

pub enum IntegrationsAction {
    None,
    /// Spawn this exec (label for the toast); `terminal` = TUI tool that
    /// needs a floating terminal rather than a detached process.
    Launch {
        exec: String,
        label: String,
        terminal: bool,
    },
    Refresh,
}

/// One selectable row: a dep or a tool.
enum Row {
    Dep,
    Tool(usize),
}

pub struct IntegrationsScreen {
    deps: Vec<DepReport>,
    tools: Vec<ToolReport>,
    selected: usize,
}

impl IntegrationsScreen {
    pub fn load(runner: &dyn CommandRunner) -> Self {
        let (deps, tools) = match Registry::load() {
            Ok(reg) => (probe_all(&reg, runner), probe_tools(&reg)),
            Err(_) => (Vec::new(), Vec::new()),
        };
        Self {
            deps,
            tools,
            selected: 0,
        }
    }

    pub fn reload(&mut self, runner: &dyn CommandRunner) {
        let keep = self.selected;
        *self = Self::load(runner);
        self.selected = keep.min(self.rows().len().saturating_sub(1));
    }

    /// Actions with a `context` for a given screen, from present tools only —
    /// e.g. the wallpaper browser's "Open in Aether" (spec 06 §5).
    pub fn context_actions(&self, context: &str) -> Vec<(String, String, bool)> {
        self.tools
            .iter()
            .filter(|t| t.present)
            .flat_map(|t| &t.spec.actions)
            .filter(|a| a.context.as_deref() == Some(context))
            .map(|a| (a.label.clone(), a.exec.clone(), a.terminal))
            .collect()
    }

    fn rows(&self) -> Vec<Row> {
        (0..self.deps.len())
            .map(|_| Row::Dep)
            .chain((0..self.tools.len()).map(Row::Tool))
            .collect()
    }

    pub fn handle(&mut self, key: KeyEvent) -> IntegrationsAction {
        let n = self.rows().len();
        match key.code {
            KeyCode::Down if n > 0 => {
                self.selected = (self.selected + 1).min(n - 1)
            }
            KeyCode::Up => self.selected = self.selected.saturating_sub(1),
            KeyCode::Home => self.selected = 0,
            KeyCode::End if n > 0 => self.selected = n - 1,
            KeyCode::Char('r') => return IntegrationsAction::Refresh,
            KeyCode::Enter => {
                if let Some(Row::Tool(i)) = self.rows().get(self.selected) {
                    let t = &self.tools[*i];
                    if t.present {
                        if let Some(a) = t.spec.actions.first() {
                            return IntegrationsAction::Launch {
                                exec: a.exec.clone(),
                                label: t.spec.id.clone(),
                                terminal: a.terminal,
                            };
                        }
                    }
                }
            }
            _ => {}
        }
        IntegrationsAction::None
    }

    pub fn render(&self, f: &mut Frame, area: Rect, skin: &Skin) {
        let mut lines: Vec<Line> = Vec::new();
        let cursor =
            |on: bool, skin: &Skin| Span::styled(if on { "▸ " } else { "  " }, skin.accent_bold());

        lines.push(Line::from(Span::styled("  Dependencies", skin.dim())));
        for (row_i, d) in self.deps.iter().enumerate() {
            let on = self.selected == row_i;
            let (mark, mark_style) = match &d.status {
                DepStatus::Present => ("✓", skin.ok()),
                DepStatus::Missing => ("✗", skin.error()),
                DepStatus::Degraded(_) => ("!", skin.warn()),
                DepStatus::Deferred => ("·", skin.dim()),
            };
            lines.push(Line::from(vec![
                cursor(on, skin),
                Span::styled(format!("{mark} "), mark_style),
                Span::styled(
                    format!("{:<18}", d.id),
                    if on { skin.accent_bold() } else { skin.body() },
                ),
                Span::styled(d.reason.clone(), skin.dim()),
            ]));
            if on {
                lines.push(Line::from(Span::styled(
                    match (&d.status, &d.install) {
                        (DepStatus::Missing, Some(cmd)) => format!("      install: {cmd}"),
                        (DepStatus::Missing, None) => format!("      {}", d.fallback),
                        (DepStatus::Degraded(why), _) => format!("      {why}"),
                        _ => format!("      unlocks: {}", d.unlocks.join(", ")),
                    },
                    skin.dim(),
                )));
            }
        }

        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            "  Companion tools — detected, never required",
            skin.dim(),
        )));
        for (i, t) in self.tools.iter().enumerate() {
            let on = self.selected == self.deps.len() + i;
            let (mark, mark_style) = if t.present {
                ("✓", skin.ok())
            } else {
                ("·", skin.dim())
            };
            let name_style = if on {
                skin.accent_bold()
            } else if t.present {
                skin.body()
            } else {
                skin.dim()
            };
            let mut spans = vec![
                cursor(on, skin),
                Span::styled(format!("{mark} "), mark_style),
                Span::styled(format!("{:<18}", t.spec.id), name_style),
            ];
            if let Some(interop) = &t.spec.interop {
                spans.push(Span::styled(format!("[{interop}] "), skin.warn()));
            }
            if let Some(home) = &t.spec.homepage {
                spans.push(Span::styled(home.clone(), skin.dim()));
            }
            lines.push(Line::from(spans));
            if on {
                let detail = if t.present {
                    match t.spec.actions.first() {
                        Some(a) => format!("      enter → {}", a.label),
                        None => "      detected — its themes already show in Themes".into(),
                    }
                } else {
                    match &t.spec.install {
                        Some(cmd) => format!("      install: {cmd}"),
                        None => "      not detected".into(),
                    }
                };
                lines.push(Line::from(Span::styled(detail, skin.dim())));
            }
        }

        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            format!(
                "  {} of {} tools detected · enter launch · r re-detect",
                self.tools.iter().filter(|t| t.present).count(),
                self.tools.len()
            ),
            skin.dim(),
        )));
        f.render_widget(Paragraph::new(lines), area);
    }
}
