//! Battery charge thresholds — ThinkPads and friends, no TLP required.
//!
//! The kernel exposes charge limits per battery under
//! `/sys/class/power_supply/BAT*/charge_control_{start,end}_threshold`
//! (thinkpad_acpi and several other vendor drivers). Studio reads and
//! writes those files directly:
//!
//! - **start** — plugged in, charging waits until the battery drops below
//!   this (ThinkPads; some vendors only expose *end*).
//! - **end**   — charging stops here. The classic longevity setting
//!   (75–80 % for a docked ThinkPad).
//!
//! Writing needs root. The frontends attempt the direct write and, when it
//! is denied, hand the user the exact `sudo` command instead of failing
//! silently (spec 06 §3). Thresholds reset on reboot; [`persist_unit`]
//! renders the standard systemd oneshot that re-asserts them at boot.
//!
//! Note: sysfs attributes must be written in place — never through the
//! atomic temp-file/rename dance used for config files.

use std::path::{Path, PathBuf};

use crate::error::Result;
use crate::StudioError;

pub const SYSFS_ROOT: &str = "/sys/class/power_supply";

/// Where [`persist_unit`] is meant to live.
pub const PERSIST_UNIT_PATH: &str = "/etc/systemd/system/battery-charge-threshold.service";

#[derive(Debug, Clone)]
pub struct Battery {
    /// `BAT0`, `BAT1`, …
    pub name: String,
    /// Charge percentage right now.
    pub capacity: Option<u8>,
    /// `Charging` / `Discharging` / `Full` / `Not charging`…
    pub status: String,
    /// `charge_control_start_threshold`, when the driver exposes it.
    pub start: Option<u8>,
    /// `charge_control_end_threshold`, when the driver exposes it.
    pub end: Option<u8>,
    dir: PathBuf,
}

impl Battery {
    /// Any threshold control at all? (Some vendors expose only *end*.)
    pub fn supports_thresholds(&self) -> bool {
        self.start_path().exists() || self.end_path().exists()
    }

    fn start_path(&self) -> PathBuf {
        self.dir.join("charge_control_start_threshold")
    }

    fn end_path(&self) -> PathBuf {
        self.dir.join("charge_control_end_threshold")
    }

    /// The exact commands to run when the direct write was denied.
    pub fn sudo_hint(&self, start: u8, end: u8) -> String {
        format!("sudo omarchy-studio battery limit {start} {end}")
    }
}

fn read_trim(path: &Path) -> Option<String> {
    std::fs::read_to_string(path)
        .ok()
        .map(|s| s.trim().to_string())
}

fn read_u8(path: &Path) -> Option<u8> {
    read_trim(path)?.parse().ok()
}

/// All batteries under `root`, sorted by name.
pub fn detect_at(root: &Path) -> Vec<Battery> {
    let Ok(read) = std::fs::read_dir(root) else {
        return Vec::new();
    };
    let mut out: Vec<Battery> = read
        .flatten()
        .filter(|e| e.file_name().to_string_lossy().starts_with("BAT"))
        .map(|e| {
            let dir = e.path();
            Battery {
                name: e.file_name().to_string_lossy().to_string(),
                capacity: read_u8(&dir.join("capacity")),
                status: read_trim(&dir.join("status")).unwrap_or_else(|| "Unknown".into()),
                start: read_u8(&dir.join("charge_control_start_threshold")),
                end: read_u8(&dir.join("charge_control_end_threshold")),
                dir,
            }
        })
        .collect();
    out.sort_by(|a, b| a.name.cmp(&b.name));
    out
}

/// The real machine.
pub fn detect() -> Vec<Battery> {
    detect_at(Path::new(SYSFS_ROOT))
}

/// Validate + write both thresholds. The kernel rejects transient
/// `start >= end` states, so the order adapts to the direction of change;
/// permission trouble comes back as one friendly error carrying the exact
/// sudo command.
pub fn set_thresholds(bat: &Battery, start: u8, end: u8) -> Result<()> {
    if start >= end || end > 100 {
        return Err(StudioError::External {
            cmd: "battery limit".into(),
            detail: format!(
                "thresholds must satisfy start < end ≤ 100 (got {start}/{end}) — \
                 e.g. `battery limit 75 80`"
            ),
        });
    }
    if !bat.supports_thresholds() {
        return Err(StudioError::External {
            cmd: "battery limit".into(),
            detail: format!(
                "{} has no charge-threshold interface — this needs a kernel driver \
                 that exposes charge_control_*_threshold (ThinkPads via thinkpad_acpi, \
                 and some ASUS/LG/Huawei laptops)",
                bat.name
            ),
        });
    }

    let write = |path: &Path, value: u8| -> std::io::Result<()> {
        // sysfs: plain in-place write, no temp-file rename
        std::fs::write(path, format!("{value}\n"))
    };
    let friendly = |e: std::io::Error| {
        if e.kind() == std::io::ErrorKind::PermissionDenied {
            StudioError::External {
                cmd: "battery limit".into(),
                detail: format!(
                    "writing thresholds needs root — run: {}",
                    bat.sudo_hint(start, end)
                ),
            }
        } else {
            StudioError::External {
                cmd: "battery limit".into(),
                detail: format!("the kernel rejected the write: {e}"),
            }
        }
    };

    // Raising the window: move `end` up first so `start` never crosses it.
    // Lowering: move `start` down first for the same reason.
    let mut writes: Vec<(PathBuf, u8)> = Vec::new();
    if bat.end_path().exists() {
        writes.push((bat.end_path(), end));
    }
    if bat.start_path().exists() {
        writes.push((bat.start_path(), start));
    }
    if end < bat.end.unwrap_or(100) {
        writes.reverse();
    }
    for (path, value) in writes {
        write(&path, value).map_err(friendly)?;
    }
    Ok(())
}

