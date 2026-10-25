'use strict';

const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');

const main = fs.readFileSync(path.join(__dirname, '..', 'main.cjs'), 'utf8');

// The body of `function name(...) { ... }` in main.cjs, by brace count.
function body(name) {
  const start = main.indexOf(`function ${name}(`);
  assert.ok(start >= 0, `main.cjs has function ${name}`);
  let depth = 0;
  for (let i = main.indexOf('{', start); i < main.length; i++) {
    if (main[i] === '{') depth++;
    if (main[i] === '}' && --depth === 0) return main.slice(start, i + 1);
  }
  throw new Error(`function ${name} does not end`);
}

test('only_dispatch_assigns_the_state', () => {
  const assignments = main.match(/\bmachine\s*=(?!=)/g) || [];
  const calls = main.match(/\blib\.next\(/g) || [];
  const dispatch = body('dispatch');
  // `let machine = lib.initial()` and the one assignment in dispatch.
  assert.equal(assignments.length, 2, 'machine is assigned twice: its start and in dispatch');
  assert.match(main, /let machine = lib\.initial\(\);/);
  assert.equal((dispatch.match(/\bmachine\s*=(?!=)/g) || []).length, 1, 'dispatch assigns it');
  assert.equal(calls.length, 1, 'next() is called once');
  assert.match(dispatch, /lib\.next\(/, 'and from dispatch');
  assert.doesNotMatch(main, /machine\.state\s*=(?!=)/, 'nothing sets the state field');
});

test('every_action_of_the_machine_has_an_executor', () => {
  const lib = require('../lib.cjs');
  const actions = new Set();
  for (const state of Object.values(lib.State)) {
    for (const type of lib.EVENTS) {
      const event = { type, at: 0, startup: 'preparing', clean: false, close: true };
      for (const action of lib.next({ ...lib.initial(), state, startedAt: 0, quickExits: 2 }, event).actions) actions.add(action);
    }
  }
  const executor = main.slice(main.indexOf('const ACTIONS = {'), main.indexOf('};', main.indexOf('const ACTIONS = {')));
  for (const action of actions) assert.match(executor, new RegExp(`\\b${action}\\b`), action);
});
