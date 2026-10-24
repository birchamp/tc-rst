#!/usr/bin/env bash
# Copy tC4's design-system CSS, fonts and images, its app-level `src/ui.css`, its interface
# catalogs and its LICENSE (and COPYING, which tC4 dropped with MIT in #838; About and License show them) into crates/tc-ui/assets, byte for byte, from one commit of a tC4 checkout, and write
# the list of what was copied to crates/tc-ui/vendored-from-tc4.txt. `cargo test -p tc-ui`
# checks every file under assets/ against that list (tests/vendored_assets.rs).
#
#   crates/tc-ui/vendor-tc4.sh /path/to/translationcore4 [COMMIT]
#
# COMMIT defaults to the checkout's HEAD. The files are read from the commit, not from the
# working tree, so a local change in the checkout never reaches the rebuild.
set -euo pipefail

HERE="$(cd "$(dirname "$0")" && pwd)"
TC4="${1:?usage: vendor-tc4.sh TC4_CHECKOUT [COMMIT]}"
COMMIT="$(git -C "$TC4" rev-parse --verify "${2:-HEAD}^{commit}")"
ASSETS="$HERE/assets"
LIST="$HERE/vendored-from-tc4.txt"

# tC4 path prefix -> folder under assets/.
asset_path() {
  case "$1" in
    src/ds/*) echo "ds/${1#src/ds/}" ;;
    src/i18n/*) echo "i18n/${1#src/i18n/}" ;;
    src/ui.css) echo "ui.css" ;;
    public/assets/*) echo "images/${1#public/assets/}" ;;
    LICENSE | COPYING) echo "about/$1" ;;
    *) echo "no place for $1" >&2; exit 1 ;;
  esac
}

entries="$(
  git -C "$TC4" ls-tree -r "$COMMIT" -- src/ds/styles.css src/ds/tokens src/ds/assets src/ui.css \
    public/assets/translationcore-logo.png public/assets/logo-mark-color.png LICENSE COPYING
  git -C "$TC4" ls-tree -r "$COMMIT" -- src/i18n | grep '\.json$'
)"

rm -rf "$ASSETS/ds" "$ASSETS/i18n" "$ASSETS/images" "$ASSETS/ui.css" "$ASSETS/about"
{
  echo "# tC4 files in crates/tc-ui/assets, copied byte for byte by vendor-tc4.sh."
  echo "# Each line: the file's git blob id in tC4, then its path in tC4."
  echo "commit $COMMIT"
  while read -r _mode type blob path; do
    [ "$type" = blob ] || continue
    dest="$ASSETS/$(asset_path "$path")"
    mkdir -p "$(dirname "$dest")"
    git -C "$TC4" cat-file blob "$blob" > "$dest"
    echo "$blob $path"
  done <<< "$entries"
} > "$LIST"
echo "copied $(grep -c '^[0-9a-f]\{40\} ' "$LIST") files from tC4 $COMMIT"
