//! TUI shell (spec 07): module rail, content pane, keybar + jargon strip,
//! help overlay, command palette. Studio themes itself from the active
//! Omarchy palette. Elm-ish: events → `handle` → optional side effect → redraw.
//!
//! v0.1 wires the Themes screen for real; the other rail entries render an
//! honest "arriving in <milestone>" placeholder so the shell is navigable
//! end-to-end without pretending features exist.

mod chord;
mod theme;
mod screens {
    pub mod animations;
    pub mod keybinds;
    pub mod looknfeel;
    pub mod notifications;
    pub mod palette_editor;
    pub mod themes;
    pub mod waybar;
}

use ratatui::crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, List, ListItem, Paragraph, Wrap};
use ratatui::Frame;

use studio_core::cmd::{CommandRunner, RealRunner};
use studio_core::modules::themes::ThemeStore;
use studio_core::omarchy::{cmds, OmarchyPaths};
use studio_core::snapshot::{SnapshotKind, SnapshotStore};

use screens::animations::{AnimAction, AnimationsScreen};
use screens::keybinds::{KeybindAction, KeybindsScreen};
use screens::looknfeel::{LookFeelAction, LookFeelScreen};
use screens::notifications::{NotifAction, NotificationsScreen};
use screens::palette_editor::{EditorAction, PaletteEditor};
use screens::themes::{ThemeAction, ThemesScreen};
use screens::waybar::{WaybarAction, WaybarScreen};
use theme::Skin;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Screen {
    Themes,
    Wallpapers,
    Keybinds,
    LookFeel,
    Animations,
    Waybar,
    Notifications,
    LockIdle,
    Snapshots,
    Integrations,
    Doctor,
}

impl Screen {
    const ALL: [Screen; 11] = [
        Screen::Themes,
        Screen::Wallpapers,
        Screen::Keybinds,
        Screen::LookFeel,
        Screen::Animations,
        Screen::Waybar,
        Screen::Notifications,
        Screen::LockIdle,
        Screen::Snapshots,
        Screen::Integrations,
        Screen::Doctor,
    ];

    fn label(self) -> &'static str {
        match self {
            Screen::Themes => "Themes",
            Screen::Wallpapers => "Wallpaper",
            Screen::Keybinds => "Keybinds",
            Screen::LookFeel => "Look & Feel",
            Screen::Animations => "Animations",
            Screen::Waybar => "Waybar",
            Screen::Notifications => "Notifs / OSD",
            Screen::LockIdle => "Lock & Idle",
            Screen::Snapshots => "Snapshots",
            Screen::Integrations => "Integrations",
            Screen::Doctor => "Doctor",
        }
    }

    /// Roadmap milestone a not-yet-built screen is coming in (spec honesty).
    fn arriving(self) -> &'static str {
        match self {
            Screen::Keybinds => "v0.2",
            Screen::LookFeel | Screen::Animations => "v0.3",
            Screen::Waybar => "v0.4",
            Screen::Notifications | Screen::LockIdle => "v0.5",
            Screen::Wallpapers | Screen::Integrations => "v0.6",
            _ => "soon",
        }
    }
}

struct Toast {
    text: String,
    ok: bool,
}

struct App {
    paths: OmarchyPaths,
    skin: Skin,
    screen: Screen,
    themes: ThemesScreen,
    keybinds: KeybindsScreen,
    looknfeel: LookFeelScreen,
    animations: AnimationsScreen,
    waybar: WaybarScreen,
    notifications: NotificationsScreen,
    /// Active palette editor (modal over the content pane), with the theme
    /// slug that was showing before it opened (restored on cancel).
    editor: Option<(PaletteEditor, String)>,
    palette: Option<CommandPalette>,
    help: bool,
    toast: Option<Toast>,
    quit: bool,
}

impl App {
    fn new(paths: OmarchyPaths) -> Self {
        let skin = Skin::from_current(&paths);
        let themes = ThemesScreen::load(&paths);
        // If a previous session died mid-preview, its live `hyprctl keyword`
        // values are still applied but not on disk — reload to discard them.
        let state = studio_core::studio_state_dir();
        if studio_core::modules::looknfeel::preview_was_active(&state) {
            let _ = RealRunner.run(&cmds::hypr_reload());
            studio_core::modules::looknfeel::clear_preview_marker(&state);
        }
        let keybinds = KeybindsScreen::load(&paths, &RealRunner);
        let looknfeel = LookFeelScreen::load(&paths);
        let animations = AnimationsScreen::load(&paths);
        let waybar = WaybarScreen::load(&paths);
        let mut notifications = NotificationsScreen::load(&paths);
        notifications.set_dnd(Some(studio_core::modules::mako::dnd_active(&RealRunner)));
        Self {
            paths,
            skin,
            screen: Screen::Themes,
            themes,
            keybinds,
            looknfeel,
            animations,
            waybar,
            notifications,
            editor: None,
            palette: None,
            help: false,
            toast: None,
            quit: false,
        }
    }

