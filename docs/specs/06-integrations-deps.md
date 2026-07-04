# Spec 06 — Integrations Registry & Dependency Guidance

Covers PRD M11 + principle 7 ("never strand the user", FR11.6/11.7). Code: `studio-core::deps`. Data: `data/integrations.toml` (embedded), user overlay at `~/.config/omarchy-studio/integrations.toml` (merged, user wins by id).

## 1. Registry format

One TOML file describes **dependencies** (things features need) and **tools** (companions we detect and interop with). See `data/integrations.toml` for the real seed; shape:

```toml
[deps.mpvpaper-rs]
kind = "binary"                 # binary | package | terminal-graphics | network | service
probe = "mpvpaper-rs"           # command -v target (binary), pacman -Qq name (package), …
package = "mpvpaper-rs"         # what to install
repo = "aur"                    # official | aur  → chooses pacman vs yay/paru in guidance
reason = "Plays video wallpapers behind your desktop"
unlocks = ["wallpapers.video"]  # feature ids gated on this dep
fallback = "Video files are listed but greyed out; images and GIFs still work."

[tools.aether]
probe = "aether"
detect-also = ["~/.config/aether"]
homepage = "https://github.com/bjarneo/aether"
install = "yay -S aether"       # shown, never auto-run without confirm
actions = [
  { id = "open", label = "Open in Aether (image filters & advanced extraction)", exec = "aether" },
]
interop = "themes"              # produces standard theme dirs → zero conversion
```

## 2. Probing

`DepStatus = Present(version?) | Missing | Degraded(reason)` resolved by kind:

| kind | probe |
|---|---|
| binary | `command -v` (via `which` logic in-process; no shell) |
| package | `pacman -Qq <name>` exit 0 |
| terminal-graphics | kitty APC query / sixel DA1 response at TUI start (never cached — terminal can change) |
| network | lazy: first wallhaven request; failure flips status until retry |
| service | `systemctl --user is-active <unit>` |

Startup probes binaries/packages in one batch (< 20 ms, cached in-process only — install state must never go stale between screens: re-probe after any guided install and on screen entry for that screen's deps).

## 3. The guidance UX (FR11.6) — frontend contract

A feature whose `requires` deps aren't all `Present` renders **visible but disabled**:

```
│ ▒ Video wallpapers                                    [needs setup] │
│   Plays video wallpapers behind your desktop.                       │
│   Missing: mpvpaper-rs (AUR)                                        │
│   → i install now (runs: yay -S mpvpaper-rs)   c copy command       │
└─────────────────────────────────────────────────────────────────────┘
```

Rules:
1. The banner names the dep, the *reason* (from registry — user-language, not package-ese), and the **exact command**.
2. `i` = guided install: spawn the command in a visible floating terminal (`omarchy-launch-floating-terminal-with-presentation`-style) so pacman/yay prompts are the real, interactive ones — **Studio never wraps sudo silently and never auto-confirms**. On terminal exit → re-probe → banner clears or reports what's still missing.
3. `c` = copy command to clipboard (`wl-copy`; itself a registered dep with fallback "shown for manual copy").
4. No AUR helper present (no `yay`/`paru`) → command degrades to the manual `git clone …/makepkg -si` steps with a note, or `pacman` if the package is official.
5. First-run wizard shows one summary screen: required deps (all ship with Omarchy — expected all-green), optional deps grouped by feature, multi-select → sequential guided install.
6. `omarchy-studio doctor --deps` prints the same as a table (CLI form in spec 08); `--json` for scripts.

## 4. Degradation matrix (FR11.7)

Single source of truth = `unlocks`/`fallback` fields; this table is generated into docs by CI (`cargo xtask gen-deps-doc`) so it can't drift:

| Dep | Missing ⇒ | Features affected |
|---|---|---|
| kitty/sixel graphics | ASCII swatches + text lists; `o` opens image in imv | theme previews, wallhaven grid, wallpaper browser |
| network | offline banner; cached thumbnails only | wallhaven |
| mpvpaper-rs / swww | video/gif entries greyed + install banner | video wallpapers |
| yay/paru | install commands become manual steps | guided install itself |
| gh (authed) | export prints manual `git remote add` + push steps | theme share |
| matugen | native Material mode used instead | FR11.3 |
| git | snapshots disabled; every apply asks explicit confirm; loud banner | undo/history |
| wl-copy | "copy" actions show the text to copy manually | misc |

## 5. Tool interop actions (M11)

- Detected tools appear in **Integrations** screen: status, version, homepage, actions.
- `interop = "themes"` tools get automatic behavior: their output themes appear in Themes browser like any other (they're standard dirs — no code); Studio adds a provenance line in theme details when recognizable (e.g. Aether writes marker comments).
- Contextual actions surface where relevant (registry `actions[].context` optional field): e.g. wallpaper browser shows "Open in Aether" only if `tools.aether` present.
- Monitors/input: Studio ships **no** editor (PRD N7); the Setup rail entry shows hyprmon/omarchy-monitor-settings launch actions if installed, else Omarchy's own `omarchy-menu setup` deep-link. This is the M11 pattern for "we don't do it, but you're two keys away".

## 6. Attribution (FR11.4)

`omarchy-studio --about` and README credit ported algorithms (Aether extraction — MIT, bjarneo) with links; `data/integrations.toml` carries `license` fields for anything we port from.
