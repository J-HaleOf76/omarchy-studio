//! Hyprland dispatcher schema (spec 05 §2).
//!
//! The bridge between Hyprland's dispatcher ids (`togglefloating`) and the
//! plain-language the UI shows ("Toggle floating"). Every dispatcher the
//! keybinds screen offers is described here: a human label, a category to
//! group it under, what kind of argument it takes, and a one-line summary.
//! Anything not in this table still round-trips — it's shown by its raw id so
//! the user is never blocked, just less guided.

/// What a dispatcher's argument means, so the editor can prompt appropriately.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArgKind {
    /// No argument (`killactive`, `fullscreen`).
    None,
    /// A shell command (`exec`).
    Command,
    /// A workspace: number, name, or relative (`+1`, `e+1`, `special:magic`).
    Workspace,
    /// A direction: l/r/u/d.
    Direction,
    /// A window reference (class/title/address) or none for "current".
    Window,
    /// Freeform text passed through (`layoutmsg`, `sendshortcut`).
    Text,
}

/// A grouping for the keybinds UI.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Category {
    Session,
    Apps,
    Window,
    Focus,
    Workspace,
    Group,
    Layout,
    Media,
    Other,
}

impl Category {
    pub fn label(self) -> &'static str {
        match self {
            Category::Session => "Session",
            Category::Apps => "Apps & commands",
            Category::Window => "Window",
            Category::Focus => "Focus & move",
            Category::Workspace => "Workspaces",
            Category::Group => "Groups (tabs)",
            Category::Layout => "Layout",
            Category::Media => "Media & hardware",
            Category::Other => "Other",
        }
    }
}

/// One dispatcher's human-facing description.
#[derive(Debug, Clone, Copy)]
pub struct Dispatcher {
    pub id: &'static str,
    pub label: &'static str,
    pub category: Category,
    pub arg: ArgKind,
    pub summary: &'static str,
}

/// The known-dispatcher table. Covers everything Omarchy's default binds use,
/// plus the common remainder from the Hyprland wiki.
pub const DISPATCHERS: &[Dispatcher] = &[
    // Session
    d(
        "exit",
        "Quit Hyprland",
        Category::Session,
        ArgKind::None,
        "Log out of the session",
    ),
    d(
        "forcerendererreload",
        "Reload renderer",
        Category::Session,
        ArgKind::None,
        "Re-read monitors & renderer",
    ),
    // Apps & commands
    d(
        "exec",
        "Run a command",
        Category::Apps,
        ArgKind::Command,
        "Launch an app or shell command",
    ),
    d(
        "execr",
        "Run (raw)",
        Category::Apps,
        ArgKind::Command,
        "Run a command without shell rules",
    ),
    d(
        "sendshortcut",
        "Send a shortcut",
        Category::Apps,
        ArgKind::Text,
        "Forward a keystroke to a window",
    ),
    // Window
    d(
        "killactive",
        "Close window",
        Category::Window,
        ArgKind::None,
        "Close the focused window",
    ),
    d(
        "togglefloating",
        "Toggle floating",
        Category::Window,
        ArgKind::None,
        "Float or re-tile the window",
    ),
    d(
        "setfloating",
        "Float window",
        Category::Window,
        ArgKind::Window,
        "Make the window float",
    ),
    d(
        "settiled",
        "Tile window",
        Category::Window,
        ArgKind::Window,
        "Return the window to tiling",
    ),
    d(
        "fullscreen",
        "Fullscreen",
        Category::Window,
        ArgKind::Text,
        "Toggle fullscreen (0 full, 1 maximize)",
    ),
    d(
        "fullscreenstate",
        "Fullscreen state",
        Category::Window,
        ArgKind::Text,
        "Set internal/client fullscreen state",
    ),
    d(
        "fakefullscreen",
        "Fake fullscreen",
        Category::Window,
        ArgKind::None,
        "Fullscreen without telling the app",
    ),
    d(
        "pseudo",
        "Pseudo-tile",
        Category::Window,
        ArgKind::None,
        "Toggle pseudo-tiling",
    ),
    d(
        "pin",
        "Pin window",
        Category::Window,
        ArgKind::None,
        "Keep a floating window on all workspaces",
    ),
    d(
        "centerwindow",
        "Center window",
        Category::Window,
        ArgKind::None,
        "Center a floating window",
    ),
    d(
        "bringactivetotop",
        "Bring to top",
        Category::Window,
        ArgKind::None,
        "Raise the active floating window",
    ),
    d(
        "resizeactive",
        "Resize window",
        Category::Window,
        ArgKind::Text,
        "Resize the active window by an amount",
    ),
    d(
        "moveactive",
        "Move window",
        Category::Window,
        ArgKind::Text,
        "Move the active floating window",
    ),
    // Focus & move
    d(
        "movefocus",
        "Move focus",
        Category::Focus,
        ArgKind::Direction,
        "Focus the neighbour in a direction",
    ),
    d(
        "movewindow",
        "Move window",
        Category::Focus,
        ArgKind::Direction,
        "Move the window in a direction",
    ),
    d(
        "swapwindow",
        "Swap window",
        Category::Focus,
        ArgKind::Direction,
        "Swap places with the neighbour",
    ),
    d(
        "focuswindow",
        "Focus a window",
        Category::Focus,
        ArgKind::Window,
        "Focus a window by class or title",
    ),
    d(
        "focusmonitor",
        "Focus monitor",
        Category::Focus,
        ArgKind::Text,
        "Focus a specific monitor",
    ),
    d(
        "focusurgentorlast",
        "Focus urgent/last",
        Category::Focus,
        ArgKind::None,
        "Jump to the urgent or last window",
    ),
    d(
        "cyclenext",
        "Cycle windows",
        Category::Focus,
        ArgKind::Text,
        "Focus the next window in the workspace",
    ),
    // Workspaces
    d(
        "workspace",
        "Go to workspace",
        Category::Workspace,
        ArgKind::Workspace,
        "Switch to a workspace",
    ),
    d(
        "movetoworkspace",
        "Move to workspace",
        Category::Workspace,
        ArgKind::Workspace,
        "Send window and follow",
    ),
    d(
        "movetoworkspacesilent",
        "Send to workspace",
        Category::Workspace,
        ArgKind::Workspace,
        "Send window, stay put",
    ),
    d(
        "movecurrentworkspacetomonitor",
        "Workspace to monitor",
        Category::Workspace,
        ArgKind::Text,
        "Move this workspace to a monitor",
    ),
    d(
        "togglespecialworkspace",
        "Toggle scratchpad",
        Category::Workspace,
        ArgKind::Text,
        "Show/hide a special workspace",
    ),
    // Groups (tabs)
    d(
        "togglegroup",
        "Toggle group",
        Category::Group,
        ArgKind::None,
        "Group windows into a tab bar",
    ),
    d(
        "changegroupactive",
        "Next tab in group",
        Category::Group,
        ArgKind::Text,
        "Cycle the active grouped window",
    ),
    d(
        "moveintogroup",
        "Move into group",
        Category::Group,
        ArgKind::Direction,
        "Add the neighbour to a group",
    ),
    d(
        "moveoutofgroup",
        "Move out of group",
        Category::Group,
        ArgKind::None,
        "Remove the window from its group",
    ),
    d(
        "lockactivegroup",
        "Lock group",
        Category::Group,
        ArgKind::Text,
        "Lock/unlock the active group",
    ),
    // Layout
    d(
        "layoutmsg",
        "Layout command",
        Category::Layout,
        ArgKind::Text,
        "Send a message to the active layout",
    ),
    d(
        "togglesplit",
        "Toggle split",
        Category::Layout,
        ArgKind::None,
        "Flip the dwindle split direction",
    ),
    d(
        "swapsplit",
        "Swap split",
        Category::Layout,
        ArgKind::None,
        "Swap the two sides of a split",
    ),
    d(
        "pseudoresize",
        "Pseudo resize",
        Category::Layout,
        ArgKind::Text,
        "Resize within pseudo-tiling",
    ),
    // Media & hardware
    d(
        "pass",
        "Pass key to window",
        Category::Media,
        ArgKind::Window,
        "Forward the key to a specific app",
    ),
    d(
        "mouse",
        "Mouse action",
        Category::Media,
        ArgKind::Text,
        "Bind a mouse-drag action",
    ),
];