    fn on_key(&mut self, key: KeyEvent) {
        self.toast = None;

        // Overlays consume input first.
        if self.help {
            self.help = false;
            return;
        }
        if self.palette.is_some() {
            self.palette_key(key);
            return;
        }
        // The palette editor is a focused mode: it owns all keys while open.
        if self.editor.is_some() {
            self.editor_key(key);
            return;
        }

        // Global chords (only when no in-screen text/chord/modal field is active).
        let capturing = (matches!(self.screen, Screen::Themes) && self.themes.is_capturing())
            || (matches!(self.screen, Screen::Keybinds) && self.keybinds.is_capturing())
            || (matches!(self.screen, Screen::LookFeel) && self.looknfeel.is_modal())
            || (matches!(self.screen, Screen::Waybar) && self.waybar.is_modal())
            || (matches!(self.screen, Screen::Notifications) && self.notifications.is_modal());
        if !capturing {
            match key.code {
                KeyCode::Char('q') => {
                    self.quit = true;
                    return;
                }
                KeyCode::Char('?') => {
                    self.help = true;
                    return;
                }
                KeyCode::Char('/') => {
                    self.palette = Some(CommandPalette::new(&self.paths));
                    return;
                }
                KeyCode::Tab => {
                    self.cycle(1);
                    return;
                }
                KeyCode::BackTab => {
                    self.cycle(-1);
                    return;
                }
                KeyCode::Char(d @ '1'..='9') => {
                    self.jump(d as usize - '1' as usize);
                    return;
                }
                KeyCode::Char('0') => {
                    self.jump(9);
                    return;
                }
                _ => {}
            }
        }
        // Esc quits from the home screen, else returns there.
        if key.code == KeyCode::Esc && !capturing {
            if self.screen == Screen::Themes {
                self.quit = true;
            } else {
                self.screen = Screen::Themes;
            }
            return;
        }

        // Delegate to the active screen.
        match self.screen {
            Screen::Themes => match self.themes.handle(key) {
                ThemeAction::None => {}
                ThemeAction::Apply(slug) => self.apply_theme(&slug),
                ThemeAction::Fork { src, name } => self.fork_theme(&src, &name),
                ThemeAction::Edit(slug) => self.open_editor(&slug),
            },
            Screen::Keybinds => match self.keybinds.handle(key) {
                KeybindAction::None => {}
                KeybindAction::Commit(summary) => self.commit_keybinds(summary),
            },
            Screen::LookFeel => match self.looknfeel.handle(key) {
                LookFeelAction::None => {}
                LookFeelAction::Preview(idx) => {
                    studio_core::modules::looknfeel::mark_preview_active(
                        &studio_core::studio_state_dir(),
                    );
                    if let Err(e) = self.looknfeel.preview(idx, &RealRunner) {
                        self.toast = Some(Toast {
                            text: format!("preview failed: {}", brief(e)),
                            ok: false,
                        });
                    }
                }
                LookFeelAction::PreviewAll => {
                    studio_core::modules::looknfeel::mark_preview_active(
                        &studio_core::studio_state_dir(),
                    );
                    match self.looknfeel.preview_all(&RealRunner) {
                        Ok(()) => {
                            self.toast = Some(Toast {
                                text: "Trying preset live — s to keep, R to revert".into(),
                                ok: true,
                            })
                        }
                        Err(e) => {
                            self.toast = Some(Toast {
                                text: format!("preview failed: {}", brief(e)),
                                ok: false,
                            })
                        }
                    }
                }
                LookFeelAction::Save => self.save_looknfeel(),
                LookFeelAction::ResetAll => self.reset_looknfeel(),
                LookFeelAction::OpenToggles => {
                    use studio_core::modules::toggles::TOGGLES;
                    let rows = TOGGLES
                        .iter()
                        .map(|t| screens::looknfeel::ToggleRow {
                            id: t.id.to_string(),
                            label: t.label.to_string(),
                            on: t.is_on(&RealRunner),
                        })
                        .collect();
                    self.looknfeel.open_toggles(rows);
                }
                LookFeelAction::FlipToggle(id) => {
                    use studio_core::modules::toggles::lookup;
                    if let Some(t) = lookup(&id) {
                        match t.toggle(&RealRunner) {
                            Ok(()) => {
                                let now = t.is_on(&RealRunner);
                                self.looknfeel.set_toggle_state(&id, now);
                                let label = match now {
                                    Some(true) => format!("{} on", t.label),
                                    Some(false) => format!("{} off", t.label),
                                    None => format!("Toggled {}", t.label),
                                };
                                self.toast = Some(Toast {
                                    text: label,
                                    ok: true,
                                });
                            }
                            Err(e) => {
                                self.toast = Some(Toast {
                                    text: format!("toggle failed: {}", brief(e)),
                                    ok: false,
                                });
                            }
                        }
                    }
                }
            },
            Screen::Animations => match self.animations.handle(key) {
                AnimAction::None => {}
                AnimAction::Apply(name) => self.apply_animation(&name),
            },
            Screen::Waybar => match self.waybar.handle(key) {
                WaybarAction::None => {}
                WaybarAction::Apply => self.apply_waybar(),
            },
            Screen::Notifications => match self.notifications.handle(key) {
                NotifAction::None => {}
                NotifAction::Save => self.apply_notifications(),
                NotifAction::ToggleDnd => self.toggle_dnd(),
                NotifAction::Test(urgency) => self.notif_test(&urgency),
                NotifAction::OsdTest => self.osd_test(),
            },
            _ => {}
        }
    }

