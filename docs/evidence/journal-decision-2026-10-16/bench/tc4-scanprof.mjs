// h2h: where readUnion's time goes, stage by stage, over the same segment files (one thread).
// Replicates journal/files.mjs readSegments + validateSegment with a timer around each step.
import fs from 'fs';
import path from 'path';
import crypto from 'crypto';
import { validateAction } from '/home/user/unfoldingword/translationcore4/journal/schema.mjs';
const dir = path.join(process.argv[2], 'journal');
const sha256 = (s) => crypto.createHash('sha256').update(s, 'utf8').digest('hex');
const t = { list: 0n, lstat: 0n, read: 0n, outerParse: 0n, sha: 0n, bodyParse: 0n, validate: 0n };
const now = () => process.hrtime.bigint();
let n = 0, bytes = 0, events = 0;
const tAll = now();
for (const actor of fs.readdirSync(dir).sort()) {
  const seg = path.join(dir, actor, 'segments');
  let a = now();
  const names = fs.readdirSync(seg).filter((f) => f.endsWith('.action.json')).sort();
  t.list += now() - a;
  for (const f of names) {
    const file = path.join(seg, f);
    a = now(); const st = fs.lstatSync(file); t.lstat += now() - a;
    a = now(); const raw = fs.readFileSync(file, { encoding: 'utf8' }); t.read += now() - a;
    bytes += st.size; n++;
    a = now(); const outer = JSON.parse(raw); t.outerParse += now() - a;
    a = now(); if (sha256(outer.body) !== outer.sha256) throw new Error('sha'); t.sha += now() - a;
    a = now(); const body = JSON.parse(outer.body); t.bodyParse += now() - a;
    a = now(); const err = validateAction(body.events); t.validate += now() - a;
    if (err) throw new Error(err);
    events += body.events.length;
  }
}
const total = Number(now() - tAll) / 1e6;
const out = { segments: n, bytes, events, total_ms: +total.toFixed(0) };
for (const [k, v] of Object.entries(t)) out[k + '_ms'] = +(Number(v) / 1e6).toFixed(0);
console.log(JSON.stringify(out));
