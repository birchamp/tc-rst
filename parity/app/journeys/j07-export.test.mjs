// Exports (D2): tC4's J7 (a facilitator exports the book: aligned and plain USFM, the PDF) and
// J23 (an OBS project as Story Markdown and the PDF), from the Community Checking export menu,
// and #420 (an aligned word keeps its `\add` style, which tC4 4.0 drops). The PDF item shows
// only with the shell's `printPdf`; the page gets a stub with the shell's contract (it takes the
// app's `/print/{job}` URL and answers `{ok: true, file}`, the file named from the page's
// `<title>`), and the stub prints that URL with this Chromium's `page.pdf()`, as tC4's journey
// prints with its bridge double. Each test hashes every project file but `.git/` before and
// after its exports and fails on any difference (tC4 J7: "MUST NOT: write anything else into
// the project"). The rules of each producer are tested in tc-export and tc-app; this file is
// what a person sees and gets.
//
//   TC4_BIN=target/debug/tc4 TC4_UI=target/tc-ui-app node --test parity/app/journeys/j07-export.test.mjs
import assert from 'node:assert/strict';
import crypto from 'node:crypto';
import fs from 'node:fs';
import path from 'node:path';
import zlib from 'node:zlib';
import { newRequestId } from '../env-rust.mjs';
import { browserTest, catalog, until, withApp } from './kit.mjs';

const en = catalog('en');
const BIBLE = 'sample_burrito';
const OBS = 'sample_burrito_obs';
const DATE = String.raw`\d{4}-\d{2}-\d{2}`;

/** sha256 of every file of `dir` but `.git/`, by relative path. */
function hashes(dir) {
  const out = {};
  const walk = (d, base) => {
    for (const e of fs.readdirSync(d, { withFileTypes: true })) {
      const rel = base ? `${base}/${e.name}` : e.name;
      if (rel === '.git') continue;
      if (e.isDirectory()) walk(path.join(d, e.name), rel);
      else out[rel] = crypto.createHash('sha256').update(fs.readFileSync(path.join(d, e.name))).digest('hex');
    }
  };
  walk(dir, '');
  return out;
}

/** The project's hashes once two reads 300 ms apart agree: a save before has landed. */
async function settled(dir) {
  return until('the project at rest', async () => {
    const first = JSON.stringify(hashes(dir));
    await new Promise((resolve) => setTimeout(resolve, 300));
    return first === JSON.stringify(hashes(dir)) && JSON.parse(first);
  });
}

/** Runs `exports` and fails if any project file but `.git/` changed. */
async function unchanged(dir, exports) {
  const before = await settled(dir);
  const result = await exports();
  assert.deepEqual(hashes(dir), before, 'every project file is byte-identical after the export');
  return result;
}

/** The entries of a zip, by name: the central directory, stored or deflated. */
function unzip(bytes) {
  const end = bytes.lastIndexOf(Buffer.from([0x50, 0x4b, 0x05, 0x06]));
  assert.ok(end >= 0, 'the download is a zip');
  const out = {};
  let at = bytes.readUInt32LE(end + 16);
  for (let n = bytes.readUInt16LE(end + 10); n > 0; n -= 1) {
    const method = bytes.readUInt16LE(at + 10);
    const size = bytes.readUInt32LE(at + 20);
    const nameLength = bytes.readUInt16LE(at + 28);
    const name = bytes.toString('utf8', at + 46, at + 46 + nameLength);
    const local = bytes.readUInt32LE(at + 42);
    const start = local + 30 + bytes.readUInt16LE(local + 26) + bytes.readUInt16LE(local + 28);
    const data = bytes.subarray(start, start + size);
    if (!name.endsWith('/')) out[name] = method === 0 ? Buffer.from(data) : zlib.inflateRawSync(data);
    at += 46 + nameLength + bytes.readUInt16LE(at + 30) + bytes.readUInt16LE(at + 32);
  }
  return out;
}

/** A USFM book cut before each `\c` and `\v`, each piece keyed by its chapter and verse. */
function pieces(usfm) {
  let chapter = '0';
  return usfm.split(/(?=\\[cv] )/).map((text) => {
    const c = text.match(/^\\c (\d+)/);
    if (c) chapter = c[1];
    const v = text.match(/^\\v (\S+)/);
    return { key: v ? `${chapter}:${v[1]}` : `${chapter}:head`, text };
  });
}

/** Titus from its tile, then Check, then Community Checking. */
async function openTitusPreview(page) {
  await page.getByTestId(`project-${BIBLE}`).getByRole('button', { name: /Titus/ }).click();
  await page.getByTestId('understand-unit-s1').waitFor();
  await page.getByTestId('mode-switch').getByRole('tab', { name: 'Check' }).click();
  await page.getByTestId('open-community-checking').click();
  await page.getByTestId('cc-page').first().waitFor();
}

