//! omarchy-studio — TUI + CLI frontends over `studio-core`.
//!
//! Dispatch (spec 08): no args → TUI; subcommand → headless CLI.
//! clap/ratatui arrive with the rest of v0.1; until then this hand-rolled
//! dispatch covers `--version` and `doctor`.

use studio_core::cmd::RealRunner;
use studio_core::omarchy::{Capabilities, OmarchyPaths};

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        None => {
            eprintln!(
                "omarchy-studio {} — TUI not built yet (roadmap v0.1).",
                studio_core::VERSION
            );
            eprintln!("Available now: omarchy-studio doctor");
            std::process::exit(1);
        }
        Some("--version" | "-V") => {
            println!(
                "omarchy-studio {} (tested against omarchy {})",
                studio_core::VERSION,
                studio_core::TESTED_OMARCHY
            );
        }
        Some("doctor") => doctor(),
        Some(other) => {
            eprintln!("unknown command `{other}` — available: --version, doctor");
            std::process::exit(2);
        }
    }
}

/// Environment report (roadmap 0.1.2; grows drift/clobber/deps checks with
/// 0.1.10–0.1.11). Exit codes per spec 08: 0 ok, 4 omarchy not found.
fn doctor() {
    let paths = match OmarchyPaths::discover() {
        Ok(p) => p,
        Err(_) => {
            eprintln!("doctor: HOME is not set — cannot locate an Omarchy install.");
            std::process::exit(4);
        }
    };
    if !paths.installed() {
        eprintln!(
            "doctor: no Omarchy install at {} (set $OMARCHY_PATH if it lives elsewhere).",
            paths.system.display()
        );
        std::process::exit(4);
    }

    let caps = Capabilities::probe(&paths, &RealRunner);
    let theme = paths
        .current_theme_name()
        .unwrap_or_else(|_| "unknown".into());

    let mark = |b: bool| if b { "yes" } else { "NO" };
    println!("omarchy-studio doctor");
    println!(
        "  omarchy         {} at {}",
        caps.omarchy_version,
        paths.system.display()
    );
    println!(
        "  hyprland        {}",
        if caps.hyprland_version.is_empty() {
            "unavailable (not in a Hyprland session?)"
        } else {
            &caps.hyprland_version
        }
    );
    println!("  current theme   {theme}");
    println!("  capabilities");
    println!(
        "    config verification (hyprctl configerrors)  {}",
        mark(caps.has_configerrors)
    );
    println!(
        "    theme template engine (default/themed)      {}",
        mark(caps.has_templates)
    );
    println!(
        "    lifecycle hooks (omarchy-hook)               {}",
        mark(caps.has_hooks)
    );
    println!(
        "    stock floating-TUI window rule               {}",
        mark(caps.tui_float_rule)
    );
    println!(
        "    video wallpapers (mpvpaper)                  {}",
        mark(caps.video_wallpapers)
    );

    if caps.omarchy_dirty {
        println!();
        println!("  ⚠ your Omarchy checkout has local modifications.");
        println!("    These will conflict when `omarchy-update` pulls. Consider moving");
        println!("    patches into ~/.config/omarchy/ override surfaces (hooks, themed/).");
    }
}
