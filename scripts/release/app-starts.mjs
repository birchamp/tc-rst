// The installed app starts (card G3): its own launcher, with no arguments, brings up the shell,
// and the shell starts tc4 on the app-data folder launch.cjs names. A server smoke test cannot
// see that: it starts tc4 itself.
//
//   node app-starts.mjs --exe <installed app executable> --app-data <dir launch.cjs names>
//
// Passes when shell.log in that folder says "tc4 started"; fails on "tc4 refused to start" or
// "tc4 did not start", or after a minute, which is a hang.
import { spawn } from 'node:child_process';
import fs from 'node:fs';
import path from 'node:path';
import { parseArgs } from 'node:util';

const { values } = parseArgs({ options: { exe: { type: 'string' }, 'app-data': { type: 'string' } } });
if (!values.exe || !values['app-data']) throw new Error('--exe and --app-data are needed');
const log = path.join(values['app-data'], 'shell.log');
fs.rmSync(log, { force: true });

const app = spawn(values.exe, [], { stdio: 'inherit', detached: process.platform !== 'win32' });
let ended = null;
app.on('exit', (code, signal) => { ended = { code, signal }; });
const stop = () => {
  if (ended) return;
  // The shell's children (tc4, Chromium's helpers) go with it.
  if (process.platform === 'win32') spawn('taskkill', ['/pid', String(app.pid), '/t', '/f'], { stdio: 'ignore' });
  else process.kill(-app.pid, 'SIGKILL');
};

const end = Date.now() + 60_000;
let verdict = null;
while (!verdict) {
  const text = fs.existsSync(log) ? fs.readFileSync(log, 'utf8') : '';
  if (text.includes('tc4 started')) verdict = { ok: true };
  else if (/tc4 (refused to start|did not start)/.test(text)) verdict = { ok: false, why: text.slice(-2000) };
  else if (ended) verdict = { ok: false, why: `the app ended (${JSON.stringify(ended)}) before tc4 started; shell.log: ${text.slice(-2000)}` };
  else if (Date.now() > end) verdict = { ok: false, why: `hung: no "tc4 started" in ${log} after a minute; shell.log: ${text.slice(-2000)}` };
  else await new Promise((resolve) => setTimeout(resolve, 250));
}
stop();
if (verdict.ok) console.log('ok app_starts_and_starts_tc4');
else {
  console.log(`FAIL app_starts_and_starts_tc4: ${verdict.why}`);
  process.exitCode = 1;
}
