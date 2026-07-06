//! Registry of everything Studio installs outside its own state dir
//! (spec 02 §6.2): hook scripts today, menu blocks tomorrow. Kept in
//! `installed.toml` under the studio state dir so `doctor` can tell exactly
//! what Studio owns (and whether it changed underneath us) and `uninstall`
//! can be exact rather than best-effort.

use std::path::{Path, PathBuf};

use crate::error::Result;

const FILE: &str = "installed.toml";

/// One thing Studio installed: where it lives, what family it belongs to,
/// and the content hash written at install time (tamper/clobber detection).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Artifact {
    pub path: PathBuf,
    /// Artifact family: `"hook"`, `"menu"`, …
    pub kind: String,
    pub hash: u64,
}

fn file(state_dir: &Path) -> PathBuf {
    state_dir.join(FILE)
}

/// All recorded artifacts (empty when nothing was ever installed).
pub fn load(state_dir: &Path) -> Vec<Artifact> {
    let Ok(content) = std::fs::read_to_string(file(state_dir)) else {
        return Vec::new();
    };
    let Ok(doc) = content.parse::<toml_edit::DocumentMut>() else {
        return Vec::new();
    };
    let Some(arr) = doc.get("artifact").and_then(|a| a.as_array_of_tables()) else {
        return Vec::new();
    };
    arr.iter()
        .filter_map(|t| {
            Some(Artifact {
                path: PathBuf::from(t.get("path")?.as_str()?),
                kind: t.get("kind")?.as_str()?.to_string(),
                hash: t.get("hash")?.as_str()?.parse().ok()?,
            })
        })
        .collect()
}

fn save(state_dir: &Path, artifacts: &[Artifact]) -> Result<()> {
    let mut doc = toml_edit::DocumentMut::new();
    let mut arr = toml_edit::ArrayOfTables::new();
    for a in artifacts {
        let mut t = toml_edit::Table::new();
        t["path"] = toml_edit::value(a.path.display().to_string());
        t["kind"] = toml_edit::value(a.kind.clone());
        // toml has no u64 — store as a string, parse on load.
        t["hash"] = toml_edit::value(a.hash.to_string());
        arr.push(t);
    }
    doc.insert("artifact", toml_edit::Item::ArrayOfTables(arr));
    let body = format!("# What omarchy-studio has installed — do not edit.\n{doc}");
    crate::configfs::atomic_write(&file(state_dir), &body)
}

/// Record (or refresh) an artifact, keyed by path.
pub fn record(state_dir: &Path, artifact: Artifact) -> Result<()> {
    let mut all = load(state_dir);
    all.retain(|a| a.path != artifact.path);
    all.push(artifact);
    save(state_dir, &all)
}

/// Forget an artifact by path (no-op when absent).
pub fn remove(state_dir: &Path, path: &Path) -> Result<()> {
    let mut all = load(state_dir);
    all.retain(|a| a.path != path);
    save(state_dir, &all)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "omarchy-studio-manifest-{tag}-{}",
            std::process::id()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn record_load_remove_round_trip() {
        let dir = tmp("rt");
        let a = Artifact {
            path: PathBuf::from("/tmp/x/hook"),
            kind: "hook".into(),
            hash: 42,
        };
        record(&dir, a.clone()).unwrap();
        assert_eq!(load(&dir), vec![a.clone()]);

        // re-record same path updates in place (no duplicates)
        let b = Artifact { hash: 43, ..a };
        record(&dir, b.clone()).unwrap();
        assert_eq!(load(&dir), vec![b.clone()]);

        remove(&dir, &b.path).unwrap();
        assert!(load(&dir).is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
