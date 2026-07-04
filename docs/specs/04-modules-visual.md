# Spec 04 — Visual Modules: Themes, Palette, Extraction, Wallpapers, Wallhaven

Covers PRD M1 (Theme Studio) and M2 (Wallpapers & Discovery). Domain logic in `studio-core::modules::{themes,wallpapers,extraction}`.

## 1. Theme model

```rust
pub struct Theme {
    pub slug: String,                  // "osaka-jade"
    pub origin: ThemeOrigin,           // System | User | Overlay (user dir shadowing system slug)
    pub palette: Palette,              // parsed colors.toml (21 keys)
    pub light: bool,                   // light.mode marker present
    pub extras: ThemeExtras,           // icons.theme, chromium.theme, vscode.json, neovim.lua, keyboard.rgb, btop.theme
    pub backgrounds: Vec<PathBuf>,     // images + videos across the 4 merged dirs
    pub preview: Option<PathBuf>,
}
```

- **List** = union of system + user dirs (dedup by slug, user wins for display, `Overlay` flagged) — same semantics as `omarchy-theme-list` + the elephant picker.
- **Apply** = `omarchy-theme-set <slug>` — never reimplemented.
- **Edit safety:** system themes are read-only; editing one forks it: create `~/.config/omarchy/themes/<slug>/` and write only changed files (leveraging Omarchy's per-file overlay), or full fork under a new slug — user chooses ("Override Tokyo Night" vs "New theme based on Tokyo Night").

## 2. Palette editor (FR1.2/1.3)

The 21 `colors.toml` keys as rows: swatch (truecolor block) + hex field + HSL sliders. Derived helpers: lighten/darken ±5 %, complement, copy-from-slot. Contrast panel: fg/bg, accent/bg WCAG ratios with pass/fail chips; ANSI strip renders `color0–15` as a mock terminal line.

**Live preview loop:** edits debounce 150 ms → write palette to a scratch copy of the theme in `~/.config/omarchy/themes/.studio-preview/` → `OMARCHY_THEME_SKIP_BACKGROUND=1 omarchy-theme-set .studio-preview` → desktop repaints. On save: write real theme, `omarchy-theme-set <real slug>`, delete scratch. On cancel: `omarchy-theme-set <previous slug>` (recorded on entry). The scratch slug is hidden from browse lists and cleaned on startup if a crash orphaned it.

Note: theme-set fires `theme-set` hooks each preview tick — acceptable (hooks are cheap notifications) but Studio sets `OMARCHY_STUDIO_PREVIEW=1` in the env so our own hook (spec 02 §4) can no-op during previews.

## 3. Template coverage manager (FR1.7)

Table = union of `default/themed/*.tpl` (built-in) and `~/.config/omarchy/themed/*.tpl` (user), columns: app, source (built-in / user override / user addition), output file, "current theme ships verbatim file?" (which beats the template). Actions: create override (copies built-in as starting point), edit (textarea with `{{ placeholder }}` autocomplete from palette keys + `_strip`/`_rgb` suffixes), delete override, apply → `omarchy-theme-refresh`. Starter catalog: `data/templates/*.tpl` shipped for apps Omarchy doesn't cover (zed, spicetify, discord/vesktop) — installing copies into `themed/`.

## 4. Palette extraction (FR1.8, D9)

Port of Aether's MIT Go engine (attributed in-code and in `--about`): median-cut over an RGB histogram → 16-slot ANSI mapping + accent/bg/fg selection, then per-mode post-transforms in HSL space:

| Mode | Transform (sketch) | v0.6 |
|---|---|---|
| Normal | raw median-cut, luminance-ordered | ✔ |
| Muted | clamp saturation ≤ 0.45 | ✔ |
| Material | tonal palette from dominant hue (matugen-style; native fallback for FR11.3) | ✔ |
| Monochromatic / Analogous / Pastel / Colorful / Bright | hue-lock / hue-neighborhood / S↓L↑ / S↑ / L↑ | behind `--features all-modes` until ported (Q9) |

Common post-pass: enforce WCAG ≥ 4.5 fg/bg (nudge L), derive `selection_*`/`cursor`, pick accent = most-saturated mid-luminance cluster. Dark/light bias flag decides bg polarity and sets `light.mode`. Pure function `extract(img, mode, bias) -> Palette` — trivially golden-tested against Aether's outputs for the same images (spec 09).

## 5. Wallpapers (M2)

Browser lists the 4 merged sources in Omarchy's own priority order (`~/.config/omarchy/backgrounds/<theme>/` → `current/theme/backgrounds/` → user theme `backgrounds/` → `videos/`), with badges for source and type (image/gif/video). Actions: set (`omarchy-theme-bg-set`), next, add (copy into user backgrounds dir for the theme), remove (user files only). Video entries **[gate]** `video_wallpapers` capability → else guided-install banner for `mpvpaper-rs` (spec 06).

## 6. Wallhaven browser (FR2.4)

- Client: `ureq` + serde against `https://wallhaven.cc/api/v1/search`; params: `q`, `categories` (general/anime/people bitmask), `purity` (SFW only unless API key present), `sorting` (relevance/toplist/random/views), `atleast` (auto-filled from current monitor res via `hyprctl monitors -j`), `ratios`, `colors` (nearest wallhaven color to any palette slot — "match my accent").
- Rate handling: 45 req/min anonymous; client enforces client-side budget + exponential backoff on 429; thumbnails (`thumbs.large`) fetched by a worker pool of 4, LRU-cached (spec 01 §7).
- Grid via `ratatui-image` (kitty/sixel); `None` graphics → list view with resolution/color-swatch text and `o` = open in `imv` (present on Omarchy).
- Download → `~/.config/omarchy/backgrounds/<theme>/wallhaven-<id>.<ext>` (or into wizard, §7). Full-image fetch shows progress; API key (optional, `config.toml`) unlocks purity tiers — the purity selector is greyed with a "requires wallhaven API key — add it in Settings" note otherwise (never silently absent), per FR11.6 UX.
- Offline / request failure → banner "wallhaven unreachable — check your connection; local features unaffected", cached thumbs still shown.

## 7. "Craft theme from this wallpaper" wizard (FR2.5)

State machine, enterable from any image (`t`) or Themes → New → From wallpaper:

```
PickImage(file|wallhaven) → Extract(mode, bias; live re-run on change)
  → Refine(palette editor §2, embedded, live desktop preview §2 loop)
  → Meta(name, light-mode confirm, icon theme suggestion by hue)
  → Materialize → Apply? → Done
```

Materialize writes `~/.config/omarchy/themes/<slug>/`: `colors.toml`, `backgrounds/<image>`, `preview.png` (compose: wallpaper thumb + palette strip; a true screenshot preview is offered post-apply via `omarchy capture screenshot` flow), optional `light.mode`. Result is a 100 % standard theme dir — shareable via FR1.6 export immediately.

## 8. Export & share (FR1.6)

`themes export <slug>`: scaffold `omarchy-<slug>-theme/` (naming convention required by `omarchy-theme-install`'s slug derivation): theme files + README.md (template with preview embed + install instructions) + LICENSE picker. `git init` + commit; `gh repo create --public --push` offered **only if** `gh` present & authed (else print the manual commands — FR11.7 fallback). Never pushes without explicit confirm (outward-facing action).
