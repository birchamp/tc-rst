// tC4 #764 repro (report only, outside the gate's glob): resolve one Translation Words check in
// the real app, then measure each progress bar's fill in px on the picker card and in the tool's
// rail, with its aria-valuenow and aria-label.
import path from 'node:path';
import { browserTest, until, withApp } from '../../../../parity/app/journeys/kit.mjs';

const SHOTS = path.resolve(import.meta.dirname, '..', 'shots');
const scopeOf = async (env) => (await env.call('app/hello', {})).sessions[0].at;

const bars = (page, scope) => page.locator(`${scope} [role="progressbar"]`).evaluateAll((els) => els.map((el) => {
  const fill = el.firstElementChild;
  return {
    in: el.closest('[data-testid]')?.getAttribute('data-testid'),
    track: Math.round(el.getBoundingClientRect().width * 100) / 100,
    fill: fill ? Math.round(fill.getBoundingClientRect().width * 100) / 100 : null,
    styleWidth: fill?.style.width, minWidth: fill?.style.minWidth,
    valuenow: el.getAttribute('aria-valuenow'), label: el.getAttribute('aria-label'),
  };
}));

browserTest('764_one_resolved_check_fills_the_bars', async () => {
  await withApp(async ({ page, env }) => {
    await page.getByTestId('project-sample_burrito').getByRole('button', { name: /Titus/ }).click();
    await page.getByRole('tab', { name: 'Check', exact: true }).click();
    await page.getByTestId('open-translationWords').waitFor();
    console.log('picker before:', JSON.stringify(await bars(page, 'body')));
    await page.getByTestId('open-translationWords').click();
    await page.getByTestId('check-progress').waitFor();
    await page.getByTestId('check-quote').waitFor();
    console.log('rail before:', JSON.stringify(await bars(page, 'body')));
    const row = page.locator('[data-testid="check-list"] [aria-current="true"]');
    const index = Number(await row.getAttribute('data-rail-index'));
    await page.getByTestId('tw-0').click();
    await page.getByTestId('mark-valid').click();
    const list = { tool: 'translationWords', unit: { type: 'book', value: 'TIT' } };
    await until('the check is resolved', async () => (await env.call('check/item', { at: await scopeOf(env), list, index })).decision.value?.status === 'valid');
    await page.waitForTimeout(800);
    console.log('rail after 1:', JSON.stringify(await bars(page, 'body')));
    console.log('rail count line:', (await page.getByTestId('check-progress').innerText()).replace(/\n/g, ' | '));
    await page.screenshot({ path: path.join(SHOTS, '764-rail.png') });
    await page.getByTestId('check-back').click();
    await page.getByTestId('open-translationWords').waitFor();
    await page.waitForTimeout(800);
    console.log('picker after 1:', JSON.stringify(await bars(page, 'body')));
    console.log('tW card text:', (await page.getByTestId('open-translationWords').innerText()).replace(/\n/g, ' | '));
    await page.screenshot({ path: path.join(SHOTS, '764-picker.png') });
  }, { scenario: 'en-pins' });
});