    /// Snapshot config.jsonc, save the bar layout, restart Waybar, refresh.
    fn apply_waybar(&mut self) {
        let Some(file) = self.waybar.config_path() else {
            return;
        };
        let store = SnapshotStore::open_or_init(
            studio_core::studio_state_dir().join("history"),
            Box::new(RealRunner),
        );
        if let Ok(s) = &store {
            let _ = s.record(
                SnapshotKind::Pre,
                "before: waybar layout change",
                std::slice::from_ref(&file),
                "waybar",
                &[],
            );
        }
        use studio_core::modules::waybar::ApplyOutcome;
        match self.waybar.commit(&RealRunner) {
            Ok(outcome) => {
                if let Ok(s) = &store {
                    let _ = s.record(
                        SnapshotKind::Post,
                        "waybar layout change",
                        std::slice::from_ref(&file),
                        "waybar",
                        &[],
                    );
                }
                self.waybar.reload(&self.paths);
                self.toast = Some(match outcome {
                    ApplyOutcome::Reverted => Toast {
                        text: "That change would have stopped Waybar — reverted.".into(),
                        ok: false,
                    },
                    _ => Toast {
                        text: "Saved bar layout · undo from Snapshots".into(),
                        ok: true,
                    },
                });
            }
            Err(e) => {
                self.toast = Some(Toast {
                    text: format!("waybar apply failed: {}", brief(e)),
                    ok: false,
                });
            }
        }
    }

    /// Snapshot the mako template, write behavior, regenerate + reload, refresh.
    fn apply_notifications(&mut self) {
        let file = self.notifications.model().tpl_path().to_path_buf();
        let store = SnapshotStore::open_or_init(
            studio_core::studio_state_dir().join("history"),
            Box::new(RealRunner),
        );
        if let Ok(s) = &store {
            let _ = s.record(
                SnapshotKind::Pre,
                "before: notification behavior",
                std::slice::from_ref(&file),
                "mako",
                &[],
            );
        }
        match self.notifications.model().apply(&RealRunner) {
            Ok(()) => {
                if let Ok(s) = &store {
                    let _ = s.record(
                        SnapshotKind::Post,
                        "notification behavior",
                        std::slice::from_ref(&file),
                        "mako",
                        &[],
                    );
                }
                self.notifications.reload(&self.paths);
                self.refresh_dnd();
                self.toast = Some(Toast {
                    text: "Saved notification behavior · undo from Snapshots".into(),
                    ok: true,
                });
            }
            Err(e) => {
                self.toast = Some(Toast {
                    text: format!("mako apply failed: {}", brief(e)),
                    ok: false,
                });
            }
        }
    }

    /// Flip Do Not Disturb live and reflect the new state on the screen.
    fn toggle_dnd(&mut self) {
        use studio_core::modules::mako;
        let now = mako::dnd_active(&RealRunner);
        match mako::set_dnd(&RealRunner, !now) {
            Ok(()) => {
                self.notifications.set_dnd(Some(!now));
                self.toast = Some(Toast {
                    text: format!("Do Not Disturb {}", if !now { "on" } else { "off" }),
                    ok: true,
                });
            }
            Err(e) => {
                self.toast = Some(Toast {
                    text: format!("couldn't toggle DND: {}", brief(e)),
                    ok: false,
                });
            }
        }
    }

    fn notif_test(&mut self, urgency: &str) {
        use studio_core::modules::mako;
        self.toast = Some(match mako::send_sample(&RealRunner, urgency) {
            Ok(()) => Toast {
                text: format!("sent a {urgency} test notification"),
                ok: true,
            },
            Err(e) => Toast {
                text: format!("couldn't send test: {}", brief(e)),
                ok: false,
            },
        });
    }

