// components/core: Badge, Button, Divider, IconButton, Overline, ProgressBar, StatusDot.
import { el, noop, cell, setTo, when, prevent, group, onDark, row, focus, hover, press, click, AR, T } from './lib.mjs';

const BADGE_TONES = ['accent', 'accentSoft', 'cultivate', 'ocean', 'neutral', 'valid', 'warn'];
const BUTTON_VARIANTS = ['primary', 'ocean', 'secondary', 'outline', 'ghost', 'valid', 'danger'];
const kebab = (s) => s.replace(/[A-Z]/g, (c) => '-' + c.toLowerCase());

/* Props tC4's new screens change while the button is on screen. Home's sync button
   (src/views/Home.jsx:65-66, #690): its title follows its label and it is busy while a sync
   runs. ItemNavButton (src/views/ItemNav.jsx:16-19, #745): the accent colour only while
   enabled. The Draft section pill (src/views/Draft.jsx:197-200, #698, #728): off, with a press
   guard, while an editing card is open. */
const SYNC = 'Sync with Door43';
const SYNCING = 'Syncing…';
const nextPill = (props) => ({ variant: 'secondary', size: 'sm', title: 'Next item', children: 'Next →', ...props });
const sectionPill = (props) => ({ variant: 'secondary', size: 'sm', children: 'Draft section 1–3', ...props });
const verseField = el('Input', { defaultValue: 'In the beginning God created', 'aria-label': 'Verse 1' });

