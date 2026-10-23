export const meta = {
  name: 'merge-main',
  description: 'Merge one verified branch or ready/<unit> ref into claude/focused-ride-5uzj7p in a separate worktree, run the full gates in that worktree\'s own target, and leave the result for the main session to push; branches from outside the chains get an independent review and fixes first',
  phases: [
    { title: 'Review', detail: 'only for branches no chain reviewed (the Windows session\'s)' },
    { title: 'Merge', detail: 'merge in .claude/worktrees/merge-main, detached at main' },
    { title: 'Gate', detail: 'fmt, clippy, tests, wasm32 clippy of tc-ui and tc-api' },
  ],
}

// args: { root, ref, label, review: bool, context?: string }
// ref is what to merge (origin/windows-hazards, ready/S2, ...); label names it in the merge message.
const A = (typeof args === 'object' && args) || {}
const ROOT = A.root
const REF = A.ref
const LABEL = A.label || A.ref
const MAINBR = 'claude/focused-ride-5uzj7p'
const MWT = `${ROOT}/.claude/worktrees/merge-main`
const ENV = "In every shell command that runs cargo, rustc, node or npx, source ~/.tc-env first ('. ~/.tc-env && ...')."
const ATTR = "End every commit message with the line 'Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>'. Never push."
const GATES = 'scripts/gates.sh, from the worktree root with CARGO_BUILD_JOBS=6 exported; it prints "gates: all N steps passed" only when every step passed'
const CTX = `This is the Rust rebuild of translationCore 4 (tC4), a Bible translation desktop app; losing, hiding or silently rewriting a translator's work is unacceptable. AGENTS.md is the rule book. The repository is ${ROOT}. ${ENV} The machine has 20 CPUs shared with other agents' builds (cargo -j 6) and about 300 GB free. Never delete anything outside the worktree you are told to use, and never touch .claude/worktrees/storage, phase0 or p0-*: other teams work there.${A.context ? '\n\n' + A.context : ''}`

const ISSUE = { type: 'object', properties: { title: { type: 'string' }, severity: { type: 'string', enum: ['blocker', 'major', 'minor'] }, evidence: { type: 'string' }, fix: { type: 'string' } }, required: ['title', 'severity', 'evidence', 'fix'] }
const VERDICT = { type: 'object', properties: { verdict: { type: 'string', enum: ['merge', 'fix_first'] }, problems: { type: 'array', items: ISSUE } }, required: ['verdict', 'problems'] }
const GATE = { type: 'object', properties: { merged: { type: 'boolean' }, green: { type: 'boolean' }, sha: { type: 'string' }, summary: { type: 'string' } }, required: ['merged', 'green', 'summary'] }

