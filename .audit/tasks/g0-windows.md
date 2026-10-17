# Windows task: the Electronite spike G0 and the storage benchmark

Run this in a Claude Code session on the owner's Windows laptop, from start to finish. The
owner is at the computer. Plan for about 60 minutes when the tools are already installed: 20
for part 1, and 40 for part 2 if task 3 of `.audit/tasks/windows-track.md` has not already
measured the storage layer. Part 3 needs a reboot and is optional.

## What this is for

The rebuild's app runs inside Electronite, an Electron fork that shapes Graphite fonts. The
design (`docs/APP-DESIGN.md`) leaves questions that only a run on Windows can answer:

- Does the shell's way of adding the security token reach every kind of request the browser
  makes by itself (pictures, help pages, fonts, downloads, printing)? (AD11)
- Do the page's own `/api/` requests wait for the shell's main thread? (AD11, AD21, G1)
- Is a custom `tc4://` scheme faster or slower than that header filter for the aligner? (Q-1)
- Is one aligner click, with a 20 KB reply, laid out and painted within 30 ms on this laptop? (Q-13)
- When the server or the shell dies, does the other side behave? (AD39)
- After a crash, how much typed text do `localStorage` and the shell's mirror file keep? (AD21)
- Does the storage layer meet its budgets on a Windows laptop with Defender on? (PERFORMANCE.md)

Part 1 runs the spike in `spikes/electronite`. Part 2 runs the storage benchmark, unless task 3
of `windows-track.md` already did. The scripts do all the measuring and write the result files.
Your job is to install, run, check, and commit.

## Run every command in PowerShell, not Git Bash

Every command in this file is a PowerShell command. Run each one in PowerShell (Windows
PowerShell 5.1 or PowerShell 7), not in Git Bash: Git Bash reads PowerShell lines differently,
and its own `tar` cannot unpack a zip. If your shell tool is Git Bash, use your PowerShell tool
instead. If you have no PowerShell tool, write the commands into a `.ps1` file in
`C:\tc4-g0` and run that file with
`powershell.exe -NoProfile -ExecutionPolicy Bypass -File C:\tc4-g0\step.ps1`.

The builds, part 1 and part 2 can each run longer than 10 minutes: start them as background
commands and check their output every few minutes. Each script prints one line per step.

## Rules

- Change no code. If a script stops with an error you can fix in the script itself (a path, a
  PowerShell detail), fix it in a separate commit on `g0-windows` and say what and why. Never
  change a script so that a result looks better.
- Do not change the storage branch, `claude/focused-ride-5uzj7p` or anything under `crates/`.
- Keep Windows Defender real-time protection on. The only exclusions allowed are the build
  `target` folders that `windows-track.md` (setup, step 4) allows. Never exclude the
  repository, `C:\tc4-g0`, a corpus or an app data folder. The reports' machine tables record
  how many folders are excluded; if the count is not 0, write under the report's title which
  folders they are. Do not change the power plan. These are the conditions the budgets are for.
- Never record a token or a password, in a file, a commit or a message. The scripts never
  write the server's token. Never commit `spikes/electronite/out/`: its Electronite profiles
  hold the session's token (it is useless after the run, but it stays private).
- Push only the new branch `g0-windows`.

## Prerequisites

The session that follows `windows-track.md` already has Git, Rust with the MSVC toolchain and
a clone. Check each item with the command in brackets, and install only what is missing.

1. **Git for Windows**, any 2.4x or newer, from https://git-scm.com/download/win. [`git --version`]
2. **Rust stable 1.97.0 or newer with the MSVC toolchain.** Download `rustup-init.exe` from
   https://rustup.rs and run it. Accept the default host `x86_64-pc-windows-msvc`. If it says the
   Visual C++ build tools are missing, let it install them (Visual Studio Build Tools 2022 with
   "Desktop development with C++"). Then open a new terminal. [`rustc --version`;
   `rustup show` names `x86_64-pc-windows-msvc`]
