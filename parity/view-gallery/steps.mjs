// The steps a gallery case takes after its state is drawn, shared by the gallery's capture and
// the Rust app's (parity/app/capture-rust.mjs --case). A target is a Playwright selector over
// the whole page (CSS, with `>> nth=N` to pick a later match); its first match must exist.

export const STEP_KINDS = ['click', 'press', 'type', 'fill', 'hover', 'focus', 'wait', 'refocus', 'blur'];

/** Why `step` is not a step of the vocabulary, or null. */
export function stepProblem(step) {
  if (!step || typeof step !== 'object' || !STEP_KINDS.includes(step.do)) return `unknown step ${JSON.stringify(step)}`;
  const need = { click: ['target'], press: ['key'], type: ['text'], fill: ['target', 'text'], hover: ['target'], focus: ['target'], wait: ['target'], refocus: ['target'], blur: [] }[step.do];
  const keys = Object.keys(step).filter((k) => k !== 'do');
  const missing = need.filter((k) => typeof step[k] !== 'string');
  const extra = keys.filter((k) => !need.includes(k));
  if (missing.length || extra.length) return `step ${JSON.stringify(step)} needs exactly ${need.join(', ')}`;
  return null;
}

/** Waits for the fonts and two frames, then `ms`. */
export const still = (page, ms) => page.evaluate(async (wait) => {
  await document.fonts.ready;
  await new Promise((r) => requestAnimationFrame(() => requestAnimationFrame(r)));
  await new Promise((r) => setTimeout(r, wait));
}, ms);

/** Tab until `target` has the focus, as a person reaches it by keyboard. */
async function focusByKeyboard(page, target) {
  for (let i = 0; i < 80; i++) {
    await page.keyboard.press('Tab');
    if (await page.evaluate((sel) => document.activeElement?.matches(sel) ?? false, target)) return;
  }
  throw new Error(`Tab never reached ${target}`);
}

/**
 * Runs `steps` on the page: `{ do: 'click', target }`, `{ do: 'press', key }` (a key or a
 * chord, as Playwright names it), `{ do: 'type', text }` (into the focused element),
 * `{ do: 'fill', target, text }`, `{ do: 'hover', target }`, `{ do: 'focus', target }` (Tab
 * until it has the focus, at most 80 presses) and `{ do: 'wait', target }` (until it shows).
 * Two steps put back the focus a route left, which a state drawn afresh does not have:
 * `{ do: 'refocus', target }` focuses the target by script, as a click had left it, with no
 * keyboard ring and no action called, and `{ do: 'blur' }` sends the focus to the page, as when
 * the focused control went away. `onStep(i)` runs before step i; after each step, the fonts, two
 * frames and 300 ms.
 */
export async function runSteps(page, steps, onStep = async () => {}) {
  for (const [i, step] of steps.entries()) {
    const problem = stepProblem(step);
    if (problem) throw new Error(problem);
    await onStep(i);
    const target = step.target === undefined ? null : page.locator(step.target).first();
    try {
      switch (step.do) {
        case 'click': await target.click({ timeout: 10_000 }); break;
        case 'press': await page.keyboard.press(step.key); break;
        case 'type': await page.keyboard.type(step.text); break;
        case 'fill': await target.fill(step.text, { timeout: 10_000 }); break;
        case 'hover': await target.hover({ timeout: 10_000 }); break;
        case 'focus': await focusByKeyboard(page, step.target); break;
        case 'wait': await target.waitFor({ timeout: 10_000 }); break;
        case 'refocus': await target.evaluate((el) => el.focus({ focusVisible: false }), null, { timeout: 10_000 }); break;
        case 'blur': await page.evaluate(() => document.activeElement?.blur()); break;
        default: throw new Error(`unknown step ${step.do}`);
      }
    } catch (error) {
      throw new Error(`step ${i + 1} (${JSON.stringify(step)}) failed: ${error.message.split('\n')[0]}`);
    }
    await still(page, 300);
  }
}
