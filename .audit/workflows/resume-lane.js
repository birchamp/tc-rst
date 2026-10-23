export const meta = {
  name: 'resume-lane',
  description: 'Finish C3 in the resume-build-lane worktree, build C4, B11, B8, C7a and C7b in lanes inside it, verify once with the owner fix rule, merge each into resume-build-lane, and run one final full gate',
  phases: [
    { title: 'Build', detail: 'C3 finishes in the worktree; other units build in lanes under target/lanes as their dependencies land' },
    { title: 'Verify', detail: 'one pass; fix blockers and majors that harm data or what a translator sees' },
    { title: 'Merge', detail: 'serialized merges into resume-build-lane with focused checks' },
    { title: 'Gate', detail: 'one full repository gate on the finished batch' },
  ],
}

const A = args
const W = '/home/bot/tc-rst/.worktrees/resume-build-lane'
const BR = 'resume-build-lane'
const TC4 = '/home/bot/unfoldingword/translationcore4'
const laneOf = (u) => `${W}/target/lanes/${u.key}`
const branchOf = (u) => `rl-${u.key}`

const ENV = `In every shell that runs cargo, rustc, node or npx, start with '. ~/.tc-env && export CARGO_NET_OFFLINE=true' and use cargo -j 6 (the machine has 20 CPUs shared by several builds). Browser journeys need PLAYWRIGHT_MODULE=/home/bot/tc-rst/parity/tc4-baseline/node_modules/playwright, TC4_BIN=target/debug/tc4 (cargo build -p tc4 --features test-hooks,fixture-backend), TC4_UI=target/tc-ui-app (CARGO_JOBS=6 crates/tc-ui/build-app.sh --out target/tc-ui-app; add --test-hooks --out target/tc-ui-app-hooks and TC4_UI_HOOKS for journeys that need hooks) and TC4_DOOR43_FAKE=target/debug/tc-door43-fake (cargo build -p tc-door43-fake), all relative to the worktree you work in; ${W}/.enjoy-logs/resume-full-gates.sh shows the whole set. Write long logs under the worktree's .enjoy-logs/ (ignored).`
const RULES = `Boundaries (owner, this conversation): work only inside ${W} and the lane worktrees under ${W}/target/lanes/. Never touch /home/bot/tc-rst (the main checkout), anything under /home/bot/tc-rst/.claude/worktrees/, the storage, s2-verify or claude/focused-ride-5uzj7p branches, or another session's work (an S2 storage session runs separately). Never push, fetch or fast-forward main. Never commit parity/tc4-baseline/node_modules (a machine-local link). Never use git stash. Tests never touch the network.`
const REACH = A.reach
const HARM = `Give each finding its harm: data when a translator's text, alignment, check decision or comment could be lost, hidden or saved as something else; user when a translator would see a wrong result, a crash, a dead end or a screen that differs from tC4 without a declared reason; internal for anything a translator would not notice (a missing or weak test, a design text, a timing not recorded, code shape). Owner rule (2026-10-20): only blockers, and majors whose harm is data or user, are fixed; everything else is logged as a minor.`
const ATTR = `End every commit message with the line 'Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>'.`
const COMMON = `You work on the Rust rebuild of translationCore 4 (tC4), a Bible translation desktop app; losing, hiding or silently rewriting a translator's work is unacceptable. AGENTS.md is the rule book ("Matching tC4": where tC4 works against what a feature is for, follow what the translator needs and record the difference). docs/APP-DESIGN.md is the design: read "Owner's answers", your unit's row in §10 and the sections it cites. docs/TC4-ISSUES.md and docs/TC4-DELTA-2026-10-19.md (owner's answers in §5) list tC4 issues per unit: the items naming your unit are part of your task. .audit/plans/RESUME-2026-10-20.md records how this continuation works and what each finished unit proved. tC4's source is at ${TC4} (d310bf3, read-only). ${ENV} ${RULES} Speed matters (owner: hours, not days): reuse code, tests and harnesses that exist, prefer the simplest change, delete before adding, prove behavior with deterministic tests rather than long reading, and run focused tests while you work (the final gate runs the whole workspace once). ${REACH} ${ATTR}`

