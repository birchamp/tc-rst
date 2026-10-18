export const meta = {
  name: 'app-phase0',
  description: 'Phase 0 of docs/APP-DESIGN.md: the Windows Electronite spike package for the owner (G0, first) and units A1-A11, each built in its own worktree when its dependencies are merged, verified independently (three lenses for the no-loss unit A4b), fixed, and merged into the phase0 integration branch',
  phases: [
    { title: 'Build', detail: 'one builder per unit, in dependency order' },
    { title: 'Verify', detail: 'independent verifiers; three lenses for A4b' },
    { title: 'Merge', detail: 'serialized merges into phase0 with the full gates' },
  ],
}

const MAIN = '/home/user/tc-rst'
const TC4 = '/home/user/unfoldingword/translationcore4'
const MAINBR = 'claude/focused-ride-5uzj7p'
const INT = 'phase0'
const INTWT = `${MAIN}/.claude/worktrees/phase0`
const ATTR = `End every commit message with the two lines 'Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>' and 'Claude-Session: https://claude.ai/code/session_01WMzGyPZKd7RwKesCe1ACqV'. Do not push.`
const GATES = `cargo fmt --all -- --check; cargo clippy --workspace --all-targets -j 2 -- -D warnings; cargo test --workspace -j 2; cargo clippy -p tc-ui --target wasm32-unknown-unknown --all-targets -j 2 -- -D warnings, once without features and once with each tc-ui feature that exists (gallery, and app and harness once they exist); and, once the crate tc-api exists, cargo clippy -p tc-api --target wasm32-unknown-unknown --all-targets -j 2 -- -D warnings`
const COMMON = `You work on the Rust rebuild of translationCore 4 (tC4), a Bible translation desktop app; losing, hiding or silently rewriting a translator's work is unacceptable. Read AGENTS.md (note "Matching tC4": where tC4's behavior works against what a feature is for, follow what the translator needs and record the difference with its reason), docs/PLAN.md, docs/ARCHITECTURE.md and docs/PERFORMANCE.md. docs/APP-DESIGN.md is the normative design for this work: read "Owner's answers (2026-10-17)", §0 to §2, §10's introduction, your unit's row in §10, every section that row cites, and the red-team answers ("How each point was answered") that mention your unit. If the design is wrong or unbuildable somewhere, do not silently deviate: fix the design text in the same branch, say why in the commit message and in your final answer. tC4's source is at ${TC4} (checked out at d310bf3; read-only; its node_modules work for running tC4 code). The storage branch and ${MAIN}/.claude/worktrees/storage belong to another team: never change them. Machine: 4 CPUs shared with other agents (cargo -j 2); limited disk: use only your worktree's own target directory; if df shows under 6 GB free and no rustc is running in your worktree, run python3 -I ${MAIN}/.audit/tools/prune-target.py <your worktree>/target. Never delete anything outside your own worktree (not the scratchpad, not other worktrees, not refs/wip/*). Tests never touch the network. If your worktree or branch already exists, a previous agent was stopped mid-task: cd into it, read git log, status and diff, keep what is sound and continue. Commit each coherent step as soon as it builds and passes its tests. Before your last commit run: ${GATES}.`

