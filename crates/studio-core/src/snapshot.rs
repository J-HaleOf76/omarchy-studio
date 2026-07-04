//! Git-backed snapshot store (spec 03 §6).
//!
//! Plain git repo at `~/.local/state/omarchy-studio/history/`, shelling out to
//! `git` (a required-with-fallback dependency: absent → snapshots disabled and
//! every apply requires explicit confirm, spec 06 §4). History is append-only;
//! undo/restore are themselves recorded as new commits, never resets.

/// Identifier of a snapshot (short git hash).
pub type SnapshotId = String;

/// Why a snapshot was taken — rendered in `snapshot list` and commit subjects.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SnapshotKind {
    /// State of all touched files at first run ("pre-Studio", restorable forever).
    Genesis,
    /// Before an apply.
    Pre,
    /// After a successful apply.
    Post,
    /// On-disk state differed from our last Post — a hand-edit, preserved.
    Drift,
    /// A restore operation (records what was restored and from where).
    Restore,
}
