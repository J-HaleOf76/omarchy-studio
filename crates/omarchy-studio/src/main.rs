//! omarchy-studio — TUI + CLI frontends over `studio-core`.
//!
//! Dispatch (spec 08): no args → TUI; subcommand → headless CLI.
//! Hand-rolled dispatch for now; clap arrives when the tree grows (v0.2).

mod tui;

use studio_core::cmd::{CommandRunner, RealRunner};
use studio_core::deps::{probe_all, DepStatus, Registry};
use studio_core::modules::themes::{slugify, ThemeOrigin, ThemeStore};
use studio_core::omarchy::{cmds, Capabilities, OmarchyPaths};
use studio_core::snapshot::{SnapshotKind, SnapshotStore};

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let argv: Vec<&str> = args.iter().map(String::as_str).collect();
    let code = match argv.as_slice() {
        [] => tui::run(),
        ["--version" | "-V"] => {
            println!(
                "omarchy-studio {} (tested against omarchy {})",
                studio_core::VERSION,
                studio_core::TESTED_OMARCHY
            );
            0
        }
        ["doctor", rest @ ..] => doctor(rest.contains(&"--deps")),
        ["theme", rest @ ..] => theme(rest),
        ["snapshot", rest @ ..] => snapshot(rest),
        ["looknfeel", rest @ ..] => looknfeel(rest),
        ["preset", rest @ ..] => preset(rest),
        ["toggle", rest @ ..] => toggle(rest),
        ["animations", rest @ ..] => animations(rest),
        ["waybar", rest @ ..] => waybar(rest),
        ["install-integration"] => install_integration(),
        ["uninstall"] => uninstall(),
        [other, ..] => {
            eprintln!("unknown command `{other}` — see `omarchy-studio` for the list");
            2
        }
    };
    std::process::exit(code);
}

fn omarchy() -> Option<OmarchyPaths> {
    let paths = OmarchyPaths::discover().ok()?;
    if paths.installed() {
        Some(paths)
    } else {
        eprintln!(
            "no Omarchy install at {} (set $OMARCHY_PATH if it lives elsewhere).",
            paths.system.display()
        );
        None
    }
}

fn history() -> Result<SnapshotStore, i32> {
    let state = studio_core::studio_state_dir();
    SnapshotStore::open_or_init(state.join("history"), Box::new(RealRunner)).map_err(|e| {
        eprintln!("snapshot store unavailable: {e:?}");
        1
    })
}

// ── doctor ──────────────────────────────────────────────────────────────────

fn doctor(with_deps: bool) -> i32 {
    let Some(paths) = omarchy() else { return 4 };
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

    if with_deps {
        println!();
        println!("  dependencies");
        match Registry::builtin() {
            Ok(reg) => {
                for r in probe_all(&reg, &RealRunner) {
                    let state = match r.status {
                        DepStatus::Present => "ok      ".into(),
                        DepStatus::Missing => "MISSING ".into(),
                        DepStatus::Deferred => "deferred".into(),
                        DepStatus::Degraded(ref why) => format!("degraded ({why})"),
                    };
                    println!("    {:<18} {}  — {}", r.id, state, r.reason);
                    if r.status == DepStatus::Missing {
                        if let Some(install) = &r.install {
                            println!("    {:<18} install: {}", "", install);
                        }
                        println!("    {:<18} without it: {}", "", r.fallback);
                    }
                }
            }
            Err(e) => println!("    registry failed to load: {e:?}"),
        }
    }
    0
}

// ── theme ───────────────────────────────────────────────────────────────────

