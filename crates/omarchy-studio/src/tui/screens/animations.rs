//! Animations screen (spec 05 §3, roadmap 0.3.6).
//!
//! Pick an animation *feel* — Off / Fast / Smooth / Minimal / Bouncy or the
//! Omarchy default. Applying writes the managed animations block and reloads
//! Hyprland, snapshot-backed for undo. (The bezier canvas editor is a separate
//! feature-flagged concern.)

use ratatui::crossterm::event::{KeyCode, KeyEvent};
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::text::{Line, Span};
use ratatui::widgets::{List, ListItem, Paragraph};
use ratatui::Frame;

use studio_core::modules::animations::{current, PRESETS};
use studio_core::omarchy::OmarchyPaths;

use crate::tui::theme::Skin;

pub enum AnimAction {
    None,
    /// Apply the preset with this name (App snapshots + reloads).
    Apply(String),
}

pub struct AnimationsScreen {
    selected: usize,
    /// Name of the currently-applied preset (or "Default"/"Custom").
    current: String,
}

impl AnimationsScreen {
    pub fn load(paths: &OmarchyPaths) -> Self {
        Self {
            selected: 0,
            current: current(paths).to_string(),
        }
    }

    pub fn reload(&mut self, paths: &OmarchyPaths) {
        self.current = current(paths).to_string();
    }

    pub fn handle(&mut self, key: KeyEvent) -> AnimAction {
        match key.code {
            KeyCode::Down => {
                self.selected = (self.selected + 1).min(PRESETS.len() - 1)
            }
            KeyCode::Up => self.selected = self.selected.saturating_sub(1),
            KeyCode::Home => self.selected = 0,
            KeyCode::End => self.selected = PRESETS.len() - 1,
            KeyCode::Enter => return AnimAction::Apply(PRESETS[self.selected].name.to_string()),
            _ => {}
        }
        AnimAction::None
    }

    pub fn render(&self, f: &mut Frame, area: Rect, skin: &Skin) {
        let rows = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Min(1), Constraint::Length(2)])
            .split(area);

        let items: Vec<ListItem> = PRESETS
            .iter()
            .enumerate()
            .map(|(i, p)| {
                let sel = i == self.selected;
                let active = p.name == self.current;
                let marker = if active { "●" } else { " " };
                let name_style = if sel { skin.selection() } else { skin.body() };
                ListItem::new(vec![Line::from(vec![
                    Span::styled(if sel { "▸ " } else { "  " }, skin.accent_bold()),
                    Span::styled(format!("{marker} "), skin.accent_bold()),
                    Span::styled(format!("{:<10}", p.name), name_style),
                    Span::styled(p.blurb, skin.dim()),
                ])])
            })
            .collect();
        f.render_widget(List::new(items), rows[0]);

        let footer = Paragraph::new(vec![
            Line::from(Span::styled(
                format!("Current: {}", self.current),
                skin.dim(),
            )),
            Line::from(Span::styled(
                "↑↓ choose   Enter apply (live reload, undoable)",
                skin.dim(),
            )),
        ]);
        f.render_widget(footer, rows[1]);
    }
}
