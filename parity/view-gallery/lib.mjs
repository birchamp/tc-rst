// The pure parts of the tC4 view-state gallery that Node and the page share: carrying tC4's
// state across JSON, the stub's `actions` object, the action trace, and the DOM digest that
// calibration compares. No import, so the page's bundle and Node load the same code, and
// `encodeValue` is self-contained so a recorder can ship its source to real tC4's page.

/** Tags for values JSON cannot carry. An object with exactly one key from TAGS is a tag. */
export const TAGS = ['$u8', '$part'];

/**
 * Walks `value` (named `path` in refusals) and returns `{ value, refused }`: the value as
 * plain JSON, and one "<path>: <why>" for everything that is not plain JSON, which a state
 * must never hold: undefined (as a property or in an array), functions, symbols, bigints,
 * non-finite numbers, Map, Set, Date, class instances, cycles, strings that start with
 * "blob:" (a URL that dies with its page), and plain objects whose only key is a tag name.
 * A Uint8Array (an uploaded file's bytes) becomes `{ $u8: <base64> }`. A value shared by two
 * places is written twice, as JSON writes it; a value that holds itself is a cycle.
 *
 * Self-contained: it closes over nothing in this module, so `String(encodeValue)` runs in a
 * page that never loaded it.
 */
export function encodeValue(value, path = 's') {
  const tags = ['$u8', '$part'];
  const refused = [];
  const ancestors = [];
  const base64 = (bytes) => {
    let binary = '';
    for (let i = 0; i < bytes.length; i += 0x8000) binary += String.fromCharCode.apply(null, bytes.subarray(i, i + 0x8000));
    return btoa(binary);
  };
  const walk = (v, at) => {
    if (v === null || typeof v === 'boolean') return v;
    if (typeof v === 'string') {
      if (v.startsWith('blob:')) refused.push(`${at}: a blob: URL`);
      return v;
    }
    if (typeof v === 'number') {
      if (!Number.isFinite(v)) refused.push(`${at}: the number ${v}`);
      return v;
    }
    if (v === undefined) {
      refused.push(`${at}: undefined`);
      return null;
    }
    if (typeof v !== 'object') {
      refused.push(`${at}: a ${typeof v}`);
      return null;
    }
    if (ancestors.includes(v)) {
      refused.push(`${at}: a cycle`);
      return null;
    }
    if (v instanceof Uint8Array) return { $u8: base64(v) };
    const proto = Object.getPrototypeOf(v);
    if (Array.isArray(v)) {
      ancestors.push(v);
      const out = Array.from(v, (x, i) => walk(x, `${at}[${i}]`));
      ancestors.pop();
      return out;
    }
    if (proto !== Object.prototype && proto !== null) {
      refused.push(`${at}: a ${proto?.constructor?.name ?? 'class'} instance`);
      return null;
    }
    const keys = Object.keys(v);
    if (keys.length === 1 && tags.includes(keys[0])) {
      refused.push(`${at}: a plain object whose only key is the tag ${keys[0]}`);
      return null;
    }
    ancestors.push(v);
    const out = {};
    for (const key of keys) out[key] = walk(v[key], /^[A-Za-z_$][\w$]*$/.test(key) ? `${at}.${key}` : `${at}[${JSON.stringify(key)}]`);
    ancestors.pop();
    return out;
  };
  const encoded = walk(value, path);
  return { value: encoded, refused };
}

/** The inverse of `encodeValue`, in the page: `{ $u8 }` becomes a Uint8Array again. Every
 * other value is returned as it is (a new object or array where something inside changed). */
export function reviveValue(value) {
  if (Array.isArray(value)) return value.map(reviveValue);
  if (value === null || typeof value !== 'object') return value;
  const keys = Object.keys(value);
  if (keys.length === 1 && keys[0] === '$u8') return Uint8Array.from(atob(value.$u8), (c) => c.charCodeAt(0));
  if (keys.length === 1 && keys[0] === '$part') throw new Error(`a state part (${value.$part}) reached the page unjoined`);
  const out = {};
  for (const key of keys) out[key] = reviveValue(value[key]);
  return out;
}

/** tC4's reducer `set` (state.jsx `reducer`): `{ ...s, ...patch }`. A nested slice in the
 * patch replaces the recorded one whole. */
export const applyPatch = (s, patch) => ({ ...s, ...patch });

/** A promise that never settles: an action whose reply never comes. */
const PENDING = () => new Promise(() => {});

