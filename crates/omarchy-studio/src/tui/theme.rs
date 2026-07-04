//! Studio self-theming (spec 07 §5): map the active Omarchy palette into a
//! ratatui style table so Studio looks native in all 19 stock themes. Falls
//! back to a sane dark palette when no theme/colors.toml is readable.

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
}

impl Default for Skin {
    /// Tokyo-night-ish dark defaults, used when nothing is readable.
    fn default() -> Self {
        Self {
            fg: Color::Rgb(0xc0, 0xca, 0xf5),
            dim: Color::Rgb(0x56, 0x5f, 0x89),
            accent: Color::Rgb(0x7a, 0xa2, 0xf7),
            warn: Color::Rgb(0xe0, 0xaf, 0x68),
            error: Color::Rgb(0xf7, 0x76, 0x8e),
            ok: Color::Rgb(0x9e, 0xce, 0x6a),
            border: Color::Rgb(0x3b, 0x42, 0x61),
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
        Self {
            fg: c("foreground", d.fg),
            dim: c("color8", d.dim),
            accent: c("accent", d.accent),
            warn: c("color3", d.warn),
            error: c("color1", d.error),
            ok: c("color2", d.ok),
            border: c("color8", d.border),
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
    /// Selected row: accent reversed for a strong, theme-consistent highlight.
    pub fn selection(&self) -> Style {
        Style::default()
            .fg(self.accent)
            .add_modifier(Modifier::REVERSED | Modifier::BOLD)
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

fn conv(c: PColor) -> Color {
    Color::Rgb(c.r, c.g, c.b)
}
