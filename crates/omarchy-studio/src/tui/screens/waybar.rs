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
use studio_core::modules::waybar::{label_for, CustomSpec, WaybarConfig, LANES, MODULES};
use studio_core::omarchy::OmarchyPaths;

use crate::tui::theme::Skin;

pub enum WaybarAction {
    None,
    /// Save the config and restart Waybar (App snapshots first).
    Apply,
}

/// The custom-module wizard's text fields (roadmap 0.4.5).
const WIZARD_LABELS: [&str; 5] = [
    "Name",
    "Command to run",
    "Refresh (seconds, blank = once)",
    "Display format",
    "On click (optional)",
];

/// A little form for scaffolding a `custom/*` module in plain language.
struct Wizard {
    fields: [String; 5],
    focus: usize,
    /// Lane the new module lands in (index into LANES).
    lane: usize,
    error: Option<String>,
}

impl Wizard {
    fn new(lane: usize) -> Self {
        let mut fields: [String; 5] = Default::default();
        fields[3] = "{}".to_string(); // sensible default format
        Self {
            fields,
            focus: 0,
            lane,
            error: None,
        }
    }

    fn spec(&self) -> CustomSpec {
        let interval = self.fields[2].trim().parse::<u32>().ok();
        let on_click = {
            let s = self.fields[4].trim();
            (!s.is_empty()).then(|| s.to_string())
        };
        let format = {
            let f = self.fields[3].trim();
            if f.is_empty() {
                "{}".into()
            } else {
                f.to_string()
            }
        };
        CustomSpec {
            name: self.fields[0].trim().to_string(),
            exec: self.fields[1].trim().to_string(),
            interval,
            format,
            on_click,
        }
    }
}

pub struct WaybarScreen {
    cfg: Option<WaybarConfig>,
    error: Option<String>,
    /// Flat cursor over (lane index, module index) pairs.
    cursor: usize,
    dirty: bool,
    /// Catalog picker overlay (add), Some(selected catalog index) while open.
    picker: Option<usize>,
    /// Custom-module wizard overlay, open when adding a `custom/*` module.
    wizard: Option<Wizard>,
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
            wizard: None,
        }
    }

    pub fn reload(&mut self, paths: &OmarchyPaths) {
        let keep = self.cursor;
        *self = Self::load(paths);
        self.cursor = keep;
    }

    pub fn is_modal(&self) -> bool {
        self.picker.is_some() || self.wizard.is_some()
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
        if self.wizard.is_some() {
            return self.handle_wizard(key);
        }
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
                self.picker = None;
                // A custom/group catalog entry opens the wizard rather than
                // dropping a placeholder; a concrete module is added directly.
                if module.id.ends_with("/*") {
                    self.wizard = Some(Wizard::new(li));
                } else if self.cfg.as_mut().unwrap().add(LANES[li], module.id).is_ok() {
                    self.dirty = true;
                }
            }
            _ => {}
        }
        WaybarAction::None
    }

    fn handle_wizard(&mut self, key: KeyEvent) -> WaybarAction {
        let Some(wiz) = self.wizard.as_mut() else {
            return WaybarAction::None;
        };
        match key.code {
            KeyCode::Esc => self.wizard = None,
            KeyCode::Tab | KeyCode::Down => wiz.focus = (wiz.focus + 1) % WIZARD_LABELS.len(),
            KeyCode::BackTab | KeyCode::Up => {
                wiz.focus = (wiz.focus + WIZARD_LABELS.len() - 1) % WIZARD_LABELS.len()
            }
            KeyCode::Backspace => {
                wiz.fields[wiz.focus].pop();
            }
            KeyCode::Char(c) => wiz.fields[wiz.focus].push(c),
            KeyCode::Enter => {
                let spec = wiz.spec();
                if spec.name.is_empty() {
                    wiz.error = Some("give the module a name".into());
                    wiz.focus = 0;
                } else if spec.exec.is_empty() {
                    wiz.error = Some("what command should it run?".into());
                    wiz.focus = 1;
                } else {
                    let lane = LANES[wiz.lane];
                    match self.cfg.as_mut().unwrap().add_custom_module(&spec, lane) {
                        Ok(_) => {
                            self.dirty = true;
                            self.wizard = None;
                        }
                        Err(e) => self.wizard.as_mut().unwrap().error = Some(friendly(&e)),
                    }
                }
            }
            _ => {}
        }
        WaybarAction::None
    }

    // IO delegated from the App. Applies with the crash watchdog: if the edit
    // stops Waybar, the previous config is restored automatically.
    pub fn commit(
        &self,
        runner: &dyn CommandRunner,
    ) -> studio_core::error::Result<studio_core::modules::waybar::ApplyOutcome> {
        use studio_core::modules::waybar::ApplyOutcome;
        match &self.cfg {
            // Give Waybar a moment to come up (or crash) before checking.
            Some(c) => c.apply_watched(runner, std::time::Duration::from_millis(900)),
            None => Ok(ApplyOutcome::Applied),
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
        if let Some(wiz) = &self.wizard {
            self.render_wizard(f, area, skin, wiz);
        }
    }

    fn render_wizard(&self, f: &mut Frame, area: Rect, skin: &Skin, wiz: &Wizard) {
        let w = 60u16.min(area.width.saturating_sub(2));
        let h = 15u16.min(area.height.saturating_sub(2));
        let x = area.x + (area.width.saturating_sub(w)) / 2;
        let y = area.y + (area.height.saturating_sub(h)) / 2;
        let rect = Rect::new(x, y, w, h);
        f.render_widget(Clear, rect);
        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(skin.accent_bold())
            .title(format!(" New custom module → {} ", LANES[wiz.lane]));
        let inner = block.inner(rect);
        f.render_widget(block, rect);

        let mut lines: Vec<Line> = Vec::new();
        for (i, label) in WIZARD_LABELS.iter().enumerate() {
            let on = i == wiz.focus;
            let marker = if on { "▸ " } else { "  " };
            let val = &wiz.fields[i];
            let shown = if on {
                format!("{val}▏") // cursor caret
            } else {
                val.clone()
            };
            let label_style = if on { skin.accent_bold() } else { skin.dim() };
            lines.push(Line::from(vec![
                Span::styled(format!("{marker}{label}: "), label_style),
                Span::styled(shown, skin.body()),
            ]));
        }
        lines.push(Line::from(""));
        if let Some(err) = &wiz.error {
            lines.push(Line::from(Span::styled(format!("  {err}"), skin.warn())));
        }
        lines.push(Line::from(Span::styled(
            "  Tab next · Enter create · Esc cancel",
            skin.dim(),
        )));
        f.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), inner);
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
