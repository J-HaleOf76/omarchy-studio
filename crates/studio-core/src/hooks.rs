//! Update-survival hooks (spec 02 §4 + §6, roadmap 0.5.5).
//!
//! Two tiny bash scripts installed into Omarchy's user hook dirs keep Studio's
//! deltas alive across system events: `theme-set.d/50-omarchy-studio` re-asserts
//! the managed CSS blocks a theme refresh may have clobbered, and
//! `post-update.d/50-omarchy-studio` checks for drift/clobbers after
//! `omarchy-update` and raises a notification when something needs a look.
//! Scripts stay ≤10 lines — the logic lives behind `omarchy-studio hook <event>`.
//!
//! Installation replicates what `omarchy-hook-install` does (mkdir -p + 0755
//! into `~/.config/omarchy/hooks/<event>.d/`) without shelling out, so it is
//! exact, idempotent, and testable. Every installed file is recorded in the
//! [`crate::manifest`] so doctor and uninstall know precisely what Studio owns.

use std::path::{Path, PathBuf};

use crate::configfs::atomic_write;
use crate::engine::content_hash;
use crate::error::Result;
use crate::manifest::{self, Artifact};
use crate::modules::{swayosd, waybar};
use crate::omarchy::OmarchyPaths;
use crate::snapshot::SnapshotStore;

/// Our slot in each hook dir — 50 leaves room on both sides.
pub const HOOK_NAME: &str = "50-omarchy-studio";
/// The Omarchy events Studio hooks (dirs are `<event>.d`).
pub const EVENTS: [&str; 2] = ["theme-set", "post-update"];

/// Where the hook for `event` lives (installed or not).
pub fn hook_path(paths: &OmarchyPaths, event: &str) -> PathBuf {
    paths.config.join(format!("hooks/{event}.d/{HOOK_NAME}"))
}