let toMerge = REF
let reviewNote = 'no review: a chain reviewed and gated this unit before marking it ready'
if (A.review) {
  phase('Review')
  const RWT = `${ROOT}/.claude/worktrees/review-${LABEL.replace(/[^A-Za-z0-9_-]/g, '-')}`
  const LB = `review/${LABEL.replace(/[^A-Za-z0-9_-]/g, '-')}`
  let problems = []
  for (let round = 1; round <= 2; round++) {
    const v = await agent(`${CTX}\n\nYou are an independent, skeptical REVIEWER of ${REF}, a branch built by another session, before it merges into ${MAINBR}. Create the worktree if it does not exist: git -C ${ROOT} worktree add -B ${LB} ${RWT} ${round === 1 ? REF : LB}; cd there; do not commit. Read the diff (git diff ${MAINBR}...HEAD) and every commit message, and the task or notes it implements. Do not trust the messages: check each claim against the code. Revert the core of each change and confirm its tests fail, then restore. Look for: lost or silently changed translator work, files written in place instead of atomically, panics or unwrap outside tests without a proof, behavior that differs between Windows, macOS and Linux without a reason, tests that cannot fail, and conflicts with ${ROOT}/.claude/worktrees/storage (read it with git -C ${ROOT} show storage:<path>; never change it), where the storage team builds held-file retries for the project store. Run cargo test for every crate the branch touches, and clippy with -D warnings. Leave the worktree clean.${problems.length ? `\n\nThis is round ${round}; the earlier problems, which a fixer addressed, were:\n${JSON.stringify(problems, null, 1)}\nCheck that each is fixed at its root.` : ''}\n\nReturn verdict merge only if nothing blocker or major remains.`, { label: `review:${LABEL}#${round}`, phase: 'Review', schema: VERDICT })
    if (!v) return { merged: false, green: false, summary: 'reviewer died' }
    problems = v.problems
    const serious = problems.filter(p => p.severity !== 'minor')
    log(`review ${round}: ${v.verdict}, ${serious.length} blocker/major, ${problems.length - serious.length} minor`)
    reviewNote = `review round ${round}: ${v.verdict}; problems: ${JSON.stringify(problems)}`
    if (v.verdict === 'merge' && !problems.length) break
    if (round === 2 && serious.length) return { merged: false, green: false, summary: 'not merged after two review rounds', problems }
    const fx = await agent(`${CTX}\n\nYou are FIXING verified review problems on ${REF}, in the worktree ${RWT} (branch ${LB}; cd there). Problems:\n${JSON.stringify(problems, null, 1)}\nFix every blocker and major at the root, and the cheap minors; add a regression test for each fix that fails without it. If you disagree with one, explain with evidence. Run cargo fmt, clippy -D warnings and the tests of every crate you touch; commit. ${ATTR}\nFinal answer: each problem and what you did.`, { label: `fix:${LABEL}#${round}`, phase: 'Review' })
    if (!fx) return { merged: false, green: false, summary: 'fixer died' }
    reviewNote += `\nfixes: ${fx}`
    toMerge = LB
    if (v.verdict === 'merge' && !serious.length) break
  }
}

phase('Merge')
const merged = await agent(`${CTX}\n\nYou MERGE ${toMerge} into ${MAINBR}. Use the worktree ${MWT}: if it does not exist, git -C ${ROOT} worktree add --detach ${MWT} ${MAINBR}; otherwise cd there, make sure git status is clean, and git checkout --detach ${MAINBR} (the branch itself stays checked out in ${ROOT}, so work detached). Then git merge --no-ff ${toMerge} with the message 'Merge ${LABEL}: <what it brings, in plain words>' and the attribution line. Resolve conflicts carefully, keeping both sides' behavior; in Cargo.lock keep the current side and let cargo update the rest; in docs, keep both sides' text and reconcile. If the merge is already up to date, say so. Then run the full gates in this worktree, which builds in its own target folder: ${GATES}. If a gate fails because of the merge, fix the cause in a separate commit and rerun all gates. Never push. ${ATTR}`, { label: `merge:${LABEL}`, phase: 'Merge' })
if (!merged) return { merged: false, green: false, summary: 'merge agent died' }

phase('Gate')
const gate = await agent(`In ${MWT} (cd there; ${ENV}), check the merge of ${toMerge} into ${MAINBR}: git log -1 --format=%H; git status --porcelain (must be empty); git merge-base --is-ancestor ${MAINBR} HEAD (must hold); git merge-base --is-ancestor ${toMerge} HEAD (must hold). Then run: ${GATES}. Then run scripts/gates.sh --check-receipt. Report merged=true if both ancestor checks hold, green=true only if every gate passed, the tree is clean and scripts/gates.sh --check-receipt exited 0, sha = HEAD, and in summary the test counts (passed, failed, ignored) and the first 40 lines of any failure verbatim.`, { label: `gate:${LABEL}`, phase: 'Gate', schema: GATE, effort: 'low' })
if (!gate) return { merged: false, green: false, summary: 'gate agent died', mergeReport: merged }
return { ...gate, mergeReport: merged, review: reviewNote }
