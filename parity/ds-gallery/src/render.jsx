// Turns a case's `render` tree (gallery-spec.json, "nodes") into React elements.
//
//   { "$el": "Button", "props": {...} }  a DS export (capitalised) or an HTML tag (lower case)
//   { "$fn": "noop" }                     a callback that does nothing; only its presence matters
//   { "$asset": "public/assets/x.png" }   a file of the tC4 checkout, served by the gallery
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

export function value(v, key) {
  if (Array.isArray(v)) return v.map((x, i) => value(x, i));
  if (v === null || typeof v !== 'object') return v;
  if ('$el' in v) return element(v, key);
  if ('$fn' in v) return noop;
  if ('$asset' in v) {
    if (!(v.$asset in ASSETS)) throw new Error(`unknown asset ${v.$asset}`);
    return ASSETS[v.$asset];
  }
  return Object.fromEntries(Object.entries(v).map(([k, x]) => [k, value(x)]));
}

function element(node, key) {
  const type = /^[a-z]/.test(node.$el) ? node.$el : DS[node.$el];
  if (!type) throw new Error(`unknown component ${node.$el}`);
  const { children, ...props } = value(node.props ?? {});
  return React.createElement(type, { ...props, key }, children);
}
