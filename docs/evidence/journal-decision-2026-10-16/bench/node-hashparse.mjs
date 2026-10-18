// h2h: Node's (OpenSSL) SHA-256 and V8's JSON.parse over the rebuild's own pack and
// pending bytes, one thread: what the same per-byte work costs in tC4's runtime.
import fs from 'fs';
import path from 'path';
import crypto from 'crypto';
const dir = process.argv[2];
const files = ['packs', 'pending'].flatMap((d) => fs.readdirSync(path.join(dir, d)).sort().map((f) => path.join(dir, d, f)));
const bufs = files.map((f) => fs.readFileSync(f));
const total = bufs.reduce((n, b) => n + b.length, 0);
const now = () => process.hrtime.bigint();
let t = now();
for (const b of bufs) crypto.createHash('sha256').update(b).digest('hex');
const fileSha = Number(now() - t) / 1e6;
const lines = [];
for (const b of bufs) { const s = b.toString('utf8').split('\n'); for (let i = 1; i < s.length; i++) if (s[i]) lines.push(s[i]); }
t = now();
for (const l of lines) { const tab = l.lastIndexOf('\t'); if (crypto.createHash('sha256').update(l.slice(0, tab)).digest('hex') !== l.slice(tab + 1)) throw new Error('hash'); }
const lineSha = Number(now() - t) / 1e6;
t = now();
let n = 0;
for (const l of lines) { const o = JSON.parse(l.slice(0, l.lastIndexOf('\t'))); n += o.seq ? 1 : 0; }
const parse = Number(now() - t) / 1e6;
console.log(JSON.stringify({ files: files.length, bytes: total, lines: lines.length, file_sha_ms: Math.round(fileSha), line_sha_ms: Math.round(lineSha), json_parse_ms: Math.round(parse),
  sha_MBps: +(total / fileSha / 1e3).toFixed(0), parse_MBps: +(total / parse / 1e3).toFixed(0) }));