fn theme(args: &[&str]) -> i32 {
    let Some(paths) = omarchy() else { return 4 };
    let store = ThemeStore::from_paths(&paths);
    match args {
        ["list"] => {
            let current = paths.current_theme_name().unwrap_or_default();
            for t in store.list() {
                let marker = if t.slug == current { "*" } else { " " };
                let origin = match t.origin {
                    ThemeOrigin::System => "",
                    ThemeOrigin::User => " (user)",
                    ThemeOrigin::Overlay => " (user overlay)",
                };
                let light = if t.light() { " · light" } else { "" };
                println!("{marker} {:<24}{origin}{light}", t.pretty);
            }
            0
        }
        ["current"] => match paths.current_theme_name() {
            Ok(name) => {
                println!("{name}");
                0
            }
            Err(_) => {
                eprintln!("no current theme recorded (is Omarchy fully set up?)");
                1
            }
        },
        ["apply", name] => {
            let slug = slugify(name);
            if store.get(&slug).is_none() {
                eprintln!("no theme named `{slug}` — see `omarchy-studio theme list`");
                return 1;
            }
            match RealRunner.run(&cmds::theme_set(&slug)) {
                Ok(out) if out.ok() => {
                    println!("applied {slug}");
                    0
                }
                Ok(out) => {
                    eprintln!("omarchy-theme-set failed: {}", out.stderr.trim());
                    1
                }
                Err(e) => {
                    eprintln!("could not run omarchy-theme-set: {e:?}");
                    1
                }
            }
        }
        ["fork", src, new] => {
            let Some(theme) = store.get(&slugify(src)) else {
                eprintln!("no theme named `{}` to fork", slugify(src));
                return 1;
            };
            match store.fork(&theme, new) {
                Ok(t) => {
                    let dest = t
                        .user_dir
                        .as_deref()
                        .map(|d| d.display().to_string())
                        .unwrap_or_default();
                    println!("forked {} → {} at {}", theme.slug, t.slug, dest);
                    println!("apply it with: omarchy-studio theme apply {}", t.slug);
                    0
                }
                Err(e) => {
                    eprintln!("fork failed: {e:?}");
                    1
                }
            }
        }
        _ => {
            eprintln!(
                "usage: omarchy-studio theme list | current | apply <name> | fork <src> <new>"
            );
            2
        }
    }
}

// ── snapshot ────────────────────────────────────────────────────────────────

fn snapshot(args: &[&str]) -> i32 {
    let store = match history() {
        Ok(s) => s,
        Err(code) => return code,
    };
    match args {
        ["list"] => {
            let entries = store.list(30).unwrap_or_default();
            if entries.is_empty() {
                println!("no snapshots yet — they appear when Studio first changes a file");
                return 0;
            }
            for e in entries {
                let kind = e
                    .kind
                    .map(|k| format!("{k:?}").to_lowercase())
                    .unwrap_or_else(|| "?".into());
                println!("{}  {:<8} {}  ({})", e.id, kind, e.summary, e.timestamp);
            }
            0
        }
        ["undo"] => {
            match store.undo_last() {
                Ok(files) => {
                    println!("restored {} file(s):", files.len());
                    for f in files {
                        println!("  {}", f.display());
                    }
                    println!("note: reload affected apps (e.g. hyprctl reload) if changes aren't visible");
                    0
                }
                Err(e) => {
                    eprintln!("{e:?}");
                    1
                }
            }
        }
        ["restore", id] => match store.restore(id) {
            Ok(files) => {
                println!("restored state as of {id} ({} file(s))", files.len());
                0
            }
            Err(e) => {
                eprintln!("restore failed: {e:?}");
                1
            }
        },
        _ => {
            eprintln!("usage: omarchy-studio snapshot list | undo | restore <id>");
            2
        }
    }
}

// ── look & feel ───────────────────────────────────────────────────────────────

fn looknfeel(args: &[&str]) -> i32 {
    use studio_core::modules::looknfeel::{lookup, LookFeel, SETTINGS};
    let Some(paths) = omarchy() else { return 4 };
    match args {
        ["list"] => {
            let lf = LookFeel::load(&paths);
            for s in SETTINGS {
                let mark = if lf.is_overridden(s.key) { "*" } else { " " };
                println!("{mark} {:<28} {:>8}   {}", s.key, lf.value(s.key), s.label);
            }
            println!("\n(* = changed from the Omarchy default)");
            0
        }
        ["get", key] => {
            if lookup(key).is_none() {
                eprintln!("unknown setting `{key}` — see `looknfeel list`");
                return 2;
            }
            println!("{}", LookFeel::load(&paths).value(key));
            0
        }
        ["set", key, value] => {
            let mut lf = LookFeel::load(&paths);
            if let Err(e) = lf.set(key, value) {
                eprintln!("{e}");
                return 2;
            }
            apply_looknfeel(&paths, &lf, &format!("looknfeel {key}={}", lf.value(key)))
        }
        _ => {
            eprintln!("usage: looknfeel list | get <key> | set <key> <value>");
            2
        }
    }
}