    fn refresh_dnd(&mut self) {
        use studio_core::modules::mako;
        self.notifications
            .set_dnd(Some(mako::dnd_active(&RealRunner)));
    }

    fn osd_test(&mut self) {
        use studio_core::modules::swayosd;
        self.toast = Some(match swayosd::self_test(&RealRunner) {
            Ok(()) => Toast {
                text: "flashed the on-screen display".into(),
                ok: true,
            },
            Err(e) => Toast {
                text: format!("couldn't flash OSD: {}", brief(e)),
                ok: false,
            },
        });
    }

    /// Snapshot looknfeel.conf, apply an animation preset, live-reload, refresh.
    fn apply_animation(&mut self, name: &str) {
        use studio_core::modules::animations;
        let Some(preset) = animations::preset(name) else {
            return;
        };
        let file = self.paths.hypr_config().join("looknfeel.conf");
        let store = SnapshotStore::open_or_init(
            studio_core::studio_state_dir().join("history"),
            Box::new(RealRunner),
        );
        if let Ok(s) = &store {
            let _ = s.record(
                SnapshotKind::Pre,
                &format!("before animations: {name}"),
                std::slice::from_ref(&file),
                "animations",
                &[],
            );
        }
        match animations::apply(&self.paths, preset, &RealRunner) {
            Ok(_) => {
                if let Ok(s) = &store {
                    let _ = s.record(
                        SnapshotKind::Post,
                        &format!("animations: {name}"),
                        std::slice::from_ref(&file),
                        "animations",
                        &[],
                    );
                }
                self.animations.reload(&self.paths);
                self.toast = Some(Toast {
                    text: format!("Animations: {name} · undo from Snapshots"),
                    ok: true,
                });
            }
            Err(e) => {
                self.toast = Some(Toast {
                    text: format!("animation change failed: {}", brief(e)),
                    ok: false,
                });
            }
        }
    }

    /// Persist the look & feel batch with a snapshot, then reload the screen.
    fn save_looknfeel(&mut self) {
        if !self.looknfeel.any_overrides() {
            self.toast = Some(Toast {
                text: "Nothing to save — no changes from the Omarchy defaults".into(),
                ok: true,
            });
            return;
        }
        let file = self.paths.hypr_config().join("looknfeel.conf");
        let store = SnapshotStore::open_or_init(
            studio_core::studio_state_dir().join("history"),
            Box::new(RealRunner),
        );
        if let Ok(s) = &store {
            let _ = s.record(
                SnapshotKind::Pre,
                "before: look & feel change",
                std::slice::from_ref(&file),
                "looknfeel",
                &[],
            );
        }
        match self.looknfeel.commit(&self.paths, &RealRunner) {
            Ok(()) => {
                if let Ok(s) = &store {
                    let _ = s.record(
                        SnapshotKind::Post,
                        "look & feel change",
                        std::slice::from_ref(&file),
                        "looknfeel",
                        &[],
                    );
                }
                studio_core::modules::looknfeel::clear_preview_marker(
                    &studio_core::studio_state_dir(),
                );
                self.looknfeel.reload(&self.paths);
                self.toast = Some(Toast {
                    text: "Saved look & feel · undo from Snapshots".into(),
                    ok: true,
                });
            }
            Err(e) => {
                self.toast = Some(Toast {
                    text: format!("save failed: {}", brief(e)),
                    ok: false,
                });
            }
        }
    }

    /// Remove all look & feel overrides and reload Hyprland to the base config.
    fn reset_looknfeel(&mut self) {
        match self.looknfeel.commit(&self.paths, &RealRunner) {
            Ok(()) => {
                studio_core::modules::looknfeel::clear_preview_marker(
                    &studio_core::studio_state_dir(),
                );
                self.looknfeel.reload(&self.paths);
                self.toast = Some(Toast {
                    text: "Reset look & feel to Omarchy defaults".into(),
                    ok: true,
                });
            }
            Err(e) => {
                self.toast = Some(Toast {
                    text: format!("reset failed: {}", brief(e)),
                    ok: false,
                });
            }
        }
    }

