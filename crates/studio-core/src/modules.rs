//! Domain modules, one per PRD feature module. Filled in per milestone:
//!
//! | mod | PRD | spec | milestone |
//! |---|---|---|---|
//! | `themes` | M1 | 04 §1–3, §8 | v0.1 |
//! | `keybinds` | M3 | 05 §1 | v0.2 |
//! | `looknfeel` | M4 | 05 §2–3 | v0.3 |
//! | `waybar` | M5 | 05 §4 | v0.4 |
//! | `notify` | M6 | 05 §5–6 | v0.5 |
//! | `lockidle` | M7 | 05 §7 | v0.5 |
//! | `wallpapers` | M2 | 04 §5–6 | v0.6 |
//! | `extraction` | FR1.8 | 04 §4 | v0.6 |

pub mod themes {
    //! Theme model & operations (spec 04 §1). Applying always shells to
    //! `omarchy-theme-set`; Studio never reimplements the apply pipeline.

    /// Where a theme lives (spec 04 §1).
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum ThemeOrigin {
        /// `$OMARCHY_PATH/themes/<slug>` — read-only; editing forks it.
        System,
        /// `~/.config/omarchy/themes/<slug>` with a unique slug.
        User,
        /// User dir shadowing a system slug — per-file overlay semantics.
        Overlay,
    }
}
