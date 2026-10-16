# Should the rebuild use tC4's journaling method?

Decision memo for the project owner. 2026-10-16. Status: revised after a red-team review.
Corrections the red team made in the body are marked *(RT-n)*; the section "Red team" lists
each one with what the text said before and the evidence. Its last part, "How each point was
answered", says what was accepted and what changed. Changes made in answer are marked *(A-n)*.
The benchmark results, scripts and probes are in `docs/evidence/journal-decision-2026-10-16/`.
Paths that start with `results/`, `bench/`, `redteam/` or `memo-probe/` are relative to it.

You wrote: "you may want to consider using tC4's current journaling method. We did a lot of speed
testing on that." This memo answers that suggestion. It compares three options, using tC4's code
at d310bf3, tC4's own speed records, the rebuild's J1 and J2 work, and benchmarks of both systems
run on one machine today.

## Decisions (owner, 2026-10-16)

- **Option B.** Moving a team from tC4 4.0 to the rebuild is one way (§13.3). Rebuild users and
  tC4 4.0 users do not share a Door43 project. Option A is not taken.
- **No way back.** Plan step 8 (an export to tC4 4.0 without history) is dropped. A tC4 4.x
  straggler's sync still refuses without overwriting, and the §13.3 bridge still converts
  segments pushed after a move, so no straggler's work is lost.
- **A first open may take up to 10 s, with progress.** This covers a project with no usable
  fold cache (received from a teammate, imported, or after the cache encoding changes) and the
  first open after a restart with a cold disk cache. An ordinary open keeps the 3 s budget.
- **The reference machine is a five-year-old Windows laptop.** Many such laptops (for example
  Intel 10th-generation Core U-series) have no SHA instructions, so budgets must hold without
  them. The owner's quiet Ice Lake run (`docs/LOCAL-BENCH-FINDINGS-2026-10-16.md`) shows what
  SHA instructions change: hashing 6.5 times faster, a first open without a cache in 3.8 s.
- **Question 3 is settled by AGENTS.md:** when two people judge a check or an alignment
  differently, both are kept and shown (P3). tC4's newest-wins rule does not apply.
- **Question 6: a tC4 4.0.x "project moved" message only if tC4 knows where the project moved
  to.** The move writes `ingredients/tc/moved.json` naming the rebuild by an application id;
  tC4 shows the notice only for an id it ships knowing, with the name and download page from
  its own table, and otherwise keeps today's refusal. The request to the tC4 team is
  `docs/TC4-REQUEST-MOVED-NOTICE.md`.
- **Units C, S1, S2, P, X and I go ahead** under B, with plan steps 1 to 7 and 9.

## The answer in short

- **Do not adopt tC4's journal format as the rebuild's storage format (option A).** It brings
  back tC4's structural, clock and refusal losses (L1 to L6, L12, L13, L16, L25) and its sync
  rules (L14, L22). The red team agrees. PLAN.md's P3 fails for those reasons.
- **Keep STORAGE.md's change model and fold (J1, J2).** Options B and C both keep them, and the
  next storage units (unit C, then S1, S2 and P) are the same under both. Work can go on now. *(A-1)*
- **Choose between B and C by your answer to question 1, before units X and I start.** If
  rebuild users and tC4 4.0 users must share one Door43 project during the pilot, choose C.
  If a one-way move is acceptable, choose B. B then needs a way back for a team that moved
  (plan step 8). *(A-1)*
- **B does not yet meet the 3 s open, and nobody has measured a full open.** The J2 fixer's
  fold cache (uncommitted, unreviewed) gives 2.6 s, but only for load and fold (O8), with a
  warm page cache. *(RT-3)* The next step, O9, as STORAGE.md writes it, projects every derived
  file at every open, and that took about 10 s wall (15 s of CPU) here. O9 must compare heads
  files instead (plan step 2). *(A-2)*
- **tC4's method is slower to open, mainly because it stores one file per action and keeps
  no fold from one open to the next.** *(RT-1, RT-4)* Its fold also projects every book, so
  the fold rows compare different stages, and with every derived file projected the rebuild
  used more CPU (16.2 s against 14.0 s). The open is the closer comparison. From local files on
  one thread, tC4 took 37.7 s for 278,041 events and 73.3 s for 576,195 (projection included),
  the rebuild 17.6 s for 310,422 changes and 31.6 s for 617,143 (no projection). tC4 never
  measured a whole-Bible open in its app; its own records estimate 3 to 6 minutes.
- **A Rust port of tC4's method pays for its file count.** *(RT-2, RT-11)* Listing and
  reading 617,143 files takes 2.8 to 3.1 s on 4 threads here; listing them and reading only
  each file's size and time takes 1.4 to 2.3 s. Without a cache on either side, a port (6 to
  10 s, estimate) and B (about 6.4 s quiet at HEAD, 10 to 14 s loaded, O9 not included)
  overlap.
- **B's design is younger than tC4's.** *(A-3)* tC4 has written segments since 2026-08-19, and
  its journal suite passes 368 of 368 checks. B has a fold and nothing below it: 16 of its 34
  closures in §6 are by design only, waiting for units not built.
- **Adopt three tC4 lessons now.** Hash each byte once, use a faster SHA-256 (`ring`), and show
  progress after 300 ms off the screen thread. With the first two, the first open without a
  cache falls to about 5 s on a quiet 4-core VM without SHA instructions (estimate). *(A-4)*

The questions only you can answer are in section 13. The main one is whether rebuild users and
tC4 4.0 users must share one Door43 project at the same time.

---

## 1. The two methods in brief

**tC4 at d310bf3.** Each user action is one sealed file:
`ingredients/checking/journal/<actorId>/segments/<firstTs>.action.json`, holding
`{container, body, sha256}`. Events are named by a hybrid-logical-clock timestamp (`ts`), and
the timestamp is the event's identity. The fold (`journal/fold.mjs`, 1,063 lines) is a pure
function of the whole event set: de-duplicate by `ts`, sort by `ts`, keep live heads per key.
A fork of verse text goes to a person. Every other fork keeps the newest head without asking.
Structural edits (spans, renumbers) form a chain per book. The journal is never compacted. Every
open and every save folds the whole set again. There is no incremental fold, and no snapshot or
cache is saved to disk. `journalingStore.ts` keeps the last fold in memory (`foldCache`,
`lastFold`) and drops it at every save. *(RT-4)* The app reaches the disk only through the
pankosmia-web HTTP server.

**The rebuild (STORAGE.md on branch `storage`).** Each change sets one whole record (a verse, an
alignment, a check decision, a comment, a pin group, a setting). Its id is the SHA-256 of its
canonical bytes, and it names as parents the versions the author's screen showed. A save is one
atomic file in `.tc/pending/`, outside the burrito. A seal at each checkpoint moves pending
changes into immutable packs under `ingredients/tc/changes/`, and old months are consolidated.
The fold computes heads from the parent graph, merges concurrent heads by fixed merge units, and
lists every remaining difference as a conflict. No clock, device or author decides what
survives. Built so far: `tc-journal` (model, codecs, fold, conflicts, projections) and
`tc-corpus` (the whole-Bible test corpus). Not built: `tc-store`, `tc-sync`, `tc-import`.

---

## 2. What was measured

All rows below were measured on one shared VM: an Intel Xeon at 2.8 GHz with 4 vCPUs, no SHA
instructions, Node 22.22.0, the rebuild in release at ea8c5f7. Runs were interleaved (tC4,
rebuild, tC4, rebuild). Load ranged from 2.6 to 17.8, so wall times vary widely; CPU times are
the firmer evidence. Files were in the page cache unless the row says "cold".

| Measure | tC4 at d310bf3 | Rebuild | Note |
|---|---|---|---|
| Fold only, about 300k, no history | 13.9 s (278,041 events; median of 30) | 1.42 s (310,422 changes; n=13) | tC4's fold also projects every book in memory; the rebuild projects on demand |
| Fold only, about 600k, with history | 25.1 s (576,195 events) | 2.89 s (617,143 changes) | |
| Fold plus projection of every derived file, about 300k | 13.9 s wall, 14.0 s CPU: the fold alone, which projects every book into memory but renders no derived file *(RT-5)* | about 11.4 s wall, 16.2 s CPU: fold (one thread) plus rendering 267 files, 235.6 MB (4 threads), from two processes *(RT-5)* | the fold gap comes mostly from projecting on demand. Not the same stage: tC4's side leaves out `checkpoint.mjs` rendering. Even so, the rebuild used more CPU *(RT-5)* |
| Open from local files, one thread, about 300k | 37.7 s (135 µs per event) | 17.6 s (57 µs per change) | tC4: its reference reader plus fold, which includes projecting every book; rebuild: read, verify, parse, fold, with no projection *(RT-6)* |
| Same, about 600k with history | 73.3 s (127 µs per event) | 31.6 s (51 µs per change) | |
| Rebuild open on 4 threads, as designed | not applicable | 12.7 s here under load; 7.8 s on a quiet VM (J2) | |
| tC4's app intake path (#423 zip read), replicated in Node | 133.7 s, 1.74 GiB peak | not applicable | leaves out the HTTP transfer and the derived-file checks |
| Cold disk, one sample each | 209 s (278,041 files; 186 s of it the scan) | 35.5 s (277 files, 4 threads) | 5.9 times here (6.7 for the history corpora). tC4 read on one thread with synchronous calls and the rebuild on four, so the share due to the file count is not isolated *(RT-7)* |
| Peak memory during the open | 837 MiB (278k), 1,034 MiB (576k) | 172 to 371 MiB | |
| Where the read time goes, one thread, about 300k | per-file list, lstat and read 9.8 s; schema checks 10.1 s; one SHA-256 pass 2.8 s | two SHA-256 passes 7.6 s; parse and checks 6.2 s | tC4 pays per file; the rebuild pays for hashing twice with a slower SHA-256 |

