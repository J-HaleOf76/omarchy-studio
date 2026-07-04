//! Golden-file round-trip tests (spec 09 §2): every parser must reproduce
//! real Omarchy config bytes exactly. As new dialects land (hyprlang in
//! 0.2.1, JSONC in 0.4.1) their fixtures join `tests/fixtures/` and this file.
//!
//! Fixtures are verbatim copies of Omarchy 3.8.2 theme files, so a change in
//! the parser that would mangle a real user's config fails here first.

use std::path::PathBuf;

use studio_core::modules::color::Color;
use studio_core::modules::themes::{Palette, PALETTE_KEYS};

fn fixtures() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/themes")
}

fn each_colors_toml(mut f: impl FnMut(&str, &str)) {
    let dir = fixtures();
    let mut any = false;
    for entry in std::fs::read_dir(&dir).expect("fixtures dir") {
        let path = entry.unwrap().path();
        if path.to_string_lossy().ends_with(".colors.toml") {
            any = true;
            let name = path.file_name().unwrap().to_string_lossy().to_string();
            let content = std::fs::read_to_string(&path).unwrap();
            f(&name, &content);
        }
    }
    assert!(any, "no colors.toml fixtures found in {}", dir.display());
}

#[test]
fn colors_toml_round_trips_byte_identical() {
    each_colors_toml(|name, content| {
        let p = Palette::parse(content).unwrap_or_else(|e| panic!("{name}: parse failed: {e:?}"));
        assert_eq!(p.to_string(), content, "{name} did not round-trip");
    });
}

#[test]
fn every_fixture_defines_the_core_palette_keys() {
    each_colors_toml(|name, content| {
        let p = Palette::parse(content).unwrap();
        // Real Omarchy themes must at least define accent/background/foreground
        // and the 16 ANSI colors; the selection/cursor pair is optional.
        for key in PALETTE_KEYS {
            let optional = matches!(
                key,
                "cursor" | "selection_foreground" | "selection_background"
            );
            if !optional {
                assert!(
                    p.get(key).is_some(),
                    "{name}: expected key `{key}` to be a valid colour"
                );
            }
        }
    });
}

#[test]
fn editing_a_fixture_preserves_all_other_bytes() {
    each_colors_toml(|name, content| {
        let mut p = Palette::parse(content).unwrap();
        let before = p.to_string();
        // Flip the accent to a known value and back; the rest must be untouched.
        let original = p.get("accent").expect("accent present");
        p.set("accent", &Color::parse("#abcdef").unwrap());
        assert!(
            p.to_string().contains("#abcdef"),
            "{name}: edit not applied"
        );
        p.set("accent", &original);
        assert_eq!(
            p.to_string(),
            before,
            "{name}: round-trip through an edit changed unrelated bytes"
        );
    });
}
