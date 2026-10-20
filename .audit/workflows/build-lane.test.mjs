import assert from 'node:assert/strict'
import { readFile } from 'node:fs/promises'
import test from 'node:test'

const source = await readFile(new URL('./build-lane.js', import.meta.url), 'utf8')
// Workflow files have a top-level return and receive their runtime from the host.
const AsyncFunction = Object.getPrototypeOf(async function () {}).constructor
const workflow = new AsyncFunction('args', 'agent', 'parallel', 'log', source.replace('export const meta', 'const meta'))
const keys = [...source.slice(0, source.indexOf('const UNITS =')).matchAll(/key: '([a-z0-9]+)'/g)].map(m => m[1])
const sha = 'a'.repeat(40)
const fixedSha = 'c'.repeat(40)

function answer(label, prompt = '') {
  if (label.startsWith('resume:')) return { kind: 'fresh', summary: 'no reusable checkpoints' }
  if (label.startsWith('ready:')) return JSON.parse(prompt.slice(prompt.lastIndexOf('\n') + 1))
  if (label.startsWith('merge:')) return { sha: 'b'.repeat(40), merged: true, pushed: true, summary: 'merged and pushed' }
  if (label.startsWith('validate')) return { sha, green: true, commands: ['cargo test -p touched'], summary: 'all required focused checks passed' }
  if (label.startsWith('verify:')) return { sha, problems: [], summary: 'independently reviewed' }
  if (label.startsWith('prove:') || label.startsWith('check:') || label.startsWith('check-')) return { sha, problems: [], complete: true, proofs: ['regression fails with mutant and passes after restore'], summary: 'all required proofs completed' }
  if (label.startsWith('fix:')) return { sha, summary: 'implemented with the required tests', prior: JSON.parse(prompt.split('Prior evidence:\n')[1]) }
  return { sha, summary: 'implemented with the required tests' }
}

async function run(selected, { args = {}, respond = () => undefined } = {}) {
  const calls = []
  const logs = []
  const result = await workflow({
    root: '/repo', tc4: '/reference', skip: keys.filter(k => !selected.includes(k)),
    done: ['a3', 'a5'], ...args,
  }, async (prompt, options) => {
    const call = { prompt, ...options }
    calls.push(call)
    const custom = await respond(call)
    return custom === undefined ? answer(options.label, prompt) : custom
  }, tasks => Promise.all(tasks.map(task => task())), message => logs.push(message))
  return { result, calls, logs }
}

test('critical units share one validation but retain every independent lens and isolated proof', async () => {
  const { result, calls } = await run(['ftext'])
  assert.equal(result.results[0].merged, true)
  assert.equal(calls.filter(c => c.label === 'validate:FIX-text').length, 1)
  const reviewers = calls.filter(c => c.label.startsWith('verify:'))
  assert.deepEqual(reviewers.map(c => c.label), ['verify:FIX-text.1', 'verify:FIX-text.2', 'verify:FIX-text.3'])
  for (const reviewer of reviewers) {
    assert.match(reviewer.prompt, /READ-ONLY review/)
    assert.match(reviewer.prompt, /do not edit source, revert changes, add\/remove temporary tests/)
    assert.match(reviewer.prompt, /do not.*launch builds or browser processes/)
    assert.match(reviewer.prompt, /all required focused checks passed/)
    assert.match(reviewer.prompt, new RegExp(sha))
  }
  const proof = calls.find(c => c.label === 'prove:FIX-text')
  assert.match(proof.prompt, /separate isolated checkout \/repo\/\.claude\/worktrees\/prove-ftext/)
  assert.match(proof.prompt, /source checkout \/repo\/\.claude\/worktrees\/lane-ftext stays read-only/)
  assert.match(proof.prompt, /give it its own target directory/)
  assert.match(proof.prompt, /seed every known no-loss bug/)
  assert.ok(calls.indexOf(proof) > Math.max(...reviewers.map(c => calls.indexOf(c))))
  assert.match(calls.find(c => c.label === 'build:FIX-text').prompt, /Before implementing a no-loss unit/)
  assert.match(calls.find(c => c.label === 'build:FIX-text').prompt, /Demonstrate at least one deliberately seeded bug is caught before extending/)
})

