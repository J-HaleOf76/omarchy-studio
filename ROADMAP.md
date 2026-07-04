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

- [ ] 0.2.1 hyprlang line CST + round-trip property tests [03 §2]
- [ ] 0.2.2 `hyprctl binds -j` model + source attribution pass [05 §1.1]
- [ ] 0.2.3 Dispatcher schema (~40 entries) + human rendering [05 §1.1]
- [ ] 0.2.4 Conflict detection + override/disable/reset writes [05 §1.2]
- [ ] 0.2.5 ChordCapture widget (kitty protocol + fallback picker) [05 §1.3, 07 §4]
- [ ] 0.2.6 Keybinds screen (table, search, edit form, jargon strip) [07]
- [ ] 0.2.7 CLI `keybind add/remove/disable/reset/check/list` [08]
- [ ] 0.2.8 Verify-rollback e2e with injected configerrors [09 §3]

## v0.3 — Look & Feel + Animations (~2 wks) — *exit: try→apply→undo presets; zero broken reloads*

- [ ] 0.3.1 looknfeel schema + form rendering from schema [05 §2, 01 §2]
- [ ] 0.3.2 Live keyword batching + preview-active crash marker [05 §2]
- [ ] 0.3.3 Preset loader + try/apply + managed section render [05 §3]
- [ ] 0.3.4 Remaining 6 presets tuned by hand on the reference machine
- [ ] 0.3.5 Toggle-flag switches (omarchy-toggle-* wrap) [05 §2]
- [ ] 0.3.6 BezierCanvas editor behind feature flag [05 §3]
- [ ] 0.3.7 CLI `looknfeel get/set`, `preset list/try/apply` [08]

## v0.4 — Waybar (~3 wks) — *exit: reorder bar + add custom module without touching JSON*

- [ ] 0.4.1 JSONC CST spike: comment-preserving edit + array reorder (go/no-go on jsonc-parser) [03 §3]
- [ ] 0.4.2 Module catalog schema (30 modules, deps, snippets) [05 §4]
- [ ] 0.4.3 Lanes widget + module forms + generic key/value editor [07 §4]
- [ ] 0.4.4 style.css managed block (geometry) + @import-position assert [03 §4]
- [ ] 0.4.5 Custom module wizard (script scaffold) [05 §4]
- [ ] 0.4.6 Crash watchdog apply flow [05 §4]
- [ ] 0.4.7 CLI `waybar modules/add/remove/move/set` [08]

## v0.5 — Notifications, OSD, Lock/Idle (~2 wks) — *exit: mako behavior survives theme switch + omarchy-update*

- [ ] 0.5.1 mako tpl materializer (include-core + colors + managed section) [05 §5]
- [ ] 0.5.2 mako schema + rule builder + live test + DND [05 §5]
- [ ] 0.5.3 swayosd editor + self-test [05 §6]
- [ ] 0.5.4 hypridle timeline + hyprlock form (backup convention) [05 §7]
- [ ] 0.5.5 Hooks install (theme-set, post-update) + doctor drift/clobber flows [02 §4, §6]
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
