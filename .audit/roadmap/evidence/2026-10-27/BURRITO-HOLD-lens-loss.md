# BURRITO-HOLD, loss lens

- Reviewer: loss lens (independent), 2026-10-27
- Change: `git diff 7285d2c0..eee0a0fe` (product code: `Writer::metadata_entries` and the new
  `Writer::listed_in_metadata`, `crates/tc-store/src/materialize.rs:1922-2040`)
- Verdict: **no blocker, no major.** 2 minors, both older than this card and not made worse by it.

## What I ran

- `tc-heavy cargo nextest run -p tc-store --test store groups::`: 24 passed (the four new store
  tests among them).
- Temporary probes (appended to `groups.rs`, removed afterwards; `git status` shows only the
  evidence files):
  - M4 kill (metadata.json lists v1, disk holds v2), TIT unreadable at open, **no save**, then
    TIT readable and a periodic checkpoint: the folder is valid. Healing does not need a
    pending edit.
  - M5 kill (metadata.json lists v2, valid folder), TIT unreadable: a checkpoint with no
    save, a save plus a checkpoint, a crash, a reopen while still held, then readable plus a
    checkpoint. The folder stays valid at every step and ends with the newest save on disk.
  - M5 kill, then another program deletes `metadata.json`, TIT unreadable: the open works,
    the app writes no metadata.json while TIT is held (the build fails because currentScope
    lists TIT and no ingredient carries it, so the group waits). The checkpoint commits. Once
    TIT reads, the folder is valid with the newest save.
  - Plant: the old `Writing{old}` fallback put back. `a_checkpoint_that_cannot_read_a_book_file_keeps_a_valid_folder_valid`
    and my M5 probe both fail (TIT listed 584/12fb…, disk 585/4451…). The fix is what makes
    them pass.

## The six questions (FUNNEL §8)

1. **Code.** The rule lives at one entry point, `metadata_entries`, in the `Writing` arm
   (`materialize.rs:2024-2040`). The match on `Disk` is now exhaustive: the old `_ => continue`
   is gone, so `Missing` and `Unreadable` are decided on purpose. No type can carry "the bytes
   are undecided, so repeat the folder's claim", because the deciding fact is a read of a file
   another program holds. The card says so. The old fallback is deleted. Nothing is missing.
2. **Checks.** The five tests named in L153 exist (four in `crates/tc-store/tests/store/groups.rs:1130-1239`,
   one in `crates/tc-sync/tests/sync/exchange.rs:408-482`). They are deterministic (FaultFs,
   ManualClock), run in the normal nextest suites that CI runs, and take under 0.6 s each.
   I checked the main plant myself (above). The card's log records the other plants.
3. **Reach.** The trigger is reachable: a power cut or kill in the milliseconds between a
   book file's rename and the `metadata.json` rename (M4), or between that and the manifest
   (M5), followed by an open while another program holds the book file. Examples: Paratext or
   an editor with the USFM open on Windows, or a cloud placeholder not yet downloaded. It is
   rare, but a translator can reach it, and the old code turned a valid folder into an invalid
   one there.
4. **Siblings.** I searched every place that lists or guesses bytes for a file it cannot read:
   - Held files (`listing_of_held`, `materialize.rs:353`): they are listed with the held md5,
     which is the bytes the app read and recorded when it made the hold. That matches what
     metadata.json already says after the hold was listed. Same rule in effect.
   - Carried files (`listing_of_file`, `materialize.rs:284-291`): an unreadable carried file
     is listed as the manifest's `Placed` entry. A carried placement has no `Writing` state.
     The carried rename runs before M3's `write_manifest` (`materialize.rs:892`, then `:955`),
     and metadata.json is renamed after that. So when the manifest is behind the disk,
     metadata.json is behind it too, and the listing equals the folder's claim. There is no
     "valid made invalid" path here.
   - Fixed files (`materialize.rs:310`): same pattern, unchanged, and they are never in a
     write group with a kill window that matters.
   - `build_tree` (`checkpoint.rs:679-1069`): it builds from the projection or snapshot, never
     from the folder's metadata.json, and runs `validate_project` before it writes the tree.
     Commits are valid by construction.
   - Exchange and the J7 Burrito zip (`tc-app/src/export.rs:419` goes to
     `tc_sync::exchange::export`): HEAD after a checkpoint. Covered by proof (3).
   - The open's O10 is the writer's first materialization (`actor.rs:53-68`), so it runs the
     same `metadata_entries`.
   - `shown_metadata` (X2 comparison) now shows metadata.json's own entry for an undecided
     path. That is more truthful, and the comparison is not harmed.
