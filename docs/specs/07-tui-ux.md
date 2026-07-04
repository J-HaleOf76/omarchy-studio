# Spec 07 — TUI & UX

ratatui + crossterm. The TUI is a thin renderer over `studio-core`; no domain logic in `tui/`.

## 1. Application shell

```
┌ Omarchy Studio ─────────────────────────── <theme> · omarchy <ver> ┐
│ rail (11 modules)  │ content pane                                  │
│                    │                                               │
├────────────────────┴───────────────────────────────────────────────┤
│ pending-changes tray (when non-empty)                              │
│ keybar: context actions · jargon strip (selected field's config)   │
└─────────────────────────────────────────────────────────────────────┘
```

Rail order: Themes · Wallpapers · Keybinds · Look&Feel · Animations · Waybar · Notifications/OSD · Lock&Idle · Snapshots · Integrations · Doctor. (Settings & About behind `,` / `F1`.)

## 2. Event & state model

- Elm-ish: `enum Msg` → `update(&mut Model, Msg) -> Vec<Effect>` → effects run on worker threads → `Msg` back via mpsc. Render at need (event-driven; 30 fps cap during animations like the bezier preview).
- One `Screen` trait per rail entry: `handle(Msg)`, `view(Frame)`, `keymap() -> &[KeyHint]` (feeds the keybar and `?` overlay automatically — keymaps are data, so help can't drift).
- Global keys: `1–9/0` jump rail · `Tab/S-Tab` section cycle · `/` command palette · `e` open raw file at line · `u` undo last apply · `a` apply pending tray · `?` help · `q`/`Esc` back/quit (with pending-changes guard).

## 3. Command palette (`/`)

Fuzzy index built at startup from: every `Setting.doc` + id, every screen, every action, every keybind chord/action, every theme name. Selecting jumps to the owning screen with the field focused. This is the discoverability backbone (PRD §2.2) — index build must stay < 50 ms.

## 4. Widgets (beyond stock ratatui)

| Widget | Used by | Notes |
|---|---|---|
| `Stepper` | numeric settings | h/l or ±; shift = big step; live-apply hook |
| `ColorField` | palette, borders | truecolor swatch + hex entry + HSL sliders popup |
| `ChordCapture` | keybinds | kitty keyboard protocol; fallback picker (spec 05 §1.3) |
| `Lanes` | waybar | 3-column reorderable lists; J/K move item, H/L move across lanes |
| `BezierCanvas` | animations | braille canvas, handle drag, animated dot |
| `SwatchStrip` | themes browser | 21-slot truecolor strip; degrades to 8-color |
| `ImageCell` | previews/wallhaven | ratatui-image; hidden when `TermGraphics::None` (layout reflows to text rows) |
| `DepBanner` | everywhere | the FR11.6 guidance banner (spec 06 §3) — one implementation, reused |
| `RevertCountdown` | risky applies | modal 10 s bar: Enter keep · Esc/timeout revert |

## 5. Studio themes itself

At startup read `current/theme/colors.toml` → map to ratatui style table (bg stays terminal-default for transparency; accent = `accent`, borders `color8`, warnings `color3`, errors `color1`). On `theme-set` hook during runtime (file watch on `current/theme.name`) → live restyle. Studio must look native in all 19 stock themes — CI screenshot grid (spec 09 §4) renders the shell in each palette.

## 6. Omarchy menu shape (with spec 02 §3)

`Style → Studio` submenu entries → deep links: Studio (home) · Themes · Keybinds · Look & Feel · Waybar · Notifications. Each = `omarchy-studio <screen>` in the floating terminal. `omarchy-studio` with a screen arg boots directly into that screen with `BACK_TO_EXIT` semantics (Esc quits — mirrors omarchy-menu behavior).

## 7. Copy rules (from PRD §7.5 + artifact-quality bar)

- Field labels are outcomes, not keys: "Space between windows", jargon strip shows `general:gaps_in = 5`.
- Errors: *what happened → why → next step*, ≤ 3 lines, no exception text (that's `--debug`).
- Toasts confirm with the same verb the button used ("Applied — u to undo", "Reverted").
- Empty states teach: empty user themes → "Fork a built-in with `f`, or craft one from a wallpaper with `t`."
