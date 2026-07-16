//! Theme-from-wallpaper materializer (spec 04 §7, roadmap 0.6.5).
//!
//! The end of the wizard's `PickImage → Extract → Refine → Meta` flow: write
//! a **100 % standard theme directory** under `~/.config/omarchy/themes/` so
//! the result is immediately listable, appliable, forkable, and shareable —
//! nothing about it says "made by Studio":
//!
//! ```text
//! themes/<slug>/
//!   colors.toml            ← the extraction, Omarchy's exact shape
//!   backgrounds/<image>    ← the wallpaper it was crafted from
//!   preview.png            ← wallpaper thumb + palette strip
//!   light.mode             ← only when the palette is light-polarity
//! ```
//!
//! Never overwrites an existing theme; the frontends offer `fork` for that.

use std::path::{Path, PathBuf};

use crate::error::Result;
use crate::modules::extraction::Extraction;
use crate::modules::themes::slugify;
use crate::omarchy::OmarchyPaths;
use crate::StudioError;

/// Preview image geometry: thumbnail above, palette strip below.
const PREVIEW_W: u32 = 480;
const PREVIEW_THUMB_H: u32 = 270;
const PREVIEW_STRIP_H: u32 = 28;

/// Marker description on the optional instant-theme keybind (ROADMAP 0.9.2).
pub const BIND_DESC: &str = "Theme from current wallpaper";

/// The wallpaper currently on screen — target of the `current/background`
/// link Omarchy's bg-set scripts maintain.
pub fn current_background(paths: &OmarchyPaths) -> Result<PathBuf> {
    let link = paths.config.join("current/background");
    std::fs::read_link(&link).map_err(|_| StudioError::External {
        cmd: "theme new".into(),
        detail: format!(
            "no current wallpaper recorded at {} — set one first",
            link.display()
        ),
    })
}

/// A theme name that won't collide: `base` if its slug is free, else the
/// first free `base-2`, `base-3`, … — what lets the instant-theme keybind
/// run non-interactively without clobber-or-fail.
pub fn unique_name(paths: &OmarchyPaths, base: &str) -> String {
    let themes = paths.config.join("themes");
    if !themes.join(slugify(base)).exists() {
        return base.to_string();
    }
    (2..)
        .map(|n| format!("{base}-{n}"))
        .find(|cand| !themes.join(slugify(cand)).exists())
        .expect("some suffix is free")
}

/// Write the theme dir. Returns its path. The image must be decodable (the
/// wizard only admits stills/gifs); `preview.png` is best-effort — a theme
/// without a preview beats no theme.
pub fn materialize(
    paths: &OmarchyPaths,
    name: &str,
    image: &Path,
    ex: &Extraction,
) -> Result<PathBuf> {
    let slug = slugify(name);
    if slug.is_empty() {
        return Err(StudioError::External {
            cmd: "theme new".into(),
            detail: "the theme needs a name".into(),
        });
    }
    let dir = paths.config.join("themes").join(&slug);
    if dir.exists() {
        return Err(StudioError::External {
            cmd: "theme new".into(),
            detail: format!("theme `{slug}` already exists — pick another name"),
        });
    }
    let image_name = image
        .file_name()
        .ok_or_else(|| StudioError::External {
            cmd: "theme new".into(),
            detail: format!("{} has no file name", image.display()),
        })?
        .to_owned();

    std::fs::create_dir_all(dir.join("backgrounds"))?;
    std::fs::write(dir.join("colors.toml"), ex.to_colors_toml())?;
    std::fs::copy(image, dir.join("backgrounds").join(&image_name))?;
    if ex.light {
        std::fs::write(dir.join("light.mode"), "")?;
    }
    if let Some(preview) = compose_preview(image, ex) {
        let _ = preview.save(dir.join("preview.png"));
    }
    Ok(dir)
}

