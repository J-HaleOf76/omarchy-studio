# v0.8/v0.9 Architecture & Implementation Guide

Self-contained implementation guide for the v0.8 (configurator parity+) and v0.9 (community & differentiation) milestones. It carries the competitive context, binding repo conventions, per-task architecture, and per-milestone definition of done — a session can implement from this document alone. Companion: [comparison.md](comparison.md) for the public-facing feature comparison.

---

## 1. Mission & competitive context

[a-la-carchy](https://github.com/DanielCoffey1/a-la-carchy) (200★) is a 10,804-line single-file bash TUI debloater/configurator for Omarchy. Stale since 2026-04-28; author publicly waiting for Omarchy 4. Source-verified inventory: 17-app + 15-webapp removal (`pacman -Rns`/`omarchy-webapp-remove`), keybind editor, **69 Hyprland settings** (General 26 / Decoration 22 / Input 16 / Gestures 5) as managed blocks in `looknfeel.conf`/`input.conf`, monitor wizard (scaled coords, rotation, primary, laptop auto-off via IPC watcher), 30 tweaks, ASUS ROG suite (asusctl), 252 community theme installs (`omarchy-theme-install` loop), pywal wallpaper-theming, tar.gz backups.

Its weaknesses (all code-verified): no releases/tags/tests/CI; **no Omarchy version detection**; edits vendored files under `~/.local/share/omarchy/default/` that `omarchy-update` silently reverts (close-window rebind, transparency toggle); "Apply all" bypasses every confirmation incl. `pacman -Rns --noconfirm`; no dependency-cascade preview; removes Docker packages but never disables `docker.service`; backups don't cover the udev//etc/default-share changes it makes.

Omarchy 4 (`omarchy-4` branch, in heavy dev): new **omarchy-shell** (own bar/panels, plugin hot-reload — displaces Waybar), two-package architecture + `[omarchy]` pacman repo, `upgrade-to-4` migration. Studio's adapter boundary makes early v4 compatibility feasible; that is the decisive strategic advantage.

**Direction:** parity first, but Studio must not become another a-la-carchy — every parity feature ships with something theirs can't do, plus net-new differentiators. ROG hardware: **post-1.0 only** (no ASUS test hardware); the comparison doc notes that niche stays theirs for now.

## 2. Repo conventions (binding for the implementing agent)

- Repo: `~/Projects/omarchy-studio` (github `arino08/omarchy-studio`). Workspace: `crates/studio-core` (engine — **never draws, never reads stdin**) + `crates/omarchy-studio` (TUI+CLI). Specs live in `docs/` (01–10), roadmap in `ROADMAP.md`.
- **No AI attribution anywhere** — commits, docs, metadata. No `Co-Authored-By`.
- Workflow per task: feature branch → commit per task → ff-merge to `main` → push → **verify CI green** (`gh run list --branch main --json status,conclusion,headSha`, match HEAD) before the next task.
- Lint gate before every commit: `rustup run 1.96.1 cargo fmt --all && rustup run 1.96.1 cargo clippy --workspace --all-targets -- -D warnings && rustup run 1.96.1 cargo test --workspace`.
- **Adapter boundary (CI-enforced grep):** the literal string `local/share/omarchy` may appear only under `crates/studio-core/src/omarchy/` — including in tests. All Omarchy paths go through `OmarchyPaths`.
- **Never write under `$OMARCHY_PATH` / `~/.local/share/omarchy/`** (a-la-carchy's footgun is our selling point). All writes: user config dirs, via the snapshot-backed apply pipeline (drift-check → pre-snapshot → hash-guarded write → reload → verify → post | rollback).
- Reuse existing machinery — do not reinvent: `Cmd` runner (timeout/capture/dry-run), `ManagedBlock`, hyprlang line-CST (byte-identical round-trip), JSONC editor, toml_edit models, snapshot store, capability/dependency registries, `Skin` theming, worker-thread pattern from `screens/wallhaven.rs` (mpsc Job/Outcome, `busy()`, poll-while-busy event loop — threads never read stdin), `ImageCell`, `chord::normalize`.
- TUI screens follow the existing pattern: `handle(KeyEvent) -> Action` enum + `render(f, area, skin, …)`; registered in the rail in `tui/mod.rs`; every feature gets a CLI twin in `main.rs`.
- Verification: unit tests in-module (fake runners/fetchers, tempdirs, golden files); drive the TUI in a tmux pty (`tmux new-session -d -x 110 -y 32 ./target/release/omarchy-studio`, `send-keys`, `capture-pane -p`). **tmux does not support the kitty keyboard protocol — uppercase-letter bindings can't be regression-tested there; test the lowercase path and unit-test `chord::normalize`.**
- Update `ROADMAP.md` checkboxes and the README feature tables/status line in the same milestone.

## 3. Step 0 — Docs commit (done with this file's introduction)

1. `docs/handoff-v0.8.md` — this guide.
2. `docs/comparison.md` — honest Studio vs a-la-carchy table (feature-by-feature from §1, our safety/update-survivability story, ROG noted as theirs). Factual tone, linkable. Cross-linked from README ("How Studio compares").
3. `ROADMAP.md` restructure: 0.7.3 moves into v0.8 (as 0.8.3); v0.8 + v0.9 milestones added exactly as §4–§5 (0.8.2 custom-config-targets is a direct community ask — note the source like v0.7 does); 0.7.2/0.7.4 stay in v0.7; "Omarchy 4 readiness" track added (§6); ROG note under Post-1.0.
4. GitHub Discussions enabled + `.github/ISSUE_TEMPLATE/` (bug + feature-request, plain) — the open feedback loop a-la-carchy lacks.

## 4. v0.8 — Configurator parity+ (each task = one branch/commit cycle)

### 0.8.1 Hyprland Configurator expansion (16 → ~70 settings)

**Where:** `crates/studio-core/src/modules/looknfeel.rs` (schema + write targets), `crates/omarchy-studio/src/tui/screens/looknfeel.rs` (category tabs).

**Design:** the schema-driven form, live `hyprctl keyword` preview, preset machinery, and snapshot pipeline already exist — this task is mostly schema data plus one structural change: settings gain a **target file**.

```rust
enum Target { Looknfeel, Input }   // → ~/.config/hypr/looknfeel.conf | input.conf (Omarchy convention)
struct Setting { key: &'static str, target: Target, group: &'static str, /* existing: label, jargon, kind, range/enum, default */ }
```

Writes stay ManagedBlock-per-file ("last value wins", originals untouched). The screen gains a tab bar (`Tab`/`Shift-Tab` or `[`/`]`) over groups: General / Layout / Misc / Decoration / Blur / Shadow / Input / Touchpad / Gestures.

**Setting inventory to add** — map each to the hyprctl keyword; ⚠️ **verify every keyword against the shipped Hyprland via `hyprctl getoption <k> -j` before inclusion — recent Hyprland reworked gestures (`gesture = N, direction, action` lines replace some `gestures:*` options) and shadow/blur live under `decoration:shadow:`/`decoration:blur:`. Drop or adapt anything the installed version rejects; the target is "comprehensive", not literally 69.**

- General/layout: col.active_border, col.inactive_border, no_border_on_floating, extend_border_grab_area, allow_tearing, dwindle:pseudotile, dwindle:force_split, dwindle:preserve_split, dwindle:smart_split, master:new_status (+ existing 5).
- Misc/binds/xwayland: misc:focus_on_activate, misc:disable_hyprland_logo, misc:vrr, misc:new_window_takes_over_fullscreen, misc:middle_click_paste, misc:enable_swallow, misc:key_press_enables_dpms, misc:mouse_move_enables_dpms, binds:workspace_back_and_forth, binds:allow_workspace_cycles, xwayland:force_zero_scaling.
- Decoration: shadow color, blur:special, blur:brightness, blur:contrast, blur:noise, blur:popups, dim_special, fullscreen_opacity, cursor hide-on-keypress (cursor:hide_on_key_press), cursor size (XCURSOR_SIZE/HYPRCURSOR_SIZE env — if not a keyword, expose via envs managed block or omit) (+ existing 11).
- Input (all new): kb sensitivity, follow_mouse, accel_profile, force_no_accel, left_handed, repeat_rate, repeat_delay, numlock_by_default, natural_scroll (mouse + touchpad), scroll_factor, scroll_method, scroll_button, touchpad:disable_while_typing, touchpad:tap-to-click, touchpad:drag_lock, touchpad:middle_button_emulation.
- Gestures (all new; version-check hard): workspace swipe + fingers + distance + invert + create-new (or the new `gesture` line syntax — if the new syntax, model as its own small ManagedBlock writer).

Color-typed settings (`col.*`): accept `rgba(RRGGBBAA)`/`rgb(RRGGBB)`; reuse palette-math hex validation; offer "use theme accent" shortcut reading the active `colors.toml`.

**Better-than-theirs:** live preview on every adjust (they only persist), preview-crash marker already handled, snapshot undo, presets.

**Tests:** schema invariants (unique keys, every key resolves via fake runner), per-target render golden, group counts; extend existing looknfeel tests. CLI `looknfeel list/get/set` picks new ids up for free — verify with one test.

### 0.8.2 Custom config targets (community ask: "can I edit my custom waybar / configs outside the default Omarchy locations?")

**Where:** new `crates/studio-core/src/modules/targets.rs`; touches path resolution in the waybar module (and any module with a user-relocatable file); Doctor; CLI verb `target`.

**Problem:** every module resolves its file(s) from fixed Omarchy-convention paths. Users with a custom Waybar config (different path, multiple bars, `waybar -c/-s` flags) or otherwise relocated configs can't point Studio at them. The editing machinery (JSONC editor, ManagedBlock, CST) is already path-agnostic — only resolution is hardcoded.

**Design — a resolution layer with three-tier precedence:**

```rust
pub struct Targets { /* loaded from [targets] in Studio's own config toml (toml_edit) */ }
// resolve(module_file) -> Resolved { path: PathBuf, source: Override | Detected | Default }
```

1. **User override:** `[targets]` table in Studio's config — e.g. `waybar.config = "~/.config/waybar/mybar.jsonc"`, `waybar.style = "…"`. CLI: `target list` (shows every module's resolved path + source), `target set <key> <path>`, `target reset <key>`. Validate on set (file exists, parses with the owning module's parser) — reject with a plain-language error otherwise.
2. **Detection:** for Waybar, read the running process's real config: scan `/proc/<pid>/cmdline` of `waybar` processes for `-c`/`--config` and `-s`/`--style` (fall back to Waybar's own search order: `~/.config/waybar/config` then `config.jsonc`). Multiple waybar instances → multiple candidates.
3. **Default:** current Omarchy-convention path (unchanged behavior for stock setups — zero config for the 90% case).

**TUI:** Waybar screen header shows the resolved target (`mybar.jsonc · override`); a `f`/target picker modal lists candidates (override, detected instances, default) and persists the choice via `target set`. If multiple bars are detected on open and no override exists, the picker shows first. Doctor gains a "config targets" row (each module: path, source, exists/parses).

**Scope:** Waybar (config + style) is the concrete, user-asked case and ships fully. The `Targets` API is generic — 0.8.4/0.8.5/tweaks route new file writes through it, and mako/swayosd/hypr overrides can be added as `[targets]` keys later without redesign (hypr files stay convention-bound for now: Hyprland itself hardcodes `~/.config/hypr/hyprland.conf` sourcing).

**Safety:** snapshot store already keys on absolute paths — custom targets get the same pre/post snapshots, drift detection, and rollback. Managed blocks in a custom file survive because we re-locate them by marker, not path assumptions.

**Tests:** precedence resolution (override > detected > default) with tempdirs, cmdline parsing fixtures (`-c`, `--config=`, absent), invalid-override rejection, round-trip of `[targets]` through toml_edit.

### 0.8.3 Apps & services manager (absorbs old 0.7.3 — their headline, done safely)

**Where:** new `crates/studio-core/src/modules/apps.rs`, new `crates/omarchy-studio/src/tui/screens/apps.rs`, CLI verb `apps`.

**Model:**

```rust
enum ItemKind { Package, WebApp, Service }
struct AppItem { id: String, label: String, kind: ItemKind, installed: bool,
                 desc: &'static str, services: Vec<String> /* e.g. docker → docker.service, docker.socket */ }
struct RemovalPlan { items: Vec<AppItem>, cascade: Vec<String> /* pacman would also remove */,
                     services: Vec<String>, desktop_files: Vec<PathBuf>, leftovers: Vec<PathBuf> }
```

- Curated registry seeded with a-la-carchy's verified lists — packages: 1password-beta, 1password-cli, docker(+buildx,+compose), gnome-calculator, kdenlive, libreoffice-fresh, localsend, obs-studio, obsidian, omarchy-chromium, pinta, signal-desktop, spotify, typora, xournalpp; webapps: Basecamp, ChatGPT, Discord, Figma, Fizzy, GitHub, Google Contacts/Maps/Messages/Photos, HEY, WhatsApp, X, YouTube, Zoom. Registry entries are hints — actual presence from `pacman -Q <pkg>` and `<data_home>/applications/<Name>.desktop` (webapp paths via the omarchy adapter; **verify against a live system where Omarchy puts webapp .desktop files — route through `OmarchyPaths`, not literals**). Also list any installed non-registry package via a search box (full `pacman -Qe` list, filterable).
- **Cascade preview (the differentiator):** before any removal, run `pacman -Rns --print --print-format '%n' <pkgs>` via the Cmd runner; show the full would-remove list in a preview pane; require explicit confirm. **No bulk-confirm bypass mode, ever.**
- **Service handling:** for items with units, `systemctl list-unit-files <unit>` → if enabled, plan `sudo systemctl disable --now <unit>` before package removal; shown in the preview.
- Webapps: `omarchy-webapp-remove <Name>` (adapter-wrapped).
- Leftovers: after removal list surviving `~/.config/<app>` / `~/.local/share/<app>` dirs (known-name map in the registry) — offer delete, default keep.
- **Restore:** every removal appends to a manifest (`state_dir/removed.toml`, toml_edit): id, kind, pkgs, date. `apps restore <id>` reinstalls via `sudo pacman -S --needed` / recreates webapp via `omarchy-webapp-install` if available (probe; else print guidance). Manifest + preview → we can undo what they can't.
- Snapshot store records config-file side effects; pacman ops are logged in the manifest (git snapshots don't cover system packages — say so in the confirm dialog).

**TUI:** checklist (Space toggles, badges: installed/webapp/service-attached), right pane = live RemovalPlan preview, Enter → confirm modal listing exact commands. **CLI:** `apps list [--installed]`, `apps remove <ids…> [--dry-run]`, `apps restore <id>`, `apps leftovers`.

**Tests:** plan-building against fake runner transcripts (pacman --print output parsing incl. multi-line + error), manifest round-trip, service-detection parsing; screen tests with fake plans.

### 0.8.4 Monitors screen

**Where:** new `crates/studio-core/src/modules/monitors.rs`, new `screens/monitors.rs`, CLI `monitor`.

- **Model:** parse `hyprctl monitors -j` (serde; reuse pattern from `monitor_atleast()` in `tui/mod.rs` — then refactor that helper to use this module). Fields: name, description(make/model), width/height@hz, scale, transform, position, focused, dpms; tag `eDP-*` as laptop.
- **Layout math** (steal their good idea, unit-test it): effective size = resolution/scale, width/height swapped for transform 1/3 (90/270°). Placement wizard: pick primary at 0x0 + rotation; place each remaining monitor Left/Right/Above/Below any placed one; compute absolute coords.
- **Writes:** `~/.config/hypr/monitors.conf` via the hyprlang CST — `monitor=NAME,WxH@Hz,XxY,scale,transform,N` lines inside a ManagedBlock + trailing `monitor=,preferred,auto,1` hotplug fallback; optional GDK_SCALE env line (1.75 when any scale >1.5). Snapshot-backed, `hyprctl reload`, verify via re-query (positions match plan) else rollback — the full apply pipeline, which they don't have.
- **Primary/workspace:** assign workspace 1..n to monitors — live `hyprctl dispatch moveworkspacetomonitor` + persist `workspace = N, monitor:NAME, default:true` lines in the managed block.
- **Identify:** `i` → `hyprctl notify` per monitor (2 s).
- **Laptop auto-off:** deploy watcher script to `~/.config/omarchy/studio/laptop-display-auto.sh` (NOT the hypr scripts dir they use) + `exec-once` in the managed block; socat → nc → 5 s-poll fallback chain; saves/restores brightness via brightnessctl (dependency-registry gated). Toggle-off removes cleanly. Snapshot-backed.
- **CLI:** `monitor list`, `monitor identify`, `monitor primary <name>`, `monitor scale <name> <f>`, `monitor apply --dry-run`.

**Tests:** layout math goldens (L-shape, stacked, rotated, scaled), hyprctl -j fixture parsing, conf render golden, verify-mismatch → rollback path with fake runner.

### 0.8.5 Tweaks catalog

**Where:** new `crates/studio-core/src/modules/tweaks.rs` (registry), new `screens/tweaks.rs`, CLI `tweak`.

```rust
struct Tweak { id, label, jargon, state: fn(&Ctx)->TweakState /* on/off/custom */,
               apply: fn(&mut Ctx)->Result<()>, revert: fn(&mut Ctx)->Result<()> }
```

Each tweak routes through an **existing engine** — no new file-format code:
- Waybar (JSONC editor + style css): 12/24 h clock, clock date toggle, tray icons, Omarchy logo module, update-icon module, window-title module.
- Looknfeel schema (now expanded): zero-gaps toggle, rounded-corners toggle (windows).
- CSS managed blocks (existing swayosd/waybar css writers; add walker/mako/hyprlock radius where our v0.5 modules already own the file): rounded corners for the other surfaces.
- Transparency: user-override managed block in a user-owned hypr file setting opacity rules — **never** the vendored `default/hypr/windows.conf` (their update-reverted footgun; called out in comparison doc).
- New small writers: compose/caps key (`~/.XCompose` + `input:kb_options` via 0.8.1 input target), alt↔super swap (kb_options), media directories (create `~/Pictures/Screenshots`, `~/Videos/Screencasts` + Omarchy screenshot/record env or config if a user-side knob exists — probe first, skip with note if vendored-only).
- Every tweak: snapshot-backed, idempotent (state fn), revertible individually.

**TUI:** grouped checklist, Space applies/reverts with toast, all through apply pipeline. **CLI:** `tweak list`, `tweak <id> on|off`.
**Tests:** registry invariants, each writer's render golden, state round-trip (apply → state on → revert → state off) against tempdir configs.

### 0.8.6 Power screen expansion (extend existing `screens/battery.rs` → Power)

- Power profiles: `powerprofilesctl list/set` (dependency-registry gate on power-profiles-daemon); persist via profile restore on login (exec-once managed block calling `powerprofilesctl set <p>`).
- AC/battery auto-switch: udev rule `/etc/udev/rules.d/99-power-profile.rules` written via `sudo tee` through the Cmd runner, with the full rule text shown in a confirm modal first (root writes always previewed). Removal path deletes the rule.
- Suspend/hibernate toggles behind explicit confirm; hibernation swap-subvolume setup is complex/root-heavy — implement detection + guided instructions first, automation only if clean (`btrfs subvolume` + `mkswap` + dracut resume args; if messy, print the steps — never half-automate root fs changes).
- Rename rail entry Battery → Power; CLI `power profile get|set|auto on|off` beside existing `battery`.
- Skip fingerprint/FIDO2 (Omarchy's own menu covers it — noted in comparison doc).

**Tests:** udev rule render golden, profile parsing, capability gating with fake runner.

## 5. v0.9 — Community & differentiation

- **0.9.1 Community themes browser** (Themes screen gains a `c` tab/modal): vendor `data/community-themes.tsv` (name, owner/repo — seed from the Omarchy extra-themes directory; add `tools/refresh-themes.sh` to re-scrape). Worker-thread installs via `omarchy-theme-install owner/repo` (reuse wallhaven Job/Outcome pattern; per-item timeout, auth-gated repos skipped with note). **Differentiator: pre-install preview** — fetch raw `colors.toml` from the repo (existing Fetch trait) and render our swatch strips; search/filter; installed markers. CLI `theme community list/search/install`.
- **0.9.2 Instant theme-from-wallpaper:** `theme new --from-current-wallpaper [--apply]` (current bg path from omarchy adapter; extraction engine exists) + optional keybind deploy (`SUPER+SHIFT+T` line via keybinds override writer, opt-in) — matches Themarchy convenience with a better extractor (median-cut vs pywal).
- **0.9.3 Zero-install trial:** `scripts/run.sh` published at a stable URL (`curl -fsSL … | sh`): downloads the versioned release binary + sha256 check into `$XDG_CACHE_HOME/omarchy-studio/`, execs it. README/website CTA. Counters their curl-pipe with a checksummed, versioned equivalent.
- **0.9.4 Snapshot timeline screen:** browsable history of the git snapshot store — every Studio change (what/when/files), colored diff pane (`git show` via runner or similar; TUI diff render), selective restore of one commit's files through the apply pipeline. CLI `snapshot log/show/restore <id> --files`. *Only possible because our store is git — flagship safety feature.*
- **0.9.5 Keymap cheatsheet export:** `keybind cheatsheet [--out file.html]` — effective keymap (hyprctl binds -j + our dispatcher labels + source attribution) rendered to a self-contained themed HTML (palette from active `colors.toml`); group by category; print-friendly.
- **0.9.6 Rice migration bundle:** `export-rice` / `import-rice` — archive of installed custom themes + Studio-managed blocks + tweak/removal manifests + keybind overrides; import **replays through the apply pipeline** with verification on the target machine (vs their blind untar). Format: tar.gz with `manifest.toml` (version, omarchy version, contents map).

## 6. Omarchy 4 readiness (continuous)

- **Now (with Step 0 or 0.8.1):** version probe in the omarchy adapter — read Omarchy's version (probe `~/.local/share/omarchy/version` or git tag via adapter; verify the real mechanism on-machine), surface in Doctor + `doctor --deps`; warn (don't block) on unknown/major versions. The guard a-la-carchy lacks.
- Track `basecamp/omarchy` `omarchy-4` branch at each milestone end; when omarchy-shell stabilizes: spike an adapter (bar model analog to the Waybar module). Two-package/pacman-repo changes likely move paths — the adapter boundary contains the blast radius.
- README/website positioning: "built to track Omarchy — adapter-isolated, version-aware, self-updating."

## 7. Build order & scope guard

Step 0 → 0.8.1 → 0.8.2 (targets layer early — 0.8.5 tweaks and all waybar writes route through it) → 0.8.3 → 0.8.4 → 0.8.5 → 0.8.6 → **0.7.2 TUI theme sync** (kept differentiator: per-tool palette generators — lazygit/yazi/fzf/bat/zellij/k9s — off `colors.toml`, opt-in, re-asserted by theme-set hook) → v0.9 in order → **0.7.4 video walkthrough** (record after parity+differentiators — stronger demo) → v1.0 polish (profiles, export/share→superseded partly by 0.9.6, template coverage, elephant provider, walker styling, tmux e2e in CI, docs/AUR).

Post-1.0: ROG basic tier (asusctl profiles/battery/kbd-brightness, capability-gated, community-tested), GUI frontend, generic-Hyprland mode.

**Scope guards:** no root automation without a previewed command; no vendored-file writes; no confirm-bypass; anything requiring hardware we don't have (ROG, fingerprint) ships as guidance or not at all; if a Hyprland keyword doesn't exist on the shipped version, drop it rather than write dead config.

## 8. Verification per milestone (definition of done)

1. Lint gate green; new module has unit tests at the density of its neighbors (golden files for every renderer/parser).
2. tmux pty drive of each new screen (launch, navigate, exercise one action with `--dry-run`-safe paths, capture-pane sanity).
3. On the reference machine: one real end-to-end per feature (e.g. remove+restore a webapp; apply a monitor layout; toggle 5 tweaks then run `omarchy-update` and prove they survive — the demo a-la-carchy fails).
4. README feature tables + status line + ROADMAP checkboxes updated in the same milestone; comparison doc updated if claims changed.
5. Push, CI green on `main` before the next task.
