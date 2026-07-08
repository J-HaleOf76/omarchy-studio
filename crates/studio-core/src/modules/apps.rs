//! Apps & services manager (roadmap 0.8.3, absorbs old 0.7.3).
//!
//! a-la-carchy's headline is a one-shot debloater: it runs `pacman -Rns
//! --noconfirm` behind an "apply all" that bypasses every prompt, never
//! disables the services a removed package left running, and keeps no record
//! of what it took out. Studio does the same job the safe way:
//!
//! * **Cascade preview** — `pacman -Rs --print` shows *everything* a removal
//!   would drag out (now-orphaned dependencies included) before a single
//!   package leaves, and surfaces pacman's refusal when an outside package
//!   still depends on the selection instead of failing mid-transaction.
//! * **Service cleanup** — a package with systemd units (Docker → `docker.
//!   service`/`.socket`) gets those disabled first, but only if they're enabled.
//! * **A restore manifest** — every removal is logged to
//!   `state_dir/removed.toml`, so `apps restore <id>` can reinstall it.
//! * **No bulk-confirm bypass, ever** — the frontend always shows the exact
//!   command list and requires an explicit yes.
//!
//! System-package changes are not git-snapshotted (the snapshot store tracks
//! files, not pacman's database); the manifest is their record, and the confirm
//! dialog says so. Web apps are Omarchy desktop launchers, removed through the
//! adapter-wrapped `omarchy-webapp-remove`.

use std::collections::HashSet;
use std::path::PathBuf;

use crate::cmd::{Cmd, CommandRunner};
use crate::error::{Result, StudioError};
use crate::omarchy::{cmds, OmarchyPaths};

/// What a removable item actually is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// A pacman package (or package group removed together).
    Package,
    /// An Omarchy web-app launcher (`.desktop` in the applications dir).
    WebApp,
}

impl Kind {
    pub fn label(self) -> &'static str {
        match self {
            Kind::Package => "package",
            Kind::WebApp => "web app",
        }
    }
}

/// A curated, removable app. Services attach to packages (there are no
/// standalone service items in the catalog today); leftover dirs are
/// HOME-relative config/data paths pacman leaves behind.
pub struct AppItem {
    /// Stable id: the primary package name, or the web-app `.desktop` basename.
    pub id: &'static str,
    pub label: &'static str,
    pub kind: Kind,
    pub desc: &'static str,
    /// Packages to remove together (empty for web apps). First is the probe key.
    pub packages: &'static [&'static str],
    /// systemd units to disable before removal, if enabled.
    pub services: &'static [&'static str],
    /// HOME-relative config/data dirs to offer deleting after removal.
    pub leftovers: &'static [&'static str],
}

macro_rules! pkg {
    ($id:literal, $label:literal, $desc:literal, [$($p:literal),*], [$($s:literal),*], [$($l:literal),*]) => {
        AppItem { id: $id, label: $label, kind: Kind::Package, desc: $desc,
                  packages: &[$($p),*], services: &[$($s),*], leftovers: &[$($l),*] }
    };
}
macro_rules! web {
    ($id:literal, $desc:literal) => {
        AppItem {
            id: $id,
            label: $id,
            kind: Kind::WebApp,
            desc: $desc,
            packages: &[],
            services: &[],
            leftovers: &[],
        }
    };
}

