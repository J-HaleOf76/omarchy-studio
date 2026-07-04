//! Git-backed snapshot store (spec 03 §6).
//!
//! Plain git repo (default: `~/.local/state/omarchy-studio/history/`) mirroring
//! tracked files under `files/<abs-path>`. Shells out to `git` through the
//! [`crate::cmd`] choke point (`git` absent → snapshots disabled, spec 06 §4).
//!
//! History is **append-only**: undo/restore are themselves recorded as new
//! commits, never resets. The apply pipeline (spec 01 §3) drives the protocol:
//! `record(Pre)` → write files → `record(Post)`; `undo_last()` returns to the
//! most recent Pre state.
//!
//! Restore semantics: restoring snapshot `id` writes back **every file present
//! in that snapshot's tree** (full tracked state at that point). Files first
//! tracked after `id` are not in its tree and stay untouched.

use std::path::{Path, PathBuf};

use crate::cmd::{Cmd, CommandRunner};
use crate::error::{Result, StudioError};

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
    /// On-disk state differed from our last snapshot — a hand-edit, preserved.
    Drift,
    /// A restore operation (records what was restored and from where).
    Restore,
}

impl SnapshotKind {
    fn prefix(self) -> &'static str {
        match self {
            SnapshotKind::Genesis => "genesis",
            SnapshotKind::Pre => "pre",
            SnapshotKind::Post => "post",
            SnapshotKind::Drift => "drift",
            SnapshotKind::Restore => "restore",
        }
    }

    fn from_subject(subject: &str) -> Option<Self> {
        let kind = subject.split(':').next()?;
        Some(match kind {
            "genesis" => SnapshotKind::Genesis,
            "pre" => SnapshotKind::Pre,
            "post" => SnapshotKind::Post,
            "drift" => SnapshotKind::Drift,
            "restore" => SnapshotKind::Restore,
            _ => return None,
        })
    }
}

/// One entry of `snapshot list`.
#[derive(Debug, Clone)]
pub struct SnapshotInfo {
    pub id: SnapshotId,
    pub kind: Option<SnapshotKind>,
    pub summary: String,
    /// ISO-8601 commit time.
    pub timestamp: String,
}

pub struct SnapshotStore {
    root: PathBuf,
    runner: Box<dyn CommandRunner>,
}

/// Field separator for `git log` parsing — cannot appear in subjects.
const SEP: char = '\u{1f}';

impl SnapshotStore {
    /// Open the store, initializing the git repo (with a store-local identity)
    /// on first use. Idempotent.
    pub fn open_or_init(root: PathBuf, runner: Box<dyn CommandRunner>) -> Result<Self> {
        std::fs::create_dir_all(root.join("files"))?;
        let store = Self { root, runner };
        if !store.root.join(".git").exists() {
            store.git(&["init", "-q", "-b", "main"])?;
            store.git(&["config", "user.name", "omarchy-studio"])?;
            store.git(&["config", "user.email", "omarchy-studio@localhost"])?;
            // Operational markers live beside the history but not in it —
            // LAST_PRE churn must not defeat the no-change commit dedup.
            std::fs::write(store.root.join(".gitignore"), "LAST_PRE\n")?;
        }
        Ok(store)
    }

    /// Capture the current on-disk state of `files` as a snapshot.
    ///
    /// If nothing changed relative to the last snapshot, no commit is created
    /// and the current head id is returned (a Pre before an apply that touches
    /// already-mirrored state is naturally deduplicated this way).
    pub fn record(
        &self,
        kind: SnapshotKind,
        summary: &str,
        files: &[PathBuf],
        module: &str,
        trailers: &[(String, String)],
    ) -> Result<SnapshotId> {
        for f in files {
            self.mirror(f)?;
            self.track_in_manifest(f, module)?;
        }
        self.git(&["add", "-A"])?;

        let staged = self.git_raw(&["diff", "--cached", "--quiet"])?;
        let has_head = self.head().is_ok();
        let id = if staged.status == 0 && has_head {
            self.head()? // nothing changed
        } else {
            let mut msg = format!("{}: {}", kind.prefix(), summary);
            if !trailers.is_empty() {
                msg.push_str("\n\n");
                for (k, v) in trailers {
                    msg.push_str(&format!("{k}: {v}\n"));
                }
            }
            // --allow-empty keeps the degenerate first-record case (no files)
            // from failing; every store thereafter has a valid head.
            self.git(&["commit", "-q", "--allow-empty", "-m", &msg])?;
            self.head()?
        };

        if kind == SnapshotKind::Pre {
            std::fs::write(self.root.join("LAST_PRE"), &id)?;
        }
        Ok(id)
    }

    /// Files whose on-disk content differs from the mirror (hand-edits since
    /// our last snapshot). Untracked-yet files are not drift.
    pub fn detect_drift(&self, files: &[PathBuf]) -> Vec<PathBuf> {
        files
            .iter()
            .filter(|f| {
                let mirrored = std::fs::read_to_string(self.mirror_path(f)).ok();
                let on_disk = std::fs::read_to_string(f).ok();
                matches!((&mirrored, &on_disk), (Some(m), Some(d)) if m != d)
                    || (mirrored.is_some() && on_disk.is_none())
            })
            .cloned()
            .collect()
    }

