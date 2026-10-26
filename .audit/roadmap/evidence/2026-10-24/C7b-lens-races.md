# C7b lens: races and protocol (track-C7b, HEAD cb7ccc7a)

Read only. `cargo test -p tc-journal -p tc-store`: all pass (properties 343 passed, 1 ignored).

## Blockers

None.

## Majors

### M1. A newer removal of a book hides the translator's edits, and the Received sheet says nothing
- Where: crates/tc-app/src/sync.rs:108-150 (`replaced`), with crates/tc-journal/src/fold/books.rs:77-117 (`status_of`).
- Events: I edit Titus verses, and the edits are on Verse keys only. Juan removes Titus later by the clock, without having seen my edits. I sync and receive the removal.
- Effect: `status_of` returns `Removed`, so my unseen edits are no longer shown. They are in History. `replaced()` looks only at the key of the received change, which is `BookPresence(Titus)`. That key has one head, the removal, so it is `Resolved::Settled`, not `SettledByRule{Newest}`, and no row is made. My edits sit on Verse keys, which `replaced()` skips (`_ => continue`). The translator's text disappears and nothing is said. H5's "no silent change goes unnoticed" fails in the most common book case. This also depends on arrival order: on Juan's side the book simply stays removed.
- The same gap applies to `KeptAsLast` (a removal the last-book rule undid) and to `RemovedWhileEdited` (a removal that a newer edit undid). Neither makes a row. Both are lower stakes.
- Fix: in `replaced()`, for a received `BookPresence` change, read the book status from the fold. Add a Book row "Juan removed Titus; your edits are in History" when the status is `Removed` and `unseen(...)` was not empty, using the count of unseen edits. Test: a scripted removal over unseen edits makes one Book row.

## Minors

### m1. A local edit or Restore of a check loses at once to a move already in the fold
- Where: crates/tc-journal/src/fold/merge.rs:316-325 (a move wins at the old place whatever its age), crates/tc-store/src/commands.rs:1501 (`restore`; decision edits behave the same). Neither refuses a check that has moved.
- Events: a page shows check Y with token [y1]. Juan's carry-over move Y->Z lands (Juan's move had y1 as parent). Using the token from before the sync, I save a decision, or Restore an older one, at Y.
- Effect: the heads become {move, mine}. The move wins although mine is newer. My decision appears only in Z's History. No Received row is made, because this is a local action and the sync that brought the move had a single head at that time. The translator saves and does not see what they saved.
- Reachability: only if a sync can land while a tool page stays open. Today sync starts from Home and E2 is not built. This becomes major if E2 adds background or USB receive while a page is open.
- Fix: in the decision, comment and restore commands, refuse with `LookAgain` when `fold.projected(Check(key))` is `MovedTo`. This costs one key.

### m2. The Received sheet repeats an old loss on every later sync of the record
- Where: sync.rs:137-180. The rows come from the key's heads after the sync, not from what the sync changed.
- Events: sync 1 receives Juan's alignment b over my a, and a row is made. Weeks later Juan realigns the verse, with b as parent. Sync 2 receives b', and a is still a losing head, so the row "Juan's alignment replaced yours" appears again.
- Effect: the counts are inflated, and the toast's `kept_newest` total grows with no new loss. Nothing is ever missed by this; the problem is repeats.
- Fix: count a loser only when it was the shown head before the sync, or when the winner is a received change that does not descend from an earlier winner over that loser. The cheapest version: compare the provisional head before and after the intake for each key.

### m3. Changes from an incomplete batch that is released later never make a Received row
- Where: sync.rs:108. Rows come only from the ids in `received`. Changes held by `IncompleteBatch` are inactive at the sync. When they become active later (the missing piece arrives on another key, or a local Release), no row is made for them.
- Effect: a teammate's newer check or alignment can replace mine with no word. It is still in History.
- Fix: compute rows from the keys whose provisional head changed during the intake or the release, not from the list of received ids.

### m4. An outside file put at a carried file's name after a removal asks a question H5 removed
- Where: crates/tc-store/src/carried.rs:242 (`parents_for_new_version`: unknown base, no parents) and merge.rs:308 (`ranks.len() > 1`, so it stays a conflict).
- Events: F has versions {a, b}. I remove F, which leaves a beside F as F-me.ext. I then copy a new recording to F.ext.
- Effect: the heads are {a, r, new(rank 0)}, which makes a CarriedFile conflict on the Settings tab. It has only the generic version cards. H5 rule 3 says carried files ask nothing. No data is lost.
- Fix: for `RecordKey::CarriedFile`, give an unknown-base outside file the shown head as its base. When the newest head is a removal, use the removal as parent, as the all-removed branch already does.

### m5. Kept-copy names move as profiles and versions arrive, and old names stay on disk
- Where: crates/tc-journal/src/fold/project.rs:1707 (newest-first numbering) and 1754 (`beside_path`). The name is `<stem>-<author>`, otherwise the short id.
- Events: (a) Juan's profile arrives after his file did, so the copy is renamed from F-3fa2b1c0.wav to F-Juan.wav. (b) A newer version from the same person comes in, so F-Juan.wav now holds the newer version and the older one moves to F-Juan-2.wav.
- Effect: the commit tree drops the old name (`kept_copy_name`), but the old file stays in the folder, so there are duplicate files. A translator who uses the folder copy to get an audio file back, which is the ruling's recovery path, finds the names shuffled. Edits are still recorded against the manifest's oid, so no wrong parent results.
- Fix: number from the oldest version, so a new version never renames an existing copy. Remove from disk a stale copy whose bytes are unchanged and are a recorded version.

### m6. The property test does not check that folds agree when they settle without a conflict
- Where: crates/tc-journal/tests/properties.rs:556 (`_ => {}`). One fold settling a key as `Merged` while another settles it as `Newest` passes. `fold_converges_under_any_union` covers the projections. The sim makes no outside edits with an unknown base, so the rank path of m4 is never exercised.

## Checked and sound
- Any arrival order: the winner is the largest (rank, at, id) of the active heads, and the move rule takes the newest move. `moves_into` is filtered by `active()` and `follow_moves` and returned as a set, so arrival order does not matter. Held batches stay inactive until complete.
- Stale token, edit or Restore: the parents are the token's heads, so a head the page never saw stays a head and is never retired. It loses only by being older and stays in History. The other computer gets a Received row when the edit arrives.
- Upgrade, carry-over and pins over two heads: `apply_upgrade` returns `PlanStale` when any key's heads changed since the plan (commands.rs:1274). `adopt_downloaded_pins` retries. Writing over the loser leaves it in History. A loser left behind by a carry move is listed in the target's History through `moved_into`, and Restore brings it to the target.
- Carried token names only the shown version: removing or editing the main file never retires a copy, or a version that arrived meanwhile. An edit on disk during a rename is recorded against the manifest's oid (carried.rs:172), and the final-look guard keeps it from being overwritten.

## FUNNEL §8 questions
1. Can a translator reach it? M1 can, with a real sync, which comes with E2; today only the scripted path exists. m1 needs a sync while a page is open. m4 and m5 can be reached now through the folder.
2. Data lost? None. Every loser stays a head, or in History.
3. What they see: M1 hides verse text with no word. m1 hides a decision the translator just saved.
4. Converges? Yes, under any order, including batches held until complete.
5. Tests: the suites pass. Gaps: m6, and no test covers a removal row (M1) or a local edit against a move (m1).
6. Fix before done: M1, with one test. The minors go to `minors.tsv`.
