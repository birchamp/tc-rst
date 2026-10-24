'use strict';

const test = require('node:test');
const assert = require('node:assert/strict');
const lib = require('../lib.cjs');

const { State, EVENTS, TRANSITIONS, next, initial } = lib;

const ACTIONS = new Set([
  'spawnServer', 'showWindow', 'loadPrepare', 'loadHome', 'reloadHome', 'focusWindow', 'askPage',
  'askUser', 'sendQuit', 'killServer', 'exitApp', 'showStartError',
]);

const sample = {
  ready: {},
  address: { startup: 'ready', at: 0 },
  homeShown: {},
  startError: { reason: 'x', at: 0 },
  serverExit: { at: 0 },
  closeRequested: {},
  pageAnswered: { clean: false },
  pageTimeout: {},
  userAnswered: { close: false },
  secondInstance: {},
  quitTimeout: {},
};

test('every_state_answers_every_event', () => {
  assert.deepEqual(Object.keys(TRANSITIONS).sort(), Object.values(State).sort());
  for (const state of Object.values(State)) {
    assert.deepEqual(Object.keys(TRANSITIONS[state]).sort(), [...EVENTS].sort(), `the events ${state} answers`);
    for (const type of EVENTS) {
      const { machine, actions } = next({ ...initial(), state }, { type, ...sample[type] });
      assert.ok(Object.values(State).includes(machine.state), `${state} + ${type} gives a state`);
      for (const action of actions) assert.ok(ACTIONS.has(action), `${state} + ${type}: ${action}`);
    }
  }
  assert.throws(() => next(initial(), { type: 'nonsense' }), /no answer/);
});

// Runs `events` from the start and gives the state and every action.
function run(events) {
  let machine = initial();
  const actions = [];
  for (const event of events) {
    const step = next(machine, event);
    machine = step.machine;
    actions.push(...step.actions);
  }
  return { state: machine.state, actions };
}

test('the_window_shows_at_ready_before_tc4_answers', () => {
  assert.deepEqual(run([{ type: 'ready' }]), { state: State.Starting, actions: ['showWindow', 'spawnServer'] });
});

test('the_address_line_leads_to_prepare_or_home', () => {
  assert.equal(run([{ type: 'ready' }, { type: 'address', startup: 'preparing', at: 1 }]).state, State.Preparing);
  assert.deepEqual(run([{ type: 'ready' }, { type: 'address', startup: 'ready', at: 1 }]).actions.slice(-1), ['loadHome']);
  const handedOff = run([{ type: 'ready' }, { type: 'address', startup: 'preparing', at: 1 }, { type: 'homeShown' }]);
  assert.equal(handedOff.state, State.Running);
});

test('tc4_ending_restarts_it_and_reloads_home', () => {
  const r = run([
    { type: 'ready' },
    { type: 'address', startup: 'ready', at: 0 },
    { type: 'serverExit', at: 60000 },
    { type: 'address', startup: 'ready', at: 60100 },
  ]);
  assert.equal(r.state, State.Running);
  assert.deepEqual(r.actions.slice(-2), ['spawnServer', 'reloadHome']);
});