const G0 = `UNIT G0 package (time-critical: the owner runs it on a Windows laptop today, 2026-10-17, at 20:00 UTC; it must be complete, reviewed and merged well before then). Build what the owner needs to run design unit G0 (docs/APP-DESIGN.md §10, row G0, and the sections it cites: AD11, Q-1, Q-13, A-3, A-14, §5.2, §5.7, §6.6) plus a Windows run of the storage benchmark, as a task a Claude Code session on the owner's Windows computer can follow start to finish without help.
(1) spikes/electronite/: a throwaway Electronite v37.1.0-graphite spike (tC4 docs/PACKAGING.md says tC4 ships Electronite v37.1.0-graphite from github.com/unfoldingWord/electronite releases; read it and tC4's scripts/desktop-main.cjs, preload.cjs and desktop-bootstrap.cjs and follow how tC4 starts, supervises and talks to its server). It is never shipped and stays outside the cargo workspace (its own Cargo.toml and Cargo.lock; add exclude = ["spikes"] to the root workspace if needed). Because the real tc-server (unit A2) is being built at the same time, the spike brings a small stand-in server binary on the same stack the design names (axum on 127.0.0.1, a per-launch token required on every request as ARCHITECTURE.md says, an address line on stdout, quit on a closed stdin) that serves exactly what the probes need; say plainly in the evidence that it is a stand-in and that unit A2 measures tc4 serve's own loopback cost. The spike measures and records: the header filter of AD11 (the shell adds the token header to requests the browser makes by itself) and a probe of every request type (fetch, image, iframe, download, print window, web font, CSS url(), the event stream), each marked passes or fails; a protocol.handle prototype for Q-1 (a privileged custom scheme proxied to the server) with the same probes; the aligner round trip: a click to a 20 KB JSON reply and its render, alone and while OBS-sized pictures and a help page load, median and p95 of at least 200 clicks, for the header filter and for protocol.handle; supervision: end the server process (the shell restarts it and reloads Home), end the shell (the server exits within 5 s), a hard kill of the shell (what remains of localStorage and of the shell's mirror file of typed text, AD21); and the Chromium and Electronite versions. A PowerShell script runs every case and writes the results as Markdown.
(2) The storage benchmark on Windows: the release foldbench of tc-corpus on the storage branch (read its docs/evidence/storage-fold-2026-10-16.md for the commands): generate the whole-Bible corpus, then load and fold with the fold cache (warm, and after a reboot or with the file cache emptied if the tester can), the first open without a cache, the Psalms projection, and peak and steady memory; with Windows Defender real-time protection on (state whether it is on); the CPU model, cores, RAM, disk type, and whether the CPU has SHA and AVX-512 instructions. Make foldbench print what it cannot measure on Windows as unavailable, not 0, if it does not already.
(3) .audit/tasks/g0-windows.md: the task for the owner's Windows session, in plain short sentences: what it is for; prerequisites with exact versions and where to get them (Git, Rust stable with the MSVC toolchain via rustup, Node 22 if needed, the Electronite release asset name and how to unpack it); how to get the code (git clone or pull ${MAINBR}; a worktree or checkout of the storage branch for part 2); the exact commands; how long each part takes; what to do if something fails; what not to change; where to write the results (docs/evidence/electronite-spike-2026-10-17.md and docs/evidence/storage-fold-windows-2026-10-17.md, with the machine described); then commit on a new branch g0-windows and push only that branch (never push ${MAINBR} or storage); never record a token or a password. Finally list what the results decide (Q-1, Q-13, the budgets on the reference machine) so the owner knows why it matters.
Test everything that can run here: the stand-in server's tests, the page's probe logic in Chromium through Playwright against the stand-in (Chromium is at /opt/pw-browsers; Electronite cannot run here), the PowerShell script's syntax if pwsh is available (if not, say so), and the foldbench commands on Linux at a small scale. Final answer: the files, what was tested here and how, and anything the owner must know before 20:00 UTC.`