Two probes I ran for this memo (17:07 to 17:12 UTC, load 3.3 to 4.4; `results/memo-probe.txt`):

| Probe | Result |
|---|---|
| SHA-256 in Rust, one thread, 426 MB in memory | `sha2` 0.11: 158 to 180 MB/s. `ring` 0.17: 245 to 343 MB/s. `ring` is 1.4 to 1.9 times faster on this CPU, round by round: 1.4 to 1.8 per 1,374 B line (median 1.6), 1.7 to 1.9 per 1 MiB chunk *(RT-8)*. |
| List and read 617,143 files of 934 B in one folder, compiled Rust | 0.45 to 0.53 s to list. 4.9 to 6.8 s to read on one thread, 2.3 to 2.6 s on 4 threads. |
| List and read 278,041 such files | 0.2 s to list. 1.9 to 2.9 s on one thread, 1.2 to 1.3 s on 4 threads. |
| Read 277 files holding 426 MB | 0.05 to 0.10 s |

The J2 fixer's uncommitted work in the storage worktree (STORAGE.md §14 working copy, 14:27
UTC) reports, with the new fold cache: cold open 2.6 s median, 2.9 s p95, at load near 9.
Without a cache (the first open), 10 to 14 s on the loaded VM. I did not re-run it, and it has
not been reviewed. *(RT-3)* "Cold" here means a new fold, not a cold disk. One process opens
the corpus again and again, so the files are in the page cache. Only load and fold (§6.1 O8)
are timed. O9 classifies every derived path against the md5 of its projection (§6.2), and
that step is not in the time. As written, O9 needs the projection of every derived path, and
projecting every derived file of the 278k corpus took 9.96 s wall and 14.97 s of CPU here
(median of 5, `results/all-combined.md`, rb-tc4shape-project). *(A-2)* The cache does not
spare the read: the open still reads and fingerprints every pack byte (664 MB). The cache is
read only by the app build that wrote it (`IndexError::Build`), so every app update pays a
full first open. *(A-5)*

