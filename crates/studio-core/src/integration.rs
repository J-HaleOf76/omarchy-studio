//! Omarchy menu integration (spec 02 §3, roadmap 0.1.12).
//!
//! Studio adds itself to the Omarchy menu (Super+Alt+Space → Style → Studio)
//! by managing a single marked block in `~/.config/omarchy/extensions/menu.sh`,
//! the file `omarchy-menu` sources before dispatch. The block overrides
//! `show_style_menu` to prepend a Studio entry; install/uninstall are exact
//! and idempotent, and never touch anything outside the markers.
//!
//! The launcher is `omarchy-launch-floating-terminal-with-presentation`, which
//! runs under the `org.omarchy.terminal` app-id — already matched by Omarchy's
//! stock float rule, so no window rule needs installing (spec 02 §3.3).

use std::path::PathBuf;

use crate::configfs::{atomic_write, CommentStyle, ManagedBlock};
use crate::error::Result;
use crate::omarchy::OmarchyPaths;

const SECTION: &str = "menu";

fn block() -> ManagedBlock {
    ManagedBlock::new(SECTION, CommentStyle::Hash)
}

fn menu_path(paths: &OmarchyPaths) -> PathBuf {
    paths.config.join("extensions/menu.sh")
}

/// The `show_style_menu` override. Mirrors Omarchy 3.8's Style menu with a
/// Studio entry prepended. Because it fully redefines the function, new stock
/// Style entries added by a future Omarchy won't appear until the user re-runs
/// `install-integration` — the documented tradeoff of menu overrides.
fn menu_body() -> String {
    r#"# Adds "Studio" to Omarchy's Style menu (Super+Alt+Space -> Style).
show_style_menu() {
  case $(menu "Style" "󰏘  Studio\n󰸌  Theme\n󰟵  Unlock\n  Font\n  Background\n  Hyprland\n󱄄  Screensaver\n  About") in
  *Studio*) omarchy-launch-floating-terminal-with-presentation omarchy-studio ;;
  *Theme*) show_theme_menu ;;
  *Unlock*) omarchy-launch-walker -m menus:omarchyunlocks --width 800 --minheight 400 ;;
  *Font*) show_font_menu ;;
  *Background*) show_background_menu ;;
  *Hyprland*) open_in_editor ~/.config/hypr/looknfeel.conf ;;
  *Screensaver*) show_screensaver_menu ;;
  *About*) show_about_menu ;;
  *) show_main_menu ;;
  esac
}"#
    .to_string()
}

/// Is the Studio menu block currently installed?
pub fn is_installed(paths: &OmarchyPaths) -> bool {
    std::fs::read_to_string(menu_path(paths))
        .map(|c| block().contains(&c))
        .unwrap_or(false)
}

/// The extensions/menu.sh path (for snapshotting before a change).
pub fn managed_file(paths: &OmarchyPaths) -> PathBuf {
    menu_path(paths)
}

/// Install (or refresh) the Studio menu entry. Idempotent. Returns the file
/// that was written.
pub fn install_menu(paths: &OmarchyPaths) -> Result<PathBuf> {
    let path = menu_path(paths);
    let existing = std::fs::read_to_string(&path).unwrap_or_default();
    let updated = block().upsert(&existing, &menu_body());
    atomic_write(&path, &updated)?;
    Ok(path)
}

/// Remove the Studio menu entry. Leaves the rest of menu.sh intact; if the file
/// becomes empty (Studio was its only content) it is removed entirely. Returns
/// the file path if it existed.
pub fn uninstall_menu(paths: &OmarchyPaths) -> Result<Option<PathBuf>> {
    let path = menu_path(paths);
    let Ok(existing) = std::fs::read_to_string(&path) else {
        return Ok(None);
    };
    let updated = block().remove(&existing);
    if updated.trim().is_empty() {
        let _ = std::fs::remove_file(&path);
    } else {
        atomic_write(&path, &updated)?;
    }
    Ok(Some(path))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fake_paths(tag: &str) -> OmarchyPaths {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .subsec_nanos();
        let root = std::env::temp_dir().join(format!(
            "omarchy-studio-integ-{tag}-{}-{nanos}",
            std::process::id()
        ));
        std::fs::create_dir_all(root.join("config/omarchy/extensions")).unwrap();
        OmarchyPaths {
            system: root.join("share/omarchy"),
            config: root.join("config/omarchy"),
            state: root.join("state/omarchy"),
        }
    }

    #[test]
    fn install_is_idempotent_and_uninstall_is_exact() {
        let paths = fake_paths("menu");
        assert!(!is_installed(&paths));

        install_menu(&paths).unwrap();
        assert!(is_installed(&paths));
        let after_first = std::fs::read_to_string(menu_path(&paths)).unwrap();

        // second install must not duplicate or drift
        install_menu(&paths).unwrap();
        let after_second = std::fs::read_to_string(menu_path(&paths)).unwrap();
        assert_eq!(after_first, after_second);
        assert_eq!(after_second.matches("show_style_menu()").count(), 1);

        // uninstall removes the (Studio-only) file entirely
        uninstall_menu(&paths).unwrap();
        assert!(!is_installed(&paths));
        assert!(!menu_path(&paths).exists());
    }

    #[test]
    fn preserves_a_users_existing_menu_overrides() {
        let paths = fake_paths("coexist");
        let user = "show_system_menu() {\n  omarchy-system-lock\n}\n";
        std::fs::write(menu_path(&paths), user).unwrap();

        install_menu(&paths).unwrap();
        let both = std::fs::read_to_string(menu_path(&paths)).unwrap();
        assert!(both.contains("show_system_menu()"));
        assert!(both.contains("show_style_menu()"));

        uninstall_menu(&paths).unwrap();
        let back = std::fs::read_to_string(menu_path(&paths)).unwrap();
        assert_eq!(back, user, "user's own override must survive uninstall");
    }
}