fn preset(args: &[&str]) -> i32 {
    use studio_core::modules::looknfeel::{preset as find, LookFeel, PRESETS};
    let Some(paths) = omarchy() else { return 4 };
    match args {
        ["list"] => {
            for p in PRESETS {
                println!("{:<18} {}", p.name, p.blurb);
            }
            0
        }
        ["try", name @ ..] | ["apply", name @ ..] if !name.is_empty() => {
            let joined = name.join(" ");
            let Some(p) = find(&joined) else {
                eprintln!("unknown preset `{joined}` — see `preset list`");
                return 2;
            };
            let mut lf = LookFeel::load(&paths);
            lf.apply_preset(p);
            if matches!(args, ["apply", ..]) {
                apply_looknfeel(&paths, &lf, &format!("preset {}", p.name))
            } else {
                match lf.preview_all(&RealRunner) {
                    Ok(()) => {
                        println!(
                            "Trying “{}” live (not saved). Run `preset apply {}` to keep it.",
                            p.name, p.name
                        );
                        0
                    }
                    Err(e) => {
                        eprintln!("preview failed: {e:?}");
                        1
                    }
                }
            }
        }
        _ => {
            eprintln!("usage: preset list | try <name> | apply <name>");
            2
        }
    }
}

/// Snapshot the bindings-adjacent looknfeel.conf, apply the model live, and
/// record the result — the shared save path for `looknfeel set` / `preset apply`.
fn apply_looknfeel(
    paths: &OmarchyPaths,
    lf: &studio_core::modules::looknfeel::LookFeel,
    summary: &str,
) -> i32 {
    let file = paths.hypr_config().join("looknfeel.conf");
    let store = history().ok();
    if let Some(s) = &store {
        let _ = s.record(
            SnapshotKind::Pre,
            &format!("before {summary}"),
            std::slice::from_ref(&file),
            "looknfeel",
            &[],
        );
    }
    match lf.apply(paths, &RealRunner) {
        Ok(_) => {
            if let Some(s) = &store {
                let _ = s.record(
                    SnapshotKind::Post,
                    summary,
                    std::slice::from_ref(&file),
                    "looknfeel",
                    &[],
                );
            }
            println!("{summary} · undo with `omarchy-studio snapshot undo`");
            0
        }
        Err(e) => {
            eprintln!("apply failed: {e:?}");
            1
        }
    }
}

fn animations(args: &[&str]) -> i32 {
    use studio_core::modules::animations::{apply, current, preset, PRESETS};
    let Some(paths) = omarchy() else { return 4 };
    match args {
        ["list"] | [] => {
            let cur = current(&paths);
            for p in PRESETS {
                let mark = if p.name == cur { "●" } else { " " };
                println!("{mark} {:<10} {}", p.name, p.blurb);
            }
            0
        }
        ["current"] => {
            println!("{}", current(&paths));
            0
        }
        ["apply", name @ ..] if !name.is_empty() => {
            let joined = name.join(" ");
            let Some(p) = preset(&joined) else {
                eprintln!("unknown animation preset `{joined}` — see `animations list`");
                return 2;
            };
            let file = paths.hypr_config().join("looknfeel.conf");
            let store = history().ok();
            if let Some(s) = &store {
                let _ = s.record(
                    SnapshotKind::Pre,
                    &format!("before animations {}", p.name),
                    std::slice::from_ref(&file),
                    "animations",
                    &[],
                );
            }
            match apply(&paths, p, &RealRunner) {
                Ok(_) => {
                    if let Some(s) = &store {
                        let _ = s.record(
                            SnapshotKind::Post,
                            &format!("animations {}", p.name),
                            std::slice::from_ref(&file),
                            "animations",
                            &[],
                        );
                    }
                    println!(
                        "Animations: {} · undo with `omarchy-studio snapshot undo`",
                        p.name
                    );
                    0
                }
                Err(e) => {
                    eprintln!("apply failed: {e:?}");
                    1
                }
            }
        }
        _ => {
            eprintln!("usage: animations list | current | apply <name>");
            2
        }
    }
}