const ISSUE = { type: 'object', properties: { title: { type: 'string' }, severity: { type: 'string', enum: ['blocker', 'major', 'minor'] }, harm: { type: 'string', enum: ['data', 'user', 'internal'] }, evidence: { type: 'string' }, fix: { type: 'string' } }, required: ['title', 'severity', 'harm', 'evidence', 'fix'] }
const VERDICT = { type: 'object', properties: { problems: { type: 'array', items: ISSUE }, summary: { type: 'string' } }, required: ['problems', 'summary'] }
const BUILT = { type: 'object', properties: { complete: { type: 'boolean' }, head: { type: 'string' }, report: { type: 'string' }, owner_question: { type: 'string' } }, required: ['complete', 'head', 'report', 'owner_question'] }
const MERGED = { type: 'object', properties: { merged: { type: 'boolean' }, sha: { type: 'string' }, summary: { type: 'string' } }, required: ['merged', 'sha', 'summary'] }
const GATE = { type: 'object', properties: { passed: { type: 'boolean' }, sha: { type: 'string' }, rust_tests: { type: 'string' }, journeys: { type: 'string' }, summary: { type: 'string' } }, required: ['passed', 'sha', 'rust_tests', 'journeys', 'summary'] }

const NOTES = A.notes || {}
const noteFor = (k) => NOTES[k] ? `\n\nNotes from the orchestrating session (the last run was cut off by a usage limit, not by a failure):\n${NOTES[k]}` : ''
const fixable = (p) => p.severity === 'blocker' || (p.severity === 'major' && p.harm !== 'internal')
const minors = []
const ownerQuestions = []

let chain = Promise.resolve()
function serial(fn) { const p = chain.then(fn); chain = p.catch(() => null); return p }

// C3 was built and given its three-lens review in this worktree by the previous agent; it only
// needs its proof finished, the gate and its commits.
const c3 = A.c3done ? Promise.resolve(A.c3done) : (async () => {
  const r = await agent(`${COMMON}\n\nYou FINISH unit C3 (comments and OBS saves, a no-loss unit) in ${W} on branch ${BR}. Its code is written and uncommitted there (git status shows it). Read .audit/plans/C3.md and .audit/plans/CONTINUE-REBUILD-2026-10-20.md first: the three-lens review is done and its findings fixed; do not start another broad review. The native focused tests pass (.enjoy-logs/continue-c3-native-final-focused.log). The browser proof is unfinished: the last run of parity/app/journeys/story-writes.test.mjs with story.test.mjs passed 7 of 8 (.enjoy-logs/continue-c3-ui-proof.log); .enjoy-logs/continue-c3-ui-proof.sh is the proof script, which also recaptures C3's screens with node parity/app/capture-rust.mjs --unit C3. Steps: (1) make the failing journey pass by fixing the product when the product is wrong (a translator's text must never be lost or hidden), or the test when the test is wrong; (2) run every C3 journey: comments.test.mjs, story-writes.test.mjs, story.test.mjs, understand.test.mjs, translate.test.mjs; (3) recapture C3's screens and retire the temporary read-only residuals in parity/app/residuals.json only where the capture proves parity; (4) commit C3 as coherent commits (code, tests, parity, design text), keeping untracked logs out of git; (5) run .enjoy-logs/resume-full-gates.sh c3 with those five journey files (it formats, runs all clippy lines, the whole workspace's tests, builds and the journeys, and ends with ALL_GATES_PASSED); fix only what it finds, commit, and rerun only the failed steps; (6) update .audit/plans/C3.md with the final proof, mark C3 done in .audit/STATUS.md (and the unit count), add a short C3 checkpoint to .audit/plans/RESUME-2026-10-20.md, and commit. ${HARM} Leave ${W} clean. If something blocks you, say exactly what.${noteFor('c3')} complete is true only when the gate passed and everything is committed; head is the final commit sha; owner_question is empty unless the design needs the owner.`, { label: 'finish:C3', phase: 'Build', schema: BUILT })
  if (r && r.owner_question) ownerQuestions.push({ unit: 'C3', q: r.owner_question })
  return r
})()

