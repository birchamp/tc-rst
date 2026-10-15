#!/usr/bin/env bash
# Build the design-system gallery: the tc-ui crate with the `gallery` feature, compiled to
# wasm32, bound with wasm-bindgen, and assembled with tC4's CSS, fonts and images into a static
# site that parity/ds-gallery/capture-rust.mjs serves.
#
#   crates/tc-ui/build-gallery.sh [--release] [--out DIR]
#
# --release builds with the `wasm-release` profile (size-optimised, LTO). The default is the dev
# profile, which builds faster and draws the same pixels. Output: target/tc-ui-gallery/ unless
# --out is given. Needs the wasm32-unknown-unknown target and wasm-bindgen 0.2.129 on PATH or in
# ~/.cargo/bin; no trunk, no cargo-leptos.
set -euo pipefail

HERE="$(cd "$(dirname "$0")" && pwd)"
ROOT="$(cd "$HERE/../.." && pwd)"
PROFILE=dev
DIR=debug
OUT="$ROOT/target/tc-ui-gallery"
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
  --profile "$PROFILE" --features gallery --manifest-path "$ROOT/Cargo.toml"

rm -rf "$OUT"
mkdir -p "$OUT"
"$BINDGEN" --target web --no-typescript --out-dir "$OUT/pkg" \
  "$ROOT/target/wasm32-unknown-unknown/$DIR/tc_ui.wasm"
cp "$HERE/gallery/index.html" "$OUT/index.html"
cp -r "$HERE/assets" "$OUT/assets"
cp "$ROOT/parity/ds-gallery/gallery-spec.json" "$OUT/gallery-spec.json"
echo "gallery: $OUT ($(du -h "$OUT/pkg/tc_ui_bg.wasm" | cut -f1) wasm)"