export default [
  ...group('Badge', { frame: { width: 200, height: 56 } }, [
    ...BADGE_TONES.map((tone) => [`${kebab(tone)}-md`, { tone, children: 'Recommended' }]),
    ...BADGE_TONES.map((tone) => [`${kebab(tone)}-sm`, { tone, size: 'sm', children: 'Recommended' }]),
    ['long-text', { tone: 'neutral', children: 'Always included in every export' }, { frame: { width: 200, height: 72 } }],
    ['rtl', { tone: 'accentSoft', children: 'موصى به' }, { document: AR }],
  ]),

  ...group('Button', { frame: { width: 240, height: 80 } }, [
    ...BUTTON_VARIANTS.flatMap((variant) => ['sm', 'md', 'lg'].map((size) =>
      [`${variant}-${size}`, { variant, size, children: T.save, onClick: noop }])),
    ...BUTTON_VARIANTS.map((variant) => [`${variant}-disabled`, { variant, disabled: true, children: T.save }]),
    ...BUTTON_VARIANTS.map((variant) =>
      [`${variant}-hover`, { variant, children: T.save, onClick: noop }, { steps: [hover('button')] }]),
    ['primary-pressed', { variant: 'primary', children: T.save, onClick: noop }, { steps: [press('button')] }],
    ['primary-focus-visible', { variant: 'primary', children: T.save, onClick: noop }, { steps: [focus('button')] }],
    ['secondary-focus-visible', { variant: 'secondary', children: T.cancel, onClick: noop }, { steps: [focus('button')] }],
    ['block', { variant: 'primary', shape: 'block', children: 'Create Bible →' }, { frame: { width: 360 } }],
    ['glyph-label', { variant: 'primary', children: '+ Add a project' }],
    ['long-text', { variant: 'primary', children: T.long }, { frame: { width: 420 } }],
    ['rtl', { variant: 'primary', children: T.saveAr }, { document: AR }],
    ['rtl-pair', { variant: 'secondary', children: T.cancelAr }, {
      document: AR, frame: { width: 280 },
      wrap: (node) => row([node, el('Button', { variant: 'primary', children: T.saveAr })], 10),
    }],
    ['sync-busy', { variant: 'outline', size: 'sm', 'data-testid': 'share-p1', title: SYNCING, 'aria-busy': true, disabled: true, children: SYNCING }],
    ['sync-press', { variant: 'outline', size: 'sm', 'data-testid': 'share-p1', title: when('busy', SYNCING, SYNC), 'aria-busy': when('busy', true),
      disabled: cell('busy'), onClick: setTo('busy', true), children: when('busy', SYNCING, SYNC) },
      { cells: { busy: false }, steps: [click('button')], note: 'The press starts a sync: the same button takes the busy title, label and aria-busy.' }],
    ['nav-next', nextPill({ onClick: noop, style: { marginInlineStart: 4, color: 'var(--accent)' } })],
    ['nav-prev-rtl', nextPill({ title: 'العنصر السابق', onClick: noop, style: { marginInlineEnd: 4, color: 'var(--accent)' }, children: '→ السابق' }), { document: AR }],
    ['nav-next-hover', nextPill({ onClick: noop, style: { marginInlineStart: 4, color: 'var(--accent)' } }), { steps: [hover('button')] }],
    ['nav-next-end', nextPill({ disabled: true, onClick: noop, style: { marginInlineStart: 4 } })],
    ['nav-to-end', nextPill({ disabled: cell('end'), onClick: setTo('end', true), style: when('end', { marginInlineStart: 4 }, { marginInlineStart: 4, color: 'var(--accent)' }) }),
      { cells: { end: false }, steps: [click('button')], note: 'The press reaches the last item: the same button turns off and drops the accent colour.' }],
    ['draft-section', sectionPill({ onClick: noop, style: { color: 'var(--accent)' } })],
    ['draft-section-hover', sectionPill({ onClick: noop, style: { color: 'var(--accent)' } }), { steps: [hover('button')] }],
    ['draft-section-focus-visible', sectionPill({ onClick: noop, style: { color: 'var(--accent)' } }), { steps: [focus('button')] }],
    ['draft-section-off', sectionPill({ onClick: noop, disabled: true, onPointerDownCapture: prevent }), {
      frame: { width: 280, height: 112 },
      wrap: (node) => el('div', { style: { display: 'flex', flexDirection: 'column', alignItems: 'flex-start', gap: 10 }, children: [verseField, node] }),
      note: 'Off while an editing card is open; a press on it keeps the focus in the card (scenarios.mjs, button-press-guard).',
    }],
  ]),

  ...group('Divider', { frame: { width: 320, height: 52 } }, [
    ['horizontal', {}],
    ['labelled', { label: 'Or' }],
    ['vertical', { orientation: 'vertical' }, {
      frame: { width: 220, height: 64 },
      wrap: (node) => row([el('Text', { role: 'ui', children: 'Titus' }), node, el('Text', { role: 'ui', children: 'Jonah' })]),
    }],
    ['on-dark', { label: 'Or' }, { frame: { height: 80 }, wrap: (node) => onDark(node) }],
    ['rtl-labelled', { label: 'أو' }, { document: AR }],
  ]),

  ...group('IconButton', { frame: { width: 80, height: 72 } }, [
    ...['outline', 'muted', 'plain'].flatMap((variant) => [
      [`${variant}-md`, { variant, title: 'Close', children: '✕', onClick: noop }],
      [`${variant}-28`, { variant, size: 28, title: 'Close', children: '✕', onClick: noop }],
      [`${variant}-hover`, { variant, title: 'Close', children: '✕', onClick: noop }, { steps: [hover('button')] }],
    ]),
    ['outline-20', { variant: 'outline', size: 20, title: 'Close', children: '✕' }],
    ['outline-disabled', { variant: 'outline', title: 'Close', disabled: true, children: '✕' }],
    ['outline-focus-visible', { variant: 'outline', title: 'Close', children: '✕', onClick: noop }, { steps: [focus('button')] }],
    ['glyph-menu', { variant: 'muted', title: 'Toggle book list', children: '☰' }],
  ]),

  ...group('Overline', { frame: { width: 260, height: 48 } }, [
    ['muted', { children: 'Bible name' }],
    ['accent', { tone: 'accent', children: 'Translation Academy' }],
    ['inverse-on-dark', { tone: 'inverse', children: 'Bible name' }, { frame: { height: 80 }, wrap: (node) => onDark(node) }],
    ['as-label', { as: 'label', children: 'Checking language' }],
    ['long-text', { children: 'Checking language for this project and its books' }, { frame: { width: 200, height: 84 } }],
    ['rtl', { children: 'اسم الكتاب المقدس' }, { document: AR }],
  ]),

  ...group('ProgressBar', { frame: { width: 280, height: 40 } }, [
    ['value-0', { value: 0 }],
    ['value-35', { value: 35 }],
    ['value-100', { value: 100 }],
    ['over-100-clamped', { value: 140 }],
    ...['valid', 'warn', 'invalid', 'ocean', 'cultivate', 'neutral'].map((tone) => [`tone-${tone}`, { value: 60, tone }]),
    ['height-3', { value: 60, height: 3 }],
    ['height-8', { value: 60, height: 8 }],
    ['rtl', { value: 35 }, { document: AR }],
  ]),

  ...group('StatusDot', { frame: { width: 64, height: 56 } }, [
    ...['valid', 'invalid', 'warn', 'accent', 'todo'].map((status) => [status, { status }]),
    ...[['valid', 'Saved'], ['invalid', 'Save failed — retry'], ['warn', 'Saving…'], ['accent', 'Syncing'], ['todo', 'Not started']]
      .map(([status, label]) => [`${status}-label`, { status, label }, { frame: { width: 220 } }]),
    ['size-12', { status: 'valid', size: 12 }],
    ['rtl-label', { status: 'valid', label: 'تم الحفظ' }, { document: AR, frame: { width: 220 } }],
  ]),
];
