//! Color math for the palette editor (spec 04 §2): hex parsing in Omarchy's
//! `colors.toml` conventions, HSL transforms for the derive helpers, and WCAG
//! contrast for the accessibility checker.

use crate::error::{Result, StudioError};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Color {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

impl Color {
    /// Accepts `#rrggbb` or bare `rrggbb` (the `keyboard.rgb` form).
    pub fn parse(s: &str) -> Result<Self> {
        let hex = s.trim().trim_start_matches('#');
        if hex.len() != 6 || !hex.chars().all(|c| c.is_ascii_hexdigit()) {
            return Err(StudioError::ParseFailed {
                file: Default::default(),
                line: None,
                hint: format!("`{s}` is not a color — expected 6 hex digits like #7aa2f7"),
            });
        }
        let byte = |i: usize| u8::from_str_radix(&hex[i..i + 2], 16).unwrap();
        Ok(Self {
            r: byte(0),
            g: byte(2),
            b: byte(4),
        })
    }

    /// `{{ key }}` form: `#7aa2f7`.
    pub fn hex(&self) -> String {
        format!("#{:02x}{:02x}{:02x}", self.r, self.g, self.b)
    }

    /// `{{ key_strip }}` form: `7aa2f7`.
    pub fn strip(&self) -> String {
        format!("{:02x}{:02x}{:02x}", self.r, self.g, self.b)
    }

    /// `{{ key_rgb }}` form: `122,162,247` (chromium.theme).
    pub fn rgb_decimal(&self) -> String {
        format!("{},{},{}", self.r, self.g, self.b)
    }

    /// → (hue degrees 0..360, saturation 0..1, lightness 0..1).
    pub fn to_hsl(&self) -> (f32, f32, f32) {
        let (r, g, b) = (
            self.r as f32 / 255.0,
            self.g as f32 / 255.0,
            self.b as f32 / 255.0,
        );
        let max = r.max(g).max(b);
        let min = r.min(g).min(b);
        let l = (max + min) / 2.0;
        if (max - min).abs() < f32::EPSILON {
            return (0.0, 0.0, l);
        }
        let d = max - min;
        let s = if l > 0.5 {
            d / (2.0 - max - min)
        } else {
            d / (max + min)
        };
        let h = if (max - r).abs() < f32::EPSILON {
            ((g - b) / d).rem_euclid(6.0)
        } else if (max - g).abs() < f32::EPSILON {
            (b - r) / d + 2.0
        } else {
            (r - g) / d + 4.0
        } * 60.0;
        (h, s, l)
    }

    pub fn from_hsl(h: f32, s: f32, l: f32) -> Self {
        let h = h.rem_euclid(360.0);
        let s = s.clamp(0.0, 1.0);
        let l = l.clamp(0.0, 1.0);
        let c = (1.0 - (2.0 * l - 1.0).abs()) * s;
        let x = c * (1.0 - ((h / 60.0).rem_euclid(2.0) - 1.0).abs());
        let m = l - c / 2.0;
        let (r, g, b) = match h as u32 {
            0..=59 => (c, x, 0.0),
            60..=119 => (x, c, 0.0),
            120..=179 => (0.0, c, x),
            180..=239 => (0.0, x, c),
            240..=299 => (x, 0.0, c),
            _ => (c, 0.0, x),
        };
        let ch = |v: f32| ((v + m) * 255.0).round().clamp(0.0, 255.0) as u8;
        Self {
            r: ch(r),
            g: ch(g),
            b: ch(b),
        }
    }

    /// Lightness shift in HSL space; `amount` in -1.0..1.0.
    pub fn lighten(&self, amount: f32) -> Self {
        let (h, s, l) = self.to_hsl();
        Self::from_hsl(h, s, l + amount)
    }

    /// WCAG relative luminance (sRGB linearized).
    pub fn relative_luminance(&self) -> f64 {
        fn lin(c: u8) -> f64 {
            let c = c as f64 / 255.0;
            if c <= 0.04045 {
                c / 12.92
            } else {
                ((c + 0.055) / 1.055).powf(2.4)
            }
        }
        0.2126 * lin(self.r) + 0.7152 * lin(self.g) + 0.0722 * lin(self.b)
    }
}

/// WCAG contrast ratio, 1.0..21.0. AA body text needs ≥ 4.5.
pub fn contrast_ratio(a: &Color, b: &Color) -> f64 {
    let (l1, l2) = (a.relative_luminance(), b.relative_luminance());
    let (hi, lo) = if l1 >= l2 { (l1, l2) } else { (l2, l1) };
    (hi + 0.05) / (lo + 0.05)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_both_hex_forms_and_formats() {
        let c = Color::parse("#7aa2f7").unwrap();
        assert_eq!((c.r, c.g, c.b), (0x7a, 0xa2, 0xf7));
        assert_eq!(Color::parse("7aa2f7").unwrap(), c);
        assert_eq!(c.hex(), "#7aa2f7");
        assert_eq!(c.strip(), "7aa2f7");
        assert_eq!(c.rgb_decimal(), "122,162,247");
    }

    #[test]
    fn rejects_garbage_with_a_human_hint() {
        for bad in ["", "#12345", "#12345g", "red"] {
            match Color::parse(bad) {
                Err(StudioError::ParseFailed { hint, .. }) => {
                    assert!(hint.contains("hex"), "hint should teach: {hint}")
                }
                other => panic!("`{bad}` should fail, got {other:?}"),
            }
        }
    }

    #[test]
    fn hsl_round_trips_within_rounding() {
        for hex in ["#1a1b26", "#7aa2f7", "#c0caf5", "#ff0000", "#00ff99"] {
            let c = Color::parse(hex).unwrap();
            let (h, s, l) = c.to_hsl();
            let back = Color::from_hsl(h, s, l);
            assert!(
                (c.r as i16 - back.r as i16).abs() <= 1
                    && (c.g as i16 - back.g as i16).abs() <= 1
                    && (c.b as i16 - back.b as i16).abs() <= 1,
                "{hex} → {:?} → {:?}",
                (h, s, l),
                back
            );
        }
    }

    #[test]
    fn lighten_moves_lightness() {
        let c = Color::parse("#1a1b26").unwrap();
        let lighter = c.lighten(0.2);
        assert!(lighter.to_hsl().2 > c.to_hsl().2);
        // clamped at the extremes
        assert_eq!(
            Color::parse("#ffffff").unwrap().lighten(0.5).hex(),
            "#ffffff"
        );
    }

    #[test]
    fn wcag_contrast_matches_known_values() {
        let black = Color::parse("#000000").unwrap();
        let white = Color::parse("#ffffff").unwrap();
        assert!((contrast_ratio(&black, &white) - 21.0).abs() < 0.01);
        assert!((contrast_ratio(&white, &white) - 1.0).abs() < 0.01);
        // order-independent
        assert_eq!(
            contrast_ratio(&black, &white).to_bits(),
            contrast_ratio(&white, &black).to_bits()
        );
    }
}