fn waybar(args: &[&str]) -> i32 {
    use studio_core::modules::waybar::{label_for, WaybarConfig, LANES};
    let Some(paths) = omarchy() else { return 4 };
    // left/center/right → the full lane key
    let lane_key = |name: &str| match name {
        "left" => Some("modules-left"),
        "center" => Some("modules-center"),
        "right" => Some("modules-right"),
        _ => None,
    };

    // Geometry via style.css managed block (font-size / radius).
    if let ["style", rest @ ..] = args {
        return waybar_style(&paths, rest);
    }

    // Read-only listing needs no snapshot.
    if let ["modules"] | [] = args {
        match WaybarConfig::load(&paths) {
            Ok(cfg) => {
                for (lane, short) in LANES.iter().zip(["left", "center", "right"]) {
                    println!("{short}:");
                    for id in cfg.lane(lane) {
                        println!("    {:<26} {}", label_for(&id), id);
                    }
                }
                0
            }
            Err(e) => {
                eprintln!("can't read waybar config: {e:?}");
                1
            }
        }
    } else {
        // mutating verbs share load → edit → snapshot → apply
        let mut cfg = match WaybarConfig::load(&paths) {
            Ok(c) => c,
            Err(e) => {
                eprintln!("can't read waybar config: {e:?}");
                return 1;
            }
        };
        let (edit, summary): (studio_core::error::Result<()>, String) = match args {
            ["add", lane, id] => match lane_key(lane) {
                Some(l) => (cfg.add(l, id), format!("waybar add {id} → {lane}")),
                None => {
                    eprintln!("lane must be left|center|right");
                    return 2;
                }
            },
            ["remove", lane, id] => match lane_key(lane) {
                Some(l) => (cfg.remove(l, id), format!("waybar remove {id}")),
                None => {
                    eprintln!("lane must be left|center|right");
                    return 2;
                }
            },
            ["move", id, lane] => match lane_key(lane) {
                Some(l) => (cfg.move_to(id, l), format!("waybar move {id} → {lane}")),
                None => {
                    eprintln!("lane must be left|center|right");
                    return 2;
                }
            },
            ["set", path, value] => (
                cfg.set_scalar(path, value),
                format!("waybar set {path}={value}"),
            ),
            _ => {
                eprintln!(
                    "usage: waybar modules | add <lane> <id> | remove <lane> <id> | move <id> <lane> | set <path> <value>"
                );
                return 2;
            }
        };
        if let Err(e) = edit {
            eprintln!("{e:?}");
            return 2;
        }
        let file = cfg.path().to_path_buf();
        let store = history().ok();
        if let Some(s) = &store {
            let _ = s.record(
                SnapshotKind::Pre,
                &format!("before {summary}"),
                std::slice::from_ref(&file),
                "waybar",
                &[],
            );
        }
        match cfg.apply(&RealRunner) {
            Ok(()) => {
                if let Some(s) = &store {
                    let _ = s.record(
                        SnapshotKind::Post,
                        &summary,
                        std::slice::from_ref(&file),
                        "waybar",
                        &[],
                    );
                }
                println!("{summary} · undo with `omarchy-studio snapshot undo`");
                0
            }
            Err(e) => {
                eprintln!("apply failed: {e:?}");
                1
            }
        }
    }
}

