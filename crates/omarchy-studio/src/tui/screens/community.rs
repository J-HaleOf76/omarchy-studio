//! Community themes browser (ROADMAP 0.9.1) — `c` in the Themes screen.
//!
//! Browses the vendored extra-themes directory with a pre-install preview
//! built like the Themes screen's: selecting a theme fetches its wallpaper
//! and its raw `colors.toml` off GitHub on a worker thread and renders the
//! thumb over swatch strips before anything touches disk — the
//! look-before-you-install step `omarchy-theme-install` alone can't give.
//! Installing shells to `omarchy-theme-install`, which clones the repo *and
//! applies the theme immediately* — the keybar says so.
//!
//! Same worker discipline as the wallhaven browser: one thread owns the
//! network and the install command, the UI sends [`Job`]s and drains
//! [`Outcome`]s, and the event loop polls only while something is in
//! flight. Wallpapers land in an on-disk cache beside the wallhaven thumbs,
//! because listing a repo's `backgrounds/` costs a GitHub API call and those
//! are capped at 60 an hour unauthenticated.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::mpsc::{channel, Receiver, Sender, TryRecvError};

use ratatui::crossterm::event::{KeyCode, KeyEvent};
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Clear, List, ListItem, Padding, Paragraph};
use ratatui::Frame;

use studio_core::cmd::{CommandRunner, RealRunner};
use studio_core::modules::community::{self, CommunityTheme};
use studio_core::modules::themes::Palette;
use studio_core::modules::wallhaven::{Fetch, FetchError, HttpFetch, ThumbCache};
use studio_core::omarchy::cmds;

use super::super::imagecell::ImageCell;
use super::super::theme::Skin;

/// Wallpaper cache cap — one still per theme, 114 themes in the directory.
const WALL_CACHE_CAP: u64 = 100 * 1024 * 1024;

/// Rows the palette + repo + install note need under a wallpaper thumb.
const TEXT_ROWS: u16 = 7;

enum Job {
    Preview(CommunityTheme),
    Wallpaper(CommunityTheme),
    Install(CommunityTheme),
}

enum Outcome {
    Preview {
        repo: String,
        result: Result<String, String>,
    },
    Wallpaper {
        repo: String,
        result: Result<PathBuf, String>,
    },
    Installed {
        name: String,
        slug: String,
        result: Result<(), String>,
    },
}

pub enum CommunityAction {
    None,
    Close,
}

/// A finished install, surfaced to the App (toast + themes reload — the
/// install script applies the theme, so `current` changed too).
pub struct InstallDone {
    pub name: String,
    pub slug: String,
}

/// What we know about a theme's palette after trying to fetch it.
enum PreviewState {
    Ready(Box<PalettePreview>),
    Missing(String),
}

/// What we know about a theme's wallpaper after trying to fetch it.
enum WallState {
    Ready(PathBuf),
    Missing(String),
}

/// The selected theme's `colors.toml`, parsed into renderable cells.
struct PalettePreview {
    bg: Color,
    fg: Color,
    accent: Color,
    ansi: Vec<Color>,
    bg_hex: String,
    light: bool,
}

