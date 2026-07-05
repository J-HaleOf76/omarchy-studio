//! Behavior toggles (spec 05 §2, roadmap 0.3.5).
//!
//! Thin, honest wrappers over Omarchy's own `omarchy-*-toggle` scripts — we
//! never reimplement their behavior, just present them as labelled switches and
//! flip them. Some toggles keep a persistent Hyprland flag file, so we can show
//! their on/off state; the rest are stateless actions (shown as “—”) that we
//! can still flip. All command names live in the omarchy adapter (spec 02 §6).

use crate::cmd::{Cmd, CommandRunner};
use crate::error::Result;
use crate::omarchy::cmds;

/// How a toggle's current state is known.
#[derive(Clone, Copy)]
pub enum State {
    /// A persistent Hyprland flag: `(flag-name, on_when_present)`. When the
    /// state file is present, the toggle is on iff `on_when_present`.
    HyprFlag(&'static str, bool),
    /// No reliable state readout — a flip-only action.
    Unknown,
}

/// One labelled switch.
#[derive(Clone, Copy)]
pub struct Toggle {
    pub id: &'static str,
    pub label: &'static str,
    pub help: &'static str,
    /// Builds the command that flips it.
    pub flip: fn() -> Cmd,
    pub state: State,
}

/// The curated set of behavior toggles.
pub const TOGGLES: &[Toggle] = &[
    Toggle {
        id: "gaps",
        label: "Window gaps",
        help: "Space between windows, or none",
        flip: cmds::toggle_window_gaps,
        // the flag records the *disabled* state, so gaps are on when it's absent
        state: State::HyprFlag("window-no-gaps", false),
    },
    Toggle {
        id: "square-aspect",
        label: "Square aspect ratio",
        help: "Keep a lone window square on wide screens",
        flip: cmds::toggle_square_aspect,
        state: State::HyprFlag("single-window-aspect-ratio", true),
    },
    Toggle {
        id: "nightlight",
        label: "Night light",
        help: "Warm the screen colour temperature",
        flip: cmds::toggle_nightlight,
        state: State::Unknown,
    },
    Toggle {
        id: "idle",
        label: "Idle lock",
        help: "Lock and sleep the screen when idle",
        flip: cmds::toggle_idle,
        state: State::Unknown,
    },
    Toggle {
        id: "waybar",
        label: "Top bar",
        help: "Show or hide the Waybar status bar",
        flip: cmds::toggle_waybar,
        state: State::Unknown,
    },
    Toggle {
        id: "notification-silencing",
        label: "Do not disturb",
        help: "Silence notifications",
        flip: cmds::toggle_notification_silencing,
        state: State::Unknown,
    },
];

pub fn lookup(id: &str) -> Option<&'static Toggle> {
    TOGGLES.iter().find(|t| t.id == id)
}

impl Toggle {
    /// Read the current state: `Some(true/false)` when knowable, `None` when the
    /// toggle is a stateless action or the probe can't run.
    pub fn is_on(&self, runner: &dyn CommandRunner) -> Option<bool> {
        match self.state {
            State::HyprFlag(flag, on_when_present) => {
                let out = runner.run(&cmds::hypr_toggle_enabled(flag)).ok()?;
                // exit 0 = flag file present
                Some(out.ok() == on_when_present)
            }
            State::Unknown => None,
        }
    }

    /// Flip the toggle by running Omarchy's own script.
    pub fn toggle(&self, runner: &dyn CommandRunner) -> Result<()> {
        let out = runner.run(&(self.flip)())?;
        if !out.ok() {
            return Err(crate::error::StudioError::External {
                cmd: (self.flip)().display(),
                detail: out.stderr.trim().to_string(),
            });
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cmd::StubRunner;

    #[test]
    fn registry_is_unique() {
        let mut ids: Vec<_> = TOGGLES.iter().map(|t| t.id).collect();
        let n = ids.len();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), n);
        assert!(lookup("gaps").is_some());
    }

    #[test]
    fn hypr_flag_state_reads_presence_with_inversion() {
        let gaps = lookup("gaps").unwrap();
        // flag present (exit 0) → gaps OFF, since on_when_present=false
        let on =
            StubRunner::default().with_ok("omarchy-hyprland-toggle-enabled window-no-gaps", "");
        assert_eq!(gaps.is_on(&on), Some(false));
        // flag absent (non-zero exit) → gaps ON
        let off = StubRunner::default().with_fail("omarchy-hyprland-toggle-enabled window-no-gaps");
        assert_eq!(gaps.is_on(&off), Some(true));

        let aspect = lookup("square-aspect").unwrap();
        // present → ON, since on_when_present=true
        let present = StubRunner::default().with_ok(
            "omarchy-hyprland-toggle-enabled single-window-aspect-ratio",
            "",
        );
        assert_eq!(aspect.is_on(&present), Some(true));
    }

    #[test]
    fn stateless_toggles_report_none() {
        let nl = lookup("nightlight").unwrap();
        assert_eq!(nl.is_on(&StubRunner::default()), None);
    }

    #[test]
    fn toggle_runs_the_script() {
        let waybar = lookup("waybar").unwrap();
        let runner = StubRunner::default().with_ok("omarchy-toggle-waybar", "");
        assert!(waybar.toggle(&runner).is_ok());
        assert_eq!(runner.calls(), vec!["omarchy-toggle-waybar"]);
    }
}
