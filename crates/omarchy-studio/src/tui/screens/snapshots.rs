//! Snapshot timeline screen (roadmap 0.9.4).
//!
//! Studio's undo store is a real git repo, so every change it has ever made is
//! browsable: the left column is the history (newest first), the right pane the
//! diff that snapshot introduced. `r` rolls the whole tree back to any point —
//! itself recorded, so a restore is just as reversible. This is the flagship
//! safety feature a-la-carchy's tarballs can't touch.

use ratatui::crossterm::event::{KeyCode, KeyEvent};
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem, Paragraph, Wrap};
use ratatui::Frame;

use studio_core::cmd::RealRunner;
use studio_core::snapshot::{SnapshotInfo, SnapshotStore};

use crate::tui::theme::Skin;
use crate::tui::ui;

pub enum SnapshotsAction {
    None,
    /// Restore the whole tree to this snapshot id (App confirms via toast).
    Restore(String),
}

pub struct SnapshotsScreen {
    store: Option<SnapshotStore>,
    entries: Vec<SnapshotInfo>,
    cursor: usize,
    /// Diff of the selected snapshot, loaded lazily on move.
    diff: String,
    /// Vertical scroll offset into the diff pane.
    scroll: u16,
    error: Option<String>,
}

impl SnapshotsScreen {
    pub fn load() -> Self {
        let store = SnapshotStore::open_or_init(
            studio_core::studio_state_dir().join("history"),
            Box::new(RealRunner),
        )
        .ok();
        let entries = store
            .as_ref()
            .and_then(|s| s.list(100).ok())
            .unwrap_or_default();
        let error = store
            .is_none()
            .then(|| "snapshot store unavailable".to_string());
        let mut me = Self {
            store,
            entries,
            cursor: 0,
            diff: String::new(),
            scroll: 0,
            error,
        };
        me.load_diff();
        me
    }

    pub fn reload(&mut self) {
        let keep = self.cursor;
        *self = Self::load();
        self.cursor = ui::clamp_index(keep, self.entries.len());
        self.load_diff();
    }

    pub fn hint(&self) -> &'static str {
        "↑↓ move · PgUp/PgDn scroll diff · r restore this point"
    }

    fn load_diff(&mut self) {
        self.scroll = 0;
        self.diff = match (&self.store, self.entries.get(self.cursor)) {
            (Some(s), Some(e)) => s.diff(&e.id).unwrap_or_default(),
            _ => String::new(),
        };
    }

    pub fn handle(&mut self, key: KeyEvent) -> SnapshotsAction {
        let before = self.cursor;
        if ui::list_nav(key.code, &mut self.cursor, self.entries.len()) {
            if self.cursor != before {
                self.load_diff();
            }
            return SnapshotsAction::None;
        }
        match key.code {
            KeyCode::PageDown => self.scroll = self.scroll.saturating_add(10),
            KeyCode::PageUp => self.scroll = self.scroll.saturating_sub(10),
            KeyCode::Char('r') | KeyCode::Enter => {
                if let Some(e) = self.entries.get(self.cursor) {
                    return SnapshotsAction::Restore(e.id.clone());
                }
            }
            _ => {}
        }
        SnapshotsAction::None
    }

    pub fn render(&self, f: &mut Frame, area: Rect, skin: &Skin) {
        if let Some(msg) = &self.error {
            f.render_widget(
                Paragraph::new(Line::from(Span::styled(msg.clone(), skin.body()))),
                area,
            );
            return;
        }
        if self.entries.is_empty() {
            f.render_widget(
                Paragraph::new(Line::from(Span::styled(
                    "No snapshots yet — they appear the first time Studio changes a file.",
                    skin.dim(),
                )))
                .wrap(Wrap { trim: false }),
                area,
            );
            return;
        }

        let cols = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Length(40), Constraint::Min(20)])
            .split(area);

        // ── timeline list ──
        let items: Vec<ListItem> = self
            .entries
            .iter()
            .enumerate()
            .map(|(i, e)| {
                let on = i == self.cursor;
                let marker = if on { "▸ " } else { "  " };
                let kind = e
                    .kind
                    .map(|k| format!("{k:?}").to_lowercase())
                    .unwrap_or_else(|| "?".into());
                let name_style = if on { skin.selection() } else { skin.body() };
                ListItem::new(vec![
                    Line::from(vec![
                        Span::styled(marker, skin.accent_bold()),
                        Span::styled(format!("{:<8}", kind), skin.dim()),
                        Span::styled(short_time(&e.timestamp), skin.dim()),
                    ]),
                    Line::from(Span::styled(format!("    {}", e.summary), name_style)),
                ])
            })
            .collect();
        f.render_widget(List::new(items), cols[0]);

        // ── diff pane ──
        let block = Block::default()
            .borders(Borders::LEFT)
            .border_style(skin.dim());
        let inner = block.inner(cols[1]);
        f.render_widget(block, cols[1]);
        let lines: Vec<Line> = self
            .diff
            .lines()
            .map(|l| {
                let style = if l.starts_with('+') && !l.starts_with("+++") {
                    skin.accent_bold()
                } else if l.starts_with('-') && !l.starts_with("---") {
                    skin.warn()
                } else if l.starts_with("@@") {
                    skin.selection()
                } else {
                    skin.dim()
                };
                Line::from(Span::styled(l.to_string(), style))
            })
            .collect();
        f.render_widget(Paragraph::new(lines).scroll((self.scroll, 0)), inner);
    }
}

/// `2026-07-08T17:21:43+05:30` → `2026-07-08 17:21`.
fn short_time(iso: &str) -> String {
    match iso.split_once('T') {
        Some((date, rest)) => {
            let hm: String = rest.chars().take(5).collect();
            format!("{date} {hm}")
        }
        None => iso.to_string(),
    }
}
