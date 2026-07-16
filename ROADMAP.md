# Roadmap

Milestones from PRD §12, broken into issue-sized tasks. Order within a milestone is the suggested build order; `[spec]` links say where the how lives. Estimates assume part-time solo work.

## v0.1 — Skeleton + Themes (~3 wks) — *exit: create & apply a custom theme end-to-end with undo, from the Omarchy menu*

- [x] 0.1.1 Error taxonomy + `Cmd` runner with timeout/capture/dry-run [01 §4, 02 §2]
- [x] 0.1.2 `omarchy::paths` + capability probe [02 §1, §5] *(on-disk probe cache deferred to 0.1.5 with toml_edit — probe is <20 ms)*
- [x] 0.1.3 Snapshot store: init, pre/post commits, restore, drift commits, undo-last; `configfs::atomic_write` landed alongside [03 §5–6]
- [x] 0.1.4 Apply pipeline core (drift-check → pre → hash-guarded write → reload → verify → post | rollback) [01 §3]
- [x] 0.1.5 `colors.toml` model + toml_edit round-trip + palette math (HSL, WCAG) [04 §1–2]
- [x] 0.1.6 Theme listing/apply/fork (overlay semantics) [04 §1]
- [x] 0.1.7 TUI shell: rail, header, keybar/jargon, help overlay, command palette, Studio self-theming [07 §1–5]
- [x] 0.1.8 Themes screen: browser with swatch strips, apply, fork (modal) [07, 04]
- [x] 0.1.9 Palette editor + scratch-theme live preview loop [04 §2]
- [x] 0.1.10 Dependency registry loader + probing [06 §1–3] *(DepBanner widget lands with the TUI, 0.1.7)*
- [x] 0.1.11 CLI: `theme list/current/apply/fork`, `snapshot list/restore/undo`, `doctor --deps` [08] *(theme new --from-image lands with v0.6 extraction; known nit: SIGPIPE panic when piped to `head` — fix with the clap migration)*
- [x] 0.1.12 Menu-block installer (idempotent, snapshot-backed) + install-integration/uninstall CLI; floating launch via omarchy-launch-floating-terminal-with-presentation [02 §3]
- [x] 0.1.13 Golden corpus vendored (3 real colors.toml), round-trip + edit-preservation tests, CI (fmt·clippy -D·test·adapter-boundary) [09 §2, §5]

## v0.2 — Keybinds (~2 wks) — *exit: rebind a default chord safely without seeing config syntax*

- [x] 0.2.1 hyprlang line CST + byte-identical round-trip over 3 vendored real hypr configs; nested/dotted get-set, binds() accessor [03 §2]
- [x] 0.2.2 bind model + source attribution: modmask<->names, render_chord, RuntimeBind (hyprctl -j via serde), ConfigBind from hyprlang Entry, layer-priority attribution [05 §1]
- [x] 0.2.3 dispatcher schema: 41 Hyprland dispatchers with plain-language labels, categories, ArgKind; lookup/label_for/in_category, unknowns fall back to raw id [05 §2]
- [x] 0.2.4 conflict detection + override writes: find_conflicts, ConfigBind render, Override model, write_overrides into user bindings.conf via ManagedBlock (idempotent, preserves user binds) [05 §3]
- [x] 0.2.5 chord capture: KeyEvent -> Hyprland chord (modmask+key name), kitty keyboard enhancement flags pushed where supported for SUPER capture [05 §4]
- [x] 0.2.6 Keybinds screen: effective keymap from hyprctl binds -j, human labels + source attribution, rebind via live chord capture + disable, snapshot-backed override writes with live reload [05]
- [ ] 0.2.7 CLI `keybind add/remove/disable/reset/check/list` [08]
- [ ] 0.2.8 Verify-rollback e2e with injected configerrors [09 §3]

## v0.3 — Look & Feel + Animations (~2 wks) — *exit: try→apply→undo presets; zero broken reloads*

