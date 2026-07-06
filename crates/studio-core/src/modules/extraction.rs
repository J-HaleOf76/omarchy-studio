//! Palette extraction (spec 04 §4, FR1.8, roadmap 0.6.2).
//!
//! Median-cut engine in the spirit of Aether's MIT Go extractor (bjarneo/aether,
//! attributed here and in `--about`): quantize the image into 16 clusters,
//! order by luminance, pick background/foreground/accent, map the ANSI slots
//! by hue, then apply per-mode post-transforms in HSL space and a WCAG ≥ 4.5
//! contrast pass. `extract` is a pure function over sampled pixels so goldens
//! lock its behavior without touching the filesystem; `extract_image` is the
//! thin decoding wrapper.

use std::path::Path;

use crate::error::Result;
use crate::modules::color::{contrast_ratio, Color};
use crate::StudioError;

/// Post-transform applied to the raw quantization (spec 04 §4 table).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    /// Raw median-cut, luminance-ordered.
    Normal,
    /// Saturation clamped ≤ 0.45 — calm, desaturated look.
    Muted,
    /// Tonal palette regenerated from the dominant hue (matugen-style).
    Material,
}

impl Mode {
    pub const ALL: [Mode; 3] = [Mode::Normal, Mode::Muted, Mode::Material];

    pub fn label(self) -> &'static str {
        match self {
            Mode::Normal => "normal",
            Mode::Muted => "muted",
            Mode::Material => "material",
        }
    }

    pub fn parse(s: &str) -> Option<Mode> {
        Mode::ALL.into_iter().find(|m| m.label() == s)
    }
}

/// Background polarity: follow the image, or force it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Bias {
    Auto,
    Dark,
    Light,
}

impl Bias {
    pub fn parse(s: &str) -> Option<Bias> {
        match s {
            "auto" => Some(Bias::Auto),
            "dark" => Some(Bias::Dark),
            "light" => Some(Bias::Light),
            _ => None,
        }
    }
}

/// A complete extracted theme palette, ready to render as `colors.toml`.
#[derive(Debug, Clone)]
pub struct Extraction {
    pub background: Color,
    pub foreground: Color,
    pub accent: Color,
    pub cursor: Color,
    pub selection_background: Color,
    pub selection_foreground: Color,
    pub ansi: [Color; 16],
    /// Light background — the wizard writes the `light.mode` marker from this.
    pub light: bool,
}

impl Extraction {
    /// Render in the exact shape Omarchy themes use (see any stock theme).
    pub fn to_colors_toml(&self) -> String {
        let mut out = String::new();
        out.push_str(&format!("accent = \"{}\"\n", self.accent.hex()));
        out.push_str(&format!("cursor = \"{}\"\n", self.cursor.hex()));
        out.push_str(&format!("foreground = \"{}\"\n", self.foreground.hex()));
        out.push_str(&format!("background = \"{}\"\n", self.background.hex()));
        out.push_str(&format!(
            "selection_foreground = \"{}\"\n",
            self.selection_foreground.hex()
        ));
        out.push_str(&format!(
            "selection_background = \"{}\"\n\n",
            self.selection_background.hex()
        ));
        for (i, c) in self.ansi.iter().enumerate() {
            out.push_str(&format!("color{i} = \"{}\"\n", c.hex()));
        }
        out
    }
}

// ── median cut ───────────────────────────────────────────────────────────────

/// Quantize samples into up to `n` clusters (average color per box).
fn median_cut(samples: &[(u8, u8, u8)], n: usize) -> Vec<Color> {
    if samples.is_empty() {
        return Vec::new();
    }
    let mut boxes: Vec<Vec<(u8, u8, u8)>> = vec![samples.to_vec()];
    while boxes.len() < n {
        // split the box with the largest single-channel range
        let (idx, chan) = boxes
            .iter()
            .enumerate()
            .map(|(i, b)| {
                let (mut lo, mut hi) = ([255u8; 3], [0u8; 3]);
                for &(r, g, bl) in b {
                    for (c, v) in [r, g, bl].into_iter().enumerate() {
                        lo[c] = lo[c].min(v);
                        hi[c] = hi[c].max(v);
                    }
                }
                let ranges = [hi[0] - lo[0], hi[1] - lo[1], hi[2] - lo[2]];
                let chan = (0..3).max_by_key(|&c| ranges[c]).unwrap();
                (i, chan, ranges[chan], b.len())
            })
            .filter(|&(_, _, range, len)| range > 0 && len > 1)
            .max_by_key(|&(_, _, range, _)| range)
            .map(|(i, c, _, _)| (i, c))
            .unwrap_or((usize::MAX, 0));
        if idx == usize::MAX {
            break; // nothing splittable left
        }
        let mut b = boxes.swap_remove(idx);
        let key = |p: &(u8, u8, u8)| match chan {
            0 => p.0,
            1 => p.1,
            _ => p.2,
        };
        b.sort_by_key(key);
        let mid = b.len() / 2;
        let right = b.split_off(mid);
        boxes.push(b);
        boxes.push(right);
    }
    boxes
        .iter()
        .map(|b| {
            let n = b.len() as u64;
            let (r, g, bl) = b.iter().fold((0u64, 0u64, 0u64), |acc, &(r, g, b)| {
                (acc.0 + r as u64, acc.1 + g as u64, acc.2 + b as u64)
            });
            Color {
                r: (r / n) as u8,
                g: (g / n) as u8,
                b: (bl / n) as u8,
            }
        })
        .collect()
}

