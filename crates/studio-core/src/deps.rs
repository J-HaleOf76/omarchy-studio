//! Dependency & tool registry (spec 06). Data lives in `data/integrations.toml`
//! (embedded) merged with `~/.config/omarchy-studio/integrations.toml`.
//!
//! Product principle 7: a feature with a missing dependency stays VISIBLE but
//! disabled, with the reason and the exact install command — never a silent
//! failure, never a stack trace.

/// Registry id, e.g. `mpvpaper-rs`, `terminal-graphics`.
pub type DepId = String;

/// How a dependency is detected (spec 06 §2).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DepKind {
    Binary,
    Package,
    TerminalGraphics,
    Network,
    Service,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DepStatus {
    Present,
    Missing,
    /// Usable but limited; carries a user-language reason.
    Degraded(String),
}

/// Everything a frontend needs to render the guidance banner (spec 06 §3):
/// what's missing, why the feature wants it, the exact command, the fallback.
#[derive(Debug, Clone)]
pub struct DepReport {
    pub id: DepId,
    pub kind: DepKind,
    pub status: DepStatus,
    /// User-language purpose, from the registry (`reason`).
    pub reason: String,
    /// Exact install command to show/run, e.g. `yay -S mpvpaper-rs`.
    /// `None` for kinds that aren't installable (network, graphics).
    pub install: Option<String>,
    /// What the user gets instead while it's missing (`fallback`).
    pub fallback: String,
}