const UNITS = [
  { key: 'g0', id: 'G0', deps: [], task: G0 },
  { key: 'a1', id: 'A1', deps: [], task: 'Build design unit A1 (tc-api foundations) exactly as its row in docs/APP-DESIGN.md §10 says, with its tests and its "done when".' },
  { key: 'a6a', id: 'A6a', deps: [], task: 'Build design unit A6a (parity harness, tC4 side) exactly as its row in docs/APP-DESIGN.md §10 says. Read parity/tc4-baseline/README.md and capture.mjs first; the baseline PNGs must stay byte-identical after the split.' },
  { key: 'a10', id: 'A10', deps: [], task: 'Build design unit A10 (the new pure crate tc-export: the print model) exactly as its row in docs/APP-DESIGN.md §10 says, with vectors recorded by running tC4\'s own functions in node.' },
  { key: 'a11', id: 'A11', deps: [], task: 'Build design unit A11 (streaming downloads in tc-door43, used by tc-resources installs) exactly as its row in docs/APP-DESIGN.md §10 says. The storage team is changing tc-resources on its own branch; keep your tc-resources change small and confined to the install path.' },
  { key: 'a2', id: 'A2', deps: ['a1'], task: 'Build design unit A2 (tc-server and tc4 serve) exactly as its row in docs/APP-DESIGN.md §10 says, including the loopback spike (plain and through a header-adding proxy) with its two p95 figures in docs/evidence/.' },
  { key: 'a3', id: 'A3', deps: ['a1'], task: 'Build design unit A3 (tc-app foundations) exactly as its row in docs/APP-DESIGN.md §10 says. The default projects folder is %USERPROFILE%\\translationCore4\\projects (or ~/translationCore4/projects), checked against the cloud-folder detection at first launch (owner, Q-4).' },
  { key: 'a4a', id: 'A4a', deps: ['a1', 'a2'], task: 'Build design unit A4a (tc-ui runtime: transport, cache, language) exactly as its row in docs/APP-DESIGN.md §10 says.' },
  { key: 'a9', id: 'A9', deps: ['a2', 'a3'], task: 'Build design unit A9 (first-launch seeding) exactly as its row in docs/APP-DESIGN.md §10 says. The storage team is changing tc-resources on its own branch; keep your tc-resources change to the seeding hooks.' },
  { key: 'a4b', id: 'A4b', deps: ['a1', 'a2', 'a3', 'a4a'], critical: true, task: 'Build design unit A4b (the tc-ui no-loss core: the save machine, the draft buffer with the shell bridge, boot.js recovery, panics, the navigation gate and close guard) exactly as its row in docs/APP-DESIGN.md §10 says. This is a no-loss unit (PLAN.md rigor): property tests, crash and reload injection at every point, and every red-team answer that names it (RT-1, RT-2, RT-3, RT-6, RT-9, RT-10, RT-15, RT-19, RT-20, RT-27).' },
  { key: 'a6b', id: 'A6b', deps: ['a6a', 'a2', 'a4a'], task: 'Build design unit A6b (parity harness, Rust side, and the gate in cargo test) exactly as its row in docs/APP-DESIGN.md §10 says.' },
  { key: 'a5', id: 'A5', deps: ['a3', 'a4a', 'a4b', 'a6b'], task: 'Build design unit A5 (shell chrome and static surfaces) exactly as its row in docs/APP-DESIGN.md §10 says. Feedback is composed here and sent later by the shell, gated by the internet switch (owner, Q-2).' },
  { key: 'a7', id: 'A7', deps: ['a6a', 'a6b'], task: 'Build design unit A7 (the tC4 view-state gallery) exactly as its row in docs/APP-DESIGN.md §10 says.' },
  { key: 'a8', id: 'A8', deps: ['a3', 'a5'], task: 'Build design unit A8 (Door43 account and the internet question) exactly as its row in docs/APP-DESIGN.md §10 says, against tc-door43-fake only.' },
]

const ISSUE = { type: 'object', properties: { title: { type: 'string' }, severity: { type: 'string', enum: ['blocker', 'major', 'minor'] }, evidence: { type: 'string' }, fix: { type: 'string' } }, required: ['title', 'severity', 'evidence', 'fix'] }
const VERDICT = { type: 'object', properties: { verdict: { type: 'string', enum: ['merge', 'fix_first'] }, problems: { type: 'array', items: ISSUE }, gates: { type: 'string' } }, required: ['verdict', 'problems', 'gates'] }
const MERGED = { type: 'object', properties: { merged: { type: 'boolean' }, head: { type: 'string' }, summary: { type: 'string' } }, required: ['merged', 'summary'] }

const wt = (u) => `${MAIN}/.claude/worktrees/p0-${u.key}`
const br = (u) => `p0-${u.key}`

// Owner, 2026-10-18: Opus plans and reviews, Sonnet edits, Haiku reads. Calls that had already finished keep their
// prompts and options unchanged, so a resume replays them from the cache.
const FINISHED = new Set(['build:G0', 'verify:G0#1', 'fix:G0#1', 'build:A1', 'verify:A1#1', 'fix:A1#1', 'build:A6a', 'verify:A6a#1', 'fix:A6a#1', 'build:A10', 'verify:A10#1', 'fix:A10#1', 'build:A11', 'verify:A11#1'])
const tier = (label, model) => FINISHED.has(label) ? {} : { model }

let mergeChain = Promise.resolve()
function serialMerge(fn) { const p = mergeChain.then(fn); mergeChain = p.catch(() => null); return p }

const LENSES = [
  'You are one of three reviewers of a no-loss unit. Your lens: can typed text ever be lost, hidden or saved as something else? Attack with reloads, crashes (test/crash), WebAssembly panics, closing the window, two tabs, retries after a reload, a save racing a refresh, NFC differences and a full disk.',
  'You are one of three reviewers of a no-loss unit. Your lens: races and protocol. Attack the save machine and the reply cache with duplicate and reordered requests, retries with another body, StaleScope, slow and failing servers, event-stream gaps and resyncs, and check every state the types allow is handled.',
  'You are one of three reviewers of a no-loss unit. Your lens: the browser and parity. Attack boot.js, the capture listener, the static callout, the navigation gate and close guard in Chromium through Playwright, and check what tC4 does in the same moments (its scheduler, drafts and close handling) so any difference is declared.',
]

