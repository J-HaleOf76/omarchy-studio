//! Waybar screen (spec 05 §4, roadmap 0.4.3).
//!
//! The status bar, laid out as its three lanes (left / center / right). Move a
//! module within its lane or across lanes, add one from the catalog, or remove
//! it — all edited through the comment-preserving JSONC editor. Save restarts
//! Waybar, snapshot-backed for undo. No JSON in sight.

use ratatui::crossterm::event::{KeyCode, KeyEvent};
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, List, ListItem, Paragraph, Wrap};
use ratatui::Frame;

use studio_core::cmd::CommandRunner;
use studio_core::modules::waybar::{label_for, WaybarConfig, LANES, MODULES};
use studio_core::omarchy::OmarchyPaths;

use crate::tui::theme::Skin;

pub enum WaybarAction {
    None,
    /// Save the config and restart Waybar (App snapshots first).
    Apply,
}

pub struct WaybarScreen {
    cfg: Option<WaybarConfig>,
    error: Option<String>,
    /// Flat cursor over (lane index, module index) pairs.
    cursor: usize,
    dirty: bool,
    /// Catalog picker overlay (add), Some(selected catalog index) while open.
    picker: Option<usize>,
}

impl WaybarScreen {
    pub fn load(paths: &OmarchyPaths) -> Self {
        let (cfg, error) = match WaybarConfig::load(paths) {
            Ok(c) => (Some(c), None),
            Err(e) => (None, Some(friendly(&e))),
        };
        Self {
            cfg,
            error,
            cursor: 0,
            dirty: false,
            picker: None,
        }
    }

    pub fn reload(&mut self, paths: &OmarchyPaths) {
        let keep = self.cursor;
        *self = Self::load(paths);
        self.cursor = keep;
    }

    pub fn is_modal(&self) -> bool {
        self.picker.is_some()
    }

    /// The (lane, module-index) pairs in display order.
    fn flat(&self) -> Vec<(usize, usize)> {
        let Some(cfg) = &self.cfg else { return vec![] };
        let mut v = Vec::new();
        for (li, (lane, _)) in LANES.iter().zip(cfg.lanes()).enumerate() {
            let _ = lane;
            let count = cfg.lane(LANES[li]).len();
            for mi in 0..count {
                v.push((li, mi));
            }
        }
        v
    }

    fn clamp_cursor(&mut self) {
        let n = self.flat().len();
        if n == 0 {
            self.cursor = 0;
        } else if self.cursor >= n {
            self.cursor = n - 1;
        }
    }

    pub fn handle(&mut self, key: KeyEvent) -> WaybarAction {
        if self.picker.is_some() {
            return self.handle_picker(key);
        }
        if self.cfg.is_none() {
            return WaybarAction::None;
        }
        let n = self.flat().len();
        match key.code {
            KeyCode::Char('j') | KeyCode::Down if n > 0 => {
                self.cursor = (self.cursor + 1).min(n - 1)
            }
            KeyCode::Char('k') | KeyCode::Up => self.cursor = self.cursor.saturating_sub(1),
            KeyCode::Char('g') => self.cursor = 0,
            KeyCode::Char('G') => self.cursor = n.saturating_sub(1),
            KeyCode::Char('J') => self.move_within(1),
            KeyCode::Char('K') => self.move_within(-1),
            KeyCode::Char('L') | KeyCode::Char('>') => self.move_lane(1),
            KeyCode::Char('H') | KeyCode::Char('<') => self.move_lane(-1),
            KeyCode::Char('a') => self.picker = Some(0),
            KeyCode::Char('d') | KeyCode::Char('x') => self.remove_selected(),
            KeyCode::Char('s') if self.dirty => return WaybarAction::Apply,
            _ => {}
        }
        WaybarAction::None
    }

    fn selected(&self) -> Option<(usize, usize)> {
        self.flat().get(self.cursor).copied()
    }

    /// Move the selected module up/down within its own lane.
    fn move_within(&mut self, dir: i64) {
        let Some((li, mi)) = self.selected() else {
            return;
        };
        let lane = LANES[li];
        let mut ids = self.cfg.as_ref().unwrap().lane(lane);
        let target = mi as i64 + dir;
        if target < 0 || target as usize >= ids.len() {
            return;
        }
        ids.swap(mi, target as usize);
        if self.cfg.as_mut().unwrap().reorder(lane, &ids).is_ok() {
            self.dirty = true;
            // follow the moved item
            self.cursor = (self.cursor as i64 + dir).max(0) as usize;
            self.clamp_cursor();
        }
    }

    /// Move the selected module to the adjacent lane (append there).
    fn move_lane(&mut self, dir: i64) {
        let Some((li, mi)) = self.selected() else {
            return;
        };
        let target_lane = li as i64 + dir;
        if target_lane < 0 || target_lane as usize >= LANES.len() {
            return;
        }
        let from = LANES[li];
        let to = LANES[target_lane as usize];
        let id = self.cfg.as_ref().unwrap().lane(from)[mi].clone();
        let cfg = self.cfg.as_mut().unwrap();
        if cfg.remove(from, &id).is_ok() && cfg.add(to, &id).is_ok() {
            self.dirty = true;
        }
        self.clamp_cursor();
    }

    fn remove_selected(&mut self) {
        let Some((li, mi)) = self.selected() else {
            return;
        };
        let lane = LANES[li];
        let id = self.cfg.as_ref().unwrap().lane(lane)[mi].clone();
        if self.cfg.as_mut().unwrap().remove(lane, &id).is_ok() {
            self.dirty = true;
            self.clamp_cursor();
        }
    }

