# The workspace on Windows (2026-10-17)

Task 1 of `.audit/tasks/windows-track.md`: build and test the whole workspace on the owner's
Windows laptop (`windows-machine-2026-10-17.md`: Windows 10 22H2, i5-8350U, 4 cores, 8 GB, SATA
SSD, Defender on, no exclusions), and fix what fails on Windows at its root.

Toolchain: Rust 1.99.1 (`x86_64-pc-windows-msvc`), Visual Studio 2022 Build Tools, Git for
Windows 2.55.0 installed with its defaults (`core.autocrlf=true`, `core.protectNTFS` on).

## The first run

Branch `claude/focused-ride-5uzj7p` at `b97fd27`, a fresh clone, an empty `target`. Each step
ran in this order, its full output kept.

| Step | Result | Time |
|---|---|---:|
| `cargo fetch` | ok | 66 s |
| `cargo fmt --all -- --check` | ok | 5 s |
| `cargo clippy --workspace --all-targets -- -D warnings` | failed: a lint new in clippy 1.99 (below) | 48 s |
| `cargo test --workspace --no-fail-fast` | 1,182 passed, 107 failed, 4 ignored, in 26 test binaries | 1,271 s, of which the build 550 s |
| `cargo clippy -p tc-ui --target wasm32-unknown-unknown --all-targets -- -D warnings` | ok | 142 s |
| the same with `--features gallery` | ok | 35 s |

## What failed, and why

### 106 tests: Git for Windows rewrote the fixtures' line endings

Git for Windows installs with `core.autocrlf=true`, and the repository had no
`.gitattributes`, so the clone wrote 1,305 text files with CRLF: fixtures, the bundled
resources, the OBS template, the parity baseline, the sources. The tests hash, measure and
write back those bytes exactly, as the app must, so they failed wherever bytes are compared:
burrito ingredients whose md5 and size no longer matched (`tc-burrito` mutations and samples,
`tc-door43` downloads), TSV and USFM write-backs (`tc-helps`, `tc-usfm`), OBS stories and the
template (`tc-obs`), pinned wire forms and the index probe (`tc-journal`), resource installs
and seeding (`tc-resources`), and the vendored assets (`tc-ui`).

It is not only a test problem. The app compiles some of these files into itself
(`include_str!` of the OBS template stories in `tc-obs/src/template.rs`, `include_bytes!` of
`index_probe.batch` in `tc-journal/src/index.rs`), so a release built from such a clone would
carry CRLF copies: `the_shipped_template_is_the_pinned_one` (`tc-obs`) and
`the_probe_is_what_the_generator_gives` (`tc-journal`) fail on exactly those bytes.

Fix (`8cccb28`): `.gitattributes` with `* -text`, so git never converts line endings,
whatever `core.autocrlf` says. A Windows clone now has the same bytes as a Linux one. Nothing
changes on Linux, where `autocrlf` is off by default and no file in the index has CRLF.

### 1 test: the fake Door43 lost names Git for Windows refuses

`a_commit_whose_names_collide_on_windows_or_macos_is_refused` (`tc-door43`). The fake server
builds each seeded commit with `git update-index --index-info`. Git for Windows turns
`core.protectNTFS` on by default, and then skips any name NTFS cannot hold, prints
"Ignoring path", and exits 0:

```text
$ printf '...\tnotes/a\n...\tnotes/a.\n...\tnotes/a \n...\tnotes/TIT.tsv:hidden\n' | git update-index --add --index-info
Ignoring path notes/a.
Ignoring path notes/a 
Ignoring path notes/TIT.tsv:hidden
exit 0
```

So on Windows the fake served a commit without the colliding names, and the client had
nothing to refuse. Door43 runs on Linux and keeps such names. Fix (`aa57878`): the fake's git
runs with `-c core.protectNTFS=false`, as git does on Door43's servers.

### Not Windows: clippy 1.99

Rust 1.99 adds `chunks_exact_to_as_chunks`, which denies `chunks_exact` with a constant size
under `-D warnings`, and its `result_large_err` denies `install_all`'s error, a resource pin
beside an install error, 344 bytes whose size does not depend on the platform. CI on Linux
fails on the first of them too: the run of 20:23 UTC on `claude/focused-ride-5uzj7p` stops at
`crates/tc-core/src/digest.rs:37`.
Fixes: `91595ba` (tc-core), `eb96531` (tc-journal, four loops), `1602fed` (tc-resources).

