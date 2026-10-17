# Windows track: work for the owner's Windows 10 computer

For a Claude Code session on the owner's Windows 10 computer. The cloud session keeps building
on Linux; this computer does what Linux cannot: build, test and measure on Windows, and reach
Door43. Written 2026-10-17.

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

## When everything is done

Tell the owner which branches are ready. The cloud session merges them after running its own
checks on Linux.