fn waybar_style(paths: &OmarchyPaths, args: &[&str]) -> i32 {
    use studio_core::modules::waybar::WaybarStyle;
    let mut style = WaybarStyle::load(paths);
    let summary: String = match args {
        ["show"] | [] => {
            let fs = style
                .font_size
                .map(|v| v.to_string())
                .unwrap_or_else(|| "default".into());
            let r = style
                .radius
                .map(|v| v.to_string())
                .unwrap_or_else(|| "default".into());
            println!("font-size: {fs}\nradius:    {r}");
            return 0;
        }
        ["font-size", n] => match n.parse::<u32>() {
            Ok(v) if (6..=40).contains(&v) => {
                style.font_size = Some(v);
                format!("waybar font-size {v}px")
            }
            _ => {
                eprintln!("font-size must be 6–40");
                return 2;
            }
        },
        ["radius", n] => match n.parse::<u32>() {
            Ok(v) if v <= 30 => {
                style.radius = Some(v);
                format!("waybar radius {v}px")
            }
            _ => {
                eprintln!("radius must be 0–30");
                return 2;
            }
        },
        ["reset"] => {
            style = WaybarStyle::default();
            "waybar style reset".into()
        }
        _ => {
            eprintln!("usage: waybar style show | font-size <n> | radius <n> | reset");
            return 2;
        }
    };
    let store = history().ok();
    let path = paths
        .config
        .parent()
        .map(|c| c.join("waybar/style.css"))
        .unwrap_or_default();
    if let Some(s) = &store {
        let _ = s.record(
            SnapshotKind::Pre,
            &format!("before {summary}"),
            std::slice::from_ref(&path),
            "waybar",
            &[],
        );
    }
    match style.apply(paths, &RealRunner) {
        Ok(_) => {
            if let Some(s) = &store {
                let _ = s.record(
                    SnapshotKind::Post,
                    &summary,
                    std::slice::from_ref(&path),
                    "waybar",
                    &[],
                );
            }
            println!("{summary} · undo with `omarchy-studio snapshot undo`");
            0
        }
        Err(e) => {
            eprintln!("apply failed: {e:?}");
            1
        }
    }
}

fn toggle(args: &[&str]) -> i32 {
    use studio_core::modules::toggles::{lookup, TOGGLES};
    let Some(_paths) = omarchy() else { return 4 };
    let state = |t: &studio_core::modules::toggles::Toggle| match t.is_on(&RealRunner) {
        Some(true) => "on",
        Some(false) => "off",
        None => "—",
    };
    match args {
        ["list"] | [] => {
            for t in TOGGLES {
                println!("{:<22} {:>3}   {}", t.id, state(t), t.label);
            }
            0
        }
        [id] => {
            let Some(t) = lookup(id) else {
                eprintln!("unknown toggle `{id}` — see `toggle list`");
                return 2;
            };
            match t.toggle(&RealRunner) {
                Ok(()) => {
                    println!("Toggled {} (now {})", t.label, state(t));
                    0
                }
                Err(e) => {
                    eprintln!("toggle failed: {e:?}");
                    1
                }
            }
        }
        _ => {
            eprintln!("usage: toggle list | toggle <id>");
            2
        }
    }
}

// ── menu integration ─────────────────────────────────────────────────────────

fn install_integration() -> i32 {
    let Some(paths) = omarchy() else { return 4 };
    let store = history().ok();
    let file = studio_core::integration::managed_file(&paths);
    if let Some(s) = &store {
        let _ = s.record(
            SnapshotKind::Pre,
            "before install-integration",
            std::slice::from_ref(&file),
            "integration",
            &[],
        );
    }
    match studio_core::integration::install_menu(&paths) {
        Ok(path) => {
            if let Some(s) = &store {
                let _ = s.record(
                    SnapshotKind::Post,
                    "install-integration",
                    std::slice::from_ref(&path),
                    "integration",
                    &[],
                );
            }
            println!("Added Studio to the Omarchy menu.");
            println!("  Open it with Super+Alt+Space -> Style -> Studio.");
            println!("  {}", path.display());
            println!("Undo any time with: omarchy-studio uninstall");
            0
        }
        Err(e) => {
            eprintln!("install failed: {e:?}");
            1
        }
    }
}

fn uninstall() -> i32 {
    let Some(paths) = omarchy() else { return 4 };
    if !studio_core::integration::is_installed(&paths) {
        println!("Studio was not installed in the Omarchy menu; nothing to remove.");
        return 0;
    }
    let store = history().ok();
    let file = studio_core::integration::managed_file(&paths);
    if let Some(s) = &store {
        let _ = s.record(
            SnapshotKind::Pre,
            "before uninstall",
            std::slice::from_ref(&file),
            "integration",
            &[],
        );
    }
    match studio_core::integration::uninstall_menu(&paths) {
        Ok(_) => {
            println!("Removed Studio from the Omarchy menu. Your other configs are untouched.");
            0
        }
        Err(e) => {
            eprintln!("uninstall failed: {e:?}");
            1
        }
    }
}
