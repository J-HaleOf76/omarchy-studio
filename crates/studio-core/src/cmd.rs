//! External command execution — the single choke point (spec 02 §2).
//!
//! Every external invocation in Studio goes through a [`CommandRunner`] so
//! that tests can stub it, `--dry-run` can intercept it, and timeout/capture
//! behavior is uniform. No module may call `std::process::Command` directly
//! (CI grep enforces this alongside the `$OMARCHY_PATH` rule, spec 02 §6).

use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use crate::error::{Result, StudioError};

/// Default per-command timeout (spec 02 §2). Omarchy/hyprctl commands are
/// fast local execs; anything slower is a hang we want surfaced, not waited on.
pub const DEFAULT_TIMEOUT: Duration = Duration::from_secs(10);

/// A planned external invocation. Built with the fluent methods; executed by
/// a [`CommandRunner`].
#[derive(Debug, Clone)]
pub struct Cmd {
    pub program: String,
    pub args: Vec<String>,
    pub envs: Vec<(String, String)>,
    pub timeout: Duration,
}

impl Cmd {
    pub fn new(program: impl Into<String>) -> Self {
        Self {
            program: program.into(),
            args: Vec::new(),
            envs: Vec::new(),
            timeout: DEFAULT_TIMEOUT,
        }
    }

    pub fn arg(mut self, a: impl Into<String>) -> Self {
        self.args.push(a.into());
        self
    }

    pub fn args<I, S>(mut self, args: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.args.extend(args.into_iter().map(Into::into));
        self
    }

    pub fn env(mut self, k: impl Into<String>, v: impl Into<String>) -> Self {
        self.envs.push((k.into(), v.into()));
        self
    }

    pub fn timeout(mut self, t: Duration) -> Self {
        self.timeout = t;
        self
    }

    /// Human/log rendering: `hyprctl binds -j`. Also the key used by stubs.
    pub fn display(&self) -> String {
        std::iter::once(self.program.as_str())
            .chain(self.args.iter().map(String::as_str))
            .collect::<Vec<_>>()
            .join(" ")
    }
}

#[derive(Debug, Clone, Default)]
pub struct CmdOutput {
    pub status: i32,
    pub stdout: String,
    pub stderr: String,
    pub duration: Duration,
}

impl CmdOutput {
    pub fn ok(&self) -> bool {
        self.status == 0
    }
}

pub trait CommandRunner: Send + Sync {
    fn run(&self, cmd: &Cmd) -> Result<CmdOutput>;
}

/// Executes for real: captured stdio, `LC_ALL=C`, kill-on-timeout.
pub struct RealRunner;

impl CommandRunner for RealRunner {
    fn run(&self, cmd: &Cmd) -> Result<CmdOutput> {
        let mut command = Command::new(&cmd.program);
        command
            .args(&cmd.args)
            .env("LC_ALL", "C")
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        for (k, v) in &cmd.envs {
            command.env(k, v);
        }

        let mut child = command.spawn().map_err(|e| {
            let detail = if e.kind() == std::io::ErrorKind::NotFound {
                "not installed (command not found)".to_string()
            } else {
                e.to_string()
            };
            StudioError::External {
                cmd: cmd.display(),
                detail,
            }
        })?;

        // Reader threads keep the pipes drained so a chatty child can't
        // deadlock on a full pipe buffer while we poll for exit.
        let mut out_pipe = child.stdout.take().expect("stdout piped");
        let mut err_pipe = child.stderr.take().expect("stderr piped");
        let out_thread = std::thread::spawn(move || {
            let mut s = String::new();
            let _ = out_pipe.read_to_string(&mut s);
            s
        });
        let err_thread = std::thread::spawn(move || {
            let mut s = String::new();
            let _ = err_pipe.read_to_string(&mut s);
            s
        });

        let start = Instant::now();
        let status = loop {
            match child.try_wait() {
                Ok(Some(status)) => break status,
                Ok(None) if start.elapsed() >= cmd.timeout => {
                    let _ = child.kill();
                    let _ = child.wait();
                    let _ = out_thread.join();
                    let _ = err_thread.join();
                    return Err(StudioError::External {
                        cmd: cmd.display(),
                        detail: format!("timed out after {:.1}s", cmd.timeout.as_secs_f32()),
                    });
                }
                Ok(None) => std::thread::sleep(Duration::from_millis(10)),
                Err(e) => {
                    let _ = child.kill();
                    return Err(StudioError::External {
                        cmd: cmd.display(),
                        detail: e.to_string(),
                    });
                }
            }
        };

        Ok(CmdOutput {
            status: status.code().unwrap_or(-1),
            stdout: out_thread.join().unwrap_or_default(),
            stderr: err_thread.join().unwrap_or_default(),
            duration: start.elapsed(),
        })
    }
}

/// `--dry-run`: records what would run, pretends success with empty output.
/// Callers must not gate verification logic on dry-run output — the apply
/// pipeline skips verification entirely under dry-run (spec 08 globals).
#[derive(Default)]
pub struct DryRunner {
    recorded: Mutex<Vec<String>>,
}