fn worker(jobs: Receiver<Job>, out: Sender<Outcome>) {
    let fetch = HttpFetch::new();
    let cache = ThumbCache::new(
        studio_core::studio_cache_dir().join("community"),
        WALL_CACHE_CAP,
    );
    for job in jobs {
        let outcome = match job {
            Job::Preview(t) => {
                let mut result = Err("no colors.toml on main or master".to_string());
                for url in t.colors_urls() {
                    match fetch.get(&url) {
                        Ok(bytes) => {
                            result = String::from_utf8(bytes)
                                .map_err(|_| "colors.toml is not UTF-8".to_string());
                            break;
                        }
                        Err(FetchError::Status(404)) => continue,
                        Err(FetchError::Status(code)) => {
                            result = Err(format!("HTTP {code}"));
                            break;
                        }
                        Err(FetchError::Network(e)) => {
                            result = Err(e);
                            break;
                        }
                    }
                }
                Outcome::Preview {
                    repo: t.repo,
                    result,
                }
            }
            Job::Wallpaper(t) => {
                let result = match &cache {
                    Ok(c) => wallpaper(&fetch, c, &t),
                    Err(e) => Err(format!("no wallpaper cache: {e:?}")),
                };
                Outcome::Wallpaper {
                    repo: t.repo,
                    result,
                }
            }
            Job::Install(t) => {
                let result = match RealRunner.run(&cmds::theme_install(&t.clone_url())) {
                    Ok(o) if o.ok() => Ok(()),
                    Ok(o) => {
                        let err = o.stderr.trim();
                        Err(if err.is_empty() {
                            "omarchy-theme-install failed".to_string()
                        } else {
                            err.to_string()
                        })
                    }
                    Err(e) => Err(brief(e)),
                };
                let slug = t.install_slug();
                Outcome::Installed {
                    name: t.name,
                    slug,
                    result,
                }
            }
        };
        if out.send(outcome).is_err() {
            return; // browser closed mid-request
        }
    }
}

/// Cache key for a theme's wallpaper — the repo, flattened to one path
/// segment (`owner/repo` → `owner-repo`).
fn wall_id(repo: &str) -> String {
    repo.replace('/', "-")
}

/// A theme's wallpaper, from the cache or off GitHub: list `backgrounds/`
/// through the contents API, take the first still, download it, cache it.
/// Every failure is a sentence for the preview pane, never fatal — a theme
/// with no wallpaper still previews its palette.
fn wallpaper(fetch: &dyn Fetch, cache: &ThumbCache, t: &CommunityTheme) -> Result<PathBuf, String> {
    let id = wall_id(&t.repo);
    if let Some(hit) = cache.get(&id) {
        return Ok(hit);
    }
    let listing = fetch.get(&t.backgrounds_url()).map_err(|e| match e {
        FetchError::Status(404) => "this theme ships no backgrounds/".to_string(),
        // The API allows 60 anonymous calls an hour, per IP.
        FetchError::Status(403) | FetchError::Status(429) => {
            "GitHub API rate limit — wallpapers return within the hour".to_string()
        }
        FetchError::Status(code) => format!("HTTP {code}"),
        FetchError::Network(e) => e,
    })?;
    let listing = String::from_utf8(listing).map_err(|_| "listing is not UTF-8".to_string())?;
    let bg = community::first_background(&listing)
        .ok_or_else(|| "no still wallpaper in backgrounds/".to_string())?;
    let bytes = fetch.get(&bg.download_url).map_err(|e| match e {
        FetchError::Status(code) => format!("HTTP {code} fetching {}", bg.name),
        FetchError::Network(e) => e,
    })?;
    cache
        .put(&id, &bg.ext(), &bytes)
        .map_err(|e| format!("caching {}: {}", bg.name, brief(e)))
}

fn brief(e: studio_core::StudioError) -> String {
    match e {
        studio_core::StudioError::External { detail, .. } => detail,
        studio_core::StudioError::ParseFailed { hint, .. } => hint,
        other => format!("{other:?}"),
    }
}

pub struct CommunityBrowser {
    rows: Vec<CommunityTheme>,
    installed: HashSet<String>,
    /// Applied filter; `/` edits a copy in `input` until Enter.
    filter: String,
    input: Option<String>,
    selected: usize,
    previews: HashMap<String, PreviewState>,
    preview_inflight: Option<String>,
    walls: HashMap<String, WallState>,
    wall_inflight: Option<String>,
    /// Name of the theme currently being installed, if any.
    installing: Option<String>,
    error: Option<String>,
    jobs: Sender<Job>,
    results: Receiver<Outcome>,
}

