// The mock's Door43 stand-in answers the sync transport as pankosmia-web does: a push sets
// Door43 `main`, a clone copies it to the fixed path, a copy moves a folder, a delete removes
// one, and each refusal carries the text that tC4 matches. The fake Door43 API answers the
// branch check from the same folders. Node built-ins only, so CI runs it:
//
//   node --test parity/tc4-baseline/mock-server/server.test.mjs
import assert from 'node:assert/strict';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import test from 'node:test';
import { createMockServer } from './server.mjs';
import { FAKE_USER, fakeDoor43 } from './fake-door43.mjs';

const PROJECT = '_local_/_local_/team_tit';
const ORIGIN = 'https://git.door43.org/facilitator-zq/team_tit.git';
const CLONE = 'git.door43.org/facilitator-zq/team_tit';
const SCRATCH = '_local_/_sync_/team_tit';

/** A state directory with one shared project, and a running mock over it. */
async function start(t) {
  const state = fs.mkdtempSync(path.join(os.tmpdir(), 'tc4-baseline-mock-'));
  t.after(() => fs.rmSync(state, { recursive: true, force: true }));
  const dir = path.join(state, 'repos', ...PROJECT.split('/'));
  fs.mkdirSync(path.join(dir, 'ingredients', 'checking', 'journal', 'a1'), { recursive: true });
  fs.writeFileSync(path.join(dir, 'metadata.json'), '{"identification":{"name":{"en":"Team Titus"}}}\n');
  fs.writeFileSync(path.join(dir, 'ingredients', 'TIT.usfm'), '\\id TIT\n');
  fs.writeFileSync(path.join(dir, 'ingredients', 'checking', 'journal', 'a1', 'actor.json'), '{}\n');
  fs.writeFileSync(path.join(dir, '.remotes.json'), JSON.stringify([{ name: 'origin', url: ORIGIN }]));
  const mock = createMockServer({ state, client: state });
  await new Promise((resolve) => mock.server.listen(0, '127.0.0.1', resolve));
  t.after(() => new Promise((resolve) => mock.server.close(resolve)));
  const base = `http://127.0.0.1:${mock.server.address().port}/api`;
  const call = async (method, route, body) => {
    const response = await fetch(`${base}${route}`, {
      method,
      ...(body ? { headers: { 'Content-Type': 'application/json' }, body: JSON.stringify(body) } : {}),
    });
    return { status: response.status, body: await response.json() };
  };
  const repo = (repoPath, ...rest) => path.join(state, 'repos', ...repoPath.split('/'), ...rest);
  const main = path.join(state, 'door43', ...CLONE.split('/'));
  return { mock, call, repo, main };
}

const PUSH = { remote: 'origin', cred_type: 'https', username: 'facilitator-zq', pass_key: 'token' };

test('a push sets Door43 main, and a clone copies main to the fixed path with its origin', async (t) => {
  const { call, repo, main } = await start(t);
  assert.deepEqual(await call('POST', `/git/push/${PROJECT}`, PUSH), { status: 401, body: { is_good: false, reason: 'offline mode' } });
  assert.equal(fs.existsSync(main), false, 'an offline push sends nothing');

  await call('POST', '/net/enable');
  assert.equal((await call('POST', `/git/push/${PROJECT}`, PUSH)).status, 200);
  assert.equal(fs.readFileSync(path.join(main, 'ingredients', 'TIT.usfm'), 'utf8'), '\\id TIT\n');
  assert.equal(fs.existsSync(path.join(main, '.remotes.json')), false, 'the pusher\'s remotes stay on the computer');
  assert.equal((await call('POST', `/git/push/${PROJECT}`, { ...PUSH, remote: 'upstream' })).status, 400);

  assert.equal((await call('POST', `/git/clone-repo/${CLONE}`)).status, 200);
  assert.equal(fs.readFileSync(repo(CLONE, 'ingredients', 'checking', 'journal', 'a1', 'actor.json'), 'utf8'), '{}\n');
  assert.deepEqual((await call('GET', `/git/remotes/${CLONE}`)).body.payload.remotes, [{ name: 'origin', url: `https://${CLONE}` }]);
  assert.deepEqual((await call('GET', `/git/branches/${CLONE}`)).body.payload.branches, [{ name: 'main', is_head: true }]);

  // tC4's refusalForClone reads this code as sync.clone-path-exists
  const again = await call('POST', `/git/clone-repo/${CLONE}`);
  assert.equal(again.status, 400);
  assert.match(again.body.reason, /code=Exists \(-4\)/);
});