/// The curated catalog (seeded from a-la-carchy's verified lists). Presence is
/// always confirmed against the live system — these are hints, never a gate.
pub const CATALOG: &[AppItem] = &[
    // packages ───────────────────────────────────────────────────────────────
    pkg!(
        "1password-beta",
        "1Password",
        "Password manager",
        ["1password-beta"],
        [],
        [".config/1Password"]
    ),
    pkg!(
        "1password-cli",
        "1Password CLI",
        "1Password command line",
        ["1password-cli"],
        [],
        []
    ),
    pkg!(
        "docker",
        "Docker",
        "Container runtime",
        ["docker", "docker-buildx", "docker-compose"],
        ["docker.service", "docker.socket"],
        [".docker"]
    ),
    pkg!(
        "gnome-calculator",
        "Calculator",
        "GNOME calculator",
        ["gnome-calculator"],
        [],
        []
    ),
    pkg!(
        "kdenlive",
        "Kdenlive",
        "Video editor",
        ["kdenlive"],
        [],
        [".local/share/kdenlive"]
    ),
    pkg!(
        "libreoffice-fresh",
        "LibreOffice",
        "Office suite",
        ["libreoffice-fresh"],
        [],
        [".config/libreoffice"]
    ),
    pkg!(
        "localsend",
        "LocalSend",
        "Local file sharing",
        ["localsend"],
        [],
        [".local/share/org.localsend.localsend_app"]
    ),
    pkg!(
        "obs-studio",
        "OBS Studio",
        "Screen recording / streaming",
        ["obs-studio"],
        [],
        [".config/obs-studio"]
    ),
    pkg!(
        "obsidian",
        "Obsidian",
        "Notes",
        ["obsidian"],
        [],
        [".config/obsidian"]
    ),
    pkg!(
        "omarchy-chromium",
        "Chromium",
        "Omarchy's Chromium browser",
        ["omarchy-chromium"],
        [],
        [".config/chromium"]
    ),
    pkg!(
        "pinta",
        "Pinta",
        "Image editor",
        ["pinta"],
        [],
        [".config/Pinta"]
    ),
    pkg!(
        "signal-desktop",
        "Signal",
        "Messaging",
        ["signal-desktop"],
        [],
        [".config/Signal"]
    ),
    pkg!(
        "spotify",
        "Spotify",
        "Music",
        ["spotify"],
        [],
        [".config/spotify"]
    ),
    pkg!(
        "typora",
        "Typora",
        "Markdown editor",
        ["typora"],
        [],
        [".config/Typora"]
    ),
    pkg!(
        "xournalpp",
        "Xournal++",
        "Handwritten notes / PDF",
        ["xournalpp"],
        [],
        [".config/xournalpp"]
    ),
    // web apps ─────────────────────────────────────────────────────────────────
    web!("Basecamp", "Basecamp web app"),
    web!("ChatGPT", "ChatGPT web app"),
    web!("Discord", "Discord web app"),
    web!("Figma", "Figma web app"),
    web!("Fizzy", "Fizzy web app"),
    web!("GitHub", "GitHub web app"),
    web!("Google Contacts", "Google Contacts web app"),
    web!("Google Maps", "Google Maps web app"),
    web!("Google Messages", "Google Messages web app"),
    web!("Google Photos", "Google Photos web app"),
    web!("HEY", "HEY web app"),
    web!("WhatsApp", "WhatsApp web app"),
    web!("X", "X (Twitter) web app"),
    web!("YouTube", "YouTube web app"),
    web!("Zoom", "Zoom web app"),
];

/// Look a catalog item up by id.
pub fn find(id: &str) -> Option<&'static AppItem> {
    CATALOG.iter().find(|i| i.id == id)
}

// ------------------------------------------------------------------ inventory

/// Which catalog items are actually present on this system.
pub struct Inventory {
    installed_pkgs: HashSet<String>,
    installed_webapps: HashSet<String>,
}

impl Inventory {
    /// Probe the system: one `pacman -Q` over every catalog package, plus a
    /// scan of the applications dir for web-app launchers.
    pub fn probe(runner: &dyn CommandRunner, paths: &OmarchyPaths) -> Self {
        let all_pkgs: Vec<&str> = CATALOG
            .iter()
            .filter(|i| i.kind == Kind::Package)
            .flat_map(|i| i.packages.iter().copied())
            .collect();
        let installed_pkgs = query_installed(runner, &all_pkgs);
        let installed_webapps = scan_webapps(&paths.applications());
        Self {
            installed_pkgs,
            installed_webapps,
        }
    }

    pub fn is_installed(&self, item: &AppItem) -> bool {
        match item.kind {
            // Installed if the primary package is present.
            Kind::Package => item
                .packages
                .first()
                .is_some_and(|p| self.installed_pkgs.contains(*p)),
            Kind::WebApp => self.installed_webapps.contains(item.id),
        }
    }

