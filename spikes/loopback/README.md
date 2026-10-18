# Loopback spike (unit A2)

A throwaway measurement, never shipped. It answers one question of APP-DESIGN §8.3 (AD18, A-14):
what does the aligner's click cost on `tc4 serve`'s own side, from a wasm `fetch` over loopback
to a parsed reply, and what does a header-adding interceptor in front of every request add?

- `src/lib.rs` is the client: it holds the event stream open and makes sequential `test/echo`
  calls (a 20 KB reply, the size of an aligner view), timing each from the request to the
  reply parsed with `tc-api`'s types.
- `run.mjs` starts `tc4 serve --test-hooks`, measures a Node baseline without a browser, then
  runs the passes in Chromium through Playwright: **plain** (the page sets `X-Tc-Token` itself,
  as the UI does) and **proxy** (the page sets none; Playwright's request interception adds it
  to every request, the way Electron's `onBeforeSendHeaders` does).
- Passes alternate, plain, proxy, proxy, plain, so drift on a shared machine hits both alike.

This is not Electronite and not Windows with Defender: G0 measured those
(`docs/evidence/electronite-spike-2026-10-17.md`). It is the server's own cost, with its real
security layer and routing, plus Chromium's `fetch` and wasm's parse.

## Run it

Build `tc4` in release with the hooks, build the client, then drive it. The two release
builds of `tc4` overwrite each other, so measure before building the plain one.

```sh
export CARGO_INCREMENTAL=0
cargo build --release -p tc4 --features test-hooks -j 2
spikes/loopback/build.sh
PLAYWRIGHT_MODULE=/home/user/tc-rst/parity/ds-gallery/node_modules/playwright \
  node spikes/loopback/run.mjs --tc4 target/release/tc4 --ui target/spike-loopback/ui \
  --clicks 500 --warmup 50 --size 20480 --passes plain,proxy,proxy,plain \
  --out docs/evidence/loopback-<date>/results.json
```

`build.sh` needs the `wasm32-unknown-unknown` target and `wasm-bindgen` 0.2.129 (the version
in `Cargo.lock`). `Cargo.lock` here is the workspace's, pruned to what the client uses.