impl CommunityBrowser {
    /// Open over the Themes screen. `installed` is the store's current slug
    /// set, so directory entries already on disk are marked.
    pub fn open(installed: HashSet<String>) -> Self {
        let (jobs_tx, jobs_rx) = channel();
        let (out_tx, out_rx) = channel();
        std::thread::spawn(move || worker(jobs_rx, out_tx));
        let mut b = Self::with_channels(jobs_tx, out_rx, installed);
        b.ensure_selected();
        b
    }

    /// Everything but the worker thread — what tests drive directly.
    fn with_channels(
        jobs: Sender<Job>,
        results: Receiver<Outcome>,
        installed: HashSet<String>,
    ) -> Self {
        Self {
            rows: community::catalog(),
            installed,
            filter: String::new(),
            input: None,
            selected: 0,
            previews: HashMap::new(),
            preview_inflight: None,
            walls: HashMap::new(),
            wall_inflight: None,
            installing: None,
            error: None,
            jobs,
            results,
        }
    }

    /// Is anything in flight? Drives the event loop's poll-vs-block choice.
    pub fn busy(&self) -> bool {
        self.installing.is_some() || self.preview_inflight.is_some() || self.wall_inflight.is_some()
    }

    fn visible(&self) -> Vec<&CommunityTheme> {
        self.rows
            .iter()
            .filter(|t| self.filter.is_empty() || community::matches(t, &self.filter))
            .collect()
    }

    fn selected_theme(&self) -> Option<CommunityTheme> {
        self.visible().get(self.selected).map(|t| (*t).clone())
    }

    /// Make sure the selected theme's palette and wallpaper are on their
    /// way: known states are used immediately, a miss becomes a worker job
    /// (one of each kind at a time, so scrolling can't flood the queue).
    fn ensure_selected(&mut self) {
        let Some(t) = self.selected_theme() else {
            return;
        };
        if !self.previews.contains_key(&t.repo) && self.preview_inflight.as_deref() != Some(&t.repo)
        {
            self.preview_inflight = Some(t.repo.clone());
            let _ = self.jobs.send(Job::Preview(t.clone()));
        }
        if !self.walls.contains_key(&t.repo) && self.wall_inflight.as_deref() != Some(&t.repo) {
            self.wall_inflight = Some(t.repo.clone());
            let _ = self.jobs.send(Job::Wallpaper(t));
        }
    }

    /// Drain worker outcomes. Returns a finished install for the App to act
    /// on (toast + Themes reload); everything else lands in browser state.
    pub fn drain(&mut self) -> Option<InstallDone> {
        let mut done = None;
        loop {
            match self.results.try_recv() {
                Ok(Outcome::Preview { repo, result }) => {
                    if self.preview_inflight.as_deref() == Some(&repo) {
                        self.preview_inflight = None;
                    }
                    let state = match result {
                        Ok(text) => match preview_of(&text) {
                            Ok(p) => PreviewState::Ready(Box::new(p)),
                            Err(e) => PreviewState::Missing(e),
                        },
                        Err(e) => PreviewState::Missing(e),
                    };
                    self.previews.insert(repo, state);
                    // Selection may have moved while this one was in flight.
                    self.ensure_selected();
                }
                Ok(Outcome::Wallpaper { repo, result }) => {
                    if self.wall_inflight.as_deref() == Some(&repo) {
                        self.wall_inflight = None;
                    }
                    let state = match result {
                        Ok(path) => WallState::Ready(path),
                        Err(e) => WallState::Missing(e),
                    };
                    self.walls.insert(repo, state);
                    self.ensure_selected();
                }
                Ok(Outcome::Installed { name, slug, result }) => {
                    self.installing = None;
                    match result {
                        Ok(()) => {
                            self.installed.insert(slug.clone());
                            done = Some(InstallDone { name, slug });
                        }
                        Err(e) => self.error = Some(format!("install failed: {e}")),
                    }
                }
                Err(TryRecvError::Empty) | Err(TryRecvError::Disconnected) => break,
            }
        }
        done
    }

