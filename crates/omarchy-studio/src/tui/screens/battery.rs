//! Power screen — battery charge thresholds (community ask: T480 owners
//! shouldn't need TLP just to set charge limits).
//!
//! Shows every battery with charge/status; on hardware that exposes
//! `charge_control_*_threshold` the start/stop window is editable in place.
//! Applying needs root: the direct write is attempted and a denial toasts
//! the exact sudo command (never a silent failure). Unsupported laptops get
//! told exactly why, and everything else on the screen still works.

use ratatui::crossterm::event::{KeyCode, KeyEvent};
use ratatui::layout::Rect;
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::Frame;

use studio_core::modules::battery::{detect, Battery};

use crate::tui::theme::Skin;

pub enum BatteryAction {
    None,
    /// Try to write the window to every threshold-capable battery.
    Apply {
        start: u8,
        end: u8,
    },
    Refresh,
}

pub struct BatteryScreen {
    bats: Vec<Battery>,
    /// Pending charge window (start, end), pre-seeded from the hardware.
    start: u8,
    end: u8,
    /// 0 = start field, 1 = end field.
    field: usize,
}

impl BatteryScreen {
    pub fn load() -> Self {
        let bats = detect();
        let (start, end) = bats
            .first()
            .map(|b| (b.start.unwrap_or(75), b.end.unwrap_or(80)))
            .unwrap_or((75, 80));
        Self {
            bats,
            start,
            end,
            field: 0,
        }
    }

    pub fn reload(&mut self) {
        *self = Self::load();
    }

    fn editable(&self) -> bool {
        self.bats.iter().any(|b| b.supports_thresholds())
    }

    pub fn handle(&mut self, key: KeyEvent) -> BatteryAction {
        match key.code {
            KeyCode::Char('r') => return BatteryAction::Refresh,
            _ if !self.editable() => return BatteryAction::None,
            KeyCode::Char('j') | KeyCode::Down => self.field = 1,
            KeyCode::Char('k') | KeyCode::Up => self.field = 0,
            KeyCode::Char('h') | KeyCode::Left => {
                if self.field == 0 {
                    self.start = self.start.saturating_sub(5);
                } else {
                    self.end = self.end.saturating_sub(5).max(self.start + 5);
                }
            }
            KeyCode::Char('l') | KeyCode::Right => {
                if self.field == 0 {
                    self.start = (self.start + 5).min(self.end - 5);
                } else {
                    self.end = (self.end + 5).min(100);
                }
            }
            KeyCode::Char('s') | KeyCode::Enter => {
                return BatteryAction::Apply {
                    start: self.start,
                    end: self.end,
                }
            }
            _ => {}
        }
        BatteryAction::None
    }

    pub fn hint(&self) -> String {
        if self.editable() {
            "j/k field · h/l adjust · s apply · r re-read".into()
        } else {
            "r re-read".into()
        }
    }

    pub fn render(&self, f: &mut Frame, area: Rect, skin: &Skin) {
        let mut lines: Vec<Line> = Vec::new();

        if self.bats.is_empty() {
            lines.push(Line::from(Span::styled(
                "  No battery found — desktop machine?",
                skin.dim(),
            )));
            f.render_widget(Paragraph::new(lines), area);
            return;
        }

        lines.push(Line::from(Span::styled("  Batteries", skin.dim())));
        for b in &self.bats {
            let charge = b
                .capacity
                .map(|c| format!("{c}%"))
                .unwrap_or_else(|| "?".into());
            lines.push(Line::from(vec![
                Span::styled(format!("    {:<6}", b.name), skin.body()),
                Span::styled(format!("{charge:<6}"), skin.accent_bold()),
                Span::styled(b.status.clone(), skin.dim()),
            ]));
            if let (Some(s), Some(e)) = (b.start, b.end) {
                lines.push(Line::from(Span::styled(
                    format!("           charges from {s}% up to {e}%"),
                    skin.dim(),
                )));
            }
        }
        lines.push(Line::from(""));

        if self.editable() {
            lines.push(Line::from(Span::styled("  Charge window", skin.dim())));
            let field = |label: &str, value: u8, on: bool, skin: &Skin| {
                Line::from(vec![
                    Span::styled(if on { "  ▸ " } else { "    " }, skin.accent_bold()),
                    Span::styled(format!("{label:<18}"), skin.body()),
                    Span::styled("‹ ", skin.accent_bold()),
                    Span::styled(
                        format!("{value:>3} %"),
                        if on { skin.accent_bold() } else { skin.body() },
                    ),
                    Span::styled(" ›", skin.accent_bold()),
                ])
            };
            lines.push(field(
                "start charging at",
                self.start,
                self.field == 0,
                skin,
            ));
            lines.push(field("stop charging at", self.end, self.field == 1, skin));
            lines.push(Line::from(""));
            lines.push(Line::from(Span::styled(
                "  75–80% is the usual longevity window for a mostly-plugged-in laptop.",
                skin.dim(),
            )));
            lines.push(Line::from(Span::styled(
                "  Applying writes sysfs directly (needs root — a denied write shows the",
                skin.dim(),
            )));
            lines.push(Line::from(Span::styled(
                "  sudo command). Thresholds reset on reboot; persist with:",
                skin.dim(),
            )));
            lines.push(Line::from(Span::styled(
                format!(
                    "      sudo omarchy-studio battery limit {} {} --persist",
                    self.start, self.end
                ),
                skin.warn(),
            )));
        } else {
            lines.push(Line::from(Span::styled(
                "  This laptop's kernel driver doesn't expose charge thresholds",
                skin.warn(),
            )));
            lines.push(Line::from(Span::styled(
                "  (charge_control_*_threshold — ThinkPads via thinkpad_acpi, and some",
                skin.dim(),
            )));
            lines.push(Line::from(Span::styled(
                "  ASUS/LG/Huawei models). Charge and status above still update live.",
                skin.dim(),
            )));
        }

        f.render_widget(Paragraph::new(lines), area);
    }
}