    /// Write back the full tracked state as of `id`; the restore itself is
    /// recorded as a new snapshot. Returns the restored file paths.
    pub fn restore(&self, id: &str) -> Result<Vec<PathBuf>> {
        let listing = self.git(&["ls-tree", "-r", "--name-only", id])?;
        let mut restored = Vec::new();
        for tree_path in listing.stdout.lines().filter(|l| l.starts_with("files/")) {
            let content = self.git(&["show", &format!("{id}:{tree_path}")])?.stdout;
            let real = PathBuf::from("/").join(tree_path.trim_start_matches("files/"));
            crate::configfs::atomic_write(&real, &content)?;
            restored.push(real);
        }
        self.record(
            SnapshotKind::Restore,
            &format!("restored state as of {id}"),
            &restored,
            "snapshot",
            &[("Restored-From".into(), id.to_string())],
        )?;
        Ok(restored)
    }

    /// Undo the most recent apply: restore the state captured by its Pre
    /// snapshot. One-shot — cleared after use; older points need an explicit
    /// `restore(id)`.
    pub fn undo_last(&self) -> Result<Vec<PathBuf>> {
        let marker = self.root.join("LAST_PRE");
        let id = std::fs::read_to_string(&marker).map_err(|_| StudioError::External {
            cmd: "snapshot undo".into(),
            detail: "nothing to undo — no apply recorded since the last undo".into(),
        })?;
        let restored = self.restore(id.trim())?;
        let _ = std::fs::remove_file(&marker);
        Ok(restored)
    }

    pub fn list(&self, limit: usize) -> Result<Vec<SnapshotInfo>> {
        if self.head().is_err() {
            return Ok(Vec::new());
        }
        let out = self.git(&[
            "log",
            &format!("-n{limit}"),
            &format!("--format=%h{SEP}%s{SEP}%cI"),
        ])?;
        Ok(out
            .stdout
            .lines()
            .filter_map(|l| {
                let mut parts = l.splitn(3, SEP);
                let id = parts.next()?.to_string();
                let subject = parts.next()?.to_string();
                let timestamp = parts.next()?.to_string();
                let kind = SnapshotKind::from_subject(&subject);
                let summary = subject
                    .split_once(": ")
                    .map(|(_, s)| s.to_string())
                    .unwrap_or(subject);
                Some(SnapshotInfo {
                    id,
                    kind,
                    summary,
                    timestamp,
                })
            })
            .collect())
    }

    pub fn head(&self) -> Result<SnapshotId> {
        let out = self.git(&["rev-parse", "--short", "HEAD"])?;
        Ok(out.stdout.trim().to_string())
    }

    // ── internals ────────────────────────────────────────────────────────────

    /// Copy the current on-disk state of `f` into the mirror; a missing real
    /// file removes its mirror entry (so `git add -A` records the deletion).
    fn mirror(&self, f: &Path) -> Result<()> {
        let dest = self.mirror_path(f);
        if f.is_file() {
            if let Some(parent) = dest.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::copy(f, &dest)?;
        } else if dest.exists() {
            std::fs::remove_file(&dest)?;
        }
        Ok(())
    }

    fn mirror_path(&self, f: &Path) -> PathBuf {
        let rel = f.to_string_lossy();
        self.root.join("files").join(rel.trim_start_matches('/'))
    }

    /// `manifest.toml`: which files we track and which module claimed them.
    fn track_in_manifest(&self, f: &Path, module: &str) -> Result<()> {
        let manifest = self.root.join("manifest.toml");
        let mut content = std::fs::read_to_string(&manifest)
            .unwrap_or_else(|_| "# files tracked by omarchy-studio snapshots\n[files]\n".into());
        let line = format!("\"{}\" = \"{}\"\n", f.display(), module);
        if !content.contains(&format!("\"{}\" =", f.display())) {
            content.push_str(&line);
            std::fs::write(&manifest, content)?;
        }
        Ok(())
    }

    /// git in the store, error on non-zero exit.
    fn git(&self, args: &[&str]) -> Result<crate::cmd::CmdOutput> {
        let out = self.git_raw(args)?;
        if !out.ok() {
            return Err(StudioError::External {
                cmd: format!("git {}", args.join(" ")),
                detail: if out.stderr.is_empty() {
                    format!("exit status {}", out.status)
                } else {
                    out.stderr.trim().to_string()
                },
            });
        }
        Ok(out)
    }

