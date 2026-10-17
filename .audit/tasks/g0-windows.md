# Windows task: the Electronite spike G0 and the storage benchmark

Run this in a Claude Code session on the owner's Windows laptop, from start to finish. The
owner is at the computer. Plan for about 90 minutes: up to 45 for the installs, 10 for part 1,
30 for part 2, and an optional reboot for part 3.

## What this is for

The rebuild's app runs inside Electronite, an Electron fork that shapes Graphite fonts. The
design (`docs/APP-DESIGN.md`) leaves questions that only a run on Windows can answer:

- Does the shell's way of adding the security token reach every kind of request the browser
  makes by itself (pictures, help pages, fonts, downloads, printing)? (AD11)
- Is a custom `tc4://` scheme faster or slower than that header filter for the aligner? (Q-1)
- Is one aligner click, with a 20 KB reply, under 30 ms on this laptop? (Q-13)
- When the server or the shell dies, does the other side behave? (AD39)
- After a crash, how much typed text do `localStorage` and the shell's mirror file keep? (AD21)
- Does the storage layer meet its budgets on a Windows laptop with Defender on? (PERFORMANCE.md)

Part 1 runs the spike in `spikes/electronite`. Part 2 runs the storage benchmark. The scripts do
all the measuring and write the result files. Your job is to install, run, check, and commit.

## Rules

- Change no code. If a script stops with an error you can fix in the script itself (a path, a
  PowerShell detail), fix it in a separate commit on `g0-windows` and say what and why. Never
  change a script so that a result looks better.
- Do not change the storage branch, `claude/focused-ride-5uzj7p` or anything under `crates/`.
- Keep Windows Defender real-time protection on. Add no exclusions. Do not change the power
  plan. These are the conditions the budgets are for.
- Never record a token or a password, in a file, a commit or a message. The scripts never
  write the server's token. Never commit `spikes/electronite/out/`: its Electronite profiles
  hold the session's token (it is useless after the run, but it stays private).
- Push only the new branch `g0-windows`.

## Prerequisites

Check each with the command in brackets. Install only what is missing.

1. **Git for Windows**, any 2.4x or newer, from https://git-scm.com/download/win. [`git --version`]
2. **Rust stable 1.97.0 or newer with the MSVC toolchain.** Download `rustup-init.exe` from
   https://rustup.rs and run it. Accept the default host `x86_64-pc-windows-msvc`. If it says the
   Visual C++ build tools are missing, let it install them (Visual Studio Build Tools 2022 with
   "Desktop development with C++": the MSVC compiler and the Windows SDK; a download of a few GB).
   Then open a new terminal. [`rustc --version`, `cargo --version`, `rustup show` names
   `x86_64-pc-windows-msvc`]
3. **Node.js is not needed.** Electronite carries its own.
4. **Electronite v37.1.0-graphite for Windows x64**, the same build tC4 ships
   (`translationcore4/docs/PACKAGING.md`, "The wrapper is Electronite"). In PowerShell:

   ```powershell
   mkdir C:\tc4-g0 -Force; cd C:\tc4-g0
   Invoke-WebRequest -Uri https://github.com/unfoldingWord/electronite/releases/download/v37.1.0-graphite/electronite-v37.1.0-graphite-win32-x64.zip -OutFile electronite.zip
   (Get-FileHash electronite.zip -Algorithm SHA256).Hash
   ```

   The hash must be `8146CA21FA090E12BCA01A8AF73D234D9A31D90F51DA0DA86BB0E1731C371B52`. If it is
   not, stop and tell the owner. Then:

   ```powershell
   Unblock-File electronite.zip
   mkdir electronite -Force; tar -xf electronite.zip -C electronite
   Test-Path C:\tc4-g0\electronite\electron.exe
   ```

   The last line must print `True`.
5. **Disk:** at least 12 GB free on `C:`. [`Get-PSDrive C`]
6. **The laptop:** plugged in, other programs closed, no other heavy work running.

## Get the code

Use `core.autocrlf=false`: the corpus and fixtures must keep their line endings byte for byte.

