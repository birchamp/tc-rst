// Median and spread of the startup timings in startup.jsonl, per program, from the runs whose
// label matches a pattern and that reached every step (a run that timed out is listed, not used).
// usage: node startup-stats.mjs <startup.jsonl> <label regex> [more regexes...]
import { readFileSync } from 'node:fs';
const [file, ...patterns] = process.argv.slice(2);
const rows = readFileSync(file, 'utf8').replace(/﻿/g, '').split('\n').filter(Boolean).map(l => JSON.parse(l));
const median = xs => { const s = [...xs].sort((a, b) => a - b); const n = s.length; return n % 2 ? s[(n - 1) / 2] : (s[n / 2 - 1] + s[n / 2]) / 2; };
for (const p of patterns) {
  const re = new RegExp(p);
  const all = rows.filter(r => re.test(r.label));
  const keys = ['window_ms', 'painted_ms', 'usable_ms', 'home_ms', 'project_ms', 'translate_ms', 'text_ms'].filter(k => all.some(r => r[k] != null));
  const last = keys[keys.length - 1];
  const ok = all.filter(r => r[last] != null);
  console.log(`== ${p}: ${ok.length} complete of ${all.length}`);
  for (const r of all.filter(r => r[last] == null)) console.log(`   not used: ${r.label} (${r.at}): stopped before ${last}`);
  for (const k of keys) {
    const xs = ok.map(r => r[k]);
    console.log(`   ${k.padEnd(13)} median ${median(xs)}  min ${Math.min(...xs)}  max ${Math.max(...xs)}  runs ${xs.join(', ')}`);
  }
  if (ok[0]?.psalms_click_ms != null) {
    const d = ok.map(r => r.project_ms - r.psalms_click_ms), t = ok.map(r => r.text_ms - r.translate_click_ms);
    console.log(`   click Psalms -> project open: median ${median(d)} (${d.join(', ')})`);
    console.log(`   click Translate -> verses drawn: median ${median(t)} (${t.join(', ')})`);
  }
}
