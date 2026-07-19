#!/usr/bin/env bash
# The least Omarchy that `installed()` and the Themes screen accept.
#
# Sourced by tools/e2e.sh and tools/screenshot-grid.sh so both drive the same
# desktop: if the screenshots and the journeys ran against different fixtures,
# the grid would stop showing what the journeys actually assert.
#
#   source tools/fixture.sh
#   studio_fixture "$ROOT"          # sets OM and HOME_DIR
#   env $(studio_fixture_env) "$BIN"
#
# Everything lives under the caller's scratch root, so this touches nothing of
# yours and needs no Omarchy, Hyprland or Wayland — CI has none of them.

# studio_fixture <root> — populate a fixture Omarchy; sets $OM and $HOME_DIR.
studio_fixture() {
    local root=$1
    OM="$root/omarchy"
    HOME_DIR="$root/home"

    mkdir -p "$OM/themes/nord" "$OM/themes/gruvbox" "$HOME_DIR/.config/omarchy/current"
    echo "3.8.9" > "$OM/version"

    cat > "$OM/themes/nord/colors.toml" <<'TOML'
background = "#2e3440"
foreground = "#d8dee9"
accent = "#88c0d0"
TOML
    cat > "$OM/themes/gruvbox/colors.toml" <<'TOML'
background = "#282828"
foreground = "#ebdbb2"
accent = "#d79921"
TOML

    echo "nord" > "$HOME_DIR/.config/omarchy/current/theme.name"
    ln -sfn "$OM/themes/nord" "$HOME_DIR/.config/omarchy/current/theme"
}

# studio_fixture_env — the env assignments that point Studio at the fixture,
# shell-quoted so they survive being spliced into a tmux command string. Every
# XDG dir is redirected too, so state and config land in the scratch root
# rather than the real ~/.local/state.
studio_fixture_env() {
    printf "HOME='%s' OMARCHY_PATH='%s' XDG_STATE_HOME='%s' XDG_CONFIG_HOME='%s' XDG_CACHE_HOME='%s'" \
        "$HOME_DIR" "$OM" \
        "$HOME_DIR/.local/state" "$HOME_DIR/.config" "$HOME_DIR/.cache"
}