    /// Installed, explicitly-installed packages *not* in the catalog — the
    /// filterable "everything else" list (`pacman -Qeq`).
    pub fn extras(runner: &dyn CommandRunner) -> Vec<String> {
        let known: HashSet<&str> = CATALOG
            .iter()
            .flat_map(|i| i.packages.iter().copied())
            .collect();
        let Ok(out) = runner.run(&Cmd::new("pacman").arg("-Qeq")) else {
            return Vec::new();
        };
        out.stdout
            .lines()
            .map(str::trim)
            .filter(|l| !l.is_empty() && !known.contains(l))
            .map(str::to_string)
            .collect()
    }
}

/// Which catalog web apps have a `.desktop` launcher in `dir`.
fn scan_webapps(dir: &std::path::Path) -> HashSet<String> {
    CATALOG
        .iter()
        .filter(|i| i.kind == Kind::WebApp)
        .filter(|i| dir.join(format!("{}.desktop", i.id)).is_file())
        .map(|i| i.id.to_string())
        .collect()
}

/// Parse `pacman -Q <pkgs…>` output into the set of installed package names.
/// Absent packages go to stderr (and set a non-zero exit) — we read stdout, so
/// a partial match still resolves the installed ones.
fn query_installed(runner: &dyn CommandRunner, pkgs: &[&str]) -> HashSet<String> {
    if pkgs.is_empty() {
        return HashSet::new();
    }
    let cmd = Cmd::new("pacman").arg("-Q").args(pkgs.iter().copied());
    let Ok(out) = runner.run(&cmd) else {
        return HashSet::new();
    };
    parse_pacman_q(&out.stdout)
}

/// `pacman -Q` prints `name version` per installed package, one per line.
fn parse_pacman_q(stdout: &str) -> HashSet<String> {
    stdout
        .lines()
        .filter_map(|l| l.split_whitespace().next())
        .filter(|n| !n.is_empty())
        .map(str::to_string)
        .collect()
}

// ------------------------------------------------------------------ plan

/// The full effect of removing a set of items, computed before anything runs.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct RemovalPlan {
    /// Ids of the catalog items being removed.
    pub ids: Vec<String>,
    /// Packages explicitly requested for removal.
    pub packages: Vec<String>,
    /// Extra packages pacman would also remove (reverse-deps) — the cascade.
    pub cascade: Vec<String>,
    /// Enabled systemd units that will be disabled first.
    pub services: Vec<String>,
    /// Web-app names to remove.
    pub webapps: Vec<String>,
    /// Config/data dirs that exist on disk and would be left behind.
    pub leftovers: Vec<PathBuf>,
    /// Set when pacman refuses the removal (e.g. a package outside the selection
    /// depends on one inside it). Removal is blocked until it's resolved — we
    /// never force it. Carries pacman's own explanation.
    pub blocker: Option<String>,
}

impl RemovalPlan {
    /// Build the plan for `items`, querying pacman for the cascade and systemd
    /// for enabled units. Read-only — nothing is removed here.
    pub fn build(
        items: &[&AppItem],
        runner: &dyn CommandRunner,
        paths: &OmarchyPaths,
    ) -> Result<Self> {
        let ids = items.iter().map(|i| i.id.to_string()).collect();
        let packages: Vec<String> = items
            .iter()
            .filter(|i| i.kind == Kind::Package)
            .flat_map(|i| i.packages.iter().map(|p| p.to_string()))
            .collect();
        let webapps: Vec<String> = items
            .iter()
            .filter(|i| i.kind == Kind::WebApp)
            .map(|i| i.id.to_string())
            .collect();

        let (cascade, blocker) = if packages.is_empty() {
            (Vec::new(), None)
        } else {
            cascade_for(&packages, runner)
        };

        let mut services = Vec::new();
        for item in items {
            for unit in item.services {
                if unit_enabled(unit, runner) && !services.iter().any(|s| s == unit) {
                    services.push(unit.to_string());
                }
            }
        }

        let home = PathBuf::from(std::env::var_os("HOME").unwrap_or_default());
        let mut leftovers = Vec::new();
        for item in items {
            for rel in item.leftovers {
                let p = home.join(rel);
                if p.exists() {
                    leftovers.push(p);
                }
            }
        }
        let _ = paths;

        Ok(Self {
            ids,
            packages,
            cascade,
            services,
            webapps,
            leftovers,
            blocker,
        })
    }