const fn d(
    id: &'static str,
    label: &'static str,
    category: Category,
    arg: ArgKind,
    summary: &'static str,
) -> Dispatcher {
    Dispatcher {
        id,
        label,
        category,
        arg,
        summary,
    }
}

/// Look up a dispatcher by its Hyprland id.
pub fn lookup(id: &str) -> Option<&'static Dispatcher> {
    DISPATCHERS.iter().find(|d| d.id == id)
}

/// The human label for a dispatcher id, falling back to the raw id so unknown
/// dispatchers are still shown (never hidden, never blocking).
pub fn label_for(id: &str) -> &str {
    lookup(id).map(|d| d.label).unwrap_or(id)
}

/// Dispatchers in a category, in table order (UI grouping).
pub fn in_category(cat: Category) -> impl Iterator<Item = &'static Dispatcher> {
    DISPATCHERS.iter().filter(move |d| d.category == cat)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn table_is_unique_and_covers_omarchy_defaults() {
        // no duplicate ids
        let mut ids: Vec<_> = DISPATCHERS.iter().map(|d| d.id).collect();
        let n = ids.len();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), n, "duplicate dispatcher id in table");

        // every dispatcher Omarchy's default binds actually use is described
        for used in [
            "exec",
            "workspace",
            "movetoworkspace",
            "movetoworkspacesilent",
            "movefocus",
            "swapwindow",
            "resizeactive",
            "moveintogroup",
            "changegroupactive",
            "fullscreen",
            "killactive",
            "togglefloating",
            "togglegroup",
            "moveoutofgroup",
            "togglespecialworkspace",
            "movecurrentworkspacetomonitor",
            "focusmonitor",
            "layoutmsg",
            "pseudo",
        ] {
            assert!(lookup(used).is_some(), "missing dispatcher: {used}");
        }
    }

    #[test]
    fn labels_and_arg_kinds() {
        assert_eq!(label_for("togglefloating"), "Toggle floating");
        assert_eq!(label_for("exec"), "Run a command");
        assert_eq!(lookup("exec").unwrap().arg, ArgKind::Command);
        assert_eq!(lookup("movefocus").unwrap().arg, ArgKind::Direction);
        assert_eq!(lookup("killactive").unwrap().arg, ArgKind::None);
        // unknown dispatcher falls back to its raw id
        assert_eq!(label_for("someplugindispatcher"), "someplugindispatcher");
        assert!(lookup("someplugindispatcher").is_none());
    }

    #[test]
    fn categories_partition_the_table() {
        // every entry is reachable via its category
        use Category::*;
        let by_cat: usize = [
            Session, Apps, Window, Focus, Workspace, Group, Layout, Media, Other,
        ]
        .iter()
        .map(|c| in_category(*c).count())
        .sum();
        assert_eq!(by_cat, DISPATCHERS.len());
    }
}