/** Story 1 of the OBS sample from its tile, then Check, then Community Checking. */
async function openStoryPreview(page, env) {
  await page.getByTestId(`project-${env.projectId(`_local_/_local_/${OBS}`)}`).getByTestId('story-tile-1').click();
  await page.getByTestId('story-understand').waitFor();
  await page.getByTestId('mode-switch').getByRole('tab', { name: 'Check' }).click();
  await page.getByTestId('open-community-checking').click();
  await page.getByTestId('cc-pictures').waitFor();
}

/** The export menu's items, in order. */
async function menuItems(page) {
  await page.getByTestId('export-menu-trigger').click();
  const items = page.getByRole('menuitem');
  await items.first().waitFor();
  const names = (await items.allTextContents()).map((s) => s.trim());
  await page.keyboard.press('Escape');
  return names;
}

/** Presses `item` and answers the file the browser downloads, and the toast that follows. */
async function download(page, item) {
  await page.getByTestId('export-menu-trigger').click();
  const [file] = await Promise.all([
    page.waitForEvent('download'),
    page.getByRole('menuitem', { name: item, exact: true }).click(),
  ]);
  assert.equal(await file.failure(), null, 'the download finished');
  const bytes = fs.readFileSync(await file.path());
  const name = file.suggestedFilename();
  await page.getByTestId('export-toast').getByText(`Export ready: ${name}`, { exact: true }).waitFor();
  assert.equal(await page.getByTestId('export-failure').count(), 0, 'no export failure');
  return { name, bytes };
}

/** The token for the page's downloads. A `<a download>` request does not carry Playwright's
 * `extraHTTPHeaders`, so it would be refused for want of the token the shell's filter adds; the
 * debug build's cookie (tc-server `security.rs`) stands in for that filter. */
const downloads = (context, env) => context.addCookies([{ name: 'tc4_token', value: env.token, url: env.origin }]);

/** The shell's `printPdf` for the page, with G1's contract: the app's own `/print/{job}` URL in,
 * this Chromium's PDF of it out, saved under the page's `<title>`. Answers what it printed. */
async function stubPrintPdf(context, env) {
  const printed = [];
  await context.exposeFunction('__tcPrintPdf', async (url) => {
    const printer = await context.newPage();
    try {
      await printer.goto(new URL(url, env.origin).href);
      const html = await printer.content();
      const title = await printer.title();
      const pdf = await printer.pdf({ preferCSSPageSize: true });
      printed.push({ url, html, title, pdf });
      return { ok: true, file: `${title}.pdf` };
    } finally {
      await printer.close();
    }
  });
  await context.addInitScript(() => {
    window.tc4Shell = { printPdf: (url) => window.__tcPrintPdf(url) };
  });
  return printed;
}

/** Presses Export PDF and answers what the stub printed, once tC4's "Saved {filename}" shows. */
async function printPdf(page, printed) {
  const before = printed.length;
  await page.getByTestId('export-menu-trigger').click();
  await page.getByRole('menuitem', { name: en['cc.exportPdf'], exact: true }).click();
  await until('the print', () => printed.length > before);
  const pdf = printed.at(-1);
  const saved = en['cc.exportSaved'].replace('{filename}', `${pdf.title}.pdf`);
  await page.getByTestId('export-toast').getByText(saved, { exact: true }).waitFor();
  assert.equal(await page.getByTestId('export-failure').count(), 0, 'no export failure');
  assert.equal(printed.length, before + 1, 'one print per press');
  return pdf;
}

/** What a printed PDF is: its bytes start as a PDF and it has `pages` pages of A4. */
function assertPdf(pdf, pages) {
  assert.equal(pdf.subarray(0, 5).toString(), '%PDF-');
  const text = pdf.toString('latin1');
  assert.equal((text.match(/\/Type\s*\/Page[^s]/g) ?? []).length, pages, 'its pages');
  assert.match(text, /\/MediaBox\s*\[\s*0 0 594\.9\d* 841\.9\d*\s*\]/, 'A4');
}