```powershell
cd C:\tc4-g0
git clone -c core.autocrlf=false https://github.com/birchamp/tc-rst.git rebuild
cd rebuild
git checkout claude/focused-ride-5uzj7p
git worktree add --detach C:\tc4-g0\storage origin/storage
git log -1 --format=%H; git -C C:\tc4-g0\storage log -1 --format=%H
```

If `C:\tc4-g0\rebuild` already exists, run `git fetch origin`, `git checkout
claude/focused-ride-5uzj7p` and `git pull --ff-only` in it instead, and update the storage
checkout with `git -C C:\tc4-g0\storage checkout --detach origin/storage`. The storage checkout
is detached on purpose: it creates no local branch, so nothing can be pushed from it.

Confirm that `spikes\electronite\run-spike.ps1` and `spikes\foldbench-windows\run-foldbench.ps1`
exist. If they do not, `claude/focused-ride-5uzj7p` does not have G0 yet: stop and tell the owner.

## How to run the commands

Run every command from `C:\tc4-g0\rebuild` in PowerShell. From Git Bash, prefix each script
with `powershell.exe -NoProfile -ExecutionPolicy Bypass -File` and use forward slashes; that
works the same. The builds and part 2 run longer than 10 minutes, so start them as background
commands and check their output every few minutes. Each script prints one line per step.

## Part 1: the Electronite spike (about 10 minutes)

1. Build the stand-in server first (2 to 5 minutes; it downloads crates):

   ```powershell
   cargo build --release --manifest-path spikes/electronite/Cargo.toml
   ```

2. Tell the owner: windows will open and close by themselves for about 5 minutes; do not touch
   the keyboard or the mouse, and do not cover or minimise the windows.
