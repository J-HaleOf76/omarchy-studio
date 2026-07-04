# Spec 00 — Overview & Decision Log

This directory turns `docs/PRD.md` (the *what/why*) into build specs (the *how*). Read this file first; each spec stands alone after that.

## Document map

| Spec | Covers | PRD modules | Primary milestone |
|---|---|---|---|
| [01-architecture](01-architecture.md) | Crate layout, core engine, apply pipeline, error model, deps | — | v0.1 |
| [02-omarchy-integration](02-omarchy-integration.md) | Paths, commands, probing, menu/hooks/elephant install, update survival | M10 | v0.1 |
| [03-config-engine](03-config-engine.md) | Parsers/writers per dialect, managed blocks, atomic writes, snapshots/undo | M9 | v0.1 |
| [04-modules-visual](04-modules-visual.md) | Themes, palette editor, extraction, wallpapers, wallhaven, wizard | M1, M2 | v0.1 + v0.6 |
| [05-modules-behavior](05-modules-behavior.md) | Keybinds, look&feel, animations, waybar, notifications, OSD, lock/idle | M3–M8 | v0.2–v0.5 |
| [06-integrations-deps](06-integrations-deps.md) | Tool registry, dependency probing, guided install, degradation matrix | M11 | v0.1 (probing) + v0.6 |
| [07-tui-ux](07-tui-ux.md) | Screen map, navigation, widgets, key model, Studio self-theming | — | v0.1 |
| [08-cli](08-cli.md) | Command tree, JSON output, exit codes | M10 | v0.1 |
| [09-testing](09-testing.md) | Unit/golden/round-trip/e2e strategy, CI | — | v0.1 |
| [10-packaging](10-packaging.md) | AUR, releases, versioning, first-run wizard, uninstall | — | v1.0 |

`ROADMAP.md` (repo root) breaks the milestones into issue-sized tasks referencing these specs.

## Decisions made (with PRD reference)

| # | Decision | Rationale |
|---|---|---|
| D1 | **Rust**, single binary, workspace of 2 crates | Lossless `toml_edit`, ratatui maturity, static bin for AUR (PRD §10.2) |
| D2 | **TUI-first** (ratatui); GUI later reuses `studio-core` | Omarchy floating-terminal culture; fastest to MVP (PRD §16 Q2) |
| D3 | No async runtime; threads + channels | Only network use is wallhaven (blocking `ureq` in worker thread); avoids tokio weight in a TUI |
| D4 | Snapshots = plain git repo under `~/.local/state/omarchy-studio/history` | Diff/restore for free; inspectable by the user with git itself (PRD FR9.1) |
| D5 | Never write to `$OMARCHY_PATH`; only user-owned surfaces | Update survival (PRD §3.6) |
| D6 | Live preview via `hyprctl keyword`, persistence via file write + reload | File truth wins on Esc/exit (PRD FR4.2) |
| D7 | Schemas (field→key→widget→doc) are embedded **data**, not code | Version-gating and community contribution (PRD §10.4) |
| D8 | Dependency/tool registry is a TOML data file compiled into the binary, overridable at `~/.config/omarchy-studio/integrations.toml` | FR11.5/FR11.6 |
| D9 | Palette extraction ported from Aether's MIT Go engine (attributed), not shelled out | Aether is a GUI, not a CLI; algorithm is the portable part (PRD §4) |
| D10 | Wallhaven via public JSON API, anonymous by default, optional API key | PRD FR2.4, §16 Q10 pending |

## Open decisions (blocked on Ariz — PRD §16)

- **Q1 name** — everything uses `omarchy-studio`; rename = crate/binary/menu-label change, cheap before v0.1 ships.
- **Q7 animation preset list** — 8 presets specced in 05; trim/extend freely.
- **Q9 extraction depth** — specs assume 3 modes at v0.6 (Normal, Muted, Material), the rest behind a feature flag until ported.
- **Q10 wallhaven API key day-one?** — specced as optional config; UI hides account features until a key exists.
- **Q11 milestone order** — ROADMAP keeps v0.6 after v0.4/v0.5; swap is a one-line reorder.

## Conventions for these specs

- File paths are literal for Omarchy 3.8.2 (verified on the reference machine 2026-07-04). Anything version-sensitive is marked **[gate]** and must go through the capability probe (spec 02 §5).
- `FRx.y` references point at `docs/PRD.md` §8.
- Code sketches are illustrative signatures, not commitments; types live in `studio-core`.
