// Node-only helpers of the gallery: the committed state files and their shared parts.
import crypto from 'node:crypto';
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

export const HERE = path.dirname(fileURLToPath(import.meta.url));
export const REPO = path.resolve(HERE, '..', '..');
export const STATES_DIR = path.join(HERE, 'states');
export const PARTS_DIR = path.join(STATES_DIR, 'parts');

export const sha256 = (bytes) => crypto.createHash('sha256').update(bytes).digest('hex');

/** A JSON file as the gallery writes it: two-space indents and a final newline. */
export const prettyJson = (value) => `${JSON.stringify(value, null, 2)}\n`;

/** A part's file: compact JSON and a final newline (parts are large data: a book's sources). */
export const partJson = (value) => `${JSON.stringify(value)}\n`;

/**
 * Moves every top-level value of `s` whose JSON is over `minBytes` into a part named by the
 * first 16 hex digits of the sha256 of its file: answers `{ s, parts }`, `s` holding
 * `{ $part: id }` in its place and `parts` mapping each id to its file's text. Parts are
 * content-addressed, so states of one scenario share them (the Titus sources of Translate,
 * Check and Align are one file).
 */
export function splitParts(s, { minBytes = 65536 } = {}) {
  const out = {};
  const parts = {};
  for (const [key, value] of Object.entries(s)) {
    const text = partJson(value);
    if (Buffer.byteLength(text) <= minBytes) {
      out[key] = value;
      continue;
    }
    const id = sha256(text).slice(0, 16);
    parts[id] = text;
    out[key] = { $part: id };
  }
  return { s: out, parts };
}

/** The inverse of `splitParts`: each top-level `{ $part: id }` read from `dir/<id>.json`,
 * whose sha256 must start with the id. */
export function joinParts(s, dir = PARTS_DIR) {
  const out = {};
  for (const [key, value] of Object.entries(s)) {
    const id = value && typeof value === 'object' && !Array.isArray(value) && Object.keys(value).length === 1 ? value.$part : undefined;
    if (id === undefined) {
      out[key] = value;
      continue;
    }
    const bytes = fs.readFileSync(path.join(dir, `${id}.json`));
    if (!sha256(bytes).startsWith(id)) throw new Error(`state part ${id} does not have the sha256 its name gives: record the states again`);
    out[key] = JSON.parse(bytes.toString('utf8'));
  }
  return out;
}

/** The parts a state file names. */
export const partsOf = (s) => Object.values(s).filter((v) => v && typeof v === 'object' && !Array.isArray(v) && Object.keys(v).length === 1 && typeof v.$part === 'string').map((v) => v.$part);

/** A recorded state, parts joined: `{ name, s, returns }`. */
export function readState(name, dir = STATES_DIR) {
  const file = JSON.parse(fs.readFileSync(path.join(dir, `${name}.json`), 'utf8'));
  if (file.name !== name) throw new Error(`states/${name}.json holds the state ${file.name}`);
  return { name, s: joinParts(file.s, path.join(dir, 'parts')), returns: file.returns };
}

/** The names of the files under `dir`, relative to it, sorted; none when it is absent. */
export function filesUnder(dir) {
  if (!fs.existsSync(dir)) return [];
  const out = [];
  const walk = (at) => {
    for (const entry of fs.readdirSync(path.join(dir, at), { withFileTypes: true })) {
      const rel = at ? `${at}/${entry.name}` : entry.name;
      if (entry.isDirectory()) walk(rel);
      else out.push(rel);
    }
  };
  walk('');
  return out.sort();
}