5. **No loss.** This is a no-loss item: three lenses. This file is the loss lens; parity and
   races are in this folder. The crash and restart cases above, together with the card's
   tests, cover kills at the rename, at metadata.json, while held, at reopen and at heal. No
   text, alignment, decision or comment is lost: the saves are in the journal, and the commit
   and exchange file carry the newest save (proofs 1 and 3).
6. **Prose and Human.** The comment at `materialize.rs:2021-2024` gives a reason and does not
   narrate the code. The doc comment on `listed_in_metadata` is one line. There is no owner
   question; a translator does not see this rule. STORAGE I7, M2 and L153 say the rule.
   Note: the worktree has an **uncommitted** edit to `docs/STORAGE.md` that I did not make
   (I7 and M2 now add "none, when it has none or cannot be read"). It matches the code. It
   belongs to the lane; the lane should commit it or drop it.

## The questions the brief asked

- **metadata.json changed outside the app.** `listed_in_metadata` repeats whatever entry
  the file on disk has. If the other program wrote a correct entry, it stays correct. If it
  wrote a wrong one, the folder was already invalid by that edit, and the app makes no new
  claim. A foreign metadata.json is first recorded through `look_at_metadata` (D7) before the
  app writes anything (`materialize.rs:881-887`). M2 and M6 read the same file, so they agree.
  The md5 goes through `Md5::parse`, and the document still goes through `build_metadata`'s
  validation. A malformed foreign entry gives `None`, so the path is not listed; it never
  produces an invalid document.
- **metadata.json missing or unreadable.** The path is not listed. For a book, the build then
  fails (currentScope lists the book) and the group waits, so nothing is written. I probed
  this: the folder stays at "no metadata.json" and heals once the book reads. That is no
  worse.
- **metadata.json lists bytes from an even older write** (an earlier group's metadata rename
  was refused, then a later group was killed past its rename). The folder already claimed
  those bytes. The new code repeats the claim and the old code guessed `old`; both are invalid
  until a read, and neither is worse. It heals on the read.
- **Can `listed_in_metadata` make the folder worse?** Only if metadata.json's entry for the
  path were right while the bytes on disk differ from it and the app would otherwise have
  guessed right. The app cannot guess right without a read, so no. The ledger also cannot
  move `Writing` to `Placed{old}` behind the folder's back while the file is unreadable: M1
  skips an unreadable path before `WriteBegun` (`materialize.rs:771-777`), so no
  `WriteFailed` follows.
- **Heals once the file reads.** Yes, at a checkpoint (with or without a pending save) and at
  an open. Any later `metadata_entries` call also reads an undecided path again
  (`Disk::Bytes` arm), so any metadata write lists the real bytes.

## Findings

### Minor 1: a book's first write killed past its rename, while held, stalls every metadata write (older than this card)

- Scenario: the first materialization of a new book (`Writing{old: None}`) is killed after
  its rename and before metadata.json. Then another program holds the new book file. The
  folder's metadata.json has no entry for the book, so `metadata_entries` leaves it out
  (`materialize.rs:2034-2037`). currentScope from the fold lists it, so `build_metadata` fails
  for every group (`build_group_metadata`, `materialize.rs:1229+`), and no other book's file
  is written into the folder until the book reads.
- Harm: none to data. Saves stay in the journal, and commits and exchanges come from the
  projection. The folder's USFM is stale while the file is held. The old code had the same
  `continue`.
- Not probed (inferred from the code). Log in `.audit/minors.tsv` if wanted.

### Minor 2: an undecided path keeps the manifest's `nfc` flag (older than this card)

- `materialize.rs:2035` uses `e.nfc`, the NFC flag recorded for the entry. That entry may
  describe the other version. A wrong flag can only change metadata.json's document-level
  normalization claim, and the app's projection is always NFC. Before this card the path
  took `e.nfc` too. There is no reachable harm I can show.

Checked and not findings: carried, held and fixed listings; `build_tree`; exchange and the J7
zip; O10; a foreign or missing metadata.json; a stale older entry.