    fn handle_picker(&mut self, key: KeyEvent) -> WaybarAction {
        let Some(sel) = self.picker.as_mut() else {
            return WaybarAction::None;
        };
        match key.code {
            KeyCode::Esc => self.picker = None,
            KeyCode::Char('j') | KeyCode::Down => *sel = (*sel + 1).min(MODULES.len() - 1),
            KeyCode::Char('k') | KeyCode::Up => *sel = sel.saturating_sub(1),
            KeyCode::Enter => {
                let module = MODULES[*sel];
                // add to the lane the cursor is currently in (default left)
                let li = self.selected().map(|(li, _)| li).unwrap_or(0);
                let lane = LANES[li];
                // strip the trailing "/*" from custom/group catalog ids
                let id = module.id.trim_end_matches("/*");
                let id = if id.is_empty() { "custom/new" } else { id };
                if self.cfg.as_mut().unwrap().add(lane, id).is_ok() {
                    self.dirty = true;
                }
                self.picker = None;
            }
            _ => {}
        }
        WaybarAction::None
    }

    // IO delegated from the App.
    pub fn commit(&self, runner: &dyn CommandRunner) -> studio_core::error::Result<()> {
        match &self.cfg {
            Some(c) => c.apply(runner),
            None => Ok(()),
        }
    }

    pub fn config_path(&self) -> Option<std::path::PathBuf> {
        self.cfg.as_ref().map(|c| c.path().to_path_buf())
    }

    pub fn render(&self, f: &mut Frame, area: Rect, skin: &Skin) {
        if let Some(msg) = &self.error {
            let p = Paragraph::new(vec![
                Line::from(Span::styled(
                    "Can't read your Waybar config",
                    skin.accent_bold(),
                )),
                Line::from(""),
                Line::from(Span::styled(msg.clone(), skin.body())),
            ])
            .wrap(Wrap { trim: false });
            f.render_widget(p, area);
            return;
        }
        let Some(cfg) = &self.cfg else { return };

        let rows = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Min(1), Constraint::Length(3)])
            .split(area);

        let mut items: Vec<ListItem> = Vec::new();
        let mut flat_i = 0usize;
        for (li, lane) in LANES.iter().enumerate() {
            let title = match li {
                0 => "Left",
                1 => "Center",
                _ => "Right",
            };
            items.push(ListItem::new(Line::from(Span::styled(
                format!("  {title}"),
                skin.dim(),
            ))));
            let ids = cfg.lane(lane);
            if ids.is_empty() {
                items.push(ListItem::new(Line::from(Span::styled(
                    "      (empty)",
                    skin.dim(),
                ))));
            }
            for id in ids {
                let selected = flat_i == self.cursor;
                let style = if selected {
                    skin.selection()
                } else {
                    skin.body()
                };
                items.push(ListItem::new(Line::from(vec![
                    Span::styled(if selected { "  ▸ " } else { "    " }, skin.accent_bold()),
                    Span::styled(format!("{:<24}", label_for(&id)), style),
                    Span::styled(id, skin.dim()),
                ])));
                flat_i += 1;
            }
        }
        f.render_widget(List::new(items), rows[0]);
        self.render_footer(f, rows[1], skin);

        if let Some(sel) = self.picker {
            self.render_picker(f, area, skin, sel);
        }
    }

    fn render_footer(&self, f: &mut Frame, area: Rect, skin: &Skin) {
        let dirty = if self.dirty {
            Span::styled("  ·  unsaved — s to apply & restart Waybar", skin.warn())
        } else {
            Span::styled("", skin.dim())
        };
        let p = Paragraph::new(vec![
            Line::from(vec![Span::styled("Bar layout", skin.dim()), dirty]),
            Line::from(Span::styled(
                "J/K move in lane   H/L move across   a add   d remove   s apply",
                skin.dim(),
            )),
        ]);
        f.render_widget(p, area);
    }

    fn render_picker(&self, f: &mut Frame, area: Rect, skin: &Skin, sel: usize) {
        let w = 56u16.min(area.width.saturating_sub(2));
        let h = 16u16.min(area.height.saturating_sub(2));
        let x = area.x + (area.width.saturating_sub(w)) / 2;
        let y = area.y + (area.height.saturating_sub(h)) / 2;
        let rect = Rect::new(x, y, w, h);
        f.render_widget(Clear, rect);
        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(skin.accent_bold())
            .title(" Add a module — Enter to add, Esc to cancel ");
        let inner = block.inner(rect);
        f.render_widget(block, rect);
        let view = (inner.height as usize).max(1);
        let scroll = sel.saturating_sub(view.saturating_sub(1));
        let items: Vec<ListItem> = MODULES
            .iter()
            .enumerate()
            .skip(scroll)
            .take(view)
            .map(|(i, m)| {
                let on = i == sel;
                let name = if on {
                    Span::styled(format!("▸ {}", m.label), skin.selection())
                } else {
                    Span::styled(format!("  {}", m.label), skin.body())
                };
                ListItem::new(Line::from(vec![
                    name,
                    Span::styled(format!("  {}", m.summary), skin.dim()),
                ]))
            })
            .collect();
        f.render_widget(List::new(items), inner);
    }
}

fn friendly(e: &studio_core::StudioError) -> String {
    match e {
        studio_core::StudioError::External { detail, .. } => detail.clone(),
        other => format!("{other:?}"),
    }
}