    /// git in the store, non-zero exit allowed (e.g. `diff --quiet` probes).
    fn git_raw(&self, args: &[&str]) -> Result<crate::cmd::CmdOutput> {
        let root = self.root.to_string_lossy().to_string();
        let mut cmd = Cmd::new("git").arg("-C").arg(root);
        for a in args {
            cmd = cmd.arg(*a);
        }
        self.runner.run(&cmd)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cmd::RealRunner;

    fn scratch(tag: &str) -> PathBuf {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .subsec_nanos();
        let dir = std::env::temp_dir().join(format!(
            "omarchy-studio-snap-{tag}-{}-{nanos}",
            std::process::id()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn store(dir: &Path) -> SnapshotStore {
        SnapshotStore::open_or_init(dir.join("history"), Box::new(RealRunner)).unwrap()
    }

    #[test]
    fn init_is_idempotent() {
        let dir = scratch("init");
        store(&dir);
        store(&dir); // second open must not fail or re-init
        assert!(dir.join("history/.git").exists());
    }

    #[test]
    fn records_and_lists_snapshots() {
        let dir = scratch("record");
        let s = store(&dir);
        let f = dir.join("config/app.conf");
        std::fs::create_dir_all(f.parent().unwrap()).unwrap();

        std::fs::write(&f, "v1\n").unwrap();
        let g = s
            .record(
                SnapshotKind::Genesis,
                "pre-Studio state",
                std::slice::from_ref(&f),
                "studio",
                &[],
            )
            .unwrap();

        std::fs::write(&f, "v2\n").unwrap();
        let p = s
            .record(
                SnapshotKind::Post,
                "blur.size 8→12",
                std::slice::from_ref(&f),
                "m4",
                &[("Setting".into(), "blur.size".into())],
            )
            .unwrap();

        assert_ne!(g, p);
        let list = s.list(10).unwrap();
        assert_eq!(list.len(), 2);
        assert_eq!(list[0].kind, Some(SnapshotKind::Post));
        assert_eq!(list[0].summary, "blur.size 8→12");
        assert_eq!(list[1].kind, Some(SnapshotKind::Genesis));
        // manifest tracks both claims
        let manifest = std::fs::read_to_string(dir.join("history/manifest.toml")).unwrap();
        assert!(manifest.contains("app.conf"));
    }

    #[test]
    fn unchanged_record_creates_no_new_commit() {
        let dir = scratch("dedup");
        let s = store(&dir);
        let f = dir.join("a.conf");
        std::fs::write(&f, "same\n").unwrap();
        let id1 = s
            .record(
                SnapshotKind::Pre,
                "first",
                std::slice::from_ref(&f),
                "m",
                &[],
            )
            .unwrap();
        let id2 = s
            .record(
                SnapshotKind::Pre,
                "second, no changes",
                std::slice::from_ref(&f),
                "m",
                &[],
            )
            .unwrap();
        assert_eq!(id1, id2);
        assert_eq!(s.list(10).unwrap().len(), 1);
    }

    #[test]
    fn detects_drift() {
        let dir = scratch("drift");
        let s = store(&dir);
        let f = dir.join("a.conf");
        std::fs::write(&f, "clean\n").unwrap();
        s.record(
            SnapshotKind::Post,
            "applied",
            std::slice::from_ref(&f),
            "m",
            &[],
        )
        .unwrap();
        assert!(s.detect_drift(std::slice::from_ref(&f)).is_empty());
        std::fs::write(&f, "hand edit\n").unwrap();
        assert_eq!(s.detect_drift(std::slice::from_ref(&f)), vec![f]);
    }

    #[test]
    fn restore_returns_old_state_and_appends_history() {
        let dir = scratch("restore");
        let s = store(&dir);
        let f = dir.join("a.conf");
        std::fs::write(&f, "v1\n").unwrap();
        let genesis = s
            .record(
                SnapshotKind::Genesis,
                "start",
                std::slice::from_ref(&f),
                "m",
                &[],
            )
            .unwrap();
        std::fs::write(&f, "v2\n").unwrap();
        s.record(
            SnapshotKind::Post,
            "change",
            std::slice::from_ref(&f),
            "m",
            &[],
        )
        .unwrap();

        let restored = s.restore(&genesis).unwrap();
        assert_eq!(restored, vec![f.clone()]);
        assert_eq!(std::fs::read_to_string(&f).unwrap(), "v1\n");
        // append-only: genesis + post + restore
        let list = s.list(10).unwrap();
        assert_eq!(list.len(), 3);
        assert_eq!(list[0].kind, Some(SnapshotKind::Restore));
    }

    #[test]
    fn undo_last_is_one_shot() {
        let dir = scratch("undo");
        let s = store(&dir);
        let f = dir.join("a.conf");
        std::fs::write(&f, "before\n").unwrap();
        // apply protocol: pre → mutate → post
        s.record(
            SnapshotKind::Pre,
            "about to apply",
            std::slice::from_ref(&f),
            "m",
            &[],
        )
        .unwrap();
        std::fs::write(&f, "after\n").unwrap();
        s.record(
            SnapshotKind::Post,
            "applied",
            std::slice::from_ref(&f),
            "m",
            &[],
        )
        .unwrap();

        s.undo_last().unwrap();
        assert_eq!(std::fs::read_to_string(&f).unwrap(), "before\n");

        let err = s.undo_last().unwrap_err();
        match err {
            StudioError::External { detail, .. } => assert!(detail.contains("nothing to undo")),
            other => panic!("wrong variant: {other:?}"),
        }
    }
}