3. **Node.js is not needed.** Electronite carries its own.
4. **Electronite v37.1.0-graphite for Windows x64**, the same build tC4 ships
   (`translationcore4/docs/PACKAGING.md`, "The wrapper is Electronite"). In PowerShell, one
   line at a time:

   ```powershell
   New-Item -ItemType Directory -Force -Path C:\tc4-g0 | Out-Null; Set-Location C:\tc4-g0
   curl.exe -L --fail -o electronite.zip https://github.com/unfoldingWord/electronite/releases/download/v37.1.0-graphite/electronite-v37.1.0-graphite-win32-x64.zip
   (Get-FileHash electronite.zip -Algorithm SHA256).Hash
   ```

   The hash must be `8146CA21FA090E12BCA01A8AF73D234D9A31D90F51DA0DA86BB0E1731C371B52`. If it is
   not, stop and tell the owner. Then unpack it with PowerShell's own `Expand-Archive` (a minute
   or two), never with a bare `tar`, which may be Git's GNU tar and cannot read a zip:

   ```powershell
   Unblock-File electronite.zip
   Expand-Archive -Path C:\tc4-g0\electronite.zip -DestinationPath C:\tc4-g0\electronite -Force
   Test-Path C:\tc4-g0\electronite\electron.exe
   ```

   The last line must print `True`. If `Expand-Archive` fails, use Windows' own tar by its full
   path: `C:\Windows\System32\tar.exe -xf C:\tc4-g0\electronite.zip -C C:\tc4-g0\electronite`.
5. **Disk:** at least 12 GB free on `C:` (4 GB if part 2 is skipped). [`Get-PSDrive C`]
6. **The laptop:** plugged in, other programs closed, no other heavy work running.

## Get the code

Reuse the clone `windows-track.md` made (`C:\src\tc-rst` there; use its real path if it is
elsewhere); call its folder `REPO` below and run every command from it. First commit and push
the work of your current branch, as `windows-track.md` asks. Then:

```powershell
Set-Location C:\src\tc-rst
git fetch origin
git switch claude/focused-ride-5uzj7p
git pull --ff-only
git log -1 --format=%H
```

Confirm that `spikes\electronite\run-spike.ps1` and `spikes\foldbench-windows\run-foldbench.ps1`
exist. If they do not, G0 has not been merged yet: tell the owner, and use the branch the owner
names instead (G0 was built on `p0-g0` and merges through `phase0`).

If there is no clone yet, make one with `core.autocrlf=false` (the corpus and fixtures must keep
their line endings byte for byte):
`git clone -c core.autocrlf=false https://github.com/birchamp/tc-rst.git C:\src\tc-rst`.
A clone made with `core.autocrlf=true` works for part 1; part 2 uses its own checkout (below).

## Part 1: the Electronite spike (about 20 minutes)

1. Build the stand-in server first (2 to 5 minutes; it downloads crates):

   ```powershell
   cargo build --release --manifest-path spikes/electronite/Cargo.toml
   ```

2. Tell the owner: windows will open and close by themselves for about 15 minutes; do not
   touch the keyboard or the mouse, and do not cover or minimise the windows.
3. Run every case (one line):

   ```powershell
   powershell -NoProfile -ExecutionPolicy Bypass -File spikes/electronite/run-spike.ps1 -Electron C:/tc4-g0/electronite/electron.exe -Server spikes/electronite/target/release/tc4-standin.exe -Report docs/evidence/electronite-spike-2026-10-17.md
   ```

4. Read `docs/evidence/electronite-spike-2026-10-17.md`. Check that:
   - the Electronite line says 37.1.0 and names a Chromium version;
   - the machine table has the CPU, cores, RAM, disk, power and Defender state;
   - sections 1 to 5 have rows, section 2 has two passes for each mode, and section 4 names a
     first write to disk in every row. A section "Cases that did not finish" means a case
     failed twice.
5. If a case did not finish, run the script once more with `-Only <case>` (for example
   `-Only endshell`) and `-Report docs/evidence/electronite-spike-2026-10-17-rerun.md`, and
   keep both files. Do not delete a report that shows a failure.
6. Add one line under the title of the report: whether this laptop is the reference machine (a
   five-year-old laptop with 4 cores, 8 GB and a SATA SSD) or a stand-in, with its model and age
   if the owner knows them.