// ── HSL helpers ──────────────────────────────────────────────────────────────

fn with_lightness(c: &Color, l: f32) -> Color {
    let (h, s, _) = c.to_hsl();
    Color::from_hsl(h, s, l.clamp(0.0, 1.0))
}

fn clamp_saturation(c: &Color, max_s: f32) -> Color {
    let (h, s, l) = c.to_hsl();
    if s > max_s {
        Color::from_hsl(h, max_s, l)
    } else {
        *c
    }
}

fn hue_distance(a: f32, b: f32) -> f32 {
    let d = (a - b).abs() % 360.0;
    d.min(360.0 - d)
}

/// Nudge foreground lightness away from the background until WCAG AA (4.5).
fn enforce_contrast(fg: &Color, bg: &Color) -> Color {
    let mut fg = *fg;
    let lighten = bg.relative_luminance() < 0.5;
    for _ in 0..20 {
        if contrast_ratio(&fg, bg) >= 4.5 {
            break;
        }
        let (h, s, l) = fg.to_hsl();
        let next = if lighten { l + 0.05 } else { l - 0.05 };
        fg = Color::from_hsl(h, s, next.clamp(0.0, 1.0));
        if !(0.01..=0.99).contains(&next) {
            break;
        }
    }
    fg
}

// ── the extractor ────────────────────────────────────────────────────────────

