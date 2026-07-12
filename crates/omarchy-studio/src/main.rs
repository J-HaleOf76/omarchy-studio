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
        ["doctor", rest @ ..] => doctor(rest.contains(&"--deps"), rest.contains(&"--quiet")),
        ["hooks", rest @ ..] => hooks_cmd(rest),
        ["hook", rest @ ..] => hook_event(rest),
        ["theme", rest @ ..] => theme(rest),
        ["snapshot", rest @ ..] => snapshot(rest),
        ["looknfeel", rest @ ..] => looknfeel(rest),
        ["preset", rest @ ..] => preset(rest),
        ["toggle", rest @ ..] => toggle(rest),
        ["animations", rest @ ..] => animations(rest),
        ["waybar", rest @ ..] => waybar(rest),
        ["notif", rest @ ..] => notif(rest),
        ["osd", rest @ ..] => osd(rest),
        ["idle", rest @ ..] => idle(rest),
        ["lock", rest @ ..] => lock(rest),
        ["wallpaper", rest @ ..] => wallpaper(rest),
        ["battery", rest @ ..] => battery(rest),
        ["update", rest @ ..] => update(rest),
        ["target", rest @ ..] => target(rest),
        ["apps", rest @ ..] => apps(rest),
        ["monitor", rest @ ..] => monitor(rest),
        ["tweak", rest @ ..] => tweak(rest),
        ["power", rest @ ..] => power(rest),
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

