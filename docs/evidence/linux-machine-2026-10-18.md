# The owner's Linux PC (2026-10-18)

The rebuild moved here from the cloud machine on 2026-10-18. Every measurement taken here names
this file.

| | |
|---|---|
| CPU | 12th Gen Intel Core i7-12700: 12 cores (8 performance, 4 efficient), 20 threads, up to 4.9 GHz |
| SHA instructions | yes (`sha_ni`) |
| AVX-512 | no (`avx2` only) |
| Memory | 61 GiB |
| Disks | the checkout on a Samsung 870 EVO SATA SSD (`/home`, ext4); a Samsung 980 PRO NVMe (`/storage`, btrfs) is unused |
| OS | Ubuntu 26.04.1 LTS, Linux 7.0.0-38, glibc 2.43 |
| Rust | 1.99.1 stable, with `wasm32-unknown-unknown` |
| Node | 22.23.3 |
| wasm-bindgen | 0.2.129 (`cargo install wasm-bindgen-cli --version 0.2.129 --locked`), for `crates/tc-ui/build-app.sh` |
| Playwright | 1.56.1, Chromium 141.0.7390.37 (build 1194), from Playwright's Ubuntu 24.04 build because 1.56.1 does not know 26.04 (`PLAYWRIGHT_HOST_PLATFORM_OVERRIDE=ubuntu24.04-x64`) |

## What this means for the numbers

- This CPU has SHA instructions, so it hashes about 6.5 times faster than the reference laptop
  (`docs/PERFORMANCE.md`, "Budgets"). A budget met here is not shown met on the reference laptop.
- Several agents build and test at once on this machine. Record the load (`uptime`) beside every
  timing, and rerun a miss once before calling it a miss.
- The parity baseline (`parity/tc4-baseline`) was recaptured here by unit A6b, and a second full
  capture at its merge into main reproduced all 62 screens byte for byte (the `fc-list`
  fingerprint in its `manifest.json` names this machine's 535 fonts). A capture on another
  machine is compared only after its font fingerprint matches.
- The baseline did not reproduce here: 57 of its 62 PNGs differ from the cloud machine's by
  fonts, and none of its DOM snapshots differ. Unit A6b recaptured it here and made every capture
  record its machine (docs/evidence/parity-harness-2026-10-18.md).
