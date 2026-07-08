//! Quick tweaks screen (roadmap 0.8.5).
//!
//! A checklist of one-key, individually-reversible tweaks. Space toggles the
//! selected one — the write happens immediately (each tweak owns a managed
//! block or a directory), and the App reloads Hyprland when the change touches
//! it. Every tweak reports its own live state, so the boxes always reflect disk.

use ratatui::crossterm::event::{KeyCode, KeyEvent};
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::text::{Line, Span};
use ratatui::widgets::{List, ListItem, Paragraph};
use ratatui::Frame;

use studio_core::modules::tweaks::{self, Ctx, State, Tweak};
use studio_core::omarchy::OmarchyPaths;

use crate::tui::theme::Skin;

pub enum TweaksAction {
    None,
    /// A tweak was written; `reload` = the App should `hyprctl reload`.
    Applied {
        label: String,
        on: bool,
        reload: bool,
    },
    Failed(String),
}

pub struct TweaksScreen {
    ctx: Ctx,
    items: Vec<Box<dyn Tweak>>,
    states: Vec<State>,
    cursor: usize,
    error: Option<String>,
}

impl TweaksScreen {
    pub fn load(_paths: &OmarchyPaths) -> Self {
        // Tweaks resolve their own paths (and injected HOME) via Ctx.
        match Ctx::discover() {
            Ok(ctx) => {
                let items = tweaks::catalog();
                let states = items.iter().map(|t| t.state(&ctx)).collect();
                Self {
                    ctx,
                    items,
                    states,
                    cursor: 0,
                    error: None,
                }
            }
            Err(e) => Self {
                ctx: Ctx {
                    paths: OmarchyPaths {
                        system: Default::default(),
                        config: Default::default(),
                        state: Default::default(),
                    },
                    home: Default::default(),
                },
                items: Vec::new(),
                states: Vec::new(),
                cursor: 0,
                error: Some(format!("{e:?}")),
            },
        }
    }

    /// Re-read every tweak's state from disk (after an apply).
    fn refresh(&mut self) {
        self.states = self.items.iter().map(|t| t.state(&self.ctx)).collect();
    }

    pub fn hint(&self) -> &'static str {
        "j/k move · Space toggle · q back"
    }

    pub fn handle(&mut self, key: KeyEvent) -> TweaksAction {
        let n = self.items.len();
        match key.code {
            KeyCode::Char('j') | KeyCode::Down if n > 0 => {
                self.cursor = (self.cursor + 1).min(n - 1)
            }
            KeyCode::Char('k') | KeyCode::Up => self.cursor = self.cursor.saturating_sub(1),
            KeyCode::Char(' ') if n > 0 => return self.toggle(),
            _ => {}
        }
        TweaksAction::None
    }

    fn toggle(&mut self) -> TweaksAction {
        let i = self.cursor;
        let want_on = self.states[i] != State::On;
        let tweak = &self.items[i];
        match tweak.set(&self.ctx, want_on) {
            Ok(_) => {
                let action = TweaksAction::Applied {
                    label: tweak.label().to_string(),
                    on: want_on,
                    reload: tweak.touches_hypr(),
                };
                self.refresh();
                action
            }
            Err(e) => TweaksAction::Failed(format!("{}: {e:?}", tweak.label())),
        }
    }

    pub fn render(&self, f: &mut Frame, area: Rect, skin: &Skin) {
        if let Some(msg) = &self.error {
            f.render_widget(
                Paragraph::new(Line::from(Span::styled(msg.clone(), skin.body()))),
                area,
            );
            return;
        }
        let rows = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Min(1), Constraint::Length(2)])
            .split(area);

        let items: Vec<ListItem> = self
            .items
            .iter()
            .enumerate()
            .map(|(i, t)| {
                let on = self.states[i] == State::On;
                let cursor = i == self.cursor;
                let box_ = if on { "[x]" } else { "[ ]" };
                let name_style = if cursor {
                    skin.selection()
                } else {
                    skin.body()
                };
                let marker = if cursor { "▸ " } else { "  " };
                ListItem::new(Line::from(vec![
                    Span::styled(format!("{marker}{box_} "), skin.accent_bold()),
                    Span::styled(format!("{:<33}", t.label()), name_style),
                    Span::styled(format!("  {}", t.detail()), skin.dim()),
                ]))
            })
            .collect();
        f.render_widget(List::new(items), rows[0]);

        let footer = Paragraph::new(vec![
            Line::from(Span::styled(
                "Quick tweaks — each one is reversible",
                skin.dim(),
            )),
            Line::from(Span::styled(self.hint(), skin.dim())),
        ]);
        f.render_widget(footer, rows[1]);
    }
}