    pub fn handle(&mut self, key: KeyEvent) -> CommunityAction {
        // Filter text entry owns every key while open.
        if let Some(buf) = self.input.as_mut() {
            match key.code {
                KeyCode::Enter => {
                    self.filter = self.input.take().unwrap_or_default();
                    self.selected = 0;
                    self.error = None;
                    self.ensure_selected();
                }
                KeyCode::Esc => self.input = None,
                KeyCode::Backspace => {
                    buf.pop();
                }
                KeyCode::Char(c) => buf.push(c),
                _ => {}
            }
            return CommunityAction::None;
        }

        let n = self.visible().len();
        if crate::tui::ui::list_nav(key.code, &mut self.selected, n) {
            self.ensure_selected();
            return CommunityAction::None;
        }
        match key.code {
            KeyCode::Esc | KeyCode::Char('q') => return CommunityAction::Close,
            KeyCode::Char('/') => self.input = Some(self.filter.clone()),
            KeyCode::Enter => self.install(),
            _ => {}
        }
        CommunityAction::None
    }

    fn install(&mut self) {
        if self.installing.is_some() {
            return;
        }
        let Some(t) = self.selected_theme() else {
            return;
        };
        self.error = None;
        self.installing = Some(t.name.clone());
        let _ = self.jobs.send(Job::Install(t));
    }

    fn status(&self) -> String {
        if let Some(name) = &self.installing {
            return format!("installing {name}… (clones the repo, then applies)");
        }
        let n = self.visible().len();
        if self.filter.is_empty() {
            format!("{n} themes")
        } else {
            format!("{n} of {} themes", self.rows.len())
        }
    }

