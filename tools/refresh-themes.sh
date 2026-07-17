#!/usr/bin/env bash
# Re-scrape the Omarchy extra-themes directory into the studio-core crate.
# The catalog lives inside the crate (not at the repo root) because
# include_str! can't reach outside a package cargo publish would ship.
# Run from the repo root; review the diff before committing — the manual's
# markup is informal and entries occasionally move or vanish.
set -euo pipefail

URL="https://learn.omacom.io/2/the-omarchy-manual/90/extra-themes"
OUT="$(dirname "$0")/../crates/studio-core/data/community-themes.tsv"

curl -fsSL "$URL" |
  # Anchor lines look like: <a href="https://github.com/owner/repo">Name</a>
  grep -oE '<a[^>]+href="https://github\.com/[^"/]+/[^"/]+/?"[^>]*>[^<]+</a>' |
  sed -E 's|.*href="https://github\.com/([^"/]+/[^"/]+)/?".*>([^<]+)</a>|\2\t\1|' |
  # Drop nav/footer links that point at GitHub but are not themes.
  grep -viE '	(basecamp|omarchy-org)/' |
  sort -f -u >"$OUT.new"

if [ ! -s "$OUT.new" ]; then
  echo "scrape produced nothing — page layout changed?" >&2
  rm -f "$OUT.new"
  exit 1
fi

mv "$OUT.new" "$OUT"
echo "wrote $(wc -l <"$OUT") themes to $OUT"
