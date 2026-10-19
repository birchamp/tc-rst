#!/usr/bin/env bash
# Builds the spike's client for the browser into target/spike-loopback/ui, the folder
# `tc4 serve --ui` serves. Nothing generated is committed.
set -euo pipefail
root=$(git rev-parse --show-toplevel)
out="$root/target/spike-loopback"
cargo build --manifest-path "$root/spikes/loopback/Cargo.toml" \
  --target wasm32-unknown-unknown --release --offline -j 2 --target-dir "$out"
mkdir -p "$out/ui/ui"
wasm-bindgen --target web --no-typescript --out-name loopback --out-dir "$out/ui/ui" \
  "$out/wasm32-unknown-unknown/release/loopback_spike.wasm"
cp "$root/spikes/loopback/page/index.html" "$out/ui/index.html"
cp "$root/spikes/loopback/page/ui/main.js" "$out/ui/ui/main.js"
ls -l "$out/ui/ui"
