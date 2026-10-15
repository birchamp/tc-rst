// components/core: Badge, Button, Divider, IconButton, Overline, ProgressBar, StatusDot.
import { el, noop, group, onDark, row, focus, hover, press, AR, T } from './lib.mjs';

const BADGE_TONES = ['accent', 'accentSoft', 'cultivate', 'ocean', 'neutral', 'valid', 'warn'];
const BUTTON_VARIANTS = ['primary', 'ocean', 'secondary', 'outline', 'ghost', 'valid', 'danger'];
const kebab = (s) => s.replace(/[A-Z]/g, (c) => '-' + c.toLowerCase());

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
