// tC4 #731 on the real app: Arabic chosen as the app language, Report a problem from the
// account menu, no Report kept. Reads what the dialog shows: the label above the box, the
// attachment, and any line that says (in Arabic) why the attachment is English.
//
//   cd <worktree> && t=$PWD/target && TC4_BIN=$t/debug/tc4 TC4_UI=$t/tc-ui-app TC4_UI_HOOKS=$t/tc-ui-app-hooks TC4_DOOR43_FAKE=$t/debug/tc-door43-fake tc-heavy node --test --test-timeout=180000 docs/evidence/fixed-in-4.1-2026-10-29/repro/731-attachment-in-arabic.test.mjs
import assert from 'node:assert/strict';
import os from 'node:os';
import path from 'node:path';
import { browserTest, catalog, openMenu, until, withApp } from '../../../../parity/app/journeys/kit.mjs';

const SHOTS = path.resolve(import.meta.dirname, '..', 'shots');
const ar = catalog('ar');

browserTest('731_report_a_problem_in_arabic', async () => {
  await withApp(async ({ page }) => {
    await openMenu(page);
    await page.getByTestId('account-report').click();
    const dialog = page.getByTestId('feedback');
    await dialog.waitFor();
    const box = page.getByTestId('feedback-attachment');
    await until('the attachment', async () => /no Report\n?$/.test(await box.textContent()));
    const shown = await box.textContent();
    const all = await dialog.textContent();
    console.log('html lang/dir:', await page.evaluate(() => `${document.documentElement.lang}/${document.documentElement.dir}`));
    console.log('attachment shown:', JSON.stringify(shown));
    console.log('Arabic label feedback.attachment in dialog:', all.includes(ar['feedback.attachment']));
    // Any Arabic sentence that mentions English (الإنجليزية) or the help desk next to the box.
    console.log('dialog mentions English in Arabic (الإنجليزية):', /الإنجليزي/.test(all));
    const arch = { x64: 'x86_64', arm64: 'aarch64' }[os.arch()] ?? os.arch();
    console.log('platform line ends with this machine\'s arch:', shown.split('\n')[0].endsWith(arch), arch);
    await page.waitForTimeout(800); // the menu and the dialog finish fading in
    await page.screenshot({ path: path.join(SHOTS, '731-attachment-in-arabic.png') });
    assert.match(shown, /^translationCore .*\nno Report\n?$/);
    assert.ok(!shown.includes('MacIntel'));
  }, { locale: 'ar' });
});
