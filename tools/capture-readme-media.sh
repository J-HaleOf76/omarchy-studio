#!/usr/bin/env bash
# Regenerate the README screenshots + tour GIF.
#
# Drives the release binary in a tmux pane, captures frames with their
# escape sequences, and rasterizes them with tools/termshot. Run from the
# repo root on a machine with an Omarchy config and the JetBrainsMono Nerd
# Font (stock on Omarchy). Only navigation keys are sent — nothing applies.
set -euo pipefail

cd "$(dirname "$0")/.."
BIN=./target/release/omarchy-studio
SESSION=oms-shots
FRAMES=$(mktemp -d)
trap 'tmux kill-session -t $SESSION 2>/dev/null || true; rm -rf "$FRAMES"' EXIT

cargo build --release
(cd tools/termshot && cargo build --release)

# Default colors from the active theme so margins match the UI.
COLORS="$HOME/.config/omarchy/current/theme/colors.toml"
BG=$(grep -m1 '^background' "$COLORS" | grep -o '#[0-9a-fA-F]*')
FG=$(grep -m1 '^foreground' "$COLORS" | grep -o '#[0-9a-fA-F]*')

tmux kill-session -t $SESSION 2>/dev/null || true
tmux new-session -d -s $SESSION -x 110 -y 32 "$BIN"
sleep 3

n=0
shot() { # shot <settle-seconds> [keys…]
  local settle=$1
  shift
  for k in "$@"; do tmux send-keys -t $SESSION "$k"; sleep 0.35; done
  sleep "$settle"
  n=$((n + 1))
  tmux capture-pane -ep -t $SESSION >"$FRAMES/$(printf '%02d' $n).txt"
}

shot 0.4                # 01 themes + logo + preview
shot 0.6 j              # 02 next theme
shot 0.6 j              # 03
shot 0.6 j              # 04
shot 0.8 2              # 05 wallpapers + in-terminal preview
shot 0.8 j j            # 06 preview follows selection
shot 1.0 t              # 07 theme-from-wallpaper wizard
shot 0.8 Right          # 08 muted mode, live re-extraction
shot 0.8 Escape 0       # 09 integrations
shot 0.8 Tab            # 10 power (battery thresholds)
shot 0.8 Tab            # 11 doctor
shot 0.6 1              # 12 back home

tmux kill-session -t $SESSION

./tools/termshot/target/release/termshot \
  --cols 110 --rows 32 --px 20 --bg "$BG" --fg "$FG" \
  --out "$FRAMES/png" --gif docs/assets/tour.gif \
  "$FRAMES"/01.txt:2200 "$FRAMES"/02.txt:900 "$FRAMES"/03.txt:900 \
  "$FRAMES"/04.txt:1300 "$FRAMES"/05.txt:1600 "$FRAMES"/06.txt:1400 \
  "$FRAMES"/07.txt:2000 "$FRAMES"/08.txt:1800 "$FRAMES"/09.txt:1600 \
  "$FRAMES"/10.txt:1600 "$FRAMES"/11.txt:1800 "$FRAMES"/12.txt:1200

# Convert the GIF to a much smaller, higher-quality MP4 video
ffmpeg -y -i docs/assets/tour.gif -movflags faststart -pix_fmt yuv420p -vf "scale=trunc(iw/2)*2:trunc(ih/2)*2" docs/assets/tour.mp4

cp "$FRAMES"/png/04.png docs/assets/themes.png
cp "$FRAMES"/png/06.png docs/assets/wallpapers.png
cp "$FRAMES"/png/08.png docs/assets/wizard.png
cp "$FRAMES"/png/11.png docs/assets/doctor.png
ls -la docs/assets/