async function verify(u, report) {
  const v = await agent(`${COMMON}\n\nYou VERIFY unit ${u.id} in its lane worktree ${laneOf(u)} (branch ${branchOf(u)}). Leave it exactly as you found it: you may edit files to revert a claimed fix and see a test fail, or to add a throwaway test, but restore everything and leave git status clean. The task:\n<<<\n${u.task}\n>>>\nThe builder reported:\n<<<\n${report}\n>>>\nWork from deterministic evidence: run the unit's tests and clippy for the crates it touches and the browser journeys it names, and check its row's "done when". Drive the screens it adds in Chromium through the journeys and compare with tC4 where the row asks for parity. Report a blocker for any loss, hiding or silent rewrite of a translator's work, a major for a wrong result or a broken invariant, and a minor for anything else, one line each. ${HARM}`, { label: `verify:${u.id}`, phase: 'Verify', schema: VERDICT })
  if (!v) return { ok: false, why: 'verifier died' }
  minors.push(...v.problems.filter(p => !fixable(p)).map(p => ({ unit: u.id, finding: p.title, evidence: p.evidence })))
  let serious = v.problems.filter(fixable)
  for (let round = 1; serious.length && round <= 2; round++) {
    log(`${u.id}: ${serious.length} to fix (round ${round})`)
    const fx = await agent(`${COMMON}\n\nYou FIX unit ${u.id} in ${laneOf(u)} (branch ${branchOf(u)}). Fix only these, at the root, with the smallest change (delete before you add), each with a regression test that fails without the fix:\n${JSON.stringify(serious, null, 1)}\nRun the tests and clippy of the crates you touch and the journeys you change; commit each fix. Final answer: each finding and what you did.`, { label: `fix:${u.id}#${round}`, phase: 'Verify' })
    if (!fx) return { ok: false, why: 'fixer died' }
    const c = await agent(`${COMMON}\n\nYou CHECK the fixes on unit ${u.id} in ${laneOf(u)} (restore anything you change; leave git status clean). These were found:\n${JSON.stringify(serious, null, 1)}\nThe fixer reported:\n<<<\n${fx}\n>>>\nFor each, run its regression test, revert the fix and confirm the test fails, then restore; run the touched crates' tests. Report only blockers and majors that remain or that the fixes introduced; do not review anything else. ${HARM}`, { label: `check:${u.id}#${round}`, phase: 'Verify', schema: VERDICT })
    if (!c) return { ok: false, why: 'checker died' }
    minors.push(...c.problems.filter(p => !fixable(p)).map(p => ({ unit: u.id, finding: p.title, evidence: p.evidence })))
    serious = c.problems.filter(fixable)
  }
  if (serious.length) return { ok: false, why: 'blockers or majors remain after two fix rounds', remaining: serious }
  return { ok: true }
}