/// The exact script installed for `event`. Logic-free by design: it defers to
/// the binary so hook behavior updates with Studio, not with reinstalls.
pub fn script_body(event: &str) -> String {
    let what = match event {
        "theme-set" => "re-assert Studio-managed style blocks after a theme change",
        _ => "check for config drift/clobbers after an omarchy-update",
    };
    format!(
        "#!/bin/bash\n\
         # Installed by omarchy-studio ('omarchy-studio hooks remove' uninstalls).\n\
         # Purpose: {what}.\n\
         command -v omarchy-studio >/dev/null 2>&1 || exit 0\n\
         omarchy-studio hook {event} \"$@\" || true\n"
    )
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HookState {
    Installed,
    Missing,
    /// A file exists in our slot but isn't our script (user- or tool-edited);
    /// install refreshes it, doctor calls it out.
    Modified,
}

/// State of each event's hook, in [`EVENTS`] order.
pub fn status(paths: &OmarchyPaths) -> Vec<(&'static str, HookState)> {
    EVENTS
        .iter()
        .map(|event| {
            let state = match std::fs::read_to_string(hook_path(paths, event)) {
                Err(_) => HookState::Missing,
                Ok(body) if body == script_body(event) => HookState::Installed,
                Ok(_) => HookState::Modified,
            };
            (*event, state)
        })
        .collect()
}

/// Are both hooks installed and unmodified?
pub fn installed(paths: &OmarchyPaths) -> bool {
    status(paths)
        .iter()
        .all(|(_, s)| *s == HookState::Installed)
}

/// Install (or refresh) both hook scripts, mode 755, recorded in the
/// manifest. Idempotent. Returns the written paths.
pub fn install(paths: &OmarchyPaths, state_dir: &Path) -> Result<Vec<PathBuf>> {
    let mut written = Vec::new();
    for event in EVENTS {
        let path = hook_path(paths, event);
        let body = script_body(event);
        atomic_write(&path, &body)?;
        let mut perms = std::fs::metadata(&path)?.permissions();
        std::os::unix::fs::PermissionsExt::set_mode(&mut perms, 0o755);
        std::fs::set_permissions(&path, perms)?;
        manifest::record(
            state_dir,
            Artifact {
                path: path.clone(),
                kind: "hook".into(),
                hash: content_hash(Some(&body)),
            },
        )?;
        written.push(path);
    }
    Ok(written)
}

/// Remove both hook scripts and their manifest entries. Files someone else
/// put in our slot are removed too — the slot name is ours. Returns what
/// was actually deleted.
pub fn uninstall(paths: &OmarchyPaths, state_dir: &Path) -> Result<Vec<PathBuf>> {
    let mut removed = Vec::new();
    for event in EVENTS {
        let path = hook_path(paths, event);
        if path.exists() {
            std::fs::remove_file(&path)?;
            removed.push(path.clone());
        }
        manifest::remove(state_dir, &path)?;
    }
    Ok(removed)
}

/// The Studio-managed style blocks a theme refresh can clobber, with the
/// component that must restart when one is repaired.
fn guarded_blocks(paths: &OmarchyPaths) -> [(PathBuf, crate::configfs::ManagedBlock); 2] {
    [
        (waybar::style_path(paths), waybar::style_block()),
        (swayosd::style_css(paths), swayosd::style_block()),
    ]
}

/// The `hook theme-set` flow: for every guarded file whose managed block
/// vanished (theme refresh rewrote the file), splice the block back from the
/// last snapshot mirror. Files we never snapshotted, or that still carry
/// their block, are left alone. Returns the repaired files.
pub fn reassert_blocks(paths: &OmarchyPaths, store: &SnapshotStore) -> Result<Vec<PathBuf>> {
    let mut repaired = Vec::new();
    for (file, block) in guarded_blocks(paths) {
        let Ok(live) = std::fs::read_to_string(&file) else {
            continue; // file gone — nothing safe to re-assert onto
        };
        if block.contains(&live) {
            continue;
        }
        let Some(mirror) = store.last_content(&file) else {
            continue; // never snapshotted — we own nothing here
        };
        let Some(body) = block.extract(&mirror) else {
            continue; // our last snapshot had no block either
        };
        atomic_write(&file, &block.upsert(&live, body))?;
        repaired.push(file);
    }
    Ok(repaired)
}

/// What `hook post-update` (and doctor) reports about update survival.
#[derive(Debug, Default)]
pub struct UpdateReport {
    /// Tracked files whose on-disk content differs from our last snapshot.
    pub drift: Vec<PathBuf>,
    /// Tracked files with a `<name>.bak.<epoch>` sibling newer than our last
    /// snapshot — the signature of an `omarchy-refresh-*` clobber (spec §6.3).
    pub clobbers: Vec<PathBuf>,
}

impl UpdateReport {
    pub fn clean(&self) -> bool {
        self.drift.is_empty() && self.clobbers.is_empty()
    }

    /// One-line summary for a desktop notification.
    pub fn summary(&self) -> String {
        let mut parts = Vec::new();
        if !self.drift.is_empty() {
            parts.push(format!("{} file(s) drifted", self.drift.len()));
        }
        if !self.clobbers.is_empty() {
            parts.push(format!(
                "{} file(s) replaced by the update",
                self.clobbers.len()
            ));
        }
        format!(
            "{} — run 'omarchy-studio doctor' to review",
            parts.join(", ")
        )
    }
}

/// Compare every snapshot-tracked file against the mirror and look for
/// refresh-backup siblings that postdate our last snapshot.
pub fn update_report(store: &SnapshotStore) -> UpdateReport {
    let tracked = store.tracked_files();
    let drift = store.detect_drift(&tracked);
    let clobbers = tracked
        .iter()
        .filter(|f| {
            let Some(ours) = store.last_snapshot_time(f) else {
                return false;
            };
            newest_bak_sibling(f).is_some_and(|t| t > ours)
        })
        .cloned()
        .collect();
    UpdateReport { drift, clobbers }
}

/// mtime of the newest `<name>.bak.*` sibling of `f`, if any.
fn newest_bak_sibling(f: &Path) -> Option<std::time::SystemTime> {
    let dir = f.parent()?;
    let prefix = format!("{}.bak.", f.file_name()?.to_string_lossy());
    std::fs::read_dir(dir)
        .ok()?
        .flatten()
        .filter(|e| e.file_name().to_string_lossy().starts_with(&prefix))
        .filter_map(|e| e.metadata().ok()?.modified().ok())
        .max()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cmd::RealRunner;
    use crate::snapshot::SnapshotKind;

    fn fake_paths(tag: &str) -> OmarchyPaths {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .subsec_nanos();
        let root = std::env::temp_dir().join(format!(
            "omarchy-studio-hooks-{tag}-{}-{nanos}",
            std::process::id()
        ));
        std::fs::create_dir_all(root.join("config/omarchy")).unwrap();
        std::fs::create_dir_all(root.join("config/waybar")).unwrap();
        std::fs::create_dir_all(root.join("state/omarchy-studio")).unwrap();
        OmarchyPaths {
            system: root.join("sys/omarchy"),
            config: root.join("config/omarchy"),
            state: root.join("state/omarchy"),
        }
    }

    fn state_dir(paths: &OmarchyPaths) -> PathBuf {
        paths
            .config
            .parent()
            .unwrap()
            .parent()
            .unwrap()
            .join("state/omarchy-studio")
    }

    #[test]
    fn install_is_idempotent_and_uninstall_is_exact() {
        let paths = fake_paths("install");
        let state = state_dir(&paths);

        assert!(!installed(&paths));
        let written = install(&paths, &state).unwrap();
        assert_eq!(written.len(), 2);
        assert!(installed(&paths));

        // executable, and recorded in the manifest
        for p in &written {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(p).unwrap().permissions().mode();
            assert_eq!(mode & 0o111, 0o111, "{} must be executable", p.display());
        }
        assert_eq!(manifest::load(&state).len(), 2);

        // reinstall doesn't duplicate or drift
        install(&paths, &state).unwrap();
        assert_eq!(manifest::load(&state).len(), 2);

        // a tampered script reads Modified, install repairs it
        std::fs::write(hook_path(&paths, "theme-set"), "#!/bin/bash\nrm -rf /\n").unwrap();
        assert!(status(&paths)
            .iter()
            .any(|(e, s)| *e == "theme-set" && *s == HookState::Modified));
        install(&paths, &state).unwrap();
        assert!(installed(&paths));

        let removed = uninstall(&paths, &state).unwrap();
        assert_eq!(removed.len(), 2);
        assert!(manifest::load(&state).is_empty());
        assert!(status(&paths).iter().all(|(_, s)| *s == HookState::Missing));
    }

    #[test]
    fn reassert_restores_a_clobbered_style_block() {
        let paths = fake_paths("reassert");
        let store_root = state_dir(&paths).join("history");
        let store = SnapshotStore::open_or_init(store_root, Box::new(RealRunner)).unwrap();

        // Studio wrote a style block, then snapshotted it (the normal apply).
        let file = waybar::style_path(&paths);
        let block = waybar::style_block();
        let styled = block.upsert("@import \"theme.css\";\n", "* { font-size: 14px; }");
        std::fs::write(&file, &styled).unwrap();
        store
            .record(
                SnapshotKind::Post,
                "waybar style",
                std::slice::from_ref(&file),
                "waybar",
                &[],
            )
            .unwrap();

        // A theme refresh rewrites the file, dropping our block.
        std::fs::write(&file, "@import \"other-theme.css\";\n").unwrap();
        let repaired = reassert_blocks(&paths, &store).unwrap();
        assert_eq!(repaired, vec![file.clone()]);
        let now = std::fs::read_to_string(&file).unwrap();
        assert!(block.contains(&now));
        assert!(now.contains("font-size: 14px"), "block body restored");
        assert!(
            now.contains("other-theme.css"),
            "the theme's own rewrite is preserved"
        );

        // second pass: nothing left to repair
        assert!(reassert_blocks(&paths, &store).unwrap().is_empty());
    }

    #[test]
    fn update_report_flags_drift_and_bak_clobbers() {
        let paths = fake_paths("report");
        let store_root = state_dir(&paths).join("history");
        let store = SnapshotStore::open_or_init(store_root, Box::new(RealRunner)).unwrap();

        let file = waybar::style_path(&paths);
        std::fs::write(&file, "a\n").unwrap();
        store
            .record(
                SnapshotKind::Post,
                "seed",
                std::slice::from_ref(&file),
                "waybar",
                &[],
            )
            .unwrap();
        assert!(update_report(&store).clean());

        // hand-edit ⇒ drift
        std::fs::write(&file, "b\n").unwrap();
        let report = update_report(&store);
        assert_eq!(report.drift, vec![file.clone()]);

        // a fresh .bak.<epoch> sibling ⇒ clobber (mirror predates it)
        std::fs::write(file.with_extension("css.bak.1700000000"), "old\n").unwrap();
        let report = update_report(&store);
        assert_eq!(report.clobbers, vec![file.clone()]);
        assert!(!report.clean());
        assert!(report.summary().contains("doctor"));
    }
}
