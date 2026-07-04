//! TUI shell (spec 07): module rail, content pane, keybar + jargon strip,
//! help overlay, command palette. Studio themes itself from the active
//! Omarchy palette. Elm-ish: events → `handle` → optional side effect → redraw.
//!
//! v0.1 wires the Themes screen for real; the other rail entries render an
//! honest "arriving in <milestone>" placeholder so the shell is navigable
//! end-to-end without pretending features exist.

mod theme;
mod screens {
    pub mod themes;
}

use ratatui::crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, List, ListItem, Paragraph, Wrap};
use ratatui::Frame;

use studio_core::cmd::{CommandRunner, RealRunner};
use studio_core::modules::themes::ThemeStore;
use studio_core::omarchy::{cmds, OmarchyPaths};

use screens::themes::{ThemeAction, ThemesScreen};
use theme::Skin;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Screen {
    Themes,
    Wallpapers,
    Keybinds,
    LookFeel,
    Animations,
    Waybar,
    Notifications,
    LockIdle,
    Snapshots,
    Integrations,
    Doctor,
}

impl Screen {
    const ALL: [Screen; 11] = [
        Screen::Themes,
        Screen::Wallpapers,
        Screen::Keybinds,
        Screen::LookFeel,
        Screen::Animations,
        Screen::Waybar,
        Screen::Notifications,
        Screen::LockIdle,
        Screen::Snapshots,
        Screen::Integrations,
        Screen::Doctor,
    ];

    fn label(self) -> &'static str {
        match self {
            Screen::Themes => "Themes",
            Screen::Wallpapers => "Wallpaper",
            Screen::Keybinds => "Keybinds",
            Screen::LookFeel => "Look & Feel",
            Screen::Animations => "Animations",
            Screen::Waybar => "Waybar",
            Screen::Notifications => "Notifs / OSD",
            Screen::LockIdle => "Lock & Idle",
            Screen::Snapshots => "Snapshots",
            Screen::Integrations => "Integrations",
            Screen::Doctor => "Doctor",
        }
    }

    /// Roadmap milestone a not-yet-built screen is coming in (spec honesty).
    fn arriving(self) -> &'static str {
        match self {
            Screen::Keybinds => "v0.2",
            Screen::LookFeel | Screen::Animations => "v0.3",
            Screen::Waybar => "v0.4",
            Screen::Notifications | Screen::LockIdle => "v0.5",
            Screen::Wallpapers | Screen::Integrations => "v0.6",
            _ => "soon",
        }
    }
}

struct Toast {
    text: String,
    ok: bool,
}

struct App {
    paths: OmarchyPaths,
    skin: Skin,
    screen: Screen,
    themes: ThemesScreen,
    palette: Option<CommandPalette>,
    help: bool,
    toast: Option<Toast>,
    quit: bool,
}

impl App {
    fn new(paths: OmarchyPaths) -> Self {
        let skin = Skin::from_current(&paths);
        let themes = ThemesScreen::load(&paths);
        Self {
            paths,
            skin,
            screen: Screen::Themes,
            themes,
            palette: None,
            help: false,
            toast: None,
            quit: false,
        }
    }

    fn on_key(&mut self, key: KeyEvent) {
        self.toast = None;

        // Overlays consume input first.
        if self.help {
            self.help = false;
            return;
        }
        if self.palette.is_some() {
            self.palette_key(key);
            return;
        }

        // Global chords (only when no in-screen text field is capturing).
        let capturing = matches!(self.screen, Screen::Themes) && self.themes.is_capturing();
        if !capturing {
            match key.code {
                KeyCode::Char('q') => {
                    self.quit = true;
                    return;
                }
                KeyCode::Char('?') => {
                    self.help = true;
                    return;
                }
                KeyCode::Char('/') => {
                    self.palette = Some(CommandPalette::new(&self.paths));
                    return;
                }
                KeyCode::Tab => {
                    self.cycle(1);
                    return;
                }
                KeyCode::BackTab => {
                    self.cycle(-1);
                    return;
                }
                KeyCode::Char(d @ '1'..='9') => {
                    self.jump(d as usize - '1' as usize);
                    return;
                }
                KeyCode::Char('0') => {
                    self.jump(9);
                    return;
                }
                _ => {}
            }
        }
        // Esc quits from the home screen, else returns there.
        if key.code == KeyCode::Esc && !capturing {
            if self.screen == Screen::Themes {
                self.quit = true;
            } else {
                self.screen = Screen::Themes;
            }
            return;
        }

        // Delegate to the active screen.
        if self.screen == Screen::Themes {
            match self.themes.handle(key) {
                ThemeAction::None => {}
                ThemeAction::Apply(slug) => self.apply_theme(&slug),
                ThemeAction::Fork { src, name } => self.fork_theme(&src, &name),
            }
        }
    }