    /// Snapshot the bindings file, persist the override block, live-reload, and
    /// refresh the screen — the full undoable apply for a keybind change.
    fn commit_keybinds(&mut self, summary: String) {
        let file = studio_core::modules::keybinds::user_bindings_path(&self.paths);
        let store = SnapshotStore::open_or_init(
            studio_core::studio_state_dir().join("history"),
            Box::new(RealRunner),
        );
        if let Ok(s) = &store {
            let _ = s.record(
                SnapshotKind::Pre,
                &format!("before: {summary}"),
                std::slice::from_ref(&file),
                "keybinds",
                &[],
            );
        }
        match self.keybinds.commit(&self.paths, &RealRunner) {
            Ok(()) => {
                if let Ok(s) = &store {
                    let _ = s.record(
                        SnapshotKind::Post,
                        &summary,
                        std::slice::from_ref(&file),
                        "keybinds",
                        &[],
                    );
                }
                self.keybinds.reload(&self.paths, &RealRunner);
                self.toast = Some(Toast {
                    text: format!("{summary} · undo with u on Snapshots"),
                    ok: true,
                });
            }
            Err(e) => {
                self.toast = Some(Toast {
                    text: format!("keybind change failed: {}", brief(e)),
                    ok: false,
                });
            }
        }
    }

    fn open_editor(&mut self, slug: &str) {
        let store = ThemeStore::from_paths(&self.paths);
        let Some(theme) = store.get(slug) else { return };
        match PaletteEditor::open(&theme) {
            Some(ed) => {
                let origin = self.paths.current_theme_name().unwrap_or_default();
                self.editor = Some((ed, origin));
            }
            None => {
                self.toast = Some(Toast {
                    text: format!("{slug} has no colors.toml to edit"),
                    ok: false,
                });
            }
        }
    }

    fn editor_key(&mut self, key: KeyEvent) {
        let Some((ed, _)) = self.editor.as_mut() else {
            return;
        };
        let store = ThemeStore::from_paths(&self.paths);
        match ed.handle(key) {
            EditorAction::None => {}
            EditorAction::Preview => {
                // Ephemeral: bypasses the snapshot pipeline by design (spec 04 §2).
                if let Err(e) = store.preview(ed.working(), ed.light, &RealRunner) {
                    self.toast = Some(Toast {
                        text: format!("preview failed: {}", brief(e)),
                        ok: false,
                    });
                }
            }
            EditorAction::Save => self.save_editor(),
            EditorAction::Cancel => self.cancel_editor(),
        }
    }

    fn save_editor(&mut self) {
        let Some((ed, _)) = self.editor.take() else {
            return;
        };
        let store = ThemeStore::from_paths(&self.paths);
        let Some(theme) = store.get(&ed.slug) else {
            return;
        };
        let dest = theme.writable_colors_path(&store.user_dir);
        let result = studio_core::configfs::atomic_write(&dest, &ed.working().to_string())
            .and_then(|_| {
                RealRunner
                    .run(&cmds::theme_set(&ed.slug))
                    .map(|o| (o, dest.clone()))
            });
        store.cleanup_preview();
        match result {
            Ok((o, _)) if o.ok() => {
                self.skin = Skin::from_current(&self.paths);
                self.themes.reload(&self.paths);
                self.toast = Some(Toast {
                    text: format!("Saved & applied {}", ed.pretty),
                    ok: true,
                });
            }
            Ok((o, _)) => {
                self.toast = Some(Toast {
                    text: format!("saved, but apply failed: {}", o.stderr.trim()),
                    ok: false,
                })
            }
            Err(e) => {
                self.toast = Some(Toast {
                    text: format!("save failed: {}", brief(e)),
                    ok: false,
                })
            }
        }
    }

    fn cancel_editor(&mut self) {
        let Some((ed, origin)) = self.editor.take() else {
            return;
        };
        let store = ThemeStore::from_paths(&self.paths);
        store.cleanup_preview();
        // Restore whatever was showing before we started previewing.
        if !origin.is_empty() {
            let _ = RealRunner.run(&cmds::theme_set(&origin));
        }
        self.skin = Skin::from_current(&self.paths);
        let msg = if ed.dirty {
            "Discarded palette edits"
        } else {
            "Closed palette editor"
        };
        self.toast = Some(Toast {
            text: msg.into(),
            ok: true,
        });
    }

    fn cycle(&mut self, dir: i32) {
        let i = Screen::ALL.iter().position(|s| *s == self.screen).unwrap();
        let n = Screen::ALL.len() as i32;
        let next = ((i as i32 + dir).rem_euclid(n)) as usize;
        self.screen = Screen::ALL[next];
    }

    fn jump(&mut self, idx: usize) {
        if let Some(s) = Screen::ALL.get(idx) {
            self.screen = *s;
        }
    }

