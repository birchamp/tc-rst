# Review of two performance budgets (2026-10-17)

## The question

The reference laptop is a Dell Latitude 7490 (4 cores, 8 GB, SATA SSD, Windows 10, Defender on). It missed two rows of `docs/PERFORMANCE.md` by what looked like a small margin. "Project | Materialize one book | Psalms | 300 ms, in the background" (PERF:101) took 342-401 ms. "Project | Memory | whole Bible open, Psalms in use | 350 MiB peak; 150 MiB steady" (PERF:106) read 192-210 MiB right after the open. The owner asked: "Make sure those cutoffs are necessary for a good user experience. Both of them are very close to the limit."

**Short answer.** Neither number protects anything a person sees. Both started as design estimates and became budgets on 2026-10-15 (commit 2609d49), with no reason recorded. But each row hides a real problem that a person does feel:
- **Materialize.** It holds up saves while it runs. On the laptop it takes about 1.0-1.3 s, not 0.34-0.40 s. The near miss measured only part of the job.
- **Memory.** The row covers only one part of the app, and the laptop figure was read at the wrong moment. With Psalms in use, the project store needs about 310-355 MiB.

Both rows should be replaced by rows that measure what a person feels.

**How this was done.**
- For each budget, one investigation took the measurements. Then an adversarial check re-ran them and corrected them. This review uses the corrected figures and lists what changed.
- Measurements ran on a shared Linux VM (4 vCPU Xeon 2.1 GHz with SHA instructions, 16 GB, load 4-12 from other jobs). They used release builds of `storage` at da6f00e. Each case ran at least twice, interleaved with the others.
- A laptop figure marked "est." is a VM figure multiplied by a ratio. The ratio comes from a row that was measured on both machines.

**Names and terms used below.**
- "PERF" is `docs/PERFORMANCE.md` on this branch.
- "STORAGE" is `docs/STORAGE.md` at storage da6f00e. Code paths are under `crates/tc-store/src/` at the same commit. The branch has since moved to 6787f59, which shifts some STORAGE lines.
- "APP" is `docs/APP-DESIGN.md`. "tC4" is `translationcore4` at d310bf3.
- The "fold" is the app's in-memory index of every change in a project.
- "fsync" forces written data onto the disk.
- "p95" means 95 of 100 runs were at least this fast.

## 1. Materialize one book: 300 ms

**What it is.** A save is durable when "Saved" shows, because it is already in the project's change files. Materializing then writes the derived files from those changes: the book's USFM, alignments, tN and TWL decisions, comments and conflicts, plus `metadata.json` (STORAGE:1815-1874; materialize.rs:176-460). Other programs, a copied folder and Home's project cards read these files (APP:177). Door43 sync and USB export do not read them. They build from memory and from the last commit (STORAGE §5.13 C3, STORAGE:3364-3366).

**Where 300 ms came from.**
- No reason is recorded. `git log -S` finds only 2609d49, and nothing in `docs/`, `tracker:research/` or `docs/rebuild.tsv` explains the number.
- That same week, STORAGE estimated the whole job at 20-60 ms (59c550b:docs/STORAGE.md:2965). It still says "A materialization lasts milliseconds" (STORAGE:83).
- 300 ms is also rule 4's threshold for showing progress (PERF:28-33). A job that ends sooner never shows an indicator.
- The real job takes 10-20 times the old estimate.

**What it protects for a person.** Nothing directly, because nobody watches a materialize. What matters is where it runs. It runs on the project actor: the single thread that takes every save, decision and alignment save from one queue (actor.rs:75-93, 180-183). A save sent while a materialize runs waits until it ends. That breaks rule 4's promise that background work "lets saves continue" (PERF:28), and STORAGE's I9 (STORAGE:85). So today the 300 ms is in effect the longest a save may wait.

