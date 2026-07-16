#!/usr/bin/env bash
# Re-scrape the Omarchy extra-themes directory into data/community-themes.tsv.
# Run from the repo root; review the diff before committing — the manual's
# markup is informal and entries occasionally move or vanish.
set -euo pipefail

URL="https://learn.omacom.io/2/the-omarchy-manual/90/extra-themes"
OUT="$(dirname "$0")/../data/community-themes.tsv"

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