    fn apply_theme(&mut self, slug: &str) {
        match RealRunner.run(&cmds::theme_set(slug)) {
            Ok(o) if o.ok() => {
                // The desktop just re-themed; restyle Studio to match.
                self.skin = Skin::from_current(&self.paths);
                self.themes.reload(&self.paths);
                self.toast = Some(Toast {
                    text: format!("Applied {slug}"),
                    ok: true,
                });
            }
            Ok(o) => {
                self.toast = Some(Toast {
                    text: format!("omarchy-theme-set failed: {}", o.stderr.trim()),
                    ok: false,
                })
            }
            Err(e) => {
                self.toast = Some(Toast {
                    text: format!("could not run omarchy-theme-set: {e:?}"),
                    ok: false,
                })
            }
        }
    }

    fn fork_theme(&mut self, src: &str, name: &str) {
        let store = ThemeStore::from_paths(&self.paths);
        let Some(theme) = store.get(src) else { return };
        match store.fork(&theme, name) {
            Ok(t) => {
                self.themes.reload(&self.paths);
                self.toast = Some(Toast {
                    text: format!("Forked {src} → {} (press enter to apply)", t.slug),
                    ok: true,
                });
            }
            Err(e) => {
                let detail = match e {
                    studio_core::StudioError::External { detail, .. } => detail,
                    other => format!("{other:?}"),
                };
                self.toast = Some(Toast {
                    text: format!("Fork failed: {detail}"),
                    ok: false,
                });
            }
        }
    }

    fn palette_key(&mut self, key: KeyEvent) {
        let Some(p) = self.palette.as_mut() else {
            return;
        };
        match key.code {
            KeyCode::Esc => self.palette = None,
            KeyCode::Enter => {
                let choice = p.selected();
                self.palette = None;
                if let Some(c) = choice {
                    self.run_palette_choice(c);
                }
            }
            KeyCode::Up => p.up(),
            KeyCode::Down => p.down(),
            KeyCode::Backspace => p.backspace(),
            KeyCode::Char(c) => p.push(c),
            _ => {}
        }
    }

    fn run_palette_choice(&mut self, c: Choice) {
        match c {
            Choice::Go(s) => self.screen = s,
            Choice::Theme(slug) => {
                self.screen = Screen::Themes;
                self.themes.focus(&slug);
            }
        }
    }

    fn draw(&mut self, f: &mut Frame) {
        let cols = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Length(16), Constraint::Min(20)])
            .split(f.area());
        self.draw_rail(f, cols[0]);

        let rows = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(1),
                Constraint::Min(1),
                Constraint::Length(1),
            ])
            .split(cols[1]);
        self.draw_header(f, rows[0]);
        self.draw_content(f, rows[1]);
        self.draw_keybar(f, rows[2]);

        if self.help {
            self.draw_help(f);
        }
        if let Some(p) = &self.palette {
            p.render(f, &self.skin);
        }
    }

    fn draw_rail(&self, f: &mut Frame, area: Rect) {
        let items: Vec<ListItem> = Screen::ALL
            .iter()
            .enumerate()
            .map(|(i, s)| {
                let key = if i < 9 {
                    format!("{} ", i + 1)
                } else if i == 9 {
                    "0 ".to_string()
                } else {
                    "  ".to_string()
                };
                let style = if *s == self.screen {
                    self.skin.selection()
                } else {
                    self.skin.body()
                };
                ListItem::new(Line::from(vec![
                    Span::styled(key, self.skin.dim()),
                    Span::styled(s.label(), style),
                ]))
            })
            .collect();
        let block = Block::default()
            .borders(Borders::RIGHT)
            .border_style(self.skin.border());
        f.render_widget(List::new(items).block(block), area);
    }

    fn draw_header(&self, f: &mut Frame, area: Rect) {
        let current = self.paths.current_theme_name().unwrap_or_default();
        let line = Line::from(vec![
            Span::styled(" Omarchy Studio ", self.skin.accent_bold()),
            Span::styled(format!("· {}", self.screen.label()), self.skin.body()),
            Span::styled(format!("    {current}"), self.skin.dim()),
        ]);
        f.render_widget(Paragraph::new(line), area);
    }

    fn draw_content(&mut self, f: &mut Frame, area: Rect) {
        let pad = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Length(1), Constraint::Min(1)])
            .split(area)[1];
        if let Some((ed, _)) = &self.editor {
            ed.render(f, pad, &self.skin);
            return;
        }
        match self.screen {
            Screen::Themes => self.themes.render(f, pad, &self.skin),
            Screen::Keybinds => self.keybinds.render(f, pad, &self.skin),
            Screen::LookFeel => self.looknfeel.render(f, pad, &self.skin),
            Screen::Animations => self.animations.render(f, pad, &self.skin),
            Screen::Waybar => self.waybar.render(f, pad, &self.skin),
            Screen::Notifications => self.notifications.render(f, pad, &self.skin),
            other => self.draw_placeholder(f, pad, other),
        }
    }

    fn draw_placeholder(&self, f: &mut Frame, area: Rect, s: Screen) {
        let text = vec![
            Line::from(Span::styled(s.label(), self.skin.accent_bold())),
            Line::from(""),
            Line::from(Span::styled(
                format!("This screen arrives in {}.", s.arriving()),
                self.skin.body(),
            )),
            Line::from(Span::styled(
                "The engine underneath (config parsing, apply pipeline, snapshots,",
                self.skin.dim(),
            )),
            Line::from(Span::styled(
                "undo) is already built and tested — this is the UI on top.",
                self.skin.dim(),
            )),
        ];
        f.render_widget(Paragraph::new(text).wrap(Wrap { trim: true }), area);
    }

    fn draw_keybar(&self, f: &mut Frame, area: Rect) {
        if let Some(t) = &self.toast {
            let style = if t.ok {
                self.skin.ok()
            } else {
                self.skin.error()
            };
            f.render_widget(
                Paragraph::new(Line::from(Span::styled(&t.text, style))),
                area,
            );
            return;
        }
        let (hint, show_globals) = match &self.editor {
            Some((ed, _)) => (ed.hint(), false),
            None => match self.screen {
                Screen::Themes => (self.themes.hint(), true),
                _ => ("tab/1-0 switch".into(), true),
            },
        };
        let mut spans = vec![Span::styled(hint, self.skin.dim())];
        if show_globals {
            spans.push(Span::styled(
                "   / search · ? help · q quit",
                self.skin.border(),
            ));
        }
        f.render_widget(Paragraph::new(Line::from(spans)), area);
    }

    fn draw_help(&self, f: &mut Frame) {
        let area = centered(f.area(), 52, 14);
        f.render_widget(Clear, area);
        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(self.skin.accent_bold())
            .title(" keys ");
        let inner = block.inner(area);
        f.render_widget(block, area);
        let rows = [
            ("1–9, 0", "jump to a module"),
            ("tab / shift-tab", "cycle modules"),
            ("/", "command palette (search everything)"),
            ("j / k", "move selection"),
            ("enter", "apply / activate"),
            ("f", "fork the selected theme"),
            ("esc", "back to Themes, or quit"),
            ("q", "quit"),
            ("?", "this help"),
        ];
        let lines: Vec<Line> = rows
            .iter()
            .map(|(k, d)| {
                Line::from(vec![
                    Span::styled(format!("  {k:<16}"), self.skin.accent_bold()),
                    Span::styled(*d, self.skin.body()),
                ])
            })
            .collect();
        f.render_widget(Paragraph::new(lines), inner);
    }
}

