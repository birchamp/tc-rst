# C7b verification lens: loss

Reviewed commits 75f7d386, ed6347e0, 5c4f9d04 and 939ce993 on `track-C7b`, read only. The
worktree also holds uncommitted WIP from another session (merge.rs: a move always wins at a check's
old key; attach.rs: `moved_into` only while the move is in force; tests). I read the WIP and noted
it below, but I judged the commits.

I did not run the tests. Three cargo test jobs were already running, two of them on tc-journal and
tc-store, and the tree holds someone else's WIP, so a run would not test the commits. Every finding
below comes from reading the code. The existing test
`an_outside_comment_edit_with_no_known_base_never_wins_and_stays_in_history` already shows the
fold behaviour behind M1.

## Blockers

None. I found no scenario where the bytes or the value of a text, alignment, decision, comment,
setting or carried file leave the change set or History.

## Majors

### M1. An outside edit of a comment, decision or alignment is hidden and the file is reverted, while its notice promises a review that resolves nothing

- **Code:**
  - `crates/tc-journal/src/outside.rs:801`: rule 10, a comment edit is always recorded with an
    unknown base. Lines 840-842: rule 9, more than 50 keys or a removal also gives an unknown base.
  - `crates/tc-journal/src/fold/meta.rs:293`: an unknown-base change has rank 0.
  - `crates/tc-journal/src/fold/merge.rs` `conflict()`: newest wins on (rank, at, id).
  - `crates/tc-journal/src/fold/resolve.rs:1266` `outside_resolution`: this only walks listed
    conflicts.
  - `crates/tc-journal/src/fold.rs:593`: `OutsideEditRecorded { unknown_base }` says "each is a
    conflict".
- **Scenario:** a translator edits a comment in `tc/comments/<BOOK>.json` in the project folder.
  Or a USFM file is changed outside the app in more than 50 places, including `\zaln`
  alignments.
- **What goes wrong:**
  - Before H5, each change was a conflict that the outside-edit review offered: *Accept all from
    the file* and the other choices.
  - Now each change is a rank-0 head that never wins. No conflict is listed.
  - The derived file is written again from the app's older version, so the person's edit
    disappears from the file they just edited.
  - The notice still says the file was recorded with an unknown base. *Accept all from the file*
  finds no conflict for these keys and does nothing.
  - The version can be found only in History, one record at a time.
  - Verse text in the same file still conflicts, so the review shows the file's words but not the
    file's alignments or comments.
- **Fix, one of these:**
  1. In `conflict()`, keep the conflict path, as before H5, when the heads include a rank-0
     External head beside a rank-1 head of a newest-wins kind. The outside-edit review then works
     as written.
  2. Make `OutsideKeep::File` also restore the batch's version of each settled newest-wins key of
     the batch, and make the Received sheet or notice list them.

  Test either fix by recording an outside comment edit, choosing *Accept all from the file*, and
  asserting that the comment is shown.

### M2. A kept copy that is no longer placed stays in the commit tree as a foreign file and comes back on other computers

- **Code:**
  - `crates/tc-store/src/checkpoint.rs:452-470` `app_owned`: only `.conflict-`/`.case-` names
    and record keys count as the app's.
  - `crates/tc-store/src/checkpoint.rs:671-673`: the foreign-entry loop.
  - `crates/tc-store/src/materialize.rs:659-675`: removes the folder copy.
- **Scenario:**
  - `audio/01.mp3` has Maria's version (newest) and Juan's version, placed beside it as
    `audio/01-Juan.mp3` and committed.
  - The translator re-records `01-Juan.mp3` in the folder. The look records it as an edit of
    Juan's version. The edit is now newest, takes `01.mp3`, and Maria's version moves to
    `01-Maria.mp3`.
  - The same thing happens when a Door43 name changes, or when a new version shifts the `-2`
    suffixes.
- **What goes wrong:**
  - Materialize removes `01-Juan.mp3` from the folder. That is the app's own file, after the
    compare-and-swap.
  - The checkpoint keeps the entry in HEAD's tree as foreign, because `-Juan` is no record key
    and no `.conflict-` name. It also lists the entry in the tree's metadata.json.
  - Door43 and exports carry a stale take named after Juan that the folder does not hold.
  - A teammate's computer records it as a new carried file. Through sync it comes back here as its
    own file, and the other names shift.
  - No bytes are lost, but a superseded audio take reappears under a person's name. The ruling
    makes that name the way translators find the other version.
  - Before H5, `.conflict-` copies were recognised and dropped.
- **Fix:** in `app_owned`, count as app-owned any path the manifest lists with an `oid` (the app
  placed it). Or have the fold expose the beside names it gave out. Test: a beside copy that stops
  being placed is in neither the folder nor the next commit's tree.

## Minors

1. **WIP only: a move at a check's old key always wins.** In the uncommitted
   `merge.rs:307-319`, a teammate's newer decision on the old key K, made without seeing the
   move, is shown nowhere. The older moved copy shows at K', and the newer decision is only in
   History(K'). That keeps no-loss, but H5 lists moved checks as newest-wins. Either the K'
   projection should take the newest of the K' heads and the decisions at K, or the Received
   sheet (step 6) must say "Juan's decision on this check is in History". The committed version
   shows both, at K (Detached on the moving computer) and at K', which is no loss.

2. **The newest head may be unreadable.**
   - Code: `merge.rs:345` `provisional_of` takes the newest head whether it is readable or not.
     `reads.rs:324` then gives an empty token.
   - Scenario: a teammate's newer version of a decision or alignment cannot be read here (for
     example, a pack is damaged).
   - The screen shows that version, which reads as nothing, for example an undecided check. The
     translator's own readable version is only in History, and editing is blocked.
   - Before H5 the same record was a listed conflict marked unreadable. Now only the count notice
     in `classify.rs:317` remains.
   - Fix: while `unreadable`, project the newest readable head.

