//! Studio self-theming (spec 07 §5): map the active Omarchy palette into a
//! ratatui style table so Studio looks native in all 19 stock themes. Falls
//! back to a sane dark palette when no theme/colors.toml is readable.
//!
//! Beyond the raw palette, the skin *derives* surfaces by blending: selected
//! rows get a bg tinted toward the accent instead of a harsh REVERSED cell,
//! which is what gives the chrome its lazygit-ish depth on every theme.

use ratatui::style::{Color, Modifier, Style};
use studio_core::modules::color::Color as PColor;
use studio_core::modules::themes::Palette;
use studio_core::omarchy::OmarchyPaths;

#[derive(Debug, Clone, Copy)]
pub struct Skin {
    pub fg: Color,
    pub dim: Color,
    pub accent: Color,
    pub warn: Color,
    pub error: Color,
    pub ok: Color,
    pub border: Color,
    /// Theme background — the base every derived surface blends from.
    pub bg: Color,
    /// Selected-row fill: bg pulled toward the accent.
    pub sel_bg: Color,
}

impl Default for Skin {
    /// Tokyo-night-ish dark defaults, used when nothing is readable.
    fn default() -> Self {
        let bg = Color::Rgb(0x1a, 0x1b, 0x26);
        let accent = Color::Rgb(0x7a, 0xa2, 0xf7);
        Self {
            fg: Color::Rgb(0xc0, 0xca, 0xf5),
            dim: Color::Rgb(0x56, 0x5f, 0x89),
            accent,
            warn: Color::Rgb(0xe0, 0xaf, 0x68),
            error: Color::Rgb(0xf7, 0x76, 0x8e),
            ok: Color::Rgb(0x9e, 0xce, 0x6a),
            border: Color::Rgb(0x3b, 0x42, 0x61),
            bg,
            sel_bg: mix(bg, accent, 0.22),
        }
    }
}

impl Skin {
    /// Load from the active theme's `current/theme/colors.toml`.
    pub fn from_current(paths: &OmarchyPaths) -> Self {
        let path = paths.config.join("current/theme/colors.toml");
        match Palette::load(&path) {
            Ok(p) => Self::from_palette(&p),
            Err(_) => Self::default(),
        }
    }

    fn from_palette(p: &Palette) -> Self {
        let c = |key: &str, fallback: Color| p.get(key).map(conv).unwrap_or(fallback);
        let d = Self::default();
        let bg = c("background", d.bg);
        let accent = c("accent", d.accent);
        Self {
            fg: c("foreground", d.fg),
            dim: c("color8", d.dim),
            accent,
            warn: c("color3", d.warn),
            error: c("color1", d.error),
            ok: c("color2", d.ok),
            border: c("color8", d.border),
            bg,
            sel_bg: mix(bg, accent, 0.22),
        }
    }

    pub fn accent_bold(&self) -> Style {
        Style::default()
            .fg(self.accent)
            .add_modifier(Modifier::BOLD)
    }
    pub fn dim(&self) -> Style {
        Style::default().fg(self.dim)
    }
    pub fn body(&self) -> Style {
        Style::default().fg(self.fg)
    }
    /// Selected row: foreground kept, background tinted toward the accent —
    /// the subtle highlight used by every list in the app.
    pub fn selection(&self) -> Style {
        Style::default()
            .fg(self.fg)
            .bg(self.sel_bg)
            .add_modifier(Modifier::BOLD)
    }
    pub fn border(&self) -> Style {
        Style::default().fg(self.border)
    }
    pub fn warn(&self) -> Style {
        Style::default().fg(self.warn)
    }
    pub fn error(&self) -> Style {
        Style::default().fg(self.error)
    }
    pub fn ok(&self) -> Style {
        Style::default().fg(self.ok)
    }
}

/// Linear RGB blend `a → b` by `t` (0.0 = a, 1.0 = b). Non-RGB colors pass
/// through unchanged (only ever constructed from RGB here).
pub fn mix(a: Color, b: Color, t: f32) -> Color {
    match (a, b) {
        (Color::Rgb(ar, ag, ab), Color::Rgb(br, bg, bb)) => {
            let l = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t).round() as u8;
            Color::Rgb(l(ar, br), l(ag, bg), l(ab, bb))
        }
        _ => a,
    }
}

fn conv(c: PColor) -> Color {
    Color::Rgb(c.r, c.g, c.b)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mix_blends_and_clamps_endpoints() {
        let a = Color::Rgb(0, 0, 0);
        let b = Color::Rgb(200, 100, 50);
        assert_eq!(mix(a, b, 0.0), a);
        assert_eq!(mix(a, b, 1.0), b);
        assert_eq!(mix(a, b, 0.5), Color::Rgb(100, 50, 25));
        // non-RGB passes through
        assert_eq!(mix(Color::Reset, b, 0.5), Color::Reset);
    }
}