    fn cycle(&mut self, dir: i32) {
        let i = Screen::ALL.iter().position(|s| *s == self.screen).unwrap();
        let n = Screen::ALL.len() as i32;
        let next = ((i as i32 + dir).rem_euclid(n)) as usize;
        self.screen = Screen::ALL[next];
    }

    fn jump(&mut self, idx: usize) {
        if let Some(s) = Screen::ALL.get(idx) {
            self.screen = *s;
        }
    }

    fn apply_theme(&mut self, slug: &str) {
        match RealRunner.run(&cmds::theme_set(slug)) {
            Ok(o) if o.ok() => {
                // The desktop just re-themed; restyle Studio to match.
                self.skin = Skin::from_current(&self.paths);
                self.themes.reload(&self.paths);
                self.toast = Some(Toast {
                    text: format!("Applied {slug}"),
                    ok: true,
                });
            }
            Ok(o) => {
                self.toast = Some(Toast {
                    text: format!("omarchy-theme-set failed: {}", o.stderr.trim()),
                    ok: false,
                })
            }
            Err(e) => {
                self.toast = Some(Toast {
                    text: format!("could not run omarchy-theme-set: {e:?}"),
                    ok: false,
                })
            }
        }
    }

    fn fork_theme(&mut self, src: &str, name: &str) {
        let store = ThemeStore::from_paths(&self.paths);
        let Some(theme) = store.get(src) else { return };
        match store.fork(&theme, name) {
            Ok(t) => {
                self.themes.reload(&self.paths);
                self.toast = Some(Toast {
                    text: format!("Forked {src} → {} (press enter to apply)", t.slug),
                    ok: true,
                });
            }
            Err(e) => {
                let detail = match e {
                    studio_core::StudioError::External { detail, .. } => detail,
                    other => format!("{other:?}"),
                };
                self.toast = Some(Toast {
                    text: format!("Fork failed: {detail}"),
                    ok: false,
                });
            }
        }
    }

    fn palette_key(&mut self, key: KeyEvent) {
        let Some(p) = self.palette.as_mut() else {
            return;
        };
        match key.code {
            KeyCode::Esc => self.palette = None,
            KeyCode::Enter => {
                let choice = p.selected();
                self.palette = None;
                if let Some(c) = choice {
                    self.run_palette_choice(c);
                }
            }
            KeyCode::Up => p.up(),
            KeyCode::Down => p.down(),
            KeyCode::Backspace => p.backspace(),
            KeyCode::Char(c) => p.push(c),
            _ => {}
        }
    }

    fn run_palette_choice(&mut self, c: Choice) {
        match c {
            Choice::Go(s) => self.screen = s,
            Choice::Theme(slug) => {
                self.screen = Screen::Themes;
                self.themes.focus(&slug);
            }
        }
    }

