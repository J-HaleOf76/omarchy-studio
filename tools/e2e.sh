#!/usr/bin/env bash
# Drive the real TUI in a tmux pty against a fixture Omarchy (spec 09 §4).
#
# Every screen is unit-tested; nothing caught "the TUI doesn't start" or "the
# rail stopped moving", because those only break when the whole binary runs.
# This launches it for real and reads the pane back.
#
# Runs entirely inside a scratch HOME + OMARCHY_PATH, so it touches nothing
# of yours and needs no Omarchy, Hyprland or Wayland — CI has none of them.
#
#   tools/e2e.sh [path-to-binary]
set -uo pipefail

BIN="${1:-./target/release/omarchy-studio}"
[ -x "$BIN" ] || { echo "no binary at $BIN — cargo build --release first" >&2; exit 1; }
BIN=$(realpath "$BIN")
command -v tmux >/dev/null || { echo "tmux not installed" >&2; exit 1; }

ROOT=$(mktemp -d)
SESSION="studio-e2e-$$"
trap 'tmux kill-session -t "$SESSION" 2>/dev/null; rm -rf "$ROOT"' EXIT

# ── fixture: the least Omarchy that `installed()` and the Themes screen accept
OM="$ROOT/omarchy"
HOME_DIR="$ROOT/home"
mkdir -p "$OM/themes/nord" "$HOME_DIR/.config/omarchy/current"
echo "3.8.9" > "$OM/version"
cat > "$OM/themes/nord/colors.toml" <<'TOML'
background = "#2e3440"
foreground = "#d8dee9"
accent = "#88c0d0"
TOML
mkdir -p "$OM/themes/gruvbox"
cat > "$OM/themes/gruvbox/colors.toml" <<'TOML'
background = "#282828"
foreground = "#ebdbb2"
accent = "#d79921"
TOML
echo "nord" > "$HOME_DIR/.config/omarchy/current/theme.name"
ln -sfn "$OM/themes/nord" "$HOME_DIR/.config/omarchy/current/theme"

fails=0
pane() { tmux capture-pane -t "$SESSION" -p; }

# Anchor on things that are only true when the step worked. The rail lists
# every module name at all times, so "Wallpaper appears" proves nothing —
# "the selection marker is on Wallpaper" does.
check() { # check <description> <extended-regex>
    if pane | grep -qiE -- "$2"; then
        echo "  ok    $1"
    else
        echo "  FAIL  $1  (no /$2/ in the pane)"
        echo "  ---- pane ----"; pane | sed 's/^/  | /'; echo "  --------------"
        fails=$((fails + 1))
    fi
}

check_gone() { # check_gone <description> <extended-regex>
    if pane | grep -qiE -- "$2"; then
        echo "  FAIL  $1  (/$2/ is still on screen)"
        echo "  ---- pane ----"; pane | sed 's/^/  | /'; echo "  --------------"
        fails=$((fails + 1))
    else
        echo "  ok    $1"
    fi
}

echo "==> launching the TUI in a pty"
tmux new-session -d -s "$SESSION" -x 110 -y 32 \
    "env HOME='$HOME_DIR' OMARCHY_PATH='$OM' \
         XDG_STATE_HOME='$HOME_DIR/.local/state' \
         XDG_CONFIG_HOME='$HOME_DIR/.config' \
         XDG_CACHE_HOME='$HOME_DIR/.cache' '$BIN'"
sleep 3

# It starts at all, draws its chrome, and reads the fixture — not a built-in
# default: `Nord` exists only because the fixture created it.
check "starts and draws the rail" '✦ Studio'
check "selection starts on Themes" '▌ Themes'
check "reads themes out of the fixture" 'Nord|Gruvbox'
check "shows the fixture's current theme" 'nord'
check "keybar offers help and quit" 'q quit'

# The rail moves and the panel title follows — the "TUI froze" canary.
tmux send-keys -t "$SESSION" Down; sleep 0.8
check "down moves the selection to Wallpaper" '▌ Wallpaper'
check "the panel title follows the rail" '╮ Wallpaper'

# Help and the palette: the two overlays a lost user reaches for.
tmux send-keys -t "$SESSION" '?'; sleep 0.8
# The keybar always reads "? help", so match the overlay's own border —
# case-insensitively, '\? Help' matches the keybar and proves nothing.
check "? opens the help overlay" '╭ \? Help'
# A lone ESC is held back while the terminal decides it isn't the start of
# an escape sequence, so give it longer than an ordinary key.
tmux send-keys -t "$SESSION" Escape; sleep 1.5
check_gone "esc closes help" '╭ \? Help'

tmux send-keys -t "$SESSION" '/'; sleep 0.8
check "/ opens the search palette" '╭ / Search'
check "the palette offers a destination" 'Go: '
tmux send-keys -t "$SESSION" Escape; sleep 1.5

# And it exits cleanly, restoring the terminal rather than wedging the pty.
echo "==> quitting"
tmux send-keys -t "$SESSION" 'q'; sleep 1.5
if tmux has-session -t "$SESSION" 2>/dev/null; then
    # The pane may survive as a dead shell; only a live TUI is a failure.
    if pane | grep -q '✦ Studio'; then
        echo "  FAIL  q did not quit"
        fails=$((fails + 1))
    else
        echo "  ok    quits on q"
    fi
else
    echo "  ok    quits on q"
fi

echo
if [ "$fails" -gt 0 ]; then
    echo "e2e: $fails check(s) failed"
    exit 1
fi
echo "e2e: all checks passed"
