//! Look & Feel screen (spec 05 §2, roadmap 0.3.1 + 0.3.2).
//!
//! A schema-driven form over the Hyprland look & feel settings. Adjusting a
//! value applies it *live* via `hyprctl keyword` (no file written) so the
//! desktop changes under you; Save persists the batch into the managed block
//! with a snapshot for undo. A preview marker on disk lets a future launch
//! notice if Studio died mid-preview and offer to restore.

use ratatui::crossterm::event::{KeyCode, KeyEvent};
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::text::{Line, Span};
use ratatui::widgets::{List, ListItem, Paragraph, Wrap};
use ratatui::Frame;

use ratatui::widgets::{Block, Borders, Clear};

use studio_core::cmd::CommandRunner;
use studio_core::modules::looknfeel::{groups, Kind, LookFeel, Setting, PRESETS, SETTINGS};
use studio_core::omarchy::OmarchyPaths;

use crate::tui::theme::Skin;

/// What the screen wants the App to do after a key.
pub enum LookFeelAction {
    None,
    /// Preview the setting at this schema index live.
    Preview(usize),
    /// Preview the whole batch live (after applying a preset).
    PreviewAll,
    /// Persist the batch (App snapshots first).
    Save,
    /// Reset every override and reload to the persisted config.
    ResetAll,
}

pub struct LookFeelScreen {
    model: LookFeel,
    /// Index into `SETTINGS`.
    selected: usize,
    scroll: usize,
    /// True once anything has been previewed but not yet saved.
    pub dirty: bool,
    /// Preset picker overlay: Some(selected preset index) while open.
    picker: Option<usize>,
}

impl LookFeelScreen {
    pub fn load(paths: &OmarchyPaths) -> Self {
        Self {
            model: LookFeel::load(paths),
            selected: 0,
            scroll: 0,
            dirty: false,
            picker: None,
        }
    }

    pub fn reload(&mut self, paths: &OmarchyPaths) {
        let keep = self.selected;
        self.model = LookFeel::load(paths);
        self.selected = keep.min(SETTINGS.len().saturating_sub(1));
        self.dirty = false;
        self.picker = None;
    }

    fn cur(&self) -> &Setting {
        &SETTINGS[self.selected]
    }

    /// True while a modal (the preset picker) owns input — the App suspends its
    /// global chords so digits/letters reach us.
    pub fn is_modal(&self) -> bool {
        self.picker.is_some()
    }

    pub fn handle(&mut self, key: KeyEvent) -> LookFeelAction {
        if self.picker.is_some() {
            return self.handle_picker(key);
        }
        let n = SETTINGS.len();
        match key.code {
            KeyCode::Char('p') => {
                self.picker = Some(0);
                return LookFeelAction::None;
            }
            KeyCode::Char('j') | KeyCode::Down => {
                self.selected = (self.selected + 1).min(n - 1);
            }
            KeyCode::Char('k') | KeyCode::Up => self.selected = self.selected.saturating_sub(1),
            KeyCode::Char('g') => self.selected = 0,
            KeyCode::Char('G') => self.selected = n - 1,
            KeyCode::Char('l') | KeyCode::Right | KeyCode::Char('+') | KeyCode::Char('=') => {
                return self.nudge(1)
            }
            KeyCode::Char('h') | KeyCode::Left | KeyCode::Char('-') | KeyCode::Char(' ') => {
                return self.nudge(-1)
            }
            KeyCode::Enter => return self.nudge(1),
            KeyCode::Char('r') => return self.reset_selected(),
            KeyCode::Char('R') => {
                self.model.clear_all();
                self.dirty = false;
                return LookFeelAction::ResetAll;
            }
            KeyCode::Char('s') => return LookFeelAction::Save,
            _ => {}
        }
        LookFeelAction::None
    }

    /// Change the selected setting by one step in `dir` and request a preview.
    fn nudge(&mut self, dir: i64) -> LookFeelAction {
        let setting = *self.cur();
        let current = self.model.value(setting.key);
        let next = match setting.kind {
            Kind::Bool => {
                let on = matches!(current.as_str(), "true" | "yes" | "on" | "1");
                if on {
                    "false".to_string()
                } else {
                    "true".to_string()
                }
            }
            Kind::Enum(opts) => {
                let i = opts.iter().position(|o| *o == current).unwrap_or(0) as i64;
                let j = (i + dir).rem_euclid(opts.len() as i64) as usize;
                opts[j].to_string()
            }
            Kind::Int { min, max } => {
                let v: i64 = current.parse().unwrap_or(min);
                (v + dir).clamp(min, max).to_string()
            }
            Kind::Float { min, max } => {
                let v: f64 = current.parse().unwrap_or(min);
                let step = 0.05;
                let nv = (v + dir as f64 * step).clamp(min, max);
                format!("{:.2}", nv)
            }
        };
        if self.model.set(setting.key, &next).is_ok() {
            self.dirty = true;
            return LookFeelAction::Preview(self.selected);
        }
        LookFeelAction::None
    }

    fn reset_selected(&mut self) -> LookFeelAction {
        let key = self.cur().key;
        self.model.clear(key);
        self.dirty = true;
        // preview the reverted (base/default) value live
        LookFeelAction::Preview(self.selected)
    }