test('three_quick_exits_stop_restarting_and_show_the_start_error', () => {
  const events = [{ type: 'ready' }, { type: 'address', startup: 'ready', at: 0 }];
  let at = 0;
  for (let i = 0; i < 2; i++) {
    at += 1000;
    events.push({ type: 'serverExit', at }, { type: 'address', startup: 'ready', at: at + 100 });
  }
  const twice = run(events);
  assert.equal(twice.state, State.Running, 'two quick exits restart');
  events.push({ type: 'serverExit', at: at + 2000 });
  const thrice = run(events);
  assert.equal(thrice.state, State.Stopped);
  assert.deepEqual(thrice.actions.slice(-1), ['showStartError']);
  // A tc4 that ran longer than 10 s starts the count again.
  const slow = run([
    { type: 'ready' },
    { type: 'address', startup: 'ready', at: 0 },
    { type: 'serverExit', at: 1000 },
    { type: 'address', startup: 'ready', at: 1100 },
    { type: 'serverExit', at: 20000 },
    { type: 'address', startup: 'ready', at: 20100 },
    { type: 'serverExit', at: 21000 },
  ]);
  assert.equal(slow.state, State.Restarting);
  // An hour's run, then three quick exits: the third stops.
  const afterLong = [{ type: 'ready' }, { type: 'address', startup: 'ready', at: 0 }, { type: 'serverExit', at: 3600000 }];
  for (let i = 1; i <= 2; i++) afterLong.push({ type: 'address', startup: 'ready', at: 3600000 + i * 1000 }, { type: 'serverExit', at: 3600500 + i * 1000 });
  assert.equal(run(afterLong).state, State.Restarting, 'two quick exits after a long run restart');
  afterLong.push({ type: 'address', startup: 'ready', at: 3610000 }, { type: 'serverExit', at: 3610500 });
  assert.equal(run(afterLong).state, State.Stopped);
  // A held app lock on a restart shows the start error at once.
  const held = run([{ type: 'ready' }, { type: 'address', startup: 'ready', at: 0 }, { type: 'serverExit', at: 60000 }, { type: 'startError', held: true, at: 70000 }]);
  assert.equal(held.state, State.Stopped);
});

test('a_close_while_running_asks_the_page_then_the_person', () => {
  const base = [{ type: 'ready' }, { type: 'address', startup: 'ready', at: 0 }, { type: 'closeRequested' }];
  assert.deepEqual(run(base).actions.slice(-1), ['askPage']);
  assert.deepEqual(run([...base, { type: 'pageAnswered', clean: true }]).actions.slice(-1), ['sendQuit']);
  assert.deepEqual(run([...base, { type: 'pageAnswered', clean: false }]).actions.slice(-1), ['askUser']);
  assert.deepEqual(run([...base, { type: 'pageTimeout' }]).actions.slice(-1), ['askUser']);
  const kept = run([...base, { type: 'pageTimeout' }, { type: 'userAnswered', close: false }]);
  assert.equal(kept.state, State.Running);
  const closed = run([...base, { type: 'pageTimeout' }, { type: 'userAnswered', close: true }]);
  assert.equal(closed.state, State.Quitting);
  assert.deepEqual(closed.actions.slice(-1), ['sendQuit']);
});

test('a_close_while_preparing_quits_without_asking', () => {
  const r = run([{ type: 'ready' }, { type: 'address', startup: 'preparing', at: 0 }, { type: 'closeRequested' }]);
  assert.deepEqual([r.state, r.actions.slice(-1)], [State.Quitting, ['sendQuit']]);
});

test('quitting_kills_tc4_after_5_s', () => {
  const r = run([
    { type: 'ready' },
    { type: 'address', startup: 'preparing', at: 0 },
    { type: 'closeRequested' },
    { type: 'quitTimeout' },
  ]);
  assert.deepEqual(r.actions.slice(-2), ['killServer', 'exitApp']);
});

test('a_second_launch_focuses_the_window_in_every_state', () => {
  for (const state of Object.values(State)) {
    assert.deepEqual(next({ ...initial(), state }, { type: 'secondInstance' }).actions, ['focusWindow'], state);
  }
});

// ---- The model: random lifecycles against a world with real processes ----

// A small seeded generator (mulberry32), so a failure can be run again from its seed.
function rng(seed) {
  let a = seed >>> 0;
  return () => {
    a = (a + 0x6d2b79f5) >>> 0;
    let t = a;
    t = Math.imul(t ^ (t >>> 15), t | 1);
    t ^= t + Math.imul(t ^ (t >>> 7), t | 61);
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  };
}