**What tC4 itself measured.** The fold alone fell from 9.3 s to 4.2 to 5.6 s on an Apple M2 Pro
after #92. A reference reader plus fold took 41 s for 278,041 files. The app's real open was
measured only on Titus: 4,002 segments in 4.6 to 5.1 s, and 40,002 segments in 81 to 90 s before
the zip read. The zip read (#423) took the 4,002-segment Windows open from over 120 s to 3.2 s.
No one measured a whole-Bible open in the app. On 2026-08-25 you ruled that a slow open must
show progress and need not meet a speed target.

---

## 3. The options

**A. Adopt tC4's method as the rebuild's storage format.** Port to Rust: sealed one-action
segments, the HLC, the 15 event ops, `fold.mjs` with its rules, the checkpoint projections, and
the round-1 sync rules G1 to G8. Adopt the conformance suite (JC-1 to JC-35) as the test of
record. Keep the format byte-compatible, so tC4 4.0 and rebuild users can share a project.
(Adopting the format but changing its rules would lose that compatibility, which is A's only
gain. That variant is a slower version of B.)

**B. Keep STORAGE.md and adopt tC4's speed techniques.** Keep J1 and J2. Finish the fold cache.
Add the tC4 lessons the design lacks. Use tC4's fixtures and conformance data as oracles for the
tC4 import. Move tC4 4.0 teams one way, as §13.3 specifies.

**C. Hybrid: STORAGE.md inside the app, tC4 4.0's format on Door43.** The parts:

| Part | From |
|---|---|
| Change model, ids and parents, fold, merge units, conflicts, review screens | STORAGE.md (J1, J2) |
| Local storage: pending files, packs, fold cache, atomic writes | STORAGE.md |
| What is committed to a team's Door43 repository: tC4 segments under each actor's folder, tC4's derived files, no packs | tC4 4.0 |
| Reading teammates' work: tC4 segments converted to changes on every sync | STORAGE.md §13.1 converter (T6, T7), run on new segments only |
| Sending own work: each change with a tC4 op written as a tC4 segment, with `base` set to the version the screen showed | new code, following tC4's §8 |
| Derived files on Door43 | tC4's projection (`fold.mjs` and `checkpoint.mjs` rules), so tC4 4.0 syncs do not rewrite them each time |
| Sync rules for a mixed team | tC4's G1 to G7 |

C is the only hybrid that lets the two apps share a project. I also looked at two other hybrids
and set them aside:

- *tC4's one file per action with STORAGE.md's fold.* It keeps the per-file cost (2.8 to 3.1 s
  on 4 threads to list and read 617k files) and gains no compatibility, because tC4 cannot read
  STORAGE.md's changes.
- *tC4's 15 ops as change bodies inside STORAGE.md's packs.* It gains no compatibility either,
  and gives up the per-record model that closes the span and renumber losses (L1 to L6).

---

## 4. Speed at whole-Bible scale

"Measured" means measured on this VM. "Estimate" is my arithmetic from measured parts.

| | A as tC4 runs it (JavaScript) | A ported to Rust | B | C |
|---|---|---|---|---|
| Cold open | Never measured in the app. tC4 estimates 3 to 6 min. Reference reader: 73.3 s at 576k events (measured); app intake path 133.7 s at 278k (measured replica) | Estimate 6 to 10 s without a cache: list and read 617k files 2.8 to 3.1 s (measured probe), one SHA pass about 0.6 s, parse and schema checks about 2 s, fold 2 to 3 s, plus reading and comparing every derived file. With a per-file cache like the rebuild's, still about 5 s, because every file must be read again to check it. A cache that trusts each file's size and time instead would take about 3.5 to 4.5 s: listing and reading the size and time of 617k files took 1.4 to 2.3 s here (red-team probe), and the fold comes on top. It would also miss damage that keeps the size *(RT-11)* | 2.6 s median with the fold cache (fixer's report, uncommitted; warm page cache, load and fold only *(RT-3)*). First open without a cache: 7.8 s on a quiet VM at 225d19e, before the fold was streamed; 10 to 14 s loaded now (measured); about 6.4 s quiet at HEAD (11.6 s loaded divided by the evidence doc's load factor of 1.8). About 5 s quiet with hash-once and `ring` (estimate) *(A-4)*. None of these include O9, which as written projects every derived file (about 10 s wall here) *(A-2)* | As B for the app's own data. The working tree also holds the team's tC4 segments, about 620k files |
| Save one verse | A full fold in the worker on every save: 25 s at 576k (measured fold time). Then each derived file the action touches is written with `update_ingredients`, and each such write makes the platform rescan every file (9 to 12 s at 278k, proxy; no measurement of it is in the results folder). Only those files are rewritten, not the whole derived set *(RT-10)* | One segment written atomically (about 0.4 ms). But a new segment is a new ingredient: keeping the folder a valid burrito means rewriting `metadata.json`, about 74 MB at 278k files and about 164 MB at 617k (estimate from tC4's ingredient entry format) *(RT-12)*, on every save. tC4 has no incremental fold, so one must be designed | One atomic write, 0.41 ms (probe); fold step 0.05 ms, 0.26 ms with a live snapshot (measured). `tc-store` is not built, so the full path is not measured | As B, plus writing tC4 segments at a checkpoint |
| Daily sync | Never measured at scale. Each try clones Door43 main in full, folds three times (75 s of folding at 576k, estimate) and re-reads every segment | Depends on the transport. With git, each commit that adds a segment writes a new tree object for that actor's folder, listing every segment in it: about 200,000 entries and 17 MB before compression at three years (estimate). A join checks out about 620k files | 1 to 3 s plus network (§14 estimate; unit X not built) | tC4's rules for the team, and git holds about 620k files, as in A |
| Memory | 1.0 GiB at 576k (measured) | not known | 256 to 374 MiB after the open (measured); the steady budget of 150 MiB is still missed | as B, plus the converter |

**Why A cannot win on speed.** tC4's speed work was real: #92 cut the fold by 59%, #94 moved it
off the screen thread, #95 and #423 cut the open to one scan and one request. But none of that
removes the work that grows with the whole project. tC4 folds everything on every open and every
save, checks every event's schema twice, and stores one file per action. At three years that is
about 620,000 files. The file count alone uses the 3 s budget if each file is read (2.8 to
3.1 s). If only each file's size and time are read, it uses half to three quarters of the
budget (1.4 to 2.3 s, red-team probe). *(RT-11)*

---

## 5. Invariants (STORAGE.md §1)

| Invariant | A | B | C |
|---|---|---|---|
| I1 Convergence | Holds for the fold (R-8.6.1). Fails for decision files, which depend on installed resources (L27) | Holds; property test `fold_converges_under_any_union` | Holds in the app. Derived files on Door43 follow tC4's rule |
| I2 Conservation, every state visible | Fails: retained and pending states and losing non-text heads are on no screen in tC4's app (L8, L24). The fold reports them, so a port's screens could show them *(RT-14)* | Holds; property test `conservation` | Holds for rebuild users. tC4 4.0 users see tC4's screens |
| I3 No survival by time or identity | Fails: newest `ts` wins non-text forks; the same-actor rule reads the actor; clocks ratchet (L13, L16) | Holds; property test `no_survival_by_time_or_identity` | Holds for rebuild users |
| I4 Observed parents | Fails: `base` is the current head at write time (L15). A port could set `base` to the shown head | Holds; `tokens.rs` and property test `stale_tokens_never_supersede_unseen` | Holds; segments written with `base` = shown head |
| I5 Total fold | Fails: several defects make the whole fold throw (L12); an unknown `v` refuses it (L25) | Holds; property test `fold_never_panics` | Holds in the app. tC4 4.0 users still refuse |
| I6 Atomic files | Can hold: the rebuild owns its writes (tC4's platform does not fsync) | Holds by design; `atomic_write` not built | As B |
| I7 Valid burrito between saves | Holds only with an O(project) `metadata.json` rewrite per save | Holds by design: saves stay outside `ingredients/` | As B locally |
| I8 Nothing overwritten | Partly: outside USFM edits are reconciled; other derived files stop the open; Door43 edits are replaced (L14) | Holds by design (S2, X not built) | Holds in the app, if it ignores tC4's own rewrites (§13.3 rule) |
| I9 No editing lock | Depends on the sync. tC4's sync holds the project (L23) | Holds by design (X not built) | As B for rebuild users |
| I10 Newer data refused in words or carried | Fails (L25) | Holds; `loss_newer_minor_carried` | Holds in the app; the app cannot send new kinds to tC4 4.0 users |
| I11 Deletions are deliberate | Not specified the same way | Holds by design | As B |
| I12 No machine input | Fails (L27) | Holds; `projection_ignores_installed_resources` | Fails for decision files on Door43 if they must match tC4's |

---

## 6. Silent-loss cases (STORAGE.md §15), case by case

"Reopens" means the loss comes back. For B, "test" names the test in `tc-journal` at storage HEAD
6e8adac; "waits" means the case is closed by design but its test belongs to a unit not built
yet. For C, the first part is for rebuild users, the second for tC4 4.0 teammates.

| # | Case | tC4 at d310bf3 | A | B | C |
|---|---|---|---|---|---|
| L1 | Two concurrent spans in one book | still true (E1 rerun) | reopens | closed; `loss_e1_two_spans_one_book`, `loss_e1_reversed_order` | closed / still lost. A rebuild span becomes `text.structure.apply`, which tC4 may pend |
| L2 | Spans that share a verse | still true | reopens | closed; `loss_e1_overlapping_spans_conflict` | closed / still lost |
| L3 | Span against an older-ts edit | still true | reopens | closed; `loss_e2a_span_vs_edit_elsewhere` | closed / still lost |
| L4 | Phantom conflict | still true | reopens | closed; `loss_e2b_no_phantom_conflict` | closed / still true |
| L5 | Edit of a verse merged away | still true | reopens | closed; `loss_e3_edit_vs_merge_away` | closed / still lost |
| L6 | Renumber against an edit | still true in code | reopens | closed; `loss_renumber_vs_edit` | closed / still lost |
| L7 | One verse edited two ways | shown for review | holds | holds; `loss_same_verse_two_ways` | holds / holds |
| L8 | Newest wins for alignments and checks | still true, normative (R-8.7.12, JC-35 G7) | the suite requires the newest head as the projected value. It does not require hiding the fork: the fold lists it in `forks[]` (R-8.6.4), and JC-35 G7 asserts that it is listed. A port can show it, as C does. tC4 hides it by its app rule (D98 point 8) *(RT-13)* | closed; `loss_e4_alignment_conflict_visible`, `loss_check_two_ways_visible` | closed; a rebuild user can resolve with `supersedes` / newest wins until then |
| L9 | Pins mixed by two gateway changes | still true | reopens | closed; `loss_two_gateway_changes_never_mix` | closed / still mixed |
| L10 | Two `book.add` of one book | still true | reopens | closed; `loss_two_book_adds` | closed / still hidden |
| L11 | Two people resolve differently | still true | reopens | closed; `resolve_differently_reconflicts` | closed / unlabelled |
| L12 | Poison segment stops every open and sync | still true | reopens | closed; `loss_e5_poison_never_blocks` | closed / tC4 users blocked |
| L13 | Fast clock wins and drags clocks | still true (E6) | reopens | closed; `loss_e6_fast_clock_cannot_hide` | closed / still true |
| L14 | Door43 web edits overwritten | still true, normative (R-8.7.9) | reopens with tC4's sync rules | closed by design; waits (X) | closed / tC4 users overwrite |
| L15 | Stale whole-book save reverts work | mechanism present | can be closed inside the format by setting `base` to the shown head | closed; `loss_f5_stale_buffer_never_reverts` | closed / still present in tC4 4.0 |
| L16 | One identity on two machines | still true | reopens | closed; `loss_f7_one_identity_two_machines` | closed / still true |
| L17 | Lost secret orphans the outbox | still true | closes if staged saves are files, not browser storage | closed by design; waits (S1) | closed |
| L18 | Non-atomic writes, torn `metadata.json` | still true | closes with `atomic_write`; `metadata.json` stays huge | closed by design; waits (S1, S2) | closed |
| L19 | Damaged scratch copy locks the project | mostly fixed (D101) | depends on the sync design | closed by design; waits (X) | closed / tC4's residual stays |
| L20 | False "synced" | still true | sync design | closed by design; waits (X) | closed |
| L21 | One extra file on Door43 blocks every sync | still true | reopens for mixed teams | closed by design; waits (X) | the app must never commit a file tC4 does not derive |
| L22 | One bad incoming file refuses the whole sync | still true, normative (R-8.7.8, JC-35 G2) | reopens; required by the suite | closed by design; waits (X) | closed / tC4 users blocked |
| L23 | Project locked during a sync | still true | sync design | closed by design; waits (X) | closed / still true |
| L24 | Retained and pending states on no screen | still true | reopens unless the app adds screens for them | closed; property `conservation` | closed / still true |
| L25 | Version skew reads as "damaged" | still true | reopens | closed; `loss_newer_minor_carried` (refusing a newer major waits, S1) | the app cannot send any change tC4 4.0 lacks an op for |
| L26 | Full clone per sync retry | still true | sync design | closed by design; waits (X) | tC4 users still clone |
| L27 | Decision files depend on installed resources | still true | reopens (R-8.7.9 input b) | closed; `projection_ignores_installed_resources` | files flip between apps, or the app copies tC4's rule |
| L28 | Username into public history | partly changed | app policy | closed by design; waits (X) | tC4 users still seal it at first sync |
| L29 | Audio never reaches Door43 | still true | sync design | closed by design; waits (X) | tC4 users never send or receive it |
| L30 | Branch other than `main` | still true | sync design | closed by design; waits (X) | tC4 users refused |
| L31 | Private repositories | still true | sync design | closed by design; waits (X) | tC4 users refused |
| L32 | OBS refused; no USB | still true | sync design | closed by design; waits (X) | OBS teams cannot mix |
| L33 | In-process locks only | partly changed | closes with an OS lock | closed by design; waits (S1) | closed |
| L34 | 403 or protected branch | still true | sync design | closed by design; waits (X) | closed / still heuristic |

**Totals.** Under A, up to 18 cases reopen: 16 through the format and its fold (L1 to L6, L8 to
L13, L16, L24, L25, L27) and 2 through tC4's sync rules (L14, L22). Only L22 is required by
tC4's own suite. For L8 the suite requires the newest head as the projected value, not a
hidden fork. L8 and L24 reopen only if the port also keeps tC4's screens, because the fold
reports the losing heads (`forks`, `retained`, `pendingStructural`). A port that shows them
reopens 16. *(RT-14)* L21 also reopens for mixed teams. Under B, 18 of the 34 have
fold-level tests today, and the other 16 are closed by design with tests that wait for S1, S2
and X. Under C, rebuild users get B's protection, but tC4 4.0 teammates keep
tC4's losses, and C adds constraints (L21, L25, L27).

**Cases L35 to L102** were found against STORAGE.md's own design. Under A most do not apply as
written, because the mechanisms differ. Two reopen directly: book generations (L51; tC4 keeps
a restored book's old checks, alignments and notes as `prior-generation`), and work tC4 4.0
already hid before an import (L60). Under A, L60 cannot be fixed at all, because the hidden work
stays hidden by the same rules. Under B, the fold-level cases among L35 to L102 have tests in
`loss.rs` (67 tests in that file *(RT-15)*), and the store, materialize and sync cases (for example L40,
L47, L48, L64, L65, L80) wait for S1, S2 and X.

---

## 7. Compatibility with tC4 4.0 projects and teams

**Where tC4 4.0 is today.** tC4 4.0 is not released. The last tag is v4.0.0-rc.1 (2026-10-02),
and it does not contain team sync. Team sync landed on 2026-10-13 to 2026-10-15, and D102
(2026-10-15) put it into v4.0.0. v4.0.0 is a pilot release for selected external users. So no
released tC4 build syncs a team through Door43 today; only test builds from tC4's `main` can.
Some pilot teams will sync by the time the rebuild ships.

| | A | B | C |
|---|---|---|---|
| Open a tC4 4.0 project | Directly. Staged saves in tC4's browser outbox are not visible to the rebuild (true for every option); opening the project once in tC4 first writes them, because tC4 replays its outbox at every open | Import (§13.1): convert each event, check the result against tC4's own derived files, surface hidden work as conflicts. Budget: 2 min for a whole Bible. Unit I, not built | As B |
| Move a Door43 team | No move needed | One way (§13.3). After the move, a tC4 4.x client's sync stops before its push. *(RT-9)* The last check before the push is the verify, and it fails with `sync.verify-failed` because it lists `ingredients/tc/**` as unexpected files (`verify.ts` `readDerivedDisk` and `classifyDivergence`; only `audio/` and OBS base files are skipped). An earlier check in the sync core can refuse first. The person reads "The app found a problem in the combined project, so it did not send it to Door43. Nothing changed. Ask for help." (`refusal.sync.verify-failed`). So 4.x cannot overwrite the moved project. Its work stays local until the person updates and imports | No move needed |
| Rebuild and tC4 4.0 users on one Door43 project | Yes, if the rebuild writes only tC4's 15 ops, adds no file under `ingredients/` that tC4 does not derive, and projects derived files byte for byte as tC4 does. Each rebuild feature beyond tC4 4.0 then needs a tC4 release first, because an unknown op stops tC4's fold (R-8.3.9). After 4.0.0, D47 lets the format change only with a `schemaVersion` bump, a written migration and fixtures, except for additive optional fields *(RT-16)* | No. Safe, because 4.x refuses rather than overwrites. No way back today; plan step 8 adds one without history *(A-7)* | Yes, with C's limits: no packs on Door43, no change kinds tC4 lacks, tC4's derived bytes |

The §13.3 bridge already takes tC4 segments that a 4.x straggler pushes after a move, and converts
them. So B loses no straggler's work. It only stops stragglers from syncing until they update.

---

## 8. Scripture Burrito validity

- **A.** Valid, but every segment is an ingredient. `metadata.json` lists every one: about 74 MB
  at 278,041 files and about 164 MB at 617k, at about 265 B per entry in tC4's format
  (estimate). *(RT-12)* A save adds an
  ingredient, so the folder is invalid until `metadata.json` is rewritten. Segment names are
  valid ingredient paths (tC4 maps `|` to `,`).
- **B.** Valid at every commit, exchange file and export (I7). The working folder is valid
  between saves, because saves stay in `.tc/`. It is invalid only inside a write group
  (milliseconds for a materialization, seconds for a large intake). About 600 ingredients and a
  `metadata.json` of about 120 KB (estimate).
- **C.** The Door43 tree is tC4's layout, valid but with A's large `metadata.json`. Local folders
  are as B.

---

## 9. Spec and test maturity

**tC4.** BURRITO-SPEC §8 is normative, with 105 rule ids and 70 refusal codes. The journal
conformance suite passes 368 of 368 checks at d310bf3 (34 groups, JC-1 to JC-35, fast-check seed
20260707, 200 runs per property). The phase-1 suite passes 52 of 52, and 301 journal and sync
vitest tests pass. tC4's own open residuals (D55): no mutation hardening per statement, and no
generated joins of three or more devices. Segments have been written since 2026-08-19. Team
sync is three days old and is in no release yet. The whole-Bible scale was tested for the fold
only.

**The rebuild.** STORAGE.md is normative (3,933 lines). `tc-journal` and `tc-corpus` hold about
36,000 lines (18,053 of library code, 13,609 of tests, 4,300 of corpus). The workspace passed
661 tests at f37c0b7 (the builder's report), including 16 property tests (`properties.rs`), 67
tests in `loss.rs` *(RT-15)*, and operation-count tests on Psalms and ten times Psalms. A fresh reviewer
attacked J2 and found 12 problems, including a blocker. Eleven are fixed, and the four rules a
test could not catch now fail tests when removed. The twelfth, the missed budgets, is partly
addressed: the cold open by the uncommitted fold cache; the Psalms projection and the steady
memory are still missed.
Nothing below the fold exists yet: no store, no crash injection, no sync, no import.

**What of tC4's suite B can use.** Its fixtures and data checks are good oracles for the tC4
import and the tC4-shaped sidecars: JC-2 skeleton byte identity, JC-8 reconcile, JC-15 seed
migration, JC-26 pin golden, JC-31 value grammars, JC-33 OBS, the team-sync fixture of JC-35,
and the E1 to E6, L2, L9 to L11 and L16 experiment scripts. Its checks of tC4's fold rules
cannot be adopted, because they require the losses: JC-4 last writer wins, JC-21 and JC-22
structural chains, JC-28 the same-actor rule, JC-29 generations, and JC-35 G2 and G7.

---

## 10. The cost of changing now

| | A | B | C |
|---|---|---|---|
| J1 and J2 | Most is thrown away: about 31,700 lines of `tc-journal` (model, canonical bytes, packs, fold, merge units, conflicts, choices, tokens, the fold cache). Kept: the corpus generator (needs a tC4 writer), parts of the USFM codec, `tc-burrito`. In calendar time this is about a day and a half of work; A's real cost is P3 and the speed work tC4 lacks, not these lines *(A-6)* | All kept | All kept |
| New work | Port about 3,750 lines of `journal/*.mjs` and the app logic of `journalingStore.ts` (3,507 lines, built around the HTTP platform), and about 5,100 lines of conformance checks. *(RT-17)* `journalingStore.ts` is 3,507 of the 6,276 lines in `src/data/journal/`. `journalStore.ts` (824), `opsLog.ts` (717), `verify.ts` (316) and `seal.ts` (159) also hold rules that a byte-compatible port must match, and so does the sync shell `src/data/share/syncOperation.ts` (787). Then design what tC4 lacks: an incremental fold, a cache, a valid burrito between saves. Rewrite STORAGE.md | Commit and review the fold cache. Hash once and a faster SHA-256 (small codec changes in J1). Then the planned units | B's work, plus less than first stated *(A-6)*. Unit I already ports tC4's validator, replays its per-key parents, converts events and checks against tC4's derived files (§13.1 T2, T6, T7), and §13.3's bridge already converts segments that arrive after a move. C adds: a tC4 writer with an HLC; a mapping of every change kind to a tC4 op; sidecars byte-identical to tC4's (I compares fields, not bytes); the bridge's converter made incremental; G1 to G7. It also needs a rule that a rebuild user's own change, sent as a segment and received back, keeps its id, for example by storing it from the start in the form T6 gives it. Otherwise each synced change comes back as a second copy (my inference from T6; not in STORAGE.md). Smaller than one unit the size of X or I |
| Units not built (C, S1, S2, P, X, I) | All re-planned against a new spec. I gets smaller for tC4 4.0 projects; tC3, USFM and burrito import remain | As planned, with the speed items added to S1 and P | X and I grow |
| PLAN.md | P3 cannot be met. It must be rewritten to allow tC4's structural rules (L1 to L6), the fold's refusals (L12, L25) and the sync rules (L14, L22). Newest-wins for alignments and checks is not the obstacle: the fold lists those forks, and a port can show them *(RT-18)* | Unchanged | Unchanged for rebuild users |

J1 and J2 ran from 2026-10-15 to 2026-10-16 in the storage chain, with a review and a fix round.

---

## 11. Risks

**A.**
- It brings back up to 18 known loss cases (16 if the port shows the fold's forks and retained
  heads *(RT-14)*), and PLAN.md's P3 fails.
- Cost grows with the number of files: open, git trees, `metadata.json`, cold disks, and
  antivirus on Windows. At three years a project holds about 620,000 files.
- It starts with no incremental fold and no cache, which are the hard parts of the speed work.
- The rebuild is tied to tC4 4.0's frozen format. Every improvement needs a tC4 release in step.
- One gateway change over a checked book is one action, and one action has a 4 MiB limit. A
  Psalms-sized change (about 15k decisions of 1.2 KB) would be refused. This is inferred from
  the code, not tested.
- tC4's whole-Bible behavior was never measured end to end.

**B.**
- No full open (O1 to O13) has been measured. The 2.6 s covers O8 with a warm page cache.
  O9 as written projects every derived file (about 10 s here) until plan step 2 changes it.
  A cold disk reads 664 MB more: about 1.3 s on a SATA SSD (red-team estimate). *(A-2)*
- The 2.6 s rests on a fold cache that is not committed or reviewed. The cache trusts a
  64-bit fingerprint of each unchanged file. A reviewer must attack its key.
- The first open without a cache is slower: a received project, or, until plan step 1 changes
  the key, the first open after every app update. About 6.4 s quiet at HEAD and 10 to 14 s
  loaded; about 5 s quiet with hash-once and `ring` (estimate). *(A-4, A-5)*
- Save, checkpoint, sync and import are specified but not built. Their numbers are estimates.
  16 of B's 34 closures in §6 wait for those units. *(A-3)*
- Mixed teams cannot share a project. After a team moves, a tC4 4.x user's sync stops with a
  message that reads like damage ("… Ask for help."). Nothing is overwritten.
- A moved team has no way back to tC4 today: the import removes `checking/journal/**` and
  nothing exports to tC4's format (plan step 8 adds an export without history). *(A-7)*
- A blocked tC4 4.x user might share the project again from tC4, which makes a second Door43
  repository and splits the team. No code or report shows this; it is a risk only.
- More conflicts reach people than in tC4 (STORAGE.md §18 item 5).
- The Psalms projection (358 ms against 300 ms) and the steady memory budget (about 205 MB
  against 150 MiB) are still missed.

**C.**
- Two sets of rules run on one project. Rebuild users see conflicts that tC4 users do not.
- tC4 4.0 users still lose work by tC4's rules, and some losses come from rebuild users' own
  edits (a rebuild span written as `text.structure.apply` can pend in tC4's fold).
- Door43 holds the per-file layout, so A's git and `metadata.json` costs apply to sync.
- Every new rebuild feature needs a tC4 op first, or it cannot travel.

---

## 12. Recommendation and plan

**Recommendation, revised after the red team.** *(A-1)*

1. **Not A.** Do not adopt tC4's journal format, fold rules or conformance suite as the test of
   record. A reopens up to 18 loss cases (16 if a port shows the fold's forks) *(RT-14)*, and
   P3 fails for its structural rules, refusals, clocks and sync rules *(RT-18)*. A is slower at
   every measured open, mostly because of its file count and the lack of a lasting fold.
2. **Keep STORAGE.md, J1 and J2, and go on with units C, S1, S2 and P.** These units are the
   same under options B and C. Add the tC4 lessons below and make O9 incremental.
3. **B or C: decide before X and I start, from your answer to question 1.** B if a one-way
   move is acceptable, with a way back without history (plan step 8). C if rebuild and tC4 4.0
   users must share one project during the pilot. C meets P3 only for rebuild users, and tC4
   4.0 users still lose work by tC4's rules.
4. **Do not call the 3 s budget met** until a full open (O1 to O13) is measured with a cold
   page cache, without a cache, and on the reference laptop. As written, B misses it because
   O9 projects every file. With O9 made incremental, no measured part rules 3 s out, but no
   measurement shows it met.

**tC4's speed techniques, and what each one gives B.**

| tC4 technique | In B today | What it would save |
|---|---|---|
| Read the journal in bulk, not one file or request per action (#423) | Yes: about 300 pack and pending files | Already saved: 2.8 to 3.1 s of file handling at 617k files (probe). On a cold disk the gap was 5.9 to 6.7 times, with one reading thread against four *(RT-7)* |
| One scan per open (#95) | Yes: O8 reads each file once | Nothing more. Add the operation-count test "open reads each pack once" in S1 |
| Hash each byte once (tC4 hashes each segment body once) | No: P2 hashes each whole file, then P4 hashes each line | About 5 s of single-thread CPU of 16.8 s, about 1 s on 4 threads, on a first open without SHA instructions (estimate). Only files missing from the cache pay it |
| A fast SHA-256 (tC4 gets OpenSSL through Node) | `sha2` 0.11 in software on CPUs without SHA instructions | `ring` is 1.4 to 1.9 times faster here, median 1.6 per line (probe) *(RT-8)*: about 0.4 s on 4 threads on the one pass left after hash-once (estimate) *(A-4)*. No gain on CPUs with SHA instructions, where `sha2` already uses them |
| Check each event once (#80 found 1.8 s of re-checking; tC4 still pays it) | Yes: V1 to V8 run once, at parse | Nothing more. The fold cache must keep it so |
| Precompute sort keys; one set per book (#92, −59%) | Done in J2 (compact tables) | Nothing more |
| Fold off the screen thread (#94) | Required by PERFORMANCE.md rule 3 | Responsiveness, not time. Build it in S1 and the app |
| Progress after 300 ms (#95, J15) | Rule 4 asks for progress; no threshold | Adopt the 300 ms threshold and a J15-style test |

The fold cache is the rebuild's own step, and it is what gets load and fold to 2.6 s. tC4 never
built a fold cache that lasts from one open to the next. It keeps only the last fold in memory
until the next save *(RT-4)*.

**Plan.** Steps marked *(A-n)* are new or changed.

1. **Finish J2's budget work.** Commit the fold cache with its tests. Run a fresh review that
   attacks the cache key, the fingerprint and the stale-cache cases. Key the cache on its
   encoding version and the format minor, not the app build, so an app update keeps it; the
   test `the_encoding_is_pinned` already forces a new version when the encoding changes.
   *(A-5)*
2. **Make O9 incremental, in STORAGE.md before S1.** A derived path is converged without being
   read or projected when its size and mtime equal the manifest entry, the fold's heads for
   every record in its scope equal its heads file (by the token rule of §2.7), and the
   projection version is unchanged. Only other paths are read and projected. The storage-damage
   check (size and mtime equal, bytes differ) moves to a background pass after the project is
   usable. Operation-count test: an open after no change projects no file. *(A-2)*
3. **Hash once.** In J1's codec, compute a pack's name and a pending file's name from the
   header line and the line ids in order, instead of from the SHA-256 of the whole file. Each
   line still verifies against its own id, so the name still covers every byte, and a first
   read hashes each byte once. Measure before and after. Keep it only if it wins. It changes
   the format, so it must land before any project is written.
4. **Faster SHA-256.** Hash bulk data with `ring` 0.17.14, which is already in `Cargo.lock`
   (through `rustls-webpki` and `quinn-proto`). Measure `ring` against `sha2` on a CPU with SHA
   instructions too, and keep the faster one there.
5. **Measure the full open (O1 to O13)** once S1 can run it, on the whole-Bible corpus: with
   the cache and a warm page cache, with the cache and a cold page cache, without a cache, and
   after an app update. Median and p95, load noted. Repeat on the reference laptop (4 cores,
   SSD, 8 GB) before the format freezes (STORAGE.md §16.5). Until then, report the 3 s budget
   as not shown. *(A-2)*
6. **Open with progress after 300 ms**, off the screen thread, with a test, in S1.
7. **Use tC4's fixtures in unit I.** Run the tC4 import against the JC-35 team-sync fixture,
   the JC-2 skeleton identity corpus, the JC-15 seed migration data, the JC-26 pin golden files
   and the JC-31 value grammars. Replay the E1 to E6, L2, L9 to L11 and L16 experiments through
   the import, and check that the hidden work comes back as conflicts (L60).
8. **A way back, under B.** Export a moved project as a tC4 4.0 project without a journal: the
   USFM, the tC4-shaped sidecars and `metadata.json`, and no `ingredients/tc/`. tC4 4.x seeds
   a journal from such files the first time it opens them (BURRITO-SPEC R-8.8.2 and R-8.8.3;
   `seedProject` and `seedCandidate` in `journalingStore.ts`, lines 1583 and 1670). The
   sidecars must hold no field tC4 does not know, or tC4 refuses the seed (R-8.8.2). The export
   carries the current text, checks and alignments. History and open conflicts stay in the rebuild's copy, and the team shares the
   export from tC4 as a new Door43 project. Test it against tC4's seed check at d310bf3. Not
   built and not tried yet. *(A-7)*
9. **Decide B or C before units X and I** (question 1). Until then, resume the storage chain at
   unit C, then S1, S2 and P, with steps 2 to 6 in S1 and P. *(A-1)*
10. **Plan the team move with tC4 (optional).** Ask tC4 4.0.x to show "This project moved to the
    new translationCore" instead of `sync.verify-failed` when it sees `ingredients/tc/`. B is
    safe without it: 4.x refuses rather than overwrites.

---

## 13. Questions only the owner can answer

1. **Mixed teams.** Must rebuild users and tC4 4.0 users work on one Door43 project at the same
   time? If yes, for how long? Yes leads to C; A is not needed for it. No allows B with a
   one-way move (§13.3). The answer is needed before units X and I, not now. *(A-1)*
2. **Pilots.** How many tC4 4.0 teams will sync through Door43 before the rebuild ships? Team
   sync is not in any tC4 release yet, but D102 puts it into v4.0.0.
3. **The rule for check results and alignments.** tC4 ruled that the newest wins (D98 point 8).
   PLAN.md's P3 says every such conflict is shown to a person. Which rule stands for the
   rebuild? A requires tC4's rule for the projected value and for tC4 4.0's sync report. The
   rebuild's own screens could still show the fork *(RT-13)*.
4. **The 3 s budget.** For tC4 you ruled that a slow open must show progress and need not meet
   a speed target. Is 3 s firm for the rebuild? Does it cover the first open after a restart,
   when the disk cache is cold? Is a slower first open with progress acceptable for a received
   project (about 5 to 8 s on 4 cores without SHA instructions, estimate)? *(A-2, A-4)*
5. **The reference laptop.** Which machine is it, and does it have SHA instructions? Most recent
   Intel and AMD processors and all Apple silicon do. With them, the no-cache open is much
   cheaper.
6. **A tC4 change.** May we ask for a small tC4 4.0.x change so 4.x clients say the project
   moved, or must the rebuild work with tC4 4.0 exactly as it ships?
7. **A way back.** Under B, is a way back without history (plan step 8) enough for a pilot team
   that moved and wants to return to tC4, or must such a team keep its history? *(A-7)*

---

## Sources

- tC4 at d310bf3, `unfoldingWord/translationCore4`: `journal/fold.mjs`,
  `journal/schema.mjs`, `journal/checkpoint.mjs:205` (`isUnjournaledIngredient`: `audio/` only),
  `src/data/journal/verify.ts:135-149, 268-284`, `src/data/journal/seal.ts:31` (4 MiB),
  `docs/BURRITO-SPEC.md` R-8.1.10, R-8.1.15, R-8.2.5, R-8.3.9, R-8.6.4, R-8.7.8, R-8.7.9,
  R-8.7.12, `conformance/validate-journal.mjs:4857-5102` (JC-35, G2 and G7),
  `docs/DECISIONS.md` D47, D52, D98, D101, D102, `docs/ROADMAP.md`, `docs/evidence/*`.
  `git merge-base --is-ancestor 72318d4 v4.0.0-rc.1` is false: rc.1 has no team sync.
- The rebuild: `git show storage:docs/STORAGE.md` (§1, §13, §14, §15, §18);
  `docs/PLAN.md` (P3); `docs/PERFORMANCE.md`;
  the storage worktree at 6e8adac (`crates/tc-journal/tests/loss.rs`,
  68 tests; `crates/tc-journal/src/pack.rs` `read_pack`) and its uncommitted
  `crates/tc-journal/src/index.rs` and STORAGE.md working copy (§14 cold open row, 14:27 UTC);
  `.audit/rebuild.tsv`.
- Head-to-head benchmark:
  `docs/evidence/journal-decision-2026-10-16/results/all-combined.md`,
  `docs/evidence/journal-decision-2026-10-16/results/extras-summary.md`, `docs/evidence/journal-decision-2026-10-16/results/rounds2-summary.md`, `docs/evidence/journal-decision-2026-10-16/results/cold/`,
  `docs/evidence/journal-decision-2026-10-16/results/tc4full/`; scripts in `docs/evidence/journal-decision-2026-10-16/bench/`.
- Answers to the red team: storage worktree `docs/STORAGE.md` working copy §6.1 O8 and O9,
  §6.2, §13.1 T2 to T9, §13.3; `crates/tc-journal/src/index.rs` (`IndexError::Build`,
  `INDEX_VERSION`); `crates/tc-journal/tests/loss.rs` line 764;
  `docs/evidence/storage-fold-2026-10-16.md` (11,572 ms, load factor 1.8, one SHA-256 pass
  1,022 ms); tC4 `docs/BURRITO-SPEC.md` R-8.8.2 and R-8.8.3 and `journalingStore.ts` lines
  1583 and 1670 (seeding a project that has no journal).
- Probes for this memo:
  `docs/evidence/journal-decision-2026-10-16/results/memo-probe.txt`;
  source in
  `docs/evidence/journal-decision-2026-10-16/memo-probe/src/main.rs`.
- Arithmetic marked "estimate": `metadata.json` at 617k files = 56.7 MB × 617/278 (superseded:
  one tC4 ingredient entry for a segment, pretty-printed as in `conformance/sample-burrito`,
  with a 16-character actor id, is about 265 B, so 278,041 entries make about 74 MB and 617,143
  make about 164 MB *(RT-12)*); a tree of an
  actor folder = 617k / 3 actors × about 85 B per entry; hash-once saving = one of J2's two
  SHA-256 passes (about 5 s single-thread, 1.0 s on 4 threads quiet); `ring` saving = one pass
  divided by 1.6 to 1.9.

---

## Red team

The red team was asked to argue against recommendation B as strongly as the facts allow. It
checked each factual claim against tC4 at d310bf3, the storage worktree at 6e8adac (and its
uncommitted fold cache), the results folder and the benchmark scripts. It re-ran three things:

- tC4's journal conformance suite, in the scratch copy whose `journal/` and `conformance/`
  match d310bf3 byte for byte. Result: 368 passed, 0 failed.
- tC4's journal and sync vitest files.
- A probe that lists and stats 617,143 files.

Paths below are relative to
`docs/evidence/journal-decision-2026-10-16/`
unless they name a repository.

### The strongest argument against B

B asks the owner to accept a cost that is certain and near. The gain it offers in return is
not yet shown.

**The cost.** From the day a rebuild user joins a team, that team cannot mix tC4 4.0 and the
rebuild. tC4 v4.0.0 with team sync is due to tag as soon as its gates are green (D102). It goes
to pilot users, who will bring real project data soon after (`docs/ROADMAP.md`, "Who 4.0.0 is
for").

- The first rebuild user on a team has two choices: stop syncing, or move the whole team
  (§13.3).
- After the move, every tC4 4.x teammate's sync fails with a message that reads like damage:
  "The app found a problem in the combined project … Ask for help."
- A team that has moved has no way back to tC4. The import commit removes
  `checking/journal/**` (§13.1 T9), and no exporter to tC4's journal is planned.

**The gain, on speed.** The memo's head-to-head compares tC4's fold, which includes projection,
with the rebuild's fold alone. On the only stage that includes projection on both sides, the
rebuild used more CPU (16.2 s against 14.0 s). B's 3 s open comes from one measurement with
three limits:

- It covers one of the open's 13 steps (O8).
- The page cache was warm.
- It uses a cache that is uncommitted and unreviewed, and that every app update throws away.

**What is measured and real.** Two things:

- One file per action costs 1.4 to 3.1 s per open at 617k files on this VM.
- tC4's structural and clock rules lose work.

Option C keeps B's fold and protections inside the app and keeps tC4 4.0 teams working. Its
extra cost is smaller than the memo says, because unit I must port tC4's validator and replay
under B anyway (F5). The recommendation should therefore wait for the owner's answer to
question 1. If mixed teams are needed for the pilot period, the choice is C. If they are not,
the choice is B. B should not be presented as dominant until a full open (O1 to O13) is
measured with a cold page cache.

### Findings

**F1. B's 3 s open is not yet shown end to end.**
- *Only load and fold, with a warm page cache.* The 2.6 s comes from `foldbench`, which times
  load and fold (§6.1 O8) only. One process reopens the corpus again and again, so the files
  sit in the page cache. The storage worktree's working copy of STORAGE.md §14 says so: "395–403
  MiB when one process opens the project again and again".
- *O9 is not in the time, and its cost is unknown.* O9 classifies every derived and carried
  path. Its first test is `d == e`, where `e` is the md5 of the projection (§6.2). Rendering
  every derived file of the tc4shape corpus took 9.96 s wall and 14.97 s CPU here (median of 5,
  `results/all-combined.md`, rb-tc4shape-project). The heads files (§3.6) could let `tc-store`
  skip unchanged paths. No spec text or code says O9 does that, so the cost of O9 is unknown.
- *A cold disk still reads every byte.* The cache does not spare the read. Each file is read in
  full and fingerprinted (`crates/tc-journal/src/index.rs`, module header). On this VM, B's
  cold-disk opens without a cache took 35.5 s for 426 MB and 36.7 s for 664 MB
  (`results/cold/`). This VM's disk is not the reference laptop's SSD. On a SATA SSD at about
  500 MB/s, reading 664 MB adds about 1.3 s to the warm figure, about 3.9 s in all (estimate).
  After a reboot, the first open of the day is a cold-disk open. PERFORMANCE.md does not say
  whether "cold open" means a cold disk.
- *Every update and every received project pays the full open.* The cache is stamped with the
  app build (`IndexError::Build`). The no-cache open is 10 to 14 s loaded now. Quiet, it is
  about 6.4 s at HEAD: the evidence doc's 11.6 s loaded, divided by its own load factor of 1.8.
- *Hash-once and `ring` save less than the memo says.* Hash-once removes one SHA-256 pass,
  1,022 ms on 4 threads quiet (evidence doc). `ring` then saves about 0.4 s on the remaining
  per-line pass, because its median gain per line is 1.6 times. That gives about 5 s, the top of
  the memo's "4.5 to 5 s".
- *The cache is not committed.* `index.rs` and `tests/index.rs` are untracked in the worktree.
  No review has attacked the cache key.

**F2. The head-to-head mostly measures where projection happens, not the journal method.**
- *tC4's fold does projection work that the rebuild's fold does not.* `fold.mjs` returns
  projected `books`, `decisions` and `alignments`, and computes `verseTextMd5` validity. tC4's
  own C1 breakdown, run on this VM at d310bf3, puts 6,292 ms of an 18,291 ms fold on the
  alignment projection alone (`results/tc4full/tc4-bench-full.txt`).
- *With projection on both sides, the rebuild uses more CPU.* With every derived file rendered,
  the rebuild used 16.2 s CPU (1,226 + 14,970 ms) against tC4's 14.0 s (RT-5).
- *The measurements do support a cost for the file count.* At 617k files:
  - reading every file takes 2.8 to 3.1 s (memo probe);
  - listing every file and reading its size and time takes 1.4 to 2.3 s, with one thread per
    actor folder (`redteam/statprobe.txt`, load 8 to 10);
  - listing alone takes 0.47 to 0.85 s.

  tC4 also checks every event's schema twice: `validateSegment` calls `validateAction`, and
  `fold()` step 1 calls `validateEvent` again. A port need not do that.
- *What this means.* One file per action costs a port 1.4 to 3 s of the 3 s budget. Most of the
  rest of tC4's slowness comes from JavaScript and from choices a port can drop. The memo's
  real speed case is the file count and the cache, not a faster fold.

**F3. The loss count for A treats tC4's screens as part of its format.**
- *The suite requires a projected value, not a hidden fork.* The memo builds A with tC4's suite
  as the test of record, and then counts L8 as "required by the suite". R-8.6.4 says a non-text
  fork is "surfaced in `forks[]` … visible, never silent". The JC-35 G7 check asserts that the
  check fork is listed (`otherForks.length === 1 && otherForks[0].key === ct.key`), and that its
  projected value is the newest head.
- *The hiding is an app rule.* D98 point 8 says "Only verse text goes to the conflict review".
  That is a rule for tC4's app, not for its format.
- *B also picks a provisional value by clock.* The rebuild's `loss_e6_fast_clock_cannot_hide`
  asserts "the clock picks only this".
- *The memo treats A and C differently.* It grants C "closed" for L8 for rebuild users but
  denies the same to A, although both put tC4's format on Door43.
- *The case against A still stands.* L1 to L6 (structural chains), L12 and L25 (the fold's
  refusals), L13 and L16 (clocks and identity), L9 to L11, L27, and L14 and L22 (the sync
  rules) are real and remain under A. The count falls from 18 to 16 (RT-14), and P3 fails for
  those reasons, not because of newest-wins (RT-18).

**F4. B's cost in compatibility is near and concrete, and the memo understates it.**
- *Timing.* No tag carries team sync yet: `git merge-base --is-ancestor 72318d4 v4.0.0-rc.1`
  is false. But D102 puts team sync into v4.0.0, which tags "when the gates are green". The date
  before D102 was 2026-10-16, today. The memo says "Some pilot teams will sync by the time the
  rebuild ships". That is the expected case, not a rare one.
- *What a straggler sees (checked in the code).* `syncOperation.ts` commits, then verifies,
  then pushes, and it never force-pushes. The verify lists `tc/**` as extra files. So nothing
  is overwritten. But the person sees a message that tells them their project has a problem
  and to ask for help (RT-9). The memo's plan step 9 fixes that message only with a tC4 4.0.x
  release. That is the same release coordination the memo counts against A.
- *No way back.* T9 removes `checking/journal/**`, and nothing exports back to tC4's journal. If
  the rebuild has a blocking bug during the pilot, a team that moved cannot return to tC4 4.x
  without losing its history. Whether tC4 4.x can open a moved project at all, by seeding from
  the tC4-shaped sidecars, is not checked.
- *A risk not yet seen.* A blocked tC4 4.x user may share the project again from tC4, which
  creates a new Door43 repository. The team would then be split across two repositories. No
  code or report shows this happening; it is a risk only.

**F5. The memo's costs for A, B and C are uneven.**
- *A's port surface is understated.* See RT-17: about 2,800 more lines of tC4 app and sync code
  carry rules a byte-compatible port must match.
- *B already ports much of tC4, which C reuses.*
  - Unit I must port tC4's validator (§13.1 T2: container, sha256, schema v1 to v3, actor
    binding, file name).
  - It must replay tC4's per-key rules to compute parents ("the heads of k in the replay right
    after E was applied", T6).
  - It must convert `text.structure.apply` transitions slot by slot (the T6 table).
  - It must check the result against tC4's derived files (T7).

  C's converter is the same code, run on new segments. What C adds is a tC4 writer with an HLC,
  byte-identical sidecar rendering (B's sidecars are already tC4-shaped, `schemaVersion` 1 and
  2), and G1 to G7. "Roughly one more unit the size of X or I" counts the shared part of I
  twice.
- *The work A would throw away is about a day and a half.*
  - STORAGE.md was first committed on 2026-10-15 at 07:58 UTC.
  - `tc-journal`'s first commit was at 12:44 the same day.
  - J2's review fixes are from 2026-10-16.

  "About 31,700 lines" sounds large. In calendar time it is small. By the same measure, the
  port's line count is cheap too.

**F6. The two designs differ in maturity, and the memo's summary hides it.**
- *tC4's format.* Segments have been written since 2026-08-19. Measured against that format at
  d310bf3:
  - the journal suite: 368 of 368 (re-run, `redteam/validate-journal.txt`);
  - the normative gate: 105 rules (`normative/check.mjs` passes);
  - 70 refusal codes;
  - the journal and sync vitest files: 323 passed, 8 skipped and 1 timeout of 332, with the
    VM at load 7 to 10.
- *B.* B has a fold and nothing below it. Sixteen of its 34 closures "wait" for units that are
  not built. A fresh review of J2 found 12 problems, one of them a blocker, and the fixes
  landed today.
- *What the memo should say.* The memo states both facts in §9. Its headline ("B … can meet the
  3 s budget"; "closed" in §6) reads as if B's design-level closures and the 2.6 s weighed as
  much as tC4's tested behavior. They do not yet.

**Where the memo holds up.** Each of these was checked and stands:
- the per-file cost (memo probe and red-team probe);
- the per-save `metadata.json` rewrite under A, which is larger than stated (RT-12);
- the per-commit git tree of about 17 MB per actor folder;
- tC4's open memory of 0.8 to 1.0 GiB;
- tC4's per-save full fold (`publishAndRegenerate` sets `foldCache = null`, then
  `ensureFold()`);
- L22 and L8's projected value, both normative;
- the single 4 MiB action limit for a gateway change (`applyGatewayChange` publishes one
  action; `seal.ts:31`);
- the fact that 4.x refuses rather than overwrites.

### Corrections made in the body

| Id | Where | It said | It now says | Evidence |
|---|---|---|---|---|
| RT-1 | Short answer, bullet 2; §12 | "tC4's method is slower at every stage both systems share"; "A is slower at every measured stage" | tC4 is slower to open. Its fold also projects every book, so the fold rows do not compare one stage. With projection on both sides the rebuild used more CPU | `journal/fold.mjs` (return value, `verseTextMd5`); `results/tc4full/tc4-bench-full.txt` C1; `results/all-combined.md` |
| RT-2 | Short answer, bullet 3 | "A Rust port of tC4's method would not be faster than B" | true with a fold cache on both sides; without one, the estimates overlap (port 6 to 10 s, B 7.8 s quiet to 14 s loaded) | §4's own estimates; `redteam/statprobe.txt` |
| RT-3 | Short answer, bullet 5; §2; §4; §12 | 2.6 s "cold open", with no qualifier | load and fold (O8) only, warm page cache, one process reopening; the cache still reads every byte | storage worktree `docs/STORAGE.md` working copy §14, §6.1 O9, §6.2; `crates/tc-journal/src/index.rs` header |
| RT-4 | Short answer, bullet 5; §1; §12 | "tC4 never built a cache or snapshot" | tC4 keeps the last fold in memory (`foldCache`, `lastFold` from #94) and drops it at every save; nothing lasts from one open to the next | `src/data/journal/journalingStore.ts` lines 509-519, 947, 1303 |
| RT-5 | §2, row 3 | tC4 "13.9 s wall, 14.0 s CPU" under "fold plus projection of every derived file" | that is the fold alone; tC4's rendering of derived files (`checkpoint.mjs`) is not in it; the rebuild's figure adds two processes | `bench/tc4-open.mjs` times `fold(readUnion())`; `bench/h2h.rs` `project()` |
| RT-6 | §2, row 4 | (the note did not say that tC4's open includes projection) | added | as RT-5 |
| RT-7 | §2, cold-disk row; §12 | "one file per event is 6 to 7 times slower to read from a cold disk" | 5.9 times (278k) and 6.7 times (history); one thread with synchronous calls against four threads; one sample each | `results/cold/tc4-open.txt` 209,253 ms; `rb-tc4shape-t4.txt` 35,465 ms; `tc4-open-history.txt` 245,475 ms; `rb-bible-t4.txt` 36,708 ms |
| RT-8 | §2, probe | "`ring` is 1.6 to 1.9 times faster" | 1.4 to 1.9 round by round; per line 1.44 to 1.83 (median 1.6) | `results/memo-probe.txt` |
| RT-9 | §7, B | "a tC4 4.x client's sync stops with `sync.verify-failed`" | it stops before the push; the verify is the last check and an earlier one can refuse first; the person's message added | `src/data/share/syncOperation.ts` steps 4 to 6; `src/i18n/en.json:138` |
| RT-10 | §4, A as tC4 runs it, save | "and a whole derived-file rewrite" | only the derived files the action touches, each written with `update_ingredients`, which makes the platform rescan every file; no measurement of the 9 to 12 s proxy is in the results folder | `journalingStore.ts` `publishAndRegenerate` → `installAndConverge(affected)` → `installDerived`; `src/data/serverApi.ts` 164-168 |
| RT-11 | §4, A ported, cold open; "Why A cannot win" | a size and time cache "could come near 3 s"; "The file count alone uses the 3 s budget" | about 3.5 to 4.5 s; the file count uses the budget when each file is read, and half to three quarters of it when only size and time are read | `redteam/statprobe.txt` (list 0.47 to 0.85 s; list and stat 1.4 to 2.3 s on 3 threads, 2.7 to 4.1 s by path) |
| RT-12 | §4, A ported, save; §8; Sources | `metadata.json` about 57 MB at 278k ("tC4's measured proxy") and 126 MB at 617k | about 74 MB and 164 MB; no measurement of the proxy was found | entry format of `conformance/sample-burrito/metadata.json`; actor id is "a" + 15 hex (`identity.ts:195`); `segmentName` in `journal/files.mjs` |
| RT-13 | §6, L8 under A; §13, question 3 | "reopens; required by the suite"; "A requires tC4's rule" | the suite requires the newest head as the projected value and lists the fork; a port can show it | BURRITO-SPEC R-8.6.4; `conformance/validate-journal.mjs` G7 check; D98 point 8 |
| RT-14 | §5, I2; §6 Totals; §11; §12 | "18 cases reopen … Two of them (L8, L22) are required by tC4's own suite" | up to 18; 16 if the port shows `forks`, `retained` and `pendingStructural`; only L22 is required by the suite | as RT-13; `journal/fold.mjs` 769-792 |
| RT-15 | §6; §9 | 68 tests in `loss.rs`; 17 property tests | 67; 16 (each count had included a comment line containing "#[test]") | `git show 6e8adac:crates/tc-journal/tests/loss.rs \| grep -c '^\s*#\[test\]'` |
| RT-16 | §7, A | "tC4 locks its format after 4.0.0 (D47)" | after 4.0.0 a format change needs a `schemaVersion` bump, a written migration and fixtures; additive optional fields are exempt | `docs/DECISIONS.md` 571-577 |
| RT-17 | §10, A, new work | the port is 3,750 + 3,507 + 5,100 lines | adds `journalStore.ts` 824, `opsLog.ts` 717, `verify.ts` 316, `seal.ts` 159, `syncOperation.ts` 787 | `wc -l` at d310bf3 |
| RT-18 | §10, PLAN.md row | P3 must be rewritten "to allow newest-wins for alignments and checks" | P3 fails under A because of the structural rules, the fold's refusals and the sync rules | as RT-13; `docs/PLAN.md` P3 |

### Claims checked and found correct

**The rebuild's tests and code.**
- Every test named in §5 and §6 exists at 6e8adac. All 24 were checked by name.
- `tc-journal` has 18,053 lines of library code and 13,609 of tests; `tc-corpus` has 4,300.
- `ring` 0.17.14 is in `Cargo.lock`, through `quinn-proto` and `rustls-webpki`. `sha2` is
  0.11.1.
- The J2 fixer's 2.6 s, 2.9 s and 10 to 14 s are in the working copy of §14. The 7.8 s at
  225d19e is in the evidence doc.

**tC4's code and specification.**
- `fold.mjs` has 1,063 lines and `journal/*.mjs` 3,754. There are 15 ops.
- `journalingStore.ts` has 3,507 lines and `validate-journal.mjs` 5,109.
- There are 70 refusal codes, and the normative gate passes.
- `seal.ts:31` sets the 4 MiB limit.
- tC4 replays the outbox at open (`recoverAndConverge` step 1).
- `verify.ts` skips only journal files, OBS base files and `audio/`.

**tC4's dates and decisions.**
- v4.0.0-rc.1 was tagged on 2026-10-02, and the sync shell commit 72318d4 is not in it.
- D102 is quoted correctly.

**The benchmark figures.** Every median in §2 matches `results/all-combined.md`. The memo probe
figures match `results/memo-probe.txt`.

**tC4's own records.**
- #92 took the fold from 9,279 ms to 4,239 ms.
- The reference reader plus fold took 41,258 ms.
- The Titus open took 5,135 ms.
- #423 took the large open from "over 120 s" to "3.2 s click-to-text" (commit b10f33b).
- tC4 estimated a whole-Bible open at 3 to 4 minutes, then about 6 minutes.

### What the memo should add before the owner decides

1. Measure a full open (O1 to O13) with the fold cache:
   - with a cold page cache;
   - after an app update (no cache);
   - on the reference laptop.
2. State the cost of O9's classification, or specify how the heads files let O9 skip
   unchanged paths.
3. Make the recommendation depend on the owner's answer to question 1: C for mixed teams in the
   pilot period, B otherwise. Show C's cost net of unit I.
4. Add a way back for a team that moved, or say plainly that there is none.

### How each point was answered

I checked each point again before answering it. "Changed" names the edit, marked *(A-n)* in the
body.

| Point | Answer | Evidence | Changed |
|---|---|---|---|
| Strongest argument: wait for question 1; B is not dominant until a full open is measured | Partly accepted. B or C does wait for question 1, and B's 3 s is not shown. The rest of the recommendation does not wait: A is out under either answer (the red team's own F3), and the next units are the same under B and C | F3, last point; §3 | A-1: short answer, §12, plan step 9, question 1 |
| F1: only O8, warm page cache | Accepted (already RT-3) | storage worktree §14 working copy | A-2: plan step 5 measures O1 to O13 |
| F1: O9 not timed, cost unknown | Accepted, and it is worse than unknown. §6.2's first test needs `e`, the md5 of the projection, for every derived path, so O9 as written projects every derived file at every open: 9.96 s wall, 14.97 s of CPU here | STORAGE.md §6.1 O9, §6.2; `results/all-combined.md` rb-tc4shape-project | A-2: plan step 2 (O9 compares heads files), §2, §4, §11 |
| F1: a cold disk still reads every byte | Accepted. PERFORMANCE.md does not say whether a cold open means a cold disk | PERFORMANCE.md budget table; `results/cold/` | Question 4; plan step 5 |
| F1: the cache is stamped with the app build; quiet no-cache open about 6.4 s | Accepted | `IndexError::Build` in `index.rs`; evidence doc: 11,572 ms at a load factor of 1.8 | A-5: plan step 1 keys the cache on its encoding version. A-4: §4, §11 |
| F1: hash-once and `ring` save less | Accepted: about 5 s, not 4.5 to 5 s | evidence doc: one pass 1,022 ms on 4 threads; RT-8 | A-4: short answer, §4, §11, §12, question 4 |
| F1: the cache is not committed | Accepted; it was already plan step 1 | `git status` in the storage worktree | none |
| F2: the head-to-head measures where projection happens | Accepted (RT-1, RT-5, RT-6). One limit: 16.2 s against 14.0 s compares rendering every file to bytes with projecting into memory, so it does not show B's fold slower either. The speed case is the file count and the lasting fold | `bench/h2h.rs`, `bench/tc4-open.mjs` | short answer, bullet on speed |
| F3: the loss count treats tC4's screens as format | Accepted (RT-13, RT-14, RT-18). B also picks a provisional value by clock, but only for which candidate shows while the conflict stays listed; a person still chooses | `loss.rs` line 764 (the test then keeps the slower clock's version) | none beyond RT-13, RT-14, RT-18 |
| F4: the cost is near | Accepted | D102 | question 2 |
| F4: a straggler sees a message that reads like damage | Accepted (RT-9). Rejected: that asking tC4 for a better message is the same coordination as A. B is safe without it, because 4.x refuses and the §13.3 bridge converts later segments. A needs a tC4 release before each new op (R-8.3.9) | `syncOperation.ts`; STORAGE.md §13.3; BURRITO-SPEC R-8.3.9 | §11; plan step 10 marked optional |
| F4: no way back | Accepted. tC4 4.x seeds a journal when it opens a project that has none, so an export without history is a way back. It is not built or tried | BURRITO-SPEC R-8.8.2, R-8.8.3; `journalingStore.ts` 1583, 1670 | A-7: plan step 8, §11, question 7 |
| F4: a team split across two repositories | Accepted as a risk; nothing shows it happening | none | §11 |
| F5: A's port surface understated | Accepted (RT-17) | `wc -l` at d310bf3 | none beyond RT-17 |
| F5: C's cost counts unit I twice | Partly accepted. T2, T6, T7 and the §13.3 bridge are B's work too. C still adds a writer, an op mapping, byte-identical sidecars, an incremental converter and G1 to G7. It also needs a rebuild user's own change to come back from Door43 with the same id, which the red team did not count | STORAGE.md §13.1 T2, T6, T7; §13.3 | A-6: §10 |
| F5: A throws away only about a day and a half | Accepted as a fact. It does not change the choice: A's cost is P3 and the speed work tC4 lacks | `git log` of the storage branch | A-6: §10 |
| F6: maturity hidden by the headline | Accepted | §6 totals; `redteam/validate-journal.txt` | A-3: short answer, §11 |
| Add 1 to 4: full open, O9, a recommendation that depends on question 1 with C's net cost, a way back | Accepted, all four | as above | plan steps 2, 5, 8 and 9; §10; §12 |
