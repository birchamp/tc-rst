export const meta = {
  name: 'fix-text-align',
  description: 'Finish FIX-text by making the export\'s word list the single source of the aligner\'s words (a property test proves they agree on every bundled verse and on generated USFM), then build FIX-align on it; each verified once and merged into main',
  phases: [{ title: 'FIX-text' }, { title: 'FIX-align' }],
}
const R = args.root
const ENV = "In every shell command that runs cargo, source ~/.tc-env first ('. ~/.tc-env && ...'); use cargo -j 6."
const CTX = `You work on the Rust rebuild of translationCore 4, a Bible translation app where losing, hiding or silently rewriting a translator's work is unacceptable (AGENTS.md). tC4's source is at ${args.tc4} (d310bf3, read-only; tC4 main 9ad4d5c by git show). ${ENV} End each commit message with 'Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>'. Speed matters (owner: hours, not days): delete before you add, one source of truth instead of two functions kept in step, deterministic tests instead of long reading.`
const ISSUE = { type: 'object', properties: { title: { type: 'string' }, severity: { type: 'string', enum: ['blocker', 'major', 'minor'] }, evidence: { type: 'string' }, fix: { type: 'string' } }, required: ['title', 'severity', 'evidence', 'fix'] }
const V = { type: 'object', properties: { problems: { type: 'array', items: ISSUE }, summary: { type: 'string' } }, required: ['problems', 'summary'] }
const M = { type: 'object', properties: { merged: { type: 'boolean' }, pushed: { type: 'boolean' }, sha: { type: 'string' }, summary: { type: 'string' } }, required: ['merged', 'pushed', 'summary'] }
const GATES = `cargo fmt --all -- --check; cargo clippy --workspace --all-targets -j 6 -- -D warnings; cargo test --workspace -j 6 --no-fail-fast; cargo clippy -p tc-ui --target wasm32-unknown-unknown --all-targets -j 6 -- -D warnings with no features and with --features gallery, app and test-hooks; cargo clippy -p tc-api --target wasm32-unknown-unknown --all-targets -j 6 -- -D warnings; cargo clippy -p tc-server -p tc4 --lib --bins -- -D warnings; cargo clippy -p tc-app --lib -- -D warnings`
const merge = (branch, label, title) => agent(`${CTX}\n\nMerge ${branch} into claude/focused-ride-5uzj7p and push it. The owner authorized this standing step (merge into main in a separate worktree, run the full gates there, then push main; push only claude/focused-ride-5uzj7p): when every gate is green, push without asking again. Use the worktree ${R}/.claude/worktrees/merge-main: cd there, make sure git status is clean, git fetch origin, git checkout --detach origin/claude/focused-ride-5uzj7p, git merge --no-ff ${branch} -m 'Merge ${label}: ${title}' (plus the attribution line). Gates once: ${GATES}. Fix only what the merge broke. Then git fetch origin; if origin moved, merge it and rerun the gates only if the new commits touch Rust or Cargo files; git push origin HEAD:claude/focused-ride-5uzj7p (never force); in ${R} run git merge --ff-only origin/claude/focused-ride-5uzj7p (leave local edits alone if they block it). Report merged, pushed, sha, test counts.`, { label: `merge:${label}`, phase: label, schema: M })