async function verifyAndFix(u, report) {
  for (let round = 1; round <= 3; round++) {
    if (u.key === 'g0' && round > 1) return { ok: false, report, why: 'G0 was merged into main by hand and run on Windows on 2026-10-17' }
    let problems = []
    let verdicts = []
    const base = `${COMMON}\n\nYou are an independent, skeptical VERIFIER of design unit ${u.id}, in the git worktree ${wt(u)} (cd there; branch ${br(u)}; do not create another worktree; leave it clean with HEAD unchanged). The task was:\n<<<\n${u.task}\n>>>\nThe builder reported:\n<<<\n${report}\n>>>\nDo not trust the report. Read the diff (git diff ${INT}...HEAD) and the design rows it implements. Check every claim against the code and, where it concerns tC4, against tC4's source. Revert the core of each change and confirm its tests fail, then restore. Attack it with throwaway tests (never commit them). Run the gates (${GATES}). Report only problems you verified by running code. verdict = merge only if the unit's "done when" is met, the gates pass and no blocker or major problem remains.`
    if (u.critical) {
      const vs = await parallel(LENSES.map((lens, i) => () => agent(`${base}\n\n${lens}`, { label: `verify:${u.id}#${round}.${i + 1}`, phase: 'Verify', schema: VERDICT })))
      verdicts = vs.filter(Boolean)
      if (!verdicts.length) return { ok: false, report, why: 'verifiers died' }
    } else {
      const v = await agent(base, { label: `verify:${u.id}#${round}`, phase: 'Verify', schema: VERDICT })
      if (!v) return { ok: false, report, why: 'verifier died' }
      verdicts = [v]
    }
    problems = verdicts.flatMap(v => v.problems)
    const serious = problems.filter(p => p.severity !== 'minor')
    const allMerge = verdicts.every(v => v.verdict === 'merge')
    log(`${u.id} round ${round}: ${allMerge ? 'merge' : 'fix_first'}, ${serious.length} blocker/major, ${problems.length - serious.length} minor`)
    if (allMerge && !problems.length) return { ok: true, report }
    if (round === 3 && serious.length) return { ok: false, report, problems }
    const fx = await agent(`${COMMON}\n\nYou are FIXING verified problems in design unit ${u.id}, in the git worktree ${wt(u)} (cd there; branch ${br(u)}). The task:\n<<<\n${u.task}\n>>>\nThe verifiers found:\n<<<\n${JSON.stringify(problems, null, 1)}\n>>>\nFix every blocker and major at the root and the minors; add a regression test for each that fails without the fix. If you disagree, explain with evidence. Run the gates; commit.${FINISHED.has(`fix:${u.id}#${round}`) ? '' : ' A previous fixer may have been stopped part way (a usage limit): if the worktree has uncommitted changes or commits after the verification, read them, keep what is sound, and continue.'} ${ATTR} Final answer: each problem fixed or rejected (why), test counts.`, { label: `fix:${u.id}#${round}`, phase: 'Build', ...tier(`fix:${u.id}#${round}`, 'sonnet') })
    if (!fx) return { ok: false, report, why: 'fixer died' }
    report += `\n\nRound ${round} fixes:\n${fx}`
    if (allMerge && !serious.length) return { ok: true, report }
  }
  return { ok: false, report }
}

const done = {}
const byKey = Object.fromEntries(UNITS.map(u => [u.key, u]))
function run(u) {
  if (done[u.key]) return done[u.key]
  done[u.key] = (async () => {
    const deps = await Promise.all(u.deps.map(k => run(byKey[k])))
    const failed = u.deps.filter((k, i) => !deps[i] || !deps[i].merged)
    if (failed.length) { log(`${u.id}: skipped, dependencies not merged: ${failed.join(', ')}`); return { key: u.key, merged: false, skipped: true } }
    let plan = ''
    if (!FINISHED.has(`build:${u.id}`)) {
      plan = await agent(`${COMMON}\n\nYou are the PLANNER of design unit ${u.id}. A separate agent on a faster model implements your plan, and independent verifiers check the result. First create the unit's worktree from the integration branch: if branch ${INT} does not exist yet, create it from ${MAINBR} (git -C ${MAIN} branch ${INT} ${MAINBR}); then git -C ${MAIN} worktree add -b ${br(u)} ${wt(u)} ${INT} (if it exists, use it). Do not change code and do not commit. Read the unit's row in docs/APP-DESIGN.md §10 and every section it cites, the code it builds on, and tC4's source where it matters. Write the plan to ${wt(u)}/.audit/plans/${u.id}.md (the only file you write; the builder commits it): the steps in order, each with the files, types and function signatures to add or change, the invariants and edge cases it must keep (above all, that no translator's work is lost, hidden or silently rewritten, and that the screens match tC4 4.0 where the row says so), the tests that prove the row's "done when" (named, with what each asserts), the measurements to take, and the traps you found. Make it concrete enough that the builder never has to rediscover what you read.\n\nYOUR UNIT (${u.id}):\n${u.task}\n\nFinal answer: the plan's path and a ten-line summary.`, { label: `plan:${u.id}`, phase: 'Build' })
      if (!plan) { log(`${u.id}: planner died`); return { key: u.key, merged: false } }
    }
    const report = await agent(plan
      ? `${COMMON}\n\nYour worktree ${wt(u)} (branch ${br(u)}, from ${INT}) was created by the planner; cd there and work only there. The planner's plan is ${wt(u)}/.audit/plans/${u.id}.md; its summary:\n<<<\n${plan}\n>>>\nRead the plan in full and follow it. Where the code or the design proves the plan wrong, do what the design requires and say so in your final answer. Commit the plan file with your first commit.\n\nYOUR UNIT (${u.id}):\n${u.task}\n\n${ATTR} Final answer: what you built (files, public API), how it meets the row's "done when", test counts, measured numbers, design text you changed and why, where you departed from the plan and why, and anything left open.`
      : `${COMMON}\n\nCreate your worktree from the integration branch: if branch ${INT} does not exist yet, create it from ${MAINBR} first (git -C ${MAIN} branch ${INT} ${MAINBR}); then git -C ${MAIN} worktree add -b ${br(u)} ${wt(u)} ${INT}, cd ${wt(u)} and work only there.\n\nYOUR UNIT (${u.id}):\n${u.task}\n\n${ATTR} Final answer: what you built (files, public API), how it meets the row's "done when", test counts, measured numbers, design text you changed and why, and anything left open.`,
      { label: `build:${u.id}`, phase: 'Build', ...tier(`build:${u.id}`, 'sonnet') })
    if (!report) { log(`${u.id}: builder died`); return { key: u.key, merged: false } }
    const v = await verifyAndFix(u, report)
    if (!v.ok) { log(`${u.id}: not verified`); return { key: u.key, merged: false, ...v } }
    const m = await serialMerge(() => agent(`${COMMON}\n\nYou MERGE verified design unit ${u.id} into the integration branch ${INT}. Use the worktree ${INTWT} (create it with git -C ${MAIN} worktree add ${INTWT} ${INT} if it does not exist; create branch ${INT} from ${MAINBR} first if it does not exist). In it: git merge --no-ff ${MAINBR} if ${MAINBR} has commits ${INT} lacks; then git merge --no-ff ${br(u)} with the message 'Merge ${br(u)}: <what unit ${u.id} brings>' plus the attribution lines. Resolve conflicts carefully, keeping both sides' behavior (Cargo.lock: keep the current side and let cargo add what is missing; never cargo update). Run every gate: ${GATES}. Fix anything the merge broke in its own commit. ${ATTR} Then remove the unit's worktree to free disk (git -C ${MAIN} worktree remove --force ${wt(u)}; the branch stays). merged = true only if the merge is committed and every gate passes. In summary give the head sha, test counts and anything unresolved.`, { label: `merge:${u.id}`, phase: 'Merge', schema: MERGED, model: 'sonnet' }))
    if (!m) { log(`${u.id}: merge agent died`); return { key: u.key, merged: false } }
    log(`${u.id}: ${m.merged ? 'merged into ' + INT + ' at ' + (m.head || '?') : 'NOT merged'}`)
    return { key: u.key, merged: m.merged, head: m.head, summary: m.summary }
  })()
  return done[u.key]
}

const results = await Promise.all(UNITS.map(u => run(u)))
return results.map(r => r && ({ key: r.key, merged: r.merged, skipped: r.skipped || false, head: r.head || null }))
