# Omarchy Studio

**The one-stop theming cockpit for [Omarchy](https://omarchy.org)** — wallpapers, palettes, themes, keybinds, animations, Waybar, notifications, lock/idle — all editable from a keyboard-driven TUI with live preview and one-key undo. No config-file jargon required (but the raw file is always one keystroke away).

> Status: **pre-alpha — specification & scaffold.** Nothing is usable yet.
> Working name `omarchy-studio`; final name TBD (PRD §16 Q1).

## Why

Omarchy's menu covers *picking* a theme; everything past that is hand-editing five config dialects across four directory trees, with no discoverability and no undo. The community solved colors six times over (Aether, Omarchist, tema, generators) — nobody built the control center for the *behavioral* half: keybinds, animations, bar layout, notification behavior. Studio does both halves, natively, and integrates with the tools that already exist.

## Design pillars

1. **Never fight Omarchy** — write only to user-owned files and sanctioned extension surfaces; apply through `omarchy-theme-set` / `omarchy-theme-refresh` / `omarchy-restart-*`.
2. **The file is still the truth** — comment-preserving edits; hand-edits and Studio edits coexist.
3. **Show, don't describe** — live preview via `hyprctl keyword`, with confirm-or-revert.
4. **Undo is sacred** — git-backed snapshots of every change Studio ever makes.
5. **Explain everything** — every field shows the config line it generates.
6. **Keyboard-first, menu-native** — launches from the Omarchy menu as a floating terminal.
7. **Never strand the user** — missing dependency ⇒ visible-but-disabled feature with the exact install command and a guided install, never a silent failure.

## Repository map

| Path | What |
|---|---|
| `docs/PRD.md` | Product requirements (v0.3) — the *what* and *why* |
| `docs/specs/` | Build specs — the *how* (start at `00-overview.md`) |
| `ROADMAP.md` | Milestones broken into issue-sized tasks |
| `crates/studio-core/` | Engine library: config model, parsers, apply pipeline, snapshots, Omarchy adapter |
| `crates/omarchy-studio/` | The binary: TUI + CLI frontends |
| `data/integrations.toml` | Declarative registry of external tools & dependencies (FR11.6/M11) |
| `data/animation-presets/` | Animation preset bundles shipped with Studio |

## Building

```bash
cargo check          # skeleton compiles with zero external deps (for now)
```

## License

MIT.