**When a person waits, and for how long on the laptop.**
- **Saving.**
  - A materialize of the whole book runs 3 s after the last save (actor.rs:27, 130-135), when the person leaves a book or mode (handle.rs:647-655), and inside every checkpoint (actor.rs:206-220).
  - One Psalms verse save marks every Psalms file for rewriting (project.rs:93-105).
  - A save that lands in that window waits about 0.3-1.3 s (est.). The budget is 25 ms (PERF:84, 89).
  - "Saving…" stays up meanwhile. Every move to another mode, another book, Home or an export also waits (APP:1273-1276).
  - A checker who clicks every 3-6 s lands in the window often. A rough estimate is 15-30% of clicks.
- **The whole project at once** (found by the challenge).
  - These actions mark every derived file: adding, removing, restoring or rescoping a book, changing resource pins (which resource versions the project uses), and a resource or gateway upgrade (project.rs:95-97, 106-109).
  - The next materialize then renders all 329 files on the actor. That takes 5.1-10.7 s on the VM and about 10-25 s on the laptop (est.).
  - A save sent during it waited 0.9-8.3 s on the VM.
- **Screens.** Book reads share a thread pool with the render (handle.rs:670-690; tc-journal fold/project.rs:157-160). On the VM, a Genesis read went from 9-11 ms to a p95 of 186-296 ms during a materialize. That threatens the "open a book 150 ms" budget (PERF:83) right after the person leaves a book.
- **Sync and USB export.** Both start with a checkpoint (STORAGE:3090, 3364). After a recent save it takes about 0.7-1.5 s (est.). The checkpoint budget is 500 ms (PERF:102).
- **Going Home.** The project closes in the background.
  - Reopening that project, or syncing it from Home, waits for the close to end (APP:771-772). That is about 1.4-3.5 s (est.) after a save.
  - Until the close ends, Home's card shows the old drafted percent (APP:177).
- **Another program holding a derived file open** (Paratext, an indexer). The actor stalls 2 s for each blocked file, roughly every 5 s, until the other program lets go (materialize.rs:27, 542-553; actor.rs:196-200).
- **An app update that changes how files are derived.** Every file is rewritten before the project opens (STORAGE:2055-2066). The estimate is 30-40 s on the laptop; the open budget is 10 s (PERF:100). No row covers this case.

**What tC4 4.0 does.**
- It writes the derived file inside every save, after folding the whole history again (tC4 src/data/journal/journalingStore.ts:929-962, 2322-2370). With tC4's own code on the VM, that is about 9.5 s per Psalms save in a whole Bible.
- Its checkpoint runs in the same queue as saves (journalingStore.ts:2323, 3262-3283) and starts at every mode switch (tC4 src/state.jsx:4033-4037). So tC4 also makes saves wait.
- The rebuild is already far faster. Row A below therefore rests on the rebuild's own rule 4, not on matching tC4.

**Numbers measured.**

| Row | Laptop | VM, quietest round | VM, other rounds |
|---|---|---|---|
| Psalms projection (rendering only), median / p95 | 342-401 / 384-490 ms | 164 / 233 ms | 245-512 ms median |
| Psalms USFM alone (one thread), median | 35.9-47.0 ms | 11.8-12.4 ms | |
| Psalms materialize after a verse save, median / p95 | est. 1.0-1.3 s | 382 / 476 ms | 505-1227 ms median |
| Psalms materialize after a decision, median | est. 1.0-1.4 s | 473 ms | 610-1001 ms |
| Jude materialize after a verse save, median | est. 0.16-0.3 s | 76 ms | 87-224 ms |
| A save sent during a materialize, median / p95 | est. 0.3-1.3 s | 301 / 734 ms | 478-679 ms median; worst 1.6 s (4.4 s at load 10-12) |
| A save with nothing else running, median / p95 | | 8.2 / 27.2 ms | 7.4-19.8 ms median |
| Checkpoint after one save, median | est. 0.7-1.5 s | | 319-694 ms (three runs) |
| Close after one save | est. 1.4-3.5 s | | 646-1701 ms (four runs) |
| Whole-project materialize | est. 10-25 s | | 5.1-10.7 s; peak memory 560-574 MiB |

