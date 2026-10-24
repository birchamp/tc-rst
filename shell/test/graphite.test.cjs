'use strict';

const test = require('node:test');
const assert = require('node:assert/strict');
const g = require('../graphite.cjs');

test('graphite_verdict_names_each_limit_it_breaks', () => {
  // G0's Electronite on Linux and Windows.
  assert.deepEqual(g.graphiteVerdict({ fontLoaded: true, boxWidth: 302.9, ascent: 61, descent: 13.2 }), { ok: true, broken: [] });
  // Chromium 141 without Graphite: wide and flat.
  const plain = g.graphiteVerdict({ fontLoaded: true, boxWidth: 639.8, ascent: 46, descent: 13 });
  assert.equal(plain.ok, false);
  assert.deepEqual(plain.broken, ['boxWidth 639.8 outside < 480', 'ascent 46 outside 55..70']);
  const nothing = g.graphiteVerdict({ fontLoaded: false, boxWidth: 302.9, ascent: 61, descent: 30 });
  assert.deepEqual(nothing.broken, ['fontLoaded false: the Awami Nastaliq face did not load', 'descent 30 outside 8..20']);
  assert.equal(g.graphiteVerdict({ fontLoaded: true, boxWidth: NaN, ascent: 61, descent: 13 }).ok, false, 'a missing measure fails');
  assert.equal(g.graphiteVerdict({ fontLoaded: true, boxWidth: 480, ascent: 61, descent: 13 }).ok, false, '480 is not joined');
});
