export const meta = {
  name: 'merge-queue',
  description: 'Merge several verified refs into main one after another in the merge-main worktree, each with its full gates, and push each to main as a fast-forward once green',
  phases: [
    { title: 'Merge', detail: 'merge-main.js for each item, in order' },
    { title: 'Push', detail: 'fast-forward main to the green merge, then the next item' },
  ],
}

// args: { root, items: [{ ref, label, review, context }] }
// One queue at a time: it owns .claude/worktrees/merge-main and is the only writer of main while it runs.
const A = args
const MAINBR = 'claude/focused-ride-5uzj7p'
const MWT = `${A.root}/.claude/worktrees/merge-main`
const PUSHED = { type: 'object', properties: { pushed: { type: 'boolean' }, sha: { type: 'string' }, summary: { type: 'string' } }, required: ['pushed', 'summary'] }
const results = []
for (const item of A.items) {
  phase('Merge')
  const r = await workflow({ scriptPath: `${A.root}/.audit/workflows/merge-main.js` }, { root: A.root, ref: item.ref, label: item.label, review: !!item.review, context: item.context })
  if (!r || !r.merged || !r.green) {
    results.push({ label: item.label, merged: false, why: r ? r.summary : 'merge-main died' })
    log(`${item.label}: not merged; the queue stops so main stays in order`)
    break
  }
  phase('Push')
  const p = await agent(`Push a gated merge to main. In ${MWT}: run git fetch origin. HEAD must be ${r.sha} and the tree clean. If origin/${MAINBR} is an ancestor of HEAD, run git push origin HEAD:${MAINBR}. If it is not (someone pushed meanwhile), do not push: report pushed=false with what origin/${MAINBR} gained. After a push, in ${A.root}: git merge --ff-only origin/${MAINBR} (if it refuses because of local edits there, leave them and say so), and delete the local ref ${item.ref.startsWith('ready/') ? item.ref : '(none)'} if it is a ready/ ref (git -C ${A.root} branch -D ${item.ref}). Never force-push and never push any other branch.`, { label: `push:${item.label}`, phase: 'Push', schema: PUSHED, effort: 'low' })
  results.push({ label: item.label, merged: true, green: true, sha: r.sha, pushed: !!(p && p.pushed), summary: r.summary, mergeReport: r.mergeReport, review: r.review, push: p && p.summary })
  if (!p || !p.pushed) { log(`${item.label}: merged and green but not pushed; the queue stops`); break }
  log(`${item.label}: on main at ${r.sha}`)
}
return results