test('a clone of a repository that Door43 does not hold fails as a failed clone, and changes nothing', async (t) => {
  const { mock, call, repo } = await start(t);
  assert.deepEqual(await call('POST', `/git/clone-repo/${CLONE}`), { status: 401, body: { is_good: false, reason: 'offline mode' } });
  await call('POST', '/net/enable');
  const missing = await call('POST', `/git/clone-repo/${CLONE}`);
  assert.equal(missing.status, 400);
  assert.match(missing.body.reason, /^could not clone repo: /);
  assert.doesNotMatch(missing.body.reason, /code=Exists/);
  assert.equal(fs.existsSync(repo(CLONE)), false);

  mock.door43.push(PROJECT);
  assert.equal((await call('POST', `/git/clone-repo/${CLONE}`)).status, 200);
  await call('POST', `/git/delete/${CLONE}`);
  mock.door43.remove(PROJECT);
  assert.equal((await call('POST', `/git/clone-repo/${CLONE}`)).status, 400, 'a repository taken off Door43 no longer clones');
});

test('a copy with delete_src moves the folder, and a delete removes it', async (t) => {
  const { call, repo } = await start(t);
  const copy = (src, target, deleteSrc) => call('POST', `/git/copy/${src}?target_path=${target}${deleteSrc ? '&delete_src=true' : ''}`);

  assert.equal((await copy(PROJECT, SCRATCH, false)).status, 200);
  assert.ok(fs.existsSync(repo(PROJECT, 'metadata.json')), 'without delete_src the source stays');
  assert.deepEqual(await copy(PROJECT, SCRATCH, false), { status: 400, body: { is_good: false, reason: 'Target repo already exists' } });

  const moved = '_local_/_sync_/moved';
  assert.equal((await copy(SCRATCH, moved, true)).status, 200);
  assert.equal(fs.existsSync(repo(SCRATCH)), false, 'delete_src deletes the source');
  assert.equal(fs.readFileSync(repo(moved, 'ingredients', 'TIT.usfm'), 'utf8'), '\\id TIT\n');
  assert.deepEqual(await copy(SCRATCH, '_local_/_sync_/other', false), { status: 400, body: { is_good: false, reason: 'Source repo not found or not a directory' } });

  assert.equal((await call('POST', `/git/delete/${moved}`)).status, 200);
  assert.equal(fs.existsSync(repo(moved)), false);
  // tC4's isNotFound reads this io error as "not there", so a sync's clean-up goes on
  assert.deepEqual(await call('POST', `/git/delete/${moved}`), {
    status: 400, body: { is_good: false, reason: 'could not delete repo: No such file or directory (os error 2)' },
  });
});

test('Door43 main exists for the branch check exactly while a clone would find it', async (t) => {
  const { mock } = await start(t);
  const answer = fakeDoor43({ door43: mock.door43 });
  const api = (route) => new URL(`https://git.door43.org/api/v1${route}`);
  const basic = `Basic ${Buffer.from(`${FAKE_USER.username}:${FAKE_USER.password}`).toString('base64')}`;
  const { sha1 } = answer('POST', api(`/users/${FAKE_USER.username}/tokens`), { authorization: basic }, '{"name":"tc4"}').body;
  // door43Api.ts hasBranch: 200 is a branch, 404 is none, and tC4 then skips the sync
  const hasMain = () => answer('GET', api(`/repos/${FAKE_USER.username}/team_tit/branches/main`), { authorization: `token ${sha1}` }).status;

  assert.equal(hasMain(), 404, 'a repository that Door43 does not hold has no main');
  mock.door43.push(PROJECT);
  assert.equal(hasMain(), 200);
  assert.equal(answer('GET', api(`/repos/${FAKE_USER.username}/team_tit/branches/draft`), { authorization: `token ${sha1}` }).status, 404);
  mock.door43.remove(PROJECT);
  assert.equal(hasMain(), 404, 'a repository taken off Door43 has no main');
});

test('a clone whose connection drops fails once as a failed clone, and leaves nothing behind', async (t) => {
  const { mock, call, repo } = await start(t);
  await call('POST', '/net/enable');
  mock.door43.push(PROJECT);
  mock.door43.dropNextClone(PROJECT);

  const dropped = await call('POST', `/git/clone-repo/${CLONE}`);
  assert.equal(dropped.status, 400);
  // tC4's refusalForClone reads anything but offline mode and code=Exists as sync.clone-failed
  assert.equal(dropped.body.reason, 'could not clone repo: failed to connect to git.door43.org; class=Os (2)');
  assert.equal(fs.existsSync(repo(CLONE)), false);

  assert.equal((await call('POST', `/git/clone-repo/${CLONE}`)).status, 200, 'only the next clone drops');
});
