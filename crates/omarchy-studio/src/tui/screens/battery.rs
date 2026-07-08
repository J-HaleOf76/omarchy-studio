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

use studio_core::cmd::RealRunner;
use studio_core::modules::battery::{detect, Battery};
use studio_core::modules::power::{self, Profile};

use crate::tui::theme::Skin;

pub enum BatteryAction {
    None,
    /// Try to write the window to every threshold-capable battery.
    Apply {
        start: u8,
        end: u8,
    },
    /// Switch the active power profile (and persist it at login).
    SetProfile(String),
    Refresh,
}

pub struct BatteryScreen {
    bats: Vec<Battery>,
    /// Pending charge window (start, end), pre-seeded from the hardware.
    start: u8,
    end: u8,
    /// 0 = start field, 1 = end field.
    field: usize,
    /// Available power profiles (empty when power-profiles-daemon is absent).
    profiles: Vec<Profile>,
}

impl BatteryScreen {
    pub fn load() -> Self {
        let bats = detect();
        let (start, end) = bats
            .first()
            .map(|b| (b.start.unwrap_or(75), b.end.unwrap_or(80)))
            .unwrap_or((75, 80));
        let profiles = if power::available() {
            power::list(&RealRunner).unwrap_or_default()
        } else {
            Vec::new()
        };
        Self {
            bats,
            start,
            end,
            field: 0,
            profiles,
        }
    }

    pub fn reload(&mut self) {
        *self = Self::load();
    }

    /// The profile after the active one (wraps), for the `p` cycle key.
    fn next_profile(&self) -> Option<String> {
        if self.profiles.is_empty() {
            return None;
        }
        let cur = self.profiles.iter().position(|p| p.active).unwrap_or(0);
        let next = (cur + 1) % self.profiles.len();
        Some(self.profiles[next].name.clone())
    }

    fn editable(&self) -> bool {
        self.bats.iter().any(|b| b.supports_thresholds())
    }

    pub fn handle(&mut self, key: KeyEvent) -> BatteryAction {
        match key.code {
            KeyCode::Char('r') => return BatteryAction::Refresh,
            KeyCode::Char('p') => {
                if let Some(name) = self.next_profile() {
                    return BatteryAction::SetProfile(name);
                }
            }
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
        let profile = if self.profiles.is_empty() {
            ""
        } else {
            " · p profile"
        };
        if self.editable() {
            format!("j/k field · h/l adjust · s apply{profile} · r re-read")
        } else {
            format!("r re-read{profile}")
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

        if !self.profiles.is_empty() {
            lines.push(Line::from(Span::styled("  Power profile", skin.dim())));
            let mut spans = vec![Span::raw("    ")];
            for (i, p) in self.profiles.iter().enumerate() {
                if i > 0 {
                    spans.push(Span::styled("   ", skin.dim()));
                }
                let style = if p.active {
                    skin.accent_bold()
                } else {
                    skin.dim()
                };
                let dot = if p.active { "● " } else { "○ " };
                spans.push(Span::styled(format!("{dot}{}", p.name), style));
            }
            lines.push(Line::from(spans));
            lines.push(Line::from(Span::styled(
                "    p cycles the active profile (persisted at login)",
                skin.dim(),
            )));
            lines.push(Line::from(""));
        }

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