// The world the executor acts on: tc4 processes, the window's questions and the timers. It
// answers the machine's actions the way main.cjs and the operating system do, and offers the
// events that can happen next.
function simulate(seed, steps) {
  const random = rng(seed);
  const pick = (list) => list[Math.floor(random() * list.length)];
  let machine = initial();
  let now = 0;
  const servers = []; // { id, phase: 'starting'|'up'|'gone', asked }
  let ended = false;
  let window = false;
  let pageAsked = false;
  let userAsked = false;
  let quitTimer = false;
  let maxAlive = 0;
  let serverless = null;
  let stuck = null;
  const trace = [];
  const alive = () => servers.filter((s) => s.phase !== 'gone');
  const current = () => alive()[alive().length - 1];

  const act = (action) => {
    if (action === 'spawnServer') servers.push({ id: servers.length, phase: 'starting', asked: false });
    else if (action === 'showWindow') window = true;
    else if (action === 'askPage') pageAsked = true;
    else if (action === 'askUser') userAsked = true;
    else if (action === 'sendQuit') {
      const s = current();
      if (s) s.asked = true;
      quitTimer = true;
    } else if (action === 'killServer') alive().forEach((s) => (s.phase = 'gone'));
    else if (action === 'exitApp') ended = true;
    maxAlive = Math.max(maxAlive, alive().length);
  };
  const dispatch = (event) => {
    trace.push(`${machine.state} + ${event.type}`);
    const step = next(machine, { at: now, ...event });
    machine = step.machine;
    for (const action of step.actions) {
      act(action);
      if (ended) return;
    }
    // A page that shows as Home or Prepare has a tc4 behind it.
    const live = [State.Preparing, State.Running].includes(machine.state);
    if (live && !alive().some((x) => x.phase === 'up') && serverless === null) serverless = trace.length;
    // A tc4 that never answers must not keep the app open: quitting always has its kill timer.
    if (machine.state === State.Quitting && alive().length > 0 && !quitTimer && stuck === null) stuck = trace.length;
  };

  dispatch({ type: 'ready' });
  for (let i = 0; i < steps && !ended; i++) {
    now += Math.floor(random() * 8000);
    const options = [];
    const s = current();
    if (s && s.phase === 'starting') {
      options.push(() => {
        s.phase = 'up';
        dispatch({ type: 'address', startup: pick(['ready', 'preparing']) });
      });
      options.push(() => {
        s.phase = 'gone';
        dispatch({ type: 'startError', reason: 'exit 4' });
      });
    }
    // A tc4 still starting, whose stdin closed, ends.
    if (s && s.phase === 'starting' && s.asked) options.push(() => {
      s.phase = 'gone';
      dispatch({ type: 'startError', reason: 'stdin closed' });
    });
    if (s && s.phase === 'up') {
      options.push(() => {
        s.phase = 'gone';
        dispatch({ type: 'serverExit' });
      });
      // tc4 answers the quit message by ending.
      if (s.asked) options.push(() => {
        s.phase = 'gone';
        dispatch({ type: 'serverExit' });
      });
    }
    if (window) options.push(() => dispatch({ type: 'closeRequested' }));
    options.push(() => dispatch({ type: 'secondInstance' }));
    if (pageAsked) options.push(() => {
      pageAsked = false;
      dispatch(random() < 0.5 ? { type: 'pageAnswered', clean: random() < 0.5 } : { type: 'pageTimeout' });
    });
    if (userAsked) options.push(() => {
      userAsked = false;
      dispatch({ type: 'userAnswered', close: random() < 0.5 });
    });
    if (quitTimer) options.push(() => {
      quitTimer = false;
      dispatch({ type: 'quitTimeout' });
    });
    // The shell itself crashes: tc4 sees its stdin close and ends (SHELL.md item 3).
    options.push(() => {
      alive().forEach((x) => (x.phase = 'gone'));
      ended = true;
      trace.push('shell crashed');
    });
    pick(options)();
  }
  return { ended, alive: alive().length, maxAlive, serverless, stuck, state: machine.state, trace };
}

test('random_lifecycles_end_with_no_tc4_running', () => {
  const base = Number(process.env.SHELL_MODEL_SEED || Date.now() % 1e9);
  for (let i = 0; i < 2000; i++) {
    const seed = base + i;
    const result = simulate(seed, 60);
    const where = `seed ${seed} (SHELL_MODEL_SEED=${seed}): ${result.trace.join(', ')}`;
    assert.ok(result.maxAlive <= 1, `two tc4 at once, ${where}`);
    assert.equal(result.stuck, null, `quitting with tc4 alive and no kill timer, ${where}`);
    assert.equal(result.serverless, null, `Preparing or Running with no tc4, ${where}`);
    if (result.ended) assert.equal(result.alive, 0, `tc4 left running after the end, ${where}`);
  }
});