/// Pure extraction over pre-sampled pixels (spec: `extract(img, mode, bias)`).
pub fn extract(samples: &[(u8, u8, u8)], mode: Mode, bias: Bias) -> Extraction {
    let mut clusters = median_cut(samples, 16);
    if clusters.is_empty() {
        clusters = vec![Color {
            r: 26,
            g: 27,
            b: 38,
        }];
    }
    // pad degenerate images (few distinct colors) with lightness steps
    while clusters.len() < 16 {
        let base = clusters[clusters.len() % clusters.len().max(1)];
        let step = clusters.len() as f32 / 16.0;
        clusters.push(with_lightness(&base, step));
    }
    clusters.sort_by(|a, b| {
        a.relative_luminance()
            .partial_cmp(&b.relative_luminance())
            .unwrap()
    });

    // polarity: follow the image mean unless forced
    let mean_lum: f64 = samples
        .iter()
        .map(|&(r, g, b)| Color { r, g, b }.relative_luminance())
        .sum::<f64>()
        / samples.len().max(1) as f64;
    let light = match bias {
        Bias::Auto => mean_lum > 0.5,
        Bias::Dark => false,
        Bias::Light => true,
    };

    // background from the polarity end, pushed comfortably away from mid
    let bg_seed = if light {
        clusters[clusters.len() - 1]
    } else {
        clusters[0]
    };
    let (_, _, bg_l) = bg_seed.to_hsl();
    let background = if light {
        with_lightness(&bg_seed, bg_l.max(0.88))
    } else {
        with_lightness(&bg_seed, bg_l.min(0.13))
    };

    // foreground from the opposite end, contrast-enforced
    let fg_seed = if light {
        clusters[0]
    } else {
        clusters[clusters.len() - 1]
    };
    let foreground = enforce_contrast(&fg_seed, &background);

    // accent = most saturated mid-luminance cluster
    let accent_seed = clusters
        .iter()
        .filter(|c| {
            let (_, _, l) = c.to_hsl();
            (0.25..=0.75).contains(&l)
        })
        .max_by(|a, b| {
            let sa = a.to_hsl().1;
            let sb = b.to_hsl().1;
            sa.partial_cmp(&sb).unwrap()
        })
        .copied()
        .unwrap_or(clusters[clusters.len() / 2]);

    // ANSI slots: hue-matched chromatics, luminance-derived greys
    let targets = [0.0f32, 120.0, 60.0, 240.0, 300.0, 180.0]; // r g y b m c
    let (acc_h, acc_s, _) = accent_seed.to_hsl();
    let chroma: Vec<Color> = targets
        .iter()
        .map(|&t| {
            clusters
                .iter()
                .filter(|c| c.to_hsl().1 >= 0.15)
                .min_by(|a, b| {
                    hue_distance(a.to_hsl().0, t)
                        .partial_cmp(&hue_distance(b.to_hsl().0, t))
                        .unwrap()
                })
                .filter(|c| hue_distance(c.to_hsl().0, t) <= 60.0)
                .copied()
                // no cluster near this hue — synthesize from the accent's chroma
                .unwrap_or_else(|| Color::from_hsl(t, acc_s.max(0.35), 0.55))
        })
        .collect();
    let mut ansi = [background; 16];
    ansi[0] = if light {
        with_lightness(&background, 0.75)
    } else {
        with_lightness(&background, 0.22)
    };
    ansi[7] = with_lightness(&foreground, if light { 0.35 } else { 0.75 });
    ansi[8] = with_lightness(&ansi[0], if light { 0.6 } else { 0.35 });
    ansi[15] = with_lightness(&foreground, if light { 0.2 } else { 0.9 });
    for (i, c) in chroma.iter().enumerate() {
        ansi[i + 1] = *c;
        ansi[i + 9] = c.lighten(0.12);
    }

    let mut ex = Extraction {
        background,
        foreground,
        accent: accent_seed,
        cursor: accent_seed,
        selection_background: foreground,
        selection_foreground: background,
        ansi,
        light,
    };

    match mode {
        Mode::Normal => {}
        Mode::Muted => {
            ex.accent = clamp_saturation(&ex.accent, 0.45);
            ex.cursor = ex.accent;
            for c in ex.ansi.iter_mut() {
                *c = clamp_saturation(c, 0.45);
            }
        }
        Mode::Material => {
            // tonal palette from the dominant (accent) hue: neutral surfaces
            // tinted by the hue, chromatics as fixed-tone hue offsets
            let base_l = if light { 0.93 } else { 0.10 };
            ex.background = Color::from_hsl(acc_h, 0.18, base_l);
            ex.foreground = enforce_contrast(
                &Color::from_hsl(acc_h, 0.10, if light { 0.15 } else { 0.90 }),
                &ex.background,
            );
            ex.accent = Color::from_hsl(acc_h, acc_s.max(0.45), 0.60);
            ex.cursor = ex.accent;
            ex.selection_background = ex.foreground;
            ex.selection_foreground = ex.background;
            let offsets = [0.0f32, 30.0, -30.0, 60.0, -60.0, 180.0];
            for (i, off) in offsets.into_iter().enumerate() {
                let h = (acc_h + off).rem_euclid(360.0);
                ex.ansi[i + 1] = Color::from_hsl(h, 0.45, 0.55);
                ex.ansi[i + 9] = Color::from_hsl(h, 0.45, 0.67);
            }
            ex.ansi[0] = Color::from_hsl(acc_h, 0.15, if light { 0.80 } else { 0.20 });
            ex.ansi[7] = Color::from_hsl(acc_h, 0.10, if light { 0.35 } else { 0.75 });
            ex.ansi[8] = Color::from_hsl(acc_h, 0.12, if light { 0.60 } else { 0.35 });
            ex.ansi[15] = Color::from_hsl(acc_h, 0.08, if light { 0.20 } else { 0.90 });
        }
    }

    // final safety: fg/bg must clear AA whatever the mode did
    ex.foreground = enforce_contrast(&ex.foreground, &ex.background);
    ex
}

