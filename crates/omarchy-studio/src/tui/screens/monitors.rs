//! Monitors screen (roadmap 0.8.4).
//!
//! Lists the displays Hyprland reports, lets you nudge scale, flip a monitor
//! on/off, and identify which physical panel is which. Save persists the layout
//! to `monitors.conf` (managed block + hotplug fallback) through the snapshot
//! pipeline, so a bad layout is one `snapshot undo` away.

use ratatui::crossterm::event::{KeyCode, KeyEvent};
use ratatui::layout::{Constraint, Direction, Layout as LLayout, Rect};
use ratatui::text::{Line, Span};
use ratatui::widgets::{List, ListItem, Paragraph};
use ratatui::Frame;

use studio_core::cmd::RealRunner;
use studio_core::modules::monitors::{self as mon, Layout, Monitor};
use studio_core::omarchy::OmarchyPaths;

use crate::tui::theme::Skin;

pub enum MonitorsAction {
    None,
    /// Flash each monitor's name on-screen.
    Identify,
    /// Persist the edited layout (App snapshots + reloads).
    Save(Layout),
}

pub struct MonitorsScreen {
    live: Vec<Monitor>,
    layout: Layout,
    cursor: usize,
    dirty: bool,
    error: Option<String>,
}

impl MonitorsScreen {
    pub fn load(_paths: &OmarchyPaths) -> Self {
        let (live, error) = match mon::load(&RealRunner) {
            Ok(m) => (m, None),
            Err(e) => (Vec::new(), Some(friendly(&e))),
        };
        let layout = Layout::from_monitors(&live);
        Self {
            live,
            layout,
            cursor: 0,
            dirty: false,
            error,
        }
    }

    pub fn reload(&mut self, paths: &OmarchyPaths) {
        let keep = self.cursor;
        *self = Self::load(paths);
        self.cursor = keep.min(self.layout.monitors.len().saturating_sub(1));
    }

    pub fn hint(&self) -> &'static str {
        "↑↓ move · +/- scale · d disable · i identify · s save"
    }

    fn nudge_scale(&mut self, delta: f64) {
        let Some(s) = self.layout.monitors.get_mut(self.cursor) else {
            return;
        };
        let current: f64 = s.scale.parse().unwrap_or(1.0);
        let next = (current + delta).clamp(0.5, 3.0);
        s.scale = mon::fmt_scale((next * 100.0).round() / 100.0);
        self.dirty = true;
    }

    pub fn handle(&mut self, key: KeyEvent) -> MonitorsAction {
        let n = self.layout.monitors.len();
        match key.code {
            KeyCode::Down if n > 0 => {
                self.cursor = (self.cursor + 1).min(n - 1)
            }
            KeyCode::Up => self.cursor = self.cursor.saturating_sub(1),
            KeyCode::Char('+') | KeyCode::Char('=') => self.nudge_scale(0.25),
            KeyCode::Char('-') | KeyCode::Char('_') => self.nudge_scale(-0.25),
            KeyCode::Char('d') => {
                if let Some(s) = self.layout.monitors.get_mut(self.cursor) {
                    s.disabled = !s.disabled;
                    self.dirty = true;
                }
            }
            KeyCode::Char('i') => return MonitorsAction::Identify,
            KeyCode::Char('s') if self.dirty => return MonitorsAction::Save(self.layout.clone()),
            _ => {}
        }
        MonitorsAction::None
    }

    pub fn render(&self, f: &mut Frame, area: Rect, skin: &Skin) {
        if let Some(msg) = &self.error {
            f.render_widget(
                Paragraph::new(vec![
                    Line::from(Span::styled("Can't read your monitors", skin.accent_bold())),
                    Line::from(""),
                    Line::from(Span::styled(msg.clone(), skin.body())),
                    Line::from(Span::styled("(are you in a Hyprland session?)", skin.dim())),
                ]),
                area,
            );
            return;
        }
        let rows = LLayout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Min(1), Constraint::Length(2)])
            .split(area);

        let mut items: Vec<ListItem> = Vec::new();
        for (i, s) in self.layout.monitors.iter().enumerate() {
            let on = i == self.cursor;
            let live = self.live.iter().find(|m| m.name == s.name);
            let laptop = live.map(Monitor::is_laptop).unwrap_or(false);
            let name_style = if on { skin.selection() } else { skin.body() };
            let marker = if on { "▸ " } else { "  " };
            let state = if s.disabled { " · OFF" } else { "" };
            let tag = if laptop { "  laptop" } else { "" };
            items.push(ListItem::new(Line::from(vec![
                Span::styled(format!("{marker}{:<11}", s.name), name_style),
                Span::styled(
                    format!("{}  @ {}x{}  scale {}{state}", s.mode, s.x, s.y, s.scale),
                    if s.disabled { skin.dim() } else { skin.body() },
                ),
                Span::styled(tag, skin.dim()),
            ])));
            if let Some(m) = live {
                let (ew, eh) = effective_of(m, &s.scale);
                if (ew, eh) != (m.width, m.height) {
                    items.push(ListItem::new(Line::from(Span::styled(
                        format!("             effective {ew}x{eh}"),
                        skin.dim(),
                    ))));
                }
            }
        }
        f.render_widget(List::new(items), rows[0]);

        let dirty = if self.dirty {
            Span::styled("  ·  unsaved — s to apply & reload", skin.warn())
        } else {
            Span::styled("", skin.dim())
        };
        let footer = Paragraph::new(vec![
            Line::from(vec![Span::styled("Displays", skin.dim()), dirty]),
            Line::from(Span::styled(self.hint(), skin.dim())),
        ]);
        f.render_widget(footer, rows[1]);
    }
}

/// Effective size using the edited scale string (falls back to the live scale).
fn effective_of(m: &Monitor, scale_str: &str) -> (u32, u32) {
    let scale = scale_str.parse().unwrap_or(m.scale);
    mon::effective_size(m.width, m.height, scale, m.transform)
}

fn friendly(e: &studio_core::StudioError) -> String {
    match e {
        studio_core::StudioError::External { detail, .. } => detail.clone(),
        other => format!("{other:?}"),
    }
}