What the script does, so you can explain it: it builds nothing else and changes nothing outside
`spikes/electronite/out/` and the report. For each of three ways to give the page its token it
probes every request type, times the page's `/api/` requests while the shell's main thread is
idle and while it is busy, then times 200 aligner clicks alone and 200 while pictures and a
help page load. It runs the three ways twice, the second time in the opposite order. Then it
ends the server, closes the window, ends the shell's process (twice: with the server started
plainly and detached), and ends the shell while the page types, after Chromium has written
`localStorage` to disk once, to see what was kept, with a normal quit while typing as the
control.

## Part 2: the storage benchmark

### First: has task 3 already done it?

Task 3 of `windows-track.md` runs the same benchmark on the branch `windows-bench`. Run:

```powershell
git fetch origin
git ls-tree --name-only origin/windows-bench docs/evidence/storage-fold-windows-2026-10-17.md
```

- **If it prints the file name**, task 3 is done. Do not run part 2. Write this line under the
  title of the spike's report: "The storage benchmark on this laptop:
  `docs/evidence/storage-fold-windows-2026-10-17.md` on `windows-bench` (windows-track task 3)."
  Go to "Commit and push".
- **If it prints nothing**, run part 2 below. It writes its own file,
  `docs/evidence/storage-fold-windows-g0-2026-10-17.md`, so it never collides with task 3's.
  Tell the owner that task 3 need not run the benchmark again.

### Running it (about 40 minutes)

1. Make a detached checkout of the storage branch, with its line endings kept byte for byte
   (it creates no local branch, so nothing can be pushed from it):

   ```powershell
   git -c core.autocrlf=false worktree add --detach C:\tc4-g0\storage origin/storage
   git -C C:\tc4-g0\storage log -1 --format=%H
   ```

   If `C:\tc4-g0\storage` already exists, update it instead:
   `git -c core.autocrlf=false -C C:\tc4-g0\storage checkout --detach origin/storage`.

2. Build foldbench in the storage checkout (5 to 10 minutes):

   ```powershell
   cargo build --release -p tc-corpus --example foldbench --manifest-path C:/tc4-g0/storage/Cargo.toml
   ```