    /// Is there anything to do?
    pub fn is_empty(&self) -> bool {
        self.packages.is_empty() && self.webapps.is_empty()
    }

    /// Can this plan be executed? False when pacman flagged a dependency block.
    pub fn can_execute(&self) -> bool {
        !self.is_empty() && self.blocker.is_none()
    }

    /// The exact commands the removal will run, in order — shown in the confirm
    /// dialog. Nothing is hidden; there is no bulk-confirm bypass.
    pub fn commands(&self) -> Vec<String> {
        let mut cmds = Vec::new();
        for unit in &self.services {
            cmds.push(format!("sudo systemctl disable --now {unit}"));
        }
        if !self.packages.is_empty() {
            cmds.push(format!(
                "sudo pacman -Rns --noconfirm {}",
                self.packages.join(" ")
            ));
        }
        if !self.webapps.is_empty() {
            cmds.push(format!("omarchy-webapp-remove {}", self.webapps.join(" ")));
        }
        cmds
    }

    /// Execute the plan. `dry_run` returns the command list without running
    /// anything. Real runs shell out through `runner` (sudo prompts happen in
    /// the user's terminal). Leftover dirs are *not* touched here — deletion is
    /// a separate, explicit opt-in ([`delete_leftovers`]).
    pub fn execute(&self, runner: &dyn CommandRunner, dry_run: bool) -> Result<Vec<String>> {
        let cmds = self.commands();
        if dry_run {
            return Ok(cmds);
        }
        if let Some(why) = &self.blocker {
            return Err(StudioError::External {
                cmd: "pacman -Rns".into(),
                detail: why.clone(),
            });
        }
        for unit in &self.services {
            run_checked(
                runner,
                Cmd::new("sudo").args(["systemctl", "disable", "--now", unit]),
            )?;
        }
        if !self.packages.is_empty() {
            let cmd = Cmd::new("sudo")
                .args(["pacman", "-Rns", "--noconfirm"])
                .args(self.packages.iter().cloned());
            run_checked(runner, cmd)?;
        }
        if !self.webapps.is_empty() {
            run_checked(runner, cmds::webapp_remove(&self.webapps))?;
        }
        Ok(cmds)
    }
}

/// Delete leftover config/data dirs (opt-in; default is to keep them).
pub fn delete_leftovers(dirs: &[PathBuf]) -> Result<()> {
    for d in dirs {
        if d.is_dir() {
            std::fs::remove_dir_all(d)?;
        } else if d.exists() {
            std::fs::remove_file(d)?;
        }
    }
    Ok(())
}

/// `pacman -Rs --print` over `packages`: the full set that would be removed,
/// minus the packages we explicitly asked for = the cascade of now-orphaned
/// dependencies. `-n` (nosave) is dropped here because pacman rejects it
/// alongside `--print`; it only affects `.pacsave` backups, not the set.
///
/// Returns `(cascade, blocker)`: if pacman refuses (a package outside the
/// selection depends on one inside it) the second value carries its
/// explanation and the cascade is empty — the removal is not safe to run.
fn cascade_for(packages: &[String], runner: &dyn CommandRunner) -> (Vec<String>, Option<String>) {
    let cmd = Cmd::new("pacman")
        .args(["-Rs", "--print", "--print-format", "%n"])
        .args(packages.iter().cloned());
    let out = match runner.run(&cmd) {
        Ok(o) => o,
        Err(e) => return (Vec::new(), Some(brief_err(e))),
    };
    if !out.ok() {
        return (Vec::new(), Some(out.stderr.trim().to_string()));
    }
    let requested: HashSet<&str> = packages.iter().map(String::as_str).collect();
    let cascade = out
        .stdout
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !requested.contains(l))
        .map(str::to_string)
        .collect();
    (cascade, None)
}

fn brief_err(e: StudioError) -> String {
    match e {
        StudioError::External { detail, .. } => detail,
        other => format!("{other:?}"),
    }
}

/// Is a systemd unit enabled? `systemctl is-enabled` prints the state and exits
/// 0 only when enabled.
fn unit_enabled(unit: &str, runner: &dyn CommandRunner) -> bool {
    let Ok(out) = runner.run(&Cmd::new("systemctl").args(["is-enabled", unit])) else {
        return false;
    };
    matches!(out.stdout.trim(), "enabled" | "enabled-runtime")
}