How the laptop estimates were made:
- The projection uses several threads. That row is 2.1-2.6 times slower on the laptop than in the quiet VM round.
- The USFM row uses one thread. It is 2.9-4.0 times slower on the laptop.
- About half of a materialize is one-thread work: reading back and hashing 18.7 MB, writing, and listing every derived file in the project. So the laptop figure sits at the top of the range, about 1.0-1.3 s.
- The laptop therefore misses 300 ms by three to four times, not by a small margin.
- Defender's scan of new files and NTFS write costs are not included. Nothing measures them yet.

**What the challenge changed.**
- It found the whole-project materialize. Its 560-574 MiB peak also breaks the memory row.
- It raised the laptop estimate from 0.8-1.0 s to 1.0-1.3 s, using the one-thread ratio.
- tC4 also makes saves wait behind its checkpoint. The investigation had said it did not.
- A verse save must rewrite the decisions files too, because they carry per-verse text (tc-journal fold/project.rs:901, 950-952). Rewriting only the USFM and alignments is not possible.
- A count test cannot require the same total work on both corpora. `metadata.json` is rewritten whole every time (materialize.rs:615-700), and rule 1 exempts background work (PERF:12-15, 150-152).
- A file held open stalls the actor 2 s per file, about every 5 s. It does not delay every save by 2 s.
- A 1 s row conflicts with rule 4 unless it says that materialize shows no indicator.

**Recommendation: reframe.** Replace the 300 ms row with rows that measure what a person feels.
- **A. New row:** "Editor/Checking | Save a verse, record a decision or save an alignment while a materialize, checkpoint, merge commit or upgrade write runs, including a whole-project materialize | Psalms in the whole Bible | 25 ms p95 including fsync."
  - Why this number: it is the save budget of PERF:84 and 89. "Saved" and every move between screens wait on it, and rule 4 already promises it.
  - It needs APP's R21 (APP:1819): render and write from a snapshot, off the actor, with the derived-file fsyncs paced behind saves.
- **B. Replaces PERF:101:** "Project | Materialize one book, off the actor, from a snapshot | Psalms | 1 s p95 on the reference laptop; it shows no indicator and no screen waits for it (an exemption from rule 4); open a book 150 ms, open Check 250 ms and chapter 16 ms hold while it runs."
  - Why 1 s: nobody watches the materialize itself. The number bounds the rare waits that do show: a sync's checkpoint phase, quitting, and reopening after Home. It also bounds how far files on disk lag a save (3 s plus 1 s). About 1 s keeps a person's train of thought.
  - Today's estimate is 1.0-1.3 s, so meeting 1 s needs three fixes:
    - Drop the whole-project listing (materialize.rs:191-197), which costs 35-67 ms.
    - Hash each buffer once instead of five times (materialize.rs:233, 279, 298, 349, 543).
    - Skip reading back a file that is known to be unchanged.
- **C. New count test (rule 2):** materializing Jude after one save projects and reads back only Jude's files, both on the corpus and on the corpus made ten times larger. This test fails today.
- **D. New row:** "Project | Close a project | whole Bible, Psalms changed | 3 s on the reference laptop."
  - Why 3 s: a reopen or a sync from Home waits for the close, and the close must fit inside the 5 s the app allows at quit (APP:947-950).
- **E. New row:** "Project | Materialize many or all derived files (a sync intake, a derivation update, a book added, removed, restored or rescoped, a pins change, a resource upgrade) | whole Bible | background work under rule 4: progress, saves continue, off the actor, one book at a time, inside the memory peak of M2."
  - Why: today this takes about 10-25 s on the laptop, and saves are blocked the whole time.
  - A book change should also mark only that book's files, as STORAGE B5 says (STORAGE:1502).

The Checkpoint row (PERF:102, 500 ms) has the same cause: 319-694 ms median on the VM. It should be reviewed next.

