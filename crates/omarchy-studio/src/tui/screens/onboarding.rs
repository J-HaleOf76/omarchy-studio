//! First-run on-ramp (spec 10 §3, roadmap 1.0.4).
//!
//! The things a fresh install can't discover on its own: that nothing is
//! tracked yet, that Studio isn't wired into Omarchy, and that the terminal
//! tools don't follow the theme — until you say so. Shown once, over
//! everything, when Studio has never recorded a change; every step is optional
//! and re-runnable later (`omarchy-studio install-integration` / `themesync`).
//!
//! Like every other screen this one only *renders* status and reports intent —
//! the App owns each side effect, so the core/UI split holds and the screen
//! stays trivially testable.

use ratatui::crossterm::event::{KeyCode, KeyEvent};
use ratatui::layout::Rect;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Clear, Padding, Paragraph};
use ratatui::Frame;

use crate::tui::theme::Skin;
use crate::tui::ui;

/// One set-up action the wizard offers, in display order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Step {
    /// Start the git-backed change history — the restorable pre-Studio state.
    History,
    /// Add Studio to the Omarchy menu and install its theme/update hooks.
    Integration,
    /// Recolor terminal tools (fzf, lazygit) to follow the active theme.
    ThemeSync,
}

impl Step {
    /// Fixed order the wizard presents; also the on-screen row order.
    pub const ALL: [Step; 3] = [Step::History, Step::Integration, Step::ThemeSync];

    fn title(self) -> &'static str {
        match self {
            Step::History => "Track every change",
            Step::Integration => "Wire into Omarchy",
            Step::ThemeSync => "Theme your terminal tools",
        }
    }

    fn detail(self) -> &'static str {
        match self {
            Step::History => "Snapshot your setup so any change can be rolled back",
            Step::Integration => "Add Studio to the menu; survive theme swaps & updates",
            Step::ThemeSync => "Recolor fzf and lazygit to match your Omarchy theme",
        }
    }

    /// Verb shown on the selected, not-yet-done row.
    fn verb(self) -> &'static str {
        match self {
            Step::History => "start tracking",
            Step::Integration => "install",
            Step::ThemeSync => "turn on",
        }
    }
}

/// Live "already set up?" flags, computed by the App from disk.
#[derive(Debug, Clone, Copy, Default)]
pub struct Status {
    pub history: bool,
    pub integration: bool,
    pub themesync: bool,
}

impl Status {
    fn done(&self, step: Step) -> bool {
        match step {
            Step::History => self.history,
            Step::Integration => self.integration,
            Step::ThemeSync => self.themesync,
        }
    }

    /// Nothing left for the on-ramp to offer — used to skip it for someone
    /// who is already fully set up rather than show an all-ticked card.
    pub fn all_done(&self) -> bool {
        Step::ALL.iter().all(|&s| self.done(s))
    }
}

pub enum OnboardingAction {
    None,
    /// Run the selected step's side effect (the App performs it, then hands
    /// back a fresh [`Status`] via [`OnboardingScreen::set_status`]).
    Run(Step),
    /// Dismiss the wizard and mark first-run seen — land on the app.
    Finish,
}

pub struct OnboardingScreen {
    sel: usize,
    status: Status,
}

impl OnboardingScreen {
    pub fn new(status: Status) -> Self {
        Self { sel: 0, status }
    }

    /// Replace the status after a step ran (App recomputes from disk).
    pub fn set_status(&mut self, status: Status) {
        self.status = status;
    }

    pub fn handle(&mut self, key: KeyEvent) -> OnboardingAction {
        if ui::list_nav(key.code, &mut self.sel, Step::ALL.len()) {
            return OnboardingAction::None;
        }
        match key.code {
            KeyCode::Enter | KeyCode::Char(' ') => {
                let step = Step::ALL[self.sel];
                if self.status.done(step) {
                    OnboardingAction::None
                } else {
                    OnboardingAction::Run(step)
                }
            }
            KeyCode::Esc | KeyCode::Char('q') | KeyCode::Char('d') => OnboardingAction::Finish,
            _ => OnboardingAction::None,
        }
    }