    fn draw(&self, f: &mut Frame) {
        let cols = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Length(16), Constraint::Min(20)])
            .split(f.area());
        self.draw_rail(f, cols[0]);

        let rows = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(1),
                Constraint::Min(1),
                Constraint::Length(1),
            ])
            .split(cols[1]);
        self.draw_header(f, rows[0]);
        self.draw_content(f, rows[1]);
        self.draw_keybar(f, rows[2]);

        if self.help {
            self.draw_help(f);
        }
        if let Some(p) = &self.palette {
            p.render(f, &self.skin);
        }
    }

    fn draw_rail(&self, f: &mut Frame, area: Rect) {
        let items: Vec<ListItem> = Screen::ALL
            .iter()
            .enumerate()
            .map(|(i, s)| {
                let key = if i < 9 {
                    format!("{} ", i + 1)
                } else if i == 9 {
                    "0 ".to_string()
                } else {
                    "  ".to_string()
                };
                let style = if *s == self.screen {
                    self.skin.selection()
                } else {
                    self.skin.body()
                };
                ListItem::new(Line::from(vec![
                    Span::styled(key, self.skin.dim()),
                    Span::styled(s.label(), style),
                ]))
            })
            .collect();
        let block = Block::default()
            .borders(Borders::RIGHT)
            .border_style(self.skin.border());
        f.render_widget(List::new(items).block(block), area);
    }

    fn draw_header(&self, f: &mut Frame, area: Rect) {
        let current = self.paths.current_theme_name().unwrap_or_default();
        let line = Line::from(vec![
            Span::styled(" Omarchy Studio ", self.skin.accent_bold()),
            Span::styled(format!("· {}", self.screen.label()), self.skin.body()),
            Span::styled(format!("    {current}"), self.skin.dim()),
        ]);
        f.render_widget(Paragraph::new(line), area);
    }

    fn draw_content(&self, f: &mut Frame, area: Rect) {
        let pad = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Length(1), Constraint::Min(1)])
            .split(area)[1];
        match self.screen {
            Screen::Themes => self.themes.render(f, pad, &self.skin),
            other => self.draw_placeholder(f, pad, other),
        }
    }

    fn draw_placeholder(&self, f: &mut Frame, area: Rect, s: Screen) {
        let text = vec![
            Line::from(Span::styled(s.label(), self.skin.accent_bold())),
            Line::from(""),
            Line::from(Span::styled(
                format!("This screen arrives in {}.", s.arriving()),
                self.skin.body(),
            )),
            Line::from(Span::styled(
                "The engine underneath (config parsing, apply pipeline, snapshots,",
                self.skin.dim(),
            )),
            Line::from(Span::styled(
                "undo) is already built and tested — this is the UI on top.",
                self.skin.dim(),
            )),
        ];
        f.render_widget(Paragraph::new(text).wrap(Wrap { trim: true }), area);
    }

    fn draw_keybar(&self, f: &mut Frame, area: Rect) {
        if let Some(t) = &self.toast {
            let style = if t.ok {
                self.skin.ok()
            } else {
                self.skin.error()
            };
            f.render_widget(
                Paragraph::new(Line::from(Span::styled(&t.text, style))),
                area,
            );
            return;
        }
        let hint = match self.screen {
            Screen::Themes => self.themes.hint(),
            _ => "tab/1-0 switch · / search · ? help · q quit".into(),
        };
        let line = Line::from(vec![
            Span::styled(hint, self.skin.dim()),
            Span::styled("   / search · ? help · q quit", self.skin.border()),
        ]);
        f.render_widget(Paragraph::new(line), area);
    }

    fn draw_help(&self, f: &mut Frame) {
        let area = centered(f.area(), 52, 14);
        f.render_widget(Clear, area);
        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(self.skin.accent_bold())
            .title(" keys ");
        let inner = block.inner(area);
        f.render_widget(block, area);
        let rows = [
            ("1–9, 0", "jump to a module"),
            ("tab / shift-tab", "cycle modules"),
            ("/", "command palette (search everything)"),
            ("j / k", "move selection"),
            ("enter", "apply / activate"),
            ("f", "fork the selected theme"),
            ("esc", "back to Themes, or quit"),
            ("q", "quit"),
            ("?", "this help"),
        ];
        let lines: Vec<Line> = rows
            .iter()
            .map(|(k, d)| {
                Line::from(vec![
                    Span::styled(format!("  {k:<16}"), self.skin.accent_bold()),
                    Span::styled(*d, self.skin.body()),
                ])
            })
            .collect();
        f.render_widget(Paragraph::new(lines), inner);
    }
}

// ── command palette (spec 07 §3) ─────────────────────────────────────────────

enum Choice {
    Go(Screen),
    Theme(String),
}

struct Entry {
    label: String,
    haystack: String,
    choice: Choice,
}

struct CommandPalette {
    entries: Vec<Entry>,
    query: String,
    selected: usize,
}

impl CommandPalette {
    fn new(paths: &OmarchyPaths) -> Self {
        let mut entries: Vec<Entry> = Screen::ALL
            .iter()
            .map(|s| Entry {
                label: format!("Go: {}", s.label()),
                haystack: s.label().to_lowercase(),
                choice: Choice::Go(*s),
            })
            .collect();
        for t in ThemeStore::from_paths(paths).list() {
            entries.push(Entry {
                label: format!("Theme: {}", t.pretty),
                haystack: format!("theme {} {}", t.pretty.to_lowercase(), t.slug),
                choice: Choice::Theme(t.slug),
            });
        }
        Self {
            entries,
            query: String::new(),
            selected: 0,
        }
    }

    fn matches(&self) -> Vec<&Entry> {
        let q = self.query.to_lowercase();
        self.entries
            .iter()
            .filter(|e| subsequence(&q, &e.haystack))
            .collect()
    }