test('each missing critical lens blocks proof and merge instead of accepting the remaining reviews', async t => {
  for (const lens of [1, 2, 3]) await t.test(`lens ${lens}`, async () => {
    const { result, calls } = await run(['ftext'], { respond: c => c.label === `verify:FIX-text.${lens}` ? null : undefined })
    assert.equal(result.results[0].merged, false)
    assert.match(result.results[0].why, /required verifier/)
    assert.equal(calls.some(c => c.label.startsWith('prove:') || c.label.startsWith('merge:')), false)
  })
})

test('missing, incomplete and mismatched mutation proof all block a merge', async t => {
  for (const [name, proof] of [
    ['missing', null],
    ['incomplete', { ...answer('prove:unit'), complete: false }],
    ['no proof outcomes', { ...answer('prove:unit'), proofs: [] }],
    ['other SHA', { ...answer('prove:unit'), sha: fixedSha }],
  ]) await t.test(name, async () => {
    const { result, calls } = await run(['ftext'], { respond: c => c.label.startsWith('prove:') ? proof : undefined })
    assert.equal(result.results[0].merged, false)
    assert.match(result.results[0].why, /mutation proof/)
    assert.equal(calls.some(c => c.label.startsWith('merge:')), false)
  })
})

test('validation and every review must identify the exact built SHA', async t => {
  for (const label of ['validate:FIX-text', 'verify:FIX-text.2']) await t.test(label, async () => {
    const { result, calls } = await run(['ftext'], { respond: c => c.label === label ? { ...answer(label), sha: fixedSha } : undefined })
    assert.equal(result.results[0].merged, false)
    assert.equal(calls.some(c => c.label.startsWith('merge:')), false)
  })
})

const dataProblem = { title: 'saved words disappear after restart', severity: 'blocker', harm: 'data', evidence: 'model sequence restart', fix: 'keep the saved version' }
const internalMajor = { title: 'model explanation lacks a sentence', severity: 'major', harm: 'internal', evidence: 'design note', fix: 'write a sentence' }

test('data findings require a fix, fresh validation and isolated regression proof; internal findings remain logged', async () => {
  const { result, calls } = await run(['ftext'], { respond: c => {
    if (c.label === 'verify:FIX-text.1') return { ...answer(c.label), problems: [dataProblem, internalMajor] }
    if (c.label === 'fix:FIX-text#1') return { ...answer(c.label, c.prompt), sha: fixedSha, summary: 'fixed saved words' }
    if (c.label === 'validate-1:FIX-text' || c.label === 'check-1:FIX-text') return { ...answer(c.label), sha: fixedSha }
  } })
  assert.equal(result.results[0].merged, true)
  assert.deepEqual(result.minors.map(m => m.finding), [internalMajor.title])
  const fix = calls.find(c => c.label === 'fix:FIX-text#1')
  assert.match(fix.prompt, /saved words disappear/)
  assert.doesNotMatch(fix.prompt.split('IMPORTANT restart cursor:')[0], /model explanation lacks/)
  const check = calls.find(c => c.label === 'check-1:FIX-text')
  assert.match(check.prompt, new RegExp(fixedSha))
  assert.match(check.prompt, /sole MUTATION EXECUTOR/)
  assert.match(calls.find(c => c.label.startsWith('merge:')).prompt, new RegExp(`reviewed source SHA ${fixedSha}`))
})

test('a missing fix checker blocks merge even after a successful fresh validation', async () => {
  const { result, calls } = await run(['ftext'], { respond: c => {
    if (c.label === 'verify:FIX-text.1') return { ...answer(c.label), problems: [dataProblem] }
    if (c.label === 'check-1:FIX-text') return null
  } })
  assert.equal(result.results[0].merged, false)
  assert.match(result.results[0].why, /fix check missing/)
  assert.equal(calls.some(c => c.label.startsWith('merge:')), false)
})

