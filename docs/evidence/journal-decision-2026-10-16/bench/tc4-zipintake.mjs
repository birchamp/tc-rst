// h2h: the CLIENT side of tC4's fastest open path (#423), replicated in Node over the same
// segments: one zip of checking/journal -> fflate unzipSync -> strFromU8 per entry ->
// classifySegments (yield every 50, name round-trip, validateSegment over WebCrypto,
// actor and ts checks) -> ts sort -> fold. Excluded: the HTTP transfer, the server's
// zipping, postMessage to the fold worker, inventoryDisk and compareDisk.
import fs from 'fs';
import { unzipSync, strFromU8 } from '/home/user/unfoldingword/translationcore4/node_modules/fflate/esm/index.mjs';
import { fold } from '/home/user/unfoldingword/translationcore4/journal/fold.mjs';
import { validateAction } from '/home/user/unfoldingword/translationcore4/journal/schema.mjs';
import { segmentTs, segmentName } from '/home/user/unfoldingword/translationcore4/journal/files.mjs';
import { actorSlugError, isTs } from '/home/user/unfoldingword/translationcore4/journal/grammar.mjs';
const zipPath = process.argv[2];
const PREFIX = 'checking/journal/';
const now = () => process.hrtime.bigint();
const ms = (a, b) => +(Number(b - a) / 1e6).toFixed(0);
const sha256Hex = async (text) => {
  const digest = await crypto.subtle.digest('SHA-256', new TextEncoder().encode(text));
  return [...new Uint8Array(digest)].map((b) => b.toString(16).padStart(2, '0')).join('');
};
const validateSegment = async (raw) => {
  let outer; try { outer = JSON.parse(raw); } catch { return { ok: false, reason: 'outer-parse' }; }
  if (!outer || outer.container !== 1 || typeof outer.body !== 'string' || typeof outer.sha256 !== 'string') return { ok: false, reason: 'shape' };
  if ((await sha256Hex(outer.body)) !== outer.sha256) return { ok: false, reason: 'checksum' };
  let body; try { body = JSON.parse(outer.body); } catch { return { ok: false, reason: 'body-parse' }; }
  if (!body || !Array.isArray(body.events)) return { ok: false, reason: 'no-events' };
  const err = validateAction(body.events);
  return err ? { ok: false, reason: err } : { ok: true, events: body.events };
};
const c0 = process.cpuUsage();
const t0 = now();
const zip = fs.readFileSync(zipPath);
const t1 = now();
const entries = unzipSync(zip, { filter: () => true });
const t2 = now();
const byIpath = new Map();
for (const [name, bytes] of Object.entries(entries)) {
  if (name.endsWith('/')) continue;
  byIpath.set(PREFIX + name.replaceAll('\\', '/'), strFromU8(bytes));
}
const t3 = now();
const byActor = new Map();
for (const p of byIpath.keys()) {
  const parts = p.slice(PREFIX.length).split('/');
  if (parts.length !== 3 || parts[1] !== 'segments' || actorSlugError(parts[0])) continue;
  (byActor.get(parts[0]) ?? byActor.set(parts[0], []).get(parts[0])).push(parts[2]);
}
const actions = []; let invalid = 0; let since = 0;
for (const actor of [...byActor.keys()].sort()) {
  const dir = PREFIX + actor + '/segments';
  const segs = [];
  for (const name of byActor.get(actor)) {
    if (++since >= 50) { since = 0; await new Promise((r) => setTimeout(r, 0)); }
    const ts = segmentTs(name);
    if (!isTs(ts) || segmentName(ts) !== name) { invalid++; continue; }
    const v = await validateSegment(byIpath.get(dir + '/' + name));
    if (!v.ok || v.events.some((e) => e.actor !== actor) || v.events[0].ts !== ts) { invalid++; continue; }
    segs.push({ name, ts, events: v.events });
  }
  segs.sort((a, b) => (a.name < b.name ? -1 : a.name > b.name ? 1 : 0));
  for (const s of segs) actions.push(s);
}
actions.sort((a, b) => (a.ts < b.ts ? -1 : a.ts > b.ts ? 1 : 0));
const events = actions.flatMap((a) => a.events);
const t4 = now(); const c4 = process.cpuUsage();
const r = fold(events);
const t5 = now(); const c5 = process.cpuUsage();
if (invalid) throw new Error(invalid + ' invalid segments');
const cpu = (a, b) => +(((b.user - a.user) + (b.system - a.system)) / 1000).toFixed(0);
console.log(JSON.stringify({ segments: byIpath.size, events: events.length, books: Object.keys(r.books).length, zip_bytes: zip.length,
  read_zip_ms: ms(t0, t1), unzip_ms: ms(t1, t2), decode_ms: ms(t2, t3), classify_ms: ms(t3, t4), intake_ms: ms(t0, t4), fold_ms: ms(t4, t5), total_ms: ms(t0, t5),
  intake_cpu_ms: cpu(c0, c4), fold_cpu_ms: cpu(c4, c5), maxrss_mib: Math.round(process.resourceUsage().maxRSS / 1024) }));