fn doctor(with_deps: bool, quiet: bool) -> i32 {
    let Some(paths) = omarchy() else { return 4 };

    // --quiet: machine mode for the post-update hook — nothing on a clean
    // system, terse drift lines + exit 1 when something needs a look.
    if quiet {
        let Ok(store) = history() else { return 1 };
        let report = studio_core::hooks::update_report(&store);
        if report.clean() {
            return 0;
        }
        for f in &report.drift {
            println!("drift {}", f.display());
        }
        for f in &report.clobbers {
            println!("clobber {}", f.display());
        }
        return 1;
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

    println!("  update survival");
    for (event, state) in studio_core::hooks::status(&paths) {
        let word = match state {
            studio_core::hooks::HookState::Installed => "installed",
            studio_core::hooks::HookState::Missing => {
                "not installed (omarchy-studio hooks install)"
            }
            studio_core::hooks::HookState::Modified => {
                "MODIFIED — not our script (hooks install repairs)"
            }
        };
        println!("    {:<18} hook  {}", event, word);
    }
    if let Ok(store) = history() {
        let report = studio_core::hooks::update_report(&store);
        if report.clean() {
            println!("    tracked files      all match the last snapshot");
        } else {
            for f in &report.drift {
                println!(
                    "    drifted            {} (hand-edited since our snapshot)",
                    f.display()
                );
            }
            for f in &report.clobbers {
                println!(
                    "    clobbered          {} (an update left a newer .bak sibling)",
                    f.display()
                );
            }
            println!("    review with `omarchy-studio snapshot list`, restore with `snapshot restore <id>`");
        }
    }

    {
        use studio_core::modules::targets::{self, Overrides};
        let overrides = Overrides::load(&studio_core::studio_config_dir().join("config.toml"));
        let resolved = targets::resolve_all(&paths, &overrides);
        if !resolved.is_empty() {
            println!("  config targets");
            for (spec, r) in resolved {
                let note = if r.path.is_file() { "" } else { "  (missing)" };
                println!(
                    "    {:<14} {:<9} {}{}",
                    spec.key,
                    r.source.label(),
                    r.path.display(),
                    note
                );
            }
        }
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
        // Preview a palette extracted from an image (spec 04 §4). The
        // theme-from-wallpaper wizard (0.6.5) builds on this; here it prints
        // the colors.toml so the result is scriptable today.
        ["new", name, rest @ ..] if rest.first() == Some(&"--from-image") => {
            use studio_core::modules::extraction::{extract_image, Bias, Mode};
            use studio_core::modules::wizard::materialize;
            let Some(image) = rest.get(1) else {
                eprintln!(
                    "usage: theme new <name> --from-image <path> [--mode m] [--bias b] [--apply]"
                );
                return 2;
            };
            let flag = |k: &str| {
                rest.iter()
                    .position(|a| *a == k)
                    .and_then(|i| rest.get(i + 1))
            };
            let mode = flag("--mode")
                .and_then(|s| Mode::parse(s))
                .unwrap_or(Mode::Normal);
            let bias = flag("--bias")
                .and_then(|s| Bias::parse(s))
                .unwrap_or(Bias::Auto);
            let image = std::path::Path::new(image);
            let ex = match extract_image(image, mode, bias) {
                Ok(ex) => ex,
                Err(e) => {
                    eprintln!("extraction failed: {e:?}");
                    return 1;
                }
            };
            match materialize(&paths, name, image, &ex) {
                Ok(dir) => {
                    let slug = dir.file_name().unwrap_or_default().to_string_lossy();
                    println!(
                        "theme `{slug}` created at {} ({} · {})",
                        dir.display(),
                        mode.label(),
                        if ex.light { "light" } else { "dark" }
                    );
                    if rest.contains(&"--apply") {
                        let out = RealRunner.run(&cmds::theme_set(&slug));
                        match out {
                            Ok(o) if o.ok() => println!("applied."),
                            _ => {
                                eprintln!(
                                    "created, but omarchy-theme-set failed — apply from the TUI"
                                );
                                return 1;
                            }
                        }
                    } else {
                        println!("apply with: omarchy-studio theme apply {slug}");
                    }
                    0
                }
                Err(e) => {
                    eprintln!("could not create the theme: {e:?}");
                    1
                }
            }
        }
        ["extract", image, rest @ ..] => {
            use studio_core::modules::extraction::{extract_image, Bias, Mode};
            let mode = rest
                .first()
                .and_then(|s| Mode::parse(s))
                .unwrap_or(Mode::Normal);
            let bias = rest
                .get(1)
                .and_then(|s| Bias::parse(s))
                .unwrap_or(Bias::Auto);
            match extract_image(std::path::Path::new(image), mode, bias) {
                Ok(ex) => {
                    print!("{}", ex.to_colors_toml());
                    eprintln!(
                        "\n# {} · {} — pipe into a theme's colors.toml, or wait for `theme new --from-image`",
                        mode.label(),
                        if ex.light { "light" } else { "dark" }
                    );
                    0
                }
                Err(e) => {
                    eprintln!("extraction failed: {e:?}");
                    1
                }
            }
        }
        _ => {
            eprintln!(
                "usage: omarchy-studio theme list | current | apply <name> | fork <src> <new> \
                 | new <name> --from-image <path> [--mode normal|muted|material] [--bias auto|dark|light] [--apply] \
                 | extract <image> [normal|muted|material] [auto|dark|light]"
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
        ["list"] | ["log"] => {
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
        ["show", id] => {
            match store.changed_files(id) {
                Ok(files) => {
                    if files.is_empty() {
                        println!("{id} changed no tracked files");
                    } else {
                        println!("files changed by {id}:");
                        for f in files {
                            println!("  {}", f.display());
                        }
                    }
                }
                Err(e) => {
                    eprintln!("no such snapshot `{id}`: {}", brief(e));
                    return 1;
                }
            }
            if let Ok(diff) = store.diff(id) {
                println!();
                print!("{diff}");
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
        ["restore", id, "--files", files @ ..] if !files.is_empty() => {
            let paths: Vec<std::path::PathBuf> =
                files.iter().map(std::path::PathBuf::from).collect();
            match store.restore_files(id, &paths) {
                Ok(restored) => {
                    println!("restored {} file(s) from {id}:", restored.len());
                    for f in restored {
                        println!("  {}", f.display());
                    }
                    0
                }
                Err(e) => {
                    eprintln!("restore failed: {}", brief(e));
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
            eprintln!("usage: omarchy-studio snapshot list | log | show <id> | undo | restore <id> [--files <f>…]");
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
    // Snapshot every file Studio may touch (looknfeel.conf + input.conf) so
    // undo restores all of them.
    let files = studio_core::modules::looknfeel::LookFeel::managed_paths(paths);
    let store = history().ok();
    if let Some(s) = &store {
        let _ = s.record(
            SnapshotKind::Pre,
            &format!("before {summary}"),
            &files,
            "looknfeel",
            &[],
        );
    }
    match lf.apply(paths, &RealRunner) {
        Ok(_) => {
            if let Some(s) = &store {
                let _ = s.record(SnapshotKind::Post, summary, &files, "looknfeel", &[]);
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
            ["new", name, rest @ ..] => {
                use studio_core::modules::waybar::CustomSpec;
                let mut spec = CustomSpec {
                    name: (*name).to_string(),
                    format: "{}".into(),
                    ..Default::default()
                };
                let mut lane = "left";
                let mut it = rest.iter();
                while let Some(flag) = it.next() {
                    let Some(val) = it.next() else {
                        eprintln!("{flag} needs a value");
                        return 2;
                    };
                    match *flag {
                        "--exec" => spec.exec = (*val).to_string(),
                        "--interval" => match val.parse::<u32>() {
                            Ok(n) => spec.interval = Some(n),
                            Err(_) => {
                                eprintln!("--interval must be a number of seconds");
                                return 2;
                            }
                        },
                        "--format" => spec.format = (*val).to_string(),
                        "--on-click" => spec.on_click = Some((*val).to_string()),
                        "--lane" => lane = val,
                        other => {
                            eprintln!("unknown flag {other}");
                            return 2;
                        }
                    }
                }
                if spec.exec.trim().is_empty() {
                    eprintln!("a custom module needs --exec <command>");
                    return 2;
                }
                let Some(l) = lane_key(lane) else {
                    eprintln!("--lane must be left|center|right");
                    return 2;
                };
                let summary = format!("waybar new custom/{} → {lane}", spec.slug());
                (cfg.add_custom_module(&spec, l).map(|_| ()), summary)
            }
            _ => {
                eprintln!(
                    "usage: waybar modules | add <lane> <id> | remove <lane> <id> | move <id> <lane> | set <path> <value> | new <name> --exec <cmd> [--interval N] [--format F] [--on-click C] [--lane left|center|right]"
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
        use studio_core::modules::waybar::ApplyOutcome;
        match cfg.apply_watched(&RealRunner, std::time::Duration::from_millis(900)) {
            Ok(ApplyOutcome::Reverted) => {
                eprintln!("{summary}: that change stopped Waybar, so it was reverted.");
                1
            }
            Ok(_) => {
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
    use studio_core::modules::waybar::{resolve_style, WaybarStyle};
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
    // Snapshot the file we will actually write — the resolved target, which may
    // be a custom/relocated style.css (roadmap 0.8.2), not the default path.
    let path = resolve_style(paths).path;
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

fn notif(args: &[&str]) -> i32 {
    use studio_core::modules::mako::{
        dnd_active, lookup, send_sample, set_dnd, MakoBehavior, Rule, SAMPLE_URGENCIES, SETTINGS,
    };
    let Some(paths) = omarchy() else { return 4 };

    // Live-only verbs (no config write) first.
    match args {
        ["dnd", "status"] | ["dnd"] => {
            println!(
                "do-not-disturb is {}",
                if dnd_active(&RealRunner) { "on" } else { "off" }
            );
            return 0;
        }
        ["dnd", state @ ("on" | "off")] => {
            return match set_dnd(&RealRunner, *state == "on") {
                Ok(()) => {
                    println!("do-not-disturb {state}");
                    0
                }
                Err(e) => {
                    eprintln!("{e:?}");
                    1
                }
            };
        }
        ["test", urgency] if SAMPLE_URGENCIES.contains(urgency) => {
            return match send_sample(&RealRunner, urgency) {
                Ok(()) => 0,
                Err(e) => {
                    eprintln!("{e:?}");
                    1
                }
            };
        }
        _ => {}
    }

    let mut b = MakoBehavior::load(&paths);
    if let Some(real) = b.degraded() {
        eprintln!(
            "note: ~/.config/mako/config is a real file ({}), not the theme symlink — \
             theming won't reach mako. Restore the symlink for full Studio support.",
            real.display()
        );
    }

    let (summary, changed) = match args {
        ["list"] | [] => {
            for s in SETTINGS {
                let mark = if b.is_overridden(s.key) { "*" } else { " " };
                println!("{mark} {:<16} {:>22}   {}", s.key, b.value(s.key), s.label);
            }
            if !b.rules.is_empty() {
                println!("\nrules:");
                for (i, r) in b.rules.iter().enumerate() {
                    println!("  [{i}] {}", r.describe());
                }
            }
            println!("\n(* = changed from the Omarchy default)");
            return 0;
        }
        ["get", key] => {
            if lookup(key).is_none() {
                eprintln!("unknown setting `{key}` — see `notif list`");
                return 2;
            }
            println!("{}", b.value(key));
            return 0;
        }
        ["set", key, value] => {
            if let Err(e) = b.set(key, value) {
                eprintln!("{e}");
                return 2;
            }
            (format!("notif {key}={}", b.value(key)), true)
        }
        ["rule", "add", crit, action] => match (parse_pairs(crit), parse_pairs(action)) {
            (Some(criteria), Some(settings)) if !criteria.is_empty() && !settings.is_empty() => {
                b.add_rule(Rule { criteria, settings });
                ("notif rule added".to_string(), true)
            }
            _ => {
                eprintln!(
                    "usage: notif rule add <k=v[,k=v]> <k=v[,k=v]>  (criteria, then settings)"
                );
                return 2;
            }
        },
        ["rule", "remove", idx] => match idx.parse::<usize>() {
            Ok(i) if i < b.rules.len() => {
                b.remove_rule(i);
                (format!("notif rule {i} removed"), true)
            }
            _ => {
                eprintln!("no rule #{idx} — see `notif list`");
                return 2;
            }
        },
        _ => {
            eprintln!(
                "usage: notif list | get <key> | set <key> <value> | rule add <crit> <action> | rule remove <n> | dnd on|off|status | test low|normal|critical"
            );
            return 2;
        }
    };
    let _ = changed;

    // Snapshot the template, apply (write → theme-refresh → reload).
    let file = b.tpl_path().to_path_buf();
    let store = history().ok();
    if let Some(s) = &store {
        let _ = s.record(
            SnapshotKind::Pre,
            &format!("before {summary}"),
            std::slice::from_ref(&file),
            "mako",
            &[],
        );
    }
    match b.apply(&RealRunner) {
        Ok(()) => {
            if let Some(s) = &store {
                let _ = s.record(
                    SnapshotKind::Post,
                    &summary,
                    std::slice::from_ref(&file),
                    "mako",
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

fn osd(args: &[&str]) -> i32 {
    use studio_core::modules::swayosd::{self, Swayosd};
    let Some(paths) = omarchy() else { return 4 };

    if let ["test"] = args {
        return match swayosd::self_test(&RealRunner) {
            Ok(()) => {
                println!("flashed the OSD");
                0
            }
            Err(e) => {
                eprintln!("{e:?}");
                1
            }
        };
    }

    let mut osd = Swayosd::load(&paths);
    let summary = match args {
        ["show"] | ["list"] | [] => {
            println!("show-percentage  {}", osd.show_percentage);
            println!("max-volume       {}", osd.max_volume);
            println!(
                "top-margin       {}",
                osd.top_margin
                    .map(|m| m.to_string())
                    .unwrap_or_else(|| "default".into())
            );
            println!(
                "radius           {}",
                osd.radius
                    .map(|r| r.to_string())
                    .unwrap_or_else(|| "default".into())
            );
            println!(
                "font             {}",
                osd.font_pt
                    .map(|f| format!("{f}pt"))
                    .unwrap_or_else(|| "default".into())
            );
            return 0;
        }
        ["set", "show-percentage", v] => match parse_bool(v) {
            Some(b) => {
                osd.show_percentage = b;
                format!("osd show-percentage={b}")
            }
            None => {
                eprintln!("show-percentage is on or off");
                return 2;
            }
        },
        ["set", "max-volume", v] => match v.parse::<i64>() {
            Ok(n) if (1..=200).contains(&n) => {
                osd.max_volume = n;
                format!("osd max-volume={n}")
            }
            _ => {
                eprintln!("max-volume must be 1–200");
                return 2;
            }
        },
        ["set", "top-margin", v] => match v.parse::<f64>() {
            Ok(m) if (0.0..=1.0).contains(&m) => {
                osd.top_margin = Some(m);
                format!("osd top-margin={m}")
            }
            _ => {
                eprintln!("top-margin must be between 0.0 and 1.0");
                return 2;
            }
        },
        ["set", "radius", v] => match v.parse::<i64>() {
            Ok(n) if (0..=40).contains(&n) => {
                osd.radius = Some(n);
                format!("osd radius={n}")
            }
            _ => {
                eprintln!("radius must be 0–40");
                return 2;
            }
        },
        ["set", "font", v] => match v.parse::<i64>() {
            Ok(n) if (6..=32).contains(&n) => {
                osd.font_pt = Some(n);
                format!("osd font={n}pt")
            }
            _ => {
                eprintln!("font must be 6–32 (pt)");
                return 2;
            }
        },
        _ => {
            eprintln!(
                "usage: osd show | set show-percentage <on|off> | set max-volume <n> | set top-margin <0..1> | set radius <n> | set font <pt> | test"
            );
            return 2;
        }
    };

    let files = [
        osd.config_path().to_path_buf(),
        osd.style_path().to_path_buf(),
    ];
    let store = history().ok();
    if let Some(s) = &store {
        let _ = s.record(
            SnapshotKind::Pre,
            &format!("before {summary}"),
            &files,
            "swayosd",
            &[],
        );
    }
    match osd.apply(&RealRunner) {
        Ok(()) => {
            if let Some(s) = &store {
                let _ = s.record(SnapshotKind::Post, &summary, &files, "swayosd", &[]);
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

fn fmt_dur(secs: i64) -> String {
    if secs >= 60 && secs % 60 == 0 {
        format!("{}m", secs / 60)
    } else if secs >= 60 {
        format!("{}m{}s", secs / 60, secs % 60)
    } else {
        format!("{secs}s")
    }
}

fn idle(args: &[&str]) -> i32 {
    use studio_core::modules::lockidle::{Hypridle, Stage};
    let Some(paths) = omarchy() else { return 4 };
    let mut idle = match Hypridle::load(&paths) {
        Ok(i) => i,
        Err(e) => {
            eprintln!("can't read hypridle.conf: {e:?}");
            return 1;
        }
    };
    let stage_of = |name: &str| match name {
        "screensaver" => Some(Stage::Screensaver),
        "lock" => Some(Stage::Lock),
        "screen-off" => Some(Stage::ScreenOff),
        "suspend" => Some(Stage::Suspend),
        _ => None,
    };
    let summary = match args {
        ["timeline"] | ["list"] | [] => {
            println!("idle timeline:");
            for l in idle.timeline() {
                println!("  {:>6}  {}", fmt_dur(l.timeout), l.stage.label());
            }
            return 0;
        }
        ["set", stage, secs] => {
            let Some(want) = stage_of(stage) else {
                eprintln!("stage must be screensaver|lock|screen-off|suspend");
                return 2;
            };
            let Ok(seconds) = secs.parse::<i64>() else {
                eprintln!("seconds must be a whole number");
                return 2;
            };
            let Some(listener) = idle.timeline().into_iter().find(|l| l.stage == want) else {
                eprintln!("no `{stage}` stage in your hypridle.conf");
                return 1;
            };
            if let Err(e) = idle.set_timeout(&listener, seconds) {
                eprintln!("{e:?}");
                return 2;
            }
            format!("idle {stage} → {}", fmt_dur(seconds))
        }
        _ => {
            eprintln!("usage: idle timeline | set <screensaver|lock|screen-off|suspend> <seconds>");
            return 2;
        }
    };

    let file = idle.path().to_path_buf();
    let store = history().ok();
    if let Some(s) = &store {
        let _ = s.record(
            SnapshotKind::Pre,
            &format!("before {summary}"),
            std::slice::from_ref(&file),
            "hypridle",
            &[],
        );
    }
    match idle.apply(&RealRunner) {
        Ok(()) => {
            if let Some(s) = &store {
                let _ = s.record(
                    SnapshotKind::Post,
                    &summary,
                    std::slice::from_ref(&file),
                    "hypridle",
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

fn lock(args: &[&str]) -> i32 {
    use studio_core::modules::lockidle::Hyprlock;
    let Some(paths) = omarchy() else { return 4 };

    if let ["avatar", "list"] = args {
        let choices = Hyprlock::avatar_choices(&paths);
        if choices.is_empty() {
            println!("(no images in ~/.config/omarchy/profile_images/)");
        }
        for p in choices {
            println!("{}", p.display());
        }
        return 0;
    }
    if let ["preview"] = args {
        eprintln!("preview locks your screen — run `hyprlock` yourself when ready.");
        return 0;
    }

    let mut lock = match Hyprlock::load(&paths) {
        Ok(l) => l,
        Err(e) => {
            eprintln!("can't read hyprlock.conf: {e:?}");
            return 1;
        }
    };
    let summary = match args {
        ["show"] | ["list"] | [] => {
            let opt = |o: Option<String>| o.unwrap_or_else(|| "—".into());
            println!("avatar   {}", opt(lock.avatar_path()));
            println!("size     {}", opt(lock.avatar_size()));
            println!("blur     {}", opt(lock.blur_passes()));
            println!("dim      {}", opt(lock.dim()));
            return 0;
        }
        ["avatar", path] => {
            if !lock.set_avatar(path) {
                eprintln!("no avatar image field in hyprlock.conf");
                return 1;
            }
            format!("lock avatar → {path}")
        }
        ["size", px] => match px.parse::<i64>() {
            Ok(n) if (32..=512).contains(&n) && lock.set_avatar_size(n) => {
                format!("lock avatar size → {n}")
            }
            _ => {
                eprintln!("size must be 32–512");
                return 2;
            }
        },
        ["blur", n] => match n.parse::<i64>() {
            Ok(v) if (0..=10).contains(&v) && lock.set_blur_passes(v) => format!("lock blur → {v}"),
            _ => {
                eprintln!("blur must be 0–10");
                return 2;
            }
        },
        ["dim", v] => match v.parse::<f64>() {
            Ok(f) if (0.0..=1.0).contains(&f) && lock.set_dim(f) => {
                format!("lock dim → {f:.2}")
            }
            _ => {
                eprintln!("dim must be between 0.0 and 1.0");
                return 2;
            }
        },
        _ => {
            eprintln!(
                "usage: lock show | avatar <path> | avatar list | size <px> | blur <n> | dim <0..1> | preview"
            );
            return 2;
        }
    };

    let file = lock.path().to_path_buf();
    let store = history().ok();
    if let Some(s) = &store {
        let _ = s.record(
            SnapshotKind::Pre,
            &format!("before {summary}"),
            std::slice::from_ref(&file),
            "hyprlock",
            &[],
        );
    }
    match lock.save() {
        Ok(backup) => {
            if let Some(s) = &store {
                let _ = s.record(
                    SnapshotKind::Post,
                    &summary,
                    std::slice::from_ref(&file),
                    "hyprlock",
                    &[],
                );
            }
            if let Some(dir) = backup {
                println!("(backed up your original lock screen to {})", dir.display());
            }
            println!("{summary} · undo with `omarchy-studio snapshot undo`");
            0
        }
        Err(e) => {
            eprintln!("save failed: {e:?}");
            1
        }
    }
}

fn parse_bool(s: &str) -> Option<bool> {
    match s.to_ascii_lowercase().as_str() {
        "on" | "true" | "1" | "yes" => Some(true),
        "off" | "false" | "0" | "no" => Some(false),
        _ => None,
    }
}

/// Parse `k=v,k=v` into pairs, or None if any token lacks an `=`.
fn parse_pairs(s: &str) -> Option<Vec<(String, String)>> {
    s.split(',')
        .map(|kv| {
            kv.split_once('=')
                .map(|(k, v)| (k.trim().to_string(), v.trim().to_string()))
        })
        .collect()
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

// ── wallpapers (spec 04 §5) ─────────────────────────────────────────────────

/// `battery` / `battery limit <start> <end> [--persist]` — ThinkPad-style
/// charge thresholds straight through sysfs, no TLP. Direct writes need
/// root; denied writes print the exact sudo command instead of failing
/// silently. --persist (as root) installs the boot-time systemd oneshot.
fn battery(args: &[&str]) -> i32 {
    use studio_core::modules::battery as bat;
    let bats = bat::detect();
    if bats.is_empty() {
        eprintln!("no battery found under {}", bat::SYSFS_ROOT);
        return 1;
    }
    match args {
        [] | ["status"] => {
            for b in &bats {
                let charge = b
                    .capacity
                    .map(|c| format!("{c}%"))
                    .unwrap_or_else(|| "?".into());
                println!("{}  {charge}  {}", b.name, b.status);
                match (b.start, b.end) {
                    (Some(s), Some(e)) => {
                        println!("    charges from {s}% up to {e}%  (battery limit <start> <end>)")
                    }
                    (None, Some(e)) => println!("    charging stops at {e}%"),
                    _ => println!(
                        "    no charge-threshold interface — needs a driver that exposes \
                         charge_control_*_threshold (ThinkPads, some ASUS/LG/Huawei)"
                    ),
                }
            }
            0
        }
        ["limit", start, end, rest @ ..] => {
            let (Ok(start), Ok(end)) = (start.parse::<u8>(), end.parse::<u8>()) else {
                eprintln!("thresholds are percentages, e.g. `battery limit 75 80`");
                return 2;
            };
            let persist = rest.contains(&"--persist");
            for b in &bats {
                if let Err(e) = bat::set_thresholds(b, start, end) {
                    let msg = match e {
                        studio_core::StudioError::External { detail, .. } => detail,
                        other => format!("{other:?}"),
                    };
                    eprintln!("{}: {msg}", b.name);
                    return 1;
                }
                println!("{}: charges from {start}% up to {end}%", b.name);
                if persist {
                    let unit = bat::persist_unit(b, start, end);
                    match std::fs::write(bat::PERSIST_UNIT_PATH, unit) {
                        Ok(()) => {
                            let _ = std::process::Command::new("systemctl")
                                .args(["daemon-reload"])
                                .status();
                            let _ = std::process::Command::new("systemctl")
                                .args(["enable", "battery-charge-threshold.service"])
                                .status();
                            println!(
                                "persisted: {} (re-applied at every boot)",
                                bat::PERSIST_UNIT_PATH
                            );
                        }
                        Err(e) => {
                            eprintln!(
                                "couldn't write {}: {e} — run the whole command with sudo",
                                bat::PERSIST_UNIT_PATH
                            );
                            return 1;
                        }
                    }
                } else {
                    println!("    (resets on reboot — add --persist under sudo to keep it)");
                }
            }
            0
        }
        _ => {
            eprintln!("usage: omarchy-studio battery [status] | limit <start> <end> [--persist]");
            2
        }
    }
}

// ── update ──────────────────────────────────────────────────────────────────

fn update(args: &[&str]) -> i32 {
    use studio_core::modules::update as upd;
    let check_only = args.contains(&"--check");
    let fetch = upd::HttpFetch::new();
    let release = match upd::check(&fetch, upd::CURRENT) {
        Ok(None) => {
            println!("up to date (v{})", upd::CURRENT);
            return 0;
        }
        Ok(Some(r)) => r,
        Err(studio_core::StudioError::External { detail, .. }) => {
            eprintln!("update check failed: {detail}");
            return 1;
        }
        Err(e) => {
            eprintln!("update check failed: {e:?}");
            return 1;
        }
    };
    println!("update available: v{} → v{}", upd::CURRENT, release.version);
    if let Some(line) = release.notes.lines().find(|l| !l.trim().is_empty()) {
        println!("  {}", line.trim());
    }
    if check_only {
        println!("run `omarchy-studio update` to install it");
        return 0;
    }
    let Ok(exe) = std::env::current_exe() else {
        eprintln!("cannot resolve the running binary's path");
        return 1;
    };
    match upd::install_kind(&RealRunner, &exe) {
        upd::InstallKind::Packaged { package } => {
            println!("this binary is owned by the `{package}` package — update it with pacman/your AUR helper");
            0
        }
        upd::InstallKind::Unwritable(path) => {
            eprintln!(
                "{} isn't writable — re-run with enough permissions or reinstall",
                path.display()
            );
            1
        }
        upd::InstallKind::SelfManaged(path) => match upd::apply(&fetch, &release, &path) {
            Ok(()) => {
                println!(
                    "updated: {} is now v{} (restart omarchy-studio to run it)",
                    path.display(),
                    release.version
                );
                0
            }
            Err(studio_core::StudioError::External { detail, .. }) => {
                eprintln!("update failed: {detail}");
                1
            }
            Err(e) => {
                eprintln!("update failed: {e:?}");
                1
            }
        },
    }
}

// ── power (profiles + AC/battery auto-switch, roadmap 0.8.6) ─────────────────

fn power(args: &[&str]) -> i32 {
    use studio_core::modules::power as pw;

    if !pw::available() {
        eprintln!("power-profiles-daemon not found (install power-profiles-daemon).");
        return 4;
    }
    let Some(paths) = omarchy() else { return 4 };

    match args {
        ["profile", "list"] | ["profile"] => match pw::list(&RealRunner) {
            Ok(profiles) => {
                let persisted = pw::persisted(&paths);
                for p in profiles {
                    let mark = if p.active { "●" } else { "○" };
                    let tag = if persisted.as_deref() == Some(&p.name) {
                        "  (persisted at login)"
                    } else {
                        ""
                    };
                    println!("  {mark} {}{tag}", p.name);
                }
                0
            }
            Err(e) => {
                eprintln!("can't list profiles: {}", brief(e));
                1
            }
        },
        ["profile", "get"] => match pw::active(&RealRunner) {
            Some(name) => {
                println!("{name}");
                0
            }
            None => {
                eprintln!("couldn't read the active profile");
                1
            }
        },
        ["profile", "set", name, rest @ ..] => {
            let persist = rest.contains(&"--persist");
            let out = RealRunner.run(&pw::set_cmd(name));
            match out {
                Ok(o) if o.ok() => println!("profile → {name}"),
                Ok(o) => {
                    eprintln!("couldn't set profile: {}", o.stderr.trim());
                    return 1;
                }
                Err(e) => {
                    eprintln!("couldn't set profile: {}", brief(e));
                    return 1;
                }
            }
            if persist {
                match pw::set_persist(&paths, Some(name)) {
                    Ok(_) => println!("persisted at login (autostart.conf)"),
                    Err(e) => eprintln!("couldn't persist: {}", brief(e)),
                }
            }
            0
        }
        ["auto", "off", rest @ ..] => {
            let yes = rest.contains(&"--yes");
            println!("would run:  {}", pw::remove_rule_cmd());
            if yes {
                run_shell(&pw::remove_rule_cmd())
            } else {
                println!("\nre-run with --yes to remove the auto-switch rule.");
                0
            }
        }
        ["auto", on_ac, on_bat, rest @ ..] => {
            let yes = rest.contains(&"--yes");
            let rule = pw::udev_rule(on_ac, on_bat);
            println!("udev rule ({}):\n", studio_core::modules::power::UDEV_PATH);
            print!("{rule}");
            let cmd = pw::install_rule_cmd(&rule);
            println!("\nwould run:  {cmd}");
            if yes {
                run_shell(&cmd)
            } else {
                println!("\nthis writes a root-owned file — re-run with --yes to install it.");
                0
            }
        }
        _ => {
            eprintln!("usage: power profile list|get|set <name> [--persist] | auto <ac> <bat> [--yes] | auto off [--yes]");
            2
        }
    }
}

/// Run a shell command line (used for the previewed root writes; invoked only
/// after an explicit --yes). sudo prompts happen in the user's terminal.
fn run_shell(cmd: &str) -> i32 {
    match RealRunner.run(&studio_core::cmd::Cmd::new("sh").args(["-c", cmd])) {
        Ok(o) if o.ok() => {
            println!("done.");
            0
        }
        Ok(o) => {
            eprintln!("failed: {}", o.stderr.trim());
            1
        }
        Err(e) => {
            eprintln!("failed: {}", brief(e));
            1
        }
    }
}

// ── tweak (quick tweaks catalog, roadmap 0.8.5) ──────────────────────────────

fn tweak(args: &[&str]) -> i32 {
    use studio_core::modules::tweaks::{self, State};
    let Some(_paths) = omarchy() else { return 4 };
    let ctx = match tweaks::Ctx::discover() {
        Ok(c) => c,
        Err(e) => {
            eprintln!("can't resolve paths: {}", brief(e));
            return 1;
        }
    };

    match args {
        ["list"] | [] => {
            for t in tweaks::catalog() {
                let mark = if t.state(&ctx) == State::On {
                    "on "
                } else {
                    "off"
                };
                println!("  [{mark}] {:<24} {}", t.id(), t.detail());
            }
            0
        }
        [id, verb @ ("on" | "off")] => {
            let Some(t) = tweaks::find(id) else {
                eprintln!("unknown tweak `{id}` — see `tweak list`");
                return 2;
            };
            let on = *verb == "on";
            match t.set(&ctx, on) {
                Ok(changed) => {
                    if changed.is_empty() {
                        println!("{id} already {verb}");
                    } else {
                        println!("{id} {verb}");
                        if t.touches_hypr() {
                            let _ = RealRunner.run(&cmds::hypr_reload());
                        }
                    }
                    0
                }
                Err(e) => {
                    eprintln!("couldn't set {id}: {}", brief(e));
                    1
                }
            }
        }
        _ => {
            eprintln!("usage: tweak list | <id> on|off");
            2
        }
    }
}

// ── monitor (displays, roadmap 0.8.4) ────────────────────────────────────────

fn monitor(args: &[&str]) -> i32 {
    use studio_core::modules::monitors as mon;
    let Some(paths) = omarchy() else { return 4 };

    let live = match mon::load(&RealRunner) {
        Ok(m) => m,
        Err(e) => {
            eprintln!("can't read monitors (in a Hyprland session?): {}", brief(e));
            return 1;
        }
    };

    match args {
        ["list"] | [] => {
            for m in &live {
                let (ew, eh) = m.effective_size();
                let tags = [
                    m.focused.then_some("focused"),
                    m.is_laptop().then_some("laptop"),
                    m.disabled.then_some("disabled"),
                ]
                .into_iter()
                .flatten()
                .collect::<Vec<_>>()
                .join(", ");
                println!(
                    "{:<10} {:<22} {} @ {}x{}  scale {}  transform {}{}",
                    m.name,
                    m.label(),
                    m.mode(),
                    m.x,
                    m.y,
                    mon::fmt_scale(m.scale),
                    m.transform,
                    if tags.is_empty() {
                        String::new()
                    } else {
                        format!("  [{tags}]")
                    }
                );
                if (ew, eh) != (m.width, m.height) {
                    println!("           effective {ew}x{eh}");
                }
            }
            0
        }
        ["identify"] => {
            for (i, m) in live.iter().enumerate() {
                let _ = RealRunner.run(&mon::identify_cmd(i, &m.name));
            }
            println!("flashed each monitor's name for 2s.");
            0
        }
        ["scale", name, value, rest @ ..] => {
            let dry_run = rest.contains(&"--dry-run");
            let Ok(scale) = value.parse::<f64>() else {
                eprintln!("scale must be a number like 1, 1.25, 1.5");
                return 2;
            };
            if !live.iter().any(|m| &m.name == name) {
                eprintln!("no monitor named `{name}` — see `monitor list`");
                return 2;
            }
            let mut layout = mon::Layout::from_monitors(&live);
            for s in &mut layout.monitors {
                if &s.name == name {
                    s.scale = mon::fmt_scale(scale);
                }
            }
            monitor_write(
                &paths,
                &layout,
                &format!("monitor scale {name} {value}"),
                dry_run,
            )
        }
        ["apply", rest @ ..] => {
            let dry_run = rest.contains(&"--dry-run");
            let layout = mon::Layout::from_monitors(&live);
            monitor_write(
                &paths,
                &layout,
                "monitor apply (persist current layout)",
                dry_run,
            )
        }
        _ => {
            eprintln!(
                "usage: monitor list | identify | scale <name> <f> [--dry-run] | apply [--dry-run]"
            );
            2
        }
    }
}

/// Render the layout into monitors.conf and (unless dry-run) snapshot, write,
/// and reload Hyprland.
fn monitor_write(
    paths: &OmarchyPaths,
    layout: &studio_core::modules::monitors::Layout,
    summary: &str,
    dry_run: bool,
) -> i32 {
    use studio_core::modules::monitors as mon;
    let path = mon::conf_path(paths);
    let existing = mon::read_conf(paths);
    let updated = mon::render_conf(&existing, layout);
    if dry_run {
        println!("would write {}:\n", path.display());
        println!("{}", layout.render_body());
        return 0;
    }
    let store = history().ok();
    if let Some(s) = &store {
        let _ = s.record(
            SnapshotKind::Pre,
            &format!("before {summary}"),
            std::slice::from_ref(&path),
            "monitors",
            &[],
        );
    }
    if let Err(e) = studio_core::configfs::atomic_write(&path, &updated) {
        eprintln!("couldn't write {}: {}", path.display(), brief(e));
        return 1;
    }
    let reload = RealRunner.run(&cmds::hypr_reload());
    if let Some(s) = &store {
        let _ = s.record(
            SnapshotKind::Post,
            summary,
            std::slice::from_ref(&path),
            "monitors",
            &[],
        );
    }
    match reload {
        Ok(o) if o.ok() => {
            println!("applied · undo from Snapshots");
            0
        }
        _ => {
            println!("wrote {} (hyprctl reload unavailable)", path.display());
            0
        }
    }
}

// ── apps (apps & services manager, roadmap 0.8.3) ────────────────────────────

fn apps(args: &[&str]) -> i32 {
    use studio_core::modules::apps::{self, Inventory, Manifest, RemovalPlan};
    let Some(paths) = omarchy() else { return 4 };

    match args {
        ["list", rest @ ..] => {
            let only_installed = rest.contains(&"--installed");
            let with_all = rest.contains(&"--all");
            let inv = Inventory::probe(&RealRunner, &paths);
            println!("apps");
            for item in apps::CATALOG {
                let installed = inv.is_installed(item);
                if only_installed && !installed {
                    continue;
                }
                let mark = if installed { "●" } else { "○" };
                let svc = if item.services.is_empty() {
                    String::new()
                } else {
                    format!("  [{}]", item.services.join(", "))
                };
                println!(
                    "  {mark} {:<18} {:<9} {}{}",
                    item.id,
                    item.kind.label(),
                    item.desc,
                    svc
                );
            }
            if with_all {
                let extras = Inventory::extras(&RealRunner);
                if !extras.is_empty() {
                    println!("\nother explicitly-installed packages ({}):", extras.len());
                    for name in extras {
                        println!("  ○ {name}");
                    }
                }
            }
            0
        }
        ["remove", rest @ ..] => {
            let dry_run = rest.contains(&"--dry-run");
            let yes = rest.contains(&"--yes");
            let ids: Vec<&str> = rest
                .iter()
                .copied()
                .filter(|a| !a.starts_with("--"))
                .collect();
            if ids.is_empty() {
                eprintln!("usage: apps remove <id…> [--dry-run] [--yes]");
                return 2;
            }
            let mut items = Vec::new();
            for id in &ids {
                match apps::find(id) {
                    Some(i) => items.push(i),
                    None => {
                        eprintln!("unknown app `{id}` — see `apps list`");
                        return 2;
                    }
                }
            }
            let plan = match RemovalPlan::build(&items, &RealRunner, &paths) {
                Ok(p) => p,
                Err(e) => {
                    eprintln!("couldn't build removal plan: {}", brief(e));
                    return 1;
                }
            };
            print_plan(&plan);
            if dry_run {
                return 0;
            }
            if plan.blocker.is_some() {
                // print_plan already explained why; just signal failure.
                return 1;
            }
            if !yes {
                println!("\nre-run with --yes to run the commands above.");
                return 0;
            }
            match plan.execute(&RealRunner, false) {
                Ok(_) => {
                    let mut manifest = Manifest::load(&studio_core::studio_state_dir());
                    let now = now_unix();
                    if let Ok(id) = manifest.record(&plan, now) {
                        let _ = manifest.save();
                        println!("\nremoved. restore with `apps restore {id}`");
                    }
                    0
                }
                Err(e) => {
                    eprintln!("removal failed: {}", brief(e));
                    1
                }
            }
        }
        ["restore", id, rest @ ..] => {
            let yes = rest.contains(&"--yes");
            let manifest = Manifest::load(&studio_core::studio_state_dir());
            let Some(entry) = manifest.get(id) else {
                eprintln!(
                    "no removal logged with id `{id}` — see `apps restore` history in removed.toml"
                );
                return 2;
            };
            let cmds = Manifest::restore_commands(&entry);
            if cmds.is_empty() {
                println!("nothing to reinstall for {id} (web apps can't be auto-restored — reinstall them from the browser)");
                return 0;
            }
            for c in &cmds {
                println!("  {c}");
            }
            if !entry.webapps.is_empty() {
                println!("note: web apps ({}) must be reinstalled by hand — their URL/icon weren't recorded", entry.webapps.join(", "));
            }
            if !yes {
                println!("\nre-run with --yes to reinstall.");
                return 0;
            }
            for c in &cmds {
                let parts: Vec<&str> = c.split_whitespace().collect();
                let cmd = studio_core::cmd::Cmd::new(parts[0]).args(parts[1..].iter().copied());
                match RealRunner.run(&cmd) {
                    Ok(o) if o.ok() => {}
                    Ok(o) => {
                        eprintln!("reinstall failed: {}", o.stderr.trim());
                        return 1;
                    }
                    Err(e) => {
                        eprintln!("reinstall failed: {}", brief(e));
                        return 1;
                    }
                }
            }
            println!("restored.");
            0
        }
        ["leftovers"] => {
            let inv = Inventory::probe(&RealRunner, &paths);
            let home = std::path::PathBuf::from(std::env::var_os("HOME").unwrap_or_default());
            let mut any = false;
            for item in apps::CATALOG {
                // Show leftover dirs for apps that are NOT installed (true cruft).
                if inv.is_installed(item) {
                    continue;
                }
                for rel in item.leftovers {
                    let p = home.join(rel);
                    if p.exists() {
                        println!("  {} ({})", p.display(), item.id);
                        any = true;
                    }
                }
            }
            if !any {
                println!("no leftover config dirs from removed apps.");
            }
            0
        }
        _ => {
            eprintln!("usage: apps list [--installed] [--all] | remove <id…> [--dry-run] [--yes] | restore <id> [--yes] | leftovers");
            2
        }
    }
}

fn print_plan(plan: &studio_core::modules::apps::RemovalPlan) {
    if !plan.packages.is_empty() {
        println!("packages:  {}", plan.packages.join(" "));
    }
    if !plan.cascade.is_empty() {
        println!("also removes (orphaned deps): {}", plan.cascade.join(" "));
    }
    if !plan.services.is_empty() {
        println!("services (disable first):     {}", plan.services.join(", "));
    }
    if !plan.webapps.is_empty() {
        println!("web apps:  {}", plan.webapps.join(", "));
    }
    if !plan.leftovers.is_empty() {
        println!("leftover config dirs (kept unless you delete them):");
        for p in &plan.leftovers {
            println!("    {}", p.display());
        }
    }
    if let Some(why) = &plan.blocker {
        println!("\n⚠ pacman won't remove this yet:");
        println!("    {why}");
        println!("    resolve that dependency first, then try again.");
    } else {
        println!("\nwill run:");
        for c in plan.commands() {
            println!("  {c}");
        }
        println!("(pacman changes aren't git-snapshotted — this removal is logged for restore)");
    }
}

fn now_unix() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or_default()
}

// ── target (custom config targets, roadmap 0.8.2) ────────────────────────────

fn target(args: &[&str]) -> i32 {
    use studio_core::modules::targets::{self, Overrides};
    let Some(paths) = omarchy() else { return 4 };
    let config_toml = studio_core::studio_config_dir().join("config.toml");

    match args {
        ["list"] | [] => {
            let overrides = Overrides::load(&config_toml);
            println!("config targets");
            for (spec, resolved) in targets::resolve_all(&paths, &overrides) {
                let exists = if resolved.path.is_file() {
                    ""
                } else {
                    "  (missing)"
                };
                println!(
                    "  {:<14} {:<9} {}{}",
                    spec.key,
                    resolved.source.label(),
                    resolved.path.display(),
                    exists
                );
            }
            println!();
            println!("set an override with `target set <key> <path>` (e.g. waybar.config)");
            0
        }
        ["set", key, path] => {
            let expanded = expand_user(path);
            if let Err(e) = targets::validate(key, &expanded) {
                eprintln!("can't set {key}: {}", brief(e));
                return 1;
            }
            let mut overrides = Overrides::load(&config_toml);
            if let Err(e) = overrides.set(key, path) {
                eprintln!("{}", brief(e));
                return 2;
            }
            if let Err(e) = overrides.save() {
                eprintln!("couldn't write {}: {}", config_toml.display(), brief(e));
                return 1;
            }
            println!("{key} → {}", expanded.display());
            0
        }
        ["reset", key] => {
            let mut overrides = Overrides::load(&config_toml);
            match overrides.reset(key) {
                Ok(true) => {
                    if let Err(e) = overrides.save() {
                        eprintln!("couldn't write {}: {}", config_toml.display(), brief(e));
                        return 1;
                    }
                    println!("{key} reset to default");
                    0
                }
                Ok(false) => {
                    println!("{key} has no override");
                    0
                }
                Err(e) => {
                    eprintln!("{}", brief(e));
                    2
                }
            }
        }
        _ => {
            eprintln!("usage: target list | set <key> <path> | reset <key>");
            2
        }
    }
}

/// Render a `StudioError` as its user-language line (spec 01 §4).
fn brief(e: studio_core::StudioError) -> String {
    use studio_core::StudioError::*;
    match e {
        External { detail, .. } => detail,
        ParseFailed { hint, .. } => hint,
        VerifyFailed { step, .. } => format!("{step} failed"),
        MissingDependency(r) => format!("{} not available", r.id),
        OmarchyMismatch { action, .. } => action,
        Io(err) => err.to_string(),
    }
}

/// Expand a leading `~/` for display and validation of a user-supplied path.
fn expand_user(raw: &str) -> std::path::PathBuf {
    if let Some(rest) = raw.strip_prefix("~/") {
        if let Some(home) = std::env::var_os("HOME") {
            return std::path::PathBuf::from(home).join(rest);
        }
    }
    std::path::PathBuf::from(raw)
}

fn wallpaper(args: &[&str]) -> i32 {
    use studio_core::modules::wallpapers::Wallpapers;
    let Some(paths) = omarchy() else { return 4 };
    if let Some(("wallhaven", rest)) = args.split_first().map(|(a, r)| (*a, r)) {
        return wallhaven(&paths, rest);
    }
    let w = Wallpapers::load(&paths);
    match args {
        ["list"] => {
            if w.entries.is_empty() {
                println!("no wallpapers found for theme `{}`", w.theme);
                return 0;
            }
            for (i, e) in w.entries.iter().enumerate() {
                let mark = if w.is_current(e) { "*" } else { " " };
                println!(
                    "{mark} {:>2}  {:<36} {:<6} {}",
                    i + 1,
                    e.name(),
                    e.kind.label(),
                    e.source.label()
                );
            }
            0
        }
        ["current"] => match &w.current {
            Some(p) => {
                println!("{}", p.display());
                0
            }
            None => {
                eprintln!("no current background link");
                1
            }
        },
        ["set", which] => {
            // by 1-based index, listed name, or a path
            let path = which
                .parse::<usize>()
                .ok()
                .and_then(|n| w.entries.get(n.wrapping_sub(1)))
                .map(|e| e.path.clone())
                .or_else(|| {
                    w.entries
                        .iter()
                        .find(|e| e.name() == *which)
                        .map(|e| e.path.clone())
                })
                .unwrap_or_else(|| std::path::PathBuf::from(which));
            match Wallpapers::set(&RealRunner, &path) {
                Ok(()) => {
                    println!("wallpaper set: {}", path.display());
                    0
                }
                Err(e) => {
                    eprintln!("couldn't set wallpaper: {e:?}");
                    1
                }
            }
        }
        ["next"] => match Wallpapers::next(&RealRunner) {
            Ok(()) => {
                println!("cycled to the next background");
                0
            }
            Err(e) => {
                eprintln!("couldn't cycle: {e:?}");
                1
            }
        },
        ["add", file] => match w.add(&paths, std::path::Path::new(file)) {
            Ok(dest) => {
                println!(
                    "added {} — apply with: omarchy-studio wallpaper set {}",
                    dest.display(),
                    dest.file_name().unwrap_or_default().to_string_lossy()
                );
                0
            }
            Err(e) => {
                eprintln!("couldn't add: {e:?}");
                1
            }
        },
        ["remove", name] => {
            let Some(entry) = w.entries.iter().find(|e| e.name() == *name) else {
                eprintln!("no wallpaper named `{name}` — see `omarchy-studio wallpaper list`");
                return 1;
            };
            match Wallpapers::remove(entry) {
                Ok(()) => {
                    println!("removed {}", entry.path.display());
                    0
                }
                Err(e) => {
                    eprintln!("couldn't remove: {e:?}");
                    1
                }
            }
        }
        _ => {
            eprintln!("usage: wallpaper list | current | set <n|name|path> | next | add <file> | remove <name>");
            2
        }
    }
}

// ── update-survival hooks (spec 02 §4, §6) ──────────────────────────────────

/// `wallpaper wallhaven search <query> [--color <hex>] [--ratio <r>] [--top]
/// [--page N] [--download <n>]` — anonymous wallhaven.cc search (SFW; an API
/// key in ~/.config/omarchy-studio/config.toml unlocks more), optionally
/// pulling one result straight into backgrounds/<theme>/ (spec 04 §6).
fn wallhaven(paths: &OmarchyPaths, args: &[&str]) -> i32 {
    use studio_core::modules::wallhaven::{nearest_color, Client, Query, Sorting};
    let Some(("search", rest)) = args.split_first().map(|(a, r)| (*a, r)) else {
        eprintln!(
            "usage: omarchy-studio wallpaper wallhaven search <query> \
             [--color <hex>] [--ratio 16x9] [--top] [--page N] [--download <n>]"
        );
        return 2;
    };
    let Some(q) = rest.first().filter(|a| !a.starts_with("--")) else {
        eprintln!("what should I search for? e.g. `wallpaper wallhaven search \"jade city\"`");
        return 2;
    };
    let flag = |k: &str| {
        rest.iter()
            .position(|a| *a == k)
            .and_then(|i| rest.get(i + 1))
    };
    let mut query = Query {
        q: q.to_string(),
        ..Query::default()
    };
    if rest.contains(&"--top") {
        query.sorting = Sorting::Toplist;
    }
    if let Some(r) = flag("--ratio") {
        query.ratios = Some(r.to_string());
    }
    if let Some(c) = flag("--color") {
        match nearest_color(c) {
            Some(picker) => query.color = Some(picker.to_string()),
            None => {
                eprintln!("`{c}` is not a color — expected hex like #7aa2f7");
                return 2;
            }
        }
    }
    if let Some(p) = flag("--page").and_then(|p| p.parse().ok()) {
        query.page = p;
    }

    let mut client = Client::online();
    let page = match client.search(&query) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("{e:?}");
            return 1;
        }
    };
    if page.data.is_empty() {
        println!("no results for `{}`", query.q);
        return 0;
    }
    for (i, wp) in page.data.iter().enumerate() {
        println!("{:>2}  {}  {:<11} {}", i + 1, wp.id, wp.resolution, wp.url);
    }
    println!(
        "page {}/{} · {} total · --page N for more · --download <n> to fetch one",
        page.meta.current_page, page.meta.last_page, page.meta.total
    );

    if let Some(n) = flag("--download").and_then(|n| n.parse::<usize>().ok()) {
        let Some(wp) = page.data.get(n.saturating_sub(1)) else {
            eprintln!("--download {n} is out of range (1–{})", page.data.len());
            return 2;
        };
        let theme = paths.current_theme_name().unwrap_or_default();
        let dir = paths.config.join(format!("backgrounds/{theme}"));
        match client.download(wp, &dir) {
            Ok(dest) => {
                println!("downloaded → {}", dest.display());
                println!(
                    "set it with: omarchy-studio wallpaper set {}",
                    wp.file_name()
                );
                println!(
                    "or craft a theme: omarchy-studio theme new <name> --from-image {}",
                    dest.display()
                );
            }
            Err(e) => {
                eprintln!("{e:?}");
                return 1;
            }
        }
    }
    0
}

fn hooks_cmd(args: &[&str]) -> i32 {
    use studio_core::hooks;
    let Some(paths) = omarchy() else { return 4 };
    let state = studio_core::studio_state_dir();
    match args {
        ["install"] => match hooks::install(&paths, &state) {
            Ok(written) => {
                for p in &written {
                    println!("installed {}", p.display());
                }
                println!("Studio now survives theme changes and omarchy-update.");
                0
            }
            Err(e) => {
                eprintln!("hook install failed: {e:?}");
                1
            }
        },
        ["remove"] => match hooks::uninstall(&paths, &state) {
            Ok(removed) if removed.is_empty() => {
                println!("no Studio hooks were installed.");
                0
            }
            Ok(removed) => {
                for p in &removed {
                    println!("removed {}", p.display());
                }
                0
            }
            Err(e) => {
                eprintln!("hook removal failed: {e:?}");
                1
            }
        },
        ["status"] => {
            for (event, state) in hooks::status(&paths) {
                let word = match state {
                    hooks::HookState::Installed => "installed",
                    hooks::HookState::Missing => "missing",
                    hooks::HookState::Modified => "MODIFIED (not our script)",
                };
                println!("{event:<14} {word}");
            }
            0
        }
        _ => {
            eprintln!("usage: hooks install | remove | status");
            2
        }
    }
}

/// Plumbing verbs the installed hook scripts call. Quiet and forgiving by
/// design: they run unattended inside Omarchy's own flows.
fn hook_event(args: &[&str]) -> i32 {
    use studio_core::hooks;
    let Some(paths) = omarchy() else { return 4 };
    let Ok(store) = history() else { return 1 };
    match args.first() {
        // After a theme apply: put back any managed style block the theme
        // refresh dropped, then restart the affected components.
        Some(&"theme-set") => {
            let repaired = match hooks::reassert_blocks(&paths, &store) {
                Ok(r) => r,
                Err(_) => return 1,
            };
            for file in &repaired {
                let _ = store.record(
                    SnapshotKind::Post,
                    "hook: re-asserted managed block",
                    std::slice::from_ref(file),
                    "hooks",
                    &[],
                );
                let name = file.to_string_lossy();
                let component = if name.contains("waybar") {
                    studio_core::omarchy::Component::Waybar
                } else {
                    studio_core::omarchy::Component::Swayosd
                };
                let _ = RealRunner.run(&cmds::restart(component));
            }
            0
        }
        // After omarchy-update: check for drift/clobbers and notify the
        // desktop when something needs a look. Exit 0 either way.
        Some(&"post-update") => {
            let report = hooks::update_report(&store);
            if !report.clean() {
                let _ = RealRunner.run(&cmds::notify_send(
                    "normal",
                    "Omarchy Studio",
                    &report.summary(),
                ));
            }
            0
        }
        _ => {
            eprintln!("usage: hook theme-set | post-update  (called by installed hooks)");
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

    // Hooks first — they reference the binary that's about to go away.
    match studio_core::hooks::uninstall(&paths, &studio_core::studio_state_dir()) {
        Ok(removed) => {
            for p in removed {
                println!("removed {}", p.display());
            }
        }
        Err(e) => eprintln!("hook removal failed (continuing): {e:?}"),
    }

    if !studio_core::integration::is_installed(&paths) {
        println!("Studio was not installed in the Omarchy menu; nothing else to remove.");
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
