//! studio-core — engine for Omarchy Studio.
//!
//! Frontends (TUI/CLI, later GUI) live in the `omarchy-studio` crate; this
//! crate never prints, never draws, never prompts (spec 01 §1).
//!
//! Module map (each corresponds to a build spec in `docs/specs/`):
//! - [`cmd`]      — external command runner: timeout/capture/dry-run/stub (spec 02 §2)
//! - [`omarchy`]  — adapter: paths, command wrappers, capability probe (spec 02)
//! - [`configfs`] — dialect parsers/writers, managed blocks, atomic writes (spec 03)
//! - [`engine`]   — settings schema, apply pipeline, verification (spec 01 §2–3)
//! - [`snapshot`] — git-backed history, undo/restore (spec 03 §6)
//! - [`deps`]     — dependency & tool registry, guided-install guidance (spec 06)
//! - [`integration`] — omarchy menu install/uninstall (spec 02 §3)
//! - [`modules`]  — domain logic per PRD module (specs 04–05)

pub mod cmd;
pub mod configfs;
pub mod deps;
pub mod engine;
pub mod error;
pub mod integration;
pub mod modules;
pub mod omarchy;
pub mod snapshot;

pub use error::StudioError;

/// Studio's own version, single source of truth for `--version`.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Omarchy versions this build is tested against (spec 10 §2).
pub const TESTED_OMARCHY: &str = "3.8";

/// Studio's own state root (spec 01 §7): `$XDG_STATE_HOME/omarchy-studio`,
/// defaulting to `~/.local/state/omarchy-studio`.
pub fn studio_state_dir() -> std::path::PathBuf {
    std::env::var_os("XDG_STATE_HOME")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| {
            std::path::PathBuf::from(std::env::var_os("HOME").unwrap_or_default())
                .join(".local/state")
        })
        .join("omarchy-studio")
}
