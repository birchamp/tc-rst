// The tokens page. Every value is read from tC4's token files, so a token added or changed there
// shows up here on the next `npm run spec`. A specimen is built from DS Text and plain elements
// with inline styles that read the token (var(--name)), never a copied value.
import fs from 'node:fs';
import path from 'node:path';
import { el, group, onDark, T } from './lib.mjs';

/** Custom properties declared in the file's plain `:root { … }` blocks, in source order, each
 * with the `---- Section ----` comment it sits under. */
function rootTokens(file) {
  const css = fs.readFileSync(file, 'utf8');
  const out = [];
  let section = null;
  for (const block of css.matchAll(/([^{}]*)\{([^{}]*)\}/g)) {
    const selector = block[1].replace(/\/\*[\s\S]*?\*\//g, '').trim();
    if (selector !== ':root') continue;
    for (const line of block[2].split('\n')) {
      const head = line.match(/\/\*\s*-{4}\s*(.+?)\s*-{4}\s*\*\//);
      if (head) section = head[1];
      const decl = line.replace(/\/\*.*?\*\//g, '').match(/^\s*(--[\w-]+)\s*:\s*([^;]+);/);
      if (decl) out.push({ name: decl[1], value: decl[2].trim(), section });
    }
  }
  return out;
}

const SWATCH_W = 150;
const swatch = ({ name, value }) => el('div', {
  style: { width: SWATCH_W, display: 'flex', flexDirection: 'column', gap: 4 },
  children: [
    el('div', { style: { height: 44, borderRadius: 8, background: `var(${name})`, border: '1px solid rgba(35,31,32,.10)' } }),
    el('Text', { role: 'captionStrong', as: 'div', style: { overflowWrap: 'anywhere' }, children: name }),
    el('Text', { role: 'caption', as: 'div', style: { overflowWrap: 'anywhere' }, children: value }),
  ],
});
const wrapRow = (children, gap = 12) => el('div', { style: { display: 'flex', flexWrap: 'wrap', gap }, children });

/** A frame tall enough for `n` items of height `h` laid `perRow` to a row. */
const gridFrame = (n, perRow, itemW, h, gap = 12, pad = 16) => ({
  width: pad * 2 + perRow * itemW + (perRow - 1) * gap,
  height: pad * 2 + Math.ceil(n / perRow) * h + (Math.ceil(n / perRow) - 1) * gap,
});

const slug = (s) => s.toLowerCase().replace(/\(.*?\)/g, '').trim().replace(/[^a-z0-9]+/g, '-').replace(/^-|-$/g, '');
const TONES = ['neutral', 'accent', 'ocean', 'cultivate', 'valid', 'warn', 'invalid'];
const SLOTS = ['tone', 'fill', 'on-fill', 'soft', 'on-soft', 'text', 'border', 'hover', 'press'];
const CONTEXT = ['--fg', '--fg-strong', '--fg-muted', '--fg-faint', '--line', '--line-strong', '--fill-plain', '--fill-quiet', '--fill-quiet-hover'];

const line = (children, gap = 16) => el('div', { style: { display: 'flex', alignItems: 'baseline', gap }, children });
const nameTag = (text) => el('Text', { role: 'captionStrong', style: { width: 150, flex: 'none' }, children: text });

export function tokenCases(tc4) {
  const dir = path.join(tc4, 'src/ds/tokens');
  const read = (f) => rootTokens(path.join(dir, f));
  const colors = read('colors.css');
  const typography = read('typography.css');
  const sections = [];
  for (const t of colors) {
    const s = sections.find((x) => x.title === t.section);
    if (s) s.tokens.push(t); else sections.push({ title: t.section, tokens: [t] });
  }
  /* A swatch is 104 px with a one-line value; a long value (a gradient) wraps at about 24 characters. */
  const swatchFrame = (n, values = []) => gridFrame(n, 5, SWATCH_W, 104 + 18 * Math.max(0, ...values.map((v) => Math.ceil(v.length / 24) - 1)));

  const uiSizes = typography.filter((t) => /^--fs-/.test(t.name) && !/verse|dropcap|adjust/.test(t.name));
  const verseSteps = [['sm', 'sm'], ['', ''], ['md', 'md'], ['lg', 'lg']].map(([f]) => ({
    fs: `--fs-verse${f ? '-' + f : ''}`, lh: `--lh-verse${f ? '-' + f : ''}`,
  }));
  const spacing = read('spacing.css').filter((t) => /^--space-/.test(t.name));
  const radius = read('radius.css').filter((t) => t.name !== '--radius-pill').concat({ name: '--radius-pill', value: '999px' });
  const strokes = read('border.css').filter((t) => /^--stroke/.test(t.name));
  const shadows = read('elevation.css');
  const controls = read('control.css').filter((t) => /^--control-h-/.test(t.name));
  const ROLES = ['display', 'h1', 'h2', 'h3', 'title', 'titleSm', 'strong', 'body', 'ui', 'caption', 'captionStrong',
    'overline', 'label', 'labelNum', 'labelMicro', 'meta', 'scripture', 'scriptureMd', 'scriptureSm', 'quote'];

  return group('Tokens', {}, [
    ...sections.map(({ title, tokens }) => [`color-${slug(title)}`, { children: wrapRow(tokens.map(swatch)) }, {
      frame: swatchFrame(tokens.length, tokens.map((t) => t.value)), note: `colors.css, section “${title}”.`,
    }]),
    ...TONES.map((tone) => [`tone-${tone}`, {
      'data-tone': tone, children: wrapRow(SLOTS.map((s) => swatch({ name: s === 'tone' ? '--tone' : `--tone-${s}`, value: `--t-${tone}${s === 'tone' ? '' : '-' + s}` }))),
    }, { frame: swatchFrame(SLOTS.length), note: 'tone.css: the nine slots under data-tone.' }]),
    ...TONES.map((tone) => [`tone-${tone}-on-dark`, {
      'data-tone': tone, children: wrapRow(SLOTS.map((s) => swatch({ name: s === 'tone' ? '--tone' : `--tone-${s}`, value: `--t-${tone}${s === 'tone' ? '' : '-' + s}` }))),
    }, {
      frame: { width: swatchFrame(SLOTS.length).width + 28, height: swatchFrame(SLOTS.length).height + 28 },
      wrap: (n) => onDark(n),
      note: 'context.css: the same slots inside a data-on="dark" ground (a solid Ocean Surface).',
    }]),
    ['context-light', { children: wrapRow(CONTEXT.map((name) => swatch({ name, value: 'data-on="light"' }))) }, { frame: swatchFrame(CONTEXT.length) }],
    ['context-dark', { children: wrapRow(CONTEXT.map((name) => swatch({ name, value: 'data-on="dark"' }))) }, {
      frame: { width: swatchFrame(CONTEXT.length).width + 28, height: swatchFrame(CONTEXT.length).height + 28 }, wrap: (n) => onDark(n),
    }],
    ['type-scale-ui', { style: { display: 'flex', flexDirection: 'column', gap: 10 }, children: uiSizes.map((t) => line([
      nameTag(`${t.name} ${t.value}`),
      el('span', { style: { fontFamily: 'var(--font-ui)', fontSize: `var(${t.name})`, fontWeight: 'var(--fw-heavy)', lineHeight: 1.2, color: 'var(--text-body)' }, children: 'Translate the Bible' }),
    ])) }, { frame: { width: 640, height: 16 * 2 + uiSizes.reduce((h, t) => h + Math.ceil(parseFloat(t.value) * 1.2) + 10, -10) + 24 } }],
    ['type-scale-scripture', { style: { display: 'flex', flexDirection: 'column', gap: 10 }, children: [
      ...verseSteps.map(({ fs, lh }) => line([
        nameTag(`${fs} / ${lh}`),
        el('span', { style: { fontFamily: 'var(--font-scripture)', fontSize: `var(${fs})`, lineHeight: `var(${lh})`, color: 'var(--text-scripture)' }, children: 'Paul, a servant of God' }),
      ])),
      line([nameTag('--fs-dropcap'), el('span', { style: { fontFamily: 'var(--font-scripture)', fontSize: 'var(--fs-dropcap)', lineHeight: 1, fontWeight: 700, color: 'var(--uw-inspire-700)' }, children: '2' })]),
    ] }, { frame: { width: 640, height: 300 } }],
    ['type-weights', { style: { display: 'flex', flexDirection: 'column', gap: 8 }, children: [
      ...['regular', 'medium', 'bold', 'heavy', 'black'].map((w) => line([
        nameTag(`--fw-${w}`),
        el('span', { style: { fontFamily: 'var(--font-ui)', fontSize: 'var(--fs-title-sm)', fontWeight: `var(--fw-${w})`, color: 'var(--text-body)' }, children: 'Mulish — Translate the Bible' }),
      ])),
      ...[['400', 'normal'], ['700', 'normal'], ['400', 'italic'], ['700', 'italic']].map(([w, s]) => line([
        nameTag(`Charis SIL ${w} ${s}`),
        el('span', { style: { fontFamily: 'var(--font-scripture)', fontSize: 'var(--fs-verse)', fontWeight: w, fontStyle: s, color: 'var(--text-scripture)' }, children: 'Paul, a servant of God' }),
      ])),
    ] }, { frame: { width: 640, height: 340 } }],
    ['type-roles', { style: { display: 'flex', flexDirection: 'column', gap: 10 }, children: ROLES.map((role) => line([
      nameTag(role),
      el('Text', { role, style: { minWidth: 0 }, truncate: true, children: /^scripture|quote/.test(role) ? 'Paul, a servant of God' : 'Your projects' }),
    ])) }, { frame: { width: 640, height: 820 }, note: 'Every Text role, one line each (the Text cases show each role alone).' }],
    ['font-families', { style: { display: 'flex', flexDirection: 'column', gap: 12 }, children: [
      ['--font-ui', 'Translate the Bible · مشاريعك'],
      ['--font-scripture', 'Paul, a servant of God · Павел, раб Божий · Phao-lô'],
      ['--font-greek', T.el1.slice(0, 38)],
      ['--font-hebrew', T.he1],
      ['--font-arabic', T.ar1.slice(0, 40)],
      ['--font-nastaliq', T.urdu.slice(0, 26)],
      ['--font-mono', 'en_tn v86 · 2026-10-14'],
    ].map(([name, sample]) => line([
      nameTag(name),
      el('span', { style: { fontFamily: `var(${name})`, fontSize: 'var(--fs-verse)', lineHeight: name === '--font-nastaliq' ? 'var(--lh-nastaliq)' : 1.4, color: 'var(--text-scripture)', whiteSpace: 'nowrap' }, children: sample }),
    ])) }, { frame: { width: 760, height: 420 }, note: '--font-mono names no bundled face: it renders in the machine’s monospace. Awami Nastaliq needs Graphite, which Chromium lacks.' }],
    ['spacing', { style: { display: 'flex', flexDirection: 'column', gap: 8 }, children: spacing.map((t) => line([
      nameTag(`${t.name} ${t.value}`),
      el('div', { style: { width: `var(${t.name})`, height: 14, background: 'var(--uw-inspire)', borderRadius: 2 } }),
    ], 16)) }, { frame: { width: 360, height: 16 * 2 + spacing.length * 19 + (spacing.length - 1) * 8 } }],
    ['radius', { children: wrapRow(radius.map((t) => el('div', { style: { width: 96, display: 'flex', flexDirection: 'column', gap: 4 }, children: [
      el('div', { style: { height: 64, background: 'var(--surface-muted)', border: '1px solid var(--border-input)', borderRadius: `var(${t.name})` } }),
      el('Text', { role: 'captionStrong', as: 'div', children: t.name }),
      el('Text', { role: 'caption', as: 'div', children: t.value }),
    ] })), 16) }, { frame: gridFrame(radius.length, 5, 96, 112, 16) }],
    ['stroke', { children: wrapRow([
      ...strokes.map((t) => ({ name: t.name, value: t.value, border: `var(${t.name}) solid var(--uw-ocean)` })),
      { name: 'dashed', value: '--stroke-selected dashed --border-dashed', border: 'var(--stroke-selected) dashed var(--border-dashed)' },
      { name: 'dashed accent', value: '--border-dashed-accent', border: 'var(--stroke-selected) dashed var(--border-dashed-accent)' },
      { name: 'danger', value: '--border-danger', border: 'var(--stroke) solid var(--border-danger)' },
    ].map((s) => el('div', { style: { width: 150, display: 'flex', flexDirection: 'column', gap: 4 }, children: [
      el('div', { style: { height: 48, background: '#fff', border: s.border, borderRadius: 8 } }),
      el('Text', { role: 'captionStrong', as: 'div', children: s.name }),
      el('Text', { role: 'caption', as: 'div', style: { overflowWrap: 'anywhere' }, children: s.value }),
    ] })), 16) }, { frame: gridFrame(7, 4, 150, 120, 16) }],
    ['elevation', { children: wrapRow(shadows.map((t) => el('div', { style: { width: 140, display: 'flex', flexDirection: 'column', gap: 8 }, children: [
      el('div', { style: { height: 72, background: '#fff', borderRadius: 12, boxShadow: `var(${t.name})` } }),
      el('Text', { role: 'captionStrong', as: 'div', children: t.name }),
    ] })), 40) }, { frame: { ...gridFrame(shadows.length, 5, 140, 100, 40, 40), padding: 40 } }],
    ['control-heights', { children: wrapRow(controls.map((t) => el('div', { style: { display: 'flex', flexDirection: 'column', gap: 4 }, children: [
      el('div', { style: { width: 120, height: `var(${t.name})`, background: 'var(--surface-accent-soft)', border: '1px solid var(--uw-inspire)', borderRadius: 'var(--radius-pill)' } }),
      el('Text', { role: 'captionStrong', as: 'div', children: `${t.name} ${t.value}` }),
    ] })), 24) }, { frame: { width: 32 + 3 * 160 + 2 * 24, height: 112 } }],
  ]);
}