const UNITS = A.units
const byKey = Object.fromEntries(UNITS.map(u => [u.key, u]))
const results = {}
function run(u) {
  if (results[u.key]) return results[u.key]
  results[u.key] = (async () => {
    const inBatch = u.deps.filter(k => byKey[k] || k === 'c3')
    const deps = await Promise.all(inBatch.map(k => k === 'c3' ? c3.then(r => ({ merged: !!(r && r.complete) })) : run(byKey[k])))
    const unmet = inBatch.filter((k, i) => !deps[i] || !deps[i].merged)
    if (unmet.length) { log(`${u.id}: waits for ${unmet.join(', ')}`); return { key: u.key, merged: false, waits: unmet } }
    const built = (A.built || {})[u.key] || await agent(`${COMMON}\n\nYou BUILD unit ${u.id}. Create your lane worktree inside the resume worktree: git -C ${W} worktree add -b ${branchOf(u)} ${laneOf(u)} ${BR} (if it exists, cd there, read git log and git status, and continue). Then ln -s /home/bot/tc-rst/parity/tc4-baseline/node_modules ${laneOf(u)}/parity/tc4-baseline/node_modules if it is missing. Work only in ${laneOf(u)}; its build folder is its own target/. Write a short plan to .audit/plans/${u.id}.md first, then build.\n\nYOUR UNIT (${u.id}):\n${u.task}${noteFor(u.key)}\n\nCommit each coherent step once it builds and its tests pass. Run the tests and clippy of the crates you touch and the journeys you add as you go, including the wasm clippy line for tc-ui when you touch it; do not run the whole workspace repeatedly. complete is true when the row's "done when" is met and committed; head is your last commit; report says what you built, how it meets "done when", test counts, design text you changed and why, and what is left; owner_question puts any question the owner must answer in plain words a translator would understand, with what the person would see under each answer and your recommendation, or is empty.`, { label: `build:${u.id}`, phase: 'Build', schema: BUILT })
    if (!built) { log(`${u.id}: builder died`); return { key: u.key, merged: false, why: 'builder died' } }
    if (built.owner_question) ownerQuestions.push({ unit: u.id, q: built.owner_question })
    if (!built.complete) { log(`${u.id}: builder reports incomplete`); return { key: u.key, merged: false, why: 'incomplete', report: built.report } }
    const v = await verify(u, built.report)
    if (!v.ok) { log(`${u.id}: not merged (${v.why})`); return { key: u.key, merged: false, why: v.why, remaining: v.remaining } }
    const c3r = await c3
    if (!c3r || !c3r.complete) return { key: u.key, merged: false, why: 'C3 is not committed, so the worktree cannot take a merge' }
    const unitMinors = minors.filter(m => m.unit === u.id)
    const m = await serial(() => agent(`${COMMON}\n\nYou MERGE branch ${branchOf(u)} (unit ${u.id}) into ${BR} in ${W}. If the branch tip is already in ${BR} (git merge-base --is-ancestor ${branchOf(u)} HEAD), an earlier merge agent was cut off after its merge commit: do not merge again; keep any uncommitted record edits it left (such as .audit/minors.tsv) and do the steps after the merge. Otherwise git status there must be clean before you start; if it is not, stop and say what is there. git merge --no-ff ${branchOf(u)} -m 'Merge ${u.id}: <what it brings, in plain words>' plus the attribution line. Resolve conflicts keeping both sides' behavior (Cargo.lock: keep the current side and let cargo add what is missing; parity record files: keep every case and recapture only what the merge changed). Then run in ${W}: cargo fmt --all -- --check; cargo clippy --workspace --all-targets -j 6 -- -D warnings; cargo clippy -p tc-ui --target wasm32-unknown-unknown --all-targets --features app -j 6 -- -D warnings; the tests of every crate the unit or the merge touched; and the unit's own journeys after rebuilding tc4 and the UI app. Fix only what the merge broke, in its own commit. Append these minor findings to .audit/minors.tsv (header 'unit\\tfinding\\tevidence'), one line each, then mark ${u.id} done in .audit/STATUS.md (with the unit count) and add a short ${u.id} checkpoint to .audit/plans/RESUME-2026-10-20.md, and commit: ${JSON.stringify(unitMinors)}. Then git -C ${W} worktree remove ${laneOf(u)} (keep the branch). Report merged, the sha and test counts.`, { label: `merge:${u.id}`, phase: 'Merge', schema: MERGED }))
    log(`${u.id}: ${m && m.merged ? 'merged at ' + m.sha : 'NOT merged'}`)
    return { key: u.key, merged: !!(m && m.merged), sha: m && m.sha }
  })()
  return results[u.key]
}

const unitResults = await Promise.all(UNITS.map(run))
const c3r = await c3
const mergedAny = unitResults.some(r => r && r.merged)
let gate = null
if (mergedAny) {
  phase('Gate')
  gate = await agent(`${COMMON}\n\nYou run the FINAL GATE on ${BR} in ${W} after these merges: ${JSON.stringify(unitResults.filter(r => r && r.merged).map(r => r.key))}. git status must be clean. Run .enjoy-logs/resume-full-gates.sh batch followed by every parity/app/journeys/*.test.mjs file (it formats, runs every clippy line, the whole workspace's tests, the builds and the journeys, and ends with ALL_GATES_PASSED). A failure: find whether the product or the test is wrong, fix it at the root with the smallest change and a regression test, commit, and rerun only the failed steps; a timing test that fails only under load gets a rerun alone before it counts. Then record the gate's result (Rust test count, journeys) in .audit/plans/RESUME-2026-10-20.md and commit. ${HARM} passed is true only when every step passed on the final commit.`, { label: 'gate:batch', phase: 'Gate', schema: GATE })
}
return { c3: c3r && { complete: c3r.complete, head: c3r.head, report: c3r.report }, units: unitResults, gate, ownerQuestions, minors }
