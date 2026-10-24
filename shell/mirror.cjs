'use strict';

// The shell's mirror of typed text (SHELL.md item 6, AD21): every BufferLine the page sends
// with bufferChanged is appended to drafts-buffer.log in the app-data folder, so text typed but
// not yet saved survives tc4, the shell or the computer stopping. This file is its one writer.

const fs = require('node:fs');
const path = require('node:path');

const LOG = 'drafts-buffer.log';
const SYNC_MS = 1000;

const isString = (v) => typeof v === 'string';
const isObject = (v) => typeof v === 'object' && v !== null && !Array.isArray(v);
// serde reads a missing Option field as None.
const optional = (v) => v === undefined || v === null || isObject(v);
const hasOnly = (value, names) => Object.keys(value).every((name) => names.includes(name));

// The key's fields, in the order tc_api::buffer::BufferKey compares them. Like serde's
// deny_unknown_fields there, a key with another field does not parse.
function slotOf(key, instance) {
  if (!isObject(key) || !hasOnly(key, ['project', 'id', 'field'])) return null;
  const { project, id, field } = key;
  if (!isString(project) || !isString(id) || !isObject(field) || !hasOnly(field, ['kind', 'place'])) return null;
  if (!isString(field.kind) || !isString(field.place) || !isString(instance)) return null;
  return JSON.stringify([project, id, field.kind, field.place, instance]);
}

// A line as tc_api::buffer::BufferLine reads it, or null. A changed line must hold a whole
// entry: the page skips one it cannot read, and a half entry kept here would hide the older
// whole one of its field.
function parseLine(line) {
  let value;
  try {
    value = JSON.parse(line);
  } catch (_) {
    return null;
  }
  if (!isObject(value) || !isObject(value.value)) return null;
  const { key, entry, instance, text } = value.value;
  if (value.type === 'changed') {
    if (!isObject(entry)) return null;
    const whole =
      Number.isInteger(entry.api) && Array.isArray(entry.base) && isString(entry.text) &&
      Number.isInteger(entry.at) && optional(entry.sent) && optional(entry.draft);
    const slot = whole ? slotOf(key, entry.instance) : null;
    return slot && { type: 'changed', slot, text: entry.text };
  }
  if (value.type === 'removed') {
    if (text !== undefined && text !== null && !isString(text)) return null;
    const slot = slotOf(key, instance);
    return slot && { type: 'removed', slot, text: isString(text) ? text : null };
  }
  return null;
}

const sameText = (a, b) => a === b || a.normalize('NFC') === b.normalize('NFC');

// tc_ui::app::buffer::current_lines, one line at a time: the last line per field and server
// run; a removal ends its own run's entry, and one that names a text only while the entry holds
// that text; a line that does not parse is skipped.
class Reduction {
  constructor() {
    this.slots = new Map();
  }

  add(line) {
    const parsed = parseLine(line);
    if (!parsed) return;
    if (parsed.type === 'changed') {
      // Deleted first, so the newest change of a slot is also the newest in the Map's order.
      this.slots.delete(parsed.slot);
      this.slots.set(parsed.slot, { line, text: parsed.text });
    } else if (parsed.text === null) {
      this.slots.delete(parsed.slot);
    } else {
      const held = this.slots.get(parsed.slot);
      if (held && sameText(held.text, parsed.text)) this.slots.delete(parsed.slot);
    }
  }

  lines() {
    return [...this.slots.values()].map((slot) => slot.line);
  }
}

function reduce(lines) {
  const reduction = new Reduction();
  for (const line of lines) reduction.add(line);
  return reduction.lines();
}

// Replaces `file` with `text` so that a crash leaves either the old file or the new one:
// a temporary file in the same folder, fsync, rename over the file, fsync the folder.
async function replaceAtomically(file, text, fsp = fs.promises) {
  const dir = path.dirname(file);
  const temporary = path.join(dir, `.${path.basename(file)}.${process.pid}.tmp`);
  const handle = await fsp.open(temporary, 'w');
  try {
    await handle.writeFile(text, 'utf8');
    await handle.sync();
  } finally {
    await handle.close();
  }
  await fsp.rename(temporary, file);
  // Windows cannot open a folder to fsync it; its rename is already durable on NTFS.
  if (process.platform !== 'win32') {
    const folder = await fsp.open(dir, 'r');
    try {
      await folder.sync();
    } finally {
      await folder.close();
    }
  }
}

// Reads the log the last run left, writes it back reduced, and opens it to append. Every call is
// asynchronous: a synchronous write on the main thread would hold up every request the window
// makes, because Electron passes them through that thread (G0: /api/ p95 7-11 ms became 202 ms).
async function openMirror(appData, { fsp = fs.promises, onError = () => {} } = {}) {
  const file = path.join(appData, LOG);
  await fsp.mkdir(appData, { recursive: true });
  let text = '';
  try {
    text = await fsp.readFile(file, 'utf8');
  } catch (error) {
    if (error.code !== 'ENOENT') throw error;
  }
  const reduction = new Reduction();
  for (const line of text.split('\n')) reduction.add(line);
  const kept = reduction.lines();
  await replaceAtomically(file, kept.map((line) => `${line}\n`).join(''), fsp);
  const handle = await fsp.open(file, 'a');

  let queued = [];
  let writing = null;
  let dirty = false;
  let syncing = null;
  let closed = false;

  // One write at a time keeps the lines in order; lines that come meanwhile go out together.
  const drain = () => {
    if (writing || queued.length === 0) return;
    const chunk = queued.join('');
    queued = [];
    writing = handle
      .appendFile(chunk, 'utf8')
      .then(() => {
        dirty = true;
      })
      .catch((error) => onError(error))
      .finally(() => {
        writing = null;
        drain();
      });
  };
  const sync = () => {
    if (syncing) return syncing;
    dirty = false;
    syncing = handle
      .sync()
      .catch((error) => {
        dirty = true;
        onError(error);
      })
      .finally(() => {
        syncing = null;
      });
    return syncing;
  };
  const timer = setInterval(() => {
    if (dirty) sync();
  }, SYNC_MS);
  timer.unref();

  return {
    file,
    // One BufferLine as JSON text. Lines go to the reduction at once, so takeBuffered answers
    // with them before they reach the disk.
    append(line) {
      if (closed || !isString(line) || /[\r\n]/.test(line)) return;
      reduction.add(line);
      queued.push(`${line}\n`);
      drain();
    },
    take() {
      return reduction.lines();
    },
    // Every line written and synced: before the shell ends.
    async flush() {
      while (writing || queued.length > 0) {
        drain();
        await writing;
      }
      await sync();
    },
    async close() {
      await this.flush();
      closed = true;
      clearInterval(timer);
      await handle.close();
    },
  };
}

module.exports = { LOG, Reduction, openMirror, parseLine, reduce, replaceAtomically };
