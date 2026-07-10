//! Notifications screen (spec 05 §5, roadmap 0.5.2).
//!
//! A schema-driven form over mako's notification behavior. Unlike Look & Feel
//! there's no live `hyprctl` preview — mako changes go through a template
//! regenerate — so edits accumulate in-memory and Save writes + reloads. The
//! screen also toggles Do Not Disturb (live) and fires sample notifications so
//! the user can see their settings land. Per-app rules are shown here; the CLI
//! `notif rule add` is the full builder.

use ratatui::crossterm::event::{KeyCode, KeyEvent};
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, List, ListItem, Paragraph, Wrap};
use ratatui::Frame;

use studio_core::modules::mako::{groups, Kind, MakoBehavior, Setting, SAMPLE_URGENCIES, SETTINGS};
use studio_core::omarchy::OmarchyPaths;

use crate::tui::theme::Skin;

/// What the screen wants the App to do after a key.
pub enum NotifAction {
    None,
    /// Persist the behavior (App snapshots first).
    Save,
    /// Flip Do Not Disturb (App runs makoctl and reports the new state).
    ToggleDnd,
    /// Fire a sample notification at this urgency.
    Test(String),
    /// Flash the SwayOSD popup (no-op bump) so the user can preview its look.
    OsdTest,
}

pub struct NotificationsScreen {
    model: MakoBehavior,
    selected: usize,
    scroll: usize,
    pub dirty: bool,
    /// Cached DND state (None = not yet known), set by the App.
    dnd: Option<bool>,
    /// Live-test urgency picker: Some(index) while open.
    test_picker: Option<usize>,
}

impl NotificationsScreen {
    pub fn load(paths: &OmarchyPaths) -> Self {
        Self {
            model: MakoBehavior::load(paths),
            selected: 0,
            scroll: 0,
            dirty: false,
            dnd: None,
            test_picker: None,
        }
    }

    pub fn reload(&mut self, paths: &OmarchyPaths) {
        let keep = self.selected;
        let dnd = self.dnd;
        *self = Self::load(paths);
        self.selected = keep.min(SETTINGS.len().saturating_sub(1));
        self.dnd = dnd;
    }

    /// The App tells us the current DND state (read via makoctl).
    pub fn set_dnd(&mut self, on: Option<bool>) {
        self.dnd = on;
    }

    pub fn is_modal(&self) -> bool {
        self.test_picker.is_some()
    }

    pub fn model(&self) -> &MakoBehavior {
        &self.model
    }

    fn cur(&self) -> &Setting {
        &SETTINGS[self.selected]
    }

    pub fn handle(&mut self, key: KeyEvent) -> NotifAction {
        if self.test_picker.is_some() {
            return self.handle_test_picker(key);
        }
        let n = SETTINGS.len();
        match key.code {
            KeyCode::Down => self.selected = (self.selected + 1).min(n - 1),
            KeyCode::Up => self.selected = self.selected.saturating_sub(1),
            KeyCode::Home => self.selected = 0,
            KeyCode::End => self.selected = n - 1,
            KeyCode::Right | KeyCode::Char('+') | KeyCode::Char('=') => {
                self.nudge(1)
            }
            KeyCode::Left | KeyCode::Char('-') => self.nudge(-1),
            KeyCode::Enter => self.nudge(1),
            KeyCode::Char('r') => {
                let key = self.cur().key;
                self.model.clear(key);
                self.dirty = true;
            }
            KeyCode::Char('d') => return NotifAction::ToggleDnd,
            KeyCode::Char('o') => return NotifAction::OsdTest,
            KeyCode::Char('t') => self.test_picker = Some(1), // default: normal
            KeyCode::Char('s') if self.dirty => return NotifAction::Save,
            _ => {}
        }
        NotifAction::None
    }

    /// Cycle the selected setting by one step. Text settings aren't adjustable
    /// inline (edit them via the CLI) — a no-op here.
    fn nudge(&mut self, dir: i64) {
        let setting = *self.cur();
        let current = self.model.value(setting.key);
        let next = match setting.kind {
            Kind::Bool => {
                let on = matches!(current.as_str(), "true" | "yes" | "on" | "1");
                if on { "false" } else { "true" }.to_string()
            }
            Kind::Enum(opts) => {
                let i = opts.iter().position(|o| *o == current).unwrap_or(0) as i64;
                let j = (i + dir).rem_euclid(opts.len() as i64) as usize;
                opts[j].to_string()
            }
            Kind::Int { min, max } => {
                let v: i64 = current.parse().unwrap_or(min);
                let step = int_step(setting.key);
                (v + dir * step).clamp(min, max).to_string()
            }
            Kind::Text => return,
        };
        if self.model.set(setting.key, &next).is_ok() {
            self.dirty = true;
        }
    }

    fn handle_test_picker(&mut self, key: KeyEvent) -> NotifAction {
        let Some(sel) = self.test_picker.as_mut() else {
            return NotifAction::None;
        };
        match key.code {
            KeyCode::Esc => self.test_picker = None,
            KeyCode::Down => *sel = (*sel + 1).min(SAMPLE_URGENCIES.len() - 1),
            KeyCode::Up => *sel = sel.saturating_sub(1),
            KeyCode::Enter => {
                let urgency = SAMPLE_URGENCIES[*sel].to_string();
                self.test_picker = None;
                return NotifAction::Test(urgency);
            }
            _ => {}
        }
        NotifAction::None
    }