test('default concurrency bounds complete unit pipelines to two', async () => {
  let active = 0
  let peak = 0
  const { result } = await run(['a8', 'flife', 'ftext'], { respond: async c => {
    if (c.label.startsWith('build:')) peak = Math.max(peak, ++active)
    await new Promise(resolve => setImmediate(resolve))
    if (c.label.startsWith('merge:')) active--
  } })
  assert.equal(peak, 2)
  assert.equal(active, 0)
  assert.ok(result.results.every(r => r.merged))
})

test('a dependent queued first does not hold a slot while its prerequisite runs', { timeout: 2000 }, async () => {
  const { result, calls } = await run(['ftext', 'falign'], { args: {
    maxActiveUnits: 1,
    refs: [{ key: 'dependent-ref', ref: 'reviewed-ref', label: 'dependent ref', deps: ['falign'] }],
  } })
  assert.ok(result.results.every(r => r.merged))
  const labels = calls.map(c => c.label)
  assert.ok(labels.indexOf('merge:FIX-text') < labels.indexOf('build:FIX-align'))
  assert.ok(labels.indexOf('merge:FIX-align') < labels.indexOf('merge:dependent ref'))
})

test('S2 handoff information never satisfies an unmerged storage prerequisite', async () => {
  const { result, calls } = await run(['d1', 'd2', 'f1'], { args: {
    done: ['c2', 'c5', 'b8', 'c1a'],
    external: { S2: { status: 'proving', branch: 's2-verify', merged: true, next: 'Finish its proof and merge handoff' }, I: { status: 'waiting' } },
  } })
  assert.equal(calls.length, 0)
  assert.ok(result.results.every(r => !r.merged))
  assert.deepEqual(result.prerequisites.map(p => p.key), ['S2', 'I'])
  assert.equal(result.prerequisites[0].merged, false)
  assert.equal(result.prerequisites[0].branch, 's2-verify')
  assert.equal(result.prerequisites[0].next, 'Finish its proof and merge handoff')
})