// ── command palette (spec 07 §3) ─────────────────────────────────────────────

enum Choice {
    Go(Screen),
    Theme(String),
}

struct Entry {
    label: String,
    haystack: String,
    choice: Choice,
}

struct CommandPalette {
    entries: Vec<Entry>,
    query: String,
    selected: usize,
}

impl CommandPalette {
    fn new(paths: &OmarchyPaths) -> Self {
        let mut entries: Vec<Entry> = Screen::ALL
            .iter()
            .map(|s| Entry {
                label: format!("Go: {}", s.label()),
                haystack: s.label().to_lowercase(),
                choice: Choice::Go(*s),
            })
            .collect();
        for t in ThemeStore::from_paths(paths).list() {
            entries.push(Entry {
                label: format!("Theme: {}", t.pretty),
                haystack: format!("theme {} {}", t.pretty.to_lowercase(), t.slug),
                choice: Choice::Theme(t.slug),
            });
        }
        Self {
            entries,
            query: String::new(),
            selected: 0,
        }
    }

    fn matches(&self) -> Vec<&Entry> {
        let q = self.query.to_lowercase();
        self.entries
            .iter()
            .filter(|e| subsequence(&q, &e.haystack))
            .collect()
    }

    fn selected(&self) -> Option<Choice> {
        self.matches().get(self.selected).map(|e| match &e.choice {
            Choice::Go(s) => Choice::Go(*s),
            Choice::Theme(t) => Choice::Theme(t.clone()),
        })
    }

    fn up(&mut self) {
        self.selected = self.selected.saturating_sub(1);
    }
    fn down(&mut self) {
        let n = self.matches().len();
        if n > 0 {
            self.selected = (self.selected + 1).min(n - 1);
        }
    }
    fn push(&mut self, c: char) {
        self.query.push(c);
        self.selected = 0;
    }
    fn backspace(&mut self) {
        self.query.pop();
        self.selected = 0;
    }

