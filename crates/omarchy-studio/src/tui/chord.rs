//! Chord capture (spec 05 §4, roadmap 0.2.5).
//!
//! Translates a live terminal key event into a Hyprland chord `(modmask, key)`,
//! so the keybinds screen can offer "press the shortcut you want" instead of
//! making the user type modifier names. Capturing SUPER and distinguishing
//! left/right modifiers needs the terminal's Kitty keyboard protocol; we push
//! those enhancement flags while the capture overlay is open (see
//! [`enhancement_flags`]). Without them the mapping still works for CTRL/ALT/
//! SHIFT chords — SUPER just won't arrive, which we surface to the user.

use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use studio_core::modules::keybinds::mods;

/// The keyboard-enhancement flags we need for full chord capture. Pushed while
/// the TUI runs (where the terminal supports it), popped on exit.
pub fn enhancement_flags() -> ratatui::crossterm::event::KeyboardEnhancementFlags {
    use ratatui::crossterm::event::KeyboardEnhancementFlags as F;
    // Report all keys (incl. modifiers as events) and disambiguate them, so a
    // bare SUPER press is visible and SUPER-as-a-modifier is distinguishable.
    // REPORT_ALTERNATE_KEYS matters for ordinary chords too: without it the
    // kitty protocol delivers shift+a as `Char('a')`+SHIFT, and every
    // uppercase binding (`A`, `G`, waybar's `J/K/H/L`, …) goes dead in
    // kitty/ghostty/alacritty while still working in tmux/xterm.
    F::DISAMBIGUATE_ESCAPE_CODES | F::REPORT_ALL_KEYS_AS_ESCAPE_CODES | F::REPORT_ALTERNATE_KEYS
}

/// Undo the kitty protocol's lowercase-plus-SHIFT reporting so screens can
/// keep matching `Char('A')` & co. Also what a text field wants: shift+a
/// must insert `A`. A no-op for events that already carry the shifted char
/// (legacy encoding, or REPORT_ALTERNATE_KEYS honored by the terminal).
pub fn normalize(mut ev: KeyEvent) -> KeyEvent {
    if ev.modifiers.contains(KeyModifiers::SHIFT) {
        if let KeyCode::Char(c) = ev.code {
            if c.is_ascii_lowercase() {
                ev.code = KeyCode::Char(c.to_ascii_uppercase());
            }
        }
    }
    ev
}

/// Convert a key event into a Hyprland chord `(modmask, key_name)`.
///
/// Returns `None` for a press that is *only* a modifier (Shift/Ctrl/Alt/Super
/// with no base key) — the caller keeps waiting for the base key. Also `None`
/// for keys that can't anchor a bind (e.g. a lone modifier via `KeyCode::Modifier`).
pub fn capture(ev: &KeyEvent) -> Option<(u16, String)> {
    let key = key_name(ev.code)?;
    let mut mask = 0u16;
    let m = ev.modifiers;
    if m.contains(KeyModifiers::SUPER) {
        mask |= mods::SUPER;
    }
    if m.contains(KeyModifiers::CONTROL) {
        mask |= mods::CTRL;
    }
    if m.contains(KeyModifiers::ALT) {
        mask |= mods::ALT;
    }
    if m.contains(KeyModifiers::SHIFT) {
        mask |= mods::SHIFT;
    }
    Some((mask, key))
}

/// Hyprland's name for a key code, or `None` if it isn't bindable on its own.
fn key_name(code: KeyCode) -> Option<String> {
    let name = match code {
        // Letters bind by their uppercase name; other printables pass through,
        // with the couple of names Hyprland spells out.
        KeyCode::Char(' ') => "space".to_string(),
        KeyCode::Char(c) if c.is_ascii_alphabetic() => c.to_ascii_uppercase().to_string(),
        KeyCode::Char(c) => c.to_string(),
        KeyCode::Enter => "Return".to_string(),
        KeyCode::Tab | KeyCode::BackTab => "Tab".to_string(),
        KeyCode::Backspace => "BackSpace".to_string(),
        KeyCode::Esc => "Escape".to_string(),
        KeyCode::Delete => "Delete".to_string(),
        KeyCode::Insert => "Insert".to_string(),
        KeyCode::Home => "Home".to_string(),
        KeyCode::End => "End".to_string(),
        KeyCode::PageUp => "Prior".to_string(),
        KeyCode::PageDown => "Next".to_string(),
        KeyCode::Left => "left".to_string(),
        KeyCode::Right => "right".to_string(),
        KeyCode::Up => "up".to_string(),
        KeyCode::Down => "down".to_string(),
        KeyCode::F(n) => format!("F{n}"),
        // Bare modifiers / everything else: not a bindable anchor.
        _ => return None,
    };
    Some(name)
}

#[cfg(test)]
mod tests {
    use super::*;
    use studio_core::modules::keybinds::render_chord;

    fn ev(code: KeyCode, mods: KeyModifiers) -> KeyEvent {
        KeyEvent::new(code, mods)
    }

    #[test]
    fn captures_super_letter_chords() {
        let (mask, key) = capture(&ev(KeyCode::Char('t'), KeyModifiers::SUPER)).unwrap();
        assert_eq!(render_chord(mask, &key), "SUPER+T");

        let (mask, key) = capture(&ev(
            KeyCode::Char('q'),
            KeyModifiers::SUPER | KeyModifiers::SHIFT,
        ))
        .unwrap();
        assert_eq!(render_chord(mask, &key), "SUPER+SHIFT+Q");
    }

    #[test]
    fn maps_named_keys_to_hyprland_names() {
        let cases = [
            (KeyCode::Enter, "Return"),
            (KeyCode::Left, "left"),
            (KeyCode::F(5), "F5"),
            (KeyCode::Char(' '), "space"),
            (KeyCode::Esc, "Escape"),
            (KeyCode::PageUp, "Prior"),
        ];
        for (code, want) in cases {
            let (_, key) = capture(&ev(code, KeyModifiers::NONE)).unwrap();
            assert_eq!(key, want);
        }
    }

    #[test]
    fn normalize_recovers_uppercase_from_kitty_reporting() {
        // kitty protocol without alternate keys: shift+a arrives lowercase
        let n = normalize(ev(KeyCode::Char('a'), KeyModifiers::SHIFT));
        assert_eq!(n.code, KeyCode::Char('A'));
        assert!(n.modifiers.contains(KeyModifiers::SHIFT));
        // already-shifted (legacy encoding) and unshifted events pass through
        assert_eq!(
            normalize(ev(KeyCode::Char('A'), KeyModifiers::SHIFT)).code,
            KeyCode::Char('A')
        );
        assert_eq!(
            normalize(ev(KeyCode::Char('a'), KeyModifiers::NONE)).code,
            KeyCode::Char('a')
        );
        // shifted punctuation is the terminal's business — untouched
        assert_eq!(
            normalize(ev(KeyCode::Char('/'), KeyModifiers::SHIFT)).code,
            KeyCode::Char('/')
        );
    }

    #[test]
    fn ctrl_alt_chords_work_without_super() {
        let (mask, key) = capture(&ev(
            KeyCode::Char('c'),
            KeyModifiers::CONTROL | KeyModifiers::ALT,
        ))
        .unwrap();
        assert_eq!(render_chord(mask, &key), "CTRL+ALT+C");
    }

    #[test]
    fn bare_modifier_is_not_a_chord() {
        // KeyCode::Modifier isn't bindable alone; capture waits for a base key.
        assert!(capture(&ev(KeyCode::Null, KeyModifiers::SUPER)).is_none());
    }
}
