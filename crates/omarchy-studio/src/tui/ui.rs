//! Shared TUI primitives used by the shell and every screen.
//!
//! These collapse three shapes that were copy-pasted into nearly every screen:
//! bounded list-cursor movement, clamping a cursor after its list changes, and
//! centering a modal rect inside an area.

use ratatui::crossterm::event::KeyCode;
use ratatui::layout::Rect;

/// Move a list cursor in response to a standard navigation key.
///
/// Handles ↑ / ↓ / Home / End over `index`, bounded by `len` items. Returns
/// `true` when `key` was a navigation key (so the caller treats the event as
/// consumed), `false` otherwise so the caller can match its own keys. Safe on
/// an empty list: every branch clamps to `0`.
pub fn list_nav(key: KeyCode, index: &mut usize, len: usize) -> bool {
    match key {
        KeyCode::Up => *index = index.saturating_sub(1),
        KeyCode::Down => *index = (*index + 1).min(len.saturating_sub(1)),
        KeyCode::Home => *index = 0,
        KeyCode::End => *index = len.saturating_sub(1),
        _ => return false,
    }
    true
}

/// Clamp a cursor into `len` items — call after the underlying list changes so
/// a previously-selected index can't dangle past the new end.
pub fn clamp_index(index: usize, len: usize) -> usize {
    index.min(len.saturating_sub(1))
}

/// A `w`×`h` rect centered inside `area`, clamped to fit with a 1-cell margin.
pub fn centered_rect(area: Rect, w: u16, h: u16) -> Rect {
    let w = w.min(area.width.saturating_sub(2));
    let h = h.min(area.height.saturating_sub(2));
    Rect {
        x: area.x + area.width.saturating_sub(w) / 2,
        y: area.y + area.height.saturating_sub(h) / 2,
        width: w,
        height: h,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn list_nav_moves_and_clamps_at_ends() {
        let mut i = 0;
        assert!(list_nav(KeyCode::Down, &mut i, 3));
        assert_eq!(i, 1);
        assert!(list_nav(KeyCode::Down, &mut i, 3));
        assert_eq!(i, 2);
        // already at the end — stays put
        assert!(list_nav(KeyCode::Down, &mut i, 3));
        assert_eq!(i, 2);
        assert!(list_nav(KeyCode::Home, &mut i, 3));
        assert_eq!(i, 0);
        // already at the top — saturates
        assert!(list_nav(KeyCode::Up, &mut i, 3));
        assert_eq!(i, 0);
        assert!(list_nav(KeyCode::End, &mut i, 3));
        assert_eq!(i, 2);
    }

    #[test]
    fn list_nav_is_safe_on_empty_list() {
        let mut i = 0;
        assert!(list_nav(KeyCode::Down, &mut i, 0));
        assert_eq!(i, 0);
        assert!(list_nav(KeyCode::End, &mut i, 0));
        assert_eq!(i, 0);
    }

    #[test]
    fn list_nav_ignores_non_nav_keys() {
        let mut i = 1;
        assert!(!list_nav(KeyCode::Char('x'), &mut i, 3));
        assert!(!list_nav(KeyCode::Left, &mut i, 3));
        assert_eq!(i, 1);
    }

    #[test]
    fn clamp_index_pulls_into_range() {
        assert_eq!(clamp_index(9, 3), 2);
        assert_eq!(clamp_index(1, 3), 1);
        assert_eq!(clamp_index(5, 0), 0);
    }

    #[test]
    fn centered_rect_centers_and_clamps_to_area() {
        let area = Rect {
            x: 0,
            y: 0,
            width: 100,
            height: 40,
        };
        let r = centered_rect(area, 40, 10);
        assert_eq!((r.x, r.y, r.width, r.height), (30, 15, 40, 10));
        // oversized request clamps to a 1-cell margin
        let big = centered_rect(area, 200, 200);
        assert_eq!((big.width, big.height), (98, 38));
    }
}