impl DryRunner {
    pub fn recorded(&self) -> Vec<String> {
        self.recorded.lock().expect("dry-run lock").clone()
    }
}

impl CommandRunner for DryRunner {
    fn run(&self, cmd: &Cmd) -> Result<CmdOutput> {
        self.recorded
            .lock()
            .expect("dry-run lock")
            .push(cmd.display());
        Ok(CmdOutput::default())
    }
}

/// Scripted runner for tests (spec 09 §3): exact `display()` string → output.
/// Unscripted commands error loudly so tests can't silently drift.
#[derive(Default)]
pub struct StubRunner {
    scripts: Vec<(String, CmdOutput)>,
    calls: Mutex<Vec<String>>,
}

impl StubRunner {
    pub fn with(mut self, display: impl Into<String>, output: CmdOutput) -> Self {
        self.scripts.push((display.into(), output));
        self
    }

    pub fn with_ok(self, display: impl Into<String>, stdout: impl Into<String>) -> Self {
        self.with(
            display,
            CmdOutput {
                status: 0,
                stdout: stdout.into(),
                ..Default::default()
            },
        )
    }

    pub fn with_fail(self, display: impl Into<String>) -> Self {
        self.with(
            display,
            CmdOutput {
                status: 1,
                ..Default::default()
            },
        )
    }

    pub fn calls(&self) -> Vec<String> {
        self.calls.lock().expect("stub lock").clone()
    }
}

impl CommandRunner for StubRunner {
    fn run(&self, cmd: &Cmd) -> Result<CmdOutput> {
        let display = cmd.display();
        self.calls.lock().expect("stub lock").push(display.clone());
        self.scripts
            .iter()
            .find(|(d, _)| *d == display)
            .map(|(_, o)| Ok(o.clone()))
            .unwrap_or_else(|| {
                Err(StudioError::External {
                    cmd: display,
                    detail: "StubRunner: no script for this command".into(),
                })
            })
    }
}

/// In-process `command -v`: search `$PATH` for an executable file.
/// Used by dependency probing (spec 06 §2) and capability probing.
pub fn find_in_path(name: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&path) {
        let candidate = dir.join(name);
        if is_executable(&candidate) {
            return Some(candidate);
        }
    }
    None
}

fn is_executable(p: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    p.is_file()
        && std::fs::metadata(p)
            .map(|m| m.permissions().mode() & 0o111 != 0)
            .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn captures_stdout_and_status() {
        let out = RealRunner.run(&Cmd::new("echo").arg("hello")).unwrap();
        assert!(out.ok());
        assert_eq!(out.stdout, "hello\n");
        assert_eq!(out.stderr, "");
    }

    #[test]
    fn reports_nonzero_status_without_erroring() {
        let out = RealRunner
            .run(&Cmd::new("sh").args(["-c", "echo oops >&2; exit 3"]))
            .unwrap();
        assert_eq!(out.status, 3);
        assert_eq!(out.stderr, "oops\n");
    }

    #[test]
    fn missing_binary_is_a_clear_error() {
        let err = RealRunner
            .run(&Cmd::new("definitely-not-a-real-binary-omarchy-studio"))
            .unwrap_err();
        match err {
            StudioError::External { detail, .. } => assert!(detail.contains("not installed")),
            other => panic!("wrong variant: {other:?}"),
        }
    }

    #[test]
    fn kills_on_timeout() {
        let started = Instant::now();
        let err = RealRunner
            .run(
                &Cmd::new("sleep")
                    .arg("5")
                    .timeout(Duration::from_millis(100)),
            )
            .unwrap_err();
        assert!(
            started.elapsed() < Duration::from_secs(2),
            "did not kill promptly"
        );
        match err {
            StudioError::External { detail, .. } => assert!(detail.contains("timed out")),
            other => panic!("wrong variant: {other:?}"),
        }
    }

    #[test]
    fn sets_lc_all_c() {
        let out = RealRunner
            .run(&Cmd::new("sh").args(["-c", "printf %s \"$LC_ALL\""]))
            .unwrap();
        assert_eq!(out.stdout, "C");
    }

    #[test]
    fn dry_runner_records_and_pretends_success() {
        let dry = DryRunner::default();
        let out = dry.run(&Cmd::new("hyprctl").arg("reload")).unwrap();
        assert!(out.ok());
        assert_eq!(dry.recorded(), vec!["hyprctl reload"]);
    }

    #[test]
    fn stub_runner_scripts_and_rejects_unscripted() {
        let stub = StubRunner::default().with_ok("hyprctl version", "Hyprland 0.55.2\n");
        assert_eq!(
            stub.run(&Cmd::new("hyprctl").arg("version"))
                .unwrap()
                .stdout,
            "Hyprland 0.55.2\n"
        );
        assert!(stub.run(&Cmd::new("waybar")).is_err());
        assert_eq!(stub.calls().len(), 2);
    }

    #[test]
    fn finds_binaries_in_path() {
        assert!(find_in_path("sh").is_some());
        assert!(find_in_path("definitely-not-a-real-binary-omarchy-studio").is_none());
    }
}
