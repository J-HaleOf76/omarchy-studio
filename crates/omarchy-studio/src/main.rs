//! omarchy-studio — TUI + CLI frontends over `studio-core`.
//!
//! Dispatch (spec 08): no args → TUI; subcommand → headless CLI.
//! This is a scaffold stub: it proves the workspace shape and gives the
//! roadmap's task 0.1.1+ a place to land. clap/ratatui arrive with v0.1.

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        None => {
            eprintln!("omarchy-studio {} — TUI not built yet (roadmap v0.1).", studio_core::VERSION);
            eprintln!("This is the project scaffold; see docs/specs/00-overview.md.");
            std::process::exit(1);
        }
        Some("--version" | "-V") => {
            println!(
                "omarchy-studio {} (tested against omarchy {})",
                studio_core::VERSION,
                studio_core::TESTED_OMARCHY
            );
        }
        Some("doctor") => match studio_core::omarchy::OmarchyPaths::discover() {
            Ok(paths) => {
                let theme = paths
                    .current_theme_name()
                    .unwrap_or_else(|_| "unknown".into());
                println!("omarchy found: {}", paths.system.display());
                println!("current theme: {theme}");
                println!("(full doctor lands in roadmap 0.1.11)");
            }
            Err(e) => {
                eprintln!("could not locate an Omarchy install: {e:?}");
                std::process::exit(4);
            }
        },
        Some(other) => {
            eprintln!("unknown command `{other}` — scaffold supports: --version, doctor");
            std::process::exit(2);
        }
    }
}