### Not Windows: a property test found a lost `incomplete_history`

`concurrent_differences_are_visible` (`tc-journal`) failed once with a random seed, and passed
the next time. Its shrunk script, replayed as a fixed test, fails every time. A restore race
(STORAGE §7.6) lists a verse typed after *Start empty* as a candidate of the book's presence
conflict; when that verse also continued a change of a held batch, the conflict did not say
its history was incomplete (§7.4), because only the presence heads were checked. Nothing in the
path depends on the platform. Fix: see the commit that names `incomplete_history`, with a
focused test in `tests/loss.rs` and the replay in `tests/properties.rs`.

### Nothing else

With those fixed, no test failed for path separators, drive letters, file locking, renames
over open files, directory fsync, case-insensitive names, long paths or timing: the journal's
crash-injection tests and the resource store's install and seeding tests pass on NTFS with
Defender on. That covers what the tests exercise. A reading of every crate for Windows-only
hazards the tests do not reach was started and not finished; it is listed as open below.

### A Windows flake: a refused loopback connection that hangs

`a_refused_connection_is_unreachable` (`tc-door43`) failed in two of the four full runs (once
on `door43-facts` under load, once on this branch with nothing else running):
`Unreachable { cause: Timeout }` where it expects `Connect`, and its test binary took 35 s
instead of 13. The test binds `127.0.0.1:0`, closes the listener and connects to that port.

On this laptop a refused loopback connection usually fails after about 2 s, because Windows
retries the SYN after the reset (the test alone: 2.0 s, four runs out of four). Sometimes it is
not refused at all and waits until the client's 20 s connect timeout. A probe with .NET's own
sockets, outside Rust, shows the same: ten quick connects to a closed loopback port, each
capped at 5 s, gave seven refusals and three connects still waiting at 5 s, in no pattern
(one more, after a 60 s pause, was refused). The laptop runs only Microsoft's network
drivers: Defender with its Network Inspection System driver, and the Windows Filtering
Platform. So this is how a stock Windows 10 with Defender behaves, and the cause lies below
the client.

Not fixed. The client reports what Windows reports, and making the test accept a timeout would
loosen it. The owner may want to rule whether "cannot connect" and "timed out" may be one
outcome on Windows, or whether the test should make its refusal another way.

## Windows hazards the tests do not reach

Every crate was read for Windows behaviour that the passing tests cannot show, and each
finding was checked by a second reader who tried to refute it. Six held up. None is reached
by the app as built today, because no shipped binary runs these paths yet; the first-launch
shell (TC4-DELTA §3.15.10) will.

1. **A resource install fails when a scanner holds a staged file** (`tc-resources`,
   `store.rs:709`; also `:857` and `:452`). `install_until` writes about a thousand files into
   `.partial/` and renames the folder into place once. If Defender, the Search indexer or
   OneDrive still holds one of the just-closed files without `FILE_SHARE_DELETE`, the rename
   fails with error 5 or 32 and the install stops with "move into place". On a first launch
   `seed_from` turns that into a failed start, and `ResourceStore::open` fails when it cannot
   remove a leftover `.partial`. Fix: retry renames and removals on errors 5, 32 and 33 with a
   bounded back-off on Windows, and a Windows-only test that holds a staged file open.
2. **The same gap in the save path the spec describes** (STORAGE §5.0 `atomic_write` and its
   step B3, §6.1 step O2). No retry on the rename; a `NotWritten` save leaves a temp that the next open rolls
   forward, so a retried save can show a false "another computer was saving as this one" and
   a conflict with the person's own text. `tc-store` does not exist yet, so the fix belongs in
   the spec before it is written.
3. **Names NTFS folds together that the collision check keeps apart** (`tc-door43`,
   `unpack.rs:298`; `store.rs:840`, `aquifer.rs:711`). The check folds with NFC and
   `to_lowercase`, but NTFS folds `σ` and `ς` to one name, so the second file silently
   overwrites the first in staging (`File::create`), and `Installed::read` checks only the
   size. Fix: create staged files with `create_new`, so any name the volume merges stops the
   install, and add a simple uppercase fold to the collision key.
