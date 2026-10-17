**Integration of the four branches into `integrate-w2`**

All four branches merged cleanly in the requested order. One workspace test fails: `every_fixture_verse_reads_apart_and_weaves_back` in tc-usfm. It already fails the same way on main (32f0949) and needs a tc-usfm design decision, so I left it for you (details at the end). Every other gate passes, and the design-system gallery is still at 0 mismatched pixels in all 719 cases. Nothing was pushed, and `claude/focused-ride-5uzj7p` is unchanged at 32f0949.

Worktree: `/home/user/tc-rst/.claude/worktrees/integrate-w2`

**Merge commits**
- bd5eb2b1abea4d5a1d5ea96d4b93ab7192aba419: Merge tc-check
- b546815598616af1e31c467fb78e3e28877091f9: Merge tc-align
- d3da72adf68f123c562e3a61a474a5e44aa19fba: Merge tc-resources
- f116f3dd14944ec7b06dfc9dd8df3b42e9bca241: Merge tc-ui

**Commits after the merges**
- b58655e: moves every dependency that two or more crates use into `[workspace.dependencies]`: axum, base64, md-5, reqwest, sha1, tokio, unicode-general-category, unicode-normalization, url and zip. Crates that need different features add their own on top of the workspace entry. md-5 was the only version split: tc-burrito used 0.11.0 and five other crates used 0.10.6. The workspace now pins 0.10.6, and tc-burrito builds and passes its 76 tests on it. The lockfile only drops md-5 0.11 and its digest 0.11 dependencies; no other version moved.
- edb9aa2: fixes what the merge broke. tc-usfm's fixture test reads every `.usfm` file under `fixtures/`. tc-align added `fixtures/align/span-source.usfm`, whose verse 1:96 has a `\zaln-s` with only `x-content`, and tc-usfm correctly refuses it. The test now expects exactly that one refusal (`InvalidSourceWord`) and fails if the refusal goes away. I left the fixture alone because it is generated from tC4.

**Integration branch head:** edb9aa284373453eb04a2d02360ff1440630e9f9

**Conflicts and how I resolved them**
- The only conflict was `Cargo.lock` in the tc-ui merge (that branch started from an older main). I kept the current side and ran `cargo metadata --offline`, which added the 89 packages tc-ui needs. All of them are the exact versions tc-ui's own lockfile had, and no existing entry changed. I did not run `cargo update` or `generate-lockfile`.
- These files merged without conflicts:
  - **`Cargo.toml`:** tc-align added md-5 and zip to the workspace dependencies; tc-ui added the `[profile.wasm-release]` section.
  - **CI and `AGENTS.md`:** tc-ui added its two wasm32 clippy steps to `.github/workflows/ci.yml` and a matching rule to `AGENTS.md`.
  - **`.gitignore` and READMEs:** no root changes; `parity/ds-gallery` files came only from tc-ui.

**Gates (all run with `-j 3` on the final head)**

| Gate | Result |
|---|---|
| `cargo fmt --all -- --check` | pass |
| `cargo clippy --workspace --all-targets -- -D warnings` | pass |
| `cargo clippy --target wasm32-unknown-unknown -p tc-ui -- -D warnings` | pass |
| CI: tc-ui wasm32 clippy with `--all-targets` | pass |
| CI: tc-ui wasm32 clippy with `--all-targets --features gallery` | pass |
| `cargo doc --workspace --no-deps` | builds, 4 warnings (below) |
| `cargo test --workspace --no-fail-fast` | 769 passed, 1 failed, 2 ignored |

Test counts by crate:

| Crate | Passed | Failed | Ignored |
|---|---|---|---|
| tc-align | 97 | 0 | 2 |
| tc-burrito | 76 | 0 | 0 |
| tc-check | 97 | 0 | 0 |
| tc-core | 4 | 0 | 0 |
| tc-door43 | 99 | 0 | 0 |
| tc-door43-fake | 10 | 0 | 0 |
| tc-helps | 94 | 0 | 0 |
| tc-resources | 117 | 0 | 0 |
| tc-ui | 26 | 0 | 0 |
| tc-usfm | 68 | 1 | 0 |
| tc-versification | 81 | 0 | 0 |

The doc warnings come from the branches, not the merge:
- **tc-align (3):** "`reflow` is both a function and a module", at `crates/tc-align/src/lib.rs:17`, `aligner.rs:66` and `edit.rs:17`.
- **tc-ui (1):** an unresolved link to `ds` at `crates/tc-ui/src/lib.rs:3`.

**Pixel diff**
I built the gallery with `build-gallery.sh` (dev profile, 32 MB wasm) and ran `parity/ds-gallery/capture-rust.mjs`:
- 719 of 719 cases captured, 0 failed to draw.
- 719 of 719 cases at 0 mismatched pixels; 0 non-zero, 0 unexplained, 0 total mismatched pixels.
- All 719 Rust screenshots are byte-identical to the baseline in `screens/`, and `diff-report.json` did not change.

I copied the gallery's `node_modules` from the tc-ui worktree (same `package-lock.json`); that folder is git-ignored.

**What I could not resolve**
- **The failing test is not caused by the merge.** In `fixtures/scale/psa/en_ult/19-PSA.usfm`, tc-usfm refuses 50 Psalm verse 1s with `AlignmentOutsideVerseText`. These are aligned superscriptions written as `\v 1 \d \zaln-s …`, starting at 11:1. I ran the same test on a clean checkout of main (32f0949) and it fails at 11:1 the same way. tc-usfm refuses alignment inside headings on purpose (its docs in `src/align.rs` say so). Fixing it means changing how aligned `\d` superscriptions are read and written back, which is not an integration fix. Until then, tc-usfm will refuse alignments in real ULT Psalms exports.
- **The tc-align branch on its own also failed this test** because of `span-source.usfm`; edb9aa2 covers that part.

**Disk**
5.6 GB free after the work, so I kept the integration worktree's `target/` (the rule was to delete it only below 4 GB). Earlier I deleted `target/release` and `target/debug/incremental` to make room for the gallery build.