export const meta = {
  name: 'verify-loop',
  description: 'Verify a unit on its own branch until an independent checker finds no blocker or major: check, fix, check again (up to three rounds), then the full gates',
  phases: [
    { title: 'Check', detail: 'an independent, adversarial checker per round' },
    { title: 'Fix', detail: 'a fixer for what the checker found' },
    { title: 'Gate', detail: 'fmt, clippy, tests, and the unit\'s own extra checks' },
  ],
}

// args: { root, wt, branch, unit, scope, history?, gateExtra? }
// wt: the unit's worktree; scope: what the unit covers and where its spec is; history: what earlier
// reviews found and what was fixed, so the checker tries to break the fixes as well.
const A = args
const ENV = "In every shell command that runs cargo, rustc, node or npx, source ~/.tc-env first ('. ~/.tc-env && ...'); use cargo -j 6."
const CTX = `This is the Rust rebuild of translationCore 4 (tC4), a Bible translation app; losing, hiding or silently rewriting a translator's work is unacceptable. AGENTS.md is the rule book. Work only in the worktree ${A.wt} (branch ${A.branch}); nobody else works there while you do. ${ENV}\n\nUnit ${A.unit}:\n${A.scope}${A.history ? '\n\nWhat earlier reviews found and what was fixed:\n' + A.history : ''}`
const ISSUE = { type: 'object', properties: { title: { type: 'string' }, severity: { type: 'string', enum: ['blocker', 'major', 'minor'] }, evidence: { type: 'string' }, fix: { type: 'string' } }, required: ['title', 'severity', 'evidence', 'fix'] }
const CHECK = { type: 'object', properties: { problems: { type: 'array', items: ISSUE }, summary: { type: 'string' } }, required: ['problems', 'summary'] }
const GATE = { type: 'object', properties: { green: { type: 'boolean' }, sha: { type: 'string' }, summary: { type: 'string' } }, required: ['green', 'summary'] }

let found = []
const rounds = []
for (let round = 1; round <= 3; round++) {
  const c = await agent(`${CTX}\n\nYou are an independent, adversarial CHECKER, round ${round}. Do not commit, and leave the worktree clean with HEAD unchanged. Read the unit's code and tests as they are now (git log, and the diff against the merge base with claude/focused-ride-5uzj7p). Try hardest to break the latest fixes: for each, write a throwaway test of the scenario it claims to close and of the scenarios next to it (a crash at each write point, a restart, an outside edit or revert, a teammate's change arriving, a held file), run it, and remove it. Revert the core of each fix and confirm its regression test fails. A finding needs evidence you ran. Severity: blocker for any silent loss, hiding or rewriting of a translator's work; major for a wrong result or a broken invariant; minor otherwise.${found.length ? `\n\nThe previous round found these, and a fixer addressed them; check each is fixed at its root:\n${JSON.stringify(found, null, 1)}` : ''}`, { label: `check:${A.unit}#${round}`, phase: 'Check', schema: CHECK })
  if (!c) return { ok: false, rounds, why: 'checker died' }
  const serious = c.problems.filter(p => p.severity !== 'minor')
  rounds.push({ round, problems: c.problems.map(p => `${p.severity}: ${p.title}`) })
  log(`${A.unit} check ${round}: ${serious.length} blocker/major, ${c.problems.length - serious.length} minor`)
  if (!c.problems.length) break
  found = c.problems
  const f = await agent(`${CTX}\n\nYou FIX what an independent checker found (round ${round}):\n${JSON.stringify(c.problems, null, 1)}\nFix every blocker and major at its root, and the minors that are cheap. Add a regression test for each that fails without the fix (check by reverting). If you disagree with a finding, explain with evidence you ran. Commit each fix on its own once it builds and its tests pass (cargo fmt --all; cargo clippy --workspace --all-targets -- -D warnings; the tests of the crates you touch), with the line 'Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>' at the end of each message. Every commit must build. Final answer: each finding and what you did, with the commit.`, { label: `fix:${A.unit}#${round}`, phase: 'Fix' })
  if (!f) return { ok: false, rounds, why: 'fixer died' }
  if (!serious.length) break
  if (round === 3) return { ok: false, rounds, why: 'blockers or majors remain after three rounds', remaining: c.problems }
}

const g = await agent(`In ${A.wt} (cd there; ${ENV}), run: cargo fmt --all -- --check; cargo clippy --workspace --all-targets -j 6 -- -D warnings; cargo test --workspace -j 6; git status --porcelain (must be empty).${A.gateExtra ? ' ' + A.gateExtra : ''} Report green=true only if all pass and the tree is clean, sha = git rev-parse HEAD, and in summary the test counts and the first 40 lines of any failure.`, { label: `gate:${A.unit}`, phase: 'Gate', schema: GATE, effort: 'low' })
if (!g) return { ok: false, rounds, why: 'gate agent died' }
return { ok: g.green, gate: g, rounds }