    fn handle_picker(&mut self, key: KeyEvent) -> LookFeelAction {
        let Some(sel) = self.picker.as_mut() else {
            return LookFeelAction::None;
        };
        match key.code {
            KeyCode::Esc => self.picker = None,
            KeyCode::Char('j') | KeyCode::Down => *sel = (*sel + 1).min(PRESETS.len() - 1),
            KeyCode::Char('k') | KeyCode::Up => *sel = sel.saturating_sub(1),
            KeyCode::Enter => {
                let preset = &PRESETS[*sel];
                self.model.apply_preset(preset);
                self.dirty = true;
                self.picker = None;
                return LookFeelAction::PreviewAll;
            }
            _ => {}
        }
        LookFeelAction::None
    }

    // ── IO delegated from the App (keeps the screen itself pure of runners) ──

    pub fn preview(
        &self,
        idx: usize,
        runner: &dyn CommandRunner,
    ) -> studio_core::error::Result<()> {
        self.model.preview_one(SETTINGS[idx].key, runner)
    }

    pub fn preview_all(&self, runner: &dyn CommandRunner) -> studio_core::error::Result<()> {
        self.model.preview_all(runner)
    }

    pub fn commit(
        &self,
        paths: &OmarchyPaths,
        runner: &dyn CommandRunner,
    ) -> studio_core::error::Result<()> {
        self.model.apply(paths, runner).map(|_| ())
    }

    pub fn any_overrides(&self) -> bool {
        self.model.any_overrides()
    }

    pub fn render(&mut self, f: &mut Frame, area: Rect, skin: &Skin) {
        let rows = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Min(1), Constraint::Length(3)])
            .split(area);

        // Build display rows: a header per group, then its settings.
        let mut items: Vec<ListItem> = Vec::new();
        // index map: display-row -> Option<setting index>, to place the cursor
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

        let height = rows[0].height as usize;
        if sel_row < self.scroll {
            self.scroll = sel_row;
        } else if sel_row >= self.scroll + height {
            self.scroll = sel_row + 1 - height;
        }
        let visible: Vec<ListItem> = items.into_iter().skip(self.scroll).take(height).collect();
        f.render_widget(List::new(visible), rows[0]);

        self.render_footer(f, rows[1], skin);

        if let Some(sel) = self.picker {
            self.render_picker(f, area, skin, sel);
        }
    }

    fn render_picker(&self, f: &mut Frame, area: Rect, skin: &Skin, sel: usize) {
        let w = 52u16.min(area.width.saturating_sub(2));
        // each preset renders on two lines (name + blurb), plus the border.
        let h = (PRESETS.len() as u16 * 2 + 2).min(area.height.saturating_sub(2));
        let x = area.x + (area.width.saturating_sub(w)) / 2;
        let y = area.y + (area.height.saturating_sub(h)) / 2;
        let rect = Rect::new(x, y, w, h);
        f.render_widget(Clear, rect);
        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(skin.accent_bold())
            .title(" Presets — Enter to try, Esc to cancel ");
        let inner = block.inner(rect);
        f.render_widget(block, rect);

        let items: Vec<ListItem> = PRESETS
            .iter()
            .enumerate()
            .map(|(i, p)| {
                let on = i == sel;
                let name = if on {
                    Span::styled(format!("▸ {}", p.name), skin.selection())
                } else {
                    Span::styled(format!("  {}", p.name), skin.body())
                };
                ListItem::new(vec![
                    Line::from(name),
                    Line::from(Span::styled(format!("    {}", p.blurb), skin.dim())),
                ])
            })
            .collect();
        f.render_widget(List::new(items), inner);
    }

    fn setting_row<'a>(&self, i: usize, s: &'a Setting, skin: &Skin) -> ListItem<'a> {
        let selected = i == self.selected;
        let value = display_value(&self.model.value(s.key), s.kind);
        let overridden = self.model.is_overridden(s.key);
        let base = if selected {
            skin.selection()
        } else {
            skin.body()
        };
        let dot = if overridden { "●" } else { " " };
        ListItem::new(Line::from(vec![
            Span::styled(if selected { "  ▸ " } else { "    " }, skin.accent_bold()),
            Span::styled(dot.to_string(), skin.accent_bold()),
            Span::styled(format!(" {:<20}", s.label), base),
            Span::styled(format!("{value:>10}"), skin.accent_bold()),
        ]))
    }

    fn render_footer(&self, f: &mut Frame, area: Rect, skin: &Skin) {
        let s = self.cur();
        let range = match s.kind {
            Kind::Int { min, max } => format!("  ({min}–{max})"),
            Kind::Float { min, max } => format!("  ({min}–{max})"),
            Kind::Enum(opts) => format!("  ({})", opts.join(" / ")),
            Kind::Bool => String::new(),
        };
        let save = if self.dirty {
            Span::styled("  ·  unsaved — s to keep", skin.warn())
        } else {
            Span::styled("", skin.dim())
        };
        let p = Paragraph::new(vec![
            Line::from(vec![
                Span::styled(format!("{}{range}", s.help), skin.dim()),
                save,
            ]),
            Line::from(Span::styled(
                "h/l adjust   space toggle   p presets   r reset   R reset all   s save",
                skin.dim(),
            )),
        ])
        .wrap(Wrap { trim: true });
        f.render_widget(p, area);
    }
}

fn display_value(raw: &str, kind: Kind) -> String {
    match kind {
        Kind::Bool => {
            if matches!(raw, "true" | "yes" | "on" | "1") {
                "on".into()
            } else {
                "off".into()
            }
        }
        _ => raw.to_string(),
    }
}