/// Wallpaper thumbnail with the extracted palette laid out underneath —
/// the sharing-friendly at-a-glance card every stock theme ships.
fn compose_preview(image: &Path, ex: &Extraction) -> Option<image::RgbImage> {
    let thumb = image::open(image)
        .ok()?
        .thumbnail(PREVIEW_W, PREVIEW_THUMB_H)
        .to_rgb8();
    let (tw, th) = thumb.dimensions();
    let mut canvas = image::RgbImage::from_pixel(
        tw,
        th + PREVIEW_STRIP_H,
        image::Rgb([ex.background.r, ex.background.g, ex.background.b]),
    );
    image::imageops::overlay(&mut canvas, &thumb, 0, 0);

    // strip: bg · fg · accent, then ANSI 1–6 — the colors that define the feel
    let strip: Vec<crate::modules::color::Color> = [ex.background, ex.foreground, ex.accent]
        .into_iter()
        .chain(ex.ansi[1..7].iter().copied())
        .collect();
    let cell = tw / strip.len() as u32;
    for (i, c) in strip.iter().enumerate() {
        let x0 = i as u32 * cell;
        // last cell absorbs the rounding remainder
        let x1 = if i == strip.len() - 1 { tw } else { x0 + cell };
        for x in x0..x1 {
            for y in th..th + PREVIEW_STRIP_H {
                canvas.put_pixel(x, y, image::Rgb([c.r, c.g, c.b]));
            }
        }
    }
    Some(canvas)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::modules::extraction::{extract, Bias, Mode};

    fn fake_paths(tag: &str) -> OmarchyPaths {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .subsec_nanos();
        let root = std::env::temp_dir().join(format!(
            "omarchy-studio-wiz-{tag}-{}-{nanos}",
            std::process::id()
        ));
        std::fs::create_dir_all(root.join("config/omarchy")).unwrap();
        OmarchyPaths {
            system: root.join("sys/omarchy"),
            config: root.join("config/omarchy"),
            state: root.join("state/omarchy"),
        }
    }

    /// A tiny real PNG on disk: dark field + a few color blocks.
    fn test_image(dir: &Path) -> PathBuf {
        let mut img = image::RgbImage::from_pixel(32, 32, image::Rgb([18, 22, 30]));
        for y in 0..8 {
            for x in 0..16 {
                img.put_pixel(x, y, image::Rgb([200, 60, 50]));
            }
            for x in 16..32 {
                img.put_pixel(x, y, image::Rgb([90, 130, 220]));
            }
        }
        let path = dir.join("wall.png");
        img.save(&path).unwrap();
        path
    }

    fn extraction_of(path: &Path, bias: Bias) -> Extraction {
        extract(
            &crate::modules::extraction::sample_image(path).unwrap(),
            Mode::Normal,
            bias,
        )
    }

    #[test]
    fn materialize_writes_a_standard_theme_dir() {
        let paths = fake_paths("std");
        let img = test_image(paths.config.parent().unwrap());
        let ex = extraction_of(&img, Bias::Dark);

        let dir = materialize(&paths, "Jade City", &img, &ex).unwrap();
        assert_eq!(dir, paths.config.join("themes/jade-city"));
        // colors.toml is a valid Omarchy palette
        let colors = std::fs::read_to_string(dir.join("colors.toml")).unwrap();
        let pal = crate::modules::themes::Palette::parse(&colors).unwrap();
        assert_eq!(pal.get("background").unwrap(), ex.background);
        // wallpaper travels with the theme
        assert!(dir.join("backgrounds/wall.png").is_file());
        // preview decodes and has the strip below the thumb
        let preview = image::open(dir.join("preview.png")).unwrap().to_rgb8();
        assert!(preview.height() > PREVIEW_STRIP_H);
        // dark palette → no light.mode marker
        assert!(!dir.join("light.mode").exists());

        // same name again is refused, not clobbered
        assert!(materialize(&paths, "Jade City", &img, &ex).is_err());
        // and a nameless theme is refused
        assert!(materialize(&paths, "   ", &img, &ex).is_err());
    }

    #[test]
    fn unique_name_skips_taken_slugs() {
        let paths = fake_paths("uniq");
        assert_eq!(unique_name(&paths, "Jade City"), "Jade City");
        std::fs::create_dir_all(paths.config.join("themes/jade-city")).unwrap();
        assert_eq!(unique_name(&paths, "Jade City"), "Jade City-2");
        std::fs::create_dir_all(paths.config.join("themes/jade-city-2")).unwrap();
        assert_eq!(unique_name(&paths, "Jade City"), "Jade City-3");
    }

    #[test]
    fn current_background_follows_the_link() {
        let paths = fake_paths("curbg");
        assert!(current_background(&paths).is_err());
        let img = test_image(paths.config.parent().unwrap());
        std::fs::create_dir_all(paths.config.join("current")).unwrap();
        std::os::unix::fs::symlink(&img, paths.config.join("current/background")).unwrap();
        assert_eq!(current_background(&paths).unwrap(), img);
    }

    #[test]
    fn light_bias_writes_the_marker() {
        let paths = fake_paths("light");
        let img = test_image(paths.config.parent().unwrap());
        let ex = extraction_of(&img, Bias::Light);
        assert!(ex.light);
        let dir = materialize(&paths, "daylight", &img, &ex).unwrap();
        assert!(dir.join("light.mode").exists());
    }
}