browserTest('J7: aligned and plain USFM download from the export menu and leave the project unchanged', async () => {
  await withApp(async ({ page, env }) => {
    await openTitusPreview(page);
    const project = path.join(env.projectsDir, BIBLE);
    assert.deepEqual(await menuItems(page), [en['cc.exportUsfmAligned'], en['cc.exportUsfmPlain'], en['cc.exportBurritoZip']],
      'a browser has no PDF item; the Burrito zip is listed');
    const [aligned, plain] = await unchanged(project, async () => [
      await download(page, en['cc.exportUsfmAligned']),
      await download(page, en['cc.exportUsfmPlain']),
    ]);
    const stored = fs.readFileSync(path.join(project, 'ingredients', 'TIT.usfm'));

    // Plain: the stored book, byte for byte (it holds no inline \zaln).
    assert.ok(plain.bytes.equals(stored), 'plain USFM is the stored book');

    // Aligned: every piece the stored book's, but 1:1, the one verse the sample aligns, woven.
    const out = pieces(aligned.bytes.toString('utf8'));
    const book = pieces(stored.toString('utf8'));
    assert.deepEqual(out.map((p) => p.key), book.map((p) => p.key), 'the same chapters and verses');
    const woven = out.filter((p) => p.text.includes('\\zaln-s')).map((p) => p.key);
    assert.deepEqual(woven, ['1:1']);
    for (const [i, p] of out.entries()) if (p.key !== '1:1') assert.equal(p.text, book[i].text, p.key);
    assert.match(out.find((p) => p.key === '1:1').text, /\\zaln-s \|x-strong="G39720"[^\n]*x-content="Παῦλος"\\\*\\w Pablo\|/);
    assert.equal(await page.getByTestId('export-skipped').count(), 0, 'no verse left out');

  }, { init: downloads });
});

browserTest('J7: an aligned word in an add span keeps the span in the aligned USFM (tC4 #420)', async () => {
  await withApp(async ({ page, env }) => {
    await openTitusPreview(page);
    // Titus 1:1 with its aligned "siervo" marked as added, saved as the translator's save writes it.
    const at = (await env.call('app/hello', {})).sessions[0].at;
    const view = await env.call('editor/chapters', { at, book: 'TIT' });
    const row = view.chapters[0].verses.find((v) => v.key === '1');
    const words = 'Pablo, \\add siervo\\add* de Dios y apóstol de Jesucristo, según la fe de los escogidos de Dios y el conocimiento de la verdad que va con la piedad,';
    await env.call('editor/saveVerse', { at, request: newRequestId(), shown: row.words.token, words });
    const project = path.join(env.projectsDir, BIBLE);
    await until('the save on disk', () => fs.readFileSync(path.join(project, 'ingredients', 'TIT.usfm'), 'utf8').includes('\\add siervo\\add*'));
    // The first export's checkpoint commits the save (its change log and metadata.json), as
    // tC4's J7 excepts the D9 checkpoint commit; the project is compared from after it.
    await download(page, en['cc.exportUsfmAligned']);

    const [aligned, plain] = await unchanged(project, async () => [
      await download(page, en['cc.exportUsfmAligned']),
      await download(page, en['cc.exportUsfmPlain']),
    ]);
    assert.equal(await page.getByTestId('export-skipped').count(), 0, 'the verse is woven, not left out');
    const verse = pieces(aligned.bytes.toString('utf8')).find((p) => p.key === '1:1').text;
    assert.match(verse, /\\zaln-s [^\n]*x-content="δοῦλος"/, 'siervo is aligned in the file');
    // tC4 4.0 writes `\w siervo|…\w*` with the span gone (#420); the rebuild keeps it around the word.
    assert.match(verse, /\\add [^]*?\\w siervo\|[^]*?\\add\*/, verse);
    assert.match(plain.bytes.toString('utf8'), /\\add siervo\\add\*/);
  }, { init: downloads });
});

browserTest('J7: the PDF prints the print page of its job and leaves the project unchanged', async () => {
  let printed;
  await withApp(async ({ page, env }) => {
    await openTitusPreview(page);
    const project = path.join(env.projectsDir, BIBLE);
    assert.deepEqual(await menuItems(page), [en['cc.exportPdf'], en['cc.exportUsfmAligned'], en['cc.exportUsfmPlain'], en['cc.exportBurritoZip']],
      'with the shell, the PDF item comes first');
    const pdf = await unchanged(project, () => printPdf(page, printed));
    assert.match(pdf.url, /^\/print\/\d+$/, 'the shell is given the app\'s own print page');
    // Chapter 1 has verses 1 to 5 drafted; chapters 2 and 3 have none and are left out.
    assert.equal((pdf.html.match(/<section class="print-chapter"/g) ?? []).length, 1);
    assert.ok(pdf.html.includes('[ verse not yet drafted ]'), 'verses 6 to 16');
    assert.ok(pdf.html.includes('>Titus</h1>'), 'the heading is the book\'s name');
    assertPdf(pdf.pdf, 1);
  }, { init: async (context, env) => { await downloads(context, env); printed = await stubPrintPdf(context, env); } });
});