3. Run every case:

   ```powershell
   powershell -NoProfile -ExecutionPolicy Bypass -File spikes/electronite/run-spike.ps1 `
     -Electron C:/tc4-g0/electronite/electron.exe `
     -Server spikes/electronite/target/release/tc4-standin.exe `
     -Report docs/evidence/electronite-spike-2026-10-17.md
   ```

4. Read `docs/evidence/electronite-spike-2026-10-17.md`. Check that:
   - the Electronite line says 37.1.0 and names a Chromium version;
   - the machine table has the CPU, cores, RAM, disk, power and Defender state;
   - sections 1 to 5 have rows. A section "Cases that did not finish" means a case failed twice.
5. If a case did not finish, run the script once more with `-Only <case>` (for example
   `-Only endshell`) and `-Report docs/evidence/electronite-spike-2026-10-17-rerun.md`, and
   keep both files. Do not delete a report that shows a failure.
6. Add one line under the title of the report: whether this laptop is the reference machine (a
   five-year-old laptop with 4 cores, 8 GB and a SATA SSD) or a stand-in, with its model and age
   if the owner knows them.

What the script does, so you can explain it: it builds nothing else and changes nothing outside
`spikes/electronite/out/` and the report. For each of three ways to give the page its token it
probes every request type, then times 200 aligner clicks alone and 200 while pictures and a
help page load. Then it ends the server, closes the window, ends the shell's process (twice:
with the server started plainly and detached), and ends the shell while the page types, to see
what was kept.

## Part 2: the storage benchmark (about 30 minutes)

1. Build foldbench in the storage checkout (5 to 10 minutes):

   ```powershell
   cargo build --release -p tc-corpus --example foldbench --manifest-path C:/tc4-g0/storage/Cargo.toml
   ```

2. Run it. It generates the whole-Bible corpus once in `C:\tc4-g0\tc4-g0-corpus` (about 0.8 GB,
   a minute or two), copies it to `C:\tc4-g0\tc4-g0-corpus-nocache` for part 3, runs 5 rounds
   of opens (about 5 minutes), then every part once in one process (a few minutes):

   ```powershell
   powershell -NoProfile -ExecutionPolicy Bypass -File spikes/foldbench-windows/run-foldbench.ps1 `
     -Storage C:/tc4-g0/storage -SkipBuild `
     -Report docs/evidence/storage-fold-windows-2026-10-17.md
   ```

3. Read `docs/evidence/storage-fold-windows-2026-10-17.md`. Check that the storage commit is a
   commit hash (not "unknown"), the machine line names `sha` and `avx512f`, Defender's
   real-time protection is "on", and the tables have numbers. Figures that read "unavailable"
   are ones Windows did not give; leave them.
4. Add the same line about the reference machine under its title.

## Part 3, optional: after a reboot (about 10 minutes)

Do this only if the owner agrees to a reboot now.

1. Commit what you have first (see "Commit and push"), so nothing is lost.
2. Reboot. After logging in, open nothing else. Open PowerShell and run:

   ```powershell
   cd C:\tc4-g0\rebuild
   powershell -NoProfile -ExecutionPolicy Bypass -File spikes/foldbench-windows/run-foldbench.ps1 `
     -Storage C:/tc4-g0/storage -Phase cold `
     -Report docs/evidence/storage-fold-windows-2026-10-17.md
   ```

3. Start Claude Code again in `C:\tc4-g0\rebuild` and say: "Continue
   `.audit/tasks/g0-windows.md` after part 3." Check that the report now ends with "After a
   reboot", then commit and push again.

## Commit and push

```powershell
git switch -c g0-windows          # or: git switch g0-windows, if it exists
git add docs/evidence/electronite-spike-2026-10-17.md docs/evidence/electronite-spike-2026-10-17
git add docs/evidence/storage-fold-windows-2026-10-17.md docs/evidence/storage-fold-windows-2026-10-17
git status --short                # only those files; never spikes/electronite/out or target
git commit -m "Record the G0 Electronite spike and the storage benchmark on Windows"
git push -u origin g0-windows
```

Add `-rerun` files too if part 1 made any. End the commit message with the session lines your
system asks for. Push only `g0-windows`. Never push `claude/focused-ride-5uzj7p` or `storage`.

## If something fails

| What you see | What to do |
|---|---|
| `link.exe` not found, or a linker error | The C++ build tools are missing: install Visual Studio Build Tools 2022 with "Desktop development with C++", open a new terminal, build again. |
| "running scripts is disabled on this system" | Run the script through `powershell -ExecutionPolicy Bypass -File ...` as shown. |
| Windows protected your PC (SmartScreen) on `electron.exe` | Run `Unblock-File` on the zip and unpack it again; or choose "More info", "Run anyway". |
| A firewall question about `tc4-standin.exe` | Choose Cancel. The server listens on 127.0.0.1 only and needs no firewall rule. |
| `cargo build` cannot download crates | Check the internet connection and run it again. |
| The spike report lists a case that did not finish | Rerun that case with `-Only` and a `-rerun` report, as in part 1, step 5. Keep both. |
| foldbench fails while generating or opening | Keep the error text. Write it, the storage commit and the command into the report under a heading "What failed", commit, and tell the owner. |
| The disk fills | Delete `C:\tc4-g0\tc4-g0-corpus-nocache` (only part 3 needs it) and run the step again. |
| A window of the spike stays open for more than 20 minutes | Close it. The script records the case as not finished and goes on. |
| `git push` asks for a sign-in | The owner signs in through the browser window Git opens. Never type or paste a token or a password into the chat. |
| The report says "A token was found and removed" | Commit the report as it is and tell the owner: it is a bug in the spike, and the file no longer holds the token. |

## What the results decide

- **AD11:** a request type that fails under `filter` is one the header filter misses. The
  design then moves pictures, downloads and printing to capability URLs (APP-DESIGN §5.2).
- **Q-1:** the custom `tc4://` scheme is taken only if it adds less to the aligner's round trip
  than the header filter (compare the `protocol` and `filter` rows of section 2).
- **Q-13:** if the `filter` frame p95 misses 30 ms on this laptop, the owner decides whether
  the aligner may run in the page (an exception to the rule that the UI never links domain
  code).
- **AD39 and unit G1:** the "end the shell" rows show whether the server outlives the shell or
  dies with it, which decides how G1 starts `tc4`.
- **AD21:** the hard-kill rows show how much typed text `localStorage` and the shell's mirror
  keep when the whole app dies.
- **The budgets on the reference machine** (PERFORMANCE.md): the 3 s open with the fold cache,
  the 10 s open without one and after a restart, the 300 ms Psalms projection, and memory (350
  MiB peak, 150 MiB steady), on a laptop with Defender on and, if it has none, without SHA
  instructions.
