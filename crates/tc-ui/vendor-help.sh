#!/usr/bin/env bash
# Copy the help pages (tc-website's `tc-help/`) into crates/tc-ui/help, byte for byte, from one
# commit of tc-website, and write the list of what was copied to crates/tc-ui/help-pages.txt
# (APP-DESIGN AD29). `cargo test -p tc-ui` checks every file under help/ against that list
# (tests/help_pages.rs). build-app.sh copies help/ beside the UI, where `tc4 serve` serves
# `/help/*`.
#
#   crates/tc-ui/vendor-help.sh [REV]
#
# REV defaults to the pin of tC4's scripts/fetch-help.mjs at d310bf3, the rebuild's reference.
# The fetch is tC4's: sparse and shallow, only the blobs of `tc-help/` at REV (about 21 MB). It
# needs the network; tests never run it.
set -euo pipefail

HERE="$(cd "$(dirname "$0")" && pwd)"
ROOT="$(cd "$HERE/../.." && pwd)"
REPO=https://github.com/unfoldingWord/tc-website.git
REV="${1:-cdfdf9b7d534e7574beb3fcf44678973cdc5f9d7}"
OUT="$HERE/help"
LIST="$HERE/help-pages.txt"

# Inside the target folder, so nothing is left in /tmp and a full disk fails the run.
TMP="$ROOT/target/vendor-help"
rm -rf "$TMP"
mkdir -p "$TMP"
trap 'rm -rf "$TMP"' EXIT
git -C "$TMP" init -q
git -C "$TMP" config core.autocrlf false
git -C "$TMP" remote add origin "$REPO"
git -C "$TMP" sparse-checkout set --no-cone /tc-help/
git -C "$TMP" fetch -q --depth 1 --filter=blob:none origin "$REV"
git -C "$TMP" checkout -q FETCH_HEAD
HEAD="$(git -C "$TMP" rev-parse HEAD)"
if [ "$HEAD" != "$REV" ]; then
  echo "fetched $HEAD, not $REV" >&2
  exit 1
fi
if [ ! -f "$TMP/tc-help/index.html" ]; then
  echo "tc-website $REV has no tc-help/index.html" >&2
  exit 1
fi

rm -rf "$OUT"
cp -a "$TMP/tc-help" "$OUT"
{
  echo "# tc-website's help pages in crates/tc-ui/help, copied byte for byte by vendor-help.sh (AD29)."
  echo "# Each line: the file's git blob id in tc-website, then its path below tc-help/."
  echo "repository $REPO"
  echo "commit $REV"
  echo "# The pin of tC4's scripts/fetch-help.mjs at d310bf3, the rebuild's reference. Licence (Q-19):"
  echo "# tc-website holds no licence file at this commit; tC4 4.0 ships these pages in its"
  echo "# GPL-2.0-or-later build. The owner was asked to confirm (APP-DESIGN Q-19)."
  git -C "$TMP" ls-tree -r HEAD tc-help | while read -r _mode type blob path; do
    [ "$type" = blob ] || continue
    echo "$blob ${path#tc-help/}"
  done
} > "$LIST"
echo "copied $(grep -c '^[0-9a-f]\{40\} ' "$LIST") help files from tc-website $REV"