/// Decode an image, sample it down (~16k pixels), and extract.
pub fn extract_image(path: &Path, mode: Mode, bias: Bias) -> Result<Extraction> {
    let img = image::open(path)
        .map_err(|e| StudioError::External {
            cmd: format!("decode {}", path.display()),
            detail: e.to_string(),
        })?
        .to_rgb8();
    let (w, h) = img.dimensions();
    let stride = (((w as u64 * h as u64) / 16_384).max(1) as f64).sqrt() as u32;
    let stride = stride.max(1);
    let mut samples = Vec::with_capacity(16_384);
    let mut y = 0;
    while y < h {
        let mut x = 0;
        while x < w {
            let p = img.get_pixel(x, y);
            samples.push((p[0], p[1], p[2]));
            x += stride;
        }
        y += stride;
    }
    Ok(extract(&samples, mode, bias))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A synthetic "photo": four color fields with size-weighted dominance.
    fn synthetic() -> Vec<(u8, u8, u8)> {
        let mut px = Vec::new();
        px.extend(std::iter::repeat_n((18, 22, 30), 500)); // dark field
        px.extend(std::iter::repeat_n((200, 60, 50), 200)); // red
        px.extend(std::iter::repeat_n((60, 160, 90), 150)); // green
        px.extend(std::iter::repeat_n((90, 130, 220), 150)); // blue
        px.extend(std::iter::repeat_n((230, 230, 220), 100)); // highlight
        px
    }

    #[test]
    fn normal_mode_picks_dark_bg_readable_fg_saturated_accent() {
        let ex = extract(&synthetic(), Mode::Normal, Bias::Auto);
        assert!(!ex.light, "dark-dominant image ⇒ dark palette");
        assert!(ex.background.relative_luminance() < 0.2);
        assert!(
            contrast_ratio(&ex.foreground, &ex.background) >= 4.5,
            "WCAG AA enforced, got {:.2}",
            contrast_ratio(&ex.foreground, &ex.background)
        );
        let (_, s, _) = ex.accent.to_hsl();
        assert!(s > 0.3, "accent should be saturated, got s={s}");
        // ansi1 should be the red field, ansi4 the blue field
        let (h1, ..) = ex.ansi[1].to_hsl();
        assert!(hue_distance(h1, 0.0) < 45.0, "color1 hue {h1} not reddish");
        let (h4, ..) = ex.ansi[4].to_hsl();
        assert!(hue_distance(h4, 240.0) < 45.0, "color4 hue {h4} not bluish");
    }

    #[test]
    fn bias_light_flips_polarity() {
        let ex = extract(&synthetic(), Mode::Normal, Bias::Light);
        assert!(ex.light);
        assert!(ex.background.relative_luminance() > 0.5);
        assert!(contrast_ratio(&ex.foreground, &ex.background) >= 4.5);
    }

    #[test]
    fn muted_clamps_saturation_everywhere() {
        let ex = extract(&synthetic(), Mode::Muted, Bias::Dark);
        for (i, c) in ex.ansi.iter().enumerate() {
            let (_, s, _) = c.to_hsl();
            assert!(s <= 0.46, "ansi[{i}] saturation {s} > 0.45");
        }
        let (_, s, _) = ex.accent.to_hsl();
        assert!(s <= 0.46);
    }

    #[test]
    fn material_builds_a_tonal_family_from_the_dominant_hue() {
        let ex = extract(&synthetic(), Mode::Material, Bias::Dark);
        let (bg_h, bg_s, _) = ex.background.to_hsl();
        let (acc_h, ..) = ex.accent.to_hsl();
        assert!(
            hue_distance(bg_h, acc_h) < 5.0,
            "surfaces tinted by the dominant hue"
        );
        assert!(bg_s > 0.05, "material surfaces are tinted, not grey");
        assert!(contrast_ratio(&ex.foreground, &ex.background) >= 4.5);
    }

    #[test]
    fn colors_toml_round_trips_through_the_palette_parser() {
        let ex = extract(&synthetic(), Mode::Normal, Bias::Auto);
        let toml = ex.to_colors_toml();
        let palette = crate::modules::themes::Palette::parse(&toml).unwrap();
        assert_eq!(
            palette.get("background").unwrap().hex(),
            ex.background.hex()
        );
        assert_eq!(palette.get("color15").unwrap().hex(), ex.ansi[15].hex());
        assert!(palette.problems().is_empty(), "{:?}", palette.problems());
    }

    /// Golden: locks the full output for a fixed input. If the algorithm
    /// changes on purpose, update the constants deliberately.
    #[test]
    fn golden_normal_dark() {
        let ex = extract(&synthetic(), Mode::Normal, Bias::Auto);
        assert_eq!(
            (
                ex.background.hex(),
                ex.foreground.hex(),
                ex.accent.hex(),
                ex.ansi[1].hex(),
                ex.ansi[4].hex(),
            ),
            (
                "#152223".to_string(),
                "#e6e6dc".to_string(),
                "#5a82dc".to_string(),
                "#c83c32".to_string(),
                "#5a82dc".to_string(),
            ),
            "golden drifted — was the algorithm changed intentionally?"
        );
    }

    #[test]
    fn extract_image_decodes_and_samples() {
        let dir = std::env::temp_dir().join(format!("oms-extract-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("t.png");
        let mut img = image::RgbImage::new(64, 64);
        for (x, _y, p) in img.enumerate_pixels_mut() {
            *p = if x < 32 {
                image::Rgb([20, 24, 32])
            } else {
                image::Rgb([200, 80, 60])
            };
        }
        img.save(&file).unwrap();
        let ex = extract_image(&file, Mode::Normal, Bias::Auto).unwrap();
        assert!(!ex.light);
        assert!(contrast_ratio(&ex.foreground, &ex.background) >= 4.5);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
