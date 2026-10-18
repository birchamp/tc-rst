# tC4 sync and journal design: how it works, what PR #688 adds, and how translator work can be lost

Research for the Rust rebuild (multi-user offline collaboration over Door43 and USB).

- Repository: `/home/user/unfoldingword/translationcore4` (shallow clone).
- PR #688 branch: local ref `pr688`, HEAD `70d32cb` (2026-10-14).
- The clone is shallow, so `git diff main...pr688` has no merge base. The PR's own changes are
  `git diff 1dbe423 pr688`: `1dbe423` is the `origin/main` commit that the PR's last merge (`94a9fd9`)
  brought in. That diff touches 35 files, +3009 −73. `main` HEAD `d494a9d` is one PR (#709) newer and does
  not touch sync files.
- I could not read the GitHub review threads of PR #688, because GitHub access to
  `unfoldingWord/translationcore4` is not enabled in this session. The PR's open items and reviewer findings
  below come from the branch code at `70d32cb`, its commit messages (review rounds 1–7, bench rounds 1–6),
  `docs/DECISIONS.md` D98/D101, `docs/RISKS.md` rows 11–13, and the item list in the task.
- I ran six experiments on a copy of the reference fold (`journal/*.mjs` at `pr688`). They confirm the
  structural-conflict losses in §8. The script is
  `/tmp/claude-0/-home-user-tc-rst/4a142024-63e1-59b8-bcc4-3b15bbfdf386/scratchpad/scripts/concurrency.mjs`, and
  §13 has its output.

---

## 0. Summary

**The design.**

- **Storage is event-sourced, with one writer per directory.** Each save becomes one immutable, sha256-sealed
  JSON "action segment" file. The file sits under `ingredients/checking/journal/<actorId>/segments/` and is
  named by its first event's hybrid-logical-clock timestamp (`ts`). No two devices write the same file, so a
  union of journals never conflicts at the file level.
- **The fold** (`journal/fold.mjs`) is a pure, deterministic function of the event *set*:
  - de-duplicate by `ts`, then sort by `ts`;
  - each key is a last-writer-wins (LWW) register with `base`-pointer fork detection;
  - structural actions (book add, renumber, verse spans) form a separate chain with branch selection;
  - "book generations" quarantine records from a removed-then-re-added book;
  - the fold projects USFM and sidecar files, and reports `forks`, `retained`, `pendingStructural`,
    `invalid` and `autoMerged`.
- **Derived files are committed.** The USFM, the alignment, decision, resources and settings sidecars, and
  `vrs.json` are rebuilt from the fold at every save and checkpoint. The repository therefore stays a plain
  Scripture Burrito.
- **Sync round 1 (D98) is "Door43 main only, one button".**
  - The sync core (#675, `journal/sync.mjs`, merged on main) is a pure function. It takes the Door43 main
    files plus the local journal and outbox. It returns the files to write, the files to remove, and a
    Report.
  - The sync shell (#676, PR #688, `src/data/share/syncOperation.ts`):
    1. full-clones Door43 main into a scratch repository;
    2. runs the core, then writes, commits and verifies;
    3. pushes as a fast-forward, with up to 3 tries and never a force push;
    4. applies the pushed tree to the live project in place (D101).
  - Crash recovery uses a marker file in the scratch copy. Recovery only goes forward.

**Conflicts.**

- The same verse edited two ways is a fork. The newest head projects, flagged provisional, and the verse
  conflict review (#679) shows it to a person.
- Every other fork (alignment, check result, pin, setting, structural head) keeps the newest head silently.
  The sync Report only counts it (G7, D98 point 8).

**The main finding.** The guarantees are strong at the file layer: G1 (no own file lost), G2 (incoming
files checked), G4 (never a force push). They are weak at the meaning layer. The experiments show that
ordinary use can hide a translator's work from the projected text and from every screen:

- two translators each create a verse bridge in the same book;
- one creates a bridge while another edits any verse of that book;
- one merges two verses while another edits one of them.

The data stays in the journal, so this is "conserved" by spec rule R-8.6.2. But no person-facing surface
shows `retained[]` or `pendingStructural[]`, so to the translator the work is lost. Other loss paths are in
§10: whole-fold "poison" refusals, clock skew deciding newest-wins, stale-buffer whole-book writes, remote
edits overwritten without a report, an installation secret shared by two machines, and non-atomic platform
writes.

---

## 1. Layers and files

| Layer | What | Where (`main` unless noted) |
|---|---|---|
| Format and rules | BURRITO-SPEC §8 (journal), §8.7 (checkpoint and round-1 sync), Appendix A (journal conformance JC-1…JC-35) | `docs/BURRITO-SPEC.md` 1.23 |
| Reference implementation (pure; the browser bundle imports it too) | grammar, schema, HLC, files, skeleton codec, fold, checkpoint projections, reconcile/seed, the sync core, the Report and refusal codes | `journal/*.mjs` |
| App journal write path | `JournalStore` (seal → stage outbox → HTTP write), identity, fold runner and worker, `JournalingStore` (the store the UI uses: publish, regenerate derived files, the open classifier, checkpoint), verifier | `src/data/journal/*.ts` |
| Save pipeline | `SaveScheduler`: dirty buffer, 2 s debounce, whole-book writes | `src/data/saveScheduler.ts` |
| Checkpoint commit message | `checkpointMessage` | `src/data/checkpoint.ts` |
| First share | `share()`: checkpoint, create repository, add `origin`, push | `src/data/share/shareOperation.ts` |
| Sync shell (PR #688) | `sync()`, `cloneProject()`, `withClonePath()` | `src/data/share/syncOperation.ts` (new) |
| Ops log and apply (PR #688) | ops records in client settings, `applyScratch`, `finishPendingApply`, apply marker, sync generation | `src/data/journal/opsLog.ts` (+353 lines) |
| Platform | pankosmia-web 0.18.15 (`a83725b`), a Rust/actix HTTP server; every disk and git operation goes through it | `src/data/serverApi.ts` |
| Tests | the sync core (`test/sync/syncCore.test.ts`), the shell with a fake rig and kill sweeps (`test/sync/syncOperation.test.ts`, 1186 lines, PR), the live rig (`test/sync/syncRig.integration.test.ts`, PR), journal store suites, conformance `validate-journal.mjs` (368 checks), `validate-transport.mjs` (T1–T4, the long-term plan) | |
| Evidence | Automerge proof, fold benchmark, fold worker, Pankosmia sync model, transport spike | `docs/evidence/*` |

---

## 2. The journal write side

### 2.1 Files and layout (§8.1)

```
ingredients/checking/journal/<actorId>/actor.json
ingredients/checking/journal/<actorId>/segments/<encoded-first-ts>.action.json
```

**Segment container (R-8.1.1).**

- The file is `{"container":1,"body":"<string>","sha256":"<hex>"}`.
- `body` is `JSON.stringify({events:[…]})`, serialized once. `sha256` covers the UTF-8 bytes of the body
  string.
- One file holds one user action: one or more events, in strictly ascending `ts`, all from the directory's
  actor. Validation fails on `ts-order` otherwise.
- An action may mix project-scope and book-scope events.

**Validity is the commit marker (R-8.1.6).**

- The checks run in order: parse the outer JSON, check `container === 1`, check the checksum, parse the
  body, validate the schema of every event, check the actor binding, check that the filename equals the
  first event's `ts`.
- A segment that fails any check is invisible as a whole. It is never partly folded.
- Readers must not drop it silently (R-8.1.7): an open refuses on any invalid or misnamed segment
  (`segment.invalid`, `segment.misnamed`).

**Other limits and rules.**

- **Size:** 4 MiB per segment (R-8.1.9). The writer refuses to seal an action over that size.
- **Filename (R-8.1.2):** the first event's `ts` with `:` → `_` and `|` → `,`, plus `.action.json`. Example:
  `2026-10-13T10_00_00.000Z,0000,translator-a.action.json`. The encoding is injective over the `ts`
  grammar, and the filename order equals the `ts` order.
- **One stream form (R-8.1.10):** a `.jsonl` file or any other file under `journal/` is invalid.
- **`actor.json` (R-8.1.13):** `{schemaVersion:1, actorId, displayName?, device?, createdAt}`. `actorId` must
  match the directory name. The app provisions it with `device: "translation device"` and no name.
- **Disjoint writers (R-8.1.16):** a device writes only under its own actor directory. This is the whole
  merge guarantee for journal files.

### 2.2 Append-only rules

- Every mutation is a **new** segment (R-8.1.4). Nothing appends to or rewrites a journal file.
- **Immutability branches** (`files.mjs writeActionSegment`; `journalStore.ts publish`):
  - path free → write;
  - the same bytes are already there → idempotent accept (R-8.1.5);
  - a different valid segment is there → refuse (`segment.differs-from-accepted`);
  - a different invalid segment is there → refuse; only a replay from the outbox may replace it.
- **No compaction, no pruning, no retraction (R-8.1.15).** Journals are permanent. There is no tombstone
  op, and notes are grow-only.
- **Containment.** In the reference code: lstat, realpath and `O_NOFOLLOW` refuse symlinks. The schema
  refuses a `ts` or path that could escape, and the checkpoint resolves each projected path inside
  `ingredients/` (R-8.7.6).

### 2.3 Identity (D50, D53(c); `src/data/journal/identity.ts`)

- **Installation secret:** 32 random bytes, hex-encoded. It is created once with `setIfAbsent` in
  IndexedDB (database `tc4-installation`, store `kv`, key `installation-secret`). It is never exported
  (D67(4c): "a new device is a new actor").
- **Actor id:** `'a' + hex(HMAC-SHA256(secret, repoPath))[0:15]`, so 16 characters, which fits the slug
  grammar `[a-z0-9-]{4,32}`.
  - The id depends on the project path. The same installation has a different actor in every project, and
    a moved or renamed project gets a new actor.
  - G5 ("same path, same actor after a sync") is the reason the sync applies in place and does not move the
    project.
- **Door43 username (`actor.name.set`, `v: 3`, R-8.5.25/26, D98).**
  - Each actor records its Door43 username once. The first event by `ts` binds; the fold reports later ones
    in `namesRejected`.
  - The writer refuses a second name (`actor.name-already-set`).
  - The sync seals the name automatically on the first sync. It is written into public, permanent history.
- **No signatures.** The sha256 detects corruption only. Anyone with write access to the Door43
  repository can add a segment under any actor directory that does not yet hold that filename. The
  Automerge proof used Ed25519 actor manifests; the production journal has none.

### 2.4 Clock and event identity (§8.2; `journal/hlc.mjs`)

- **Format:** `ts = "<ISO-8601 UTC, fixed-width ms>|<4-hex counter>|<actorId>"`. A plain string compare is
  the total order.
- **`issue()`:**
  - if `now > lastPhysical`, the counter resets to 0;
  - otherwise the counter increments;
  - past `ffff`, the physical part goes up 1 ms and the counter resets.
- **`ratchet(ts)`:** on any received event, `lastPhysical = max(...)` and the counter is carried. The app
  clock is shared per (repoPath, actor) in the process (`journalStore.ts clockFor`). It ratchets past every
  segment of every actor and every outbox entry at open. With `ratchet:'deferred'`, minting a `ts` before the
  union read is refused (`journal.clock-not-ratcheted`).
- **Event identity is `ts` (R-8.2.5).** The union de-duplicates by `ts`. Two *different* events with one
  `ts` make the whole fold throw `corrupt union`.
- **There is no bound on clock skew.** One device with a fast clock pulls everyone's clock forward
  (experiment E6: one teammate 30 days ahead → A's next `ts` is `2026-11-13T10:00:00.000Z|0001`) and wins
  every non-text fork (§6).

### 2.5 Event envelope and operations (§8.3, §8.5)

**Envelope fields.**

| Field | Meaning |
|---|---|
| `v` | 1, 2 or 3. Any other value fails the fold. v4.0.0 builds accept only 1 and 2 (RISKS #10). |
| `op` | the operation |
| `actor` | the actor id |
| `ts` | the HLC timestamp |
| `base` | the `ts` of the head this op observed on its key, or `null` ("no prior state *I know of*", D53) |
| `generation` | the `ts` of the `book.add` this record belongs to. Required on `align.verse.set`, `check.decision.set` and `note.add`; optional on `text.verse.set`. |
| `supersedes[]` | fork resolution: the heads this event replaces |
| `batch` | the first `ts` of one user action |
| `seed` | migration or import provenance |

**Operations and their fold rules.**

| op | key | fold rule |
|---|---|---|
| `text.verse.set` | `text\|BOOK\|C:V` | LWW register with fork detection. Never rootless on a live key (R-8.5.15; a rootless write is retained `rootless-base`). |
| `book.add` | `book\|BOOK`, `skel\|BOOK`, plus a head on every slot | self-contained creation (skeleton plus initial verses). A rootless add with an identical payload *converges* (alias, D53d); a different payload forks. |
| `book.remove` | `book\|BOOK` | a chain link; the book then projects nothing (R-8.6.5) |
| `text.skeleton.set` | `skel\|BOOK` | must keep the slot set; a slot change makes the **whole fold throw** |
| `text.structure.apply` | `skel\|BOOK`, plus a post-image head on **every** destination slot | all-or-nothing: transitions must map every destination slot (R-8.5.18), and dispositions must cover exactly the affected set (R-8.5.16). A missing source pends `incomplete`; a stale source pends `conflicted`. |
| `align.verse.set` | `align\|BOOK\|C:V` | LWW with fork detection; valid only while `targetVerseMd5` equals the md5 of the plain text of the folded verse (I-3) |
| `check.decision.set` | `dec\|tool\|checkId\|book\|ch\|v\|occ` | LWW with fork detection; a decision is never deleted (invalidate and retain) |
| `note.add` | none | grow-only, permanent |
| `resource.pin.set` | `pin\|slot` | LWW with fork detection |
| `project.vrs.set` | the project | first value binds; the event must be a creation or seed event |
| `project.meta.set` | `meta\|path` | LWW per dotted path. Projects nothing in practice: a checkpoint or sync refuses any overlay (no route writes `metadata.json`). |
| `settings.set` | `set\|path` | LWW per dotted path; a prefix collision is reported `prefix-collision` |
| `text.frame.set`, `text.story.ref.set` (`v: 2`, OBS) | frame or story register | like pins; the sync refuses OBS projects |
| `actor.name.set` (`v: 3`) | the actor itself | first-value register per actor |

**Write-side duties.**

- Normalize content to NFC once, at the seal (I-4).
- Refuse non-NFC identity values (R-8.5.13).
- Refuse `U+0001` (the slot delimiter) and any embedded `\v` or `\c` in verse content (R-8.4.3).
- Limit JSON depth to 64.

### 2.6 How an app save becomes events (`JournalingStore`)

1. The UI edits verses in `SaveScheduler`. The scheduler holds an in-memory whole-book string per book and
   flushes on blur or after a 2 s debounce. It calls
   `writeBookOrStructure → store.writeBook(book, wholeUsfm)`, or `applyStructuralEdit` when a section save
   created or broke a verse span (#63).
2. `writeBook` runs inside the per-project queue:
   1. Replay the outbox (`replayOwnStagedBeforeDiff`).
   2. Decompose the incoming whole book.
   3. Refuse a change to the slot set.
   4. Emit `text.skeleton.set` when the skeleton changed.
   5. Emit one `text.verse.set` per verse whose text differs from the current fold projection. Each takes
      `base: foldOut.headsTs['text|BOOK|C:V']`.

   **Important:** `base` is the store's head *at write time*, not the head the editor showed. Book writes
   pass no `expectMd5` (`state.jsx:1582`). See failure mode F5.
3. `publishAndRegenerate(events, affectedPaths, resolutions?)`:
   1. Pre-validate the seal.
   2. Append an immutable **intent-ledger** record to IndexedDB:
      `intent:<repo>:<actor>:<ts> = {ts, kind, affectedPaths, resolutions}`.
   3. Publish: seal → `kv.setIfAbsent(outbox:<repo>:<actor>:<ts>, sealedBytes)`, which is the durable
      intent and resolves on IndexedDB `oncomplete` → under a per-path lock, read the path and apply the
      immutability branches → `POST /burrito/ingredient/raw?no_bak` (no `update_ingredients`) → delete the
      outbox key.
   4. Add the events to the in-memory set and fold again in the Web Worker (a full re-fold, not
      incremental).
   5. Install each affected derived file with `update_ingredients`, read it back, and compare.
   6. Retry any other live intent's paths that have not converged.
   7. Prune intents whose convergence is proven.
4. **Checkpoint** at mode switch, project leave or share (D9):
   1. Fold, then refuse a `project.meta.set` overlay.
   2. Build `derivedProjections` (refuses missing inputs).
   3. Refuse if any derived file differs from the fold and the ledger does not explain it
      (`checkpoint.divergence`).
   4. Write, then `remake-ingredients`.
   5. Check that `currentScope` equals the fold's scope.
   6. `add-and-commit`.

   The commit sweeps the whole tree. The server rewrites `metadata.json` in place, which is not atomic
   (PLATFORM-NOTES #50, RISKS #13).
5. **Open** (`recoverAndConverge`):
   1. Replay the outbox with its exact bytes.
   2. Read the whole union (one zipped read of `checking/journal`). Refuse on any invalid segment.
   3. Classify the intent ledger.
   4. If there is no journal, seed it: every sidecar record becomes a seeded event, all or nothing (§8.8).
   5. Otherwise fold and compare the derived files on disk with the projection:
      - `converged`;
      - `regenerated-forward` (the ledger or a journal prefix explains the difference);
      - `reconciled` (only out-of-band USFM changes: journal them as `out-of-band-usfm` events, or as one
        conservative `text.structure.apply`);
      - an unexplained difference → refuse (`open.unexplained-divergence`).

**Crash posture (R-8.1.8).** The journal states it as: "process-crash atomicity is guaranteed; power-loss
durability is detected but not guaranteed".

- The outbox (IndexedDB) and the ingredient POST are separate stores, joined by replay.
- The platform's file writes have no fsync and no temp-file-then-rename (PLATFORM-NOTES #50).
- The repository's claims do not cover the durability of IndexedDB itself. Chromium has a "relaxed"
  durability mode, and its default should be verified for the Electronite version that ships.

---

## 3. The fold (`journal/fold.mjs`, §8.6)

### 3.1 Algorithm

1. **Validate** each event with the one schema. **Any schema error makes the whole fold throw.**
2. **De-duplicate** by `ts`. The same `ts` with different content is "corrupt union" and the **whole fold
   throws**.
3. **Sort** by `ts`, as strings.
4. **Walk** the events in order and keep `heads: Map<key, Head[]>`, where a head is
   `{ts, actor, sanc, book, event}` and `sanc` is the structural anchor.

**`joinHead(key, head, base, supersedes, actor)` (lines ~337–363).**

1. No live heads → `[head]`.
2. `supersedes` names *every* live head (after the ancestry check) → `[head]` (a resolution). Each erased
   head is reported `retained:superseded`. A supersede that reaches across structural ancestry is refused
   (`supersedeRefused`).
3. Some live head's `ts` equals `base` → replace that head (a linear continuation).
4. Some live head is by the **same actor** → replace that actor's heads (same-actor events never fork,
   R-8.3.3).
5. Otherwise add a head: this is a **fork**.

**Structural ops** keep their own rules:

- A chain link with an unknown base pends.
- A base that names the wrong kind of event, or another book's event, makes the **whole fold throw**.
- A base that is present but not accepted pends, and the pending state propagates.
- A rootless `book.remove`, `text.skeleton.set` or `text.structure.apply` is retained
  `rootless-structural`.
- A `text.skeleton.set` that changes the slot set **throws**.
- A stale base on the same actor's own skeleton head **throws**.
- A disposition outside the affected set **throws**.
- `text.structure.apply` steps:
  1. Check that every source and disposition reference is live; otherwise it pends `incomplete` or
     `conflicted`.
  2. Compute the affected set, using span membership R-8.5.24.
  3. Every affected record needs exactly one disposition; otherwise it pends `incomplete`.
  4. Check for collisions; a collision pends `conflicted`.
  5. Apply: join the skeleton head, *consume* the source heads on this branch, and push a post-image text
     head on **every new slot**. Pre-images stay live for other branches.

**Generations.**

- `align`, `dec` and `note` carry `generation` (the `book.add` `ts`).
- At projection, a head whose generation is not the selected book's root is retained `prior-generation`.
- A text head's generation is its base chain. This is causal and never decided by the clock.

### 3.2 Projection

For each book:

1. The winning `book|` head must be a `book.add`.
2. The winning `skel|` head is resolved by plain LWW (no ancestry filter). Its structural ancestry,
   `chainOf(skelHead.sanc)`, is the **selected structural branch**.
3. For each slot, resolve the `text|` key. The candidates are live heads that are in the generation, have
   an anchor on the chain, and are not consumed on the chain.
   - Heads off the chain are retained `unselected-structural-branch`; heads with no anchor are retained
     `no-structural-ancestor`.
   - Several candidates with identical payloads → **auto-merge**, reported in `autoMerged`.
   - Several different candidates → **fork**. The projection takes the `maxTs` head, flagged provisional.
   - No candidate → the stub `___`.
4. Live text on a key with no slot is retained `orphaned-text`.

After the books:

- Decisions and alignments use the same per-key resolution under the book's chain.
- An alignment whose md5 is not the md5 of the projected verse goes to `invalid[]`, with `orphaned` when the
  verse has no slot.
- Pins, meta, settings and stories resolve without ancestry.
- Notes are filtered by absent book and by generation, and re-keyed or retired by structural dispositions.
- Prefix collisions on dotted paths are resolved and reported.

**Output:**

```
books, stories, decisions, alignments, pins, projectMeta, projectMetaRemoved, settings, notes,
forks, invalid, retained, autoMerged, scope, vrs, vrsRejected, names, namesRejected,
pendingStructural, headsTs, supersedeRefused, liveHeads, liveNotes
```

### 3.3 Determinism

- R-8.6.1: the fold is a function of the event set, and survives permutation, duplication and partition
  followed by union. It is property-tested (JC-3, 200 runs) and tested on the three-device, sneakernet and
  team-sync fixtures (JC-9, JC-10, JC-35 G6).
- Payload identity uses a canonical serialization (`canon`, keys in JS own-property order) of the op's
  payload fields only. An additive optional envelope field never makes a fork.
- The derived bytes depend only on the event set and the (tool, book) resolution records (R-8.7.11).
  The resolution records come from the *installed resources* of whoever ran the fold (D30). Two teammates
  with different installed resource sets can write different decision-file `resource` records for the same
  journal.

### 3.4 Conservation versus what a person sees

- R-8.6.2 says each record ends projected, or reported in `retained`, `invalid`, `pendingStructural`,
  `forks` or `autoMerged`.
- **No UI shows `retained` or `pendingStructural`.** A grep of `src/views` and `src/state.jsx` finds no
  reader. The open Report carries them to the dev Inspector only.
- The sync Report carries only `carried`, `restored`, `received`, `textConflicts`, `newestWins` and
  `names`.
- So "conserved" means "still in the journal", not "a person can see it".

### 3.5 Cost (evidence)

- Full fold of an aligned NT (15,945 events): 671 ms. Checked whole Bible (278,041 events, 247.6 MiB of
  segments): 9,279 ms. Reading and validating 278k segment files: 33 s on local disk, ~177 s through the
  server's HTTP (`bench-fold-2026-08-25.md`).
- Every save folds again from scratch in a Web Worker (`fold-worker-2026-09-14.md`; 40k events ≈ 0.2 s;
  opening 40k segments took 81–90 s).
- On Windows, a request from the browser costs 4–14 ms (`open-time-windows-2026-09-28.md`). One file per
  action makes every whole-journal operation O(files) in HTTP requests.

---

## 4. Conflict detection and presentation

- **Fork:** a key with more than one live head after the ancestry and generation filters, with different
  payloads. The fold reports `{key, heads[], provisional: maxTs}`.
- **G7 (R-8.7.12, D98 point 8).** Only `text|` forks go to a person. Every other fork keeps its newest
  head:
  - alignment (`align|`);
  - check result (`dec|`);
  - pin, setting;
  - structural head (`skel|`, `book|`).

  The sync Report lists those forks in `newestWins`, counted once (a fork that the local-only fold already
  had is not counted again). `pendingStructural` and `retained` are not in the sync Report at all.
- **Verse conflict review (#679, merged):**
  - `verseConflicts(foldOut, events)` lists the `text|` forks in canon order, with each head's actor, text
    and provisional flag.
  - The UI shows one conflict at a time: "Mine" light blue, teammates cream, the Door43 username or
    "Teammate n", and a conflict count on Home and in Translate.
  - **Keep this version** = `resolveVerse(key, keptHead, shownHeads)`:
    1. Refuse if the heads changed since the screen was drawn.
    2. Build one `text.verse.set` with the kept text and `supersedes` = all heads.
    3. Try bases (the projected head first, then the others, newest first) with a dry fold. Publish the
       first base whose fold has no fork on the key, no refused supersede, and the kept text projected.
    4. If no base works ("branches that do not meet"), refuse. There is no way forward in the UI.
  - Two people who resolve the same conflict differently make a new fork, which surfaces again.
  - The same choice made twice auto-merges, because payload identity excludes `supersedes`.
- **Alignments after text conflicts:** I-3 validity is a hash of the projected verse.
  - The losing side's alignment shows invalid while the other text is provisional.
  - It becomes valid again if its text is kept.
  - It is never silently re-attached to text it was not made for.

---

## 5. The sync core (#675, `journal/sync.mjs`, merged on main)

**Signature:**

```
syncCore({ remote: Map<ipath,string>, mine: {actor, journal: Map, outbox: Map}, resolve, files })
  → { write: Map (sorted), remove: string[], report }
```

It is pure: no disk and no network. The validator is passed in (`files.mjs` in Node, `seal.ts` in the app,
with the same verdicts).

**Steps (`syncOrRefuse`):**

1. **OBS check:** OBS metadata on Door43 → refuse `sync.obs-unsupported`.
2. **Own files (G1):**
   - The own files are every local journal file under `checking/journal/<actor>/`, plus each outbox entry.
   - An outbox entry outside the own segments path → `outbox.invalid`.
   - An outbox entry that differs from the published file at the same path → `outbox.different-action`.
3. **Own-file differences:** any own path where Door43 holds different bytes → refuse
   `sync.own-file-differs` (R-8.7.7).
4. **Incoming journal files (G2, R-8.7.8):**
   - Validate every file under `checking/journal/` on Door43: actor slug, `actor.json` validator, or segment
     path shape, then segment validator, then actor binding, then filename equals first `ts`.
   - Any failure refuses the whole sync with `sync.incoming-invalid` and lists every failing path.
5. **Local journal files:** validate each local journal file (all actors) and each outbox entry. A failure
   throws the store's own code.
6. **Teammate paths:** a path where Door43 and this computer hold different bytes → refuse
   `segment.differs-from-accepted`.
7. **Union:** fold Door43 events ∪ held events ∪ outbox events. **Any fold throw becomes a failed Report
   with no code** (the shell turns it into `sync.failed`).
8. **Metadata overlay:** a projected or removed `project.meta.set` path → refuse `sync.metadata-overlay`
   (R-8.7.10).
9. **Resolution records:** for each (tool, book) with decisions, call `resolve(tool, book, out.pins)`
   (R-8.7.9 b).
10. **Projection:** `derivedProjections(out, {baseMetadata: Door43 metadata.json, resolutions})`, then
    delete `metadata.json`.
11. **`write`** = the whole derived set, plus own files Door43 lacks (`carried`), plus teammate files this
    computer holds and Door43 lacks (`restored` — the computer re-pushes what Door43 lost).
12. **`remove`** = Door43 paths in the removable derived classes (`BOOK.usfm`, alignment and decision
    sidecars) that the fold no longer projects.
13. **Facts:**
    - `received{actor: n}`;
    - `textConflicts` (each `text|` fork with actor and text per head);
    - `newestWins` (non-text forks not already in `fold(heldEvents)`);
    - `names`.

**What the core does not do.**

- It never compares Door43's *derived* files with anything. A Door43 `TIT.usfm` edited in the Door43 web
  editor or by another tool is replaced with no report (R-8.7.9: "replaced, not merged"). JC-35 checks only
  that the fold "tells them apart".
- It does not report `retained`, `pendingStructural`, `invalid` (stale alignments), `vrsRejected` or
  `namesRejected`.

---

## 6. The sync shell (#676, PR #688)

`sync(deps, {repoPath, session}) → Report` (`op:'sync'`). It is not wired to the UI yet; the caller is
#678. The contract: after an ok sync, the caller must open the project again.

### 6.1 Outer frame

1. No username or token → `sync.auth-failed`, before any request (the platform push panics without them).
2. `inProjectQueue(repoPath, …)`: the sync holds the project's in-process mutation queue for its **whole
   run**, apply included. Saves and opens queue behind it.
3. `ops.recovered !== true` → `opslog.not-loaded` (no sync before the start recovery has loaded the ops
   log).
4. `finishPendingApply(api, repoPath)` first: finish an apply that a marker says may have begun (§6.4).
   Failure → `sync.apply-pending`.
5. `ops.begin('sync', {repoPath})`. The ops record is only the Report for the dev Inspector. It never locks
   anything (D101 point 2).
6. `syncRecorded` (§6.2). Then `withCode`: any failed Report without a code becomes `sync.failed`. Close the
   record, unless a clone path that could not be deleted keeps it open so the next start deletes the path.

### 6.2 Steps

**1. Checks (`preflight`).**

- Net gate on, else `sync.offline` (D95).
- `origin` exists, is on the configured Door43 server, and has no credentials in the URL, else
  `sync.not-shared`.
- The project is not at the fixed clone path, else `sync.clone-path-exists`.

**Read this computer's side once.** A `JournalStore` opens the project, which derives the actor id and
ratchets the clock. The shell reads every journal file (`readIngredientFiles`: one zipped read of
`checking/journal`, then per-file reads for the rest; `audio/` is never read) and the outbox.

**Steps 2–6, inside `syncWithRetries` (up to `SYNC_TRIES = 3`):**

**2. Door43 main into scratch.**

- Delete the scratch copy if it is there: `_local_/_archive_/tc4-sync--<encoded repoPath>`. `-` → `-d`,
  `/` → `-s`. The encoding is injective. `_archive_` is never listed by the platform, so Home never shows it.
- `cloneProject(api, source, scratch, beforeClone, beforeMove)` under `withClonePath(<host>/<org>/<repo>)`,
  an in-process lock with key prefix `clone-path:`. Lock order: project queue first, then this lock.
  - If `list-local-repos` shows a folder at the fixed path, it is "a clone that a kill left", so delete it.
  - `beforeClone`: the ops note `{phase:'clone', clonePath, try}`. If that write fails, the clone does not
    start.
  - `POST /git/clone-repo/<host>/<org>/<repo>` with no `branch` → a **full** clone to the fixed path. It
    sends no credentials, so only public repositories work.
  - `beforeMove`: delete any `tc4-sync/apply.json` that came from Door43 (bench round 5: Door43 content
    never authorizes an apply).
  - `POST /git/copy/<fixed>?target_path=<scratch>&delete_src=true`. This is a copy, then a delete; it is
    not a rename.
  - `GET /git/branches/<scratch>`: if the checked-out branch is not `main` → delete the scratch and refuse
    `sync.not-main`. The platform has no route that checks out `main` from `origin` with full history.
  - A move failure → delete the clone path → `sync.move-failed`. If that delete also fails,
    `clonePathLeft` keeps the ops record open.

**3. Read Door43's files.** `remote` = every scratch ingredient except `audio/`, plus `metadata.json` from
`getMetadataRaw`.

**4. Name action.**

- `journal.sealName(username, ownEventsOnDoor43)`: ratchet past own Door43 events, refuse when a name
  exists (journal, outbox or Door43), else seal `{v:3, op:'actor.name.set', base:null, username}`.
- The sealed segment goes into a *copy* of the outbox map for this sync only. It is never staged into
  IndexedDB, so a refused sync writes nothing.

**5. `syncCore`.** `resolve = resolveFromCoverage(coverage)`: project the folded pins to `resources.json`,
then `resolveToolBook(…, installed coverage)`. A refused core → delete the scratch → return the core's
Report.

**6. Write, commit, verify.**

- `writeIngredient(scratch, path, bytes, {updateIngredients:false, keepBak:false})` for each `write` entry,
  then `deleteIngredient` for each `remove` entry.
- If `git status` is not empty: `remake-ingredients`, then `add-and-commit` with the message
  "Checkpoint, sync with Door43: …".
- `verifyProjectAgainstJournal(scratch, {skipUnjournaled:true})`: fold the scratch journal, project, and
  compare byte for byte with every derived file on disk. `metadata.json` is checked semantically (scope).
  Any mismatch, invalid segment or *extra* file → `sync.verify-failed`; nothing is pushed.

**7. Push (G4).**

- `POST /git/push/<scratch>` with `{remote:'origin', cred_type:'https', username, pass_key}`.
- `refusalForPush`:
  - `NotFastForward` → `sync.non-fast-forward`. Delete the scratch and **start again at step 2**, up to 3
    tries, then refuse.
  - `code=Auth (-16)` → `sync.auth-failed`.
  - 403 → `sync.no-write-access`. **Not measured yet**: the owner account is a site admin.
  - Anything else → `sync.push-failed`.
- The platform push sets no `push_update_reference` callback (PLATFORM-NOTES #46). A rejection of a single
  ref after the pack (a protected branch, a hook) **may be reported as success**. The transport spike lists
  this as not proven.

**8. Apply (G8, D101).**

1. Ops note `{phase:'apply'}`.
2. Write the **apply marker** `tc4-sync/apply.json` (`{actor, at}`) into the *scratch copy*. If that fails,
   delete the marker and the scratch → failed Report; the project has not changed.
3. `nextSyncGeneration(repoPath)`: an in-process counter. Any store opened at the old generation now
   refuses every `queue()`d mutation and checkpoint with `sync.project-replaced`.
4. `applyScratch(api, repoPath, scratch)`:

   ```
   target    = scratch files (not audio/, not tc4-sync/)
   livePaths = project files (same filter);  live journal = project journal files
   1. journal files of target whose live bytes differ → write   (repairs a torn earlier write)
   2. non-journal files of target absent or different in the project → write
      (one GET per file to compare)
   3. live non-journal files not in target → delete   (journal files are never deleted)
   4. if git status is not empty: remake-ingredients, add-and-commit "Sync with Door43 (tC4)"
   ```

   - One retry in the same run. A second failure → `sync.apply-pending`. The marker stays; the project takes
     no open until the next start or sync finishes the apply.
5. **Clean-up:**
   1. Delete the marker.
   2. `journal.clearCarried(outbox)`: delete each outbox key whose bytes are still the carried bytes.
   3. Delete the scratch.

   Any failure here is `sync.cleanup-failed`. The sync is done; the marker or scratch is left for the next
   start or sync.
6. Return the ok Report with the core's facts plus `tries`.

**A side effect on git history.** After the first sync, the project's own `.git` no longer follows Door43's
history. The apply commits on the project's own line, and Door43 main moves through each scratch clone.
Door43 is the shared history. The project's `.git` is a local checkpoint log, and a push from it would not
be a fast-forward.

### 6.3 Locks, gates and refusals

| Mechanism | Scope | Purpose |
|---|---|---|
| `inProjectQueue(repoPath)` | in-process promise chain | serializes every store mutation, every open (one queue turn from the marker check to the end of convergence), and the whole sync |
| `withClonePath(fixed path)` | in-process | two clones of one Door43 repository (a sync, #677 open-from-Door43) never meet at the fixed path |
| `withPathLock(repo ipath)` | in-process | read-check-write of one journal file |
| `recoverySettled()` | in-process promise | opens and creates wait for the start recovery's disk rule, not for the settings read |
| `refuseNonProjectPath` | open and create | only `_local_/_local_/<name>` opens (`open.not-a-project`), so the fixed clone path is never a project |
| `refusePendingApply` | open | one GET of the scratch's paths; a marker present, or no answer, refuses `sync.apply-pending` (fails closed) |
| sync generation | in-process `Map` | a store opened before an apply is stale (`sync.project-replaced`) |
| `ops.recovered` | in-process | no sync before the start recovery loads the ops log (`opslog.not-loaded`) |

None of these locks work across processes. The design depends on D39: one app instance per machine.

**New refusal codes in PR #688** (`journal/report.mjs`; all app rules with no R-id; strings in 10
catalogs):

```
open.not-a-project, sync.auth-failed, sync.offline, sync.not-shared, sync.clone-path-exists,
sync.clone-failed, sync.move-failed, sync.verify-failed, sync.non-fast-forward,
sync.no-write-access, sync.push-failed, opslog.not-loaded, sync.failed, sync.project-replaced,
sync.not-main, sync.apply-pending, sync.cleanup-failed
```

These join the core codes `sync.own-file-differs`, `sync.incoming-invalid`, `sync.metadata-overlay`,
`sync.obs-unsupported` and `actor.name-already-set`.

### 6.4 Crash recovery: the "disk rule" (`opsLog.ts`)

```
finishPendingApply(repoPath):
  scratch has tc4-sync/apply.json (any bytes) → bump generation; applyScratch again; delete marker;
                                                 delete scratch → 'applied'
  scratch exists, no marker                   → delete scratch → 'deleted' (the project never changed)
  no scratch                                  → 'none'
```

**When the disk rule runs.**

- **At start:** `OpsLog.recover(api)` runs the disk rule for **each repository that `list-local-repos`
  names** (any path; resources included; one GET each), *before* it reads the settings document.
- **At each sync:** before the sync starts.

**What `recover` does after the disk rule.**

1. Read the client-settings document. A document that does not parse is kept under `unreadableSettings`,
   replaced with an empty ops log, and reported. Any other read failure → not recovered, so syncs are
   refused.
2. Resolve each open record:
   - **import:** delete the partial repository;
   - **export:** report it as interrupted;
   - **sync:** delete a clone path that the record names; run the disk rule again; report the sync ok if
     an apply was finished or the record is at phase `apply`, else failed and rolled back;
   - **open or checkpoint:** left to outbox replay at the next open.

**Proofs in tests (fake rig).**

- A kill after every server call after the push, and *inside* each call: a part copy, a part delete, a
  part write. The next start must leave exactly one project that opens as `converged` or
  `regenerated-forward`, with every own segment and every outbox entry.
- Settings-document cases: torn, `TypeError`, hung, and a concurrent raw writer.
- A kill inside the scratch delete.

**What the tests do not model:** power loss, fsync, `metadata.json` torn by the platform (RISKS #13), and a
scratch copy that is itself damaged.

### 6.5 What D101 removed (history of the review)

The first design (commit `72318d4`) swapped projects:

- copy the live project to a "previous copy";
- delete the live project;
- copy the scratch to the live path;
- delete the scratch;
- record each phase in the ops log;
- recovery goes forward or back ("go-back", "restore"), and the next successful checkpoint deletes the
  previous copy.

The reviews found these problems:

- a stale store's queued save plus a checkpoint could delete the previous copy, which was the only
  rollback;
- an open could interleave with a swap;
- a settings document that could not be read, or a raw writer such as `recordInstalled`, could drop the
  lock record, so a half-swapped project opened;
- a part copy at the live path could take edits that recovery then went forward over;
- a non-main default branch.

The fixes came in this order: generations, open as one queue turn, the "live mark", fail closed on an
unloaded log, `sync.not-main`. Then D101 deleted the swap, the previous copy, go-back and restore, and moved
the safety record from the settings document to the disk (the marker). Net change: −890/+525 lines.

---

## 7. Concurrent edits by two users

`A` and `B` start from the same synced state. Each edits offline. Then both sync; the order does not
matter, because the fold is order-free. "Shown" means the projected USFM or sidecar (the Door43 tree is
the same) or a review screen.

| Case | Fold result | What a person sees |
|---|---|---|
| Different verses, same book | linear on each key, no fork (JC-9, fixture `different-verses`) | both edits |
| **Same verse**, different text | `text\|…` fork; the newest `ts` projects, flagged provisional | the conflict review lists it; keep one version; the other is retained `superseded` |
| Same verse, identical text | auto-merge (`autoMerged`) | nothing; correct |
| Same verse, A edits twice, B once | A's second edit advances A's branch; the fork is A2 against B | review |
| C edits on top of the provisional head | C's base = B's head, so C advances B's branch; still forked against A | review |
| Both resolve one conflict differently | the resolutions fork again | review again |
| **Same check** (`dec` key) | fork; the newest projects (fixture `check-two-ways`: C's `todo` beats B's `valid`) | **nothing**. The sync Report counts it in `newestWins`; the losing head stays live in `forks[]` for good, because no UI resolves non-text forks |
| **Same alignment** (`align` key) | fork; the newest projects (E4: A's `done:true` record loses to B's later one; A's head is in `forks[]`, not in `retained[]`) | **nothing**, apart from the count |
| Alignment against a verse whose text a teammate changed | I-3 hash mismatch → `invalid[]` | the alignment shows invalid; correct and visible |
| Notes | grow-only union | all notes |
| Settings / pins | LWW per dotted path or slot. A gateway change is many slot events; two concurrent changes on different slots can mix | nothing |
| **Two verse bridges in one book** (E1, E1-reversed) | the *older* `text.structure.apply` applies. It pushes a post-image on every slot, so the newer one is `pendingStructural: incomplete` (undispositioned post-images). Every later edit by the newer author in that book is anchored on their own structural action, so it is retained `unselected-structural-branch` | **nothing**. 0 forks. The newer author's bridge and every edit after it in that book vanish from the USFM and from Door43 |
| A bridge, while B edits *any* verse of the book with an older `ts` (E2a) | A's action names the old head of every slot as a source. B's edit replaced one, so A's action is `conflicted` and pends forever. A's later edits in the book are hidden as in E1 | **nothing** |
| A bridge, while B edits any verse with a *newer* `ts` (E2b) | a phantom `text\|` fork on that verse: A's post-image (unchanged text) against B's edit. B's newer edit projects | the review shows a "conflict" that A never caused |
| A merges 1:4–5, while B edits 1:5 (E3) | B's head is on a key with no slot → retained `orphaned-text` | **nothing**. B's edit vanishes |
| A renumbers while B edits a moved verse | a valid `text\|` fork, which #679 handles. When the branches "do not meet", `resolveVerse` refuses | possibly a dead end |
| Two `book.add` of one book with different payloads (two independent seeds) | structural fork; the newest wins | nothing |

Section saves that create or break a verse span go through `applyStructuralEdit` (`state.jsx:1575–1593`),
which is the same builder used in the experiments. E1–E3 can therefore happen through normal Translate use
by two translators.

---

## 8. Experiment evidence (reference fold at `pr688`)

```
E1 two concurrent bridges in TIT (A 1:4-5 at 10:00, B 2:3-4 at 10:05), then one edit each
forks: []
retained by reason: {"unselected-structural-branch":1}
pendingStructural: [{"status":"incomplete","actor":"translator-b","detail":["undispositioned:text|2:3|…translator-a", …]}]
projected 1:6 has A edit: true      projected 2:6 has B edit: false
slots 1:4-5 / 2:3-4 present: true false      text forks the review would show: 0

E1 reversed (B bridges at 09:00 and drafts 2:6, 2:7; A bridges at 10:00 and drafts 1:6)
forks 0  pending [ 'translator-a:incomplete' ]  retained [ 'text|TIT|1:6:unselected-structural-branch' ]
A 1:6 shown false | B 2:6 shown true | 1:4-5 slot false | 2:3-4 slot true

E2 A bridges 1:4-5 at 10:00; B edits 3:1 at 09:59 without having received the bridge
forks: []   pendingStructural: [{"status":"conflicted","actor":"translator-a","detail":["text|TIT|3:1@…"]}]
E2 … B edits 3:1 at 10:01:  forks: [{"key":"text|TIT|3:1","n":2,"provisional":"translator-b"}]

E3 A bridges 1:4-5 at 10:00; B edits 1:5 at 10:01
forks: []  retained by reason: {"orphaned-text":1}   any projected verse shows B edit: false

E4 A and B each write the first TIT 1:1 alignment record
forks: [{"key":"align|TIT|1:1","n":2,"provisional":"translator-b"}]  A head in retained[]: false

E5 poison: the segment passes the one validator: true
fold of the union throws: text.skeleton.set changes the slot set (…) — refuse to fold

E6 fast clock (C is 30 days ahead): projected head is C: true
A clock after receiving C: 2026-11-13T10:00:00.000Z|0001|translator-a
```

---

## 9. What is not handled yet

### 9.1 Open items named in the PR and its docs

1. **No UI wiring.** `sync()` has no caller; #678 owns the button, the reopen after sync, and showing
   `textConflicts` and `newestWins`. The contract "after an ok sync the caller MUST open the project again"
   rests on the caller.
2. **No write access (HTTP 403).** The reply is `[PENDING — #673 step 4]`, matched by a heuristic on the
   status in the reason text. A refusal of a single ref after the pack (a protected branch) may look like
   success, because the push sets no `push_update_reference` callback (PLATFORM-NOTES #46).
3. **Audio.** Round 1 never reads or writes `ingredients/audio/`. Own audio never reaches Door43, and a
   teammate's audio never arrives (D101 point 5). "The audio design stays open".
4. **Private repositories.** The clone sends no credentials, so only public repositories work (D98 point 1).
5. **OBS projects:** refused (`sync.obs-unsupported`).
6. **USB, sneakernet, private teams:** later rounds.
7. **Project delete.** When the app gets a delete, it must also delete `scratchPathOf(repoPath)`. Otherwise
   a new project at the same path (same actor, G5) would take a stale apply (RISKS #11). A scratch copy whose
   project is gone stays hidden on disk for good, because the disk rule scans from the project list.
8. **`metadata.json` torn by the platform** (RISKS #13, PLATFORM-NOTES #50). This is a help-desk restore
   from the last commit. During a sync it also leaves the marker, so the project is refused as
   `apply-pending`.
9. **One app instance per machine (D39).** Every lock and the sync generation are in-process only.

### 9.2 Reviewer findings in the task, and where they stand at `70d32cb`

| Finding | Status | What remains |
|---|---|---|
| **The checkpoint deletes the pre-swap copy** | Gone with D101: no swap and no previous copy. The trigger (a stale store's queued save or checkpoint landing after a swap) is now blocked by the sync generation (`sync.project-replaced`). | The refusal turns the queued edit into a lost edit: the message tells the user to "make your last change again". The `SaveScheduler` keeps the buffer as a failure, but there is no path that rebases it onto the new fold. A naive "retry on the new store" in #678 would be dangerous (F5). |
| **An open overlapping a sync** | An open is one turn of the project queue (marker check, reads, writes); the sync holds the same queue. A test proves a sync makes no git call while an open runs. | In-process only. The marker check runs once at open; a store that is already open while an apply runs reads half-applied files (its writes are refused afterwards). A second process (two app windows, or a dev and a packaged build on one store) has no protection. |
| **Rollback state** | Removed. Recovery only goes forward. The ops record is only a Report. | Forward-only has **no escape hatch**. If the scratch copy is damaged (disk error, partial copy before the marker, part of it deleted by hand), or `applyScratch` keeps failing (disk full, a platform panic poisoning `repo_dir`), every start and every sync throws, the marker stays, and the project is **refused for good** (`sync.apply-pending`, "Wait, then open the project again…"). That breaks the new G8 text ("never makes the project unopenable"). The only way out is to delete `_local_/_archive_/tc4-sync--…` by hand. |
| **A branch other than main** | Refused: `sync.not-main` after the clone; the scratch is deleted. | No way forward: a repository whose default branch is `master` (or one renamed by an owner) can never sync. The clone always takes Door43's default branch, not `main` explicitly. Whether new projects and first shares always make `main` is not proven here. |

### 9.3 Further gaps I found

- **The sync Report and the UI leave out `retained`, `pendingStructural`, `invalid` and `supersedeRefused`**
  (§3.4, §7).
- **Edits to derived files on Door43 are overwritten without a report** (§5). This covers the Door43 web
  editor, gatewayEdit, tcCreate and git users.
- **Any non-derived extra file under `ingredients/` on Door43 blocks every sync.** The verifier counts it as
  an `extra` mismatch → `sync.verify-failed`, for every teammate, until someone fixes Door43 by hand.
- **A segment that passes the validator but makes the union fold throw blocks every teammate's sync**
  (`sync.failed`). If it reaches a local project, that project cannot open (E5). The cases are: a slot
  change in `text.skeleton.set`, a stale own skeleton base, a structural base of the wrong kind, a
  disposition outside the affected set, and two events with one `ts`. Because of R-8.1.15 there is no
  retraction, and every teammate who holds the file re-pushes it (`restored`).
- **Version skew.** The first sync of a 4.1 client writes a `v: 3` name event. Any build that accepts only
  `v` 1 and 2 then fails G2 on every sync (`sync.incoming-invalid`). The repository has no
  minimum-reader-version marker and no negotiation (RISKS #10 covers only the error text).
- **Scaling.**
  - Every sync is a full `git clone` (all history, with every derived file rewritten in each sync commit),
    a full read, two to three full folds (core, verify, then the open after), and a full push.
  - On a non-fast-forward, all of it repeats, up to 3 times.
  - A team that syncs at the end of the day can **livelock** into `sync.non-fast-forward`.
  - Each apply write is one HTTP POST: 0.33–0.43 ms from Node on an M2, but 4–14 ms from a Windows browser.
    A first sync into a large team journal makes a long crash window.
- **The resolution records depend on installed resources** (`resolveFromCoverage`). Two teammates with
  different installed resources write different decision-file `resource` records. G6 holds only "for the
  same resolution records", so Door43 can flip back and forth between them.
- **The name action records the Door43 username in public, permanent history.** It cannot be withdrawn
  (D98 point 5 amends D85).

---

## 10. USB and local exchange

- **Round 1 has none** (D98 point 1: "USB transport and private repositories come later").
- **What the format already supports:**
  - Journals are disjoint per actor and form a set union. Copying actor directories between machines
    folds the same as a git merge (JC-10: "sneakernet (file copy union) ≡ in-memory union ≡
    cross-device read").
  - A Scripture Burrito zip export keeps the journal (D80).
- **What blocks USB in the app today:**
  - D53 "joining is not merging": importing a teammate's zip makes a *new* project at a new path with a new
    actor. No operation merges two projects.
  - The long-term plan (TEAM-SYNC-PLAN S3; D67(4b)) proposes a USB team with one "integrator" device that
    carries the team main as a zip, imported under `_local_/_sideloaded_/`. It is not built.
- **Reuse:** `syncCore` is transport-free. A USB shell could pass `remote` = the journal files and base
  `metadata.json` from a USB bundle, run the same core and apply, and export
  `{own ∪ held journal files}` back to the stick. Only the shell (clone, push, retry) is Door43-specific.
- **The Pankosmia model** (`pankosmia-sync-model-2026-07-08.md`): Pankosmia's own client does a three-way
  git merge in a scratch copy, and on any conflict it aborts and stays clean, with no resolution. tC4 keeps
  the scratch principle but replaces git merging with the journal fold.

---

## 11. Failure modes that could lose a translator's work, and how a better design removes them

"Lost" here means gone from every screen, from the projected text, or from Door43, even if the bytes still
exist in some journal or git object. Ordered by how likely they are in real team use and how much work is
at stake.

**F1. Concurrent structural actions hide whole books of work (E1, E2a).**

- One translator's verse bridge (or renumber or split) plus any concurrent bridge, or any concurrent edit
  with an older `ts`, makes one structural action pend. Every edit that author makes in that book afterwards
  is anchored on the pending branch: not projected, not reviewed, not in the sync Report.
- Cause:
  - `text.structure.apply` re-heads **every** slot (transitions must map every destination);
  - verse edits anchor their identity on the structural chain;
  - G7 hides structural forks;
  - `pendingStructural` reaches no UI.
- Fix:
  - Make verse identity stable and independent of the skeleton chain: key verses by versification frame
    ids, and model a bridge as an annotation over those ids that touches only the verses involved.
  - Concurrent bridges on separate ranges then commute.
  - A bridge against an edit of a verse in that bridge becomes an ordinary verse conflict.
  - Never anchor plain verse edits to a structural branch.
  - Show every structural conflict and every pending action to a person.

**F2. Edits to a verse that a concurrent merge or renumber removed (E3) are retained `orphaned-text` and
shown nowhere.**

- Fix: the same stable-identity model, plus a person-facing "orphaned work" queue with restore.

**F3. Silent newest-wins for alignments and check results (G7, E4).**

- An alignment can be 10–30 minutes of work per verse. A concurrent teammate's write replaces it with no
  notice. "Newest" is wall-clock-based HLC, so it is unbounded (E6).
- Fix:
  - Show every fork kind in a review queue (at minimum: "your alignment of TIT 1:1 was replaced by X —
    restore?");
  - allow a "resolve" event (`supersedes`) for every register kind;
  - bound clock skew (§12.6).

**F4. Remote derived-file edits are overwritten (R-8.7.9).**

- A teammate who uses the Door43 web editor or another Burrito tool on the same repository loses their
  edit at the next tC4 sync, with no report. Git history keeps the old bytes.
- Fix: at intake, compare each Door43 derived file with the projection of *Door43's own journal*. On a
  difference, reconcile it into events attributed to an "out-of-band on Door43" source, as §8.8 does
  locally, or refuse and report. Better still, keep derived files off the shared branch (§12.3).

**F5. Stale whole-book buffers revert teammates' work.**

- `writeBook` diffs a whole-book string against the store's current fold and uses the *current* head as
  `base`. A buffer older than the fold, for example one written after a sync reopen or a retry of a
  `sync.project-replaced` failure on a new store, journals the old text of every verse a teammate changed.
  The writes are *linear* edits with base = the teammate's head, so no fork and no review: the teammate's
  work is silently reverted.
- Today the stale-store refusal blocks this, at the cost of the user's own unsaved edit ("make your last
  change again").
- Fix: the editor keeps the head `ts` it displayed per verse and sends edits as
  `(verse, observedBase, newText)`. A stale base then becomes a fork, never a revert, and a store replaced
  under an edit can simply publish it.

**F6. Poison and version skew stop the whole team (E5).**

- One segment that passes per-file validation can make the union fold throw, or a newer `v` can fail older
  readers. Every teammate's sync then fails, and any project that took the segment fails to open. Journals
  are permanent, and `restored` re-pushes the file, so the team cannot back out without hand surgery on
  git.
- Fix:
  - Make the fold *total*: every cross-event semantic defect quarantines the offending event or action,
    reported, and never throws.
  - Keep segments with an unknown version as opaque blobs: carried and pushed, but not folded, with a
    "update to see N changes" notice.
  - Keep a `journal/format.json` with the minimum reader version in the repository.
  - Allow an explicit, signed "withdraw" record that excludes a segment from the fold without deleting
    bytes.

**F7. One actor on two machines.**

- The actor id is HMAC(secret, path), and the secret lives in IndexedDB. Copying the app profile (OS
  migration, backup restore, a cloned VM) gives two machines the same actor. Same-actor events "never
  fork": the later `ts` replaces the earlier linearly, with **no fork and no retained record**. An identical
  `ts` from both machines → "corrupt union" → F6.
- Fix:
  - Detect a duplicate actor: at sync, an own-actor segment on Door43 that this device did not write, by a
    per-device nonce in `actor.json` or a signing key. Then move this device to a fresh actor and refuse
    until it does.
  - Sign segments with a per-device key.

**F8. Lost installation secret or outbox.**

- Clearing browser storage changes the actor id, which is harmless for published data. It also orphans the
  IndexedDB outbox (keyed by the old actor), so staged but unpublished actions are never replayed. The
  window is small (milliseconds per save), but the loss is silent.
- Fix: keep the outbox as atomic files next to the journal (write a temp file, fsync, rename), and the
  secret in the OS keychain or app data with a backup and restore path.

**F9. Non-atomic writes and power loss.**

- The ingredient POST and `metadata.json` (remake, commit) are written in place with no fsync or rename
  (PLATFORM-NOTES #50). A truncated `metadata.json` stops the project from opening (RISKS #13). The sync's
  file-by-file apply widens the window.
- Fix: every write goes to a temp file, then fsync the file, rename, and fsync the directory. With Rust and
  direct filesystem access (no pankosmia HTTP layer) this is easy.

**F10. A forward-only apply with a damaged scratch locks the project for good (§9.2).**

- Fix: make the local effect of a sync "add journal files" only. Each file is atomic and idempotent, so the
  journal itself is the record. Derived files are a cache rebuilt at open. No marker and no scratch apply are
  needed, and a failed rebuild never locks the journal.

**F11. A false "synced" status.**

- An unmeasured 403, a ref rejected after the pack that is reported as success, or a stale
  `restored`/`carried` loop. A user who trusts the green status and retires the machine loses the work that
  never reached Door43.
- Fix: after the push, verify with `ls-remote` or a fetch that the remote ref equals the pushed commit
  before reporting success, and show "last confirmed on Door43 at …" for each actor.

**F12. A dead end in the conflict review for structural forks.**

- `resolveVerse` refuses when "branches do not meet", and the losing version is then unreachable in the
  UI. The F1 model fix removes it.

**F13. A whole sync blocked by one extra or legacy file on Door43** (`sync.verify-failed`). This is
availability, not loss. But blocked teams work longer offline, which raises F1–F3 exposure.

- Fix: tolerate and carry unknown non-derived files as an "unjournaled class", like `audio/`. Never fail
  the whole team on one file.

**F14. The project is locked while a sync runs.**

- The sync holds the project queue for minutes on large projects. Edits made during it are refused
  afterwards (F5).
- Fix: sync in the background against a snapshot of the journal file *set*. New local segments made
  during the sync are just more own files for the next round. No lock is needed, because files are disjoint
  and append-only.

---

## 12. A sketch of a better design for the Rust rebuild

### 12.1 Keep

- The immutable, per-actor, sealed action segments: one actor per directory, never rewritten.
- The HLC-ordered event set.
- `base` and `supersedes` fork detection.
- A deterministic fold.
- Validate before folding (zero-trust intake), and process incoming work in a scratch area.
- The one-validator rule.
- The conformance discipline: rule ids claimed by executable checks, property tests over permutations.

The Automerge proof (`automerge-proof-2026-08-19.md`) concluded that a CRDT library does not remove this
machinery. Its safe shape was "an immutable action set plus the tC4 fold".

### 12.2 Change the data model

- **Stable verse identity.** Key text registers by versification-frame verse id. Represent spans, bridges
  and renumbers as per-verse annotations ("the text for these ids is stored on id X", "id Y is displayed as
  Z"). Each touches only the verses it names, so they compose with concurrent edits elsewhere. Keep a small
  structural register per affected range, not per book. This removes F1, F2 and F12.
- **Total fold.** Never throw for anything that passed per-segment validation. Every semantic defect
  becomes a reported quarantine entry. Unknown versions are carried as opaque data (F6).
- **Edits carry the base the person observed** (F5).
- **One review queue for every fork and every retained or pending record**, by kind, with "keep mine",
  "keep theirs" and "restore" as `supersedes` events. Alignments and checks get at least a notice and a
  restore (F3).
- **Bounded HLC.** Reject or flag incoming events whose physical part is more than about 24 h ahead of the
  receiver's clock. Do not ratchet past that bound. Show the skew (F3, E6).
- **Signed segments.** A per-device Ed25519 key, with the public key in `actor.json` and the signature in
  the container. Detect an actor duplicated across devices (F7).

### 12.3 Change the storage and transport

- **Journal-only exchange.** Sync the `checking/journal/**` tree; the union is file-level and cannot
  conflict.
  - If Door43 main must carry derived USFM for Door43 Preview and other tools, produce it as a
    deterministic build, ideally on a separate branch or tag, or at least compare it with the projection of
    Door43's own journal and reconcile any difference into events (F4).
- **Avoid non-fast-forward contention.** Either:
  - per-actor branches or refs (`refs/heads/actor-<id>`), so each push only fast-forwards one's own ref
    (D16/D67 "send without receive", JC-19/T3 already prove it); or
  - a push that retries only the cheap step: fetch, add own files, commit, push. Do not clone again.
- **Incremental fetch** (gitoxide or libgit2 in-process, not the pankosmia HTTP layer). Never clone the
  whole history again on each sync.
- **Pack segments for transport and storage.** Keep loose segments locally for crash safety. At sync, roll
  them into immutable per-actor packs (content-addressed, signed). This cuts the 278k-file whole-Bible case
  to thousands of files.
- **USB is the same core.** "Export bundle" = the journal files the receiver does not have (or all of
  them). "Import bundle" = validate → union → regenerate. A relay stick can carry every actor's packs.

### 12.4 Change local durability and apply

- The journal is the only durable state. Write segments by temp file, fsync, rename and directory fsync.
  The outbox becomes unnecessary.
- Derived files are a cache. Rebuild them forward at open from the fold, which is always safe and
  idempotent.
- Detect out-of-band edits of derived files with a manifest of the hashes the app last wrote, and reconcile
  them into events. Do not "explain the disk by journal prefixes".
- With that, a sync's local apply is "add files", recovery is "open the project", and there is no marker or
  lock state. This removes F9, F10 and F14.
- Use an incremental materialized fold, for example SQLite holding the current heads, updated per event.
  Run a full re-fold as a background verification. Keep the property tests that prove the incremental fold
  equals the full fold (R-8.6.1 already allows this).

### 12.5 Prove it

Keep and extend the shared team-sync fixture (`conformance/fixtures/team-sync/`) with the cases this
report found:

- two concurrent bridges;
- a bridge against an edit with an older `ts`;
- a merge against an edit of a merged verse;
- an alignment fork;
- a fast clock;
- a poison segment;
- a duplicated actor;
- a derived file edited on Door43;
- an extra file on Door43.

Each case should state what the *person* sees, not only what the fold reports.

---

## 13. Reproducing the experiments

```
mkdir exp && cp -r <repo>/journal exp/ && cp -r <repo>/conformance/fixtures/team-sync exp/
cd exp && echo '{"type":"module"}' > package.json && npm install usfm-js@3.4.3
node <scratchpad>/scripts/concurrency.mjs <abs path to exp>   # E1–E6
node <scratchpad>/scripts/e1rev.mjs <abs path to exp>         # E1 with the order reversed
```

- The scripts build structural actions with the app's own builder (`reconcileUsfm`, `seed: null`,
  `sources` for the span), as `JournalingStore.applyStructuralEdit` does.
- Each later edit takes `base = foldOut.headsTs[...]` from the author's own fold, as `writeBook` does.
- Every event used is sealed with `sealAction`; E5 also checks the segment with `validateSegment`.

---

## 14. Index of the files read

- **PR diff:** `git diff 1dbe423 pr688`. The main files:
  - `src/data/share/syncOperation.ts` (new, 530 lines);
  - `src/data/journal/opsLog.ts` (+353);
  - `src/data/journal/journalingStore.ts` (+75);
  - `src/data/journal/journalStore.ts` (+131);
  - `src/data/serverApi.ts` (+54);
  - `journal/report.mjs` (+19);
  - `src/data/journal/verify.ts`;
  - `test/sync/syncOperation.test.ts` (1186 lines);
  - `test/sync/syncRig.integration.test.ts`;
  - `docs/DECISIONS.md` D101; `docs/RISKS.md` 11–13; `docs/PLATFORM-NOTES.md` 50; `BURRITO-SPEC` §8.7
    R-8.7.10 and G8.
- **Main:**
  - `journal/{fold,files,checkpoint,sync,report,hlc,reconcile,skeleton,schema,grammar}.mjs`;
  - `src/data/journal/{journalingStore,journalStore,identity,foldRunner,foldWorker,verseConflicts,verify}.ts`;
  - `src/data/{saveScheduler,checkpoint}.ts`;
  - `src/data/share/shareOperation.ts`;
  - `test/sync/syncCore.test.ts`;
  - `conformance/validate-journal.mjs` (JC-9/10, JC-35), `conformance/validate-transport.mjs` (T1–T4);
  - `conformance/fixtures/team-sync/{README.md,cases.json}`;
  - `docs/BURRITO-SPEC.md` §6, §8, Appendix A; `docs/DECISIONS.md` D98 (and D67 excerpts);
    `docs/plans/TEAM-SYNC-PLAN.md` Round 1 and S3;
  - `docs/evidence/{automerge-proof-2026-08-19, bench-fold-2026-08-25, fold-worker-2026-09-14, pankosmia-sync-model-2026-07-08, sync-transport-spike-2026-10-13, open-time-windows-2026-09-28}.md`;
  - `docs/PACKAGING.md` (the isolated `repo_dir`).
