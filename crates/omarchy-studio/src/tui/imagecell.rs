//! In-terminal image previews (spec 07 §4, roadmap 0.6.4).
//!
//! One [`ImageCell`] lives on the App and is shared by every screen that
//! previews an image. The terminal's graphics protocol is probed once at
//! startup (kitty → sixel → half-block cells; half-blocks work everywhere,
//! so a "no graphics" terminal still gets a coarse preview). When even the
//! probe fails — not a tty, hostile terminal — screens fall back to text and
//! the `o`-opens-in-imv path (spec 06 §4 degradation table).
//!
//! The probe is deliberately *not* `Picker::from_query_stdio()`: that spawns
//! a thread which reads the tty for query responses, and when the terminal
//! doesn't answer everything (tmux) the thread never exits and steals
//! keystrokes from crossterm for the rest of the session. Environment
//! sniffing + the `TIOCGWINSZ` pixel size cover the terminals Omarchy users
//! actually run, with zero stdin reads.
//!
//! Decoding happens lazily on the draw after the shown path changes, on the
//! UI thread — fine for local wallpapers; the wallhaven grid gets a worker
//! pool when it lands (spec 01 §6).

use std::path::{Path, PathBuf};

use ratatui::layout::Rect;
use ratatui::Frame;
use ratatui_image::picker::{Picker, ProtocolType};
use ratatui_image::protocol::StatefulProtocol;
use ratatui_image::{Resize, StatefulImage};

pub struct ImageCell {
    picker: Picker,
    /// Protocol state for the image currently on screen.
    current: Option<(PathBuf, StatefulProtocol)>,
    /// Last path that failed to decode — don't retry every frame.
    failed: Option<PathBuf>,
}

/// Graphics protocol from the environment, never from a tty round-trip.
/// Conservative: inside tmux always half-blocks (passthrough is off by
/// default and detection would need the racy stdin query).
fn detect_protocol() -> ProtocolType {
    let var = |k: &str| std::env::var(k).unwrap_or_default();
    let term = var("TERM");
    if var("TMUX").is_empty() {
        if !var("KITTY_WINDOW_ID").is_empty() || term.contains("kitty") || term.contains("ghostty")
        {
            return ProtocolType::Kitty;
        }
        if var("TERM_PROGRAM") == "WezTerm" {
            return ProtocolType::Iterm2;
        }
        if term.contains("foot") {
            return ProtocolType::Sixel;
        }
    }
    ProtocolType::Halfblocks
}

impl ImageCell {
    /// Probe once at startup. Cell pixel size comes from the window-size
    /// ioctl where the terminal reports it; otherwise a common 8×16 guess —
    /// only aspect correction depends on it.
    pub fn probe() -> Self {
        let font_size = match ratatui::crossterm::terminal::window_size() {
            Ok(ws) if ws.width > 0 && ws.height > 0 && ws.columns > 0 && ws.rows > 0 => {
                (ws.width / ws.columns, ws.height / ws.rows)
            }
            _ => (8, 16),
        };
        let mut picker = Picker::from_fontsize(font_size);
        picker.set_protocol_type(detect_protocol());
        Self {
            picker,
            current: None,
            failed: None,
        }
    }

    /// Human name for the Doctor screen's capability line.
    pub fn protocol_label(&self) -> &'static str {
        match self.picker.protocol_type() {
            ProtocolType::Kitty => "kitty graphics",
            ProtocolType::Iterm2 => "iTerm2 graphics",
            ProtocolType::Sixel => "sixel",
            ProtocolType::Halfblocks => "half-block cells",
        }
    }

    /// Draw `path` scaled into `area`. Returns false when the file didn't
    /// decode — caller renders its text fallback instead.
    pub fn render(&mut self, f: &mut Frame, area: Rect, path: &Path) -> bool {
        if self.failed.as_deref() == Some(path) {
            return false;
        }
        if self.current.as_ref().map(|(p, _)| p.as_path()) != Some(path) {
            match image::open(path) {
                Ok(img) => {
                    self.current = Some((path.to_path_buf(), self.picker.new_resize_protocol(img)));
                    self.failed = None;
                }
                Err(_) => {
                    self.failed = Some(path.to_path_buf());
                    self.current = None;
                    return false;
                }
            }
        }
        let (_, protocol) = self.current.as_mut().expect("just ensured");
        f.render_stateful_widget(
            StatefulImage::default().resize(Resize::Fit(None)),
            area,
            protocol,
        );
        true
    }
}