fn run_checked(runner: &dyn CommandRunner, cmd: Cmd) -> Result<()> {
    let out = runner.run(&cmd)?;
    if out.ok() {
        Ok(())
    } else {
        Err(StudioError::External {
            cmd: cmd.display(),
            detail: out.stderr.trim().to_string(),
        })
    }
}

// ------------------------------------------------------------------ manifest

/// The removal log: every executed removal is appended so it can be undone.
/// Lives at `state_dir/removed.toml` as an array of `[[removed]]` tables.
pub struct Manifest {
    path: PathBuf,
    doc: toml_edit::DocumentMut,
}

/// One logged removal.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Removed {
    pub id: String,
    pub when: u64,
    pub packages: Vec<String>,
    pub webapps: Vec<String>,
}

impl Manifest {
    const FILE: &'static str = "removed.toml";

    pub fn load(state_dir: &std::path::Path) -> Self {
        let path = state_dir.join(Self::FILE);
        let doc = std::fs::read_to_string(&path)
            .ok()
            .and_then(|t| t.parse::<toml_edit::DocumentMut>().ok())
            .unwrap_or_default();
        Self { path, doc }
    }

    /// Append a removal. `now` is unix seconds (injected for testability); the
    /// id is that timestamp as a string.
    pub fn record(&mut self, plan: &RemovalPlan, now: u64) -> Result<String> {
        let id = now.to_string();
        let mut t = toml_edit::Table::new();
        t["id"] = toml_edit::value(&id);
        t["when"] = toml_edit::value(now as i64);
        let mut pkgs = toml_edit::Array::new();
        for p in &plan.packages {
            pkgs.push(p.as_str());
        }
        t["packages"] = toml_edit::value(pkgs);
        let mut webs = toml_edit::Array::new();
        for w in &plan.webapps {
            webs.push(w.as_str());
        }
        t["webapps"] = toml_edit::value(webs);

        let arr = self
            .doc
            .entry("removed")
            .or_insert(toml_edit::Item::ArrayOfTables(
                toml_edit::ArrayOfTables::new(),
            ))
            .as_array_of_tables_mut()
            .ok_or_else(|| StudioError::External {
                cmd: "apps manifest".into(),
                detail: "removed.toml is corrupt — `removed` is not a table array".into(),
            })?;
        arr.push(t);
        Ok(id)
    }

    pub fn save(&self) -> Result<()> {
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&self.path, self.doc.to_string())?;
        Ok(())
    }

    pub fn entries(&self) -> Vec<Removed> {
        let Some(arr) = self.doc.get("removed").and_then(|i| i.as_array_of_tables()) else {
            return Vec::new();
        };
        arr.iter()
            .map(|t| Removed {
                id: t
                    .get("id")
                    .and_then(|v| v.as_str())
                    .unwrap_or_default()
                    .to_string(),
                when: t
                    .get("when")
                    .and_then(|v| v.as_integer())
                    .unwrap_or_default() as u64,
                packages: str_array(t, "packages"),
                webapps: str_array(t, "webapps"),
            })
            .collect()
    }

    pub fn get(&self, id: &str) -> Option<Removed> {
        self.entries().into_iter().find(|e| e.id == id)
    }

    /// The reinstall commands for a logged removal. Packages come back with
    /// `pacman -S --needed`; web apps can't be fully reconstructed (their URL
    /// and icon aren't recoverable), so they're surfaced as guidance.
    pub fn restore_commands(entry: &Removed) -> Vec<String> {
        let mut cmds = Vec::new();
        if !entry.packages.is_empty() {
            cmds.push(format!(
                "sudo pacman -S --needed {}",
                entry.packages.join(" ")
            ));
        }
        cmds
    }
}

