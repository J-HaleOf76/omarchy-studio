# How Omarchy Studio compares to A La Carchy

[A La Carchy](https://github.com/DanielCoffey1/a-la-carchy) is a debloater and configurator for Omarchy — a genuinely feature-dense single-file bash TUI, and the closest tool to Studio in spirit. People keep asking how the two relate, so here is an honest, source-verified comparison (as of July 2026). Where it's ahead of us today, we say so.

## At a glance

| | A La Carchy | Omarchy Studio |
|---|---|---|
| Form | one 10.8k-line bash script, run via `curl \| bash` | Rust workspace: engine crate + TUI/CLI binary, versioned releases |
| Undo | timestamped `.tar.gz` of 8 config dirs + restore script | git-backed snapshot store: every change pre/post-committed, `snapshot undo`, automatic rollback when a change breaks a reload |
| Apply safety | confirmation prompts (bypassable via "Apply all") | apply pipeline: drift-check → snapshot → hash-guarded write → reload → **verify → rollback on failure**; Waybar crash-watchdog auto-reverts a config that kills the bar |
| Survives `omarchy-update` | mostly — but two features edit vendored files under `~/.local/share/omarchy/default/`, which updates silently revert | always — Studio never writes into Omarchy's vendored tree; every change is a user-side override or managed block |
| Omarchy version awareness | none (hard-coded paths) | adapter layer isolates all Omarchy paths; version probe + Omarchy 4 tracking on the roadmap |
| Tests / CI | none | 195+ tests, clippy `-D warnings`, adapter-boundary check, CI on every push |
| Updates | re-download the script | built-in self-update (respects pacman/AUR-owned installs) |
| Maintenance | stale since 2026-04 (author waiting for Omarchy 4) | active; issues and Discussions open |

## Feature by feature

| Domain | A La Carchy | Omarchy Studio (today) | Studio plan |
|---|---|---|---|
| Theme creation | pywal wallpaper theming ("Themarchy") | palette editor, theme fork, median-cut extraction wizard (Normal/Muted/Material), in-TUI wallhaven browser | instant theme-from-current-wallpaper one-liner (0.9.2) |
| Community themes | 252 one-click installs | — | browser **with pre-install palette preview** (0.9.1) |
| Keybinds | guided 3-step rebind, conflict warnings | live chord capture, conflict detection, per-bind source attribution, snapshot-backed overrides | cheatsheet export (0.9.5) |
| Hyprland settings | 69 settings (general/decoration/input/gestures) | 56 settings across Windows/Layout/Behavior/Decoration/Blur/Shadow/Input/Touchpad with **live preview**, presets, try/apply/undo — every keyword verified against the shipped Hyprland (0.8.1) | gesture add/remove UI |
| Custom config paths | fixed paths | per-module target overrides + running-bar `-c/-s` detection, three-tier resolution (0.8.2) | — |
| App/webapp removal | 17 apps + 15 webapps, `pacman -Rns --noconfirm` (bypassable "apply all") | **dependency-cascade preview**, service disable, leftover listing, restore manifest; pacman refusals surfaced as blockers, no confirm-bypass (0.8.3) | — |
| Monitors | detect, position wizard, primary, laptop auto-off | detect/identify, scale with live effective-size, disable, snapshot-backed `monitors.conf` + hotplug fallback (0.8.4) | full placement wizard + laptop auto-off |
| Quick tweaks | 30 toggles, edited in place | one-key catalog, each tweak a self-contained managed block, individually revertible with live state (0.8.5) | more tweaks slot into the registry |
| Power/battery | profiles, AC/battery auto-switch, charge limits | battery charge thresholds | profiles + auto-switch with previewed root writes (0.8.6) |
| Waybar | logo/clock/tray toggles | full three-lane module editor, catalog, custom-module wizard, crash watchdog | — |
| Notifications / OSD / lock / idle | — | full editors (mako rules + DND + live test, SwayOSD, hyprlock, hypridle timeline) | — |
| TUI theme sync (lazygit, yazi, fzf, bat…) | — | — | 0.7.2 |
| ASUS ROG hardware | full asusctl suite: fan curves, Aura RGB, GPU MUX, AniMe | — | post-1.0 at most — **A La Carchy is the better tool for ROG laptops today** |
| Fingerprint / FIDO2 | setup flows | — (Omarchy's own menu covers this) | not planned |

## The philosophical difference

A La Carchy is a *session*: run the script, pick changes, apply them. Studio is a *system*: an engine with a TUI and CLI on top, where every change goes through the same snapshot → write → reload → verify → rollback pipeline, so an experiment that breaks your bar or your compositor reverts itself. That difference is invisible in a feature table and is exactly why Studio exists.

Both tools follow the same golden rule for most changes — user-side overrides and managed blocks rather than editing Omarchy's files — and credit where due: A La Carchy's monitor-layout math and its breadth of curated settings informed our v0.8 plan.

*Corrections welcome — if anything above is out of date, open an issue.*