    pub fn render(&mut self, f: &mut Frame, area: Rect, skin: &Skin) {
        let cols = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(58), Constraint::Percentage(42)])
            .split(area);
        let left = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Min(1), Constraint::Length(3)])
            .split(cols[0]);

        // settings list, grouped
        let mut items: Vec<ListItem> = Vec::new();
        let mut sel_row = 0usize;
        let mut row = 0usize;
        for group in groups() {
            items.push(ListItem::new(Line::from(Span::styled(
                format!("  {group}"),
                skin.dim(),
            ))));
            row += 1;
            for (i, s) in SETTINGS.iter().enumerate() {
                if s.group != group {
                    continue;
                }
                if i == self.selected {
                    sel_row = row;
                }
                items.push(self.setting_row(i, s, skin));
                row += 1;
            }
        }
        let height = left[0].height as usize;
        if sel_row < self.scroll {
            self.scroll = sel_row;
        } else if sel_row >= self.scroll + height {
            self.scroll = sel_row + 1 - height;
        }
        let visible: Vec<ListItem> = items.into_iter().skip(self.scroll).take(height).collect();
        f.render_widget(List::new(visible), left[0]);
        self.render_footer(f, left[1], skin);
        self.render_side(f, cols[1], skin);

        if let Some(sel) = self.test_picker {
            self.render_test_picker(f, area, skin, sel);
        }
    }

    fn setting_row<'a>(&self, i: usize, s: &'a Setting, skin: &Skin) -> ListItem<'a> {
        let selected = i == self.selected;
        let value = self.model.value(s.key);
        let overridden = self.model.is_overridden(s.key);
        let star = if overridden { "*" } else { " " };
        let val_style = if overridden {
            skin.accent_bold()
        } else {
            skin.body()
        };
        let label_style = if selected {
            skin.selection()
        } else {
            skin.body()
        };
        ListItem::new(Line::from(vec![
            Span::styled(if selected { "  ▸ " } else { "    " }, skin.accent_bold()),
            Span::styled(format!("{star} "), skin.dim()),
            Span::styled(format!("{:<20}", s.label), label_style),
            Span::styled(format!("{value:>22}"), val_style),
        ]))
    }

    fn render_side(&self, f: &mut Frame, area: Rect, skin: &Skin) {
        let block = Block::default().borders(Borders::LEFT);
        let inner = block.inner(area);
        f.render_widget(block, area);

        let mut lines: Vec<Line> = Vec::new();
        // DND status
        let dnd = match self.dnd {
            Some(true) => Span::styled("Do Not Disturb: ON", skin.warn()),
            Some(false) => Span::styled("Do Not Disturb: off", skin.body()),
            None => Span::styled("Do Not Disturb: —", skin.dim()),
        };
        lines.push(Line::from(dnd));
        lines.push(Line::from(Span::styled("  d toggle · t test", skin.dim())));
        lines.push(Line::from(""));

        // help for the selected setting
        lines.push(Line::from(Span::styled(self.cur().help, skin.dim())));
        lines.push(Line::from(""));

        // rules
        lines.push(Line::from(Span::styled("Rules", skin.accent_bold())));
        if self.model.rules.is_empty() {
            lines.push(Line::from(Span::styled(
                "  none — add via `notif rule add`",
                skin.dim(),
            )));
        } else {
            for r in &self.model.rules {
                lines.push(Line::from(Span::styled(
                    format!("  {}", r.describe()),
                    skin.body(),
                )));
            }
        }
        if let Some(real) = self.model.degraded() {
            lines.push(Line::from(""));
            lines.push(Line::from(Span::styled(
                format!(
                    "⚠ mako config is a real file ({}) — theming won't reach mako",
                    real.display()
                ),
                skin.warn(),
            )));
        }
        f.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), inner);
    }

    fn render_footer(&self, f: &mut Frame, area: Rect, skin: &Skin) {
        let dirty = if self.dirty {
            Span::styled("  ·  unsaved — s to save", skin.warn())
        } else {
            Span::styled("", skin.dim())
        };
        let p = Paragraph::new(vec![
            Line::from(vec![Span::styled("Notifications", skin.dim()), dirty]),
            Line::from(Span::styled(
                "←→ adjust   r reset   d DND   t test   o flash OSD   s save",
                skin.dim(),
            )),
        ]);
        f.render_widget(p, area);
    }

    fn render_test_picker(&self, f: &mut Frame, area: Rect, skin: &Skin, sel: usize) {
        let w = 40u16.min(area.width.saturating_sub(2));
        let h = (SAMPLE_URGENCIES.len() as u16 + 2).min(area.height.saturating_sub(2));
        let x = area.x + (area.width.saturating_sub(w)) / 2;
        let y = area.y + (area.height.saturating_sub(h)) / 2;
        let rect = Rect::new(x, y, w, h);
        f.render_widget(Clear, rect);
        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(skin.accent_bold())
            .title(" Send a test — Enter, Esc to cancel ");
        let inner = block.inner(rect);
        f.render_widget(block, rect);
        let items: Vec<ListItem> = SAMPLE_URGENCIES
            .iter()
            .enumerate()
            .map(|(i, u)| {
                let on = i == sel;
                let style = if on { skin.selection() } else { skin.body() };
                ListItem::new(Line::from(vec![
                    Span::styled(if on { "▸ " } else { "  " }, skin.accent_bold()),
                    Span::styled(*u, style),
                ]))
            })
            .collect();
        f.render_widget(List::new(items), inner);
    }
}

/// Step size for an integer setting — coarse for the big millisecond/pixel
/// dimensions, single-unit for the small ones.
fn int_step(key: &str) -> i64 {
    match key {
        "default-timeout" => 500,
        "width" | "height" => 10,
        _ => 1,
    }
}