**Risks.**
- **Relaxing alone helps nobody.** If the row moves to 1 s while materialize stays on the actor, it only makes 1 s save waits legal. B must go together with A and R21.
- **Off the actor, saves still share the disk.** On the VM, a save next to large fsyncs rose to 23-33 ms p95, at the edge of 25 ms. Windows is not measured.
- **Writing from a snapshot needs safeguards.** Before each rename, a check must make sure a newer save is never overwritten (I8, STORAGE:84). The same crash tests must pass (STORAGE:2303-2308).
- **Files on disk lag.** They can be up to about 4 s behind a save. Nothing is lost, because the change files hold every save.
- **The laptop figures are estimates** and leave out Defender.

## 2. Memory: 350 MiB peak, 150 MiB steady

**What it measures.** Only the project store inside `tc4 serve`: the fold plus tc-store (PERF:161; storage:docs/evidence/storage-fold-2026-10-16.md:12). It leaves out the window (Electronite), the page, resources and the suggestion model. PERF has no memory row for the whole app.

**Where it came from.**
- STORAGE estimated "≈ 100 MiB typical, ≈ 350 MiB peak", at about 100 bytes per change, and called this an estimate (59c550b:docs/STORAGE.md:2970).
- The same day, commit 2609d49 turned it into "350 MiB peak; 150 MiB steady", with no reason given (docs/rebuild.tsv:33). 150 looks like the estimate plus a margin.
- STORAGE's known limit 16 says that meeting 150 MiB needs the fold to keep only the current values and the open books' history in memory (STORAGE:4394-4399).

**What it protects for a person.**
- No screen waits on memory. What a person can feel is the whole laptop running short of memory.
- Windows sees 7.9 GiB. It shares that with Defender, a browser, Word and Paratext. Past that limit, Windows moves memory to disk, and switching back to another program takes seconds (an estimate of 1-5 s; not measured).
- A typical mix of those programs leaves about 2.3-2.8 GB free. This comes from forum reports and was not measured.
- The 60 MiB between 150 and 210 MiB is 0.74% of the laptop's memory, about one browser tab.
- The row's other intent still matters: memory should not grow without limit as a project ages.

**When a person waits.**
- **Opening a project.** The open's memory peak is brief and happens behind a progress bar. Memory does not change how long the open takes.
- **A sync or USB export from Home while another whole Bible is open** (found by the challenge).
  - This holds two folds (APP:778-782, 2018).
  - APP's Q-15 asks whether to allow that or make the second open wait. It answers "Allow; G2 measures" (APP:2053).
  - The memory row's planned test uses one screen session and one side session (APP:1740).
  - Two folds take about 570-580 MiB for the store alone.
- **Years of use.**
  - Each year of a three-device team adds about 46-53 MiB to the steady figure and 70-77 MiB to the open's peak.
  - At twice today's history, the open peaks at 470-512 MiB.

**What tC4 4.0 does.**
- It has no memory budget.
- Its data layer alone, folding a checked whole Bible in Node, peaked at 838 MiB with 278,041 changes and at 1,033 MiB with 576,195 (docs/evidence/journal-decision-2026-10-16/results/all-combined.md:81, 91).
- Its worker keeps a second copy of the changes (tC4 src/data/journal/foldRunner.ts:10-14). With its window, the app needs 1.1-1.4 GiB or more.

**Numbers measured.**
- "Private" means memory that only this program uses: private bytes on Windows, anonymous memory on Linux.
- Linux limited to one heap (MALLOC_ARENA_MAX=1) behaves like Windows, whose heap has no per-thread pools.
- With one heap, laptop over VM is 0.97-1.07 after the open and 0.99-1.00 at the peak. The laptop estimates use that ratio.