browserTest('J23: Story Markdown holds content/ as stored, the PDF prints, and the project is unchanged', async () => {
  let printed;
  await withApp(async ({ page, env }) => {
    await openStoryPreview(page, env);
    const project = path.join(env.projectsDir, OBS);
    assert.deepEqual(await menuItems(page), [en['cc.exportPdf'], en['cc.exportStoryMarkdown'], en['cc.exportBurritoZip']]);
    const [markdown, pdf] = await unchanged(project, async () => [
      await download(page, en['cc.exportStoryMarkdown']),
      await printPdf(page, printed),
    ]);

    // The zip is content/ exactly as stored.
    const content = path.join(project, 'ingredients', 'content');
    const stored = Object.keys(hashes(content)).filter((n) => path.basename(n) !== '.DS_Store' && !n.endsWith('.bak')).sort();
    const zipped = unzip(markdown.bytes);
    assert.deepEqual(Object.keys(zipped).sort(), stored.map((n) => `content/${n}`));
    assert.ok(stored.includes('50.md'));
    for (const [name, bytes] of Object.entries(zipped)) assert.ok(bytes.equals(fs.readFileSync(path.join(project, 'ingredients', name))), name);

    // The PDF prints the drafted story.
    assert.match(pdf.url, /^\/print\/\d+$/);
    assert.ok(pdf.html.includes('class="print-story"'), 'a printed story');
    assert.equal(pdf.pdf.subarray(0, 5).toString(), '%PDF-');
  }, { init: async (context, env) => { await downloads(context, env); printed = await stubPrintPdf(context, env); } });
});

browserTest('J7 and J23: each export is named as tC4 names it', async () => {
  let printed;
  await withApp(async ({ page, env }) => {
    // tC4's `exportFilename`: the book's code (`TIT`) or the project's name, then the date. The
    // shell names the PDF from the print page's title.
    const dated = (name) => name.replace(new RegExp(DATE), '<date>');
    await openTitusPreview(page);
    const got = {
      aligned: dated((await download(page, en['cc.exportUsfmAligned'])).name),
      plain: dated((await download(page, en['cc.exportUsfmPlain'])).name),
      pdf: dated(`${(await printPdf(page, printed)).title}.pdf`),
    };
    await env.load(page, { locale: 'en' });
    await openStoryPreview(page, env);
    got.stories = dated((await download(page, en['cc.exportStoryMarkdown'])).name);
    got.storiesPdf = dated(`${(await printPdf(page, printed)).title}.pdf`);
    const name = JSON.parse(fs.readFileSync(path.join(env.projectsDir, OBS, 'metadata.json'), 'utf8')).identification.name.en;
    assert.deepEqual(got, {
      aligned: 'TIT-aligned-<date>.usfm',
      plain: 'TIT-<date>.usfm',
      pdf: 'TIT-<date>.pdf',
      stories: `${name}-stories-<date>.zip`,
      storiesPdf: `${name}-<date>.pdf`,
    });
  }, { init: async (context, env) => { await downloads(context, env); printed = await stubPrintPdf(context, env); } });
});

browserTest('J7: a save of the book while its export runs leaves the export\'s answer on screen', async () => {
  // The shell's printPdf held open until the book's preview has refreshed with another save:
  // the refresh rebuilds the screen, and the export's "Saved" toast must still show.
  let release;
  const held = new Promise((resolve) => { release = resolve; });
  let asked = false;
  await withApp(async ({ page, env }) => {
    await openTitusPreview(page);
    await page.getByTestId('export-menu-trigger').click();
    await page.getByRole('menuitem', { name: en['cc.exportPdf'], exact: true }).click();
    await until('the print asked', () => asked);
    const at = (await env.call('app/hello', {})).sessions[0].at;
    const view = await env.call('editor/chapters', { at, book: 'TIT' });
    const row = view.chapters[0].verses.find((v) => v.key === '2');
    const words = 'saved while the export runs';
    await env.call('editor/saveVerse', { at, request: newRequestId(), shown: row.words.token, words });
    await page.getByTestId('cc-page').getByText(words).first().waitFor();
    assert.ok(await page.getByTestId('export-menu-trigger').isDisabled(), 'Export stays off while the export runs');
    release();
    const saved = en['cc.exportSaved'].replace('{filename}', 'TIT.pdf');
    await page.getByTestId('export-toast').getByText(saved, { exact: true }).waitFor();
    assert.equal(await page.getByTestId('export-toast').count(), 1, 'the toast shows once');
  }, {
    init: async (context, env) => {
      await downloads(context, env);
      await context.exposeFunction('__tcPrintPdf', async () => {
        asked = true;
        await held;
        return { ok: true, file: 'TIT.pdf' };
      });
      await context.addInitScript(() => {
        window.tc4Shell = { printPdf: (url) => window.__tcPrintPdf(url) };
      });
    },
  });
});
