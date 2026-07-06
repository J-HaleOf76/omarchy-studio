//! The "omarchy studio" wordmark, hand-set in the half-block letterform style
//! of Omarchy's own logo (`$OMARCHY_PATH/logo.txt`): 3-cell strokes, ▄/▀ stroke
//! tips, and the extender row above the x-height (the d's ascender and i's dot,
//! like the m's crown in the original).
//!
//! Rendered with a vertical fade from the theme accent so it re-tints itself
//! with every theme, same as the rest of the chrome.

use ratatui::layout::Rect;
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::Frame;

use super::theme::{mix, Skin};

/// "studio" in the Omarchy letterform style. Every row is the same width.
pub const STUDIO: [&str; 9] = [
    "                                       ▄██  ▄█▄           ",
    " ▄██████▄    ▄██       ▄█   █▄    ▄███▄███        ▄█████▄ ",
    "███   █▀   ▄▄███▄▄▄   ███   ███  ███   ███  ▄██  ███   ███",
    "███        ▀▀███▀▀▀   ███   ███  ███   ███  ███  ███   ███",
    " ▀██████▄    ███      ███   ███  ███   ███  ███  ███   ███",
    "      ███    ███      ███   ███  ███   ███  ███  ███   ███",
    "▄█    ███    ███      ███   ███  ███   ███  ███  ███   ███",
    "███   ███    ███ ▄█   ███   ███  ███   ███  ███  ███   ███",
    " ▀█████▀     ▀████▀    ▀█████▀    ▀█████▀   ▀█▀   ▀█████▀ ",
];

/// Columns the wordmark needs (all rows are equal width).
pub const WIDTH: u16 = 58;
/// Rows the banner occupies: eyebrow + glyphs + breathing room.
pub const HEIGHT: u16 = 11;

/// Draw the banner: a small "omarchy" eyebrow over the "studio" glyphs,
/// fading from accent toward the muted color as the strokes descend.
pub fn render(f: &mut Frame, area: Rect, skin: &Skin) {
    let mut lines: Vec<Line> = Vec::with_capacity(STUDIO.len() + 1);
    lines.push(Line::from(Span::styled("o m a r c h y", skin.dim())));
    for (i, row) in STUDIO.iter().enumerate() {
        let t = i as f32 / (STUDIO.len() - 1) as f32;
        let color = mix(skin.accent, skin.dim, t * 0.6);
        lines.push(Line::from(Span::styled(
            *row,
            ratatui::style::Style::default().fg(color),
        )));
    }
    f.render_widget(Paragraph::new(lines), area);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wordmark_rows_are_uniform_width() {
        for row in STUDIO {
            assert_eq!(row.chars().count(), WIDTH as usize, "row: {row}");
        }
        assert!(HEIGHT as usize > STUDIO.len());
    }
}