| Figure | Laptop (working set unless marked private) | VM, one heap | VM, default Linux heap |
|---|---|---|---|
| foldbench, right after the open | 192-210 MiB | 197 MiB | 218-230 MiB |
| foldbench, peak of the open | 269-273 MiB; 282-283 MiB private | 271 MiB | 289-294 MiB |
| foldbench, a minute after the Psalms steps | 45-114 MiB, read during exit (below) | 242-259 MiB | 303-307 MiB |
| First open without the fold cache, peak | 332-333 MiB; 368-369 MiB private | | |
| tc-store, right after the open | est. 242-269 MiB | 249-251 MiB | 280-289 MiB |
| tc-store, Psalms in use, held a minute | est. 310-355 MiB private | 319-329 MiB private | 405-407 MiB |
| tc-store, peak during Psalms work | est. 360-365 MiB private | 346-351 MiB | 465-469 MiB |
| tc-store, peak at close (writes the fold cache) | est. 400-410 MiB private | 388-392 MiB | 467-469 MiB |
| Growth per added change | | 233-234 B | 256-284 B |
| Window: Electronite with tC4's own page (Linux VM) | | 189-213 MiB private | |

**Why the laptop's 45-114 MiB is not the steady figure.**
- It was read after foldbench's last line, while the process was freeing the fold and exiting (spikes/foldbench-windows/run-foldbench.ps1:176-190, 301-312; storage crates/tc-corpus/examples/foldbench.rs:1225-1230).
- On the VM, the fold stays resident, and its memory stays flat, for 60 s after the steps.
- So the laptop's real steady figure is well above 150 MiB.

**Whole app on the laptop (est.).**
- One session: window 190-213 MiB, store 310-355 MiB, resources up to 120 MiB (PERF:97, two books, APP:813), lexicons 10-30 MiB. That is about 0.6-0.7 GiB.
- A side session adds a second fold of about 250-270 MiB. Counting peaks, the challenge estimates 0.9-1.15 GiB.

**What the challenge changed.**
- The store's "Psalms in use" figure had been estimated by adding numbers, not measured. Measured through tc-store, it is 72-82 MiB above the open. So the laptop needs about 310-355 MiB, not 290-320, and the 300 MiB the investigation proposed would fail on the day it was adopted.
- Closing a project builds the 64.5 MB fold cache in memory (actor.rs:338-350). The close peaks at 388-392 MiB, above a 350 MiB peak.
- The first open without a cache already reaches 368-369 MiB private bytes on the laptop (docs/evidence/storage-fold-windows-g0-2026-10-17.md:53).
- It found the side session.
- The proposed whole-app row counted two things in different ways. On Linux it counted the window by PSS, which splits shared libraries between processes. On Windows it counted private memory. These differ by about 140 MiB. Both now count private memory.
- foldbench after the steps with one heap is 242-259 MiB, not 242.
- 90% of 7.9 GiB is 7.6 GB, so a typical laptop's headroom is 2.3-2.8 GB.

