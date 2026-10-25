# The Electronite shell

The desktop app's main process: it keeps one running copy, starts and supervises `tc4`, gives
the browser's own requests the token, mirrors typed text to `drafts-buffer.log`, asks before
closing over unsaved text, and answers `window.tc4Shell`. The interface is
`docs/contracts/SHELL.md` version 1.

| File | What |
|---|---|
| `lib.cjs` | The pure decisions: the lifecycle machine (`State`, `TRANSITIONS`, `next()`), the arguments, the address line, the token filter, links, window options, the menus |
| `mirror.cjs` | The one writer of `drafts-buffer.log`, and its reduction (`current_lines`) |
| `bridge.cjs` | `BRIDGE`, one entry per member of `docs/contracts/shell-bridge.json` |
| `main.cjs` | The executor: `dispatch()` runs the machine's actions against Electronite |
| `preload.cjs` | `window.tc4Shell` in the page |

Run it:

```bash
electron shell/main.cjs --tc4 <exe> --ui <dir> --app-data <dir> [--bundle <dir>] [-- <more tc4 arguments>]
```

The shell writes `shell.log` in the app-data folder, with every token taken out.

## Tests

The unit tests need only Node 22:

```bash
node --test 'shell/test/*.test.cjs'
```

`SHELL_MODEL_SEED=<n>` runs the two model tests from a given seed; a failure prints its seed.

The end-to-end tests run the real shell in Electronite v37.1.0-graphite over a test build of
`tc4`, driven by Playwright (`npm ci` in `parity/tc4-baseline`, or `PLAYWRIGHT_MODULE`):

```bash
cargo build -p tc4 --features test-hooks,fixture-backend
crates/tc-ui/build-app.sh --out target/tc-ui-app
crates/tc-ui/build-app.sh --test-hooks --out target/tc-ui-app-hooks
cargo run -p tc-app --example bundle_store -- --from bundle/en --only en_obs-tn --out target/shell-bundle
TC4_ELECTRON=<electronite>/electron TC4_BIN=target/debug/tc4 TC4_UI=target/tc-ui-app \
  TC4_UI_HOOKS=target/tc-ui-app-hooks TC4_BUNDLE=target/shell-bundle \
  xvfb-run -a node --test --test-concurrency=1 'shell/e2e/*.test.cjs'
```

Without a build a test is skipped and says why; `.github/workflows/shell.yml` fails a run with a
skip. Each system's frame test exists only on that system.
