// Turns a case's `render` tree (gallery-spec.json, "nodes") into React elements.
//
//   { "$el": "Button", "props": {...} }  a DS export (capitalised) or an HTML tag (lower case)
//   { "$fn": "noop" }                     a callback that does nothing; only its presence matters
//   { "$asset": "public/assets/x.png" }   a file of the tC4 checkout, served by the gallery
//
//   { "$cell": "on" }                     the current value of one of the case's `cells`
//   { "$fn": "set", "cell": "on", "from": "0.target.checked" }
//                                         a callback that writes a cell: `value`, or the value
//                                         at `from` in its arguments (0 is the first)
//
// Everything else is plain JSON and passes through unchanged.
import React from 'react';
import * as DS from '@ds/index.js';
import wordmarkLogo from '@tc4/public/assets/translationcore-logo.png';
import markLogo from '@tc4/public/assets/logo-mark-color.png';

const ASSETS = {
  'public/assets/translationcore-logo.png': wordmarkLogo,
  'public/assets/logo-mark-color.png': markLogo,
};

const noop = () => {};

/** The value at `path` ("0.target.checked") in a callback's arguments. */
function pick(args, path) {
  return String(path).split('.').reduce((v, k) => (v == null ? v : v[k]), args);
}

/** A tree value as React values. `cells` holds the case's cells and their setter. */
export function value(v, key, cells) {
  if (Array.isArray(v)) return v.map((x, i) => value(x, i, cells));
  if (v === null || typeof v !== 'object') return v;
  if ('$el' in v) return element(v, key, cells);
  if ('$cell' in v) return cells.values[v.$cell];
  if ('$fn' in v) {
    if (v.$fn !== 'set') return noop;
    return (...args) => cells.set(v.cell, 'value' in v ? v.value : pick(args, v.from));
  }
  if ('$asset' in v) {
    if (!(v.$asset in ASSETS)) throw new Error(`unknown asset ${v.$asset}`);
    return ASSETS[v.$asset];
  }
  return Object.fromEntries(Object.entries(v).map(([k, x]) => [k, value(x, undefined, cells)]));
}

function element(node, key, cells) {
  const type = /^[a-z]/.test(node.$el) ? node.$el : DS[node.$el];
  if (!type) throw new Error(`unknown component ${node.$el}`);
  const { children, ...props } = value(node.props ?? {}, undefined, cells);
  return React.createElement(type, { ...props, key }, children);
}

/** A case's tree with its cells: an owner that keeps them as state and draws again on a write. */
export function Tree({ tree, initial }) {
  const [values, setValues] = React.useState(initial ?? {});
  const set = React.useCallback((name, x) => setValues((s) => ({ ...s, [name]: x })), []);
  return value(tree, undefined, { values, set });
}