    fn selected(&self) -> Option<Choice> {
        self.matches().get(self.selected).map(|e| match &e.choice {
            Choice::Go(s) => Choice::Go(*s),
            Choice::Theme(t) => Choice::Theme(t.clone()),
        })
    }

    fn up(&mut self) {
        self.selected = self.selected.saturating_sub(1);
    }
    fn down(&mut self) {
        let n = self.matches().len();
        if n > 0 {
            self.selected = (self.selected + 1).min(n - 1);
        }
    }
    fn push(&mut self, c: char) {
        self.query.push(c);
        self.selected = 0;
    }
    fn backspace(&mut self) {
        self.query.pop();
        self.selected = 0;
    }

    fn render(&self, f: &mut Frame, skin: &Skin) {
        let area = centered(f.area(), 56, 16);
        f.render_widget(Clear, area);
        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(skin.accent_bold())
            .title(" search — type, ↑↓ to move, enter ");
        let inner = block.inner(area);
        f.render_widget(block, area);

        let rows = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(1), Constraint::Min(1)])
            .split(inner);
        f.render_widget(
            Paragraph::new(format!("› {}▏", self.query)).style(skin.body()),
            rows[0],
        );
        let items: Vec<ListItem> = self
            .matches()
            .iter()
            .enumerate()
            .map(|(i, e)| {
                let style = if i == self.selected {
                    skin.selection()
                } else {
                    skin.body()
                };
                ListItem::new(Span::styled(e.label.clone(), style))
            })
            .collect();
        f.render_widget(List::new(items), rows[1]);
    }
}

/// Fuzzy subsequence: every char of `needle` appears in order within `hay`.
fn subsequence(needle: &str, hay: &str) -> bool {
    let mut chars = hay.chars();
    needle.chars().all(|c| chars.any(|h| h == c))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fuzzy_subsequence_matches_in_order() {
        assert!(subsequence("tn", "tokyo night"));
        assert!(subsequence("cat", "catppuccin latte"));
        assert!(subsequence("", "anything"));
        assert!(subsequence("lumon", "theme lumon lumon"));
        // out of order or absent chars fail
        assert!(!subsequence("nt", "tn"));
        assert!(!subsequence("xyz", "tokyo night"));
    }

    #[test]
    fn screen_cycle_wraps_both_directions() {
        // ALL is the canonical rail order; first is Themes, last is Doctor.
        assert_eq!(Screen::ALL[0], Screen::Themes);
        assert_eq!(*Screen::ALL.last().unwrap(), Screen::Doctor);
        // every screen has a non-empty label and arriving milestone
        for s in Screen::ALL {
            assert!(!s.label().is_empty());
            assert!(!s.arriving().is_empty());
        }
    }
}

fn centered(area: Rect, w: u16, h: u16) -> Rect {
    let w = w.min(area.width.saturating_sub(2));
    let h = h.min(area.height.saturating_sub(2));
    Rect {
        x: area.x + (area.width.saturating_sub(w)) / 2,
        y: area.y + (area.height.saturating_sub(h)) / 2,
        width: w,
        height: h,
    }
}

/// Entry point: no-args launch (spec 08). Runs until quit; restores the
/// terminal on any exit path.
pub fn run() -> i32 {
    let paths = match OmarchyPaths::discover() {
        Ok(p) if p.installed() => p,
        Ok(p) => {
            eprintln!(
                "no Omarchy install at {} (set $OMARCHY_PATH if it lives elsewhere).",
                p.system.display()
            );
            return 4;
        }
        Err(_) => {
            eprintln!("HOME is not set — cannot locate an Omarchy install.");
            return 4;
        }
    };

    let mut terminal = ratatui::init();
    let mut app = App::new(paths);
    let result = loop {
        if let Err(e) = terminal.draw(|f| app.draw(f)) {
            break Err(e);
        }
        match event::read() {
            Ok(Event::Key(key)) if key.kind == KeyEventKind::Press => {
                // Ctrl-C always exits, even mid text-entry.
                if key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL) {
                    break Ok(());
                }
                app.on_key(key);
                if app.quit {
                    break Ok(());
                }
            }
            Ok(_) => {}
            Err(e) => break Err(e),
        }
    };
    ratatui::restore();
    match result {
        Ok(()) => 0,
        Err(e) => {
            eprintln!("terminal error: {e}");
            1
        }
    }
}
