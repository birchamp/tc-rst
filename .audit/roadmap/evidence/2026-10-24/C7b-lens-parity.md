# C7b lens: browser and parity

Reviewer: independent parity lens (read only). Tree: track-C7b at cb7ccc7a, diff 26386fb2..HEAD.
tC4 reference: /home/bot/unfoldingword/tc4-ref at d310bf3.

## Blocker

B1. **A loser of newest-wins cannot be reached or restored by a translator.** H5 rests on "the
older one stays in History, and History's *Restore this version* brings it back" (ruling, rule 2;
the owner: "If it's saved in history, how will the user get them back"). In the rebuild, History
is drawn only inside a listed conflict entry (crates/tc-ui/src/views/review.rs:606, 1161-1240;
toggle at crates/tc-ui/src/app/web/review.rs:101). Restore is `KeepChoice::Restore` sent through
`conflicts/resolve` (crates/tc-ui/src/app/web/review.rs:217), and `choose` needs a `ConflictId`
and review tokens (crates/tc-app/src/conflicts.rs:598-636). `conflicts/restore` was withdrawn in
C7a (.audit/roadmap/units/C7a.md:88-94, "every History is on a record in conflict"). The store's
`Command::Restore` (crates/tc-store/src/commands.rs:223) has no API caller in tc-app. H5 removed
the conflict for checks, moved checks, alignments, comments, settings, pins and books, so none of
them has an entry, a History button or a Restore. No Check, Align, Settings or Resources screen
names its record for `conflicts/history`. The journey author says the same
(parity/app/journeys/conflict-kinds.test.mjs:8-11). Reachable: two teammates check the same
note; the older decision is gone from every screen. The card's own wrong state "a loser no screen
can reach" and INV-THE-LOSER-STAYS-IN-HISTORY ("can be restored") fail at the UI. The fold and the
store keep the data (no data loss on disk), but the translator cannot get it back.
tC4: no History at all (docs/PLAN.md:135 "tC4 has no History (#78)"); silent newest-wins
(tC4 journal/sync.mjs:26-29, G7). So the rebuild matches tC4 here, but not the ruling.
Kinds: check, moved check (the old-place decision is listed only in the new check's History,
moved_into), alignment, comment, setting/meta/versification/layout, pins, book removed while
edited: all unreachable. Carried file: reachable (the copy `<stem>-<who>.<ext>` sits in the
folder). Removed book: reachable (add the book again, H22).
Fix shape: a `conflicts/history` + restore entry point keyed by record, not by conflict (a
History button on the Check, Align and Settings screens, and on the Received rows), with the
store's `Command::Restore` behind a shown-token, now that a settled record has one head shown.

## Major

M1. **The Received sheet's "what was replaced" is computed but never shown.** `replaced` rows
(crates/tc-app/src/sync.rs:108-183, crates/tc-api/src/jobs.rs:190-193) reach no view: grep finds
no reader in crates/tc-ui. The C7b log step 7 defers them to E2. After a Door43 sync, the only
signal is tC4's count line. H5 says "That way no silent change goes unnoticed", and the card lists
"a Received sheet that says nothing about a replaced version" as a wrong state. Either draw the
rows under the sync toast (reachable now, on every Door43 sync) or record on the card that the
ruling's sentence waits for E2 and tell the owner. As it stands the Door43 sync matches tC4
(count only) and the ruling is unmet. Harms what a translator sees.

M2. **docs/PLAN.md declared differences are stale and contradict H5.** PLAN.md is untouched by
C7b. docs/PLAN.md:61-62 ("A conflict between two check decisions is shown and can be resolved")
is now false. docs/PLAN.md:265-273 (B11: the review lists alignments, checks, settings, sources,
books in tabs; "2 versions · Review" on Align and Check) describes the removed behavior.
docs/PLAN.md:133-135 (History as a choice on the conflict) needs the H5 case. Lines to add are
below. (A doc major because AGENTS.md requires each difference recorded and told to the owner.)

## Minor

