//! Themes screen (spec 07 §8, PRD M1): browse with swatch strips, apply, fork.
//! First real Studio screen — the template the others follow.
//!
//! Wide terminals get a live preview pane: the selected theme's wallpaper
//! plus a mock terminal window drawn entirely in that theme's palette — how
//! the desktop would *feel*, before applying anything.

use std::path::PathBuf;

use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Paragraph};
use ratatui::Frame;

use studio_core::modules::themes::{Theme, ThemeOrigin, ThemeStore, PALETTE_KEYS};
use studio_core::omarchy::OmarchyPaths;

use crate::tui::imagecell::ImageCell;
use crate::tui::theme::Skin;

/// Everything the preview pane needs, loaded once per theme (not per frame).
struct Preview {
    bg: Color,
    fg: Color,
    accent: Color,
    sel_bg: Color,
    ansi: Vec<Color>,
    wallpaper: Option<PathBuf>,
    light: bool,
    bg_hex: String,
}

/// A pre-rendered browser row (swatches loaded once, not per frame).
struct Row {
    slug: String,
    pretty: String,
    origin: ThemeOrigin,
    light: bool,
    swatch: Vec<Color>,
    preview: Option<Preview>,
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

    pub fn render(&self, f: &mut Frame, area: Rect, skin: &Skin, images: &mut ImageCell) {
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

        // Wide terminals: list left, live theme preview right.
        let (list_area, preview_area) = if area.width >= 82 {
            let cols = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([Constraint::Length(44), Constraint::Min(30)])
                .split(area);
            (cols[0], Some(cols[1]))
        } else {
            (area, None)
        };

        // Scroll the viewport so the selection always stays on screen.
        let h = list_area.height as usize;
        let offset = (self.selected + 1).saturating_sub(h);
        let lines: Vec<Line> = rows
            .iter()
            .enumerate()
            .skip(offset)
            .take(h.max(1))
            .map(|(i, r)| self.render_row(r, i == self.selected, skin))
            .collect();
        f.render_widget(Paragraph::new(lines), list_area);

        if let Some(pane) = preview_area {
            self.render_preview(f, pane, skin, images);
        }

        if let Some(buf) = &self.fork_input {
            self.render_fork_modal(f, area, buf, skin);
        }
    }

