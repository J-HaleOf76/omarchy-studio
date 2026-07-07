//! Theme-from-wallpaper wizard modal (spec 04 §7, roadmap 0.6.5).
//!
//! Opened with `t` on any still/gif in the wallpaper browser. The image is
//! decoded and sampled **once**; every mode/bias change re-runs the pure
//! extraction over the cached samples, so the swatch strip answers
//! instantly. Enter materializes a standard theme dir (core `wizard`
//! module); refining further is the normal palette editor on the new theme —
//! the wizard doesn't duplicate it.

use std::path::{Path, PathBuf};

use ratatui::crossterm::event::{KeyCode, KeyEvent};
use ratatui::layout::Rect;
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Clear, Padding, Paragraph};
use ratatui::Frame;

use studio_core::error::Result;
use studio_core::modules::extraction::{extract, sample_image, Bias, Extraction, Mode};

use crate::tui::theme::Skin;

pub enum WizardAction {
    None,
    Cancel,
    /// Materialize with the current name/extraction.
    Create,
}

pub struct WizardScreen {
    pub image: PathBuf,
    pub name: String,
    pub mode: Mode,
    pub bias: Bias,
    pub extraction: Extraction,
    samples: Vec<(u8, u8, u8)>,
}

impl WizardScreen {
    pub fn open(image: &Path) -> Result<Self> {
        let samples = sample_image(image)?;
        let mode = Mode::Normal;
        let bias = Bias::Auto;
        let name = image
            .file_stem()
            .map(|s| s.to_string_lossy().replace(['-', '_'], " "))
            .unwrap_or_default();
        Ok(Self {
            image: image.to_path_buf(),
            name,
            mode,
            bias,
            extraction: extract(&samples, mode, bias),
            samples,
        })
    }

    fn re_extract(&mut self) {
        self.extraction = extract(&self.samples, self.mode, self.bias);
    }

    fn cycle_mode(&mut self, dir: isize) {
        let i = Mode::ALL.iter().position(|m| *m == self.mode).unwrap_or(0);
        let n = Mode::ALL.len() as isize;
        self.mode = Mode::ALL[((i as isize + dir).rem_euclid(n)) as usize];
        self.re_extract();
    }

    fn cycle_bias(&mut self, dir: isize) {
        let i = Bias::ALL.iter().position(|b| *b == self.bias).unwrap_or(0);
        let n = Bias::ALL.len() as isize;
        self.bias = Bias::ALL[((i as isize + dir).rem_euclid(n)) as usize];
        self.re_extract();
    }

    pub fn handle(&mut self, key: KeyEvent) -> WizardAction {
        match key.code {
            KeyCode::Esc => return WizardAction::Cancel,
            KeyCode::Enter if !self.name.trim().is_empty() => return WizardAction::Create,
            KeyCode::Left => self.cycle_mode(-1),
            KeyCode::Right => self.cycle_mode(1),
            KeyCode::Up => self.cycle_bias(-1),
            KeyCode::Down => self.cycle_bias(1),
            KeyCode::Backspace => {
                self.name.pop();
            }
            KeyCode::Char(c) => self.name.push(c),
            _ => {}
        }
        WizardAction::None
    }

    pub fn render(&self, f: &mut Frame, area: Rect, skin: &Skin) {
        let w = 72u16.min(area.width.saturating_sub(2));
        let h = 14u16.min(area.height.saturating_sub(2));
        let rect = Rect::new(
            area.x + (area.width.saturating_sub(w)) / 2,
            area.y + (area.height.saturating_sub(h)) / 2,
            w,
            h,
        );
        f.render_widget(Clear, rect);
        let block = Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(skin.accent_bold())
            .padding(Padding::horizontal(2))
            .title(Line::from(vec![
                Span::styled(" ✦ ", skin.accent_bold()),
                Span::styled("Theme from wallpaper ", skin.body()),
            ]))
            .title_bottom(
                Line::from(Span::styled(
                    " enter create · ←→ mode · ↑↓ bias · esc cancel ",
                    skin.dim(),
                ))
                .right_aligned(),
            );
        let inner = block.inner(rect);
        f.render_widget(block, rect);

        let rgb = |c: &studio_core::modules::color::Color| Color::Rgb(c.r, c.g, c.b);
        let ex = &self.extraction;
        let selector = |label: &str, value: String| {
            Line::from(vec![
                Span::styled(format!("{label:<12}"), skin.dim()),
                Span::styled("‹ ", skin.accent_bold()),
                Span::styled(format!("{value:<12}"), skin.body()),
                Span::styled("›", skin.accent_bold()),
            ])
        };

        // the palette this extraction produces, as colored blocks on its bg
        let mut strip: Vec<Span> = vec![Span::styled(format!("{:<12}", "palette"), skin.dim())];
        strip.push(Span::styled(
            " abc ",
            Style::default()
                .fg(rgb(&ex.foreground))
                .bg(rgb(&ex.background)),
        ));
        strip.push(Span::styled(
            " abc ",
            Style::default().fg(rgb(&ex.accent)).bg(rgb(&ex.background)),
        ));
        strip.push(Span::raw(" "));
        for c in &ex.ansi[..8] {
            strip.push(Span::styled("██", Style::default().fg(rgb(c))));
        }

        let lines = vec![
            Line::from(vec![
                Span::styled(format!("{:<12}", "image"), skin.dim()),
                Span::styled(
                    self.image
                        .file_name()
                        .map(|n| n.to_string_lossy().to_string())
                        .unwrap_or_default(),
                    skin.body(),
                ),
            ]),
            Line::from(""),
            selector("mode", self.mode.label().into()),
            selector(
                "background",
                match (self.bias, ex.light) {
                    (Bias::Auto, true) => "auto → light".into(),
                    (Bias::Auto, false) => "auto → dark".into(),
                    (b, _) => b.label().into(),
                },
            ),
            Line::from(""),
            Line::from(strip),
            Line::from(vec![
                Span::styled(format!("{:<12}", ""), skin.dim()),
                Span::styled(
                    format!(
                        "bg {} · fg {} · accent {}",
                        ex.background.hex(),
                        ex.foreground.hex(),
                        ex.accent.hex()
                    ),
                    skin.dim(),
                ),
            ]),
            Line::from(""),
            Line::from(vec![
                Span::styled(format!("{:<12}", "name"), skin.dim()),
                Span::styled("❯ ", skin.accent_bold()),
                Span::styled(self.name.clone(), skin.body()),
                Span::styled("▏", skin.accent_bold()),
            ]),
        ];
        f.render_widget(Paragraph::new(lines), inner);
    }
}