    fn render(&self, f: &mut Frame, skin: &Skin) {
        let area = centered(f.area(), 56, 16);
        f.render_widget(Clear, area);
        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(skin.accent_bold())
            .title(" search — type, ↑↓ to move, enter ");
        let inner = block.inner(area);
        f.render_widget(block, area);

        let rows = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(1), Constraint::Min(1)])
            .split(inner);
        f.render_widget(
            Paragraph::new(format!("› {}▏", self.query)).style(skin.body()),
            rows[0],
        );
        let items: Vec<ListItem> = self
            .matches()
            .iter()
            .enumerate()
            .map(|(i, e)| {
                let style = if i == self.selected {
                    skin.selection()
                } else {
                    skin.body()
                };
                ListItem::new(Span::styled(e.label.clone(), style))
            })
            .collect();
        f.render_widget(List::new(items), rows[1]);
    }
}

/// One-line rendering of a core error for a toast (no debug spew).
fn brief(e: studio_core::StudioError) -> String {
    use studio_core::StudioError::*;
    match e {
        External { detail, .. } => detail,
        ParseFailed { hint, .. } => hint,
        VerifyFailed { step, .. } => format!("{step} failed"),
        MissingDependency(r) => format!("{} not available", r.id),
        OmarchyMismatch { action, .. } => action,
        Io(err) => err.to_string(),
    }
}

/// Fuzzy subsequence: every char of `needle` appears in order within `hay`.
fn subsequence(needle: &str, hay: &str) -> bool {
    let mut chars = hay.chars();
    needle.chars().all(|c| chars.any(|h| h == c))
}

fn centered(area: Rect, w: u16, h: u16) -> Rect {
    let w = w.min(area.width.saturating_sub(2));
    let h = h.min(area.height.saturating_sub(2));
    Rect {
        x: area.x + (area.width.saturating_sub(w)) / 2,
        y: area.y + (area.height.saturating_sub(h)) / 2,
        width: w,
        height: h,
    }
}

/// Entry point: no-args launch (spec 08). Runs until quit; restores the
/// terminal on any exit path.
pub fn run() -> i32 {
    let paths = match OmarchyPaths::discover() {
        Ok(p) if p.installed() => p,
        Ok(p) => {
            eprintln!(
                "no Omarchy install at {} (set $OMARCHY_PATH if it lives elsewhere).",
                p.system.display()
            );
            return 4;
        }
        Err(_) => {
            eprintln!("HOME is not set — cannot locate an Omarchy install.");
            return 4;
        }
    };

    let mut terminal = ratatui::init();
    // Enable Kitty keyboard flags where supported, so chord capture can see
    // SUPER and disambiguate modifiers. No-op on terminals that lack it.
    let kbd_enhanced = matches!(
        ratatui::crossterm::terminal::supports_keyboard_enhancement(),
        Ok(true)
    );
    if kbd_enhanced {
        let _ = ratatui::crossterm::execute!(
            std::io::stdout(),
            ratatui::crossterm::event::PushKeyboardEnhancementFlags(chord::enhancement_flags())
        );
    }
    let mut app = App::new(paths);
    let result = loop {
        if let Err(e) = terminal.draw(|f| app.draw(f)) {
            break Err(e);
        }
        match event::read() {
            Ok(Event::Key(key)) if key.kind == KeyEventKind::Press => {
                // Ctrl-C always exits, even mid text-entry.
                if key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL) {
                    break Ok(());
                }
                app.on_key(key);
                if app.quit {
                    break Ok(());
                }
            }
            Ok(_) => {}
            Err(e) => break Err(e),
        }
    };
    if kbd_enhanced {
        let _ = ratatui::crossterm::execute!(
            std::io::stdout(),
            ratatui::crossterm::event::PopKeyboardEnhancementFlags
        );
    }
    ratatui::restore();
    match result {
        Ok(()) => 0,
        Err(e) => {
            eprintln!("terminal error: {e}");
            1
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fuzzy_subsequence_matches_in_order() {
        assert!(subsequence("tn", "tokyo night"));
        assert!(subsequence("cat", "catppuccin latte"));
        assert!(subsequence("", "anything"));
        assert!(subsequence("lumon", "theme lumon lumon"));
        // out of order or absent chars fail
        assert!(!subsequence("nt", "tn"));
        assert!(!subsequence("xyz", "tokyo night"));
    }

    #[test]
    fn screen_cycle_wraps_both_directions() {
        // ALL is the canonical rail order; first is Themes, last is Doctor.
        assert_eq!(Screen::ALL[0], Screen::Themes);
        assert_eq!(*Screen::ALL.last().unwrap(), Screen::Doctor);
        // every screen has a non-empty label and arriving milestone
        for s in Screen::ALL {
            assert!(!s.label().is_empty());
            assert!(!s.arriving().is_empty());
        }
    }
}
