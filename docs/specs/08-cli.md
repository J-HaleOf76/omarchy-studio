# Spec 08 — CLI

`omarchy-studio` with no args → TUI. With subcommands → headless CLI over the same `studio-core` ops (FR10.4). clap derive; global flags: `--json`, `--dry-run` (print the ApplyPlan; write nothing), `--debug`, `--yes` (skip confirms — except guided installs, which always confirm).

## Command tree (v1.0 surface; milestones add branches incrementally)

```
omarchy-studio
├── <screen>                     # themes|wallpapers|keybinds|looknfeel|animations|
│                                # waybar|notify|lockidle|snapshots|integrations — open TUI there
├── theme
│   ├── list [--json]
│   ├── current
│   ├── apply <slug>             # wraps omarchy-theme-set
│   ├── new <slug> [--from <slug>|--from-image <path> [--mode <m>] [--light]]
│   ├── edit <slug> set <color-key> <hex>       # palette mutation + refresh
│   └── export <slug> [--dir <path>] [--gh]
├── wallpaper
│   ├── list | set <path> | next
│   └── wallhaven search <query> [--color <hex>] [--ratio 16x9] [--top] [--download <n>]
├── keybind
│   ├── list [--source user|default|all] [--json]
│   ├── add --chord "SUPER, B" (--exec <cmd> | --dispatch <d> [--arg <a>]) [--desc <s>]
│   ├── remove --chord <c> | disable --chord <c> | reset --chord <c>
│   └── check --chord <c>        # conflict probe, exit 1 if taken
├── looknfeel
│   ├── get <id> | set <id> <value>              # id = schema path, e.g. blur.size
│   └── preset list|try|apply <name>             # animation presets
├── waybar
│   ├── modules [--json] | add <module> [--lane left|center|right] [--pos <n>]
│   ├── remove <module> | move <module> --lane <l> --pos <n>
│   └── set <module>.<key> <value>
├── notify set <id> <value> | rule add … | test [urgency]
├── snapshot list | show <id> | restore <id> | undo
├── doctor [--deps] [--quiet] [--json]           # drift, clobbers, deps, dirty $OMARCHY_PATH
├── install-integration | uninstall              # menu block, hooks, elephant provider (FR10.1/10.5/10.6)
└── about                                        # version, licenses/attribution, capability probe dump
```

## Conventions

- Exit codes: 0 ok · 1 domain failure (conflict, verify-failed-and-rolled-back) · 2 usage · 3 missing dependency (stderr carries the guidance text; `--json` carries the DepReport) · 4 omarchy mismatch.
- `--json` output shapes live beside the schemas (`data/schemas/json-output/*.json` examples) and are covered by golden tests — they are API, scripts will depend on them.
- Every mutating verb prints the snapshot id it created: `applied (snapshot 41) — undo: omarchy-studio snapshot restore 40`.
- `set` accepts human values where the schema defines them (`on/off`, `#hex`, `1.5s`) and normalizes.
