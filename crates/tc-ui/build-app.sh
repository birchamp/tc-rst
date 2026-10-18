#!/usr/bin/env bash
# Build the folder `tc4 serve --ui` serves: the tc-ui crate with the `app` feature compiled to
# wasm32 and bound with wasm-bindgen, beside tC4's CSS, fonts, images and catalogs and the
# page that loads them (APP-DESIGN 5.3, 6.9).
#
#   crates/tc-ui/build-app.sh [--release] [--out DIR]
#
# --release builds with the `wasm-release` profile (size-optimised, LTO); the default is the
# dev profile, which builds faster and runs the same code. Output: target/tc-ui-app/ unless
# --out is given:
#
#   index.html              the page, with the build's folder name in its links (no-store)
#   ui/<build>/             everything else; <build> is 16 hex digits of a SHA-256 over every
#                           file's path and bytes
#
# The folder is hashed, not each file, because tC4's CSS imports tokens and fonts by relative
# URL. Set TC4_BUILD_COMMIT for this script and for the build of `tc4` alike: the page reloads
# once when the server reports another commit (APP-DESIGN 3.10). Needs the wasm32-unknown-unknown
# target and wasm-bindgen 0.2.129 on PATH or in ~/.cargo/bin; no trunk, no cargo-leptos.
set -euo pipefail

HERE="$(cd "$(dirname "$0")" && pwd)"
ROOT="$(cd "$HERE/../.." && pwd)"
PROFILE=dev
DIR=debug
OUT="$ROOT/target/tc-ui-app"
while [ $# -gt 0 ]; do
  case "$1" in
    --release) PROFILE=wasm-release; DIR=wasm-release ;;
    --out) OUT="$2"; shift ;;
    *) echo "unknown option $1" >&2; exit 2 ;;
  esac
  shift
done

BINDGEN="$(command -v wasm-bindgen || echo "$HOME/.cargo/bin/wasm-bindgen")"
want="wasm-bindgen 0.2.129"
have="$("$BINDGEN" --version)"
if [ "$have" != "$want" ]; then
  echo "need $want (the crate pins wasm-bindgen =0.2.129), found $have" >&2
  exit 1
fi

cargo build -j "${CARGO_JOBS:-2}" -p tc-ui --lib --target wasm32-unknown-unknown \
  --profile "$PROFILE" --features app --manifest-path "$ROOT/Cargo.toml"

# Where cargo put the wasm: CARGO_TARGET_DIR or build.target-dir move it out of ./target.
TARGET_DIR="$(cargo metadata --format-version 1 --no-deps --manifest-path "$ROOT/Cargo.toml" |
  sed -n 's/.*"target_directory":"\([^"]*\)".*/\1/p')"
WASM="$TARGET_DIR/wasm32-unknown-unknown/$DIR/tc_ui.wasm"
if [ ! -f "$WASM" ]; then
  echo "cargo built no $WASM" >&2
  exit 1
fi

# Staged inside the target folder, so a full disk shows up as a failed build and nothing is
# left in /tmp.
STAGE="$TARGET_DIR/tc-ui-app-stage"
rm -rf "$STAGE"
mkdir -p "$STAGE"
"$BINDGEN" --target web --no-typescript --out-dir "$STAGE" "$WASM"
cp "$HERE/app/boot.js" "$HERE/assets/ui.css" "$STAGE/"
cp -r "$HERE/assets/ds" "$STAGE/ds"
cp -r "$HERE/assets/images" "$STAGE/images"
cp -r "$HERE/assets/i18n" "$STAGE/i18n"

BUILD="$(cd "$STAGE" && find . -type f -print0 | LC_ALL=C sort -z | xargs -0 sha256sum |
  sha256sum | cut -c1-16)"

rm -rf "$OUT"
mkdir -p "$OUT/ui"
mv "$STAGE" "$OUT/ui/$BUILD"
sed "s/@BUILD@/$BUILD/g" "$HERE/app/index.html" > "$OUT/index.html"
echo "app: $OUT (build $BUILD, $(du -h "$OUT/ui/$BUILD/tc_ui_bg.wasm" | cut -f1) wasm)"
