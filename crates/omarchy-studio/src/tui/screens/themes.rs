//! Themes screen (spec 07 §8, PRD M1): browse with swatch strips, apply, fork.
//! First real Studio screen — the template the others follow.

use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};
use ratatui::Frame;

use studio_core::modules::themes::{Theme, ThemeOrigin, ThemeStore, PALETTE_KEYS};
use studio_core::omarchy::OmarchyPaths;

use crate::tui::theme::Skin;

/// A pre-rendered browser row (swatches loaded once, not per frame).
struct Row {
    slug: String,
    pretty: String,
    origin: ThemeOrigin,
    light: bool,
    swatch: Vec<Color>,
}

/// What the screen wants the app to do (screens stay pure of side effects).
pub enum ThemeAction {
    None,
    /// Run `omarchy-theme-set <slug>` and toast the result.
    Apply(String),
    /// Fork `src` into a new user theme named `name`.
    Fork {
        src: String,
        name: String,
    },
    /// Open the palette editor for `slug`.
    Edit(String),
}

pub struct ThemesScreen {
    rows: Vec<Row>,
    current: String,
    selected: usize,
    /// Some(buffer) while typing a fork name for the selected theme.
    fork_input: Option<String>,
}

impl ThemesScreen {
    pub fn load(paths: &OmarchyPaths) -> Self {
        let store = ThemeStore::from_paths(paths);
        let current = paths.current_theme_name().unwrap_or_default();
        let rows = store.list().iter().map(row_of).collect();
        let mut s = Self {
            rows,
            current,
            selected: 0,
            fork_input: None,
        };
        s.select_current();
        s
    }

    fn select_current(&mut self) {
        if let Some(i) = self.rows.iter().position(|r| r.slug == self.current) {
            self.selected = i;
        }
    }

    /// Move the selection to a specific slug (command-palette jump).
    pub fn focus(&mut self, slug: &str) {
        if let Some(i) = self.rows.iter().position(|r| r.slug == slug) {
            self.selected = i;
        }
    }

    /// True while a text field owns the keyboard (fork-name entry) — the app
    /// suspends its global chords so letters land in the buffer.
    pub fn is_capturing(&self) -> bool {
        self.fork_input.is_some()
    }

    fn visible(&self) -> Vec<&Row> {
        self.rows.iter().collect()
    }

    /// Key handling. Returns an action for the app to execute.
    pub fn handle(&mut self, key: ratatui::crossterm::event::KeyEvent) -> ThemeAction {
        use ratatui::crossterm::event::KeyCode::*;

        // Fork-name entry captures all keys until Enter/Esc.
        if let Some(buf) = self.fork_input.as_mut() {
            match key.code {
                Enter => {
                    let name = buf.trim().to_string();
                    self.fork_input = None;
                    if let Some(src) = self.selected_slug() {
                        if !name.is_empty() {
                            return ThemeAction::Fork { src, name };
                        }
                    }
                }
                Esc => self.fork_input = None,
                Backspace => {
                    buf.pop();
                }
                Char(c) => buf.push(c),
                _ => {}
            }
            return ThemeAction::None;
        }

        let visible_len = self.visible().len();
        match key.code {
            Char('j') | Down if visible_len > 0 => {
                self.selected = (self.selected + 1).min(visible_len - 1)
            }
            Char('k') | Up => self.selected = self.selected.saturating_sub(1),
            Char('g') => self.selected = 0,
            Char('G') => self.selected = visible_len.saturating_sub(1),
            Enter => {
                if let Some(slug) = self.selected_slug() {
                    return ThemeAction::Apply(slug);
                }
            }
            Char('f') if self.selected_slug().is_some() => self.fork_input = Some(String::new()),
            Char('e') => {
                if let Some(slug) = self.selected_slug() {
                    return ThemeAction::Edit(slug);
                }
            }
            _ => {}
        }
        ThemeAction::None
    }

    fn selected_slug(&self) -> Option<String> {
        self.visible().get(self.selected).map(|r| r.slug.clone())
    }