    pub fn render(&self, f: &mut Frame, area: Rect, skin: &Skin, images: &mut ImageCell) {
        f.render_widget(Clear, area);
        let block = Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(skin.accent_bold())
            .title(Line::from(vec![
                Span::styled(" ✦ ", skin.accent_bold()),
                Span::styled("community themes ", skin.body()),
            ]))
            .title_top(
                Line::from(Span::styled(format!(" {} ", self.status()), skin.dim()))
                    .right_aligned(),
            )
            .title_bottom(
                Line::from(Span::styled(
                    " / filter · ⏎ install + apply · esc ",
                    skin.dim(),
                ))
                .right_aligned(),
            )
            .padding(Padding::horizontal(1));
        let inner = block.inner(area);
        f.render_widget(block, area);

        let rows = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(1), // filter line
                Constraint::Length(1), // spacer / error banner
                Constraint::Min(1),    // list + preview
            ])
            .split(inner);

        let f_line = match &self.input {
            Some(buf) => Line::from(vec![
                Span::styled(" filter ", skin.accent_bold()),
                Span::styled(buf.clone(), skin.body()),
                Span::styled("▏", skin.accent_bold()),
            ]),
            None => Line::from(vec![
                Span::styled(" filter ", skin.dim()),
                Span::styled(
                    if self.filter.is_empty() {
                        "(all — / to type)".to_string()
                    } else {
                        self.filter.clone()
                    },
                    skin.body(),
                ),
            ]),
        };
        f.render_widget(Paragraph::new(f_line), rows[0]);
        if let Some(err) = &self.error {
            f.render_widget(
                Paragraph::new(Span::styled(format!(" {err}"), skin.error())),
                rows[1],
            );
        }

        let cols = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Length(40), Constraint::Min(20)])
            .split(rows[2]);
        self.render_list(f, cols[0], skin);
        self.render_preview(f, cols[1], skin, images);
    }

    fn render_list(&self, f: &mut Frame, area: Rect, skin: &Skin) {
        let visible = self.visible();
        if visible.is_empty() {
            f.render_widget(
                Paragraph::new(Span::styled(" nothing matches the filter", skin.dim())),
                area,
            );
            return;
        }
        // Keep the selection inside the viewport.
        let h = area.height as usize;
        let offset = (self.selected + 1).saturating_sub(h);
        let items: Vec<ListItem> = visible
            .iter()
            .enumerate()
            .skip(offset)
            .take(h)
            .map(|(i, t)| {
                let sel = i == self.selected;
                let on_disk = self.installed.contains(&t.install_slug());
                let spans = vec![
                    Span::styled(if sel { "▌" } else { " " }.to_string(), skin.accent_bold()),
                    Span::styled(
                        if on_disk { "● " } else { "  " }.to_string(),
                        skin.accent_bold(),
                    ),
                    Span::styled(
                        format!("{:<36}", t.name),
                        if sel { skin.accent_bold() } else { skin.body() },
                    ),
                ];
                let item = ListItem::new(Line::from(spans));
                if sel {
                    item.style(Style::default().bg(skin.sel_bg))
                } else {
                    item
                }
            })
            .collect();
        f.render_widget(List::new(items), area);
    }

    fn render_preview(&self, f: &mut Frame, area: Rect, skin: &Skin, images: &mut ImageCell) {
        let Some(t) = self.selected_theme() else {
            return;
        };
        let on_disk = self.installed.contains(&t.install_slug());
        let block = Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(skin.border())
            .title(Span::styled(format!(" {} ", t.name), skin.dim()));
        let inner = block.inner(area);
        f.render_widget(block, area);

        // The theme's wallpaper above its palette, like the Themes screen —
        // colours alone don't tell you what a rice looks like. Falls back to
        // palette-only when the pane is short or the wallpaper never landed.
        let wall = match self.walls.get(&t.repo) {
            Some(WallState::Ready(p)) => Some(p),
            _ => None,
        };
        let (wall_area, text_area) = match wall {
            Some(_) if inner.height >= TEXT_ROWS + 5 => {
                let split = Layout::default()
                    .direction(Direction::Vertical)
                    .constraints([Constraint::Min(3), Constraint::Length(TEXT_ROWS)])
                    .split(inner);
                (Some(split[0]), split[1])
            }
            _ => (None, inner),
        };
        if let (Some(wa), Some(wp)) = (wall_area, wall) {
            images.render(f, wa, wp);
        }

        let mut lines: Vec<Line> = Vec::new();
        match self.previews.get(&t.repo) {
            Some(PreviewState::Ready(p)) => {
                let on_bg = |c: Color| Style::default().fg(c).bg(p.bg);
                // The palette on its own background: label rows + ANSI strip.
                lines.push(Line::from(Span::styled(
                    " background  foreground  accent ".to_string(),
                    on_bg(p.fg),
                )));
                lines.push(Line::from(vec![
                    Span::styled(" ████ ", on_bg(p.bg)),
                    Span::styled("      ████ ", on_bg(p.fg)),
                    Span::styled("    ████ ", on_bg(p.accent)),
                ]));
                let mut strip: Vec<Span> = vec![Span::styled(" ", on_bg(p.fg))];
                for c in &p.ansi {
                    strip.push(Span::styled("██", Style::default().fg(*c).bg(p.bg)));
                }
                lines.push(Line::from(strip));
                lines.push(Line::from(Span::styled(
                    format!(" {} · {}", if p.light { "light" } else { "dark" }, p.bg_hex),
                    on_bg(p.fg),
                )));
            }
            Some(PreviewState::Missing(e)) => {
                lines.push(Line::from(Span::styled(
                    format!("no palette preview: {e}"),
                    skin.dim(),
                )));
            }
            None => {
                lines.push(Line::from(Span::styled(
                    if self.preview_inflight.as_deref() == Some(&t.repo) {
                        "fetching colors.toml…"
                    } else {
                        "no preview yet"
                    },
                    skin.dim(),
                )));
            }
        }
        // Only worth a line when there's no thumb to speak for itself.
        if wall_area.is_none() {
            lines.push(Line::from(Span::styled(
                match self.walls.get(&t.repo) {
                    Some(WallState::Missing(e)) => format!("no wallpaper: {e}"),
                    Some(WallState::Ready(_)) => "wallpaper needs a taller pane".to_string(),
                    None if self.wall_inflight.as_deref() == Some(&t.repo) => {
                        "fetching wallpaper…".to_string()
                    }
                    None => "no wallpaper yet".to_string(),
                },
                skin.dim(),
            )));
        }
        lines.push(Line::default());
        lines.push(Line::from(Span::styled(
            format!("github.com/{}", t.repo),
            skin.dim(),
        )));
        lines.push(Line::from(Span::styled(
            if on_disk {
                format!(
                    "installed as `{}` — ⏎ reinstalls + applies",
                    t.install_slug()
                )
            } else {
                "⏎ installs and applies immediately".to_string()
            },
            if on_disk { skin.body() } else { skin.dim() },
        )));
        f.render_widget(Paragraph::new(lines), text_area);
    }
}

