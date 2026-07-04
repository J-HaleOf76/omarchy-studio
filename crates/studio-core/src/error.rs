//! Error taxonomy (spec 01 §4).
//!
//! Frontend contract: every variant renders as *what happened → why → what to
//! do next* in plain language. `MissingDependency` is not presented as an
//! error at all — it becomes the guided-install banner (spec 06 §3).

use std::path::PathBuf;

#[derive(Debug)]
pub enum StudioError {
    /// A feature's external requirement is absent. Carries everything the
    /// frontend needs to render guidance (spec 06).
    MissingDependency(Box<crate::deps::DepReport>),
    /// A config file failed to parse; `hint` is user-language.
    ParseFailed {
        file: PathBuf,
        line: Option<usize>,
        hint: String,
    },
    /// Post-apply verification failed; states whether rollback succeeded.
    VerifyFailed {
        step: String,
        stderr: String,
        rolled_back: bool,
        snapshot_id: Option<String>,
    },
    /// Capability probe found an Omarchy/Hyprland we can't safely drive.
    OmarchyMismatch {
        expected: String,
        found: String,
        action: String,
    },
    /// External command failed (captured stderr in `detail`).
    External {
        cmd: String,
        detail: String,
    },
    Io(std::io::Error),
}

impl From<std::io::Error> for StudioError {
    fn from(e: std::io::Error) -> Self {
        StudioError::Io(e)
    }
}

pub type Result<T> = std::result::Result<T, StudioError>;
