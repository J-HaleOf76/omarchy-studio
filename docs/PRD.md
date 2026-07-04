# PRD — Omarchy Studio

**A unified theming & desktop-configuration tool for Omarchy — every knob, no config-file jargon.**

| | |
|---|---|
| Working title | **Omarchy Studio** (binary: `omarchy-studio`) — name open for review, see §16 |
| Version | 0.3 draft — approved direction (0.2: one-stop-shop scope; 0.3: guided dependency handling for missing tools/packages) |
| Date | 2026-07-04 |
| Author | Ariz (+ Claude research) |
| Target platform | Omarchy ≥ 3.8 (verified against 3.8.2 on this machine), Hyprland 0.55.x |
| Status | Awaiting stakeholder (you) review |

---

## 1. Executive summary

Omarchy ships an opinionated, beautiful Hyprland desktop, but everything beyond *picking a theme* still means hand-editing config files: `bindings.conf` for keybinds, `looknfeel.conf` for gaps/blur/animations, `config.jsonc` + CSS for Waybar, ini files for notifications, and so on. The community has built **color/wallpaper theming GUIs** (Aether, Omarchist, tema, half a dozen palette generators) — but **nobody has built the control center**: one place to manage keybinds, animations, bar layout, notification behavior, lock/idle, OSD, and themes together, integrated into Omarchy's own menu.

**Omarchy Studio** is that control center: a keyboard-driven TUI (with a headless CLI core, GUI-ready later) that

- edits every user-facing Omarchy surface through forms, pickers, and live previews — never raw text (though an "open in editor" escape hatch is always one keystroke away);
- is the **one-stop shop for the whole theming journey**: browse wallhaven for a wallpaper, extract a palette from it (multiple modes), import Base16/community schemes, craft the full theme, then tune the behavior — all without leaving Studio. Where good open-source tools already exist (Aether, omarchist, generators), Studio ports their proven algorithms (MIT/Apache) or interoperates with their outputs instead of shelling out to their GUIs;
- **only writes to Omarchy's sanctioned files and extension points**, and applies changes through Omarchy's own primitives (`omarchy-theme-set`, `omarchy-theme-refresh`, `omarchy-restart-*`), so it never fights the distro or breaks on `omarchy-update`;
- previews changes **live** (via `hyprctl keyword`, terminal OSC sequences, `makoctl reload`, Waybar restarts) with automatic revert if you don't confirm;
- snapshots every change into a local git-backed history — one-key undo for anything it ever did;
- plugs into the Omarchy menu (`Super + Alt + Space → Style → Studio`) via the sanctioned `extensions/menu.sh` + Walker/elephant provider mechanism, and launches as a floating terminal window exactly like Omarchy's own TUIs.

---

## 2. Problem statement

1. **The config cliff.** Omarchy's menu covers theme *selection*, but customization beyond that drops you into a text editor (`Style → Hyprland` literally opens `looknfeel.conf` in nvim; `Setup → Keybindings` opens `bindings.conf`). For anyone who doesn't already speak Hyprland/Waybar/mako config dialects, that's a wall: five different syntaxes (hyprlang, JSONC, GTK-CSS, ini, TOML), scattered across `~/.config/hypr`, `~/.config/waybar`, `~/.config/mako`, `~/.config/omarchy`, with layering rules you have to reverse-engineer from `hyprland.conf`'s `source=` order.
2. **No discoverability.** You can't customize what you can't see. Which Waybar modules exist? What animation curves are possible? Which keybinds are already taken (and by which of the 6 sourced binding files)? Today the answer is "read the source of `~/.local/share/omarchy`."
3. **No safety net.** A typo in `looknfeel.conf` breaks compositor reload; editing files that `omarchy-refresh-*` manages gets silently clobbered (with a `.bak` you have to notice); there's no undo.
4. **Fragmented tooling.** Colors are solved 6 times over (Aether, Omarchist, tema, 3+ generators); monitors have 2 TUIs; keybinds, animations, notification behavior, Waybar layout have *nothing*. Each tool has its own UI paradigm and none integrate with each other or (except partially) with the Omarchy menu.

**User statement:** *"I want to customize and edit everything — notifications, alerts, waybar, hyprland, keybinds, animations — from one GUI/TUI, without the config-file jargon, integrated into Omarchy's menu-centric theming system."*

---

## 3. Background: how Omarchy theming actually works (research findings)

> Verified by source-reading the local install: `~/.local/share/omarchy` (`$OMARCHY_PATH`), Omarchy **3.8.2**, git `master`. This section is the technical foundation the whole product design rests on.

### 3.1 Theme anatomy

A theme is a directory in `$OMARCHY_PATH/themes/<slug>` (19 built-ins, read-only) or `~/.config/omarchy/themes/<slug>` (user). Its **source of truth is `colors.toml`**: `accent`, `cursor`, `foreground`, `background`, `selection_{fore,back}ground`, `color0`–`color15`. Everything else is optional and additive:

| File | Purpose |
|---|---|
| `light.mode` | Marker file — presence flips system to light mode (gsettings `prefer-light` + Adwaita) |
| `icons.theme` | GNOME icon theme name (Yaru variants) |
| `chromium.theme` | Browser accent as decimal `R,G,B` |
| `neovim.lua` / `vscode.json` | Editor colorscheme plugin/extension declarations |
| `btop.theme` | Native btop theme (shipped, or template-generated) |
| `keyboard.rgb` | Hardware RGB keyboard accent |
| `backgrounds/`, `videos/` | Wallpapers (this machine is patched for video wallpapers via `mpvpaper-rs`/`swww`) |
| `preview.png`, `unlock.png`, `preview-unlock.png` | Picker & lock-screen art |

### 3.2 The template engine

Per-app color configs are **not** stored per theme. At apply time, `omarchy-theme-set-templates` expands `*.tpl` files — from `~/.config/omarchy/themed/` (user, wins) then `$OMARCHY_PATH/default/themed/` (built-in) — substituting `{{ key }}`, `{{ key_strip }}` (no `#`), `{{ key_rgb }}` (decimal) from `colors.toml`. A file the theme ships verbatim beats the template. Built-in templates cover: alacritty, kitty, ghostty, foot, btop, chromium, gum, helix, hyprland (border colors), hyprlock, mako, obsidian, swayosd, walker, waybar (as `@define-color`), and more.