fn str_array(t: &toml_edit::Table, key: &str) -> Vec<String> {
    t.get(key)
        .and_then(|v| v.as_array())
        .map(|a| {
            a.iter()
                .filter_map(|v| v.as_str())
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cmd::{CmdOutput, StubRunner};

    fn ok(stdout: &str) -> CmdOutput {
        CmdOutput {
            status: 0,
            stdout: stdout.into(),
            ..Default::default()
        }
    }

    fn fake_paths(tag: &str) -> (OmarchyPaths, PathBuf) {
        let root = std::env::temp_dir().join(format!(
            "omarchy-studio-apps-{tag}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&root).unwrap();
        // Paths are only used for `applications()` (HOME-based) here, so the
        // exact tree doesn't matter — kept off the adapter-only literal.
        let paths = OmarchyPaths {
            system: root.join("sys"),
            config: root.join("cfg/omarchy"),
            state: root.join("state/omarchy"),
        };
        (paths, root)
    }

    #[test]
    fn catalog_ids_are_unique() {
        let mut seen = HashSet::new();
        for item in CATALOG {
            assert!(seen.insert(item.id), "duplicate id {}", item.id);
            if item.kind == Kind::Package {
                assert!(!item.packages.is_empty(), "{} has no packages", item.id);
            }
        }
    }

    #[test]
    fn parse_pacman_q_reads_names() {
        let set = parse_pacman_q("docker 1.2.3\nspotify 4.5\n\n");
        assert!(set.contains("docker"));
        assert!(set.contains("spotify"));
        assert_eq!(set.len(), 2);
    }

    #[test]
    fn inventory_marks_installed_by_primary_package() {
        let runner = StubRunner::default().with(
            // Cmd::display joins program + args by spaces.
            format!(
                "pacman -Q {}",
                CATALOG
                    .iter()
                    .filter(|i| i.kind == Kind::Package)
                    .flat_map(|i| i.packages.iter().copied())
                    .collect::<Vec<_>>()
                    .join(" ")
            ),
            ok("docker 1.0\nspotify 2.0\n"),
        );
        let (paths, _root) = fake_paths("inv");
        let inv = Inventory::probe(&runner, &paths);
        assert!(inv.is_installed(find("docker").unwrap()));
        assert!(inv.is_installed(find("spotify").unwrap()));
        assert!(!inv.is_installed(find("obsidian").unwrap()));
    }

    #[test]
    fn webapp_detected_from_desktop_file() {
        // scan_webapps takes an explicit dir — no HOME dependency, so this is
        // safe under the parallel test runner.
        let (_paths, root) = fake_paths("web");
        let apps = root.join("applications");
        std::fs::create_dir_all(&apps).unwrap();
        std::fs::write(apps.join("GitHub.desktop"), "[Desktop Entry]\n").unwrap();
        let found = scan_webapps(&apps);
        assert!(found.contains("GitHub"));
        assert!(!found.contains("Zoom"));

        // is_installed reads that set (fields are private but visible in-module).
        let inv = Inventory {
            installed_pkgs: HashSet::new(),
            installed_webapps: found,
        };
        assert!(inv.is_installed(find("GitHub").unwrap()));
        assert!(!inv.is_installed(find("Zoom").unwrap()));
    }

    #[test]
    fn plan_computes_cascade_and_commands() {
        let (paths, root) = fake_paths("plan");
        std::env::set_var("HOME", &root);
        let docker = find("docker").unwrap();
        let runner = StubRunner::default()
            .with(
                "pacman -Rs --print --print-format %n docker docker-buildx docker-compose",
                ok("docker\ndocker-buildx\ndocker-compose\nlazydocker\n"),
            )
            .with_ok("systemctl is-enabled docker.service", "enabled\n")
            .with_ok("systemctl is-enabled docker.socket", "disabled\n");
        let plan = RemovalPlan::build(&[docker], &runner, &paths).unwrap();
        assert!(plan.blocker.is_none());
        assert!(plan.can_execute());
        assert_eq!(plan.cascade, vec!["lazydocker"]);
        assert_eq!(plan.services, vec!["docker.service"]);
        let cmds = plan.commands();
        assert_eq!(cmds[0], "sudo systemctl disable --now docker.service");
        assert!(
            cmds[1].starts_with("sudo pacman -Rns --noconfirm docker docker-buildx docker-compose")
        );
    }

    #[test]
    fn dependency_break_blocks_the_plan() {
        let (paths, root) = fake_paths("blocked");
        std::env::set_var("HOME", &root);
        let docker = find("docker").unwrap();
        // pacman refuses: an outside package still depends on the selection.
        let runner = StubRunner::default()
            .with(
                "pacman -Rs --print --print-format %n docker docker-buildx docker-compose",
                CmdOutput {
                    status: 1,
                    stderr:
                        "error: removing docker breaks dependency 'docker' required by ufw-docker"
                            .into(),
                    ..Default::default()
                },
            )
            .with_ok("systemctl is-enabled docker.service", "disabled\n")
            .with_ok("systemctl is-enabled docker.socket", "disabled\n");
        let plan = RemovalPlan::build(&[docker], &runner, &paths).unwrap();
        assert!(plan.blocker.is_some());
        assert!(!plan.can_execute());
        assert!(plan.blocker.as_ref().unwrap().contains("ufw-docker"));
        // executing a blocked plan is refused, not forced.
        assert!(plan.execute(&runner, false).is_err());
    }

    #[test]
    fn webapp_plan_uses_omarchy_remover() {
        let (paths, root) = fake_paths("webplan");
        std::env::set_var("HOME", &root);
        let gh = find("GitHub").unwrap();
        let runner = StubRunner::default();
        let plan = RemovalPlan::build(&[gh], &runner, &paths).unwrap();
        assert!(plan.packages.is_empty());
        assert_eq!(plan.webapps, vec!["GitHub"]);
        assert_eq!(plan.commands(), vec!["omarchy-webapp-remove GitHub"]);
    }

    #[test]
    fn dry_run_executes_nothing() {
        let (paths, root) = fake_paths("dry");
        std::env::set_var("HOME", &root);
        let gh = find("GitHub").unwrap();
        let runner = StubRunner::default();
        let plan = RemovalPlan::build(&[gh], &runner, &paths).unwrap();
        let cmds = plan.execute(&runner, true).unwrap();
        assert_eq!(cmds, vec!["omarchy-webapp-remove GitHub"]);
        // no commands were actually run
        assert!(runner
            .calls()
            .iter()
            .all(|c| !c.contains("omarchy-webapp-remove")));
    }

    #[test]
    fn execute_runs_disable_then_remove() {
        let (paths, root) = fake_paths("exec");
        std::env::set_var("HOME", &root);
        let docker = find("docker").unwrap();
        let build = StubRunner::default()
            .with(
                "pacman -Rs --print --print-format %n docker docker-buildx docker-compose",
                ok("docker\ndocker-buildx\ndocker-compose\n"),
            )
            .with_ok("systemctl is-enabled docker.service", "enabled\n")
            .with_ok("systemctl is-enabled docker.socket", "enabled\n");
        let plan = RemovalPlan::build(&[docker], &build, &paths).unwrap();

        let exec = StubRunner::default()
            .with_ok("sudo systemctl disable --now docker.service", "")
            .with_ok("sudo systemctl disable --now docker.socket", "")
            .with_ok(
                "sudo pacman -Rns --noconfirm docker docker-buildx docker-compose",
                "",
            );
        plan.execute(&exec, false).unwrap();
        let calls = exec.calls();
        assert_eq!(calls[0], "sudo systemctl disable --now docker.service");
        assert!(calls
            .last()
            .unwrap()
            .starts_with("sudo pacman -Rns --noconfirm docker"));
    }

    #[test]
    fn manifest_round_trips_and_restores() {
        let (_paths, root) = fake_paths("manifest");
        let state = root.join("state");
        let mut m = Manifest::load(&state);
        let plan = RemovalPlan {
            ids: vec!["docker".into()],
            packages: vec!["docker".into(), "docker-compose".into()],
            webapps: vec!["GitHub".into()],
            ..Default::default()
        };
        let id = m.record(&plan, 1_700_000_000).unwrap();
        m.save().unwrap();

        let reloaded = Manifest::load(&state);
        let entry = reloaded.get(&id).expect("entry present");
        assert_eq!(entry.packages, vec!["docker", "docker-compose"]);
        assert_eq!(entry.webapps, vec!["GitHub"]);
        assert_eq!(
            Manifest::restore_commands(&entry),
            vec!["sudo pacman -S --needed docker docker-compose"]
        );
    }
}
