//! Settings schema + apply pipeline (spec 01 §2–3).
//!
//! Every mutation flows: plan → snapshot(pre) → write(atomic) → reload →
//! verify → commit | rollback. Live preview deliberately bypasses this and is
//! reconciled on screen exit (save = pipeline, cancel = `hyprctl reload`).

use std::path::PathBuf;
use std::time::Duration;

/// Schema-path id, e.g. `looknfeel.blur.size` (spec 01 §2).
pub type SettingId = String;

/// How risky an apply is — drives frontend confirmation UX.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Risk {
    /// Instant + trivially reversible (palette tweak with live preview).
    Safe,
    /// Restarts a component (waybar, mako); brief visual blip.
    Restarty,
    /// Could degrade the session; arms the confirm-or-revert countdown.
    Risky,
}

/// One planned file mutation, produced by `configfs`.
#[derive(Debug)]
pub struct FileEdit {
    pub file: PathBuf,
    pub new_content: String,
    /// Hash of the content we parsed — write refuses if disk changed (spec 03 §5).
    pub parsed_hash: u64,
}

/// Ordered post-write reload actions (spec 02 §2 wrappers).
#[derive(Debug)]
pub enum ReloadStep {
    HyprReload,
    ThemeRefresh,
    Restart(crate::omarchy::Component),
    MakoReload,
}

/// Post-reload checks; any failure triggers rollback (spec 01 §3).
#[derive(Debug)]
pub enum Verification {
    /// `hyprctl configerrors` must be empty. [gate: has_configerrors]
    HyprctlConfigErrorsEmpty,
    /// Process must still be alive after the grace period (waybar watchdog).
    ProcessAlive { name: &'static str, grace: Duration },
    /// Arbitrary command must exit 0 (e.g. `makoctl reload`).
    CommandOk { cmd: String },
}

#[derive(Debug)]
pub struct ApplyPlan {
    pub summary: String,
    pub edits: Vec<FileEdit>,
    pub reload: Vec<ReloadStep>,
    pub verify: Vec<Verification>,
    pub risk: Risk,
}
