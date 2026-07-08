# Handoff: Entering v0.9 Milestone

This document tracks the work completed during the recent session and outlines exactly where the next agent should pick up.

## 1. What was completed in the current session

### TUI Polish & Navigation
- **Mouse Support**: Implemented `EnableMouseCapture` and handled `on_mouse` events in `App`, allowing users to click on the left rail to instantly switch screens.
- **Keyboard Navigation**: Shifted the global keybar hint from `1-0` to `^j`/`^k` to reflect a more fluid module scrolling paradigm.
- **Rail State Tracking**: Implemented `ListState` for the left rail so that it scrolls correctly and tracks focus.
- **Aesthetics**: Removed the left border from the main panel so it visually merges with the rail's right border, and removed the numbers from the rail labels.

### Repository & Tooling Improvements
- **Installer (`install.sh`)**: Created a robust, one-shot install script that tries to fetch the latest pre-built binary and falls back to `cargo install --git` if no binaries are available.
- **GitHub Actions**: Added a `workflow_dispatch` trigger to `release.yml` so releases can be run manually.
- **Cleanup**: Removed the `.claude/` directory from git tracking to prevent cluttering the repo with agent-specific files.
- **README Updates**: Added a ToC, ASCII branding, and updated the Quick Start shortcut table to reflect the new `Ctrl+J/K` and mouse navigation.
- **Clippy Fixes**: Fixed `clippy::collapsible_if` warnings blocking the CI in `crates/omarchy-studio/src/tui/mod.rs` (`on_mouse`).

### 🚨 Temporary Fixes Applied
- **Updates (`omarchy-studio update`)**: Because GitHub Actions currently fails to attach a release binary to tags, `studio-core::modules::update::apply()` has been temporarily hacked to run `curl -sL https://raw.githubusercontent.com/arino08/omarchy-studio/main/install.sh | sh`. This is also logged at the top of `ROADMAP.md`. 
  - *Action for future agent*: Revert this hack once the GitHub release workflow is correctly building and attaching the `omarchy-studio-linux-x86_64` binary to a release tag.

## 2. Deferred / Postponed Work
- The user specifically requested to **pause** work on the `fzf`-style fuzzy finder (for themes/wallpapers) and transition animations for now. Do not prioritize these until explicitly asked.

## 3. Where to pick up (Next Steps)
We are now officially ready to tackle the **v0.9** roadmap milestone. 

1. **Review `ROADMAP.md`**: Look at the v0.9 section. 
2. **Next Feature - Snapshot Timeline (0.9.4)**: The next major item to build is the Snapshot timeline screen, which currently just shows an "arriving in..." placeholder in the TUI.
3. **Double-check CI**: Ensure the GitHub Actions builds are staying green across platforms as you build.