    /// The selected theme, mocked up in its own colors: wallpaper thumb on
    /// top (when the terminal can draw it), a fake terminal window below,
    /// and the full ANSI strip — Omarchy's picker idea, in cells.
    fn render_preview(&self, f: &mut Frame, area: Rect, skin: &Skin, images: &mut ImageCell) {
        let Some(row) = self.visible().get(self.selected).copied() else {
            return;
        };
        let block = Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(skin.border())
            .title(Span::styled(format!(" {} ", row.pretty), skin.dim()));
        let inner = block.inner(area);
        f.render_widget(block, area);
        let Some(p) = &row.preview else {
            f.render_widget(
                Paragraph::new("this theme has no colors.toml to preview").style(skin.dim()),
                inner,
            );
            return;
        };

        // wallpaper thumb above the mock when there is room for both
        let (wall_area, mock_area) = match &p.wallpaper {
            Some(_) if inner.height >= 15 => {
                let split = Layout::default()
                    .direction(Direction::Vertical)
                    .constraints([
                        Constraint::Length(inner.height.saturating_sub(11)),
                        Constraint::Length(11),
                    ])
                    .split(inner);
                (Some(split[0]), split[1])
            }
            _ => (None, inner),
        };
        if let (Some(wa), Some(wp)) = (wall_area, &p.wallpaper) {
            images.render(f, wa, wp);
        }

        // mock terminal window, every cell in the theme's palette
        let on_bg = |c: Color| Style::default().fg(c).bg(p.bg);
        let w = (mock_area.width as usize).saturating_sub(2).clamp(10, 34);
        let bar = "─".repeat(w.saturating_sub(2));
        // room between the two │ borders
        let iw = w.saturating_sub(2);
        let pad = move |s: &str| format!("{s:<iw$}");
        let framed = |spans: Vec<Span<'static>>, p: &Preview| {
            let mut v = vec![Span::styled("│", on_bg(p.accent))];
            v.extend(spans);
            v.push(Span::styled("│", on_bg(p.accent)));
            Line::from(v)
        };
        let color = |i: usize, p: &Preview| p.ansi.get(i).copied().unwrap_or(p.fg);
        let mut lines: Vec<Line> = vec![
            Line::from(Span::styled(format!("╭{bar}╮"), on_bg(p.accent))),
            framed(
                vec![
                    Span::styled(" ❯ ".to_string(), on_bg(p.accent)),
                    Span::styled(
                        format!("{:<jw$}", "omarchy-studio", jw = iw.saturating_sub(3)),
                        on_bg(p.fg),
                    ),
                ],
                p,
            ),
            framed(
                vec![Span::styled(
                    pad(" themes  waybar  hypr"),
                    on_bg(color(4, p)),
                )],
                p,
            ),
            framed(
                vec![Span::styled(pad(" ✓ theme applied"), on_bg(color(2, p)))],
                p,
            ),
            framed(
                vec![Span::styled(pad(" ! 2 warnings"), on_bg(color(3, p)))],
                p,
            ),
            framed(
                vec![Span::styled(pad(" ✗ 1 conflict"), on_bg(color(1, p)))],
                p,
            ),
            framed(
                vec![Span::styled(
                    pad(" selected line"),
                    Style::default().fg(p.fg).bg(p.sel_bg),
                )],
                p,
            ),
            Line::from(Span::styled(format!("╰{bar}╯"), on_bg(p.accent))),
        ];
        let mut strip: Vec<Span> = vec![Span::styled(" ", on_bg(p.fg))];
        for c in &p.ansi {
            strip.push(Span::styled("██", Style::default().fg(*c).bg(p.bg)));
        }
        lines.push(Line::from(strip));
        lines.push(Line::from(Span::styled(
            format!(
                " {} · {}",
                if p.light { "light ☀" } else { "dark" },
                p.bg_hex
            ),
            on_bg(p.fg),
        )));
        f.render_widget(
            Paragraph::new(lines).style(Style::default().bg(p.bg)),
            mock_area,
        );
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
    let palette = t.palette().ok();
    // Swatch = accent + background + a spread of the ANSI colors, when present.
    let swatch = palette
        .as_ref()
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
    let preview = palette.as_ref().and_then(|p| {
        let rgb = |k: &str| p.get(k).map(|c| Color::Rgb(c.r, c.g, c.b));
        let bg = rgb("background")?;
        let fg = rgb("foreground")?;
        Some(Preview {
            bg,
            fg,
            accent: rgb("accent").unwrap_or(fg),
            sel_bg: rgb("selection_background").unwrap_or(bg),
            ansi: (0..16).filter_map(|i| rgb(&format!("color{i}"))).collect(),
            wallpaper: first_background(t),
            light: t.light(),
            bg_hex: p.get("background").map(|c| c.hex()).unwrap_or_default(),
        })
    });
    Row {
        slug: t.slug.clone(),
        pretty: t.pretty.clone(),
        origin: t.origin,
        light: t.light(),
        swatch,
        preview,
    }
}

/// First still image in the theme's `backgrounds/` (merged view) — the pane's
/// "what would my desktop look like" half.
fn first_background(t: &Theme) -> Option<PathBuf> {
    let dir = t.file("backgrounds")?;
    let mut files: Vec<PathBuf> = std::fs::read_dir(dir)
        .ok()?
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            let ext = p
                .extension()
                .map(|e| e.to_string_lossy().to_lowercase())
                .unwrap_or_default();
            matches!(ext.as_str(), "jpg" | "jpeg" | "png" | "webp")
        })
        .collect();
    files.sort();
    files.into_iter().next()
}
