// h2h: the bench's [open] step, fold(readUnion(journalDir)), timed in a fresh process.
import path from 'path';
import { fold } from '/home/user/unfoldingword/translationcore4/journal/fold.mjs';
import { readUnion } from '/home/user/unfoldingword/translationcore4/journal/files.mjs';
const dir = path.join(process.argv[2], 'journal');
const reps = Number(process.env.REPS || 1);
const ms = (a, b) => Number(b - a) / 1e6;
for (let i = 0; i < reps; i++) {
  const c0 = process.cpuUsage(); const t0 = process.hrtime.bigint();
  const events = readUnion(dir);
  const t1 = process.hrtime.bigint(); const c1 = process.cpuUsage();
  const r = fold(events);
  const t2 = process.hrtime.bigint(); const c2 = process.cpuUsage();
  const dirty = ['forks', 'invalid', 'retained', 'pendingStructural', 'vrsRejected', 'supersedeRefused'].filter((k) => r[k].length > 0);
  if (dirty.length) throw new Error('not clean: ' + dirty);
  const cpu = (a, b) => ((b.user - a.user) + (b.system - a.system)) / 1000;
  console.log(JSON.stringify({ rep: i, events: events.length, books: Object.keys(r.books).length,
    scan_ms: +ms(t0, t1).toFixed(1), fold_ms: +ms(t1, t2).toFixed(1), total_ms: +ms(t0, t2).toFixed(1),
    scan_cpu_ms: cpu(c0, c1), fold_cpu_ms: cpu(c1, c2), maxrss_mib: Math.round(process.resourceUsage().maxRSS / 1024) }));
}
