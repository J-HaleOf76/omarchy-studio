//! Apps & services screen (roadmap 0.8.3).
//!
//! A checklist of the removable catalog with live installed markers. Select the
//! ones to remove, press Enter, and Studio builds the removal plan — the full
//! cascade of orphaned dependencies, the services it will disable, the config
//! dirs it will leave behind — and shows the exact commands in a confirm modal
//! before anything runs. pacman refusing a removal (an outside package still
//! needs the selection) is surfaced as a blocker, never forced.

use ratatui::crossterm::event::{KeyCode, KeyEvent};
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, List, ListItem, Paragraph, Wrap};
use ratatui::Frame;

use studio_core::cmd::RealRunner;
use studio_core::modules::apps::{self, AppItem, Inventory, Kind, RemovalPlan};
use studio_core::omarchy::OmarchyPaths;

use crate::tui::theme::Skin;

pub enum AppsAction {
    None,
    /// Build the removal plan for these catalog ids (App has the runner/paths).
    Preview(Vec<String>),
    /// Execute this (already-previewed, confirmed) plan.
    Remove(RemovalPlan),
}

pub struct AppsScreen {
    /// Catalog items, in display order.
    items: Vec<&'static AppItem>,
    installed: Vec<bool>,
    selected: Vec<bool>,
    cursor: usize,
    /// Confirm modal: the built plan, shown before execution.
    confirm: Option<RemovalPlan>,
}

impl AppsScreen {
    pub fn load(paths: &OmarchyPaths) -> Self {
        let inv = Inventory::probe(&RealRunner, paths);
        let items: Vec<&'static AppItem> = apps::CATALOG.iter().collect();
        let installed: Vec<bool> = items.iter().map(|i| inv.is_installed(i)).collect();
        let selected = vec![false; items.len()];
        Self {
            items,
            installed,
            selected,
            cursor: 0,
            confirm: None,
        }
    }

    pub fn reload(&mut self, paths: &OmarchyPaths) {
        let keep = self.cursor;
        *self = Self::load(paths);
        self.cursor = keep.min(self.items.len().saturating_sub(1));
    }

    pub fn is_modal(&self) -> bool {
        self.confirm.is_some()
    }

    /// Called by the App once it has built the plan (or found it empty).
    pub fn open_confirm(&mut self, plan: RemovalPlan) {
        self.confirm = Some(plan);
    }