    /// Reload after a mutation (apply changes `current`, fork adds a row).
    pub fn reload(&mut self, paths: &OmarchyPaths) {
        *self = Self::load(paths);
    }

    /// The jargon/status hint for the keybar.
    pub fn hint(&self) -> String {
        if self.fork_input.is_some() {
            "type a name for the new theme · enter save · esc cancel".into()
        } else {
            "j/k move · enter apply · e edit colours · f fork".into()
        }
    }

    pub fn render(&self, f: &mut Frame, area: Rect, skin: &Skin) {
        let rows = self.visible();
        if rows.is_empty() {
            let msg = if self.rows.is_empty() {
                "No themes found. Is Omarchy installed?"
            } else {
                "No theme matches your filter."
            };
            f.render_widget(Paragraph::new(msg).style(skin.dim()), area);
            return;
        }

        // One line per theme: marker, swatch strip, name, badges.
        let lines: Vec<Line> = rows
            .iter()
            .enumerate()
            .map(|(i, r)| self.render_row(r, i == self.selected, skin))
            .collect();
        f.render_widget(Paragraph::new(lines), area);

        if let Some(buf) = &self.fork_input {
            self.render_fork_modal(f, area, buf, skin);
        }
    }

    fn render_row<'a>(&self, r: &'a Row, selected: bool, skin: &Skin) -> Line<'a> {
        let mut spans = Vec::new();
        let marker = if r.slug == self.current { "● " } else { "  " };
        spans.push(Span::styled(
            marker,
            if r.slug == self.current {
                skin.accent_bold()
            } else {
                skin.dim()
            },
        ));
        for c in &r.swatch {
            spans.push(Span::styled("█", Style::default().fg(*c)));
        }
        spans.push(Span::raw("  "));
        let name_style = if selected {
            skin.selection()
        } else {
            skin.body()
        };
        spans.push(Span::styled(format!("{:<22}", r.pretty), name_style));
        let badge = match r.origin {
            ThemeOrigin::System => "",
            ThemeOrigin::User => " user",
            ThemeOrigin::Overlay => " overlay",
        };
        if !badge.is_empty() {
            spans.push(Span::styled(badge, skin.dim()));
        }
        if r.light {
            spans.push(Span::styled(" ☀", skin.warn()));
        }
        Line::from(spans)
    }

    fn render_fork_modal(&self, f: &mut Frame, area: Rect, buf: &str, skin: &Skin) {
        let w = 48.min(area.width.saturating_sub(4));
        let modal = Rect {
            x: area.x + (area.width.saturating_sub(w)) / 2,
            y: area.y + area.height / 2,
            width: w,
            height: 3,
        };
        f.render_widget(ratatui::widgets::Clear, modal);
        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(skin.accent_bold())
            .title(" fork theme ");
        let inner = block.inner(modal);
        f.render_widget(block, modal);
        let parts = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Length(2), Constraint::Min(1)])
            .split(inner);
        f.render_widget(Paragraph::new("› ").style(skin.dim()), parts[0]);
        f.render_widget(
            Paragraph::new(format!("{buf}▏")).style(skin.body()),
            parts[1],
        );
    }
}

fn row_of(t: &Theme) -> Row {
    // Swatch = accent + background + a spread of the ANSI colors, when present.
    let swatch = t
        .palette()
        .ok()
        .map(|p| {
            [
                "accent",
                "background",
                "color1",
                "color2",
                "color3",
                "color4",
                "color5",
                "color6",
            ]
            .iter()
            .filter_map(|k| p.get(k))
            .map(|c| Color::Rgb(c.r, c.g, c.b))
            .collect::<Vec<_>>()
        })
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| vec![Color::DarkGray; 3]);
    let _ = PALETTE_KEYS; // keys referenced for future full-swatch mode
    Row {
        slug: t.slug.clone(),
        pretty: t.pretty.clone(),
        origin: t.origin,
        light: t.light(),
        swatch,
    }
}
