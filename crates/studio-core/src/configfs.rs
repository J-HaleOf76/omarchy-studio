//! Config parsers/writers (spec 03). One submodule per dialect as they land:
//! `hyprlang` (0.2.1), `jsonc` (0.4.1), `managed` blocks, `atomic` writes.
//!
//! Invariant for every full parser: `to_string(parse(s)) == s` byte-identical
//! for the entire golden corpus (spec 09 §2). Unknown content is preserved,
//! never normalized, never dropped.

/// Where an effective value came from — Hyprland's source layering (spec 03 §2).
/// Studio only ever WRITES to the `User` layer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Layer {
    /// `$OMARCHY_PATH/default/hypr/*` — read-only Omarchy defaults.
    Default,
    /// `~/.config/omarchy/current/theme/hyprland.conf`.
    Theme,
    /// `~/.config/hypr/*.conf` — sourced last, wins.
    User,
    /// `~/.local/state/omarchy/toggles/hypr/*.conf`.
    Toggle,
    /// Present at runtime but not found in any sourced file (e.g. plugins).
    Runtime,
}

/// Provenance of a parsed value/bind (spec 03 §2).
#[derive(Debug, Clone)]
pub struct SourceRef {
    pub file: std::path::PathBuf,
    pub line: usize,
    pub layer: Layer,
}