    pub fn hint(&self) -> &'static str {
        if self.confirm.is_some() {
            "y confirm · n/Esc cancel"
        } else {
            "j/k move · Space select · Enter preview · q back"
        }
    }

    pub fn handle(&mut self, key: KeyEvent) -> AppsAction {
        if self.confirm.is_some() {
            return self.handle_confirm(key);
        }
        let n = self.items.len();
        match key.code {
            KeyCode::Char('j') | KeyCode::Down if n > 0 => {
                self.cursor = (self.cursor + 1).min(n - 1)
            }
            KeyCode::Char('k') | KeyCode::Up => self.cursor = self.cursor.saturating_sub(1),
            KeyCode::Char('g') => self.cursor = 0,
            KeyCode::Char('G') => self.cursor = n.saturating_sub(1),
            // Only installed items can be selected for removal.
            KeyCode::Char(' ') if self.installed.get(self.cursor).copied().unwrap_or(false) => {
                self.selected[self.cursor] = !self.selected[self.cursor];
            }
            KeyCode::Enter => {
                let ids: Vec<String> = self
                    .items
                    .iter()
                    .zip(&self.selected)
                    .filter(|(_, &s)| s)
                    .map(|(i, _)| i.id.to_string())
                    .collect();
                if !ids.is_empty() {
                    return AppsAction::Preview(ids);
                }
            }
            _ => {}
        }
        AppsAction::None
    }

    fn handle_confirm(&mut self, key: KeyEvent) -> AppsAction {
        match key.code {
            KeyCode::Esc | KeyCode::Char('n') => self.confirm = None,
            KeyCode::Char('y') => {
                if let Some(plan) = self.confirm.take() {
                    if plan.can_execute() {
                        // Clear the selection so a re-entry starts fresh.
                        self.selected.iter_mut().for_each(|s| *s = false);
                        return AppsAction::Remove(plan);
                    }
                    // Blocked plan: nothing to do, just close.
                }
            }
            _ => {}
        }
        AppsAction::None
    }

    pub fn render(&self, f: &mut Frame, area: Rect, skin: &Skin) {
        let rows = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Min(1), Constraint::Length(2)])
            .split(area);

        let mut items: Vec<ListItem> = Vec::new();
        let mut last_kind: Option<Kind> = None;
        for (idx, item) in self.items.iter().enumerate() {
            if last_kind != Some(item.kind) {
                let title = match item.kind {
                    Kind::Package => "Packages",
                    Kind::WebApp => "Web apps",
                };
                items.push(ListItem::new(Line::from(Span::styled(
                    format!("  {title}"),
                    skin.dim(),
                ))));
                last_kind = Some(item.kind);
            }
            let on_cursor = idx == self.cursor;
            let installed = self.installed[idx];
            let checked = self.selected[idx];
            let box_ = if checked {
                "[x]"
            } else if installed {
                "[ ]"
            } else {
                "   "
            };
            let name_style = if !installed {
                skin.dim()
            } else if on_cursor {
                skin.selection()
            } else {
                skin.body()
            };
            let marker = if on_cursor { "▸ " } else { "  " };
            let svc = if item.services.is_empty() {
                String::new()
            } else {
                format!("  ⚙ {}", item.services.join(", "))
            };
            let tail = if installed { "" } else { "  (not installed)" };
            items.push(ListItem::new(Line::from(vec![
                Span::styled(format!("{marker}{box_} "), skin.accent_bold()),
                Span::styled(format!("{:<20}", item.label), name_style),
                Span::styled(format!("{}{svc}{tail}", item.desc), skin.dim()),
            ])));
        }
        f.render_widget(List::new(items), rows[0]);

        let count = self.selected.iter().filter(|s| **s).count();
        let footer = Paragraph::new(vec![
            Line::from(Span::styled(
                format!("{count} selected for removal"),
                if count > 0 { skin.warn() } else { skin.dim() },
            )),
            Line::from(Span::styled(self.hint(), skin.dim())),
        ]);
        f.render_widget(footer, rows[1]);

        if let Some(plan) = &self.confirm {
            self.render_confirm(f, area, skin, plan);
        }
    }

    fn render_confirm(&self, f: &mut Frame, area: Rect, skin: &Skin, plan: &RemovalPlan) {
        let w = 78u16.min(area.width.saturating_sub(2));
        let h = 22u16.min(area.height.saturating_sub(2));
        let x = area.x + (area.width.saturating_sub(w)) / 2;
        let y = area.y + (area.height.saturating_sub(h)) / 2;
        let rect = Rect::new(x, y, w, h);
        f.render_widget(Clear, rect);
        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(skin.accent_bold())
            .title(" Confirm removal ");
        let inner = block.inner(rect);
        f.render_widget(block, rect);

        let mut lines: Vec<Line> = Vec::new();
        if !plan.packages.is_empty() {
            lines.push(kv(skin, "packages", &plan.packages.join(" ")));
        }
        if !plan.cascade.is_empty() {
            lines.push(kv(skin, "also removes", &plan.cascade.join(" ")));
        }
        if !plan.services.is_empty() {
            lines.push(kv(skin, "disable first", &plan.services.join(", ")));
        }
        if !plan.webapps.is_empty() {
            lines.push(kv(skin, "web apps", &plan.webapps.join(", ")));
        }
        if !plan.leftovers.is_empty() {
            lines.push(Line::from(Span::styled(
                "  leftover config (kept):",
                skin.dim(),
            )));
            for p in &plan.leftovers {
                lines.push(Line::from(Span::styled(
                    format!("    {}", p.display()),
                    skin.dim(),
                )));
            }
        }
        lines.push(Line::from(""));

        if let Some(why) = &plan.blocker {
            lines.push(Line::from(Span::styled(
                "  pacman won't remove this yet:",
                skin.warn(),
            )));
            lines.push(Line::from(Span::styled(format!("    {why}"), skin.body())));
            lines.push(Line::from(""));
            lines.push(Line::from(Span::styled(
                "  Esc to go back — resolve the dependency first.",
                skin.dim(),
            )));
        } else {
            lines.push(Line::from(Span::styled("  will run:", skin.accent_bold())));
            for c in plan.commands() {
                lines.push(Line::from(Span::styled(format!("    {c}"), skin.body())));
            }
            lines.push(Line::from(""));
            lines.push(Line::from(Span::styled(
                "  pacman changes aren't git-snapshotted — this is logged for restore.",
                skin.dim(),
            )));
            lines.push(Line::from(""));
            lines.push(Line::from(Span::styled(
                "  y to run   ·   n / Esc to cancel",
                skin.warn(),
            )));
        }
        f.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), inner);
    }
}

fn kv<'a>(skin: &Skin, k: &'a str, v: &str) -> Line<'a> {
    Line::from(vec![
        Span::styled(format!("  {k:<14}"), skin.dim()),
        Span::styled(v.to_string(), skin.body()),
    ])
}
