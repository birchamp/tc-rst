# Windows track: work for the owner's Windows 10 computer

For a Claude Code session on the owner's Windows 10 computer. The cloud session keeps building
on Linux; this computer does what Linux cannot: build, test and measure on Windows, and reach
Door43. Written 2026-10-17.

> **Roles from 2026-10-19:** the owner set up a Windows 11 VM on the Linux PC, and the Linux
> session runs every Windows build and test there (MSVC), on each new main. This laptop only
> measures what needs the reference hardware: memory and startup (tC4, Paratext, Word, Edge, and
> later the rebuild), the cold start after a reboot, and the rebuild's budgets. Task 7's hazard 6
> (the proxy lookup) moved to the Linux session and the VM. Do not run builds or test suites here
> unless the Linux session asks.

## Rules

- Read `AGENTS.md`, `docs/PLAN.md` and `docs/PERFORMANCE.md` first. They apply here too.
- Work only on the branches named below. Never push `claude/focused-ride-5uzj7p` or `storage`:
  the cloud session pushes those and merges your branches after it checks them.
- Commit each coherent step and push your branch often, so nothing is lost if this computer
  restarts.
- Never record a token, password or private path in a file you commit.
- If a step needs a decision only the owner can make, write it in your branch's notes file and
  ask the owner; do not guess.
- Keep this file's tasks in order unless the owner says otherwise. Task 4 has a fixed time.

## Setup (once)

1. Git for Windows. Run `git config --global core.longpaths true`. Clone into a short path, for
   example `C:\src\tc-rst`: `git clone https://github.com/birchamp/tc-rst C:\src\tc-rst`.
2. Rust stable with the MSVC toolchain: install the Visual Studio 2022 Build Tools with the
   "Desktop development with C++" workload, then rustup from https://rustup.rs. Add the
   browser target: `rustup target add wasm32-unknown-unknown`.
3. Node.js 22 LTS (for the parity and spike scripts).
4. For build speed only, you may add `C:\src\tc-rst\target` (and other `target` folders) to
   Windows Defender's exclusions. Never exclude a project folder or the app data folder: the
   benchmarks must measure with Defender on.
