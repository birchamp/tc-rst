# Gap: tc4-straggler-sync-safety

## Summary
The STORAGE §13.3 claim holds for the normal case, but its wording is too broad and one state escapes it. The sync code is the same at all three commits: syncOperation.ts, verify.ts, checkpoint.mjs, opsLog.ts and serverApi.ts have the same blob ids at d310bf3, a5ce0307 and b86270f. In sync.mjs only the alignReplaced facts differ. When a tC4 straggler presses Sync on a Bible project in a moved repository whose default branch is main, tC4 refuses `sync.verify-failed` before it pushes (syncOperation.ts:654; push at :664). It writes its own journal and derived files into a scratch clone, commits there, and then runs verify. Verify lists every non-hidden file under ingredients/ through pankosmia's WalkDir (get_repo_file_paths.rs:48-55) and marks `tc/project.json`, `tc/format.json` and `tc/changes/**` as 'extra', because they are not projected (verify.ts:268-287). It then deletes the scratch clone. The straggler's project and outbox are not touched. The other states:
- main exists but is not the default branch: `sync.not-main` (:255).
- Private repository: the clone fails because it sends no credentials. The code is inferred to be `sync.clone-failed`.
- Audio is never read, so the tc/** files still cause the refusal.
- A binary carried file outside audio/ fails the read first (`sync.failed`).

The exception: when Door43 has no `main` branch at all, tC4's Home flow does not sync. `hasBranch('main')` answers 404, so it opens the Upload dialog and pushes the straggler's local branch with no verify (state.jsx:6514-6517; shareOperation.ts:172). The inference is that this creates `main` on Door43. It overwrites nothing, but the straggler is told the upload succeeded, and the team never sees the work. Reaching this state takes an owner's action on Door43. No tC4 test and no rebuild test or fixture covers a moved repository. Unit X is still to do.

# tC4 straggler and a moved Door43 repository: what tC4 does

Paths: TC4 = `scratchpad/ext/tc4-tip` (git). LANE = `scratchpad/snap/resume-build-lane`. Line numbers are at b86270f unless a row says otherwise.

## 0. Versions: the code is the same at all three commits

| File | d310bf3 | a5ce0307 (tag v4.0.0-rc.2) | b86270f |
|---|---|---|---|
| src/data/share/syncOperation.ts | bd45b8ef | bd45b8ef | bd45b8ef |
| src/data/journal/verify.ts | 0ab9381a | same | same |
| journal/checkpoint.mjs | 25c837b2 | same | same |
| src/data/journal/opsLog.ts | 2f00f258 | same | same |
| src/data/serverApi.ts | 9d6515be | same | same |
| src/data/share/shareOperation.ts | a2b05e49 | same | same |
| src/data/share/door43Api.ts, src/views/Home.jsx, src/data/share/syncs.ts, src/data/openCopyMarks.ts | same | same | same |
| journal/sync.mjs | 6d0929a6 | b1838170 | b1838170 |
| src/state.jsx | e6268693 | 3ff11cb4 | 3ff11cb4 |

- The diff from d310bf3 to a5ce0307 in sync.mjs adds only the `alignReplaced` facts (D108). The rest of that diff is the 8 MiB segment limit in seal.ts and files.mjs.
- `git diff --stat a5ce0307 b86270f -- src/data/share src/data/journal journal` is empty.
- In state.jsx, only the line numbers move: `mainOnDoor43` is at 139-149 at d310bf3 and at 145-155 at tip.
- **The labels are off.** d310bf3 (2026-10-16) is *not* the tag v4.0.0-rc.1. That tag is 6f878aad (2026-10-02), and it has no syncOperation.ts at all (`git cat-file -e` fails). a5ce0307 sits on release/4.0 and is not an ancestor of b86270f (`merge-base --is-ancestor` fails).

## 1. The code path for a Bible straggler pressing Sync

1. **Home decides between sync and upload** (state.jsx:145-155, :6514-6517).
   - It calls `door43.hasBranch(…,'main')`. Any error counts as true.
   - If Door43 has `main`, it runs the sync.
   - If Door43 answers 404, it opens the Upload dialog instead (`if (shared) return a.openUpload`).
2. **`sync()` clones Door43 main into the scratch path** (syncOperation.ts:620-621). The clone sends no credentials (pankosmia a83725b clone_repo.rs:106 `Repository::clone`).
   - If the clone's HEAD is not `main`, it refuses `sync.not-main` (:252-256).
3. **It reads every remote file** except `audio/`, `tc4-sync/` and `tc4-open/` (:623 → opsLog.ts:341, :346-357), one text read per file.
   - A file that is not UTF-8 gives HTTP 400 (pankosmia raw_text_ingredient.rs:31, :51-55). That error is not a Refusal, so the report becomes `sync.failed` (:567-574).
4. **`syncCore` builds what to write.**
   - The remote has no journal, so G1 and G2 pass trivially (sync.mjs:160-174).
   - `write` = the straggler's derived projection, plus each of its own journal files ("carried"), plus each teammate journal file it holds ("restored") (sync.mjs:221-222; :216-217 at d310bf3).
   - `remove` = remote USFM and alignment/tW/tN sidecars that its fold does not project (sync.mjs:226; checkpoint.mjs:212-215).
   - `tc/**` is never in `remove`.
5. **It writes, removes, remakes and commits, in the scratch copy only** (:644-650).
6. **It verifies** with `skipUnjournaled: true` (:652).
   - The disk set is every path that `/burrito/paths` lists, minus the journal, `LICENSE.md`, `content/front|back/` and `audio/` (verify.ts:135-148; runtime.ts:169-170).
   - `/burrito/paths` lists every file under ingredients/ except paths with a dot segment and `*.bak` (get_repo_file_paths.rs:48-55).
   - A listed path with no projection is 'extra' (checkpoint.mjs:263-275; verify.ts:273-287).
   - `tc/project.json`, `tc/format.json`, `tc/changes/**` and `tc/comments/*` therefore make `ok=false`.
   - It throws `sync.verify-failed` (:653-659).
7. **The push (:664) is never reached.** The catch deletes the scratch copy (:690).
   - The project folder is written only by `applyScratch` after the push (:763).
   - The outbox is cleared only at :780.
   - `sealName` seals without writing (journalStore.ts:504-524).
   - So the local work stays as it was.

The straggler sees: "The synced copy did not pass its check. Nothing was sent to Door43. Ask for help." (en.json:1002). It is not told to update.

## 2. (a) Verdict table

All three columns follow the identical code above. Cells that cite state.jsx give d310bf3 / tip line numbers.

| Remote state | d310bf3 | a5ce0307 | b86270f | Straggler's local work |
|---|---|---|---|---|
| (1) Moved repository as the §13.3 merge commit leaves it: Bible, `main` is the default, public, `ingredients/tc/**` present, no `checking/journal/` | **Refuses `sync.verify-failed`** syncOperation.ts:654. Mismatches include `tc/format.json` and `tc/project.json` as 'extra'. No push. | same | same | Kept. Project and outbox untouched (:763, :780). |
| (2) As (1), and the straggler holds unsynced tC4 events | Same refusal. The events are written to the scratch copy only, as carried files (sync.mjs:216 at d310bf3). | same | same | Kept. |
| (3a) Default branch is not `main`, but a `main` branch exists | hasBranch → true → sync → clone HEAD ≠ main → **`sync.not-main`** :255. No push. | same | same | Kept. |
| (3b) Default branch is not `main`, and **no `main` branch exists** | hasBranch → 404 → Upload dialog (state.jsx:6098). "Upload changes" → `share()` push, with **no verify** (shareOperation.ts:172). pankosmia pushes `refs/heads/<local HEAD>` with no force (push.rs:110-121). **Pushes.** Inference: this creates `main` on Door43 holding the straggler's whole local tC4 tree (journal, derived files, metadata.json). The team's branch is untouched. | same (state.jsx:6517) | same | Kept locally. The pushed copy is invisible to the rebuild, which fetches only `refs/heads/{b}` (LANE STORAGE.md:3257). The straggler is told it succeeded. Its next Sync stops at `sync.not-main`. |
| (3c) Private repository | If hasBranch → 200 (the straggler is a collaborator): the sync's clone has no credentials and fails. Inferred code `sync.clone-failed` (:162, thrown at :234). If hasBranch → 404: an upload push that Door43 refuses (code not traced). No push lands. | same | same | Kept. |
| (4a) Moved repository carrying audio under `ingredients/audio/` | Audio is never read (opsLog.ts:341; verify.ts:139). Still **`sync.verify-failed`** because of tc/**. | same | same | Kept. |
| (4b) Carried binary outside `audio/` (image, PDF, a binary `.conflict-` copy) | The remote read gives HTTP 400 at :623 → **`sync.failed`**. No push. | same | same | Kept. |
| Extra: moved OBS repository | No sync for OBS (syncs.ts:8-9). Upload pushes, and the push is not a fast-forward → **`share.non-fast-forward`** shareOperation.ts:206-208. If there is no `main` → as (3b). | same | same | Kept. |
| Extra: a 4.x teammate uses "Open from Door43" | **`open-door43.no-journal`** :347-349 | same | same | Not applicable. |
| Extra: the real rc.1 tag 6f878aad | No sync. Home always shows "Upload changes" (Home.jsx:85-90 at 6f878aad). The push is not a fast-forward → `share.non-fast-forward`. If there is no `main` → as (3b). | – | – | Kept. |

**What tC4 would push if verify ever passed.** This is counterfactual: it needs every tc/** file and every rebuild-only path gone from Door43 main, and that would take deleting them by hand.
- It would add the straggler's whole journal under `checking/journal/**`.
- It would overwrite every USFM file and sidecar with the straggler's older fold.
- It would remove the books that the moved team added.
- The rebuild's next sync would then classify the repository as a tC4 team repository again (N1, LANE STORAGE.md:3281), not run the Bridge on it.

## 3. (b) tC4 tests

**No tC4 test syncs against a repository with no journal plus non-derived files under ingredients/.** These are the closest:
- test/journalVerify.test.ts:77-85 (blob 0259f663 at all three commits): an underived `checking/extra.json` gives an 'extra' mismatch and `ok=false`. It runs verify alone, without `skipUnjournaled`, but that option affects only audio/.
- test/sync/syncOperation.test.ts:311-324: `sync.verify-failed` pushes nothing (remote `version` stays 1). It triggers the failure with a garbled write, not an extra file.
- test/sync/syncOperation.test.ts:693-702: `sync.not-main`. It calls `sync()` directly, so it skips Home's hasBranch step.
- test/sync/syncOperation.test.ts:806-825: audio on Door43 is tolerated and the sync is ok.
- test/sync/syncCore.test.ts:120-148: a remote that lost a teammate journal file → `restored`.
- test/syncShareGuard.test.tsx:174-188 (blob 42b9bd35 at all three): with Door43 main absent, Share or Sync runs the upload push. This is the (3b) path.
- e2e/sync-refusal.spec.ts is about #801 (`sync.project-replaced`). e2e/j26-team-sync.spec.ts and e2e/sync-button.spec.ts only check that `refs/heads/main` exists.
- conformance/validate-transport.mjs tests the PROPOSED long-term transport (T4), not round 1.

## 4. (c) Rebuild coverage

**Nothing would catch a change in tC4's behavior.**
- Unit X ("Door43 sync and USB exchange") is still "to do" (LANE .audit/STATUS.md:20).
- No crate implements the move, the §13.1 import or the merge commit. A grep for `checking/journal` in LANE crates hits only tc-app seed code. No tc-sync or tc-import crate exists in either lane or s2.
- `SyncBlock::Tc4TeamRepository` exists only as an API enum value and a message (tc-api/src/refusal.rs:405-406).
- crates/tc-api/tests/refusals.rs:122 lists `sync.verify-failed` as a code the rebuild drops for its *own* sync.
- The parity notes record that tC4 refused `sync.verify-failed` when Door43 main had no journal (LANE parity/tc4-baseline/README.md:553-556; parity/app/env-tc4.mjs:180-182). The cause was not traced, and that tree was not the moved shape.
- The parity harness could host a straggler scenario: tC4 is pinned at d310bf3 (README.md:10), and the mock server serves clone (server.mjs:367) and push (:357). That harness's capture needs a tC4 clone, so it is not in CI.

## 5. (d) Guards to add (proposals) and the §13.3 sentences to correct

Even without the (3b) push, guards are worth adding, because the safety rests on one condition. Door43 main must hold at least one regular file under `ingredients/` whose path:
- has no dot segment and does not end in `.bak`;
- is outside `audio/`, `checking/journal/`, `tc4-sync/`, `tc4-open/`, `LICENSE.md` and `content/front|back/`;
- is a path tC4 never projects.

`ingredients/tc/project.json` meets this in every tree, because §9.4 always writes "the local file" there (LANE STORAGE.md:3341).

- **P1 (CI-able, needs no tC4).** A tc-store/tc-sync test that every commit tree `build_tree` makes, each chain commit of §9.5 included, contains `ingredients/tc/project.json` and `ingredients/tc/format.json` as regular files with no dot segment. The proof that tC4 refuses such a file is test/journalVerify.test.ts:77-85 together with checkpoint.mjs:263-275 and get_repo_file_paths.rs:48-55.
- **P2 (evidence run, repeated whenever the tC4 reference moves).** A parity/tc4-baseline scenario:
  - seed the mock Door43 main with a moved-shaped tree;
  - press Sync in tC4;
  - assert `sync.verify-failed`, no `/git/push/` request, Door43 main unchanged, and the local project unchanged;
  - add one variant with no `main` branch, which should show the (3b) push.
- **P3 (for 3b).** At each sync of a moved repository, read `origin.list()`, which the rebuild already calls at Confirm (LANE STORAGE.md:3437). If `refs/heads/main` exists, b ≠ main, and its tree holds `ingredients/checking/journal/` segments not yet converted, fetch it and run the Bridge. Show "Juan still uses translationCore 4; ask them to update."

**§13.3 sentences that are false or too broad** (LANE STORAGE.md; the text is identical in s2 at :4493+, storage at :4427+ and main at :3995+):
- :4014-4015, "a tC4 4.x sync of a moved repository refuses before it pushes, so it overwrites nothing". False for (3b): tC4 pushes a new `main`. Inference: "overwrites nothing" still holds.
- :4027-4029, "tC4's round-1 sync refuses a repository with files it does not derive under `ingredients/` (`sync.verify-failed`)". Too broad:
  - audio/, LICENSE.md, content/front|back/, dot paths and *.bak are tolerated;
  - tc4-sync/ and tc4-open/ are deleted;
  - binary files give `sync.failed`;
  - a non-main default branch gives `sync.not-main`;
  - a private repository gives a clone failure;
  - with no `main` branch, Home uploads instead of syncing.

## 6. Side note (outside this dimension)

tC4 writes each action into a staging area in IndexedDB before it writes the segment file, and deletes the staged copy after the write. A crash between those two steps leaves the action only in IndexedDB (journalStore.ts:655-675, :700-712). The next tC4 open of the project writes it into the folder (journalingStore.ts:1102). The §13.1 import reads only files (T2). So a straggler who updates right after such a crash, without opening the project in tC4 first, would lose that one action. This is an inference; STORAGE §13 does not mention tC4's staging area.

## Findings
- [F1] (high/high) At d310bf3, a5ce0307 and b86270f, a tC4 Bible sync of a moved repository (main is the default, tc/** present, no journal) refuses sync.verify-failed before it pushes, and the straggler's project and outbox are untouched.
  - evidence: TC4 b86270f src/data/share/syncOperation.ts:644-659: write and remove go to the scratch copy, then verify, then throw sync.verify-failed; push is at :664; TC4 verify.ts:135-148 and :268-287: listed non-journal, non-audio paths with no projection are kind 'extra', so ok=false; pankosmia a83725b src/endpoints/burrito2/get_repo_file_paths.rs:48-55: WalkDir lists every file under ingredients/ except dot segments and *.bak, so tc/format.json is listed; TC4 journal/checkpoint.mjs:263-275 classifyDivergence: a disk path absent from the projections is diverged; TC4 syncOperation.ts:690 deletes the scratch copy; the project is written only by applyScratch at :763; the outbox is cleared only at :780; LANE crates/tc-journal/src/format.rs:16 FORMAT_PATH = ingredients/tc/format.json; crates/tc-store/src/layout.rs:21 PROJECT_JSON = ingredients/tc/project.json
  - rec: Keep §13.3's core claim, but make the condition it rests on an explicit, tested rebuild invariant (F3).
- [F2] (medium/high) The sync code is the same at all three commits: syncOperation.ts, verify.ts, checkpoint.mjs, opsLog.ts, serverApi.ts, shareOperation.ts, door43Api.ts and Home.jsx have the same blob ids; sync.mjs differs only in the alignReplaced facts.
  - evidence: git rev-parse <c>:src/data/share/syncOperation.ts gives bd45b8ef at d310bf3, a5ce0307 and b86270f; verify.ts gives 0ab9381a at all three; checkpoint.mjs gives 25c837b2 at all three; git diff d310bf3 a5ce0307 -- journal/sync.mjs: only alignReplaced (D108); git diff --stat a5ce0307 b86270f -- src/data/share src/data/journal journal: empty
  - rec: Verdicts can be stated once for all three commits. Re-check when the reference moves to the v4.0.0 tag.
- [F3] (high/high) The refusal depends on a single condition: Door43 main must hold a regular file under ingredients/ with no dot segment, not ending in .bak, outside audio/, checking/journal/, tc4-sync/, tc4-open/, LICENSE.md and content/front|back/, that tC4 never projects. ingredients/tc/project.json meets it in every tree the rebuild builds.
  - evidence: TC4 verify.ts:138-139 skips journal, OBS base and audio; runtime.ts:169-170 isObsBaseIngredient; openCopyMarks.ts:23-25 reserved folders, deleted before the move (syncOperation.ts:620-621); LANE docs/STORAGE.md:3341: ingredients/tc/project.json is always 'the local file' in build_tree; TC4 test/journalVerify.test.ts:77-85 proves an underived checking/extra.json is 'extra' and ok=false
  - rec: Proposal: add a tc-store/tc-sync test that every build_tree commit, chain commits included, contains ingredients/tc/project.json and ingredients/tc/format.json as regular files with no dot segment.
- [F4] (medium/medium) When the moved repository has no `main` branch, tC4's Home does not sync. It opens the Upload dialog and pushes the straggler's local branch with no verify. Inference: this creates `main` on Door43 with the straggler's tC4 tree, overwrites nothing, tells the straggler it succeeded, and stays invisible to the rebuild.
  - evidence: TC4 b86270f src/state.jsx:145-155 mainOnDoor43/syncsNow: hasBranch(...,'main') 404 means do not sync; :6514-6517 if (shared) return a.openUpload (d310bf3 :139-149, :6095-6098); TC4 shareOperation.ts:172 api.push with no verify; pankosmia a83725b src/endpoints/git2/push.rs:110-121 pushes the HEAD ref name, no force; TC4 test/syncShareGuard.test.tsx:174-188 (blob 42b9bd35 at all three): with Door43 main absent, Share or Sync runs the upload push; LANE docs/STORAGE.md:3257 fetches only +refs/heads/{b}
  - rec: Correct STORAGE.md:4014-4015. Proposal: at each sync of a moved repository, check origin.list() for a refs/heads/main that differs from b and holds tC4 journal segments, run the Bridge on it, and show the 'ask them to update' notice.
- [F5] (low/high) A moved repository whose default branch is not main while a main branch still exists is refused with sync.not-main before any read or push.
  - evidence: TC4 syncOperation.ts:247-257 (listBranches is_head != 'main' throws sync.not-main); TC4 test/sync/syncOperation.test.ts:693-702
  - rec: List this as a separate refusal code in §13.3 rather than under sync.verify-failed.
- [F6] (low/medium) A private moved repository cannot be cloned by tC4: the clone sends no credentials. The sync fails before any push, with the inferred code sync.clone-failed.
  - evidence: TC4 serverApi.ts:536-545 says the clone sends no credentials; pankosmia clone_repo.rs:106 Repository::clone(&url, local_path); TC4 syncOperation.ts:157-163 refusalForClone; thrown at :234; LANE docs/PLAN.md:24 and :51 list private repositories as a rebuild extension
  - rec: Confirm the exact code in the parity scenario with a private repository in the mock, or against Door43 QA.
- [F7] (low/high) Carried audio does not change the verdict: tC4 never reads audio/, so tc/** still causes sync.verify-failed. A carried binary file outside audio/ makes the remote read fail first, with sync.failed.
  - evidence: TC4 opsLog.ts:341 and :346-357 (inApplyTree excludes audio; readIngredient per file); verify.ts:139; TC4 test/sync/syncOperation.test.ts:806-825: audio on Door43 alone gives an ok sync; pankosmia raw_text_ingredient.rs:31 and :51-55: read_to_string failure gives HTTP 400; TC4 syncOperation.ts:567-574 withCode gives sync.failed
  - rec: Note in §13.3 that audio is tolerated and that the refusal comes from tc/**, never from carried files.
- [F8] (low/high) tC4 OBS projects and the real v4.0.0-rc.1 tag do not sync; they upload. Against a moved repository with main present, the push is not a fast-forward and is refused with share.non-fast-forward.
  - evidence: TC4 src/data/share/syncs.ts:8-9 (OBS never syncs); Home.jsx:62-66; TC4 shareOperation.ts:206-208 maps NotFastForward to share.non-fast-forward; :22-24 says the remote is left unchanged (verified on their rig); git show 6f878aad:src/views/Home.jsx:85-90 always shows Upload changes; 6f878aad has no syncOperation.ts
  - rec: Add an OBS row and a 'no main branch' row to §13.3's straggler bullet.
- [F9] (medium/high) No tC4 unit, e2e or conformance test syncs against a repository with no journal and non-derived files under ingredients/. sync-refusal.spec.ts covers #801 (sync.project-replaced), not foreign repositories.
  - evidence: TC4 e2e/sync-refusal.spec.ts:1-16 header (#801); test names at :176-514; grep of test/sync, e2e/j26-team-sync.spec.ts and e2e/sync-button.spec.ts for format.json, ingredients/tc and extra finds no such case; Closest: test/journalVerify.test.ts:77-85; test/sync/syncOperation.test.ts:311-324
  - rec: Do not rely on tC4 to keep this behavior; prove it from the rebuild side (P2 parity scenario).
- [F10] (high/high) No rebuild test or fixture would detect a change in tC4's straggler behavior: unit X is still to do, and no crate implements the move or the merge commit.
  - evidence: LANE .audit/STATUS.md:20 'X | Door43 sync and USB exchange | to do'; LANE crates: no tc-sync or tc-import crate; checking/journal appears only in tc-app/src/backend/seed.rs and tests; LANE crates/tc-api/src/refusal.rs:405-406: Tc4TeamRepository is only an enum value; tc-api/tests/refusals.rs:122 lists verify-failed as a dropped code for the rebuild's own sync; LANE parity/tc4-baseline/mock-server/server.mjs:357 and :367 serve push and clone; README.md:10 pins tC4 at d310bf3
  - rec: Proposal: add a parity/tc4-baseline straggler scenario (Sync against a moved-shaped mock main; assert sync.verify-failed, zero /git/push/, main unchanged), with a no-main variant; re-run it at each new tC4 reference.
- [F11] (medium/high) The STORAGE §13.3 sentence at :4027-4029 is too broad and names one code for several outcomes.
  - evidence: LANE docs/STORAGE.md:4027-4029 text; Tolerated classes: TC4 verify.ts:138-139; runtime.ts:169-170; Other codes: syncOperation.ts:255 (not-main), :162/:234 (clone-failed), :567-574 (sync.failed); state.jsx:6517 (upload path)
  - rec: Reword it to: 'tC4 4.0 refuses a sync of a moved repository before it pushes, because ingredients/tc/** is not a file its fold derives (sync.verify-failed). With main not the default it refuses sync.not-main. When Door43 has no main branch, tC4 offers Upload changes instead, which pushes a new main branch that the rebuild does not read.'
- [F12] (low/high) After a sync.verify-failed refusal, the straggler is told 'The synced copy did not pass its check. Nothing was sent to Door43. Ask for help.' It is not told to update.
  - evidence: TC4 src/i18n/en.json:1002; src/views/SyncResult.jsx:29; LANE crates/tc-ui/assets/i18n-rebuild/en.json:151 (the mover's message)
  - rec: Keep the move prompt telling the mover that teammates on tC4 must update (LANE STORAGE.md:4020-4021). No tC4-side change is possible.
- [F13] (low/medium) An action that a crash leaves only in tC4's IndexedDB staging area is not in the project folder. The §13.1 import reads only files, so a straggler who updates before reopening the project in tC4 could lose that one action. This is an inference and outside this dimension.
  - evidence: TC4 src/data/journal/journalStore.ts:655-675 (stage) and :700-712 (stage before the write); journalingStore.ts:1102 (replayStaged at open); LANE docs/STORAGE.md:3917-3940 (§13.1 T1-T2 read files only); grep of STORAGE.md for 'outbox' finds only L17 at :4195
  - rec: Hand this to the import (unit I) planner: open the project once in tC4 before import, or document the limit.
- [F14] (low/high) The task's labels are off: d310bf3 is not the tag v4.0.0-rc.1 (6f878aad, 2026-10-02, which has no sync), and a5ce0307 (v4.0.0-rc.2) is on release/4.0 and is not an ancestor of b86270f.
  - evidence: git -C TC4 rev-parse v4.0.0-rc.1^{} = 6f878aad; git cat-file -e 6f878aad:src/data/share/syncOperation.ts fails; git merge-base --is-ancestor a5ce0307 b86270f fails; git tag --contains a5ce0307 = v4.0.0-rc.2
  - rec: In plans, call d310bf3 'the reference commit (package 4.0.0-rc.1)', not 'the rc.1 tag'.

## Unverified
- No run of tC4 against a moved-shaped tree: running npm/node was forbidden, so every verdict comes from reading the code. Confirm with the proposed parity/tc4-baseline scenario (mock server.mjs serves clone and push).
- (3b): that pankosmia's push of refs/heads/main to a Door43 repository without main creates the branch rather than being refused, and that a tC4 project's local branch is named main. Check with a run against the parity mock or Door43 QA, and with `git -C <tC4 project> symbolic-ref HEAD`.
- Reachability of (3b) and (3a): whether a Door43 (Gitea) owner can rename or delete main and change the default branch through the web UI. The rebuild itself only creates main (LANE crates/tc-door43/src/client.rs:531). Check in Door43 QA repository settings.
- (3c): the exact refusal code for a private repository (sync.clone-failed is inferred from refusalForClone at syncOperation.ts:157-163), and what an upload push to a private repository without access returns. Check against Door43 QA.
- Why tC4 refused sync.verify-failed when Door43 main had no journal in the parity capture (LANE parity/tc4-baseline/README.md:553-556). The cause was not traced.
- Whether tC4 builds after b86270f (4.0.x or 4.1) keep this verify. Only re-running the parity scenario at each new reference can show it.
- Whether every chain commit (STORAGE §9.5) and the first move commit carry ingredients/tc/project.json. This is inferred from the §9.4 table (LANE STORAGE.md:3341); no code exists yet to check.
- The IndexedDB staging-area loss path (F13) needs a crash between stage and write. Not reproduced.
