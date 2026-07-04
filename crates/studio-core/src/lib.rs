//! studio-core — engine for Omarchy Studio.
//!
//! Frontends (TUI/CLI, later GUI) live in the `omarchy-studio` crate; this
//! crate never prints, never draws, never prompts (spec 01 §1).
//!
//! Module map (each corresponds to a build spec in `docs/specs/`):
//! - [`omarchy`]  — adapter: paths, command wrappers, capability probe (spec 02)
//! - [`configfs`] — dialect parsers/writers, managed blocks, atomic writes (spec 03)
//! - [`engine`]   — settings schema, apply pipeline, verification (spec 01 §2–3)
//! - [`snapshot`] — git-backed history, undo/restore (spec 03 §6)
//! - [`deps`]     — dependency & tool registry, guided-install guidance (spec 06)
//! - [`modules`]  — domain logic per PRD module (specs 04–05)

pub mod configfs;
pub mod deps;
pub mod engine;
pub mod error;
pub mod modules;
pub mod omarchy;
pub mod snapshot;

pub use error::StudioError;

/// Studio's own version, single source of truth for `--version`.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Omarchy versions this build is tested against (spec 10 §2).
pub const TESTED_OMARCHY: &str = "3.8";