m1. **The toast number can differ from tC4's for the same sync.** tC4 counts forks, one per record
key, regardless of how many heads (tC4 journal/sync.mjs:236-241; SyncResult.jsx:56 `newestWins.length`).
The rebuild sums one per losing head per key (crates/tc-app/src/sync.rs:96, 157-181). With three
concurrent versions of one check (mine and two teammates'), tC4 says "1 conflict kept the newest
version" and the rebuild says "2 conflicts". The wording ("conflicts") counts records; the doc
comment (jobs.rs:179) counts versions. Count distinct keys for `kept_newest` to match tC4. For the
two-teammate journey case (2 heads per key) both say 2.
m2. tC4 drops forks the local fold already had (`hadBefore`, sync.mjs:238-240). The rebuild
counts any key a received change touched that is settled by Newest, even if its heads were
already forked before the sync (for example a re-sent older segment whose change is now an
ancestor). Rare; same direction as m1.
m3. Rows are silently skipped when a device has no label (sync.rs:151-163). person_labels
includes `me` and every profile, so likely unreachable; a `continue` that drops a count would be
better as a fallback label.
m4. An outside-edit conflict on the Checks, Alignments or Settings tab shows the "Later" bar
("These need a choice this version of translationCore cannot make yet") and History with Restore
(crates/tc-ui/src/app/review.rs:271-287, views/review.rs:304-310). The toast no longer counts these
(`tally.others` is unused since sync.rs:96), and `review()` opens only for verse or alignment
conflicts, so a sync never points the translator at a Checks or Settings outside-edit conflict.
Home's chip shows verse kinds only.
m5. The book restore race (Restore the text vs Start empty) is on the Text tab
(crates/tc-app/src/conflicts.rs:49-51) but `choices(BookPresence)` is `Later`
(crates/tc-ui/src/app/review.rs:271), so the one book question H5 kept is answered only through
History's Restore, under a bar that says no choice can be made. tC4 has no such conflict.
m6. A path-case clash produces no Received row and no toast count (ReplacedKind has no path case);
the second file appears at a `.case-` path unannounced.
m7. STORAGE.md L35 (line 4898) still names the removed *Apply Pedro's change to the moved check*;
the H5 preface covers §8.3 but not the loss table row.

## Tabs versus tC4's review

tC4's review is one screen of verse-text conflicts, no tabs (tC4 src/views/ConflictReview.jsx:1-9).
The rebuild keeps Text (now with the book restore race), Alignments, Checks, Settings (outside-edit
conflicts only, rank differs: crates/tc-journal/src/fold/merge.rs:306-309), Detached and Notices,
shown only while one holds something (PLAN.md:268). With no outside edit, a translator after a
team sync sees tC4's review: Text only. Matches.

## Declared-difference lines to add to docs/PLAN.md

Replace docs/PLAN.md:61-62 with:

- Only verse text and story frame text conflict (owner ruling H5, 2026-10-24). A check,
  alignment, comment, setting, resource pin or book keeps the newest version with no question, as
  in tC4, and the older one stays in History, where *Restore this version* brings it back. tC4
  keeps the older one in its journal where no screen reaches it.

Replace docs/PLAN.md:265-273's first two bullets with the H5 form (the tabs hold only outside-edit
conflicts, records that lost their place and notices; no "2 versions · Review" for a teammate's
check or alignment), and add under "in the conflict review":

- The Received sheet says what a newer version replaced: "Juan's alignment of Titus 1:1 replaced
  yours. Your version is in History." tC4's sync result only counts "{n} conflicts kept the
  newest version" (H5).
- Two versions of an audio or other carried file are both kept. The newest keeps the name, the
  other is saved beside it under its author's name, `jon_01-Juan.wav` (`-2`, `-3` while the name
  is taken). tC4 does not journal `audio/` (tC4 journal/checkpoint.mjs:205) and keeps one copy (H5
  rule 3).
- A check that moved (a carry-over) keeps the move at its old place even when a teammate's
  decision there is newer; that decision is listed in the History of the check it moved to.
  Newest-wins there would bring back a check the new source no longer has. tC4 keys decisions
  without moves.
- A file whose name differs only in letter case from another keeps one name by byte order, never
  by which is newer; the other is placed beside it. A newer recording that took the name would,
  on Windows and macOS, read the other file's bytes. tC4 leaves this to git.
- An edit made to a project file outside the app, with no known base, never wins by date alone:
  it stays a conflict with the app's own version, under Checks, Alignments or Settings, until the
  person restores one from History. tC4 refuses the sync while its own file differs
  (`sync.own-file-differs`, tC4 src/views/SyncResult.jsx:15).
- A newer removal of a book removes it, and its edits stay in History; a newer edit keeps it; the
  last book of a project is never removed by a sync (H5 rules 4 and 5).

## FUNNEL §8

1. Code: the rule is one predicate (`newest_wins`, merge.rs) and an enum (`AutoRule::Newest`);
   good. Missing structure: History/Restore keyed by conflict, so a settled record has no route.
   A record-keyed restore entry point would make B1 impossible. Major-class finding (B1).
2. Checks: property tests exist and were shown to fail (log). No test covers "a translator can
   open History on a settled check and restore"; none can pass today. The toast count has no
   test against tC4's per-key count (m1).
3. Reach: every finding above is reachable by two teammates syncing, except m2/m3 (rare) and m6
   (needs two OSes).
4. Siblings: the same gap holds for every newest-wins kind; carried files and removed books are
   the only reachable ones.
5. No loss: on disk, yes (fold keeps heads; store restore tested). For the translator, no: B1.
6. Prose and Human: PLAN.md not updated (M2); the path-case change from accepted default 6 was
   told to the orchestrator, not yet to the owner as a declared difference; the Received rows
   deferral to E2 is not told to the owner.
