//! Palette editor (spec 04 §2, PRD FR1.2/1.3): edit the 21 `colors.toml`
//! keys with hex entry and lighten/darken helpers, a live WCAG contrast
//! strip, and whole-desktop live preview via the scratch theme. Save writes
//! the real theme; cancel restores what was showing before.
//!
//! Screens stay side-effect-free: this one mutates only its in-memory palette
//! and returns an [`EditorAction`]; the app performs preview/save/cancel.

use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::Frame;

use studio_core::modules::color::{contrast_ratio, Color as PColor};
use studio_core::modules::themes::{Palette, Theme, PALETTE_KEYS};

use crate::tui::theme::Skin;

pub enum EditorAction {
    None,
    /// Re-render the desktop from the working palette.
    Preview,
    /// Persist to the real theme and apply it.
    Save,
    /// Discard edits and restore the previously-showing theme.
    Cancel,
}

pub struct PaletteEditor {
    pub slug: String,
    pub pretty: String,
    pub light: bool,
    working: Palette,
    selected: usize,
    /// Some(buffer) while typing a hex value into the selected key.
    edit: Option<String>,
    pub dirty: bool,
}

impl PaletteEditor {
    pub fn open(theme: &Theme) -> Option<Self> {
        let working = theme.palette().ok()?;
        Some(Self {
            slug: theme.slug.clone(),
            pretty: theme.pretty.clone(),
            light: theme.light(),
            working,
            selected: 0,
            edit: None,
            dirty: false,
        })
    }

    pub fn working(&self) -> &Palette {
        &self.working
    }

    pub fn hint(&self) -> String {
        if self.edit.is_some() {
            "type a hex colour (#rrggbb) · enter set · esc cancel".into()
        } else {
            let star = if self.dirty { " ·  unsaved" } else { "" };
            format!("j/k move · enter edit · [ / ] darken/lighten · s save · esc cancel{star}")
        }
    }

    pub fn handle(&mut self, key: ratatui::crossterm::event::KeyEvent) -> EditorAction {
        use ratatui::crossterm::event::KeyCode::*;

        if let Some(buf) = self.edit.as_mut() {
            match key.code {
                Enter => {
                    let text = buf.clone();
                    match PColor::parse(&text) {
                        Ok(c) => {
                            self.edit = None;
                            self.working.set(self.key(), &c);
                            self.dirty = true;
                            return EditorAction::Preview;
                        }
                        // Keep the field open on a bad value; the strip shows it.
                        Err(_) => {}
                    }
                }
                Esc => self.edit = None,
                Backspace => {
                    buf.pop();
                }
                Char(c) if buf.len() < 7 => buf.push(c),
                _ => {}
            }
            return EditorAction::None;
        }

        match key.code {
            Char('j') | Down => self.selected = (self.selected + 1).min(PALETTE_KEYS.len() - 1),
            Char('k') | Up => self.selected = self.selected.saturating_sub(1),
            Char('g') => self.selected = 0,
            Char('G') => self.selected = PALETTE_KEYS.len() - 1,
            Enter => {
                let cur = self
                    .working
                    .get(self.key())
                    .map(|c| c.hex())
                    .unwrap_or_else(|| "#".into());
                self.edit = Some(cur);
            }
            Char(']') => return self.nudge(0.04),
            Char('[') => return self.nudge(-0.04),
            Char('s') => return EditorAction::Save,
            Esc => return EditorAction::Cancel,
            _ => {}
        }
        EditorAction::None
    }

    fn nudge(&mut self, amount: f32) -> EditorAction {
        if let Some(c) = self.working.get(self.key()) {
            self.working.set(self.key(), &c.lighten(amount));
            self.dirty = true;
            return EditorAction::Preview;
        }
        EditorAction::None
    }

    fn key(&self) -> &'static str {
        PALETTE_KEYS[self.selected]
    }

    pub fn render(&self, f: &mut Frame, area: Rect, skin: &Skin) {
        let rows = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(1),
                Constraint::Min(1),
                Constraint::Length(2),
            ])
            .split(area);

        f.render_widget(
            Paragraph::new(Line::from(vec![
                Span::styled("Editing ", skin.dim()),
                Span::styled(&self.pretty, skin.accent_bold()),
                Span::styled(if self.light { "  (light)" } else { "" }, skin.warn()),
            ])),
            rows[0],
        );

        let lines: Vec<Line> = PALETTE_KEYS
            .iter()
            .enumerate()
            .map(|(i, k)| self.render_key(i, k, skin))
            .collect();
        f.render_widget(Paragraph::new(lines), rows[1]);

        self.render_contrast(f, rows[2], skin);
    }

    fn render_key<'a>(&self, i: usize, key: &'a str, skin: &Skin) -> Line<'a> {
        let selected = i == self.selected;
        let editing = selected && self.edit.is_some();
        let color = self.working.get(key);
        let swatch = color.map(pc).unwrap_or(Color::DarkGray);

        let value = if editing {
            format!("{}▏", self.edit.as_deref().unwrap_or(""))
        } else {
            color.map(|c| c.hex()).unwrap_or_else(|| "unset".into())
        };
        let name_style = if selected {
            skin.selection()
        } else {
            skin.body()
        };
        let val_style = if editing {
            skin.accent_bold()
        } else if color.is_none() {
            skin.error()
        } else {
            skin.dim()
        };
        Line::from(vec![
            Span::raw(if selected { "› " } else { "  " }),
            Span::styled("███ ", Style::default().fg(swatch)),
            Span::styled(format!("{key:<22}"), name_style),
            Span::styled(value, val_style),
        ])
    }

    fn render_contrast(&self, f: &mut Frame, area: Rect, skin: &Skin) {
        let get = |k: &str| self.working.get(k);
        let mut spans = vec![Span::styled("contrast  ", skin.dim())];
        for (label, a, b) in [
            ("fg/bg", "foreground", "background"),
            ("accent/bg", "accent", "background"),
        ] {
            if let (Some(x), Some(y)) = (get(a), get(b)) {
                let ratio = contrast_ratio(&x, &y);
                let pass = ratio >= 4.5;
                let style = if pass { skin.ok() } else { skin.warn() };
                spans.push(Span::styled(
                    format!(
                        "{label} {ratio:.1}:1 {}   ",
                        if pass { "AA" } else { "low" }
                    ),
                    style,
                ));
            }
        }
        f.render_widget(Paragraph::new(Line::from(spans)), area);
    }
}

fn pc(c: PColor) -> Color {
    Color::Rgb(c.r, c.g, c.b)
}