**Implication:** `~/.config/omarchy/themed/*.tpl` is the sanctioned way for any tool to (a) theme apps Omarchy doesn't cover and (b) override how any app is themed. This is a core integration surface for Studio.

### 3.3 Apply pipeline (`omarchy-theme-set <name>`)

Stage in `current/next-theme` (system theme copied first, user theme **overlaid** file-by-file on top) → synthesize `colors.toml` from `alacritty.toml` if absent → expand templates → **atomic swap** into `~/.config/omarchy/current/theme/` (a real merged directory) → write slug to `current/theme.name` → set wallpaper → restart components (`omarchy-restart-{waybar,swayosd,terminal,hyprctl,btop,mako,helix,…}`) → app setters (`omarchy-theme-set-{gnome,browser,vscode,obsidian,foot,keyboard}`) → fire **`omarchy-hook theme-set <slug>`**.

`omarchy-theme-refresh` re-runs this for the current theme without touching the wallpaper — **the primitive Studio calls after any palette/template edit**.

### 3.4 How apps consume the theme (verified wiring)

- `~/.config/mako/config` → **symlink** to `current/theme/mako.ini`, which `include=`s read-only `$OMARCHY_PATH/default/mako/core.ini` (behavior) then sets colors. Behavior customization therefore goes through a user `themed/mako.ini.tpl` override.
- `~/.config/waybar/style.css` starts with `@import "../omarchy/current/theme/waybar.css"` (colors as `@define-color`); layout/structure CSS is user-owned. `config.jsonc` (modules) is user-owned.
- `~/.config/swayosd/style.css` — same `@import` pattern.
- Hyprland: `~/.config/hypr/hyprland.conf` sources, **in override order**: Omarchy defaults (read-only: autostart, bindings/*, envs, looknfeel, input, windows) → theme `hyprland.conf` (accent border) → **user files** `monitors.conf`, `input.conf`, `bindings.conf`, `looknfeel.conf`, `autostart.conf` → dynamic toggle flags in `~/.local/state/omarchy/toggles/hypr/`. User files win because they're sourced last.

### 3.5 Extension surfaces (the integration contract)

| Surface | Mechanism |
|---|---|
| `~/.config/omarchy/themed/*.tpl` | Add/override per-app theming templates |
| `~/.config/omarchy/hooks/<event>.d/*` | Lifecycle hooks: `theme-set` (slug as `$1`), `post-update`, `post-boot`, `font-set`, `battery-low`. Install via `omarchy-hook-install` |
| `~/.config/omarchy/extensions/menu.sh` | Sourced by `omarchy-menu` before dispatch; may redefine any `show_*` menu function (single file — tools must merge, not glob) |
| `~/.config/elephant/menus/*.lua` | Walker/elephant graphical menu providers (how the theme picker with previews works) |
| `omarchy-menu <substring>` | Deep-link CLI into any menu (e.g. `omarchy-menu theme`) |
| `omarchy-restart-*` / `omarchy-theme-refresh` | Safe live-reload primitives |
| `omarchy-refresh-config <path>` | Backup-then-replace pattern (`<file>.bak.<epoch>`, prints diff) — the convention Studio mirrors |

### 3.6 Update survival

`omarchy-update` git-pulls `$OMARCHY_PATH` and runs migrations (325 shipped; ledger in `~/.local/state/omarchy/migrations/`). Migrations may rewrite user configs. Rules for a durable tool: **never edit `$OMARCHY_PATH`** (this machine's dirty checkout — the hand-patched video-wallpaper scripts — is the cautionary tale: it will conflict on update), keep customizations in the user surfaces above, and re-validate managed files after updates (via a `post-update.d` hook).

### 3.7 Launch conventions

Omarchy TUIs (wiremix, bluetui, btop) get floating 875×600 centered windows via window-rule tags matching class `org.omarchy.*` or `TUI.float` (`default/hypr/apps/system.conf`). Studio launches the same way — zero window-rule installation needed if we set class `TUI.float.omarchy-studio`.

---

## 4. Competitive landscape & gap analysis

| Tool | Stack | Covers | Doesn't cover |
|---|---|---|---|
| **Aether** (bjarneo) — semi-blessed by DHH | Go + Wails + Svelte | Wallpaper→palette extraction (8 modes), wallhaven browser, image filters, 24 presets, Base16 import, 20+ app templates | Keybinds, animations, Waybar layout, notification behavior, anything non-color |
| **Omarchist** (tahayvr, 706★) | Rust + GPUI | Visual theme designer; partial Waybar/Hyprland config editing (early dev) | Keybinds, animations, notifications, menu integration depth |
| **tema** | — | Theming UI, live previews, presets | Same gaps |
| Theme generators (JaimeStill, maxberggren, ryrobes…) | Various | Image → theme one-shot | Everything else |
| **hyprmon**, **omarchy-monitor-settings** | Go/TUI | Monitors only | Everything else |
| **omarchy-enhanced-bindings** | bash | Keybind tweaks (static) | UI, everything else |
| **Omarchy itself** (`omarchy-menu`) | bash + walker | Theme pick/install/remove, font, background, "edit this file in nvim" for the rest | Any structured editing |

**The gap Studio fills:** the *behavioral and structural* half of ricing — keybinds, animations, look-and-feel numbers, Waybar modules/layout, notification & OSD behavior, lock/idle — unified with the *visual* half (themes), in one keyboard-driven UI that respects and extends the Omarchy menu system.

**Positioning on the visual half: one-stop shop.** The table above is six single-purpose tools; Studio absorbs the high-value visual workflows *natively* — wallhaven browsing (public JSON API), palette extraction (Aether's engine is MIT — port the modes, credit the source), Base16/Base24 scheme import, image→theme crafting — rather than launching other GUIs mid-journey. Interop is guaranteed by format, not by glue: a theme made in Aether or omarchist is a standard theme dir Studio lists and edits immediately, and Studio's themes are equally standard in reverse. Installed companions are still detected and offered contextually where they genuinely outclass us (M11).

---

## 5. Goals & non-goals

### Goals
1. **G1 — Zero-jargon editing** of every user-facing Omarchy config surface: themes, wallpapers, keybinds, animations, look & feel, Waybar, mako, SwayOSD, hyprlock/hypridle, walker styling.
2. **G2 — Native citizen**: reachable from the Omarchy menu, launched like Omarchy's own TUIs, using Omarchy's own apply/reload primitives, surviving `omarchy-update`.
3. **G3 — Live feedback**: see every change within ~100 ms, confirm or auto-revert.
4. **G4 — Reversibility**: every mutation snapshotted; one-key undo; never destroy user hand-edits.
5. **G5 — Theme authorship & sharing**: create → edit → preview → export a `omarchy-<name>-theme` git repo, round-tripping with the community format.
6. **G6 — Scriptability**: everything the TUI does is also a CLI verb (`omarchy-studio get/set/apply/snapshot`), so power users and scripts get the same engine.
7. **G7 — One-stop shop**: the full theming journey — find a wallpaper, extract a palette, craft the theme, tune the behavior — happens inside Studio. Community tools are detected and interoperated with, never *required*.

### Non-goals (v1)
- **N1** — Not a general-purpose dotfile manager (no chezmoi/stow ambitions; scope = Omarchy surfaces).
- **N2** — Not an image *editor*: Aether-style wallpaper filters (blur/vignette/exposure/sharpen) stay out of scope — we absorb its extraction capabilities, not its editing canvas (deep-link to Aether for that).
- **N3** — No daemon/background service; Studio runs, applies, exits. Hooks handle the rest.
- **N4** — Not multi-distro (assume Omarchy paths; a generic-Hyprland mode may come later).
- **N5** — No GUI in v1 (architecture keeps a GUI frontend possible — see §10/§16).
- **N6** — Not an installer/package manager (no `Install → Package` duplication).
- **N7** — Monitors/input get *basic* forms only; hyprmon and Omarchy's own flows already exist (revisit post-v1).

---

## 6. Personas

1. **Ari — the tinkerer (primary; this is you).** Comfortable in the terminal, patches Omarchy source when needed (video wallpapers), but resents re-learning five config dialects for every tweak and wants one fast, keyboard-driven cockpit with undo. Cares about: speed, live preview, not breaking `omarchy-update`, video wallpaper support.
2. **The refugee.** Came to Omarchy from macOS/Windows for the aesthetics; can follow instructions but a wall of `bind = SUPER SHIFT, code:172, exec, …` sends them back to YouTube. Cares about: discoverability, safe defaults, "what does this do" explanations, not being able to break things.
3. **The theme author.** Publishes `omarchy-*-theme` repos; wants palette editing with instant full-desktop preview, coverage for extra apps via templates, and one-command export with previews and README scaffold.

---

## 7. Product principles

1. **Never fight Omarchy.** Write only to user-owned files and sanctioned extension surfaces (§3.5). Apply through Omarchy's primitives. If Omarchy has a flow (theme install via git URL), wrap it, don't reinvent it.
2. **The file is still the truth.** Studio parses real configs, preserves comments and unknown lines, and marks only its own contributions. Hand-edits and Studio edits coexist; "open raw file in $EDITOR" is always bound to `e`.
3. **Show, don't describe.** Live preview by default (compositor via `hyprctl keyword`, terminals via OSC, mako via test notification, Waybar via restart), with a visible "confirm within 10 s or revert" for risky changes (the display-settings pattern).
4. **Undo is sacred.** Git-backed snapshot store of every managed file before/after each apply. `u` = undo last, timeline view = restore any point.
5. **Explain everything.** Every field has a one-line human description and shows the generated config line, teaching the jargon passively instead of hiding it.
6. **Keyboard-first, menu-native.** Feels like Omarchy: walker-style fuzzy pickers, gum-style prompts, same fonts/colors (Studio themes itself from `current/theme`, obviously).
7. **Never strand the user.** A feature whose external requirement is missing (binary, package, network, terminal graphics protocol) stays *visible but disabled*, with a plain-language note saying what's missing, why it's needed, and the exact install command — one key installs it (with confirmation) or copies the command. No silent failures, no stack traces, no "go search the wiki". New users should never have to debug the tool they just installed.

---

## 8. Feature specification

Priorities: **P0** = MVP, **P1** = v1.0, **P2** = post-1.0. Each module is a top-level TUI screen and a CLI namespace.

### M1 — Theme Studio (P0)

*Browse, apply, create, edit, export themes.*

- **FR1.1 (P0)** Browse all themes (union of system + user, like `omarchy-theme-list`) with palette swatch strip, light/dark badge, and background thumbnail (sixel/kitty-graphics where supported, ASCII swatches otherwise). Apply on Enter → runs `omarchy-theme-set`.
- **FR1.2 (P0)** Palette editor for the 21 `colors.toml` keys: HSL/hex input, terminal-rendered swatches, contrast checker (fg/bg, accent/bg — WCAG ratio), 16-color ANSI preview strip, derive-variants helpers (lighten/darken, complement).
- **FR1.3 (P0)** Live preview: on change, debounce 150 ms → write palette to a scratch theme copy → `OMARCHY_THEME_SKIP_BACKGROUND=1` refresh → whole desktop updates. Esc restores pre-edit state from snapshot.
- **FR1.4 (P0)** Create theme: fork any existing theme (system themes overlay-forked into `~/.config/omarchy/themes/`), start from current, import an `alacritty.toml` (reusing Omarchy's own converter semantics), or pick from a **bundled Base16/Base24 catalog** (tinted-theming's 250+ community schemes, offline, fuzzy-searchable with swatch previews).
- **FR1.5 (P1)** Theme extras editor: `light.mode` toggle, `icons.theme` picker (installed Yaru variants), `chromium.theme` color, `vscode.json` + `neovim.lua` pickers (with lists of known theme names), `keyboard.rgb`.
- **FR1.6 (P1)** Export & share: scaffold `omarchy-<name>-theme/` git repo — theme files, generated `preview.png` (via `omarchy capture screenshot` prompt flow), README from template, `git init` + optional `gh repo create`. Install remote themes by URL (wraps `omarchy-theme-install`).
- **FR1.7 (P1)** Template coverage manager: list every app template (built-in + user `themed/*.tpl`), show which the current theme overrides, create/edit user overrides with placeholder autocomplete (`{{ accent_rgb }}` etc.), and a starter catalog of community templates for uncovered apps (zed, spotify/spicetify, discord…). Applies via `omarchy-theme-refresh`.
- **FR1.8 (P1)** **Native palette extraction** from any image: median-cut/k-means core with the extraction modes proven in Aether's MIT-licensed Go engine, ported to Rust with attribution (Normal, Monochromatic, Analogous, Pastel, Material, Colorful, Muted, Bright — see §16 Q9 for v0.6 depth), light/dark bias, automatic accent selection with WCAG guard, and per-slot manual override before committing to `colors.toml`.

### M2 — Wallpapers & Discovery (P1)

- **FR2.1** Grid/list of the wallpapers for the current theme across all four source dirs Omarchy merges (`~/.config/omarchy/backgrounds/<theme>/`, theme `backgrounds/`, user theme dirs, `videos/`); set on Enter (`omarchy-theme-bg-set`), cycle, add (file picker / path), remove (user dirs only).
- **FR2.2** **Video wallpaper aware**: detect and support the `mpvpaper-rs`/`swww` video/GIF flow present on this machine (see §16 Q6 — this patch should graduate into Studio or an upstream PR rather than remain a dirty checkout).
- **FR2.3 (P2)** Per-theme user background dirs management; import from URL.
- **FR2.4 (P1)** **Built-in wallhaven.cc browser**: search / toplist / random with category, resolution, ratio, and *color* filters (search by your palette's accent!), thumbnail grid via kitty/sixel graphics, download into a theme's backgrounds dir. Anonymous by default; optional API key in Studio config unlocks purity tiers and favorites (§16 Q10). Native via wallhaven's public JSON API — no external tool needed.
- **FR2.5 (P1)** **"Craft theme from this wallpaper"** — the flagship one-stop wizard: pick or download an image → FR1.8 extraction (choose mode, tweak slots live) → generate a complete theme dir (`colors.toml`, background, auto `preview.png`, optional `light.mode` from luminance) → live-preview via scratch apply → name, save, set. Also invocable from any wallpaper anywhere in Studio (`t` = "theme from this").

### M3 — Keybinds (P0)

*The flagship "no jargon" module.*

- **FR3.1 (P0)** Unified bind table from `hyprctl binds -j` (ground truth — exactly what `omarchy-menu-keybindings` renders), joined with source attribution by parsing the sourced files: *Omarchy default* (read-only, `$OMARCHY_PATH/default/hypr/bindings/*.conf`), *theme*, *user* (`~/.config/hypr/bindings.conf`), *toggle*. Fuzzy search by key, action, or description.
- **FR3.2 (P0)** Add/edit user binds via form: **key-capture widget** (press the chord, we record modmask+keysym), dispatcher picker with plain-language names ("Launch app", "Focus window left", "Resize", "Run command"…), argument helpers (app picker for `exec`, direction picker, workspace number).
- **FR3.3 (P0)** Conflict detection: live "this chord is taken by X (source: Y)" with one-key "override it" → writes `unbind` + new `bind` to user `bindings.conf` (the only sanctioned way to mask read-only defaults).
- **FR3.4 (P1)** Bulk ops: disable a default bind (unbind only), reset user file section to stock, import/export bind sets.
- **FR3.5 (P1)** Cheat-sheet export (markdown/PNG) and instant "test bind" (fires the dispatcher once via `hyprctl dispatch`).
- **FR3.6 (P0)** Apply = write file → `hyprctl reload` → verify via `hyprctl configerrors`; on errors, auto-rollback from snapshot and show the error translated to the offending row.

### M4 — Look & Feel + Animations (P0)

- **FR4.1 (P0)** Form-based editing of the `looknfeel.conf` surface: gaps (in/out), border size, rounding, active/inactive border (color-from-theme toggle vs custom), blur (enabled, size, passes, noise), shadows, dim, active/inactive opacity, layout (dwindle/master + their sub-options).
- **FR4.2 (P0)** **Instant preview without saving**: every slider/stepper fires `hyprctl --batch keyword <k> <v>`; Save persists to `~/.config/hypr/looknfeel.conf`; Esc reverts via re-`hyprctl reload` (which restores file truth). This is the cheapest, safest live-tuning loop Hyprland offers.
- **FR4.3 (P0)** Animation presets library: named, curated bundles of bezier + `animation=` trees — ship ~8 (e.g. *Omarchy Default* (the shipped set), *Snappy*, *Silky*, *Minimal*, *Zoomy*, *Slide*, *Off/Performance*, *Battery-saver*). One-key try (live via keywords), one-key apply (persist).
- **FR4.4 (P1)** Advanced animation editor: per-target rows (windows, workspaces, fade, border, borderangle, layers) with enable/duration/curve/style; **bezier curve editor** rendered as a braille-canvas graph with handle nudging and an animated ball preview; custom curves saved to a personal preset store.
- **FR4.5 (P1)** Window-rule quick toggles surfaced from Omarchy's toggle system (`~/.local/state/omarchy/toggles/hypr/`): no-gaps-when-single, aspect ratio, opacity rules — as switches, wrapping the existing `omarchy-toggle-*` scripts where they exist.

### M5 — Waybar (P1)

- **FR5.1 (P0-parse/P1-full)** Round-trip `~/.config/waybar/config.jsonc` **preserving comments** (CST-based edit, not parse-reserialize). Backup + diff on every write, mirroring `omarchy-refresh-config` conventions.
- **FR5.2 (P1)** Layout editor: three lanes (left/center/right); move/add/remove/reorder modules; module catalog with descriptions and dependency notes (~30 stock modules: clock, battery, network, pulseaudio/wireplumber, tray, hyprland/workspaces, cpu, memory, temperature, bluetooth, power-profiles, idle_inhibitor, custom/*…).
- **FR5.3 (P1)** Per-module forms for the high-traffic settings (clock format with live strftime preview, battery states/thresholds, workspace count/persistence/icons, tray spacing…), generic key-value editor for the long tail.
- **FR5.4 (P1)** Bar geometry & style: position, height, margins, font size, spacing — written as structured edits to user `style.css` **below** the theme `@import` (theme keeps owning colors; Studio owns a clearly-marked `/* omarchy-studio */` block for geometry).
- **FR5.5 (P1)** Custom module wizard: name, `exec` script (scaffolded to `~/.config/waybar/scripts/`), interval, click actions, format icons.
- **FR5.6 (P1)** Apply = write → `omarchy-restart-waybar`; validate JSONC first; revert-on-crash watchdog (if waybar dies within 3 s, restore snapshot and restart).
- **FR5.7 (P2)** Bar presets (Omarchy stock, minimal, dense) and preset export/import.

### M6 — Notifications & OSD (P1)

- **FR6.1 (P1)** Mako behavior editor: default timeout, placement/anchor, max-visible, max stack height, icon size, grouping, per-urgency styling (colors default to theme tokens; timeouts/borders editable), per-app rules (`[app-name=Spotify]` criteria) via a rule-builder.
- **FR6.2** Mechanism: Studio materializes a user `~/.config/omarchy/themed/mako.ini.tpl` that (a) still `include=`s Omarchy's `core.ini` first — inheriting upstream behavior improvements, (b) appends Studio-managed overrides (later keys win), (c) keeps theme color placeholders. Then `omarchy-theme-refresh` + `makoctl reload`.
- **FR6.3 (P1)** Live test: fire sample notifications at each urgency (`notify-send`) directly from the editor; do-not-disturb toggle (wraps Omarchy's existing toggle/`makoctl` mode).
- **FR6.4 (P1)** SwayOSD: position (top/bottom %), max-volume, styling knobs (writes user `~/.config/swayosd/config.toml` + managed block in `style.css` below theme import) → `omarchy-restart-swayosd`, with a volume-bump self-test.

### M7 — Lock & Idle (P1)

- **FR7.1 (P1)** Hypridle timeline editor: visual sequence (dim → lock → screen off → suspend) with per-stage timeouts and on/off, AC vs battery profiles; writes `~/.config/hypr/hypridle.conf` → `omarchy-restart-hypridle`.
- **FR7.2 (P1)** Hyprlock: clock format, avatar (from `~/.config/omarchy/profile_images/`), blur/dim amounts, show/hide input hints; colors stay theme-owned. Respects Omarchy's `lock-screen-backup.*` convention before first edit. "Preview lock" action (hyprlock immediate).
- **FR7.3 (P2)** Screensaver & branding: edit `branding/screensaver.txt` / `about.txt` with figlet-style preview (wraps `omarchy-branding-*`).

### M8 — Launcher styling (P2)

- **FR8.1** Walker appearance knobs (width, rows, fuzziness threshold) via `~/.config/walker/config.toml`; styling stays theme-owned (`walker.css` template). Deep behavioral config out of scope.

### M9 — Snapshots, undo & profiles (P0)

- **FR9.1 (P0)** Snapshot store: bare git repo at `~/.local/state/omarchy-studio/history` tracking every file Studio manages (user hypr confs, waybar, mako tpl, swayosd, themes it edits, `themed/`, `extensions/menu.sh`). Auto-commit before & after each apply with structured messages (`m4: blur.size 8→12`).
- **FR9.2 (P0)** `u` undo last apply; timeline screen to inspect diffs and restore any point (restore = write files + run the right restart primitives).
- **FR9.3 (P1)** Named profiles ("work", "demo", "battery"): a saved bundle of theme + looknfeel + waybar + binds deltas, switchable from Studio, CLI, or the Omarchy menu entry.
- **FR9.4 (P1)** Doctor screen: detect drift (managed files changed outside Studio — fine, just re-parse), `omarchy-refresh-*` clobbers (`.bak.<epoch>` siblings newer than our last write → offer 3-way view), dirty `$OMARCHY_PATH` checkout warning (this machine!), failed-migration ledger entries.

### M10 — Omarchy menu & system integration (P0)

- **FR10.1 (P0)** Menu entries via the sanctioned single-file mechanism: idempotently manage a marked block in `~/.config/omarchy/extensions/menu.sh` that extends `show_style_menu` with `Studio` (and submenu deep-links: Themes/Keybinds/Animations/Waybar/Notifications). Handle the file-may-exist case by parsing for our markers; never clobber foreign content.
- **FR10.2 (P1)** Elephant provider `~/.config/elephant/menus/omarchy_studio.lua`: walker-native list of Studio screens + profiles with icons, so `Super+Space → "studio"` works too.
- **FR10.3 (P0)** Launch convention: `omarchy-studio` TUI runs in a floating terminal (class `TUI.float.omarchy-studio` → picked up by Omarchy's existing float rules); suggested keybind `Super+Ctrl+S` offered (written to user `bindings.conf` with conflict check by M3's own engine).
- **FR10.4 (P0)** CLI parity: `omarchy-studio <module> get/set/apply/…` (e.g. `omarchy-studio looknfeel set blur.size 12`, `omarchy-studio keybind add --chord "SUPER, B" --exec omarchy-launch-browser`, `omarchy-studio snapshot restore <id>`), plus `omarchy-studio doctor`. Machine-readable `--json`.
- **FR10.5 (P1)** Hooks: install `theme-set.d/50-omarchy-studio` (re-assert managed blocks that themes could affect) and `post-update.d/50-omarchy-studio` (run doctor, warn on clobbers) via `omarchy-hook-install`.
- **FR10.6 (P0)** Uninstall: `omarchy-studio uninstall` removes menu block, hooks, elephant provider; leaves user configs as-is (they're valid standalone); offers full restore-to-pre-Studio from snapshot 0.

### M11 — Tool integrations & interop (P1)

*Studio as the hub of the open-source theming ecosystem, not an island.*

- **FR11.1 (P1)** Tool registry (declarative, data-driven): detect installed companions — Aether, omarchist, matugen, hyprmon, omarchy-monitor-settings, theme generators — and surface contextual "open in / import from" actions where they genuinely outclass Studio (Aether's image filters, hyprmon's drag-and-drop monitor layout).
- **FR11.2 (P1)** Format-level interop guarantees: any tool that produces a standard theme dir (Aether, omarchist, the generators) shows up in Studio's browser instantly and is editable without conversion; Studio's themes are equally standard in reverse. Round-trip covered by golden tests against sample outputs of each major tool.
- **FR11.3 (P1)** Scheme ecosystems: the bundled Base16/Base24 catalog (FR1.4); Material You generation via matugen when installed, with FR1.8's Material mode as the native fallback.
- **FR11.4 (P2)** Provenance: ported algorithms (extraction modes etc.) carry license attribution (MIT/Apache) in-app and in docs.
- **FR11.5 (P2)** Community adapters: the registry is a TOML file others can PR — adding detect/launch/import support for a new tool requires no Studio code changes.
- **FR11.6 (P0)** **Dependency awareness & guided install**: at startup and per-feature, probe every external requirement (`command -v` for binaries, pacman database for packages, terminal capability handshake for graphics, network reachability for wallhaven). A missing requirement renders the feature greyed-out with an inline banner — *what's missing, why it's needed, exact command* (`yay -S mpvpaper-rs`) — plus `i` = run the install in a confirmation flow (pacman/yay in a spawned terminal, never silent sudo) and `c` = copy the command. The first-run wizard summarizes optional dependencies up front; `omarchy-studio doctor --deps` lists every dependency with install state and what it unlocks.
- **FR11.7 (P1)** Documented degradation matrix — every feature declares its fallback when a dependency is absent (no kitty/sixel graphics → ASCII swatch previews; no network → wallhaven browser shows offline notice, local features unaffected; no `mpvpaper-rs`/`swww` → video wallpapers prompt to install; no `gh` → theme export skips repo creation, prints manual steps). Fallbacks are part of each module's spec, not an afterthought.

---

## 9. UX design

### 9.1 Navigation model

- **Home = module rail** (left) + content pane (right) + context footer (keys). Global fuzzy command palette on `/` ("blur", "volume osd position", "SUPER+T"…) jumping straight to any setting — the discoverability answer.
- Vim keys + arrows everywhere; `e` = open underlying file in `$EDITOR` at the relevant line; `?` = help overlay; every field shows its generated config snippet in a one-line "jargon strip" at the bottom (passive learning).
- **Apply model:** field changes preview live where safe (M4 keywords, M1 debounced refresh); destructive/restarty changes (waybar, mako) batch into a visible "pending changes" tray → `a` applies all → snapshot → apply → verify → toast. Risky applies arm a 10 s revert countdown.

### 9.2 Sketch — Keybinds screen

```
┌ Omarchy Studio ────────────────────────────────── osaka-jade · 3.8.2 ┐
│ Themes      │  Keybinds                          [/] search  [n] new │
│ Wallpaper   │ ────────────────────────────────────────────────────── │
│▸Keybinds    │  CHORD            ACTION                     SOURCE    │
│ Look&Feel   │  SUPER+RETURN     Launch terminal            default   │
│ Animations  │  SUPER+B          Launch browser             default   │
│ Waybar      │ ▸SUPER+T          Launch btop        ⚠ user override   │
│ Notifs/OSD  │  SUPER+SPACE      App launcher (walker)      default   │
│ Lock&Idle   │  SUPER+CTRL+S     Omarchy Studio             user      │
│ Snapshots   │ ────────────────────────────────────────────────────── │
│ Doctor      │  bind = SUPER, T, exec, $TERMINAL -e btop              │
├─────────────┴────────────────────────────────────────────────────────┤
│ enter edit · n new · d disable · r reset · e raw file · u undo · ? │
└──────────────────────────────────────────────────────────────────────┘
```

### 9.3 Sketch — Look & Feel with live preview

```
│ Gaps inner   ◂ 5 ▸     Gaps outer  ◂ 10 ▸      ● live preview ON     │
│ Rounding     ◂ 0 ▸     Border      ◂ 2 ▸       colors: from theme    │
│ Blur         [x] on    size ◂ 8 ▸  passes ◂ 2 ▸                       │
│ ── jargon strip ──  decoration:blur:size = 8   (looknfeel.conf)      │
│           [s]ave to looknfeel.conf   [esc] revert (hyprctl reload)   │
```

---

## 10. Architecture & tech stack

### 10.1 Shape

```
┌───────────────────────────── omarchy-studio (single Rust binary) ───┐
│ TUI frontend (ratatui + crossterm)      CLI frontend (clap)         │
│ ──────────────────────────────────────────────────────────────────  │
│ Core engine (library crate — GUI-reusable later)                    │
│  • Config model: typed settings schema per module                   │
│  • Parsers/writers (comment- & layout-preserving):                  │
│      hyprlang (custom line-oriented CST — small grammar)            │
│      JSONC   (CST edit for waybar config.jsonc)                     │
│      TOML    (toml_edit — lossless, perfect for colors/walker/osd)  │
│      INI     (line editor for mako tpl)                             │
│      CSS     (managed-block editor only; never full CSS parse)      │
│  • Apply pipeline: snapshot → write(atomic tmp+rename) → reload     │
│      primitive → verify (hyprctl configerrors / process alive /     │
│      makoctl reload rc) → confirm-or-rollback                       │
│  • Snapshot store (git2 or shelling to git)                         │
│  • Omarchy adapter: paths, omarchy-* command wrappers, version      │
│      detection ($OMARCHY_PATH/version), capability probing          │
└─────────────────────────────────────────────────────────────────────┘
```

### 10.2 Stack recommendation: **Rust + ratatui**

- Ecosystem fit: Omarchy community tooling is Rust/Go-heavy (omarchist = Rust/GPUI; walker era is fast native tools); single static binary, trivial AUR packaging (`omarchy-studio-bin`), no runtime deps.
- `ratatui` is the most mature TUI kit (widgets, canvas for the bezier editor and swatches), `toml_edit` gives lossless TOML for free, `crossterm` handles kitty-keyboard-protocol key capture (needed for M3's chord recorder).
- Terminal graphics: `ratatui-image` (kitty/sixel autodetect — Alacritty/Ghostty/Kitty all present here) with ANSI-swatch fallback.
- Alternative considered: Go + bubbletea (fine, slightly weaker lossless-TOML/JSONC story); Tauri/GPUI GUI-first (heavier, worse fit with Omarchy's terminal-native UX, slower to MVP). The core/frontend split keeps a GUI (egui or Tauri) as a later additive frontend. See §16 Q2.

### 10.3 Write-safety strategy (the heart of trust)

1. Parse existing file into a CST; unknown/user content is untouchable by construction.
2. Studio-owned regions delimited by `# ── omarchy-studio managed (do not edit inside) ──` markers in shared files (`bindings.conf` additions, `style.css` geometry block, `extensions/menu.sh`); files Studio fully owns (its own tpl overrides) are marked in a header.
3. Atomic write (tmp + rename), then reload primitive, then **verify**: `hyprctl configerrors` must be empty; restarted process must be alive after 3 s; otherwise auto-rollback from the pre-apply snapshot and surface the error mapped back to the field.
4. Every write mirrors Omarchy's own backup etiquette in spirit, but through the git history store (richer than `.bak.<epoch>` litter) — with `doctor` reconciling against `.bak` files other Omarchy tools create.

### 10.4 Schema & version resilience

Module schemas (field → config key → widget → validation → docs line) live as data (embedded TOML), versioned per Omarchy/Hyprland capability: on startup Studio reads `$OMARCHY_PATH/version`, `hyprctl version`, probes for known landmarks (e.g. does `default/themed/` exist), and degrades gracefully (unknown Omarchy version → warn, restrict to conservative surfaces). Unknown keys in user files are preserved and shown in a generic editor rather than dropped.

---

## 11. Distribution & packaging

- AUR: `omarchy-studio` (source) + `omarchy-studio-bin`. One-liner installer for non-AUR flow. MIT license.
- First-run wizard: install menu integration (FR10.1/10.2), offer keybind, create snapshot 0 ("pre-Studio state").
- Follows Omarchy CLI etiquette (AGENTS.md): `omarchy-studio` name, `--help` with examples; no sudo anywhere.
- Docs: README + a "what file did that touch?" reference table (transparency builds trust with the config-file crowd).

---

## 12. Roadmap

| Milestone | Scope | Exit criteria |
|---|---|---|
| **v0.1 — Skeleton + Themes** (~3 wks) | Core engine, snapshot store, TUI shell, M1 browse/apply/palette-edit + live refresh, M10 CLI basics + menu entry | Create & apply a custom theme end-to-end with undo, from the Omarchy menu |
| **v0.2 — Keybinds** (~2 wks) | M3 complete (capture, conflicts, override, verify/rollback) | Refugee persona rebinds a default chord safely without seeing config syntax |
| **v0.3 — Look & Feel + Animations** (~2 wks) | M4 forms + live keywords + preset library; bezier editor behind a flag | Try→apply→undo animation presets; zero broken reloads |
| **v0.4 — Waybar** (~3 wks) | M5 JSONC CST, lanes editor, module forms, crash watchdog | Reorder bar + add custom module without touching JSON |
| **v0.5 — Notifications, OSD, Lock/Idle** (~2 wks) | M6, M7, hooks (FR10.5), doctor (FR9.4) | mako behavior via tpl override survives theme switch + omarchy-update |
| **v0.6 — Wallpaper & Palette Lab** (~2–3 wks) | M2 complete (wallhaven browser FR2.4, video flow FR2.2), extraction engine FR1.8, theme-from-wallpaper wizard FR2.5, integrations registry FR11.1–11.3 | Find a wallpaper on wallhaven, craft & apply a full matching theme — entirely inside Studio |
| **v1.0 — Polish** (~2 wks) | Profiles, theme export/share, elephant provider, template coverage manager, docs, AUR | A stranger installs from AUR and rices start-to-finish — wallpaper to keybinds — without opening an editor |
| Post-1.0 | GUI frontend, walker behavior, monitors/input, generic-Hyprland mode | — |

---

## 13. Risks & mitigations

| Risk | Likelihood | Mitigation |
|---|---|---|
| Omarchy moves fast (325 migrations; file layouts change) | High | Only sanctioned surfaces; capability probing + version gates (§10.4); `post-update` doctor hook; CI job that runs schema checks against omarchy master weekly |
| `omarchy-refresh-*` clobbers user configs Studio manages | Medium | Detect `.bak.<epoch>` siblings; 3-way re-apply flow in doctor; keep Studio deltas re-appliable from history |
| Comment-preserving JSONC/hyprlang editing is fiddly | Medium | CST line-editors with golden-file round-trip tests over the real Omarchy defaults + fuzzing; worst case, fall back to managed-block-only mode for that file |
| Upstream/DHH philosophy skews config-file-purist; menu extension is a single shared file | Medium | Position as community add-on (like omarchist); idempotent marked block in `menu.sh`; degrade to elephant provider + keybind if the file is user-owned |
| Two binds surfaces (defaults read-only) confuse users | Medium | Source attribution column + explicit "override" verb (unbind+bind) — never edit `$OMARCHY_PATH` |
| Live `hyprctl keyword` preview drifts from file truth | Low | Esc/exit always ends with `hyprctl reload` (file truth wins) |
| Overlap with Omarchist evolving config features | Medium | Differentiate: TUI-native, keybinds/animations/behavior depth, snapshot/undo, menu integration; revisit at v0.4 |
| This machine's patched `$OMARCHY_PATH` (video wallpapers) conflicts on update | Existing issue | Studio's M2 supports the flow via user-space; recommend upstreaming or moving the patch to hooks (doctor flags dirty checkout) |
| One-stop scope creep — absorbing every tool bloats Studio | Medium | Absorb only the theming journey (find → extract → craft → tune); everything else integrates via the M11 registry, which is data, not code; N2 keeps image editing out |
| wallhaven API changes / rate limits; ported extraction drifts from Aether upstream | Low | API client behind a trait with cached thumbnails + graceful offline mode; extraction ports are versioned snapshots with attribution, not a tracking dependency |

---

## 14. Success metrics

- **Task success:** the three persona journeys (§6) completable without opening a text editor — measured by dogfooding scripts in CI (tmux-driven TUI e2e).
- **Safety:** zero unrecovered broken-desktop states in beta (every failed apply auto-rolled back); `doctor` clean after an `omarchy-update` across ≥2 releases.
- **Adoption (post-AUR):** AUR votes / GitHub stars vs. omarchist's 706★ benchmark within 6 months; listed in awesome-omarchy.
- **Personal bar (v0.3):** you stop editing `looknfeel.conf`/`bindings.conf` by hand because Studio is faster.

---

## 15. Explicitly out of scope (v1 recap)

Package/webapp install flows, monitor layout (hyprmon exists — integrated via M11), audio/network/bluetooth (wiremix/impala/bluetui exist), plymouth/SDDM boot theming (theme-owned; risky+sudo), multi-machine sync, image *editing* (filters — Aether's turf, reachable via M11), non-Omarchy distros.

---

## 16. Open questions for your review

1. **Name.** `omarchy-studio` is descriptive; alternatives: `loom` ("weaves your rice"), `atelier`, `omarchy-forge`, `rice` (`omarchy-rice`). Preference?
2. **TUI-first confirmed?** The PRD bets on TUI (fits Omarchy's floating-terminal culture, fastest to MVP, works over SSH). GUI later via the same core. If you'd rather have GUI-first (Tauri/Svelte like Aether, or GPUI like Omarchist), M-scopes hold but timeline roughly doubles and §10.2 changes.
3. **Rust/ratatui vs Go/bubbletea?** Recommendation is Rust (lossless `toml_edit`, `ratatui` maturity, AUR-friendly static bin). Veto?
4. **Keybind override UX:** comfortable with `unbind` + `bind` in user `bindings.conf` as the override mechanism (the only clean one), including surfacing Omarchy default binds as "overridable" rather than "editable"?
5. **Waybar style scope:** geometry/spacing/font only (theme keeps owning colors) — enough, or do you want per-module color overrides too (would add a Studio-managed CSS layer after the theme import)?
6. **Your video-wallpaper patch:** should Studio absorb it properly (M2 ships the mpvpaper-rs/swww flow in user-space + we PR it upstream), so your `$OMARCHY_PATH` checkout can go clean again?
7. **Animation preset list:** happy with the 8 proposed (§FR4.3), or do you want community-imitating presets (end-4 style, Hyprland-wiki classics) in v1?
8. **Profiles (FR9.3):** P1 as specced, or cut to post-1.0 to ship faster?
9. **Extraction depth at v0.6:** port all 8 of Aether's modes up front, or ship 3 (Normal, Muted, Material) and grow from feedback? (All 8 ≈ one extra week.)
10. **Wallhaven account features:** anonymous browsing (SFW categories, default rate limits) enough for v0.6, or API-key config (purity tiers, favorites) from day one?
11. **Milestone order:** v0.6 (Wallpaper & Palette Lab) is currently *after* the behavior modules. If the one-stop visual journey matters most to you day-to-day, we can swap it with v0.4 (Waybar) — say the word.

---

## Appendix A — Environment snapshot (this machine, 2026-07-04)

Omarchy 3.8.2 (git master, **dirty**: video-wallpaper patch in `bin/`), Hyprland 0.55.2, Waybar 0.15.0, mako 1.11.0, walker 2.16.2 + elephant, swayosd 0.3.1, hyprlock 0.9.5, hypridle 0.1.7, alacritty 0.17 + ghostty 1.3.1 + kitty, swaybg 1.2.2, awww 0.12.1, btop 1.4.7. Active theme `osaka-jade` (user overlay adds `backgrounds/` + `videos/`); wallpaper `Angel.jpg`; `themed/`, `hooks/`, `extensions/` currently empty (greenfield for Studio's integration).

## Appendix B — Integration cheat sheet (commands Studio calls)

`omarchy-theme-set <name>` · `omarchy-theme-refresh` · `omarchy-theme-list/current/install/remove/update` · `omarchy-theme-bg-set/next` · `omarchy-restart-{waybar,mako,swayosd,hypridle,walker,terminal,btop}` · `hyprctl {keyword,--batch,reload,configerrors,binds -j,dispatch}` · `makoctl {reload,mode}` · `notify-send` · `gsettings` (read-only checks) · `omarchy-hook-install` · `omarchy-menu <substring>` · `omarchy-launch-walker --dmenu`

Files written (all user-owned): `~/.config/hypr/{bindings,looknfeel,hypridle,hyprlock,autostart}.conf` · `~/.config/waybar/{config.jsonc,style.css,scripts/}` · `~/.config/swayosd/{config.toml,style.css}` · `~/.config/omarchy/{themes/*,themed/*.tpl,extensions/menu.sh,hooks/*.d/*,backgrounds/*}` · `~/.config/elephant/menus/omarchy_studio.lua` · `~/.local/state/omarchy-studio/*`

## Appendix C — Sources

- Local source audit: `~/.local/share/omarchy` @ v3.8.2 (`bin/omarchy-theme-set`, `bin/omarchy-theme-set-templates`, `bin/omarchy-menu`, `bin/omarchy-hook*`, `default/themed/*.tpl`, `default/hypr/*`, `default/elephant/*.lua`, `AGENTS.md`, `migrations/`)
- [The Omarchy Manual — Themes](https://learn.omacom.io/2/the-omarchy-manual/52/themes) · [Making your own theme](https://learn.omacom.io/2/the-omarchy-manual/92/making-your-own-theme) · [Omarchy CLI](https://learn.omacom.io/2/the-omarchy-manual/115/omarchy-cli)
- [Aether](https://github.com/bjarneo/aether) · [Omarchist](https://github.com/tahayvr/omarchist) · [awesome-omarchy](https://github.com/aorumbayev/awesome-omarchy) · [DHH on colors.toml/Aether](https://x.com/dhh/status/2020090907744432644)
- Integration targets: [wallhaven API](https://wallhaven.cc/help/api) · [tinted-theming (Base16/Base24 schemes)](https://github.com/tinted-theming/schemes) · [matugen](https://github.com/InioX/matugen)