3. **A kept copy cannot be removed.**
   - Code: `reads.rs:327` names only the shown head in the token. `carried.rs:119`: a deletion in
     the folder is no removal (I11).
   - Scenario: a translator removes `01.mp3`. `01-Juan.mp3` stays, and deleting it in the folder
     puts it back (MissingFileRebuilt).
   - Nothing is lost, but nothing on any screen can retire that version.
   - Fix (UI, step 6/7): a removal of a kept copy that names that head.

4. **Kept-copy names can move between versions.** At `project.rs:1707`, names are given newest
   first, so an older Juan version that arrives later takes `01-Juan.mp3`. The file a translator
   listened to as "Juan's" now holds another take, and the earlier one becomes `01-Juan-2.mp3`.
   The bytes are safe: the compare-and-swap and `placed_version` (which works by the entry's oid)
   keep an edit made meanwhile. Consider giving names oldest first, so names stay stable as
   versions arrive.

5. **Stale wording.**
   - `fold.rs:593`, the `OutsideEditRecorded` doc: "each is a conflict".
   - `tc-store/src/drafts.rs:19`: "a record that changed meanwhile becomes a conflict".
   - `command.rs:141`, the `MoveCheck` doc: "a decision already at `to` is a visible conflict".

   All three are false for newest-wins kinds now.

6. **Test clock.** `drafts.rs` `a_newer_alignment_draft_survives_a_crash_before_the_saves_ack`
   now passes while *Keep draft* loses to the older save. That happens only because the test
   clock restarts at reopen. With a real clock the kept draft is newest, but the test no longer
   proves that *Keep draft* shows the draft. Give the reopened handle a clock past the save's
   `at` and assert that the draft is projected.

## What holds (checked)

- **Losers stay in History.**
  - An edit or Restore after settlement names all heads, except unreadable ones (`reads.rs:338-345`).
    The losers become ancestors, and `history()` lists every active change, heads or not, so
    nothing is retired out of History.
  - Unreadable heads are never named, so they stay heads.
  - Carried files name only the shown head, so the other version stays placed.
- **Moved checks.**
  - `moved_into` plus `history()` list the old key's decisions in the new check's History and
    leave out the MovedTo forms. This works across chains.
  - `restore` writes such a version at the shown key. `outside.rs:183` filters the borrowed
    versions out of outside-edit parents.
  - The context is kept verbatim, as MoveCheck does.
- **Books.**
  - A newest removal → Removed. The unseen edits stay heads and come back with *Restore the
    text*.
  - A newer unseen edit → RemovedWhileEdited, still shown.
  - Last book → KeptAsLast. If another book is added later, an edit made meanwhile keeps the book
    as a newer unseen edit.
  - *Start empty* names every head, which is pre-existing and asked once.
- **Carried bytes.**
  - The kept-copy names avoid every carried key and every `.case-` path (`collision_form`).
  - A real file that arrives later at a kept-copy name takes the name, and the kept copy moves to
    `-2`. The compare-and-swap keeps any bytes a person put there: they are recorded as an edit
    whose parents come from the entry's oid.
  - A `.conflict-` copy in an old project is app-owned, and its blob is placed again at the new
    name.
- **Old projects.** Conflicts are derived (I1) and no ConflictId is persisted in tc-store, so
  there is nothing to migrate.

## FUNNEL §8, loss lens

1. **Code.** `newest_wins` is one predicate, and that is good. M1 needs a structural rule:
   rank 0 against rank 1 in a newest-wins kind must decide "conflict or rule" in that one place,
   not in each caller. M2 needs one source of truth for which paths the app owns (the manifest's
   oid), not name patterns.
2. **Checks.**
   - The named property tests exist, with mutation proofs in the log.
   - Missing tests:
     - *Accept all from the file* over an outside comment or alignment edit (M1).
     - A kept copy leaving the tree once it is unplaced (M2).
     - A projection that is readable when the newest head is not (minor 2).
   - `the_loser_stays_in_history` skips carried files, and no test restores a carried version.
3. **Reach.** M1 is reached by editing the project's files outside the app, a supported flow
   (§11.1). M2 is reached by re-recording a kept audio file, which is the natural action. Minors
   1, 3 and 4 are reachable through ordinary sync. Minor 2 needs an unreadable pack, which is rare.
4. **Siblings.** The same "rank 0 never wins" outcome reaches settings, pins and project details
   edited in metadata.json outside the app. `MetadataEditedOutside` and `DerivedFieldsEdited`
   cover some fields; check the rest. The same name-pattern ownership exists for `.case-`, where
   it is fine.
5. **No loss.** No bytes or values leave History. M1 and minor 2 hide a person's newest work
   behind an older version, with misleading or no words. M2 brings back a superseded file. I ran
   no crash-model run; see the note at the top.
6. **Prose and Human.**
   - The stale docs in minor 5 need updating.
   - The owner should hear about M1: "an edit made outside the app to a comment or alignment no
     longer asks and does not show". The owner accepted the default "a version from outside with
     no known base never wins", but probably without knowing that every outside comment edit has
     no known base.
