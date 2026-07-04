//! Config parsers/writers (spec 03). One submodule per dialect as they land:
//! `hyprlang` (0.2.1), `jsonc` (0.4.1), `managed` blocks.
//!
//! Invariant for every full parser: `to_string(parse(s)) == s` byte-identical
//! for the entire golden corpus (spec 09 §2). Unknown content is preserved,
//! never normalized, never dropped.

use std::path::Path;

/// Where an effective value came from — Hyprland's source layering (spec 03 §2).
/// Studio only ever WRITES to the `User` layer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Layer {
    /// `$OMARCHY_PATH/default/hypr/*` — read-only Omarchy defaults.
    Default,
    /// `~/.config/omarchy/current/theme/hyprland.conf`.
    Theme,
    /// `~/.config/hypr/*.conf` — sourced last, wins.
    User,
    /// `~/.local/state/omarchy/toggles/hypr/*.conf`.
    Toggle,
    /// Present at runtime but not found in any sourced file (e.g. plugins).
    Runtime,
}

/// Provenance of a parsed value/bind (spec 03 §2).
#[derive(Debug, Clone)]
pub struct SourceRef {
    pub file: std::path::PathBuf,
    pub line: usize,
    pub layer: Layer,
}

/// Atomic file write (spec 03 §5): tmp sibling + fsync + rename, preserving
/// an existing file's permissions. Parent directories are created.
///
/// Symlinks are never written *through*: if `path` is a symlink this errors —
/// callers must target the real user-owned source file (the mako-config-is-a-
/// symlink case, spec 05 §5).
pub fn atomic_write(path: &Path, content: &str) -> crate::error::Result<()> {
    use std::io::Write;

    if path.is_symlink() {
        return Err(crate::StudioError::External {
            cmd: format!("write {}", path.display()),
            detail: "refusing to write through a symlink — edit the real source file".into(),
        });
    }
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let mode = std::fs::metadata(path).ok().map(|m| {
        use std::os::unix::fs::PermissionsExt;
        m.permissions().mode()
    });

    let tmp = path.with_extension("tmp.omarchy-studio");
    {
        let mut f = std::fs::File::create(&tmp)?;
        f.write_all(content.as_bytes())?;
        f.sync_all()?;
    }
    if let Some(mode) = mode {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(mode))?;
    }
    std::fs::rename(&tmp, path)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(tag: &str) -> std::path::PathBuf {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .subsec_nanos();
        let dir = std::env::temp_dir().join(format!(
            "omarchy-studio-configfs-{tag}-{}-{nanos}",
            std::process::id()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn atomic_write_creates_and_replaces() {
        let dir = scratch("aw");
        let f = dir.join("nested/new.conf");
        atomic_write(&f, "one").unwrap();
        assert_eq!(std::fs::read_to_string(&f).unwrap(), "one");
        atomic_write(&f, "two").unwrap();
        assert_eq!(std::fs::read_to_string(&f).unwrap(), "two");
        // no tmp litter
        assert!(!f.with_extension("tmp.omarchy-studio").exists());
    }

    #[test]
    fn atomic_write_preserves_mode() {
        use std::os::unix::fs::PermissionsExt;
        let dir = scratch("mode");
        let f = dir.join("script.sh");
        std::fs::write(&f, "#!/bin/bash\n").unwrap();
        std::fs::set_permissions(&f, std::fs::Permissions::from_mode(0o755)).unwrap();
        atomic_write(&f, "#!/bin/bash\necho hi\n").unwrap();
        let mode = std::fs::metadata(&f).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o755);
    }

    #[test]
    fn atomic_write_refuses_symlinks() {
        let dir = scratch("sym");
        let real = dir.join("real.ini");
        std::fs::write(&real, "x").unwrap();
        let link = dir.join("link.ini");
        std::os::unix::fs::symlink(&real, &link).unwrap();
        let err = atomic_write(&link, "y").unwrap_err();
        match err {
            crate::StudioError::External { detail, .. } => {
                assert!(detail.contains("symlink"))
            }
            other => panic!("wrong variant: {other:?}"),
        }
        assert_eq!(std::fs::read_to_string(&real).unwrap(), "x");
    }
}