**Recommendation: reframe.** Replace PERF:106 with three rows.
- **M1. New row:** "App | Memory | whole Bible open (about 620,000 changes), Psalms in use, read a minute after the last action | 1 GiB steady; 1.25 GiB peak with a side session (a sync or export from Home beside another whole Bible)."
  - What it counts: the summed private working set of `tc4 serve` and every Electronite process (Task Manager's Memory column). On Linux, summed private memory (USS).
  - Owner: tc-app with the shell.
  - Why: the total protects the person, not any one part. 1 GiB keeps tC4 under half of a typical laptop's headroom. Today's estimate is 0.6-0.7 GiB for one session and 0.9-1.15 GiB with a side session. Both are below tC4 4.0.
- **M2. Replaces PERF:106:** "Project | Memory of the project store (fold and tc-store, without resources), measured through tc-store: open, Psalms saves and decisions, leaving Psalms, close | whole Bible, Psalms in use | 400 MiB steady private bytes, read a minute after the last action, never just before exit; 450 MiB peak private bytes, including the close and the first open without a cache."
  - Why: the laptop estimate is 310-355 MiB steady, 360-365 MiB at the peak and 400-410 MiB at close. 400 and 450 leave at least 10% room, so the row fails on a real regression or on years of growth, not on noise.
  - 400 MiB is about 5% of the laptop's memory.
- **M3. New row:** "Project | Memory growth with history | whole Bible at twice today's history (1.2 million changes, `foldbench save bible-share 2.0`) | steady memory grows by at most 300 B per added change with one heap, and by under 350 B with the default Linux heap."
  - Why: this is the part of the original intent that still protects a person. A project kept for 6-10 years must not drift into paging without anyone noticing.
  - Measured today: 233-234 B with one heap and 256-284 B with the default heap.
  - The row sets no total at 1.2 million changes, because 470-512 MiB is already measured there.

**Risks.**
- **Less pressure on the real fix.** Relaxing 150 MiB removes the pressure for the one change that bounds memory: keeping old history out of memory (STORAGE:4394-4399). Without it, a three-device team adds about 50 MiB a year. At 2 million changes, the store holds about 520-560 MiB steady, close to tC4. M3 guards against this.
- **Much of the whole-app figure is estimated.** Not measured: the window on Windows (its GPU process may be larger), the rebuild's own page, resources, lexicons and the suggestion model.
- **Outside figures are weak.** The figures for Windows and for other programs come from forum reports.
- **The exit explanation is unconfirmed.** The explanation of the laptop's 45-114 MiB has not been checked on the laptop itself.
- **The Linux heap costs more.** A Linux build with the default heap carries about 65 MiB more.

## Owner's ruling (2026-10-18)

1. **Materialize: yes.** "Adding a book is a task that people expect to take time, and a progress bar makes sense. Autosave needs to happen in a way that doesn't interrupt the user." Rows A, B, D and E and count test C replace the 300 ms row in `docs/PERFORMANCE.md`; a whole-project materialize shows a progress bar and saves continue meanwhile.
2. **Memory: yes.** Rows M1-M3 replace the 350/150 MiB row. The owner asked to compare the limits with other programs a translator uses (for example Microsoft Word with Psalms open) on the reference laptop, so we know they are reasonable: task 5 of `tracker:tasks/windows-track.md`.

## What the owner was asked

1. **Materialize.** Should "Materialize one book | Psalms | 300 ms" be replaced with rows A-E? Under those rows, a save waits at most 25 ms while background work runs, and a book's files are written off the actor within 1 s on the laptop. The condition is that the storage team moves that work off the actor (R21). Yes or no?
2. **Memory.** Should "350 MiB peak; 150 MiB steady" be replaced with rows M1-M3? Those rows set the whole app at 1 GiB steady (1.25 GiB with a side session), the project store at 400 MiB steady and 450 MiB peak in private bytes, and growth at most 300 B per change. Yes or no?

## What unit P does meanwhile

Unit P measures what a person feels. It does not chase a number nobody feels (commit 4f53e88).
- It does not try to bring the on-actor Psalms materialize under 300 ms. It does not rework the fold's tables to reach 150 MiB.
- It measures save and decision latency while a materialize, a checkpoint, a close or a whole-project materialize runs (R9, APP:1807). It also measures screen reads during them.
- It moves the materialize, checkpoint, close and memory-hold cases into `storebench`, commits them, and runs them on the Dell 7490 with Defender on. It reads memory as private bytes a minute after the last action, never at exit.
- It adds count test C and growth test M3.
- It lands the fixes that shorten real waits:
  - no whole-project listing
  - one hash per buffer
  - no read-back of unchanged files
  - a book change that marks only that book's files

Raw logs and bench sources are not committed. They are in these folders: `/tmp/claude-0/-home-user-tc-rst/4a142024-63e1-59b8-bcc4-3b15bbfdf386/scratchpad/matbin/`, `/tmp/claude-0/-home-user-tc-rst/4a142024-63e1-59b8-bcc4-3b15bbfdf386/scratchpad/rev-mat/`, `/tmp/claude-0/-home-user-tc-rst/4a142024-63e1-59b8-bcc4-3b15bbfdf386/scratchpad/budget-mem/` and `/tmp/claude-0/-home-user-tc-rst/4a142024-63e1-59b8-bcc4-3b15bbfdf386/scratchpad/budget-mem-check/`.