5. Record the computer once in `docs/evidence/windows-machine-2026-10-17.md`: Windows version
   and build, CPU model, cores and threads, whether the CPU has SHA and AVX-512 instructions
   (for example with `coreinfo` from Sysinternals, or from the CPU model's specification),
   memory, disk type and free space, and whether Defender real-time protection is on.

## Task 1 (now): build and test the whole workspace on Windows

Branch: `windows-fixes`, from `claude/focused-ride-5uzj7p`.

1. `git checkout -b windows-fixes origin/claude/focused-ride-5uzj7p`
2. Run, in order, and keep the full output of each:
   - `cargo fmt --all -- --check`
   - `cargo clippy --workspace --all-targets -- -D warnings`
   - `cargo test --workspace --no-fail-fast`
   - `cargo clippy -p tc-ui --target wasm32-unknown-unknown --all-targets -- -D warnings`
     (and again with `--features gallery`)
3. Every failure that passes on Linux is a Windows bug in the rebuild, and the app's main
   users are on Windows. Fix each one at its root, in the code, never by skipping or
   loosening the test. Typical causes: path separators and drive letters, `\r\n` line endings
   (check `.gitattributes` and how tests read fixtures), file locking (Windows cannot rename
   or delete a file that is open), rename-over-existing and directory fsync rules, case
   insensitivity, long paths, temp-folder cleanup, and timing. Each fix must keep Linux green:
   do not change behavior on Linux unless it was wrong there too.
4. If a test reads something outside the repository (for example a tC4 checkout at a Linux
   path), make it use the repository's own fixtures, or mark the dependency clearly, and note
   it.
5. Write `docs/evidence/windows-build-2026-10-17.md`: the machine, how long a clean build and
   the test run took, every failure you found with its cause and fix, and anything left open.
6. Push `windows-fixes`. Tell the owner it is ready to merge.

## Task 2: the Door43 facts the cloud cannot reach

Branch: `door43-facts` (it exists on GitHub; pull it). Follow `.audit/tasks/door43-facts.md`
on that branch, "Status" section first: steps 1 to 3, the bundle half of step 4, step 5 (run
the live example once against git.door43.org with the internet switch on) and, only if the
owner agrees, step 6. Push `door43-facts`.

## Task 3: the storage benchmark on Windows

Branch: `windows-bench`, from `origin/storage`.

1. Read `docs/evidence/storage-fold-2026-10-16.md` on that branch for the commands.
2. With Defender on, run the release `foldbench` of `tc-corpus` on the whole-Bible corpus:
   load and fold with the fold cache (warm; and after a reboot if possible), the first open
   without a cache, the Psalms projection, peak and steady memory. Use at least 5 runs for
   actions over 1 s and 20 otherwise; give median and p95.
3. Compare each number with its budget in `docs/PERFORMANCE.md` (3 s ordinary open; 10 s first
   open without a cache; 300 ms Psalms projection; 350 MiB peak, 150 MiB steady).
4. Write `docs/evidence/storage-fold-windows-2026-10-17.md` and push `windows-bench`.

## Task 4 (at 20:00 UTC today, 16:00 US Eastern): the Electronite spike, G0

The cloud session is finishing this package now and will merge it into
`claude/focused-ride-5uzj7p` before 20:00 UTC. At that time, pull that branch and follow
`.audit/tasks/g0-windows.md`. Its storage-benchmark part is the same work as task 3: if
task 3 is done, point to its evidence file instead of running it again. If that file is not
there yet, wait, and do tasks 1 to 3 meanwhile.

## Task 5: how much memory other programs use with Psalms open (added 2026-10-18)

Branch: `windows-memory`, from `origin/claude/focused-ride-5uzj7p`. The owner accepted new memory
limits (`docs/PERFORMANCE.md`: the whole app 1 GiB steady; the project store 400 MiB) and wants
to know they are reasonable next to programs a translator already runs. Read
`docs/BUDGET-REVIEW-2026-10-17.md`, section 2, first.

1. Measure every program the same way the limit does: its **private working set** (Task
   Manager's Memory column), summed over all of its processes, read a minute after the last
   action, three readings a minute apart (give the median). In PowerShell:
   `(Get-Counter '\Process(WINWORD*)\Working Set - Private').CounterSamples` (change the name
   for each program; sum the samples). Defender stays on. Close other programs first, except
   where a step says otherwise.
2. **Microsoft Word with Psalms.** Make a document of the whole of Psalms from the bundled ULT:
   expand `bundle/en/en_ult_v91.sb.zip` (`Expand-Archive`) to a folder outside the repository,
   take `19-PSA.usfm`, and turn it into plain text: one paragraph per verse with its number,
   chapter headings, and no USFM markers or alignment data (drop `\zaln-s ... \*`, `\zaln-e\*`
   and the attributes of `\w word|...\w*`, keep the words). Check a few verses against the
   USFM by eye. Open it in Word, save it as `.docx`, close Word, open the `.docx` again, scroll
   to the end and back, wait a minute, measure. Record the file size and page count.
3. **A web browser with Psalms.** Edge or Chrome with the ULT Psalms page on git.door43.org
   (internet on; read only) or the same text as a local HTML file; sum every browser process.
4. **tC4 4.0 and Paratext, if installed.** Do not install either without the owner's yes. If
   tC4 4.0 is installed, open a project with Psalms in the translationWords check, wait,
   measure (sum every process of the app). If Paratext 9 is installed, open a project with
   Psalms, measure.
5. **The rebuild's window.** Start the G0 spike's Electronite shell with its page, as part 1 of
   `.audit/tasks/g0-windows.md` does, leave it idle a minute, measure every Electronite process.
   This is the Windows figure the budget review only estimated.
6. **Everything together.** With Word (Psalms), the browser (Psalms) and, if installed, tC4 4.0
   all open, record `(Get-Counter '\Memory\Available MBytes')` and
   `(Get-Counter '\Memory\Committed Bytes')`, and the laptop's total RAM. This says how much
   room an 8 GB laptop has left for our app.
7. Write `docs/evidence/memory-compare-windows-<date>.md` (the machine, each program's version,
   every reading, and one table: program, what was open, private working set) and push
   `windows-memory`.

## Task 6: fix the Windows hazards your build report found (added 2026-10-18)

Branch: `windows-hazards`, from `origin/claude/focused-ride-5uzj7p` (it now holds your
`windows-fixes` and `door43-facts`). Do task 5 first. Fix each at its root with a test that
fails without the fix, on Windows where the hazard is Windows-only (mark such tests
`#[cfg(windows)]` and say so); keep Linux green.

1. Hazard 1: retry renames and removals of staged resource files on errors 5, 32 and 33 with a
   bounded back-off (`tc-resources` `store.rs`), with a Windows test that holds a staged file
   open without `FILE_SHARE_DELETE` during the install.
2. Hazards 3 and 4: create staged files with `create_new`, so any name the volume merges (NTFS
   case folding such as σ and ς, 8.3 short names) stops the install with a clear error instead
   of overwriting; add the uppercase fold to the collision key (`tc-door43` `unpack.rs`,
   `tc-resources` `store.rs` and `aquifer.rs`).
3. Hazard 5: refuse a folder name that ends in a space or a dot, at the same three places.
4. Hazard 6: one shared HTTP client constructor for `tc-door43` and `tc-resources` that asks
   WinHTTP for each origin's proxy (WPAD and PAC included) on Windows, still behind the
   internet switch; test it against a local PAC file and a local proxy, never the network.
5. `a_refused_connection_is_unreachable` fails in about half your runs: find why on Windows and
   fix the cause (in the test only if the code is right).
6. Raise the case count of every team property that counts the missing-parent case to 256, as
   `concurrent_differences_are_visible` does, so the census stops missing it.
7. Write what you changed into your build report's "Open" section, rerun the full steps, push
   `windows-hazards`, and tell the owner it is ready.

Hazard 2 (a save's rename blocked by a scanner) belongs to `tc-store` on the `storage` branch;
the storage team handles it.

## Task 7: finish hazard 6, and rerun on Windows (added 2026-10-18)

`windows-hazards` is merged into `claude/focused-ride-5uzj7p` (125dd41) after a review on Linux.
The review rebased your commits onto main and fixed two things in the shared client: it keeps the
system's own proxy for an origin a finder names none for, and it checks the internet switch
again after a proxy lookup. Two parts are still open.

Branch: `windows-proxy`, from `origin/claude/focused-ride-5uzj7p`.

1. Rerun the full steps of task 1 on Windows at the merge, and record the result in your build
   report. The branch was rebased after your last Windows run.
2. Hazard 6, the lookup itself: `system_proxy_finder()` returns `None` everywhere, so a
   translator on a network that sets its proxy by PAC or WPAD cannot reach Door43. Fill it on
   Windows with a dependency that asks WinHTTP (`WinHttpGetIEProxyConfigForCurrentUser`, then
   `WinHttpGetProxyForUrl` with auto-detect and the PAC URL), never with `unsafe` in our crates
   (AGENTS.md; the Linux session decided this on 2026-10-18). Pick a maintained crate, or a
   thin crate of Microsoft's (`windows` or `windows-sys`) wrapped in the smallest safe API you
   can, and say in the commit why you picked it; if every choice needs `unsafe` in our own
   code, stop and write that in your notes for the owner instead. Keep the lookup off the
   async runtime's threads (it can take seconds), cache each origin's answer for the session,
   and keep it behind the internet switch. Test it against a local PAC file and a local proxy,
   never the network.
3. Push `windows-proxy` and tell the owner it is ready.

## When everything is done

Tell the owner which branches are ready. The cloud session merges them after running its own
checks on Linux.