    pub fn render(&self, f: &mut Frame, content: Rect, skin: &Skin) {
        let area = ui::centered_rect(content, 72, 17);
        f.render_widget(Clear, area);
        let block = Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(skin.accent_bold())
            .title(Line::from(vec![
                Span::styled(" ✦ ", skin.accent_bold()),
                Span::styled("Welcome to Studio ", skin.body()),
            ]))
            .title_bottom(
                Line::from(Span::styled(
                    " enter set up · d done · esc skip ",
                    skin.dim(),
                ))
                .right_aligned(),
            )
            .padding(Padding::new(2, 2, 1, 0));
        let inner = block.inner(area);
        f.render_widget(block, area);

        let mut lines: Vec<Line> = vec![
            Line::from(Span::styled(
                "Rice your desktop without editing configs — and undo anything.",
                skin.body(),
            )),
            Line::from(Span::styled(
                "Three optional steps. Skip any now; re-run later from the CLI.",
                skin.dim(),
            )),
            Line::from(""),
        ];

        for (i, &step) in Step::ALL.iter().enumerate() {
            let done = self.status.done(step);
            let selected = i == self.sel;
            let (mark, mark_style) = if done {
                ("✓", skin.ok())
            } else {
                ("○", skin.accent_bold())
            };
            let title_style = if selected {
                skin.selection()
            } else {
                skin.body()
            };
            let marker = if selected { "▸ " } else { "  " };
            let trailing = if done {
                "done".to_string()
            } else if selected {
                format!("enter to {}", step.verb())
            } else {
                String::new()
            };
            lines.push(Line::from(vec![
                Span::styled(marker, skin.accent_bold()),
                Span::styled(format!("{mark} "), mark_style),
                Span::styled(format!("{:<26}", step.title()), title_style),
                Span::styled(trailing, skin.dim()),
            ]));
            lines.push(Line::from(vec![
                Span::styled("    ", skin.dim()),
                Span::styled(step.detail(), skin.dim()),
            ]));
        }

        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            "Everything here is reversible. Open Doctor to check dependencies.",
            skin.dim(),
        )));

        f.render_widget(Paragraph::new(lines), inner);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    fn all_off() -> Status {
        Status::default()
    }

    #[test]
    fn enter_on_an_undone_step_runs_it() {
        let mut s = OnboardingScreen::new(all_off());
        match s.handle(key(KeyCode::Enter)) {
            OnboardingAction::Run(step) => assert_eq!(step, Step::History),
            _ => panic!("expected Run(History)"),
        }
    }

    #[test]
    fn enter_on_a_done_step_does_nothing() {
        let mut s = OnboardingScreen::new(Status {
            history: true,
            ..Status::default()
        });
        // selection starts on History, which is already done
        assert!(matches!(
            s.handle(key(KeyCode::Enter)),
            OnboardingAction::None
        ));
    }

    #[test]
    fn down_moves_to_the_next_step() {
        let mut s = OnboardingScreen::new(all_off());
        assert!(matches!(
            s.handle(key(KeyCode::Down)),
            OnboardingAction::None
        ));
        match s.handle(key(KeyCode::Enter)) {
            OnboardingAction::Run(step) => assert_eq!(step, Step::Integration),
            _ => panic!("expected Run(Integration) after one Down"),
        }
    }

    #[test]
    fn navigation_clamps_at_the_ends() {
        let mut s = OnboardingScreen::new(all_off());
        // up from the top stays on the first step
        s.handle(key(KeyCode::Up));
        match s.handle(key(KeyCode::Enter)) {
            OnboardingAction::Run(step) => assert_eq!(step, Step::History),
            _ => panic!("expected History"),
        }
        // past the bottom stays on the last step
        for _ in 0..10 {
            s.handle(key(KeyCode::Down));
        }
        match s.handle(key(KeyCode::Enter)) {
            OnboardingAction::Run(step) => assert_eq!(step, Step::ThemeSync),
            _ => panic!("expected ThemeSync"),
        }
    }

    #[test]
    fn esc_d_and_q_all_finish() {
        for code in [KeyCode::Esc, KeyCode::Char('d'), KeyCode::Char('q')] {
            let mut s = OnboardingScreen::new(all_off());
            assert!(matches!(s.handle(key(code)), OnboardingAction::Finish));
        }
    }

    #[test]
    fn all_done_only_when_every_step_is() {
        assert!(!all_off().all_done());
        assert!(!Status {
            history: true,
            integration: true,
            themesync: false,
        }
        .all_done());
        assert!(Status {
            history: true,
            integration: true,
            themesync: true,
        }
        .all_done());
    }

    #[test]
    fn set_status_flips_a_row_to_done() {
        let mut s = OnboardingScreen::new(all_off());
        s.set_status(Status {
            history: true,
            ..Status::default()
        });
        // History is now done, so Enter on it is a no-op
        assert!(matches!(
            s.handle(key(KeyCode::Enter)),
            OnboardingAction::None
        ));
    }
}