test('every stage consults, begins and saves receipts; merge still requires ancestry and actual full gates', async () => {
  const { calls } = await run(['ftext'])
  for (const call of calls.filter(c => !c.label.startsWith('resume:'))) {
    assert.match(call.prompt, /lane-state\.mjs' read/)
    assert.match(call.prompt, /lane-state\.mjs' begin/)
    assert.match(call.prompt, /lane-state\.mjs' save/)
    assert.match(call.prompt, /reusable=true/)
    assert.match(call.prompt, /Never use a report when reusable=false/)
    assert.match(call.prompt, /failed begin or save command fails this stage/)
  }
  const merge = calls.find(c => c.label.startsWith('merge:'))
  assert.match(merge.prompt, /both are ancestors of origin\//)
  assert.match(merge.prompt, /successful full-gate receipt for the recorded merged SHA/)
  assert.match(merge.prompt, /run-gates\.mjs --cwd \/repo\/\.claude\/worktrees\/lane-merge/)
  assert.match(merge.prompt, /moved source requires validation and review again/)
  for (const other of calls.filter(c => !c.label.startsWith('merge:'))) assert.doesNotMatch(other.prompt, /Run the full gates once/)
})

test('invalid pipeline limits fail before launching work', async () => {
  for (const maxActiveUnits of [0, -1, 1.5, '2']) {
    await assert.rejects(run(['ftext'], { args: { maxActiveUnits } }), /positive integer/)
  }
})

test('restart after a committed fix resumes targeted validation/check, then terminal receipt skips broad work', async () => {
  let cursor
  const interrupted = await run(['ftext'], { respond: c => {
    if (c.label === 'verify:FIX-text.1') return { ...answer(c.label), problems: [dataProblem, internalMajor] }
    if (c.label === 'fix:FIX-text#1') {
      cursor = { ...answer(c.label, c.prompt), sha: fixedSha }
      return cursor
    }
    if (c.label === 'validate-1:FIX-text') return null
  } })
  assert.equal(interrupted.result.results[0].merged, false)
  assert.equal(cursor.sha, fixedSha)
  assert.equal(cursor.prior.reviews.length, 3)
  let terminal
  const resumed = await run(['ftext'], { respond: c => {
    if (c.label.startsWith('resume:')) return { kind: 'fix', report: cursor, summary: 'resume committed fix cursor' }
    if (c.label === 'validate-1:FIX-text' || c.label === 'check-1:FIX-text') return { ...answer(c.label), sha: fixedSha }
    if (c.label === 'ready:FIX-text') {
      terminal = answer(c.label, c.prompt)
      return terminal
    }
  } })
  assert.equal(resumed.result.results[0].merged, true)
  assert.deepEqual(resumed.calls.map(c => c.label), ['resume:FIX-text', 'validate-1:FIX-text', 'check-1:FIX-text', 'ready:FIX-text', 'merge:FIX-text'])
  assert.equal(terminal.sha, fixedSha)
  assert.equal(terminal.evidence.reviews[0].sha, sha)
  assert.equal(terminal.evidence.fixes[0].proof.sha, fixedSha)
  const ready = await run(['ftext'], { respond: c => c.label.startsWith('resume:') ? { kind: 'verified', report: terminal, summary: 'complete verified chain at final SHA' } : undefined })
  assert.equal(ready.result.results[0].merged, true)
  assert.deepEqual(ready.calls.map(c => c.label), ['resume:FIX-text', 'merge:FIX-text'])
  assert.deepEqual(ready.result.minors.map(m => m.finding), [internalMajor.title])
  assert.match(ready.calls[0].prompt, /replay every command of the final validation report through node/)
  assert.match(ready.calls[0].prompt, /missing logs, changed runner inputs or stale receipts rerun/)
})

test('an incomplete claimed verified checkpoint falls back to full review; failed current evidence blocks it', async () => {
  const malformed = { sha, summary: 'claims ready', evidence: { built: answer('build:unit') } }
  const { result, calls } = await run(['ftext'], { respond: c => c.label.startsWith('resume:') ? { kind: 'verified', report: malformed, summary: 'incomplete chain' } : undefined })
  assert.equal(result.results[0].merged, true)
  assert.equal(calls.filter(c => c.label.startsWith('verify:')).length, 3)
  const blocked = await run(['ftext'], { respond: c => c.label.startsWith('resume:') ? { kind: 'blocked', summary: 'current gate replay failed' } : undefined })
  assert.equal(blocked.result.results[0].merged, false)
  assert.deepEqual(blocked.calls.map(c => c.label), ['resume:FIX-text'])
})

test('build and fix instructions preserve every required pre-commit gate', async () => {
  const { calls } = await run(['ftext'], { respond: c => c.label === 'verify:FIX-text.1' ? { ...answer(c.label), problems: [dataProblem] } : undefined })
  for (const stage of calls.filter(c => c.label.startsWith('build:') || c.label.startsWith('fix:'))) {
    assert.match(stage.prompt, /Before EACH commit run the full required fmt, workspace clippy and workspace tests/)
    assert.match(stage.prompt, /applicable wasm and no-test-hook checks/)
    assert.match(stage.prompt, /run-gates\.mjs/)
  }
})

test('cached terminal evidence with malformed nested commands or proofs cannot bypass validation', async t => {
  let terminal
  await run(['ftext'], { respond: c => {
    if (c.label === 'ready:FIX-text') {
      terminal = answer(c.label, c.prompt)
      return terminal
    }
  } })
  for (const [name, mutate] of [
    ['empty summary', r => { r.summary = '  ' }],
    ['nonstring command', r => { r.evidence.validation.commands = [null] }],
    ['blank command', r => { r.evidence.validation.commands = ['  '] }],
    ['nonstring proof', r => { r.evidence.proof.proofs = [{}] }],
    ['blank proof', r => { r.evidence.proof.proofs = ['\n'] }],
  ]) await t.test(name, async () => {
    const malformed = structuredClone(terminal)
    mutate(malformed)
    const { result, calls } = await run(['ftext'], { respond: c => c.label.startsWith('resume:') ? { kind: 'verified', report: malformed, summary: 'claimed cached checkpoint' } : undefined })
    assert.equal(result.results[0].merged, true)
    assert.equal(calls.filter(c => c.label.startsWith('verify:')).length, 3)
    assert.equal(calls.filter(c => c.label.startsWith('build:')).length, 1)
  })
})
