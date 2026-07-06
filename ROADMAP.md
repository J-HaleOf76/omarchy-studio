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
- [ ] 0.5.4 hypridle timeline + hyprlock form (backup convention) [05 §7]
- [x] 0.5.5 Hooks install (theme-set, post-update) + doctor drift/clobber flows [02 §4, §6]
- [ ] 0.5.6 Doctor screen [07]

## v0.6 — Wallpaper & Palette Lab (~2–3 wks) — *exit: wallhaven → crafted theme, entirely inside Studio*

- [ ] 0.6.1 Wallpaper browser (4-source merge, video gating) [04 §5]
- [ ] 0.6.2 Extraction port: median-cut + Normal/Muted/Material + golden tests vs Aether [04 §4]
- [ ] 0.6.3 wallhaven client (search/filters/rate budget/thumb cache) [04 §6]
- [ ] 0.6.4 ImageCell + graphics probe + imv fallback [07 §4, 06]
- [ ] 0.6.5 Theme-from-wallpaper wizard [04 §7]
- [ ] 0.6.6 Integrations screen (tool registry, contextual actions) [06 §5]
- [ ] 0.6.7 CLI `wallpaper wallhaven search/download`, `theme new --from-image` [08]

## v1.0 — Polish (~2 wks) — *exit: a stranger installs from AUR and rices start-to-finish*

- [ ] 1.0.1 Profiles (bundle save/switch) — PRD Q8 may cut this
- [ ] 1.0.2 Theme export/share + gh flow [04 §8]
- [ ] 1.0.3 Template coverage manager + starter templates [04 §3]
- [ ] 1.0.4 Elephant provider + first-run wizard + uninstall [02 §3.2, 10 §3–4]
- [ ] 1.0.5 Walker styling (M8) [05 §8]
- [ ] 1.0.6 tmux e2e journeys + screenshot grid in CI [09 §4]
- [ ] 1.0.7 Docs (quickstart, what-file-did-that-touch, tutorial) + AUR PKGBUILDs [10]
- [ ] 1.0.8 Manual test protocol run on reference machine [09 §6]

## Post-1.0 (unordered)

GUI frontend over studio-core · remaining extraction modes · walker behavior depth · monitors/input basic forms · generic-Hyprland mode · per-module waybar colors (PRD Q5) · AC/battery hypridle profiles
