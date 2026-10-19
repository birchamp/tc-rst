// The measurements of unit A5 (docs/evidence/shell-chrome-*.md): the App language preview on
// Home (a person's click on a language to the dialog's title in it, and to the next frame, with
// the long tasks; and the page's own work, from a click() the page makes to the same change),
// the boot to the mounted App, and the switch between help topics (the click to the new
// heading). PERFORMANCE rule 5: release builds, at least 20 runs, the median and the
// p95, the machine and the commit.
//
//   TC4_BIN=target/debug/tc4 TC4_UI=target/tc-ui-app-release \
//     node crates/tc-ui/app/measure-shell.mjs --runs 20 --out docs/evidence/shell-chrome-<date>/release.json
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { LABEL, choose, openLanguage, openMenu, sleep, until, withApp } from '../../../parity/app/journeys/kit.mjs';

const args = Object.fromEntries(process.argv.slice(2).reduce((pairs, arg, i, all) => {
  if (arg.startsWith('--')) pairs.push([arg.slice(2), all[i + 1]]);
  return pairs;
}, []));
const runs = Number(args.runs ?? 20);

const summary = (values) => {
  const sorted = [...values].sort((a, b) => a - b);
  const at = (q) => sorted[Math.min(sorted.length - 1, Math.ceil(q * sorted.length) - 1)];
  return { runs: sorted.length, median: Number(at(0.5).toFixed(1)), p95: Number(at(0.95).toFixed(1)), max: Number(sorted[sorted.length - 1].toFixed(1)) };
};

/** One preview: the click on `label`'s row to the title's new text, and to the next frame. */
async function preview(page, id) {
  await page.getByTestId('language-select').click();
  const option = page.getByRole('listbox').getByRole('option', { name: LABEL[id], exact: true });
  await option.waitFor();
  const timing = page.evaluate(() => new Promise((resolve) => {
    const title = document.querySelector('[data-testid="language-dialog"] [role="dialog"] [id^="tc-dlg"]');
    let clicked = null;
    const onClick = (e) => { if (e.target.closest?.('[role="option"]')) clicked = e.timeStamp; };
    document.addEventListener('click', onClick, true);
    const observer = new MutationObserver(() => {
      if (clicked === null) return;
      observer.disconnect();
      document.removeEventListener('click', onClick, true);
      const dom = performance.now() - clicked;
      requestAnimationFrame(() => resolve({ dom, frame: performance.now() - clicked }));
    });
    observer.observe(title, { childList: true, characterData: true, subtree: true });
  }));
  await option.click();
  return timing;
}

const machine = () => {
  const cpu = fs.readFileSync('/proc/cpuinfo', 'utf8');
  const flags = (/^flags\s*:\s*(.*)$/m.exec(cpu)?.[1] ?? '').split(' ');
  return {
    cpu: /^model name\s*:\s*(.*)$/m.exec(cpu)?.[1] ?? 'unknown',
    vcpus: os.cpus().length,
    sha_ni: flags.includes('sha_ni'),
    avx512f: flags.includes('avx512f'),
    load: os.loadavg(),
  };
};

const result = { ui: path.resolve(process.env.TC4_UI), runs, machine: machine() };

// 1. The preview on Home.
result.preview = await withApp(async ({ page }) => {
  await page.waitForFunction(() => window.__tcIdle === true);
  await openLanguage(page);
  // The cold pair (the first Arabic paint loads its faces) is not part of the measurement.
  await choose(page, 'ar');
  await choose(page, 'en');
  await page.evaluate(() => {
    window.__longTasks = [];
    new PerformanceObserver((list) => list.getEntries().forEach((e) => window.__longTasks.push(Math.round(e.duration)))).observe({ entryTypes: ['longtask'] });
  });
  const times = [];
  for (let i = 0; i < runs; i++) {
    times.push(await preview(page, i % 2 === 0 ? 'fr' : 'en'));
    await sleep(50);
  }
  await sleep(200);
  const longTasks = await page.evaluate(() => window.__longTasks);
  // The page's own work: the same choice made by a click() in the page, so the time Chromium
  // takes to route a pointer input to the page is not in it.
  const work = [];
  for (let i = 0; i < runs; i++) {
    await page.getByTestId('language-select').click();
    await page.getByRole('listbox').waitFor();
    work.push(await page.evaluate((label) => new Promise((resolve) => {
      const title = document.querySelector('[data-testid="language-dialog"] [role="dialog"] [id^="tc-dlg"]');
      const option = [...document.querySelectorAll('[role="listbox"] [role="option"]')].find((o) => o.textContent.includes(label));
      const observer = new MutationObserver(() => {
        observer.disconnect();
        const dom = performance.now() - started;
        requestAnimationFrame(() => resolve({ dom, frame: performance.now() - started }));
      });
      observer.observe(title, { childList: true, characterData: true, subtree: true });
      const started = performance.now();
      option.click();
    }), LABEL[i % 2 === 0 ? 'fr' : 'en']));
    await sleep(50);
  }
  return {
    dom: summary(times.map((t) => t.dom)),
    frame: summary(times.map((t) => t.frame)),
    longTasks,
    workDom: summary(work.map((t) => t.dom)),
    workFrame: summary(work.map((t) => t.frame)),
  };
});

// 2. The boot: navigation start to the mounted App (`tc4:mounted`).
result.boot = await withApp(async ({ page, env }) => {
  const marks = [];
  for (let i = 0; i < runs; i++) {
    await page.goto('about:blank');
    await page.goto(`${env.origin}/#t=${env.token}`);
    await page.getByTestId('account-menu').waitFor();
    marks.push(await page.evaluate(() => performance.getEntriesByName('tc4:mounted')[0]?.startTime));
  }
  return summary(marks);
});

// 3. A help topic switch: the click on a link to the new page's heading.
result.helpSwitch = await withApp(async ({ page }) => {
  await openMenu(page);
  await page.getByTestId('account-help').click();
  const frame = page.frameLocator('[data-testid="help-frame"]');
  await until('the help home', async () => (await frame.getByRole('heading', { level: 1 }).textContent().catch(() => null)) === 'translationCore 4 Help');
  const topics = ['section-bible-projects.html', 'bp-new-bible.html', 'bp-open-book.html', 'bp-import.html', 'bp-export-pdf.html'];
  const times = [];
  for (let i = 0; i < runs; i++) {
    const href = topics[i % topics.length];
    const help = page.frames().find((f) => f.url().includes('/help/'));
    const before = await frame.getByRole('heading', { level: 1 }).textContent();
    const timing = help.evaluate((was) => new Promise((resolve) => {
      let clicked = null;
      document.addEventListener('click', (e) => { clicked = e.timeStamp; }, { capture: true, once: true });
      const poll = () => {
        const now = document.querySelector('h1')?.textContent;
        if (clicked !== null && now && now !== was) resolve(performance.now() - clicked);
        else requestAnimationFrame(poll);
      };
      poll();
    }), before);
    await frame.locator(`nav a[href="${href}"]:visible`).click();
    times.push(await timing);
    await sleep(100);
  }
  return summary(times);
});

result.machineAfter = machine();
const text = `${JSON.stringify(result, null, 2)}\n`;
if (args.out) {
  fs.mkdirSync(path.dirname(path.resolve(args.out)), { recursive: true });
  fs.writeFileSync(args.out, text);
}
console.log(text);