/// Parse fetched `colors.toml` text into renderable cells. The palette gets
/// its own background behind every span so it reads true on any skin.
fn preview_of(text: &str) -> Result<PalettePreview, String> {
    let p = Palette::parse(text).map_err(brief)?;
    let rgb = |k: &str| p.get(k).map(|c| Color::Rgb(c.r, c.g, c.b));
    let bg_color = p.get("background").ok_or("colors.toml has no background")?;
    let bg = Color::Rgb(bg_color.r, bg_color.g, bg_color.b);
    let fg = rgb("foreground").ok_or("colors.toml has no foreground")?;
    // Perceived luminance of the background — a label, not a guarantee
    // (installed themes carry an explicit light.mode marker instead).
    let light =
        0.299 * bg_color.r as f32 + 0.587 * bg_color.g as f32 + 0.114 * bg_color.b as f32 > 128.0;
    Ok(PalettePreview {
        bg,
        fg,
        accent: rgb("accent").unwrap_or(fg),
        ansi: (0..16).filter_map(|i| rgb(&format!("color{i}"))).collect(),
        bg_hex: bg_color.hex(),
        light,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const JADE: &str = r##"
accent = "#0a9058"
foreground = "#c8d3cd"
background = "#0e1512"
color0 = "#0e1512"
color1 = "#c05b5b"
"##;

    fn browser() -> (CommunityBrowser, Receiver<Job>, Sender<Outcome>) {
        let (jobs_tx, jobs_rx) = channel();
        let (out_tx, out_rx) = channel();
        let mut installed = HashSet::new();
        installed.insert("ash".to_string());
        let b = CommunityBrowser::with_channels(jobs_tx, out_rx, installed);
        (b, jobs_rx, out_tx)
    }

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, ratatui::crossterm::event::KeyModifiers::NONE)
    }

    #[test]
    fn filter_narrows_the_list_and_resets_selection() {
        let (mut b, _jobs, _out) = browser();
        b.selected = 5;
        b.handle(key(KeyCode::Char('/')));
        for c in "rose pine".chars() {
            b.handle(key(KeyCode::Char(c)));
        }
        b.handle(key(KeyCode::Enter));
        assert_eq!(b.selected, 0);
        let v = b.visible();
        assert!(!v.is_empty() && v.len() < b.rows.len());
        assert!(v.iter().all(|t| community::matches(t, "rose pine")));
    }

    #[test]
    fn filter_input_owns_keys_until_enter() {
        let (mut b, _jobs, _out) = browser();
        b.handle(key(KeyCode::Char('/')));
        // `q` must type, not close, while editing
        assert!(matches!(
            b.handle(key(KeyCode::Char('q'))),
            CommunityAction::None
        ));
        assert_eq!(b.input.as_deref(), Some("q"));
        b.handle(key(KeyCode::Esc));
        assert!(b.input.is_none(), "esc cancels the edit, not the browser");
    }

    #[test]
    fn selection_requests_one_preview_at_a_time() {
        let (mut b, jobs, out) = browser();
        b.ensure_selected();
        let (first, wall) = match (jobs.try_recv(), jobs.try_recv()) {
            (Ok(Job::Preview(t)), Ok(Job::Wallpaper(w))) => (t, w),
            _ => panic!("selection should request its palette and its wallpaper"),
        };
        assert_eq!(first, wall, "both jobs describe the selected theme");
        assert!(b.busy());
        // A second poke while either is in flight must not double-request.
        b.ensure_selected();
        assert!(jobs.try_recv().is_err());
        out.send(Outcome::Preview {
            repo: first.repo.clone(),
            result: Ok(JADE.to_string()),
        })
        .unwrap();
        out.send(Outcome::Wallpaper {
            repo: first.repo.clone(),
            result: Ok(PathBuf::from("/x/ash.jpg")),
        })
        .unwrap();
        assert!(b.drain().is_none());
        assert!(!b.busy());
        assert!(matches!(
            b.previews.get(&first.repo),
            Some(PreviewState::Ready(_))
        ));
        assert!(matches!(
            b.walls.get(&first.repo),
            Some(WallState::Ready(p)) if p == &PathBuf::from("/x/ash.jpg")
        ));
    }

    #[test]
    fn a_wallpaperless_theme_still_previews_its_palette() {
        let (mut b, _jobs, out) = browser();
        b.wall_inflight = Some("x/y".into());
        out.send(Outcome::Wallpaper {
            repo: "x/y".into(),
            result: Err("this theme ships no backgrounds/".into()),
        })
        .unwrap();
        assert!(b.drain().is_none());
        assert!(matches!(b.walls.get("x/y"), Some(WallState::Missing(_))));
        assert!(
            b.wall_inflight.as_deref() != Some("x/y"),
            "a missing wallpaper isn't a stuck request"
        );
        assert!(
            b.error.is_none(),
            "it's a preview note, not an error banner"
        );
    }

    #[test]
    fn wallpaper_cache_ids_are_one_path_segment() {
        assert_eq!(
            wall_id("bjarneo/omarchy-ash-theme"),
            "bjarneo-omarchy-ash-theme"
        );
        assert!(!wall_id("a/b").contains('/'));
    }

    #[test]
    fn unparsable_palette_becomes_a_missing_state_not_a_crash() {
        let (mut b, _jobs, out) = browser();
        b.preview_inflight = Some("x/y".into());
        out.send(Outcome::Preview {
            repo: "x/y".into(),
            result: Ok("not = [valid".into()),
        })
        .unwrap();
        b.drain();
        assert!(matches!(
            b.previews.get("x/y"),
            Some(PreviewState::Missing(_))
        ));
    }

    #[test]
    fn install_outcome_reaches_the_app_and_marks_the_row() {
        let (mut b, jobs, out) = browser();
        b.handle(key(KeyCode::Enter));
        let t = match jobs.try_recv() {
            Ok(Job::Install(t)) => t,
            _ => panic!("enter should enqueue the install"),
        };
        assert!(b.installing.is_some());
        // Enter again while installing must not double-install.
        b.handle(key(KeyCode::Enter));
        assert!(jobs.try_recv().is_err());
        out.send(Outcome::Installed {
            name: t.name.clone(),
            slug: t.install_slug(),
            result: Ok(()),
        })
        .unwrap();
        let done = b.drain().expect("finished install surfaces");
        assert_eq!(done.slug, t.install_slug());
        assert!(b.installed.contains(&t.install_slug()));
        assert!(b.installing.is_none());
    }

    #[test]
    fn failed_install_lands_in_the_error_banner() {
        let (mut b, _jobs, out) = browser();
        b.installing = Some("Ash".into());
        out.send(Outcome::Installed {
            name: "Ash".into(),
            slug: "ash".into(),
            result: Err("git clone failed".into()),
        })
        .unwrap();
        assert!(b.drain().is_none());
        assert!(b.error.as_deref().unwrap_or("").contains("git clone"));
    }

    #[test]
    fn preview_parse_reads_the_palette() {
        let p = preview_of(JADE).expect("valid palette parses");
        assert_eq!(p.bg_hex, "#0e1512");
        assert!(!p.light);
        assert_eq!(p.ansi.len(), 2);
        assert!(preview_of("no colors here").is_err());
    }
}