/// Systemd oneshot that re-asserts the thresholds at boot (they reset on
/// power cycle). Install: write to [`PERSIST_UNIT_PATH`], then
/// `systemctl daemon-reload && systemctl enable --now <unit>` — all root.
pub fn persist_unit(bat: &Battery, start: u8, end: u8) -> String {
    let mut echoes: Vec<String> = Vec::new();
    if bat.start_path().exists() {
        echoes.push(format!("echo {start} > {}", bat.start_path().display()));
    }
    if bat.end_path().exists() {
        echoes.push(format!("echo {end} > {}", bat.end_path().display()));
    }
    format!(
        "[Unit]\n\
         Description=Battery charge thresholds ({name} {start}-{end}%)\n\
         After=multi-user.target\n\
         \n\
         [Service]\n\
         Type=oneshot\n\
         ExecStart=/bin/sh -c '{cmd}'\n\
         \n\
         [Install]\n\
         WantedBy=multi-user.target\n",
        name = bat.name,
        cmd = echoes.join("; "),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fake_sysfs(tag: &str, with_thresholds: bool) -> PathBuf {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .subsec_nanos();
        let root = std::env::temp_dir().join(format!(
            "omarchy-studio-bat-{tag}-{}-{nanos}",
            std::process::id()
        ));
        let bat = root.join("BAT0");
        std::fs::create_dir_all(&bat).unwrap();
        std::fs::write(bat.join("capacity"), "83\n").unwrap();
        std::fs::write(bat.join("status"), "Discharging\n").unwrap();
        if with_thresholds {
            std::fs::write(bat.join("charge_control_start_threshold"), "0\n").unwrap();
            std::fs::write(bat.join("charge_control_end_threshold"), "100\n").unwrap();
        }
        // an AC adapter dir that must be ignored
        std::fs::create_dir_all(root.join("ACAD")).unwrap();
        root
    }

    #[test]
    fn detect_reads_thinkpad_interface() {
        let root = fake_sysfs("detect", true);
        let bats = detect_at(&root);
        assert_eq!(bats.len(), 1, "ACAD is not a battery");
        let b = &bats[0];
        assert_eq!(b.name, "BAT0");
        assert_eq!(b.capacity, Some(83));
        assert_eq!(b.status, "Discharging");
        assert_eq!((b.start, b.end), (Some(0), Some(100)));
        assert!(b.supports_thresholds());
    }

    #[test]
    fn set_thresholds_writes_and_validates() {
        let root = fake_sysfs("set", true);
        let b = &detect_at(&root)[0];

        set_thresholds(b, 75, 80).unwrap();
        let again = &detect_at(&root)[0];
        assert_eq!((again.start, again.end), (Some(75), Some(80)));

        // lowering below the current end still lands (order adapts)
        set_thresholds(again, 40, 50).unwrap();
        let low = &detect_at(&root)[0];
        assert_eq!((low.start, low.end), (Some(40), Some(50)));

        assert!(set_thresholds(b, 80, 75).is_err(), "start must be < end");
        assert!(set_thresholds(b, 90, 120).is_err(), "end capped at 100");
    }

    #[test]
    fn missing_interface_is_a_friendly_error() {
        let root = fake_sysfs("none", false);
        let bats = detect_at(&root);
        let b = &bats[0];
        assert!(!b.supports_thresholds());
        let err = set_thresholds(b, 75, 80).unwrap_err();
        assert!(format!("{err:?}").contains("thinkpad_acpi"));
    }

    #[test]
    fn persist_unit_reasserts_both_thresholds() {
        let root = fake_sysfs("unit", true);
        let b = &detect_at(&root)[0];
        let unit = persist_unit(b, 75, 80);
        assert!(unit.contains("Type=oneshot"));
        assert!(unit.contains("echo 75 > "));
        assert!(unit.contains("echo 80 > "));
        assert!(unit.contains("charge_control_end_threshold"));
        assert!(unit.contains("WantedBy=multi-user.target"));
    }
}