- [x] 0.3.1 looknfeel schema + schema-driven form screen: grouped plain-language settings reading effective (default+user) values, h/l adjust, live render [05 §2, 01 §2]
- [x] 0.3.2 live hyprctl-keyword preview on every adjust + preview-active crash marker (startup reloads to discard stale live values) [05 §2]
- [x] 0.3.3 preset loader + try(live)/apply(persist)/undo + managed section render; preset picker overlay [05 §3]
- [x] 0.3.4 six hand-tuned presets: Omarchy default, Minimal, Cozy, Frosted glass, Performance, Focus
- [x] 0.3.5 behavior toggles: core registry wrapping omarchy-*-toggle scripts (state via omarchy-hyprland-toggle-enabled), TUI toggle overlay (t) + CLI toggle list/<id> [05 §2]
- [x] 0.3.6 Animations: 6 hand-tuned feel presets (Off/Fast/Smooth/Minimal/Bouncy/Default) via managed block + reload; screen + CLI. (Visual bezier canvas deferred to feature-flagged 0.3.6b)
- [x] 0.3.7 CLI looknfeel list/get/set + preset list/try/apply (snapshot-backed, live reload) [08]

## v0.4 — Waybar (~3 wks) — *exit: reorder bar + add custom module without touching JSON*

- [x] 0.4.1 JSONC editor (span-splice, comment-preserving): GO — parse/get/array_items/set_scalar/reorder, byte-identical round-trip of real waybar config.jsonc; no external jsonc-parser dep [03 §3]
- [x] 0.4.2 Waybar module catalog (20 modules: label/group/summary/deps/default-config snippet, custom/group fallbacks) + WaybarConfig model (lanes/reorder/add/remove/save/apply) [05 §4]
- [x] 0.4.3 Waybar screen: three-lane layout with human module labels, J/K move in lane, H/L move across lanes, a=add (catalog picker), d=remove, s=apply (restart Waybar, snapshot-backed) [07 §4]
- [x] 0.4.4 style.css geometry: managed CSS block (font-size, corner radius) appended after the theme  so it wins; CLI waybar style show/font-size/radius/reset [03 §4]
- [x] 0.4.5 custom-module wizard: JsoncDoc::insert_member + WaybarConfig::add_custom_module scaffold a custom/* config object + lane entry from a plain spec (name/exec/interval/format/on-click); TUI form (a → Custom command) + CLI waybar new [05 §4]
- [x] 0.4.6 crash watchdog: apply_watched saves+restarts then confirms Waybar stayed up (settle delay); auto-reverts the previous config if the edit killed the bar; only arms when Waybar was confirmed running first (no false reverts). Wired into TUI + CLI apply [05 §4]
- [x] 0.4.7 CLI waybar modules/add/remove/move/set (snapshot-backed, restarts Waybar) [08]

## v0.5 — Notifications, OSD, Lock/Idle (~2 wks) — *exit: mako behavior survives theme switch + omarchy-update*

- [x] 0.5.1 mako tpl materializer: MakoTpl writes ~/.config/omarchy/themed/mako.ini.tpl = include core.ini + theme colour placeholders + Studio-managed behavior block; survives theme switch + omarchy-update; detects symlink-replaced-by-realfile degrade; apply = write → omarchy-theme-refresh → makoctl reload [05 §5]
- [x] 0.5.2 mako schema (12 behavior settings, defaults mirror core.ini) + rule builder (per-app/urgency criteria→settings) + live test (notify-send low/normal/critical) + DND (makoctl mode); TUI Notifications screen (form + DND toggle + test picker + rules panel) + CLI notif list/get/set/rule/dnd/test [05 §5]
- [x] 0.5.3 SwayOSD editor: config.toml (toml_edit: show-percentage/max-volume/top-margin) + style.css managed geometry block (radius/font, after ); apply → omarchy-restart-swayosd; self-test swayosd-client raise 0 (flash). CLI osd show/set/test + TUI "o" flash on Notifs/OSD screen [05 §6]
- [x] 0.5.4 hypridle timeline + hyprlock form (backup convention) [05 §7]
- [x] 0.5.5 Hooks install (theme-set, post-update) + doctor drift/clobber flows [02 §4, §6]
- [x] 0.5.6 Doctor screen [07]

## v0.6 — Wallpaper & Palette Lab (~2–3 wks) — *exit: wallhaven → crafted theme, entirely inside Studio*

- [x] 0.6.1 Wallpaper browser (4-source merge, video gating) [04 §5]
- [x] 0.6.2 Extraction port: median-cut + Normal/Muted/Material + golden tests [04 §4]
- [x] 0.6.3 wallhaven client (search/filters/rate budget/thumb cache) [04 §6]
- [x] 0.6.4 ImageCell + graphics probe + imv fallback [07 §4, 06]
- [x] 0.6.5 Theme-from-wallpaper wizard [04 §7]
- [x] 0.6.6 Integrations screen (tool registry, contextual actions) [06 §5]
- [x] 0.6.7 CLI `wallpaper wallhaven search/download`, `theme new --from-image` [08]

## v0.7 — Community asks (~2 wks) — *exit: the tool keeps itself current and themes reach beyond Omarchy's own configs*

Sourced from user feedback on the announcement threads.

- [x] 0.7.1 Auto-update: release check on launch + `update` CLI verb — download, verify, swap binary, restart; detects pacman/AUR-owned installs and defers to the package manager there
- [x] 0.7.2 TUI theme sync: propagate the active theme's palette to terminal tools Omarchy doesn't theme — opt-in per tool, standalone generated files wired via one `~/.bashrc` managed block, regenerated by the `theme-set` hook. **Shipped: fzf, lazygit** (both verified against the real tools); the `Tool` generator registry is pluggable so yazi/bat/zellij/k9s can be added without redesign. CLI `themesync list|enable|disable|apply`
- [x] 0.7.6 Nova launcher module: configure the Quickshell Nova launcher — visual mode, backdrop, anim toggles, providers → `~/.config/nova/nova.json` (unknown keys preserved), optional launch keybind via the shared keybinds override block, launch from the rail. TUI screen + CLI `nova`
- [ ] 0.7.4 Video walkthrough: scripted asciinema/screen capture of an install-to-riced session, linked from README + website *(record after v0.9 — stronger demo)*
- [x] 0.7.5 In-TUI wallhaven browser (`w` in Wallpapers): worker-thread search, sort/ratio/color-match/purity filters, cached thumbnails, download→set/keep/wizard — finishes the spec 04 §6 story 0.6.3 started

*(0.7.3 apps & services manager moved to 0.8.3.)*

## v0.8 — Configurator parity+ (~4 wks) — *exit: no feature reason left to reach for a-la-carchy on non-ROG hardware*

Competitive milestone; full architecture in [docs/handoff-v0.8.md](docs/handoff-v0.8.md), public comparison in [docs/comparison.md](docs/comparison.md). Every task ships something the bash competition can't do (live preview, cascade preview, verify/rollback, per-change revert).

- [x] 0.8.1 Hyprland Configurator expansion: looknfeel schema 16 → **56 settings** (Windows/Layout/Behavior/Decoration/Blur/Shadow/**Input/Touchpad**), each with a `Target` file (`looknfeel.conf`/`input.conf`), `[`/`]` category tabs, live preview on every adjust; every keyword verified against the shipped Hyprland 0.55 (`hyprctl getoption`) — dead `gestures:*` keys and gradient border colors omitted, not written as inert config. Gestures deferred (0.55 replaced `gestures:*` options with the `gesture` directive — needs its own add/remove UI)
- [x] 0.8.2 Custom config targets (community ask): three-tier resolution (`[targets]` override in Studio config > running-Waybar `-c/-s` detection via `/proc` > Omarchy default) in new `studio-core::modules::targets`; Waybar `WaybarConfig`/`WaybarStyle` load & save route through it (snapshots follow the resolved path); Waybar screen shows the active target with an `f` picker (override / running instances / default), Doctor "config targets" row, `target list/set/reset` CLI with existence + JSONC-parse validation on set
- [x] 0.8.3 Apps & services manager (was 0.7.3): new `studio-core::modules::apps` — curated catalog (15 pkgs + 15 web apps), live presence from `pacman -Q` + `.desktop` scan. **Dependency-cascade preview** via `pacman -Rs --print` that surfaces pacman's refusal (an outside package still needs the selection) as a *blocker* instead of failing mid-transaction; systemd unit disable (only if enabled); leftover-config listing; removal manifest (`state_dir/removed.toml`) + `apps restore <id>`. New Apps rail screen (checklist → plan preview → confirm modal, blocked plans can't be run) + CLI `apps list [--installed] [--all] / remove <id…> [--dry-run] [--yes] / restore <id> / leftovers`. **No confirm-bypass mode, ever**; web apps removed via adapter-wrapped `omarchy-webapp-remove`
- [x] 0.8.4 Monitors screen: new `studio-core::modules::monitors` — parse `hyprctl monitors -j` (serde), transform-aware **effective-size math** (res/scale, w/h swap under 90°/270°), relative placement (`place`/`row_positions`), `monitor=` line render into a `monitors.conf` managed block + `monitor=,preferred,auto,1` hotplug fallback. Monitors rail screen (list, +/- scale with live effective-size, `d` disable, `i` identify via `hyprctl notify`, `s` save snapshot-backed + reload) + CLI `monitor list / identify / scale <name> <f> [--dry-run] / apply [--dry-run]`. Refactored `monitor_atleast()` onto the module. *(Positioning wizard beyond L/R row + laptop auto-off watcher deferred — highest-risk on live hardware.)*
- [x] 0.8.5 Tweaks catalog: new `studio-core::modules::tweaks` — a `Tweak` trait registry where each tweak owns a **uniquely-named managed block** in a user-side file (or the filesystem), reports its own on/off state, and reverts cleanly, never touching vendored files. Ships caps→escape (input.conf), inactive-window transparency (looknfeel.conf, Omarchy's `windowrule = opacity` syntax), and screenshot/screencast dirs (filesystem, revert keeps non-empty). New Tweaks rail screen (Space toggles, live state) + CLI `tweak list / <id> on|off`; extensible — more tweaks (clock format, gaps, alt↔super) slot into the registry without frontend changes
- [x] 0.8.6 Power screen expansion: new `studio-core::modules::power` — read/switch power profiles (`powerprofilesctl list/get/set`, capability-gated on power-profiles-daemon), persist a choice across logins via an `exec-once` managed block in `autostart.conf`, and render the AC/battery auto-switch **udev rule** for preview (root writes are always shown as exact text + `sudo tee` command before running, never executed by the module). Power rail screen gains a profiles row with `p` to cycle+persist; CLI `power profile list|get|set <name> [--persist]` and `power auto <ac> <bat> [--yes]` / `auto off` (previewed, `--yes` to apply). *(Suspend/hibernate automation deferred — root fs changes ship as guidance, not half-automation.)*

## v0.9 — Community & differentiation (~3 wks) — *exit: features no Omarchy tool has, built on the snapshot store and theme engine*

- [x] 0.9.1 Community themes browser: vendored theme list + refresh script, worker-thread `omarchy-theme-install`, **pre-install palette preview** from raw `colors.toml`, search/filter; CLI `theme community`
- [x] 0.9.2 Instant theme-from-wallpaper: `theme new --from-current-wallpaper [--apply]` + opt-in SUPER+SHIFT+T keybind deploy
- [x] 0.9.3 Zero-install trial: checksummed `curl | sh` runner for the release binary; README/website CTA
- [x] 0.9.4 Snapshot timeline screen: browse every change Studio ever made, colored diffs, whole-tree restore from any point (itself recorded); CLI `snapshot log`/`show <id>`/`restore <id> --files`
- [x] 0.9.5 Keymap cheatsheet export: effective keymap → self-contained themed HTML
- [ ] 0.9.6 Rice migration bundle: `export-rice`/`import-rice` — replayed through the apply pipeline with verification on the target machine

## Omarchy 4 readiness (continuous track)

- [ ] O4.1 Omarchy version probe in the adapter; Doctor surfaces it, warns on unknown/major versions
- [ ] O4.2 Track the `omarchy-4` branch (omarchy-shell, two-package layout) at each milestone end; spike an omarchy-shell adapter when it stabilizes

## v1.0 — Polish (~2 wks) — *exit: a stranger installs from AUR and rices start-to-finish*

- [ ] 1.0.1 Profiles (bundle save/switch) — PRD Q8 may cut this
- [ ] 1.0.2 Theme export/share + gh flow [04 §8] *(partly superseded by 0.9.6 rice bundle — rescope at v1.0 start)*
- [ ] 1.0.3 Template coverage manager + starter templates [04 §3]
- [ ] 1.0.4 Elephant provider + first-run wizard + uninstall [02 §3.2, 10 §3–4]
- [ ] 1.0.5 Walker styling (M8) [05 §8]
- [ ] 1.0.6 tmux e2e journeys + screenshot grid in CI [09 §4]
- [ ] 1.0.7 Docs (quickstart, what-file-did-that-touch, tutorial) + AUR PKGBUILDs [10]
- [ ] 1.0.8 Manual test protocol run on reference machine [09 §6]

## Post-1.0 (unordered)

GUI frontend over studio-core · remaining extraction modes · walker behavior depth · generic-Hyprland mode · per-module waybar colors (PRD Q5) · AC/battery hypridle profiles · ASUS ROG basic tier (asusctl profiles/battery/kbd-brightness, capability-gated — needs community testing; no ASUS reference hardware)
