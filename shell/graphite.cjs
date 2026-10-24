'use strict';

// The P8 probe (card G4): whether the browser shaped an Awami Nastaliq line with Graphite.
// Without Graphite the letters do not join or cascade: the line is about twice as wide and
// sits flat (INV-GRAPHITE-SHAPES-OR-FAILS).

const PROBE_TEXT = 'اردو نستعلیق میں لکھی جاتی ہے';
const PROBE_PX = 32;

// G0 at 32 px (docs/evidence/electronite-spike-linux-2026-10-17.md §5 and the Windows file):
// Electronite 37.1.0 on Linux and Windows width 302.9, ascent 61.0, descent 13.2; Chromium 141
// without Graphite width 639.8, ascent 46. G4 measured the shell on all three (shell.yml run
// 40121642886): width 302.9 everywhere, ascent 61 / 61 / 60.7, descent 13.2 / 13.2 / 12.6 on
// Linux, Windows and macOS; Playwright's Chromium 633, 46, 18. Each limit keeps 4 px to spare.
const JOINED_MAX_WIDTH = 480;
const ASCENT_MIN = 55;
const ASCENT_MAX = 70;
const DESCENT_MIN = 8;
const DESCENT_MAX = 17.5;

// { ok, broken: ['<measure> <value> outside <limit>', ...] } for one measurement.
function graphiteVerdict({ fontLoaded, boxWidth, ascent, descent }) {
  const broken = [];
  if (fontLoaded !== true) broken.push('fontLoaded false: the Awami Nastaliq face did not load');
  if (!(boxWidth < JOINED_MAX_WIDTH)) broken.push(`boxWidth ${boxWidth} outside < ${JOINED_MAX_WIDTH}`);
  if (!(ascent >= ASCENT_MIN && ascent <= ASCENT_MAX)) broken.push(`ascent ${ascent} outside ${ASCENT_MIN}..${ASCENT_MAX}`);
  if (!(descent >= DESCENT_MIN && descent <= DESCENT_MAX)) broken.push(`descent ${descent} outside ${DESCENT_MIN}..${DESCENT_MAX}`);
  return { ok: broken.length === 0, broken };
}

// Measures the probe line in `page` (a Playwright page) in the face at `fontUrl`, as the spike's
// probe did (spikes/electronite/page/probe.js), without its screenshot.
async function measureProbe(page, fontUrl) {
  return page.evaluate(
    async ({ text, px, fontUrl }) => {
      const round = (n) => Math.round(n * 10) / 10;
      let fontLoaded = false;
      try {
        const face = new FontFace('AwamiProbe', `url(${JSON.stringify(fontUrl)})`);
        document.fonts.add(await face.load());
        fontLoaded = (await document.fonts.load(`${px}px "AwamiProbe"`, text)).length > 0;
      } catch (_) {
        fontLoaded = false;
      }
      const line = document.createElement('p');
      line.lang = 'ur';
      line.dir = 'rtl';
      line.style.cssText = 'position:fixed;top:0;left:0;margin:0;white-space:nowrap';
      const span = document.createElement('span');
      span.style.cssText = `font:${px}px "AwamiProbe"`;
      span.textContent = text;
      line.append(span);
      document.body.append(line);
      await new Promise((resolve) => requestAnimationFrame(() => requestAnimationFrame(resolve)));
      const box = span.getBoundingClientRect();
      const context = document.createElement('canvas').getContext('2d');
      context.font = `${px}px "AwamiProbe"`;
      const metrics = context.measureText(text);
      line.remove();
      return {
        fontLoaded,
        boxWidth: round(box.width),
        boxHeight: round(box.height),
        canvasWidth: round(metrics.width),
        ascent: round(metrics.actualBoundingBoxAscent),
        descent: round(metrics.actualBoundingBoxDescent),
      };
    },
    { text: PROBE_TEXT, px: PROBE_PX, fontUrl },
  );
}

module.exports = { ASCENT_MAX, ASCENT_MIN, DESCENT_MAX, DESCENT_MIN, JOINED_MAX_WIDTH, PROBE_TEXT, graphiteVerdict, measureProbe };
