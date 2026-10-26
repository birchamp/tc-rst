// Downloads every archive `<dir>/fetched.lock` names that `<dir>` lacks (card G3 step 2): today
// only the OBS picture pack, whose entry names no file. `node fetch-pinned.mjs <dir>` on a copy of
// bundle/en; each archive lands as `<repository>-<commit>.zip`, the name bundle_store looks for,
// and only when its size and sha256 equal the lock's.
import crypto from 'node:crypto';
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const DOOR43 = 'https://git.door43.org';

/** Why `bytes` is not the archive `entry` locks, or null when it is. */
export function refusal(entry, bytes) {
  if (bytes.length !== entry.size) return `${bytes.length} bytes, fetched.lock says ${entry.size}`;
  const sha256 = crypto.createHash('sha256').update(bytes).digest('hex');
  if (sha256 !== entry.sha256.toLowerCase()) return `sha256 ${sha256}, fetched.lock says ${entry.sha256}`;
  return null;
}

/** The file name an entry's archive has in the bundle folder. */
export function fileName(entry) {
  return entry.file ?? `${entry.repo.split('/').pop()}-${entry.commit.toLowerCase()}.zip`;
}

/** Fetches what `dir` lacks; returns the names fetched. Throws naming the archive it refused. */
export async function fetchPinned(dir, { base = DOOR43, tries = 4, wait = 5000, log = console.log } = {}) {
  const lock = JSON.parse(fs.readFileSync(path.join(dir, 'fetched.lock'), 'utf8'));
  const fetched = [];
  for (const entry of lock.entries) {
    const name = fileName(entry);
    const target = path.join(dir, name);
    if (fs.existsSync(target)) continue;
    const url = `${base}/${entry.repo}/archive/${entry.commit}.zip`;
    let why = '';
    for (let attempt = 1; attempt <= tries; attempt++) {
      try {
        const response = await fetch(url);
        if (!response.ok) throw new Error(`HTTP ${response.status}`);
        const bytes = Buffer.from(await response.arrayBuffer());
        why = refusal(entry, bytes);
        // A wrong archive is not a network hiccup: another try would get the same bytes.
        if (why) break;
        const partial = `${target}.partial`;
        fs.writeFileSync(partial, bytes);
        fs.renameSync(partial, target);
        log(`fetched ${name} (${bytes.length} bytes, sha256 ${entry.sha256})`);
        fetched.push(name);
        break;
      } catch (error) {
        why = error.message;
        log(`${url}: try ${attempt} of ${tries} failed: ${why}`);
        if (attempt < tries) await new Promise((done) => setTimeout(done, wait * attempt));
      }
    }
    if (!fs.existsSync(target)) throw new Error(`INV-EVERY-PINNED-FILE-IS-SHIPPED: ${entry.repo} ${entry.commit}: refused ${url}: ${why}`);
  }
  return fetched;
}

if (process.argv[1] === fileURLToPath(import.meta.url)) {
  const dir = process.argv[2];
  if (!dir) {
    console.error('usage: node fetch-pinned.mjs <folder holding fetched.lock>');
    process.exit(2);
  }
  await fetchPinned(dir);
}