3. Run it (one line). It generates the whole-Bible corpus once in `C:\tc4-g0\tc4-g0-corpus`
   (about 0.8 GB, a minute or two), copies it to `C:\tc4-g0\tc4-g0-corpus-nocache` for part 3,
   runs 5 rounds (each: an open without a cache, an open with it, Psalms projected, and the
   app's case, `foldbench app`), then every part once in one process:

   ```powershell
   powershell -NoProfile -ExecutionPolicy Bypass -File spikes/foldbench-windows/run-foldbench.ps1 -Storage C:/tc4-g0/storage -SkipBuild -Report docs/evidence/storage-fold-windows-g0-2026-10-17.md
   ```

4. Read `docs/evidence/storage-fold-windows-g0-2026-10-17.md`. Check that the storage commit
   is a commit hash (not "unknown"), the machine line names `sha` and `avx512f`, Defender's
   real-time protection is "on", the table "The app's case for memory" has numbers, and the
   other tables have numbers. Figures that read "unavailable" are ones Windows did not give;
   leave them.
5. Add the same line about the reference machine under its title.

## Part 3, optional: after a reboot (about 10 minutes)

Only if part 2 ran here, and only if the owner agrees to a reboot now.

1. Commit and push what you have first (see "Commit and push"), so nothing is lost.
2. Reboot. After logging in, open nothing else. Open PowerShell and run, from `REPO`:

   ```powershell
   powershell -NoProfile -ExecutionPolicy Bypass -File spikes/foldbench-windows/run-foldbench.ps1 -Storage C:/tc4-g0/storage -Phase cold -Report docs/evidence/storage-fold-windows-g0-2026-10-17.md
   ```

3. Start Claude Code again in `REPO` and say: "Continue `.audit/tasks/g0-windows.md` after
   part 3." Check that the report now ends with "After a reboot", then commit and push again.

## Commit and push

```powershell
git switch -c g0-windows
git add docs/evidence/electronite-spike-2026-10-17.md docs/evidence/electronite-spike-2026-10-17
git status --short
```

If `g0-windows` exists already, use `git switch g0-windows` instead of the first line. If part
2 ran here, also add its files:
`git add docs/evidence/storage-fold-windows-g0-2026-10-17.md docs/evidence/storage-fold-windows-g0-2026-10-17`.
Add `-rerun` files too if part 1 made any. `git status --short` must list only those files;
never `spikes/electronite/out` or a `target` folder. Then:

```powershell
git commit -m "Record the G0 Electronite spike on Windows"
git push -u origin g0-windows
```

Say "and the storage benchmark" in the message if part 2 ran. End the commit message with the
session lines your system asks for. Push only `g0-windows`. Never push
`claude/focused-ride-5uzj7p` or `storage`.

## If something fails

| What you see | What to do |
|---|---|
| `link.exe` not found, or a linker error | The C++ build tools are missing: install Visual Studio Build Tools 2022 with "Desktop development with C++", open a new terminal, build again. |
| "running scripts is disabled on this system" | Run the script through `powershell -ExecutionPolicy Bypass -File ...` as shown. |
| `-Electron: command not found`, or "missing mandatory parameters" | The command ran in Git Bash. Run it in PowerShell (see the top of this file). |
| "This does not look like a tar archive" | A GNU tar tried to unpack the zip. Use `Expand-Archive` as in the prerequisites. |
| Windows protected your PC (SmartScreen) on `electron.exe` | Run `Unblock-File` on the zip and unpack it again; or choose "More info", "Run anyway". |
| A firewall question about `tc4-standin.exe` | Choose Cancel. The server listens on 127.0.0.1 only and needs no firewall rule. |
| `cargo build` cannot download crates | Check the internet connection and run it again. |
| The spike report lists a case that did not finish | Rerun that case with `-Only` and a `-rerun` report, as in part 1, step 5. Keep both. |
| Section 4 says "none in 60 s" for the first write to disk | Keep it: Chromium did not write `localStorage` to the profile within a minute. Tell the owner. |
| foldbench fails while generating or opening | Keep the error text. Write it, the storage commit and the command into the report under a heading "What failed", commit, and tell the owner. |
| The disk fills | Delete `C:\tc4-g0\tc4-g0-corpus-nocache` (only part 3 needs it) and run the step again. |
| A window of the spike stays open for more than 20 minutes | Close it. The script records the case as not finished and goes on. |
| `git push` asks for a sign-in | The owner signs in through the browser window Git opens. Never type or paste a token or a password into the chat. |
| The report says "A token was found and removed" | Commit the report as it is and tell the owner: it is a bug in the spike, and the file no longer holds the token. |

## What the results decide

- **AD11:** a request type that fails under `filter` is one the header filter misses. The
  design then moves pictures, downloads and printing to capability URLs (APP-DESIGN §5.2). Rows
  marked informational (EventSource, `api-main-thread`) never count as such a failure.
- **The main thread:** `api-main-thread` shows whether the page's `/api/` requests wait for the
  shell's main thread when it is busy. If they do, G1 keeps the mirror's writes and every other
  synchronous task off that thread (AD21).
- **Q-1:** the custom `tc4://` scheme is taken only if it adds less to the aligner's round trip
  than the header filter: compare the painted p95 of `protocol` and `filter` in section 2,
  across both passes. A difference inside the spread between passes does not decide Q-1; the
  report says which applies.
- **Q-13:** if the `filter` painted p95 misses 30 ms in either pass on this laptop, the owner
  decides whether the aligner may run in the page (an exception to the rule that the UI never
  links domain code). The frame column, which stops before layout and paint, is shown only for
  comparison.
- **AD39 and unit G1:** the "end the shell" rows show whether the server outlives the shell or
  dies with it, which decides how G1 starts `tc4`.
- **AD21:** the hard-kill rows show how much typed text, in keystrokes and in seconds,
  `localStorage` and the shell's mirror keep when the whole app dies after Chromium has
  written `localStorage` once.
- **The budgets on the reference machine** (PERFORMANCE.md): the 3 s open with the fold cache,
  the 10 s open without one and after a restart, the 300 ms Psalms projection, and memory (350
  MiB peak, 150 MiB steady) in the app's case, on a laptop with Defender on and, if it has
  none, without SHA instructions. Task 3's file or part 2's file holds them.