async function unit(label, wt, branch, task, verifyFocus) {
  const built = await agent(`${CTX}\n\nWork in ${wt} (branch ${branch}; create it with git -C ${R} worktree add -b ${branch} ${wt} claude/focused-ride-5uzj7p if it does not exist, then mkdir -p /var/tmp/tc-rst-target/${branch} and ln -s /var/tmp/tc-rst-target/${branch} ${wt}/target; if it exists, read git log and status first and merge claude/focused-ride-5uzj7p into it). ${task}\nRun the tests and clippy of the crates you touch; commit. Final answer: commits, what changed, test counts, lines added and removed.`, { label: `build:${label}`, phase: label })
  if (!built) return { ok: false, why: 'builder died' }
  let v = await agent(`${CTX}\n\nVERIFY ${label} in ${wt} (do not commit; leave it clean). Task:\n${task}\nBuilder said:\n<<<\n${built}\n>>>\n${verifyFocus} Report blockers and majors with evidence you ran; minors in one line of summary.`, { label: `verify:${label}`, phase: label, schema: V })
  if (!v) return { ok: false, why: 'verifier died' }
  let serious = v.problems.filter(p => p.severity !== 'minor')
  if (serious.length) {
    const fx = await agent(`${CTX}\n\nIn ${wt}, fix only these at the root with the smallest change (prefer removing a second code path to patching both), each with a test that fails without it:\n${JSON.stringify(serious, null, 1)}\nCommit.`, { label: `fix:${label}`, phase: label })
    if (!fx) return { ok: false, why: 'fixer died' }
    v = await agent(`${CTX}\n\nIn ${wt} (do not commit), check only these fixes deterministically (run their tests; revert each fix and see the test fail; rerun the property tests):\n${JSON.stringify(serious, null, 1)}\nFixer said:\n<<<\n${fx}\n>>>\nReport remaining or new blockers and majors only.`, { label: `check:${label}`, phase: label, schema: V })
    if (!v) return { ok: false, why: 'checker died' }
    serious = v.problems.filter(p => p.severity !== 'minor')
    if (serious.length) return { ok: false, why: 'blockers or majors remain', serious }
  }
  return { ok: true, built }
}

phase('FIX-text')
const t = await unit('FIX-text', `${R}/.claude/worktrees/app-ftext`, 'app-ftext',
  `Unit FIX-text (docs/TC4-ISSUES.md: tC4 #782 words inside nested alignments, #795 words joined after a footnote, #796 the aligner's words are the export's). The branch has fixes from earlier rounds, and its last check found two majors: an unknown marker's |attributes now show as words, and words after an unknown marker that follows a heading show in the aligner while the export drops them. Both come from keeping two word extractors in step: the shown text in crates/tc-usfm/src/derivations/shown.rs and tc_usfm::target_words for the export. Fix it at the root: make the aligner's word list (TargetVerse and anything that builds the bank) come from exactly the function the export uses, one source of truth, and delete the second extraction path where it decides words (the shown text may still render text for display, but never decides which words exist). Keep #782 and #795 fixed in that one function. Add a property test: for every verse of bundle/en (GEN and PSA at least; all books if fast) and for generated USFM with unknown markers, attributes, headings, footnotes and nested milestones, the bank's words equal the export's words exactly.`,
  'Run the new property test and the whole-book test; seed back each of the two last majors and #782, #795 and confirm a test fails; confirm only one function decides which words exist.')
if (!t.ok) return { text: t }
const mt = await merge('app-ftext', 'FIX-text', 'every word inside nested alignments and after footnotes is kept, and the aligner\'s words are exactly the export\'s (tC4 #782, #795, #796)')
if (!mt || !mt.pushed) return { text: t, mergeText: mt }

phase('FIX-align')
const a = await unit('FIX-align', `${R}/.claude/worktrees/lane-falign`, 'lane-falign',
  `Unit FIX-align (docs/TC4-ISSUES.md and docs/TC4-DELTA-2026-10-19.md §3.6 with the owner's answers in §5), on main after FIX-text: in tc-align (and tc-api where a notice crosses the wire): #787 a record reopened on changed text reports stale; #796 the target words come from the export's word list (FIX-text made that one function: use it) and one review rule holds everywhere; #794 a verse with inline alignment markup is not treated as aligned; #793 suggestion keys compare in NFC; #790 stored source cards are compared with the pinned original-language verse; #789 with Q30 (a): a reflow follows each linked word by its position, an unclear link returns to the bank and is listed, no review mark (give tc-store a function to call; its call sites are storage unit T's). Port tC4's tests for #787 and #788 from tC4 main 9ad4d5c.`,
  'Run tc-align\'s and tc-api\'s tests; for each of #787, #796, #794, #793, #790, #789 revert the fix and confirm a test fails; check no alignment link can silently move to another word or be dropped without being listed.')
if (!a.ok) return { text: t, mergeText: mt, align: a }
const ma = await merge('lane-falign', 'FIX-align', 'the aligner notices a changed verse, keeps links on the right words, and matches the export (tC4 #787, #789, #790, #793, #794, #796)')
return { text: t, mergeText: mt, align: a, mergeAlign: ma }
