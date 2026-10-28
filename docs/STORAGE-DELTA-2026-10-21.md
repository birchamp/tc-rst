# Storage delta: edits made outside translationCore, and carried files (2026-10-21)

Status: Applied to `docs/STORAGE.md` by the S2 merge.

Line numbers written `S2:n` point into the copy of `docs/STORAGE.md` from the S2 branch at
3b075d7: `tracker:roadmap/evidence/2026-10-21/STORAGE-s2-3b075d7.md` (5,517 lines). Code paths
are on the branch `s2-proof-executor` at 3b075d7. A sentence marked **(inference)** is my
reading. It is not in the rulings or the code.

The authority is the two rulings:

- `docs/rulings/2026-10-21-outside-edits.md` (called "OE" below, with its rule number);
- `docs/rulings/2026-10-21-carried-files.md` (called "CF" below, with its rule number).

---

## 1. Summary

Today's design takes another program's edit of any project file into the project on its own.
It records the edit as changes, with the parents the other editor started from (S2:4020-4158).
The owner ruled against that. The app's own record is the project (OE 1). A changed file never
breaks the project (OE 2).

- **A changed book file** (USFM, or Markdown for an OBS story) is kept first. Then the app asks:
  "This book has been changed externally. Would you like to import it into this project?"
  *Import* runs a new import, "Update a book from USFM" (or a story from Markdown). *Not now*
  keeps the project's text, and the copy stays in the notices with *Import* (OE 3).
- **Any other generated file** (`metadata.json`, the settings, the sidecars) is written again
  from the record. A copy of the changed file is kept, with a one-line notice. No question is
  asked (OE 4).
- **"Update a book"** is also an import in its own right, for a file edited anywhere (OE 5).
- **Door43 merges** teammates' work that arrives through translationCore's sync (OE 6).
- **A folder pushed to Door43 by another tool** is unsupported. Nothing is lost: the app keeps
  its own text and says so once (OE 7).
- **Edits made with Door43's own tools** are still open. That is owner question H18.
- **Carried files** (audio and other files that travel with a project): each version records its
  content hash, the version it replaced, its Door43 user and its time (CF 1). No old bytes are
  kept in the project (CF 2). Two versions that replaced the same version are a conflict. The
  choices are *Keep this one* and *Keep both* (CF 3). The feature is built when translationCore
  adds the feature that uses carried files. Until then the format keeps a place for the record
  (CF 4).

So automatic recording goes away. So do the two-phase recording of `metadata.json`, the hold
for disk damage that keeps size and time, and the backups of old carried versions inside git.
What stays is the core that tells the app's own bytes from anyone else's: the placement ledger,
the compare-and-swap before every write, and the check that the journal still holds what a book
file shows.

`docs/STORAGE.md` is being edited on the S2 branch, so it is not edited now. This delta is
applied when S2 merges. ROADMAP.md says the same under "Decisions I made".

---

## 2. The new rules, section by section

Each item gives the section, what it says today, and the new text or rule.

### 2.1 §0, the design on one page

- **Item 6** (S2:46-50). It stays. Add one sentence: "A derived file that is not what the app
  wrote is kept first; a book file then waits for the person's answer (§11.1), any other derived
  file is written again with a notice."
- **Item 7** (S2:51-54), "record edits made on Door43". Pending H18 (§2.12). If H18 says
  unsupported, the sync is: seal, fetch, take in new packs, commit with parents
  `[local, remote]`, push, confirm.
- **Requirement table**, row "Never lose, hide or rewrite work" (S2:64). Replace "deletions and
  reverts from outside tools are conflicts" with "a changed file is kept, and nothing in it
  enters the project unless a person imports it".

### 2.2 §1, invariants

| Id | Today | New |
|---|---|---|
| I8 (S2:84) | "Before the app replaces or removes a derived or carried file, it checks that the bytes on disk are the bytes it last wrote. Different bytes are recorded as changes first." | **Nothing overwritten that someone else wrote.** Before the app replaces or removes a derived or carried file, it checks whether the bytes on disk are its own: the bytes it last wrote, the bytes of a write it had begun, or the bytes the project would write now (§6.2). Other bytes are kept first. A book file or story is kept and left as it is until the person answers (§11.1). Any other derived file is copied, with a notice, and then written again. A carried file's new version is recorded as a version (§11.4). |
| I11 (S2:87) | "A missing file is never read as a deletion. Removals and deletions that come from outside tools become conflicts, not replacements. A book removal never hides work its author had not seen." | **Deletions are deliberate.** A missing file is never read as a deletion. A verse or record that an imported file lacks becomes a conflict, never a removal (§13.6). A book removal never hides work its author had not seen. |
| I13 (new) | — | **The record is the project.** No derived file's content enters the project without a person's choice (*Import*, *Bring it back*). Changes that arrive as packs from translationCore (sync, exchange file) are the only exception, plus Door43 edits while H18 keeps §11.2 as designed. A carried file is not derived: a new version of it found in the folder is recorded as a version (§11.4), since that is the only way such a file changes. |

The other invariants are unchanged. I4 (observed parents, S2:80) covers the import already: an
imported change names the heads of the version the file was written from, the "exact base an
automated writer computed". I7 (S2:83) is unchanged. The write groups are the same, and a
changed file that is kept is listed in `metadata.json` as it is on disk.

### 2.3 §3.6, local state in `.tc/`