/**
 * The stub's `actions`: a Proxy whose `get` gives ONE function per name, made on first use,
 * as tC4 makes its actions object once (state.jsx `useMemo(..., [])`), so an effect that lists
 * an action among its dependencies runs once. Symbols and `then` give undefined, so the object
 * is never taken for a promise. Each call runs `record({ name, args })`, then answers:
 * `selectors[name](s, args)` when there is a selector; else by `returns[name]`:
 * `{ value }` gives the value, `{ resolves }` a promise of it, `{ rejects: message }` a
 * promise rejected with Error(message), `{ pending: true }` a promise that never settles;
 * with no reply, undefined.
 */
export function makeActions({ s, returns = {}, selectors = {}, record }) {
  const made = new Map();
  const answer = (name, args) => {
    if (Object.hasOwn(selectors, name)) return selectors[name](s, args);
    const reply = Object.hasOwn(returns, name) ? returns[name] : undefined;
    if (!reply) return undefined;
    if ('value' in reply) return reply.value;
    if ('resolves' in reply) return Promise.resolve(reply.resolves);
    if ('rejects' in reply) return Promise.reject(new Error(reply.rejects));
    if (reply.pending === true) return PENDING();
    throw new Error(`the reply of ${name} is none of value, resolves, rejects, pending: ${JSON.stringify(reply)}`);
  };
  return new Proxy({}, {
    get(_, name) {
      if (typeof name !== 'string' || name === 'then') return undefined;
      if (!made.has(name)) {
        made.set(name, (...args) => {
          record({ name, args });
          return answer(name, args);
        });
      }
      return made.get(name);
    },
  });
}

/**
 * Arguments as the trace stores them, as plain JSON: a React synthetic event becomes
 * `{ $event: type }`, a DOM node `{ $node: nodeName }`, a function `{ $fn: true }`, undefined
 * `{ $undefined: true }`, a Uint8Array `{ $u8len: n }`, a value inside itself
 * `{ $cycle: true }`, anything deeper than 6 levels `{ $deep: true }`, a non-finite number
 * `{ $number: "NaN" }`, a bigint `{ $bigint: "12" }`, and an instance of another class
 * `{ $instance: <class name> }`.
 */
export function traceArgs(args) {
  const ancestors = [];
  const walk = (v, depth) => {
    if (v === undefined) return { $undefined: true };
    if (typeof v === 'function') return { $fn: true };
    if (typeof v === 'bigint') return { $bigint: String(v) };
    if (typeof v === 'symbol') return { $symbol: String(v.description) };
    if (typeof v === 'number') return Number.isFinite(v) ? v : { $number: String(v) };
    if (v === null || typeof v !== 'object') return v;
    if (ancestors.includes(v)) return { $cycle: true };
    if (v instanceof Uint8Array) return { $u8len: v.length };
    if (typeof v.nativeEvent === 'object' && typeof v.type === 'string') return { $event: v.type };
    if (typeof v.nodeType === 'number' && typeof v.nodeName === 'string') return { $node: v.nodeName };
    if (depth > 6) return { $deep: true };
    const proto = Object.getPrototypeOf(v);
    if (!Array.isArray(v) && proto !== Object.prototype && proto !== null) return { $instance: proto?.constructor?.name ?? 'unknown' };
    ancestors.push(v);
    const out = Array.isArray(v)
      ? v.map((x) => walk(x, depth + 1))
      : Object.fromEntries(Object.keys(v).map((k) => [k, walk(v[k], depth + 1)]));
    ancestors.pop();
    return out;
  };
  return walk(args, 0);
}

/** The gate's rule for generated ids (APP-DESIGN 7.3 layer 2, `dom.rs` `renumber`):
 * `tc-(f|g|dlg|cfm|rg)N`, the counters of tC4's Field, FieldGroup, Modal, ConfirmDialog and
 * Radio, renumbered per prefix by first appearance across `lines` in order. */
export function renumberIds(lines) {
  const seen = new Map();
  const next = new Map();
  return lines.map((line) => line.replace(/\btc-(f|g|dlg|cfm|rg)(\d+)/g, (_, prefix, n) => {
    const key = `${prefix}${n}`;
    if (!seen.has(key)) {
      const k = (next.get(prefix) ?? 0) + 1;
      next.set(prefix, k);
      seen.set(key, k);
    }
    return `tc-${prefix}#${seen.get(key)}`;
  }));
}

/** What calibration compares of a `domSnapshot`: everything it holds, with generated ids
 * renumbered as the gate renumbers them. */
export const domDigestInput = (dom) => JSON.stringify([dom.html, dom.outside, renumberIds(dom.root), dom.styles]);