4. **8.3 short-name aliases** (`unpack.rs:248`). A name such as `PROGRA~1` can open another
   file through its short name. The same `create_new` change catches it.
5. **A middle folder name ending in a space or a dot** (`unpack.rs:256`, `store.rs:833`,
   `aquifer.rs:686`). Windows strips the trailing character when it creates the folder, then
   the file under it fails with error 3, or lands in a renamed folder. Fix: refuse such names.
6. **Proxy auto-configuration is ignored** (`tc-door43` `client.rs:180`, `aquifer.rs:211`).
   reqwest's system proxy on Windows reads only the manual proxy setting, not WPAD or a PAC
   script, which many school and office networks use. Fix: a shared client constructor that
   asks WinHTTP for the proxy of each origin.

## Fixed on `windows-hazards` (task 6)

Hazards 1, 3, 4 and 5 are fixed on the branch `windows-hazards`, with the Linux CI covering
what it can and five tests marked `#[cfg(windows)]` for the rest.

1. **Held staged files.** `tc-resources` retries a rename or removal that fails with error 5, 32
   or 33, ten waits and 2,004 ms in all (the schedule of `tc-store`'s `HeldRetry::SAVE`), in
   `held.rs`. A rename that fails while its target exists is not retried, and `open` keeps a
   leftover a scanner still holds for the next open. Windows tests hold a staged file open
   without `FILE_SHARE_DELETE` (`an_install_succeeds_when_a_scanner_briefly_holds_a_staged_file`
   and four more in `store.rs`). The same five scenarios also run on every platform against a
   simulated scanner (`store::tests::a_scanner_holds_files`), so CI fails if the retry breaks.
3. **Names NTFS folds.** Staged files are created with `create_new`, so a name the volume
   merges stops the install with `DuplicateEntry` instead of overwriting. The collision key
   folds letter case and Unicode normalization as Windows and macOS do, in one place for `tc-door43` and `tc-resources`
   (`tc_door43::check_names`).
4. **8.3 short names.** The same `create_new` change, and names shaped like a short name
   (`PROGRA~1`, `IMG~1.JPG`) are refused on every platform.
5. **A trailing dot or space.** Refused for every path segment, files too, at the three
   places. The refused-name rules are written down in `docs/STORAGE.md` §3.7.

Also on the branch: the census properties that count a missing parent run 256 cases
(`team()` owns the count); `a_refused_connection_is_unreachable` became
`a_connection_that_cannot_be_made_is_unreachable` (below); and one shared HTTP client
constructor, `tc_door43::Http`, builds both network clients and asks a proxy finder only while
the internet switch is on, and keeps the system's own proxy settings for any origin the finder
names none for (hazard 6, below).

## Open

- **Hazard 2** (a save's rename blocked by a scanner): `tc-store`, on the `storage` branch.
  The held-file helper there and the one in `tc-resources` follow one policy and become one
  helper when `storage` lands.
- **Hazard 6, proxy auto-configuration: the Windows finder is built (2026-10-19); the owner's
  go-ahead on its dependency is still to be recorded here.** The constructor
  and the rules around it are done and tested on Linux against a local proxy that refuses
  every request (`tests/proxy.rs` in `tc-door43`, `tests/aquifer_proxy.rs` in
  `tc-resources`): one `Http` builds both clients, a server on this computer is never
  proxied, a finder is asked only while the internet switch is on, once for each origin, and
  again after the switch went off and on, also when that happens during a slow lookup (a
  request that finds the switch off after the lookup is refused). A proxy the finder names is
  used for its origin; for every other origin the `*_PROXY` variables and the manual proxy
  setting still apply. The first version of the constructor lost them as soon as a finder
  was given, because reqwest drops its system proxy for a client that has a proxy of its
  own; it now reads them itself (hyper-util's matcher, which reqwest uses) and falls back to
  them, and `tests/proxy.rs` runs a copy of its binary with `HTTPS_PROXY` set to check it.
  The first commit message of the constructor (`f02d606`) said that a finder's proxy was
  used "ahead of the system's own settings"; that is true only since this fix. The finder for
  Windows, which asks WinHTTP (`WinHttpGetIEProxyConfigForCurrentUser`, then `WinHttpGetProxyForUrlEx` with WPAD and the
  PAC URL), is `tc_door43::WinHttpFinder` (`src/winproxy.rs`), and `system_proxy_finder()`
  returns it on Windows. It makes no FFI call itself: the WinHTTP calls are proxy-watch
  `=0.3.1`'s (Cargo.lock checksum `aa5464483809f28430fadd3e7e09cf724e2c4b1e771895fb5066b5a2f523be44`),
  chosen and audited in `docs/evidence/proxy-watch-audit-2026-10-19.md`, whose conditions (a)
  to (j) it meets. A finder now answers a proxy, going directly, or nothing; only nothing
  falls back to the system's settings, so a PAC script's `DIRECT` is not overridden by the
  manual proxy, and every failed lookup is nothing, never directly. The audit measured that
  WinHTTP refuses a `file:///` PAC URL (12006), so the plan above for the test does not work:
  `tests/windows_proxy.rs` serves each script over loopback http at a path of its own instead,
  because WinHTTP keeps a script's answers for its URL for the whole computer. The crate's
  unsafe code relies on two undocumented WinHTTP behaviours (its `src/pac/winhttp.rs:411-416`
  and `443-457`): a call that returns at once never also calls back, and a call cancelled by
  closing its handle no longer reads its arguments.
- **The refused-connection flake: changed, not yet confirmed on Windows.** The code was right:
  Windows refuses a closed loopback port after about two seconds and sometimes not before
  the connect timeout, whatever the client is. The test now connects to loopback port 0,
  which Linux refuses at once and Winsock, as documented, answers with an invalid-address
  error (WSAEADDRNOTAVAIL) without sending anything, and expects `Connect`. It passes on Linux; run it on the laptop.
- **A Windows rerun of the whole branch.** `windows-hazards` was rebased onto
  `claude/focused-ride-5uzj7p` after the Windows runs of `c7391df`, so the Windows runs did
  not include downloads that stream into `resources/.partial/` (a scanner holding a fresh
  `door43-*.zip.part` meets the new `open` rule). Rerun `cargo fmt --all -- --check`,
  `cargo clippy --workspace --all-targets -- -D warnings` and
  `cargo test --workspace --no-fail-fast` on Windows, with the five Windows-only tests of
  `store.rs` and the one of `held.rs`, and record the result here. The simulated scanner of
  the Linux tests skips a sleep only after an error it raised itself; a hold that Windows
  reports during those tests (Defender at work) is waited out as in the app. The Windows test
  `a_real_hold_is_waited_out_while_a_scanner_is_simulated` (`held.rs`) pins that and has
  not run yet.
- From the review of the `incomplete_history` fix, for the owner: §7.4 says a conflict marked
  incomplete offers only *Apply what was recovered*, which read literally would now block a
  restore race's own choices (nothing enforces it today); a *Start empty* verse head with a
  missing parent marks its verse conflict, not the presence conflict; the property counts
  unreadable bases as heads; and a conflict's `batches` leave out the race verse heads.

## The last run

This branch at `3ecbb3e`, 2026-10-18 06:03 UTC, nothing else running.

| Step | Result | Time |
|---|---|---:|
| `cargo fmt --all -- --check` | ok | 4 s |
| `cargo clippy --workspace --all-targets -- -D warnings` | ok | 23 s |
| `cargo test --workspace --no-fail-fast` | 1,289 passed, 2 failed, 4 ignored | 932 s, of which the build 203 s |
| `cargo clippy -p tc-ui --target wasm32-unknown-unknown --all-targets -- -D warnings` | ok | 8 s |
| the same with `--features gallery` | ok | 1 s |

The two failures are the ones listed as open: `a_refused_connection_is_unreachable` (Windows,
as above) and a census miss of `provisional_usfm_never_drops_words` (any platform).

CI on Linux passed on this commit, every job: fmt, clippy, the tests, both wasm clippy runs
and the parity baseline.