- **`materialized.json`** (S2:1158). The ledger state `held` keeps one meaning: "a book file or
  story changed outside the app, kept, waiting for the answer". Its cause `damage` goes
  (§2.5). Add one member, `kept`: one entry for each kept copy that still has a notice:
  `{path, copy, md5, size, base, why}`. `base` is `placed:<md5>` (the placement the change was
  found against), `head` (R14's base, when `.tc/` was lost), `copy` (heads written into the
  path's heads file under the copy's own md5: the zip's pack-set heads of §2.13) or `none`. A
  `placed:` or `copy` base whose heads are not in the heads file counts as `none`. `why` is
  `changed` (a file changed outside) or `lost` (the app's own bytes that show work the journal
  lost, §2.5 row N4). Each kept copy has its own entry; a second save of the same file adds an
  entry, it never takes over the first one's (§2.11). The entry is written in the same
  manifest write as the ledger state that follows the copy (§2.4, crash rows in §2.6).
- **`heads/<escaped path>.bin`** (S2:1159). Keep it. It now holds the heads of each placement
  the ledger names **and of each placement a `kept` entry names as its base**. That base is
  what an *Import* of the copy compares against. The purpose column changes from "the base of
  an outside edit" to "the base of an import of a kept copy (§13.6), and what the ledger asks
  the fold before it lets the bytes be written over (§6.2 `holds`)". In the code this is the
  keep list of `put_heads` (`crates/tc-store/src/materialize.rs:1370-1374`), which today keeps
  nothing while the path is `Held`; it must also keep every base a live `kept` entry names.
- **`external/`** (S2:1163). Remove the clause "and, for a copy of `metadata.json`, while the
  recording of that version has not ended (§5.10): dismissing the notice then hides it for the
  session and keeps the copy". A copy is kept until its notice is dismissed or its *Import*
  finishes. Dismissal keeps today's byte check: it removes the copy only while every byte still
  matches. Every copy is made by one no-replace helper. A copy whose bytes are already there
  under an earlier name is found and flushed, not written twice (S3 step 2's `preserve::keep`).

### 2.4 §5.10, materialize

Today, M1 (S2:1980-1990) decides each path by the ledger. *Hold* copies and drops the path.
*Record* copies the bytes, hands them to the actor, and drops the path until the batch is
durable. M6 (S2:2020-2026) does the same for `metadata.json`. The long paragraph at
S2:2061-2102 makes `metadata.json` "pending" while its edit is recorded. Every group then waits
for the actor. The writer retries a refused batch, and the session remembers each version whose
recording has not ended.

New rule for M1 and M6. Each path is decided by the table of §6.2 (§2.5 below). The actions
are:

| Action | What the writer does |
|---|---|
| *Write*, *Remove*, *Refresh*, *Wait* | as today |
| *Keep and hold* (a book file or story that is not the app's own) | 1. Copy the bytes with the no-replace helper and flush the copy and its folder. 2. Write the heads of the base into the path's heads file, if they are not there already. 3. Write the manifest once: state `Held{d}` and a `kept` entry. 4. Drop the path from D and X. 5. Add the book to the question (§11.1). The file is left as it is. |
| *Keep and write* (any other derived file that is not the app's own, `metadata.json` included) | Step 1 as above; then step 3 writes only the `kept` entry (the state is not `Held`). Raise a one-line notice ("settings.json was changed outside translationCore and was written again. A copy is in .tc/external/…"). The path stays in D and is written in this same group. |
| *Keep and write, lost* (the app's own bytes that show work the journal lost, row N4) | Steps 1 to 3, with `why: lost`. Raise a notice with *Bring it back*. Then write the path. |
| *Restore* (`project.json`, `format.json`) | as today (§3.5): copy, then restore |

**A copy that cannot be made.** When step 1, 2 or 3 fails (a full disk, a folder another
program holds, the project turned read-only), the file is left as it is and the path waits, as
R19 does: no group writes over it, `metadata.json` lists it as it is, and the next look tries
again (L130's rule: only a durable copy counts). Nothing is ever written over bytes whose copy
is not durable. The notice says why ("Titus was changed outside translationCore. The disk is
full, so translationCore cannot keep a copy yet and leaves the file as it is.").

**The materializer leaves a changed book file alone until *Import* or *Not now*.** The reasons:

1. OE 3.4 says *Not now* "writes the book file from" the project, and OE 3.3 says *Import* writes
   it "then". So until an answer, the file is not written.
2. The person may still have the file open in Paratext or a text editor. Writing over it there
   would change a file under the editor, and its next save would write the old buffer back.
3. The state already exists and is crash-safe. A held path (S2:2590-2624) is skipped by every
   group (M0, S2:1977-1979). `metadata.json` lists its bytes as they are (M2). A commit tree takes
   the fold's projection, never the disk (§9.4, S2:3710). A deletion releases it (R16, R18);
   its `kept` entry and copy stay, with *Import*, as after *Not now*.
   Nothing new is needed for the waiting time.
4. The copy is durable before the state names it, so a crash at any point leaves either the
   changed file, found again at the next open, or the copy and its `kept` entry.

Other derived files are not left waiting. OE 4 asks no question, and `metadata.json` must list
the folder as it is (I7). A waiting state for them would bring back what the ruling removes.

**What goes from §5.10:**

- M1's *Record* branch and "Recording always ends" (S2:1985-1988).
- M6's "copied and recorded (§11.1), the file is not renamed over, and the job after their
  recording writes it" (S2:2022-2024). M6 now copies and writes in the same group.
- The `metadata.json` recording part of the paragraph at S2:2061-2101 (S2:2063-2084, from
  "`metadata.json` itself is rewritten whenever…" to "…writes the file again from the project"). Replace it with: "Bytes in
  `metadata.json` that are not the app's own are copied to `.tc/external/` with a notice, then
  written over in the same group. Nothing in them is recorded."
- The sentence at S2:2055-2060 about "the listing lags until the actor has recorded them".
- In the paragraph at S2:2033-2046 ("There is no intent file…"), the revert text: "the bytes
  before them are not the app's own any more … (R11, with the revert rule of §11.1 X3) … R9".
  Replace it with: "Bytes equal to a placement the state names (`old` or `new`) are the app's
  own (§6.2). A program that puts the app's earlier bytes back has put back a version the
  project holds, and writing over it loses nothing while the fold holds what those bytes show
  (`holds`, row N4)."
- In the residual race (S2:2103-2105), "the file watcher records that program's next save" becomes
  "the next look (a focus, an open, or the next M1 of that path) keeps that program's next save".

### 2.5 §6.2, telling the app's own writes from changes made outside

Today (S2:2501-2762), the section answers several questions: is it settled, is it clean by stat,
is it converged, is it damage (R10), is it a revert (R9, R11), what base to record against. The
new section asks one question about the bytes: **is this the app's own write?**

**The app's own bytes** at a path are:

- the bytes of the placement `Placed{p}` (the version the app last wrote);
- the bytes `old` or `new` of `Writing{old, new}` (the version it last wrote, and the version it
  recorded in the ledger before renaming);
- the bytes the project would write there now, when every value could be read (today's R2).

Everything else is **changed externally**.

These parts of §6.2 stay as they are: settled entries, clean by stat, the scope digest,
converged without reading, what a skipped read cannot cause (S2:2507-2589); the four states and
the rule that the ledger names at most two placements of a path (S2:2626-2647); the facts
`disk`, `projection`, `heads readable`, `holds` and `newer fold` (S2:2649-2665). The field
`Facts::same_stat` goes. In `ledger::next` only R10 reads it (the `damage` flag,
`crates/tc-store/src/ledger.rs:355`). The helper `Writer::same_stat`
(`crates/tc-store/src/classify.rs:159`) stays, because "clean by stat" uses it (`:185`).

**The held-paths section** (S2:2590-2624) is rewritten. A path is held only while a changed book
file or story waits for the answer. *Take in the file as an edit* becomes *Import*. *Restore
from the project* becomes *Not now*. The `StorageDamage` notice goes. A change that kept its size
and time is no longer a separate case. The storage check (O14) finds it, like any changed file,
and the same rule applies: a book file is kept and asked about, any other file is copied and
written again. ROADMAP H8 notes the same: "(b) … follows from rule 1".

**The new transition table** for a read that found bytes `d` (the first row that applies):

| # | Was | State | Condition | Next state | Action |
|---|---|---|---|---|---|
| N1 | R1 | any | newer fold | same | Wait |
| N2 | R2 | any | heads readable and `d` = projection | `Placed{d}` | Refresh. A hold ends. Its `kept` entry and copy stay, with *Import*. |
| N3 | R3 | `Held{h}`, `d = h` | | same | Refresh |
| N3a | new | own, `d` = the md5 of a kept copy whose entry lives (the person answered *Not now*) | holds is none | `Placed` | as N6-N8 (Wait, Remove or Write), with no new copy, no notice and no question: the bytes are kept already |
| N4 | R4 | own | holds is false or none | `Placed` | *Keep and write, lost*, base = the ledger's heads of `d`, cut to the changes the fold holds applied and readable (today's R4 base) |
| N5 | R5 | own | holds, projection not asked | `Placed` | Refresh |
| N6 | R6 | own | holds, heads not readable | `Placed` | Wait (`ValuesUnreadable`) |
| N7 | R7 | own | holds, projection absent | `Placed` | Remove |
| N8 | R8 | own | holds, projection differs | `Placed` | Write |
| N9 | R9, R11-R14 | not own, a book file or story | | `Held{d}` | *Keep and hold*. Base: `Placed{p}` gives `placed:p`. `Held{h}` keeps the base of the waiting entry. `Unknown` gives `head` when HEAD was made by this app, else `none` (today's R14). `Writing` with `d` neither gives `none`. |
| N10 | R9, R11-R14 | not own, any other derived file | | `Placed` after the write | *Keep and write* (for `project.json` and `format.json`, *Restore*) |
| N11 | R11-R14 for carried paths | not own, a carried file | | as §11.4 | a new version (§11.4) |

"own" means the bytes are one of the placements the state names: `Placed{p}` with `d = p`, or
`Writing{old, new}` with `d = old` or `d = new`.

Why N3a is needed: `NotNow` turns `Held{h}` into `Placed{h}` (as `Restore` does today,
`ledger.rs:584`), and no heads file names `h`, so `holds` is none for those bytes. Without
N3a, a crash before the rewrite, or a rewrite refused because Paratext still holds the file
open (a common case on Windows: `WriteFailed` gives `Placed{old}` = `Placed{h}`), would send
the person's own given-up bytes to N4: a second copy and a false notice that "a file of saved
changes was damaged". Today the same path ends in R4 and records the held bytes on its own.

The rows for a missing or unreadable file, R15 to R19 (S2:2701-2705), stay. The other events
(S2:2712-2725) stay, with these changes:

- `Recorded{d}` becomes `Imported{d}`. It is sent when the import batch of a kept copy with
  bytes `d` is durable and the path still holds `d`. Next state `Placed{d}`.
- `TakeIn` goes.
- `Restore` becomes `NotNow`: `Held{h}` becomes `Placed{h}`, then *Write*.
- `Kept{d, copy}` loses its `cause`.

**What this changes, and why it is safe.**

- Today R9 records `d = old` with no base. The owner accepted on 2026-10-19 that this could show
  an extra conflict after a crash. Now `old` is the app's own. So after a crash there is no
  extra conflict, no copy and no question. That is new row L146. The bytes `old` show a version
  the project holds. While the fold holds it, writing over it loses nothing (N8). When the fold
  no longer holds it, N4 keeps a copy. **(inference)** This retires L106, L111, L112 and L117 (§4).
- R10 (same-stat damage) goes. A bit flip in a book file is now found by the storage check and
  asked about. Nothing enters the project unless the person chooses *Import*. Any other file is
  written again from the record. L83 changes.
- `metadata.json` follows the same table as every file other than a book (S2:2740-2753 goes). It
  is never held and never pending.
- In the ledger paragraph that stays (S2:2626-2647), the sentence "The group writes
  `metadata.json` after its look, so a second write of it can begin after a look that found
  `old` and recorded it (R9), which settles nothing; that write's state names only `old` and
  its own bytes" goes. A look that finds `old` now settles the path (`Placed{old}`, N5-N8), so
  the second write begins from `Placed` like any other.

### 2.6 §6.4, crash points

- **M1, M2, M3, M6** (S2:2819-2824). Today, `d = old` goes to R9 and is "kept in `.tc/external/`,
  recorded with no base (at worst an extra conflict)". New: `d = old` is the app's own. It is
  written again from the project when the fold holds what it shows (N8), else kept (N4). No
  question and no conflict.
- **X1, X2** (S2:2840-2841, "outside-edit record") are replaced by these rows:

| # | Action | Crash after | State on disk | Next open | Result |
|---|---|---|---|---|---|
| X1 | keep a changed book file | copy written and flushed, manifest not | the file still changed; a copy with no entry | read again; the helper finds the same bytes under the earlier name, so there is no second copy; the entry is written; the question is asked | after |
| X2 | keep a changed book file | manifest written (`Held`, `kept`), question not shown | held | the question is asked at the open (O13) | after |
| X3 | *Import* | import batch durable, `kept` entry not ended | batch with `reconciles {path, md5}` | a `why: changed` entry whose md5 a durable, applied batch reconciles is ended, never imported twice; `Imported{md5}` if the path still holds those bytes; the file is written from the project. A `why: lost` entry is never ended this way (L115: an older batch that reconciled the same bytes says nothing about a save the journal lost later); *Bring it back* leaves out only the records an earlier reconciling batch changed and the fold still holds applied and readable | after |
| X3a | *Import* | import batch not durable (a crash, a full disk, a refused pending file) | `Held{h}` and its `kept` entry; perhaps an incomplete batch | an incomplete batch marks nothing imported (L127); the question is asked again at the open; after a refusal the report says why and the question stays | before |
| X4 | *Import* or *Not now* | entry ended or `NotNow` written, file not rewritten | `Placed{d}` (heads written by `Imported`) or `Placed{h}` (no heads) | `Placed{d}`: materialized at the open (N8). `Placed{h}`: N3a, materialized with no new copy or notice | after |
| X5 | keep and write another derived file, or *Keep and write, lost* | copy flushed, file not rewritten | the file still changed | read again; same copy found; written | after |
| X6 | *Keep both* (carried) | batch durable, new path not written or conflict copy not removed | the version still at its `.conflict-` path | the new path is written first, from the blob or, when the blob is missing, from the conflict copy stored again as a blob; the conflict copy is removed only after that (§2.14) | after |

### 2.7 §8, conflicts and choices

- **§8.1** (S2:3459-3464). The grouped row `OutsideEditLarge` stays, renamed to **one row per
  import**: "Imported from TIT.usfm (changed outside translationCore, 21 Oct): 12 verses in
  conflict", with *Accept all from the file*, *Keep translationCore's text* and *Review one by
  one*. It groups every conflict that one import batch made. The `CarriedFile` kind (S2:3451)
  reads: "two versions that replaced the same version, or a version that replaced none beside
  another".
- **§8.3, carried files** (S2:3545). Today: "*Keep this file*; *Keep the other*; *Keep both*". New
  (CF 3):
  - *Keep this one*, on each version. The other version's file is removed from the folder, and
    the change names every head.
  - *Keep both*. The other version becomes a new `CarriedFile` record at its own path. The name
    the ruling gives as an example, `jon_01 (Juan).wav`, breaks the path grammar (S2:1024 forbids
    the space). So the name is `<stem>-<person>.<ext>`, for example `jon_01-Juan.wav`. The person
    part is cleaned as an export file name is (§10.1 E4, S2:3959-3963). `-2`, `-3` are added when
    the name is taken. The target must be a free path (C7b's `FreePath`). See open question Q4.
- **§8.3, History** (S2:3553-3556). The cause "edited in another program" becomes "imported from
  a file". A carried file's version shows its number (its depth in the chain of versions it
  replaced), its Door43 user (the profile of the device that made it, shown only after D98
  consent, otherwise "translationCore user") and its time. *Restore this version* stays limited
  to versions whose bytes are on this computer (§5.9, S2:1937-1940).

### 2.8 §9.2 intake, §9.4 trees, §9.8 repack

- **§9.2 N4** (S2:3675). Pending H18 (§2.12).
- **§9.4** (S2:3703-3757). No change. A held book file is already taken from the fold's projection
  (S2:3710).
- **§9.8 step 1** (S2:3886-3893). Remove the paragraph about `refs/tc/carried`: "a commit with no
  parent that the writer sets at every checkpoint and before every repack … so a version
  replaced between two checkpoints … is never pruned (§11.4, L73)" and "A repack does not start
  while the reference cannot be written." Under CF 2 a replaced version that is not committed may
  be pruned after the 14 days of step 3. A version that is still a head (in conflict) stays: its
  file is in the folder, and a checkpoint commits it within 30 minutes (§5.13). The "commit being
  made" guard (S2:3909-3928) stays. It protects the blobs of a commit, not old versions.
- An existing `refs/tc/carried` (only projects made by development builds have one) is deleted
  at the next open. **(inference)** No released project has one.

### 2.9 §10, USB exchange

- **§10.2 R5** (S2:4001-4003). No change while carried files keep their interim form (open
  question Q3).
- **§10.2 R6** (S2:4004), "Edits made inside the file (§11.3)". See §2.13.

### 2.10 §11, edits made outside the app (intro)

Today (S2:4022-4025): "a derived or carried file that differs from what the app last wrote is
someone's work. It is recorded as changes … before the app writes that file again."

New: "A derived or carried file that is not the app's own (§6.2) is kept before the app writes
that file again. A book file or story waits for the person's answer. Any other derived file is
copied and written again. Nothing in a kept file enters the project unless a person imports it
(I13)."

### 2.11 §11.1, a file changed on this computer (rewritten as the ruling's flow)

**Detection.** It stays as today (S2:4029-4039): at open (O9), at window focus, by the storage
check (O14), and by M1 before any write. The watcher's events for derived files only mark the path
for the next look. **(inference, a simplification)** The watcher stays for the journal folders
(second writer, §5.0). The question itself is asked at open and at focus (OE 3). A changed book
file that M1 or the watcher finds while the window has focus raises the question at once.
**(inference)** The person is in the app, and the file is held from that moment.

**The flow for a changed book file or story `p` with bytes `b`:**

```
Q1  Keep: the copy, the heads of the base, the manifest (Held, kept), in that order (§5.10).
Q2  Decode b with the codec (§13.5) and compare it with the base, as §13.6 does (no batch).
    - Nothing differs but line ends and blanks at line ends (today's X3 whitespace rule): no
      question. The file is written again, with the `WhitespaceOnlyEdit` notice. The copy stays.
    - b cannot be decoded (a verse number twice, broken markers): the question says why and offers
      only Not now: "This book has been changed externally, and translationCore cannot read it:
      TIT 1:3 appears twice (line 212). A copy is kept at …. Fix the file, or choose Not now to
      keep the project's text." A later save of the file is looked at again.
    - Otherwise the book goes into the question.
Q3  Ask, once for every book found together:
      "This book has been changed externally. Would you like to import it into this project?"
      With several books: "These books have been changed externally: Titus, Ruth. Would you like
      to import them into this project?"   [Import]  [Not now]
Q4  Import: §13.6 on each kept copy, with the copy's base. Then Imported{md5}, and the file is
    written from the project. The report says "Titus: 12 verses imported, 2 to review."
Q5  Not now: NotNow; the file is written from the project. The kept copy stays in the conflict
    review's notices with Import and Dismiss until it is imported or dismissed.
```

- **A book file for a book the project lacks** (today X2 adds the book, S2:4066-4068; L67). It is
  kept and asked about ("A book file RUT.usfm was added outside translationCore. Would you like
  to import it into this project?"). *Import* adds the book (§13.6). On *Not now* the file moves
  out of `ingredients/`: its copy is already kept, and a file for a book the project does not
  hold would leave `currentScope` without it. **(inference)**
- **Another save of a held file.** It is kept again under a new copy, never replacing the first,
  with an entry of its own and the same base; the state becomes `Held` of the newest bytes. The
  question still names the book once, and *Import* there imports the newest copy. Each earlier
  copy keeps its own entry, so it stays in the notices with *Import* and *Dismiss* until it is
  imported or dismissed (OE 3.4). The second save need not continue the first: a cloud client
  can put back an older file after a Paratext save, and the Paratext save must not drop off
  every screen.
- **A read-only open** writes nothing, so it keeps no copy and asks nothing. It shows a notice:
  "Titus was changed outside translationCore. Open the project normally to import it."
- **Bytes the app wrote that show work the journal lost** (row N4). A copy is kept with the notice
  "Titus has work that the project no longer has (a file of saved changes was damaged). A copy is
  kept. [Bring it back] [Dismiss]". *Bring it back* runs the import of §13.6 against the N4 base.
  It works for every derived file kind that has a decoder (open question Q2).

**What goes from §11.1:** X1-X7 as automatic steps (S2:4045-4119); the toast with *Undo*
(S2:4114-4116); "What the writer does with the answer (D7)" as an automatic acceptance
(S2:4124-4148; the heads it writes become the heads `Imported{d}` writes); the `localFile`
source paragraph and `metadata.json` as a recorded file (S2:4150-4158). The decoders of X2
(S2:4047-4070) stay in `tc-journal` for §13.6, for *Bring it back*, and for §11.2 while H18 keeps
it. The "more than 50 keys" rule (S2:4093-4094, `tc_journal::outside::MANY_KEYS`) no longer
applies to an import that a person chose (§13.6). It stays for §11.2 until H18 is answered.

**Other generated files** (`metadata.json`, `checking/settings.json`, `checking/resources.json`,
the alignment and decision sidecars, `vrs.json`, `tc/comments/*.json`; `conflicts.json`,
`format.json` and `project.json` as today). These are rewritten from the record, with a kept copy
and a one-line notice (OE 4). An outside edit of `resources.json` no longer adds `ResourcesExtra`
members (§5.6, S2:1837-1839). An OBS template edit enters only through "Update a story" (§5.15,
S2:2276-2279). **(inference)** OE 4 says "and the like", and I count the sidecars among them
(open question Q8).

### 2.12 §11.2, a file edited on Door43: as designed, pending H18

Leave D1-D4 (S2:4165-4204) as written. Mark the section "pending owner question H18".

If H18 says **unsupported**:

- D1-D4 and §9.2 N4 go. The merge tree takes the fold's projection for every derived file, as for
  any commit.
- One check stays, for OE 7 and for H18 alike. At intake, for each commit after the last app
  commit (one with the `Tc-Frontier` trailer) that changed a derived file, the app says once,
  remembered by commit id: "Titus was changed on Door43 by jdoe (commit 3fa2c1d) with another
  tool. translationCore kept its own text. That version is in Door43's history."
- The device kind `Door43Edit` (`crates/tc-journal/src/ids.rs:134`) and
  `ExternalSource::Door43` can leave the format before the freeze (§2.15).
- Rows L14, L46, L90 and L91 change or retire (§4.2).

If H18 says **merged**, OE 7 cannot be kept exactly. A folder pushed by another tool is also a
commit without the trailers, and nothing in the commit tells it from a web edit. Its changed
files would be merged as Door43 edits, with conflicts shown. Nothing is lost, but the result is
not "translationCore keeps its own text". See open question H18.

### 2.13 §11.3, a file edited inside an exchange file

Today (S2:4211-4217), the §11.2 walk runs over the zip and records each difference as changes.
OE 1 says the app "never merges a file's changes into the record on its own". An exchange file is
not a Door43 repository, so H18 does not cover it. **(inference)** New rule: when a derived file
in the zip differs from the projection of the zip's own packs:

- a book file or story is kept as a copy in this project's `.tc/external/`, with a `kept` entry
  whose base is `copy`: the heads of those records in the zip's pack set, written into the
  path's heads file under the copy's md5 before the entry (§2.3). It is listed in the notices
  with *Import* ("Juan's file holds a changed Titus that was not made in translationCore.
  [Import] [Dismiss]");
- any other derived file is not taken in. A one-line notice names it.

The zip on the stick is never changed. The device kind `ExchangeEdit`
(`crates/tc-journal/src/ids.rs:137`) and `ExternalSource::ExchangeFile` are then no longer
written. See open question Q6.

### 2.14 §11.4 and §5.16, carried files

**The record (CF 1).** It needs no change of bytes. The kind `CarriedFile(path)` with
`CarriedValue::Present { oid, size, md5 }` or `Removed` (`crates/tc-journal/src/record.rs:493`)
and the change around it already hold every field the ruling names:

| CF 1 field | Where it is |
|---|---|
| identity (content hash) | `oid` (git blob id) and `md5`; a screen shows the version's number, its depth in the chain of `parents` |
| the version it replaced | the change's `parents` (`ChangeBody.parents`, `crates/tc-journal/src/change.rs:21`) |
| the Door43 user who made it | the change's `device`, whose `DeviceProfile` holds the Door43 username once D98 consent was given |
| when | the change's `at` |

**Old bytes (CF 2).** No old bytes are kept on purpose. Remove from §11.4 "The bytes of every
version recorded on this computer are git blobs in the local object database, kept reachable by
`refs/tc/carried` (§9.8), so repack never prunes a version that was superseded before a checkpoint
committed it" (S2:4227-4230). In **Placing**, replace "Bytes the app placed are written over or
removed only once their version is durable in the object database and was read back whole
through a handle of its own (L132, L135) … and makes it again if it was lost" (S2:4256-4263)
with this narrower rule:

> Bytes whose version is still a head of its record (a conflict side that moves between the path
> and its `.conflict-` copy when the provisional head changes, or the version *Keep both* moves
> to its own path) are written over or removed only after they are written and flushed at the
> place the projection now gives them. When that version's blob is missing, it is first stored
> again from the file (as L73 does). Only the bytes of a version that a newer version replaced,
> and that is no head, may go without this.

Why the narrower rule must stay: a translator records `jon_01.wav` (V1); a power cut loses the
loose blob, which nothing flushes; a teammate's V2, made from the same V0, arrives by sync and
becomes provisional. The group must write V1 to its `.conflict-` copy and V2 to the path. With
the blob gone, the conflict copy cannot be written, and replacing the path would destroy the
only bytes of V1, a version nobody chose to drop. CF 2 lets old bytes go only when "nobody else
changed that file"; here two versions stand. The rule needs no git flushes or backup packs (L135,
L136, L138 still retire): the atomic write at the new place is the durable copy.

A replaced version's bytes may go when nobody else changed that file. The versions that were
synced stay in Door43's history, and in the local `.git` as far as commits reach them.

**Conflicts (CF 3).** Two versions that replaced the same version are a `CarriedFile` conflict.
Both files stay on disk until a choice: the provisional one at the path, the other at
`<stem>.conflict-<oid8>.<ext>` as today (S2:4286-4293). The choices are in §2.7. A version that is a
head is never removed by the old-bytes rule (new row L151).

**When (CF 4).** The feature is built when translationCore adds the feature that uses carried
files. The format keeps the record now (§2.15). What the store does with carried files until then
is open question Q3. The recommendation is to keep S2's carried-file code minus the backups.

**§5.16** (S2:2281-2285). The text stays. "A file the ledger does not name as the app's is held,
not removed" becomes "… is recorded as a new version (§11.4), not removed".

### 2.15 §12, format versioning: what changes before the 1.0 freeze

The format is not frozen yet (H9), so these go into 1.0 with no minor raise. **(inference from
§12.2 rule 7, S2:4377-4385, and V's card)**

| Item | Change |
|---|---|
| `Cause::Import(ImportSource::…)` (`change.rs:46`, `:77`) | "Update a book" writes `Import(Usfm { file_name })`, which already exists. Add `ImportSource::Markdown { file_name }` for a story. Add an `unknown_base` mark to the import cause (as `External` has, `change.rs:41-44`), and rank it 0 in `CauseClass::rank` (`fold/meta.rs:292-297`), so a change an import makes with no parents is never chosen as provisional (§7.2 F7). Without it, a no-base import change, being the newest, would become the provisional text, and the book file and screens would show an old file's text over a teammate's newer verse until the review. |
| `Cause::External { source: LocalFile }` | No longer written. Remove it before the freeze if no frozen sample project holds it (V decides). |
| `ExternalSource::Door43`, `ExchangeFile`; device kinds `Door43Edit` = 4, `ExchangeEdit` = 5 | Kept while H18 is open. Removable before the freeze if H18 says unsupported and §2.13 stands. Then `Cause::External` can go entirely. |
| pending header `reconciles {path, seenMd5}` (S2:1243; `pending.rs:61`) | Kept with a narrower meaning: the batch of an *Import* or *Bring it back* of a kept copy, with `seenMd5` = the copy's md5. It makes X3 of §2.6 idempotent. `metadata.json` is no longer a valid `reconciles` path (`RepoPath::parse_source`). |
| `CarriedFile` record | No byte change (§2.14). `refs/tc/carried` is no longer written. |
| `.tc/materialized.json` (local, not exchanged, but V pins it) | `held` loses `cause: damage`. A manifest from a development build that says `damage` opens as a held change and is asked about. Add `kept`. The meaning of `Writing.old` changes, its bytes do not. |
| `.tc/heads/*.bin` | Same bytes. It also keeps the heads of the bases that `kept` entries name. |
| fold notices (`crates/tc-journal/src/fold.rs:600-704`) | `StorageDamage` goes. `OutsideEditHeld` becomes "book changed outside, waiting". `MetadataEditedOutside` becomes one notice for any rewritten generated file. These are not stored in the project. |
| `format.json` `derivedPaths` | No change. |

V no longer waits for a carried-file format change in S3 (V's card, "if H8 (c) was approved"). It
must pin the new manifest after S3 step 9 lands.

### 2.16 §13, import: "Update a book from USFM" (new §13.6)

It belongs in a **new §13.6**, not in §13.5. §13.5 (S2:4538-4597) makes a new project from raw
USFM and defines the codec. §13.6 changes an existing project. It uses §13.5's codec and its
aligned-USFM split. Two entries reach it: *Import* of a kept copy (§11.1, §11.3), and the menu
item "Update a book from USFM" (or "Update a story from Markdown") with a file chosen anywhere
(OE 5). F1 adds the menu item.

```
U1  Read and decode the file with the codec (§13.5; the same refusals, with the line). A story
    decodes into StoryBase, Frame and StoryRef. A file whose book the project lacks adds the book:
    BookPresence::Present { scope from the file, how: Added } with no parents, as X2 does today,
    so a concurrent removal is a visible BookPresence conflict (§7.6).
U2  Find the base, the version of the project the file was written from:
    a. a kept copy: the base its entry names (placed:<md5> gives the heads of that placement in
       the path's heads file, copy gives the heads kept under the copy's md5, head gives what
       HEAD's packs hold; none, or heads that are not there, give no base);
    b. otherwise, no base. (A chosen file byte-equal to a placement the ledger names would
       compare equal to its own base and import nothing, so it needs no rule of its own.)
U3  Compare each record of the file with the base, as the planner does today
    (`tc_journal::outside::plan_outside_edit`, X3 at S2:4075-4102): the whitespace rule, CRLF
    restored, the shown-digest prefilter (only records that differ are read). Then:
    - same as the base: nothing;
    - with a base, different from the base and the project not changed since (the record's heads
      are still the base heads): one change marked as imported, parents = the base heads;
    - with a base, different, and the project changed since (the heads moved): one change with
      parents = the base heads. It is a sibling of the newer head, so a conflict (X5 today).
      Import never overwrites a verse changed on both sides (L145);
    - equal to an older version in that record's own history (a revert, for example a cloud
      client restoring a copy): no parents, so a conflict unless equal (today's revert rule);
    - a verse or record the file lacks that had a value: a conflict, never a removal (I11);
    - no base: equal to the project's text, or to any version in that record's History, means
      nothing to import (the project keeps its text; the file's value is already in History).
      Any other difference is a change with no parents, so a conflict unless equal. All such
      conflicts form the import's one grouped row (§8.1). (inference: open question Q5)
    Every change made with no parents (the revert case and the no-base case) carries the
    import's `unknown_base` mark, so it is never provisional (§2.15): until the person chooses,
    the project's text stays the text shown and written.
    The "more than 50 keys" rule does not apply: the person chose the import, a removal is a
    conflict anyway, and the revert rule keeps a restored old copy from replacing newer work.
U4  Alignments, in the same batch, verse by verse:
    - a verse imported without conflict whose words are unchanged (only `layout` changed) keeps its
      alignment as it is;
    - a verse imported without conflict whose words changed: the alignment is carried over by
      `tc_align::reflow`, exactly as §5.1 step 5 does for a save (S2:1693-1704). Links to words
      the verse still has stay, new words go to the bank, words that are gone are dropped. The
      earlier alignment stays in History;
    - a verse that became a conflict: its alignment is not written. Validity is computed (§7.7);
    - a file with alignment markup (`\zaln-s`, from gatewayEdit, tcCreate or tC4): each verse
      whose markup reads apart (§13.5 "Aligned USFM", S2:4570-4592) imports its alignment as a
      change by the rules of U3. A verse whose markup does not read apart is imported as text
      with markup, and the report lists it.
U5  One batch: cause Import(Usfm { file_name }) or Import(Markdown { file_name }), this device,
    the pending header `reconciles {path, seenMd5}` for a kept copy. Changes that rewrite verse
    structure (spans, numbers) form a passage over their range (§0 item 3), as a section save
    does. Plain text changes are verse by verse. (inference: as X4 does today)
U6  Fold, end the kept entry, send Imported{md5} when the path still holds those bytes, and
    materialize the book. Report the counts: imported, in conflict, not readable.
```

How the base is known, in short. For the question flow the base is exact: it is the placement
whose bytes the file differed from, and its heads are kept for as long as the `kept` entry lives
(§2.3). For a file chosen from anywhere there is no base, and U3's no-base rule keeps every
disagreement visible. That includes the supported flow of OE 5 (edit a copy of the book file,
then Update a book): every verse edited in the file is a conflict, to accept in one click with
*Accept all from the file*. See Q5.

### 2.17 Other sections

- **§5.6** (S2:1837-1839): "written only by an import or a decoded outside edit of
  `resources.json`" becomes "written only by an import (and by a Door43 edit while §11.2 stands)".
- **§5.15** (S2:2276-2279): "an edit of the template lines made outside the app … is recorded as
  an External change on `StoryBase` (§11.1 X2)" becomes "… enters through Update a story (§13.6)".
- **§6.1 O9** (S2:2392-2405): "Edits made outside the app are recorded as changes (§11.1), or held
  (§6.2), before anything is rewritten" becomes "Changed files are kept (§11.1) before anything is
  rewritten, and changed book files are gathered for one question". "A book file the project does
  not show is examined as an outside edit and adds its book" becomes "… is kept and asked about".
- **§6.1 O13** (S2:2414): "recorded outside edits" becomes "the question about changed books, and
  the notices of kept copies".
- **§6.1 O14** (S2:2415-2432): "which holds a path that was clean by stat, so nothing is written
  over and nothing is recorded without the person" becomes "a changed book file is kept and asked
  about; any other file is copied and written again".
- **§16.2** table row "outside-edit recording | X1–X2" (S2:4932) becomes "a changed file kept,
  asked and imported | X1–X6 and X3a".
- **§18 limits** (S2:5207-5258). Limit 2: "The file watcher records the other program's next save"
  becomes "The next look keeps the other program's next save". Limit 13 is replaced: "A replaced
  carried version is not kept on this computer. A version that was synced is in Door43's
  history." Limit 14 stays. Add a limit: "A change that keeps a file's size and time is found only
  by the storage check, after the open."
- **`docs/TC4-DELTA-2026-10-16.md:1037`**, "an out-of-band edit is still recorded", becomes "… is
  kept and asked about". This is a declared difference from tC4. tC4 reconciles an out-of-band
  USFM edit on its own at open (health-storage §8, `reconcileUsfm`). The owner ruled for the
  question (AGENTS.md "Matching tC4"). Record it where APP-DESIGN lists declared differences when
  D1 builds the dialog.

---

## 3. What is removed

| Mechanism | Where it is today (S2 code at 3b075d7) | Replaced by |
|---|---|---|
| Automatic recording of outside edits of derived files | `crates/tc-store/src/outside.rs`: `examine`, `examine_copied`, `accept`, `found_outside`, `found_outside_answering`, `take_examined`, `recorded`, `edited_heads` (lines 344-905) | Keep (§5.10) and *Import* (§13.6). The pure planner `plan_outside_edit` (`crates/tc-journal/src/outside.rs:676`) is kept and reused by §13.6. |
| The two-phase recording of `metadata.json` | `outside.rs` `record_metadata_edit` (:711), `decode_metadata_edit` (:314); Writer maps `unrecorded_metadata`, `pending_record`, `shown_by_path`, `bases` (`crates/tc-store/src/writer.rs:172-241`); M6's recording branch; the writer's retry of a refused outside batch; S2:2061-2102 | copy, notice, write (§5.10) |
| Writer maps behind it | 4 of the 9 per-path maps go: `unrecorded_metadata`, `pending_record`, `shown_by_path`, `bases`. `waiting` (answers to *Take in*) becomes the answers to *Import*. `busy`, `written_seq`, `reasons` and `pending_carried` stay. | S3 step 8 moves fewer maps |
| Hold for same-stat damage | `ledger.rs` `HeldCause::Damage`, row R10, `Facts::same_stat`, the `damage` flag in `who`; the `StorageDamage` notice (`crates/tc-journal/src/fold.rs:671`) | N9/N10 for any change |
| R9's "ambiguous" bytes | `ledger.rs` `Who::Ambiguous`; S2:2033-2046's revert text | `old` is the app's own (§6.2) |
| *Take in* / *Restore* on a held file | `handle.rs:824` `restore_held`, `:848` `take_in_held`; `focus.rs:285` `take_in_held`; `writer.rs:499` | *Import* / *Not now* |
| *Undo* of an outside edit | `Command::UndoOutsideEdit` (`crates/tc-journal/src/command.rs:119`), `undo_outside_edit` (`crates/tc-store/src/commands.rs:206`) | none. History keeps every version. |
| Backups of old carried versions inside git | `git.rs`: `CARRIED_REF` (:34), the ref writer (:972), `sync_blob_backup` (:174), `one_object_packs` (:209), `copy_blob_to_durable` (:675-751); `carried.rs` `keep_prior_carried` (:428) except its "store the blob again from the file" half; `checkpoint.rs` `keep_carried_versions(_with)` (:173-191); Writer field `carried_kept`; §9.8's carried-ref step | nothing (CF 2), except §2.14's narrower rule for a version that is still a head: written at its new place first, its blob stored again from the file when missing |
| Watcher-driven recording | `watch.rs` tells the writer the paths of derived files; `focus.rs` `paths_changed` (:61) looks at each, and the ledger's *Record* records it | the watcher marks paths; the look at focus decides (inference, optional) |
| Model steps that exercised removed paths | `tests/store/model.rs`: `Damage` (:151), `Choose { take_in }`; the oracle's R9 exception ("no copy … but of the bytes a crash left", S2:4896) | §5 |
| §11.3 walk over exchange files | X unit (not built) | kept copy with *Import* (§2.13) |
| If H18 = unsupported: the Door43 walk | X unit (not built: `crates/tc-sync/src/walk.rs` in X step 5); §9.2 N4 | one notice per commit (§2.12) |

Kept, although they served recording: the placement ledger and its heads files; the planner and
the decoders in `crates/tc-journal/src/outside.rs` (for §13.6, *Bring it back*, and §11.2 while
H18 is open); the `reconciles` header; the copy helper; `holds`.

---

## 4. The rows of §15 that change

`grep -c '^| L[0-9]' ` on the S2 copy gives 143 rows, L1 to L143, each once. Only rows whose
status changes are listed. Line numbers are of the S2 copy.

### 4.1 Retired (the mechanism is gone, or the case is no longer a loss)

| Row | Line | Why it retires |
|---|---|---|
| L106 | 4803 | The reverted bytes are the `old` placement, bytes the app wrote. They are now the app's own (§6.2). Writing over them loses nothing while the fold holds what they show; when it does not, N4 keeps a copy. R9's stat test is gone. (inference; checked in the review: the heads of `old` stay in the heads file while the state is `Writing`, `materialize.rs:1372`) |
| L111 | 4808 | The same reasoning. The ledger still names at most one write per path. The "bytes it replaced are the app's only with their own size and time" part is gone. A revert to the app's own earlier bytes is not a loss. (inference) |
| L112 | 4809 | The `metadata.json` twin of L106: bytes put back after a kill past the rename are the write's `old` bytes (the model's `Revert { back: 0 }` after `KillAfterRename`), so they are now the app's own and are written over, with N4 if the fold no longer holds them. Its test asserted the opposite. |
| L117 | 4814 | As L106: a backup tool put back the app's own `old` bytes. |
| L134 | 4831 | No carried History bytes are kept (CF 2), and verification puts this case outside REACH. The blob-kind check may stay as a unit test. |
| L135 | 4832 | The durable backup of an old version is gone (CF 2: old bytes may go). |
| L136 | 4833 | The one-object backup pack is gone. |
| L138 | 4835 | The repeated backup is gone. |
| L141 | 4838 | `metadata.json` is never recorded. Its copy is flushed before the rewrite (new L147). |
| L142 | 4839 | No version is "seen already". Every rewrite of bytes that are not the app's own makes or finds a copy. |
| L143 | 4840 | No batch is made for an outside edit of `metadata.json`. |

### 4.2 Changed (new wording)

| Row | Line | New "What prevents it" |
|---|---|---|
| L41 | 4723 | Nothing enters the project without *Import*. On import, a verse the file lacks is a conflict, never a removal (I11). |
| L42 | 4724 | Nothing enters without *Import*. On import, a value equal to an older version of the record has no parents, so it is a conflict (U3). |
| L67 | 4754 | Local half: a book file for a book the project lacks is kept and asked about. *Import* adds the book; *Not now* moves the file out with its copy kept. The Door43 half is pending H18. |
| L73 | 4760 | *Restore* is offered only for versions whose bytes are here. A missing blob is stored again from the folder when the file holds its bytes, else the tree keeps the previous blob. A commit never fails for it. (`refs/tc/carried` is gone.) |
| L83 | 4770 | Every zip entry's CRC and size are checked (unchanged). On disk: nothing is taken into the project without the person. A changed book file is kept and asked about. Any other file is written again from the record, with a copy. |
| L92 | 4779 | Every non-derived metadata field is a record at import (P9). An outside edit of `metadata.json` is copied, not recorded. An OBS template edit enters through Update a story. |
| L105 | 4802 | The ledger names every change the bytes show. When the fold no longer holds one applied and readable, a copy is kept with *Bring it back* before the file is written over (N4). |
| L107 | 4804 | A path keeps its `Writing` state until a read decides it, so a retry never asks a false question about the app's own bytes. |
| L108 | 4805 | The fold does not hold a member of an incomplete batch as applied, so `holds` is false and the book file is kept with *Bring it back* (N4). |
| L115 | 4812 | The case stays, through *Bring it back*: an earlier batch reconciled the same path and md5, then the journal lost a save the file shows. A `why: lost` entry is never ended by an older reconciling batch (X3), and *Bring it back* leaves out only the records that batch changed and the fold still holds applied and readable. The copy goes only after that. |
| L125 | 4822 | No second write begins before a read settles the first, so the app's own first bytes are never taken for a change made outside (a false copy and notice). |
| L127 | 4824 | An incomplete *Import* batch never marks its kept copy imported: `reconciles` counts only for folded, readable, applied changes. |
| L132 | 4829 | The old-bytes half applies only to a version that is still a head: its bytes are written and flushed at their new place first, and its blob is stored again from the file when missing (§2.14). A replaced version that is no head may go (CF 2). The listing half stays: a refused placement is listed with the blob of the bytes that are there. |
| L140 | 4837 | An import of a file edited back to a kept head's value, after a lost head, compares that record with the fold's heads. The edit is a revert, so a conflict, never silently dropped. |

**Rows unchanged in status whose action after detection changes.** L109, L113 and L118 still read
true as written. When the fold no longer holds what the bytes show, the action is now N4's copy
with *Bring it back*, not automatic recording.

**Rows whose status depends on H18** (counted as unchanged). L14 (4691), L46 (4728), L90 (4777),
L91 (4778). If H18 says unsupported: L14 changes to "a Door43 edit made with another tool is
reported once, and the version stays in Door43's history"; L46 and L90 retire (no walk); L91
retires for web edits (the whitespace rule stays in §13.6).

**Rows that depend on open question Q3** (counted as unchanged, under the recommendation that
S2's carried code stays minus the backups). L21 (4698), L29 (4706), L43 (4725), L44 (4726), L84
(4771), L93 (4780), L129 (4826), L131 (4828), L133 (4830), L137 (4834). If carried files are
parked until the feature, these become "pending the feature".

### 4.3 Added

| Row | Case | What prevents it | Test (to write) |
|---|---|---|---|
| L144 | A changed book file is lost or written over while the question is open | The copy is durable before the state names it, and a file whose copy cannot be made is left as it is (§2.4). The file is left as it is until *Import* or *Not now*. Another save keeps another copy with its own entry. The state and the copy survive a restart, and the question is asked again. | `a_changed_book_file_is_kept_and_left_until_the_answer`, `a_question_left_unanswered_is_asked_again_at_the_next_open`, `a_second_outside_save_while_asked_keeps_both_copies`, crash rows X1-X2 |
| L145 | *Import* overwrites a verse changed on both sides since the file left the project | Parents are the base heads; a moved head makes a sibling, so a conflict (U3) | `update_a_book_conflicts_a_verse_changed_on_both_sides` |
| L146 | The app's own interrupted write is taken for a change made outside (a false question or copy after a crash) | `Placed`, `Writing` old and new, and the projection are the app's own (§6.2) | `no_crash_of_the_apps_own_write_asks_a_question` (kill at every write point of M3-M7, with and without a power cut) |
| L147 | A changed `metadata.json` or sidecar is written over before its copy is durable, then a power cut | The copy and its folder are flushed before the manifest names it and before the rename | `an_outside_edit_of_metadata_json_is_copied_durably_before_it_is_rewritten` |
| L148 | Updating from a file that left the project long ago reverts newer work | Base from the kept entry or the ledger; with no base, every disagreement is a conflict and unchanged verses stay | `updating_from_an_old_export_reverts_nothing` |
| L149 | An import leaves alignments on words the verse no longer has, or drops alignments of unchanged words | U4: kept when the words are unchanged, reflowed in the same batch when they changed | `update_a_book_keeps_alignments_of_unchanged_words_and_reflows_changed_ones` |
| L150 | A crash after the import batch imports the copy twice | `reconciles` ends the entry (X3) | `an_import_interrupted_after_its_batch_is_not_imported_twice` |
| L151 | Removing an old carried version's bytes removes a version still in conflict, or a version that moves (a provisional change, *Keep both*) whose blob was lost | Only bytes of a version that is no head may go; a head's bytes are written at their new place, from the file if the blob is missing, before the old place is written over or removed (§2.14) | `an_old_carried_version_in_a_conflict_is_never_removed`, `a_conflict_side_whose_blob_was_lost_moves_to_its_conflict_copy_before_the_path_is_replaced`, `keep_both_with_a_lost_blob_keeps_the_version` |
| L152 (only if H18 = unsupported) | A Door43 edit made with another tool is replaced without a word | The intake reports it once, naming the commit and person | `a_door43_edit_by_another_tool_is_reported_once` |

### 4.4 Counts

| | Rows |
|---|---|
| Retired | 11 |
| Changed | 14 |
| Added | 8 (plus L152 if H18 says unsupported) |
| Unchanged | 118 (of them 4 depend on H18 and 10 on Q3) |
| Rows after the delta | 143 − 11 + 8 = 140 (141 with L152) |

The review of 2026-10-21 moved L112 from changed to retired and L115 from retired to changed, so the totals stand.

---

## 5. Tests (§16)

### 5.1 Delete

| Test | Where | Why |
|---|---|---|
| `an_outside_metadata_name_survives_a_power_cut_before_its_batch_is_durable` | `tests/store/placement_regressions.rs` | L141 retired; L147 replaces it |
| `metadata_json_put_back_to_an_earlier_outside_version_is_recorded_again` | same | L142 |
| `a_refused_metadata_recording_keeps_the_edit_or_its_copy_after_dismissal`, `a_refused_metadata_batch_is_sent_again_by_the_retry_not_at_once` | same | L143 |
| `an_outside_edit_of_metadata_json_is_sealed_and_committed_with_its_packs` | `tests/store/checkpoint.rs` | no recording |
| `a_disk_that_refuses_the_batch_of_an_outside_edit_is_tried_again_by_the_writer` | `tests/store/outside.rs` | no automatic batch |
| `undo_outside_edit_restores_and_keeps_history` | same | Undo is gone |
| `crash_outside_edit_recording` | same | replaced by the X1-X6 and X3a crash tests |
| `a_revert_made_while_the_app_is_closed_after_a_crash_past_the_rename_is_recorded`, `a_revert_the_open_could_not_read_is_recorded_by_the_first_compare_and_swap` | `tests/store/groups.rs` | L106; folded into L146's test, which asserts the opposite: no question, nothing lost |
| `a_restore_of_the_book_file_with_its_old_time_after_a_kill_past_its_rename_is_kept`, `a_restore_of_metadata_json_with_its_old_time_after_a_kill_past_its_rename_is_kept`, `a_revert_to_an_earlier_write_of_two_stopped_ones_is_kept` | `tests/store/model.rs:3387-3405` (named in §15 for L111, L117) | L111, L117; same reason |
| `a_revert_of_metadata_json_after_a_kill_past_its_rename_is_kept` | `tests/store/model.rs:3392-3395` (L112) | L112 retired; L146's test covers the opposite |
| `a_replaced_carried_file_recovers_its_lost_old_blob_before_overwrite`, `a_removed_carried_file_recovers_its_lost_blob_before_unlink`, `a_cached_carried_blob_is_recovered_after_its_pack_disappears`, `a_held_carried_replacement_backs_up_its_old_version_once`, `a_held_carried_replacement_reads_its_old_version_back_once`, `a_held_carried_removal_reads_its_old_version_back_once`, `every_reference_transition_preserves_the_last_copy`, `bounded_carried_histories_preserve_independent_old_bytes`, `a_carried_replacement_waits_when_the_old_backup_blob_is_corrupt`, `a_carried_removal_waits_when_its_backup_blob_is_corrupt` | `tests/store/carried.rs` | CF 2 |
| `a_carried_replacement_recovers_the_old_blob_lost_during_a_sharing_wait` | `tests/store/carried_last_look.rs` | CF 2 |
| `a_durable_loose_blob_read_flushes_only_its_backup_and_parent_names`, `a_durable_packed_blob_read_writes_a_pack_of_only_the_affected_blob`, `a_blob_backed_up_twice_keeps_its_open_backup_pack`, `a_second_durable_read_of_a_backed_up_blob_succeeds_without_repacking`, `a_backup_flush_covers_a_recreated_fanout_and_propagates_every_failure` | `crates/tc-store/src/git.rs` tests | CF 2 |
| `carried_superseded_version_survives_repack`, `carried_versions_recorded_before_a_crash_survive_the_repack_at_the_next_open` | `tests/store/repack.rs` | `refs/tc/carried` is gone |
| model step `Damage` | `tests/store/model.rs:151` | R10 is gone; outside REACH per verification |

### 5.2 Change

| Test | Change |
|---|---|
| `every_pair_of_the_table_keeps_every_byte`, `the_reviews_cases_are_kept` (`ledger.rs`) | the new table N1-N11; the hit list loses R9, R10, R11-R14 as separate rows, and `TakeIn` |
| `random_histories_lose_no_work` and `_long` (`model.rs`) | step `Choose { take_in }` becomes `Answer { import: bool }`; `EditName` expects a copy and a notice, never a save; `RevertKeepTime` of the app's own bytes expects no question; the oracle: "with no outside step, no question, no notice and no copy" (the R9 exception goes); a put of a book file is kept until the answer; after *Import* every verse the put showed is a version in History; the put rule ("every put is in the folder, is what the project would write, or is copied", `model.rs:20-22`) gains "or is bytes of a write the app had begun (`Writing.old`) whose records are all versions in History", since `Revert` and `RevertKeepTime` of those bytes are now written over |
| `loss_materializer_never_overwrites_outside_edit`, `loss_outside_mass_delete_is_conflict`, `loss_outside_revert_is_conflict`, `loss_unreadable_outside_edit_copy_kept`, `an_edit_made_while_the_app_was_closed_is_recorded_and_agrees_with_the_project`, `take_in_held_records_as_conflicts_and_ends_the_hold`, `an_edit_with_no_heads_file_is_an_unknown_base_conflict`, `a_heads_file_without_digests_still_records_an_outside_edit`, `an_outside_edit_found_without_a_manifest_uses_the_head_base`, `two_saves_from_a_program_that_keeps_the_file_open_are_no_conflict`, `an_edit_back_to_a_kept_head_after_a_lost_one_is_recorded`, `the_watcher_records_an_edit_without_a_focus`, `the_watcher_on_a_real_folder_records_an_edit`, `loss_local_book_file_added_projects` (`outside.rs`) | each asserts keep + question, then *Import* through §13.6, instead of automatic recording |
| `an_outside_edit_of_a_sidecar_records_decisions_and_ignores_flags`, `an_outside_edit_of_an_alignment_file_is_recorded`, `loss_metadata_unmodelled_fields_kept` (outside half) | copied and written again with one notice; nothing recorded |
| `loss_obs_template_edit_recorded` | becomes Update a story |
| `outside_edit_reconcile_conserves` (`outside.rs:1937`, §16.1 S2:4883), `every_kind_of_outside_edit_in_one_project_conserves` | become `update_a_book_conserves`: random files imported through §13.6, with and without a base; no head lost; reverts and removals are conflicts |
| `reconciling_an_outside_edit_costs_what_it_changed`, `a_watcher_event_for_three_changed_verses_costs_what_they_changed` (§16.5a S2:5025) | `updating_a_book_costs_what_it_changed`: one decode, the keys that differ, three changes, the same on both corpora |
| `a_racy_or_mtime_restored_file_is_never_overwritten` (`opcount.rs`, §16.5a S2:5012) | all three cases are kept and asked; *Import* and *Not now* in place of *Take in* and *Restore* |
| `loss_bit_rot_not_recorded_as_edit` (`loss.rs`) | a same-stat change of a book file is kept and asked; of another file, copied and written again |
| `crash_materialize_ledger_tells_its_bytes_from_an_outside_edit`, `crash_materialize_with_a_carried_file_and_a_held_file` (`crash.rs`) | no question and no copy after any crash of the app's own write |
| `an_outside_revert_after_a_stopped_group_is_recorded_as_a_conflict`, `an_outside_revert_of_metadata_json_after_a_stopped_group_is_an_edit`, `an_outside_edit_of_the_bytes_a_stopped_group_left_continues_them` (`groups.rs`) | kept and asked (book), copied (`metadata.json`); the base of the import is the stopped group's placement |
| `a_book_file_holding_a_save_its_pending_file_lost_in_place_is_recorded_at_the_next_open`, `a_book_file_holding_a_save_whose_pending_file_was_deleted_before_a_crash_is_recorded`, `a_book_file_whose_heads_were_put_back_is_recorded_not_written_over`, `metadata_json_holding_a_project_detail_whose_pending_file_was_deleted_is_recorded`, `a_commit_takes_a_file_waiting_for_an_outside_edit_from_the_snapshot` (`checkpoint.rs`) | N4: a copy with *Bring it back*; after it, the save is a readable version again; the commit takes the fold's projection of a held file |
| `unreadable_but_writable_metadata_keeps_the_outside_name_until_it_can_be_read`, `a_same_size_same_mtime_edit_of_settled_metadata_is_kept_before_replacement` (`placement_regressions.rs`) | the copy is made before the replacement; nothing recorded |
| `keep_both_gives_the_other_version_its_own_file` (`carried.rs`) | the `<stem>-<person>.<ext>` name |
| `arbitrary_meta_never_blocks_commit` (`burrito.rs`, §16.1) | the generator's "decoded outside edits" source stays only while §11.2 stands |
| §16.7 mutation table (S2:5058-5178) | drop the rows for: holding a clean-by-stat path; R9 for `old` bytes; `metadata.json` pending; the end of a version's recording; the copy kept on dismissal of an unrecorded version; the wait after a refused batch; the writer's own retry; `refs/tc/carried` (two rows); the D7 heads (becomes the heads of `Imported`). Keep: the compare-and-swap, the last look, the digest prefilter, the revert and removal rules of the planner, `holds`, settled entries, the scope digest |

S2's mutation recipes (44 at 3b075d7, in the git-ignored executor) must be sorted the same way.
The proof plan names some: L111 and the `metadata.json` recipes retire; KEPT_HEAD_EDIT moves to
the import planner; SEAL_UNREADABLE, INCOMPLETE_CACHE and REMOVAL_LAST_LOOK stay;
CARRIED_REFUSED_SETTLED stays with L132's listing half. I could not read the full recipe list,
because it lives only in the S2 worktree.

### 5.3 Add

- The tests named in §4.3 (L144-L151, and L152 if H18 says unsupported).
- `a_changed_book_file_is_asked_once_with_every_book_found_together`
- `not_now_keeps_the_project_text_and_lists_the_copy_with_import`
- `import_marks_changes_as_imported_and_writes_the_book_file_from_the_project`
- `a_book_file_for_a_book_the_project_lacks_is_asked_and_not_now_moves_it_out`
- `an_undecodable_change_is_kept_and_offers_only_not_now`
- `a_whitespace_only_change_asks_nothing_and_keeps_a_copy`
- `an_outside_edit_of_a_sidecar_is_copied_and_rewritten_with_one_notice`
- `a_read_only_open_asks_nothing_and_writes_nothing`
- `keep_this_one_removes_only_the_other_file`
- `not_now_with_the_book_file_held_open_writes_it_later_with_no_second_copy_or_notice` (N3a)
- `a_second_outside_save_of_a_held_file_keeps_the_first_in_the_notices`
- `a_full_disk_leaves_a_changed_file_as_it_is_until_its_copy_is_made`
- `a_no_base_import_never_becomes_the_provisional_text`
- crash tests for rows X1-X6 and X3a of §2.6, with `kill_everywhere` and power cuts
- model: a step `Answer { import }`; the oracle checks that a put of a book file is in the folder
  or in a kept copy until it is imported or dismissed

---

## 6. Effect on the remaining work

All sizes are **estimates**. Each is inferred from the card sizes in `tracker:roadmap/units/`.

| Card | Effect | Size effect (estimate) |
|---|---|---|
| **S2** | Stop proving and fixing outside-edit recording, the `metadata.json` recording (L141-L143), R9's `old` rule (L106, L111, L112, L117), R10 and the carried backups (L132-L138). A failing recipe or seed history in those areas no longer blocks the merge. Add no rows or fixes there. Finish: validation of what stays (the ledger's own-write rule, `holds` and N4, the journal files and seal, checkpoint and git, the crash rows W, K, M, G, the op counts, burrito validity), the merge with C1a's pins and C5's draft rule, and the executor in the repository. Merge the code as it is (ROADMAP: recipes are anchored to its lines). | 1-2 d becomes about 1-1.5 d |
| **S3** | Step 8 (C5): 4 of the 9 maps are deleted, not turned into ledger states. Step 9 is no longer optional. It becomes "apply the 2026-10-21 rulings": (a) delete automatic recording and the `metadata.json` recording; (b) delete R10 and `Who::Ambiguous`, and make `old` the app's own; (c) delete the carried backups (simpler than C9: no content-addressed store, no format change); (d) add the `kept` list, the held-waiting state, *Import*, *Not now* and *Bring it back* in the store, reusing the planner. | step 8 about −2 d; step 9 about +3 to +5 d; the whole card about −1 week against "all 2-3 weeks" (C9 is gone) |
| **X** | If H18 says unsupported: the walk (step 5 item 2) and its tests go, and so does §11.3's walk; add the one notice per commit. If merged: unchanged, and OE 7 cannot be told apart (§2.12). Either way, intake has no carried backups to keep. | −1 to −2 d if unsupported; about 0 if merged |
| **I** | Adds "Update a book from USFM / a story from Markdown" as a job of its own (§13.6, U1-U6). The planner and the store entry come from S3. | +1 to +2 d |
| **F1** | Adds the menu item and screen for Update a book: choose a file, choose the book, then a result report with counts and a link to the review. | +1 to +2 sessions |
| **D1** | The outside-edit toast with *Undo* is replaced by the question dialog (one message for several books, *Import* / *Not now*). Notices list kept copies with *Import* and *Dismiss*, and *Bring it back*. *Take in* and *Restore* of a held file go. The tC4 difference is recorded. | about the same (±0.5 session) |
| **C7a / C7b** | The grouped row of one import reuses `OutsideEditLarge`'s three choices. Carried choices shrink to *Keep this one* and *Keep both*, with a free valid name. | C7b about −0.5 session |
| **E2** | A receive shows §11.3's notices with *Import*. | +0 to +0.5 session |
| **V** | Pins the new manifest (`kept`, `held` without `damage`) and the `ImportSource::Markdown` variant. It no longer waits for a carried format change. It decides whether to drop `External { LocalFile }` (and, after H18, `Door43Edit` and `ExchangeEdit`) before the freeze. | about 0 |

Net effect on storage, an estimate: less code in tc-store (the four maps, the backups, the
recording paths) and a shorter S3. The new import work lands in I and F1. Not on the critical
path, the store side of the question adds to S3 step 9.

---

## 7. Open questions

All answered on 2026-10-22: H18 option B, H19 (Q3) option A, H20 (Q5) option A with no conflict for an empty verse, H21 (Q8) option A, H22: Q2 option A, Q4 `jon_01-Juan.wav`, Q6 the notice-only choice, Q7 as listed. The rulings are in `docs/rulings/2026-10-22-H18.md` to `-H22.md` and override anything below.

### H18. Edits made with Door43's own tools (gatewayEdit, tcCreate, the web editor)

| Option | Effect on a translator | Cost |
|---|---|---|
| A. Merged (today's §11.2) | A web fix appears in the project as that person's change; overlaps are conflicts. A folder pushed by another tool is merged the same way, against OE 7, because the two cannot be told apart. | the walk in X (about 1-2 d) plus its tests; decoders for every derived file stay |
| B. Unsupported, with a notice (the roadmap's recommendation) | The sync keeps the project's text. The app says once who changed what, and that the version is in Door43's history. To bring it in: download the file and use Update a book. | smallest; one check at intake |
| C. Unsupported, with *Import* offered | As B, and a changed book file from that commit is kept as a copy with *Import* (§13.6, base = that commit's pack-set heads). Nothing from the web editor is lost or needs a download. | B plus about 0.5 d (inference) |

Recommendation: **B**, or **C** if the owner expects teams to edit book files on Door43. Both make
OE 7 and H18 follow one rule. A cannot honor OE 7.

### Q2. Work the journal lost, found only in the app's own file (row N4)

A save whose journal file another program cut or deleted can survive only in a derived file the
app wrote (L105, L108, L109, L113, L118, L139, L140). Verification says these cases are within
REACH. The rulings do not mention them.

| Option | Effect | Cost |
|---|---|---|
| A. Copy and notice with *Bring it back* for every derived file (this delta) | The person clicks once; the work comes back as an import | keeps the decoders for every file kind |
| B. *Bring it back* only for book files; other files: copy only | A lost check decision or project detail stays only in a copy, on no screen | slightly less code |
| C. Bring it back on its own (today's R4) | No click | the automatic path stays for one case; against the spirit of OE 1 |

Recommendation: **A**.

### Q3. Carried files until translationCore uses them

CF 4 says carried files "are built when translationCore adds the feature". S2 has already built
them (`crates/tc-store/src/carried.rs`, 780 lines, plus parts of `git.rs` and `checkpoint.rs`).
OBS projects use them today for `LICENSE.md` and the front and back matter (S2:1011-1013). Extra
files on Door43 rely on them too (L21).

| Option | Effect | Cost |
|---|---|---|
| A. Keep S2's carried code as the interim, minus the backups (CF 2), with CF 3's two choices | Extra files and OBS front matter travel and conflict visibly now | the deletions only; C7b offers the two choices |
| B. Park it: no carried executor; files the app does not derive are kept verbatim like foreign entries | A teammate's change to such a file is not merged and may not reach Door43 | a new verbatim path for OBS files; S2 code removed |

Recommendation: **A**. Ask the owner, because CF 4's words suggest B.

### Q4. The name of the second file under *Keep both*

The ruling's example `jon_01 (Juan).wav` has a space, and the path grammar forbids it (S2:1024).
Options: `jon_01-Juan.wav` (recommended), `jon_01_Juan.wav`, or `jon_01.Juan.wav`. The effect is
only what the translator sees in the folder.

### Q5. Update a book with no known base

A file chosen from anywhere, not byte-equal to a version the app wrote.

| Option | Effect | Risk |
|---|---|---|
| A. Every change is a conflict unless equal; values already in History import nothing; one grouped row (this delta) | One review row with *Accept all from the file*. The project's text stays shown until then (`unknown_base`, §2.15) | every verse edited in the file, not only a revert, waits for *Accept all*: the supported flow of OE 5 always ends in the review |
| B. Treat the project's current heads as the base | No conflicts at all | a teammate's newer verse is replaced without a word (it stays in History) |
| C. Ask which checkpoint the file came from, and use that commit's heads | Exact | one more step on the screen; F1 is larger |

Recommendation: **A**.

### Q6. §11.3, a changed file inside an exchange file

The rulings do not name it. This delta reads OE 1 and OE 3 as: keep a changed book file as a copy
with *Import*, and ignore other files with a notice. The other choice is to keep today's walk, if
H18 says merged. A third, simpler choice: treat it as OE 7 treats a folder pushed by another
tool. An export takes every derived file from HEAD's tree, which is the fold's projection
(§10.1 E2, §9.4), so a difference means someone edited the zip by hand. The app keeps its own
text and says once "Juan's file holds a changed Titus that was not made in translationCore. It
was not taken in; the file on the stick still has it." Nothing is lost, since the zip is never
changed, and no `copy` base or kept entry is needed. Recommendation: the delta's reading, or
the third choice if the owner prefers fewer mechanisms. Ask with H18.

### Q7. Small choices made here (tell the owner)

- A change of line ends only asks nothing. It keeps a copy and shows the existing notice.
- A file that cannot be read asks with *Not now* only.
- The question is raised at once when the app finds the change while it has focus, not only at
  open and focus.
- *Undo* after an import is not offered. History keeps every version.
- A read-only open keeps no copy and asks nothing; it shows a notice.
- A book file for a book the project lacks is moved out of `ingredients/` on *Not now* (its copy
  is kept). The project then has no file of that book, which is what "writes the book file from"
  the project gives.
- A full disk leaves a changed file as it is until a copy can be made (§2.4).

### Q8. Are alignment and check-decision files "other generated files"?

OE 4 names "`metadata.json`, settings files and the like". This delta counts the alignment and
decision sidecars among them (§2.11), so a hand edit of an alignment file, or check decisions
made in it by another program on the same folder, are copied and written over with a one-line
notice, never imported. The translator's alignment or decision work then lives only in
`.tc/external/`, on no screen, and dismissing the notice deletes the copy.

| Option | Effect | Cost |
|---|---|---|
| A. Sidecars are other generated files (this delta) | One rule for every file but book files; the supported way to bring alignments in is an aligned USFM file through Update a book (U4) | none |
| B. Sidecars are kept and asked like book files, with *Import* through their decoders | A decision or alignment made elsewhere can come in with one click | keeps the sidecar decoders and an import planner per kind |

Recommendation: **A**, because a supported path for alignments exists (aligned USFM) and
decisions have no outside editor. Ask the owner, because the ruling's words do not settle it.

---

## 8. Not decided by these rulings

SQLite as the local store was discussed but not decided. It is a separate option, not part of this
delta.

---

## Review (2026-10-21)

An adversarial review against the two rulings, the S2 copy of STORAGE.md and the code on
`s2-proof-executor` at 3b075d7. Each finding names its severity and what was changed above.

### Findings

| # | Severity | Finding | Changed |
|---|---|---|---|
| R1 | blocker | §2.14 removed the "durable before destroyed" rule of **Placing** for every carried file, not only for replaced versions. A version that is still a head can have its only bytes in the file: a local recording whose loose blob a power cut lost (nothing flushes it), then a teammate's version from the same base arrives by sync and becomes provisional. The group cannot write the `.conflict-` copy, and replacing the path destroys the recording. *Keep both* has the same hole. CF 2 lets old bytes go only when "nobody else changed that file". | §2.14 keeps a narrower rule: a head's bytes are written and flushed at their new place first, and its blob is stored again from the file when missing. No git flushes or backup packs come back. Crash row X6, rows L132 and L151, §3 and §5.3 follow. |
| R2 | major | "Another save of a held file" moved the `kept` entry to the newest copy. The earlier copy then had no notice and no *Import*, so an outside save dropped off every screen. OE 3.4 keeps each copy "until it is imported or dismissed". The second save need not continue the first (a cloud client putting back an older file). | §2.3 and §2.11: every copy has its own entry; the question imports the newest; earlier copies stay in the notices. |
| R3 | major | X3 ended any entry whose path and md5 a durable batch reconciles. That brings L115's loss back: an import batch reconciled bytes `d`; later the journal lost a save the file shows; N4 makes a `why: lost` entry for the same `d`; X3 ends it at the next open, and the copy (the last trace of the save) goes. L115 was wrongly retired. | X3 now ends only `why: changed` entries; *Bring it back* uses L115's per-record rule. L115 moved from retired to changed. |
| R4 | major | No rule said what happens when the copy cannot be made (full disk, a held folder, a read-only project). *Keep and write* said the path "is written in this same group", which an implementer could follow after a failed copy. | §2.4: the file is left as it is and the path waits; only a durable copy counts (L130). L144 and §5.3 follow. |
| R5 | major | `NotNow` makes `Placed{h}` (as `Restore` does, `ledger.rs:584`), and no heads file names `h`, so `holds` is none. A rewrite refused because Paratext still holds the file (common on Windows: `WriteFailed` gives `Placed{h}` again), or a crash before the rewrite, sent the person's given-up bytes to N4: a second copy and a false "a file of saved changes was damaged" notice. X4 claimed N8. | New row N3a and its reason in §2.5; X4 corrected. |
| R6 | major | The import's changes would rank as provisional. `CauseClass::rank` (`fold/meta.rs:292-297`) puts only `External { unknown_base: true }` last; a no-parents `Import(Usfm)` change, being the newest, would win. The book file and screens would show an old file's text over a teammate's newer verse until the review. That regresses F7 and goes against L148's purpose. | §2.15 adds an `unknown_base` mark to the import cause (before the freeze); U3 says every no-parents import change carries it. Test in §5.3. |
| R7 | major | L112 is the `metadata.json` twin of L106. Its case ("put back by another program while closed: taken for the app's own and written over") is now the intended behavior, since `old` is the app's own. The delta "changed" it with wording that restates the rule, and left its test, which asserts the opposite. The model's put rule (`model.rs:20-22`) would also report every such overwrite as a loss. | L112 retired; its test added to §5.1; the model's oracle change added to §5.2; §2.5 and §6 name L112. |
| R8 | minor | I13 said no file's content enters without a person's choice, but N11 records a carried file's new version on its own. | I13 limited to derived files, with the reason. |
| R9 | minor | The `kept` entry's `base` had no value for §2.13's zip heads; a base whose heads are gone was undefined. | `base: copy` added; missing heads count as `none` (§2.3, U2, §2.13). |
| R10 | minor | U2 b was dead: a chosen file byte-equal to a placement compares equal to its own base and imports nothing. Q5 understated option A: in the supported flow of OE 5 every edited verse is a conflict, not only a revert. | U2 b removed; the base paragraph and Q5 say what a translator meets. |
| R11 | minor | The code keeps no heads while a path is `Held` (`put_heads`, `materialize.rs:1370-1374`), so the kept base would be pruned unless the keep list changes. | Pointer added in §2.3. |
| R12 | minor | The ledger paragraph kept in §6.2 still has R9's "second write of `metadata.json`" sentence. | Added to what goes (§2.5). |
| R13 | minor | Wording: L105 said *Import* for a book file where §2.11 says *Bring it back*; *Keep and write* referred to the `Held` step; a deletion of a held file did not say the copy stays. | Fixed in place. |
| R14 | minor | Q7 left out three choices made here: the read-only open, *Not now* moving a file of a book the project lacks, and the full-disk wait. Sidecars as "other generated files" was an interpretation with no open question. | Added to Q7; new Q8. |
| R15 | minor | Simplicity, not changed. (a) The file watcher on derived files is not needed under OE 3: open, focus, M1 before any write and the storage check already look; the delta keeps it as "optional". Recommend dropping it. (b) Q2's option A keeps a decoder for every sidecar kind only for journal loss. (c) §2.13's kept copy for a hand-edited zip is a mechanism for a case an export cannot produce (exports take HEAD's tree, §10.1 E2); Q6 now offers the simpler OE 7-style notice. | Q6 extended; (a) and (b) left to S3 and the owner. |
| R16 | minor | Not changed. After *Import* with Paratext still open, a save of its stale buffer is kept and asked again; a second *Import* of it can settle a conflict to the file's value, because the two heads become equal. It is the person's choice, not silent. | none |
| R17 | minor | Not changed. Short paths `tests/store/model.rs` are `crates/tc-store/tests/store/model.rs`; `model.rs:151` is the doc line of `Damage` (152). | none |

### Question 3: is it safe to count `Writing.old` as the app's own?

Decision: **safe**, with N3a. Evidence: `old` is bytes the app itself wrote from the record; while
the state is `Writing`, its heads stay in the heads file (`put_heads` keeps `old` and `new`,
`materialize.rs:1372`), so `holds` can be asked of them, and N4 keeps a copy whenever the journal
no longer holds what they show. The scenarios tried: (1) Paratext had loaded `old` before the
app's rename and saves its unchanged buffer after a crash: no edit exists, and writing over it
is right. (2) A cloud client or backup tool puts `old` back on purpose: the text is a version in
History, and only the intent to revert is not followed, in the crash window only. (3) A person
types a file equal to `old`: every verse is then a version in History. (4) `old` is outside bytes
the person gave up with *Not now* (no heads): N4 would raise a false notice, which N3a now
prevents. No scenario found loses a person's text. Tell the owner the one visible effect: after a
crash, a backup tool's restore of the version just before the app's last write is written over
without a question, and that version stays in History.

### Spot-check of §4 and the code

The line numbers of 29 rows (L14, L41, L42, L46, L67, L73, L83, L90-L92, L105-L108, L111, L112,
L115, L117, L125, L127, L132, L134-L136, L138, L140-L143), of the ten Q3 rows and of L109, L113
and L118 match the S2 copy. The code references checked and right: `ledger.rs:355`
(`damage`), `:358` (`Who::Ambiguous`), `:584` (`Restore`); `classify.rs:159`, `:185`;
`ids.rs:134`, `:137`; `record.rs:493`; `change.rs:21`, `:46`, `:77`; `pending.rs:61`;
`command.rs:119`; `commands.rs:206`; `git.rs:34`, `:174`, `:209`, `:675`, `:972`;
`carried.rs:428`; `checkpoint.rs:173-191`; `fold.rs:671`; `handle.rs:824`, `:848`;
`focus.rs:61`, `:285`; `writer.rs:499`; the nine Writer maps (S3 step 8);
`tc-journal/src/outside.rs:676`; `tc-store/src/outside.rs:314`, `:344`, `:711`. L134 outside
REACH is confirmed by `tracker:roadmap/evidence/2026-10-21/verification.md:169`. Two rows were
wrong, L112 and L115 (R7 and R3 above); the totals stand at 11 retired, 14 changed, 8 added.

### For the owner

- Q5: with no base, the supported flow of OE 5 (edit, Update a book, sync) always ends in the
  conflict review, with *Accept all from the file*. Is that acceptable, or should the app ask
  which checkpoint the file came from (option C)?
- Q8: are alignment and check-decision files "other generated files"?
- Q3: CF 4 says carried files are built later; the delta keeps S2's carried code now.
- H18 and Q6, as written.
- The `Writing.old` effect above, as a declared behavior.
