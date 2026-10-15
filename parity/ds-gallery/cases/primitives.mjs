// components/primitives: Action, Choice, Field, FieldGroup, Input, Layer, Progress, Rule, Stack,
// Surface, Switcher, Table, Text.
import { el, noop, group, onDark, row, focus, hover, press, AR, T } from './lib.mjs';

const WEIGHTS = ['fill', 'soft', 'outline', 'quiet', 'text', 'row'];
const TONES = ['accent', 'ocean', 'cultivate', 'valid', 'warn', 'invalid', 'neutral'];
const label = (w) => (w === 'row' ? 'Sign in to Door43' : 'Mark valid');
const action = (props) => ({ onClick: noop, ...props });

const box = (w, h, bg = 'var(--uw-inspire-200)') => el('div', { style: { width: w, height: h, background: bg, borderRadius: 6 } });
const BOXES = [box(48, 32), box(32, 48, 'var(--uw-cultivate)'), box(64, 24, 'var(--uw-kindle)'), box(40, 40, 'var(--uw-ocean)')];

const panel = (title, body) => el('Surface', {
  fill: 'card', radius: '2xl', elevation: 'modal', pad: 'lg', style: { width: 320 },
  children: [el('Text', { role: 'h3', as: 'h3', children: title }), el('Text', { role: 'body', style: { marginTop: 8 }, children: body })],
});
const sidePanel = (title) => el('Surface', {
  fill: 'card', radius: 'none', elevation: 'drawer', pad: 'lg', style: { width: 260, height: '100%' },
  children: el('Text', { role: 'h3', children: title }),
});
const toastPanel = el('Surface', {
  fill: 'card', border: 'line', radius: 'lg', elevation: 'hover', pad: 'sm',
  children: el('Text', { role: 'ui', children: 'Exported Titus as USFM.' }),
});

const MODES = [{ value: 'read', label: 'Understand' }, { value: 'draft', label: 'Translate' }, { value: 'check', label: 'Check' }];
const HELPS = [{ value: 'notes', label: 'Notes', count: 12 }, { value: 'words', label: 'Words', count: 4 }, { value: 'questions', label: 'Questions', count: 3 }];

const COLUMNS = [
  { key: 'book', header: 'Book' }, { key: 'code', header: 'Code' },
  { key: 'verses', header: 'Verses', align: 'numeric' }, { key: 'drafted', header: 'Drafted', align: 'numeric' },
];
const ROWS = [
  { id: 'tit', book: 'Titus', code: 'TIT', verses: 46, drafted: '12%' },
  { id: 'jon', book: 'Jonah', code: 'JON', verses: 48, drafted: '4%' },
  { id: 'rut', book: 'Ruth', code: 'RUT', verses: 85, drafted: '0%' },
];

const ROLE_TEXT = {
  display: 'Your projects', h1: 'Your projects', h2: 'Project settings', h3: 'Add a project', title: 'Equipo Ejemplo',
  titleSm: 'No projects yet', strong: 'Translation Notes', body: T.note, ui: 'Show all 66 books',
  caption: 'Renaming arrives with a later increment.', captionStrong: 'Save failed — retry', overline: 'Bible name',
  label: 'Recommended', labelNum: '12% · 2:14', labelMicro: 'Always included', meta: 'en_tn v86 · 2026-10-14',
  scripture: T.en1, scriptureMd: T.en1, scriptureSm: T.en1, quote: 'servant of God',
};
const ROLE_WIDTH = { display: 520, h1: 460, body: 420, scripture: 560, scriptureMd: 520, scriptureSm: 460 };
const ROLE_HEIGHT = { display: 80, h1: 72, body: 120, scripture: 140, scriptureMd: 132, scriptureSm: 108 };

export default [
  ...group('Action', { frame: { width: 240, height: 80 } }, [
    ...WEIGHTS.map((weight) => [weight, action({ weight, children: label(weight) })]),
    ...TONES.map((tone) => [`fill-${tone}`, action({ weight: 'fill', tone, children: 'Mark valid' })]),
    ...['ocean', 'invalid', 'valid'].map((tone) => [`outline-${tone}`, action({ weight: 'outline', tone, children: 'Mark invalid' })]),
    ['row-invalid', action({ weight: 'row', tone: 'invalid', children: 'Remove project' })],
    ['soft-invalid', action({ weight: 'soft', tone: 'invalid', children: 'Remove' })],
    ['text-neutral', action({ weight: 'text', tone: 'neutral', children: 'Show all 50 stories →' })],
    ['fill-sm', action({ weight: 'fill', size: 'sm', children: 'Mark valid' })],
    ['fill-lg', action({ weight: 'fill', size: 'lg', children: 'Mark valid' })],
    ['text-sm', action({ weight: 'text', size: 'sm', children: 'Translation Academy →' })],
    ['text-lg', action({ weight: 'text', size: 'lg', children: 'Translation Academy →' })],
    ['shape-block', action({ weight: 'fill', shape: 'block', children: 'Create Bible →' }), { frame: { width: 360 } }],
    ['shape-square', action({ weight: 'quiet', tone: 'neutral', shape: 'square', title: 'Close', children: '✕' }), { frame: { width: 80 } }],
    ['icon-only-sm', action({ weight: 'soft', tone: 'neutral', iconOnly: true, size: 'sm', title: 'Close', children: '✕', style: { borderRadius: 'var(--radius-pill)' } }), { frame: { width: 80 } }],
    ...WEIGHTS.map((weight) => [`${weight}-disabled`, { weight, disabled: true, children: label(weight) }]),
    ['link-href', { weight: 'text', href: '#guide', children: 'Read the article →' }],
    ...WEIGHTS.map((weight) => [`${weight}-hover`, action({ weight, children: label(weight) }), { steps: [hover('button')] }]),
    ['fill-pressed', action({ weight: 'fill', children: 'Mark valid' }), { steps: [press('button')] }],
    ['quiet-pressed', action({ weight: 'quiet', children: 'Cancel' }), { steps: [press('button')] }],
    ['fill-focus-visible', action({ weight: 'fill', children: 'Mark valid' }), { steps: [focus('button')] }],
    ['text-focus-visible', action({ weight: 'text', children: 'Translation Academy →' }), { steps: [focus('button')] }],
    ...['fill', 'soft', 'outline', 'quiet', 'text'].map((weight) =>
      [`${weight}-on-dark`, action({ weight, tone: weight === 'fill' ? 'accent' : 'neutral', children: weight === 'text' ? 'Sign in' : 'Share' }), { frame: { height: 100 }, wrap: (n) => onDark(n) }]),
    ['outline-accent-on-dark', action({ weight: 'outline', tone: 'accent', children: 'Upload changes' }), { frame: { height: 100 }, wrap: (n) => onDark(n) }],
    ['text-invalid-on-dark', action({ weight: 'text', tone: 'invalid', children: 'Sign out' }), { frame: { height: 100 }, wrap: (n) => onDark(n) }],
    ['long-text', action({ weight: 'fill', children: T.long }), { frame: { width: 420 } }],
    ['rtl', action({ weight: 'fill', children: T.saveAr }), { document: AR }],
  ]),

  ...group('Choice', { frame: { width: 80, height: 64 } }, [
    ...['check', 'radio', 'switch'].flatMap((mark) => ['sm', 'md', 'lg'].flatMap((size) => [
      [`${mark}-${size}`, { mark, size }, { frame: { height: size === 'lg' ? 80 : 64 } }],
      [`${mark}-${size}-on`, { mark, size, checked: true }, { frame: { height: size === 'lg' ? 80 : 64 } }],
    ])),
    ['check-indeterminate', { mark: 'check', indeterminate: true }],
    ['check-on-valid', { mark: 'check', checked: true, tone: 'valid' }],
    ['check-on-invalid', { mark: 'check', checked: true, tone: 'invalid' }],
    ['switch-on-valid', { mark: 'switch', checked: true, tone: 'valid' }],
    ['check-disabled', { mark: 'check', disabled: true }],
    ['check-disabled-on', { mark: 'check', disabled: true, checked: true }],
    ['switch-disabled-on', { mark: 'switch', disabled: true, checked: true }],
    ['check-focus-visible', { mark: 'check', onChange: noop }, { steps: [focus('input')] }],
    ['radio-focus-visible', { mark: 'radio', onChange: noop }, { steps: [focus('input')] }],
    ['switch-focus-visible', { mark: 'switch', checked: true, onChange: noop }, { steps: [focus('input')] }],
    ['switch-on-rtl', { mark: 'switch', checked: true }, { document: AR }],
  ]),

  ...group('Field', { frame: { width: 340, height: 96 } }, [
    ['label', { label: 'Bible name', children: el('Input', { placeholder: 'e.g. Kanuri Bible' }) }],
    ['required', { label: 'Bible name', required: true, children: el('Input', { placeholder: 'e.g. Kanuri Bible' }) }],
    ['optional', { label: 'Description', optional: true, children: el('Input', {}) }],
    ['hint', { label: 'Code', hint: 'A BCP 47 tag, such as es-419.', children: el('Input', { defaultValue: 'es-419' }) }, { frame: { height: 120 } }],
    ['error', { label: 'Code', error: 'Enter a language code, such as es-419.', children: el('Input', { defaultValue: 'english' }) }, { frame: { height: 120 } }],
    ['hint-error', { label: 'Code', hint: 'A BCP 47 tag, such as es-419.', error: 'The server refused this code.', children: el('Input', { defaultValue: 'english' }) }, { frame: { height: 140 } }],
    ['disabled', { label: 'Code', disabled: true, children: el('Input', { defaultValue: 'es-419' }) }],
    ['size-sm', { label: 'Code', size: 'sm', children: el('Input', { defaultValue: 'es-419' }) }],
    ['placement-end', { label: 'Stay signed in', placement: 'end', children: el('Choice', { mark: 'check', checked: true }) }, { frame: { height: 64 } }],
    ['placement-end-hint', { label: 'Stay signed in', hint: 'Keeps a token on this computer.', placement: 'end', children: el('Choice', { mark: 'check' }) }, { frame: { height: 80 } }],
    ['placement-start', { label: 'Verse numbers', placement: 'start', children: el('Choice', { mark: 'switch', checked: true }) }, { frame: { height: 64 } }],
    ['placement-start-error', { label: 'Verse numbers', placement: 'start', error: 'Could not save this setting.', children: el('Choice', { mark: 'switch' }) }, { frame: { height: 96 } }],
    ['rtl', { label: 'الرمز', hint: 'رمز لغة، مثل ar.', error: 'أدخل رمز اللغة.', children: el('Input', { defaultValue: 'عربي' }) }, { document: AR, frame: { height: 140 } }],
  ]),

  ...group('FieldGroup', { frame: { width: 380, height: 130 } }, [
    ['radios', { legend: 'Text direction', role: 'radiogroup', name: 'dir', children: [
      el('Field', { label: 'Left to right', placement: 'end', children: el('Choice', { mark: 'radio', value: 'ltr', checked: true }) }),
      el('Field', { label: 'Right to left', placement: 'end', children: el('Choice', { mark: 'radio', value: 'rtl' }) }),
    ] }],
    ['row', { legend: 'Paper', role: 'radiogroup', name: 'paper', direction: 'row', gap: 20, children: [
      el('Field', { label: 'A4', placement: 'end', children: el('Choice', { mark: 'radio', value: 'a4', checked: true }) }),
      el('Field', { label: 'Letter', placement: 'end', children: el('Choice', { mark: 'radio', value: 'letter' }) }),
    ] }, { frame: { height: 96 } }],
    ['columns', { legend: 'Books', columns: 2, children: ['Ruth', 'Esther', 'Jonah', 'Titus'].map((b, i) =>
      el('Field', { label: b, placement: 'end', children: el('Choice', { mark: 'check', checked: i % 2 === 1 }) })) }],
    ['hint-required', { legend: 'Checking language', hint: 'Notes, words and questions come in this language.', required: true, role: 'radiogroup', name: 'gl', children: [
      el('Field', { label: 'English', placement: 'end', children: el('Choice', { mark: 'radio', value: 'en', checked: true }) }),
      el('Field', { label: 'Español (Latinoamérica)', placement: 'end', children: el('Choice', { mark: 'radio', value: 'es-419' }) }),
    ] }, { frame: { height: 160 } }],
    ['error', { legend: 'Licence', error: 'Choose a licence to share the project.', role: 'radiogroup', name: 'lic', children: [
      el('Field', { label: 'CC BY-SA 4.0', placement: 'end', children: el('Choice', { mark: 'radio', value: 'by-sa' }) }),
      el('Field', { label: 'CC0 1.0', placement: 'end', children: el('Choice', { mark: 'radio', value: 'cc0' }) }),
    ] }, { frame: { height: 160 } }],
    ['disabled', { legend: 'Text direction', disabled: true, role: 'radiogroup', name: 'dir2', children: [
      el('Field', { label: 'Left to right', placement: 'end', children: el('Choice', { mark: 'radio', value: 'ltr', checked: true }) }),
      el('Field', { label: 'Right to left', placement: 'end', children: el('Choice', { mark: 'radio', value: 'rtl' }) }),
    ] }],
    ['rtl', { legend: 'اتجاه النص', role: 'radiogroup', name: 'dir3', children: [
      el('Field', { label: 'من اليسار إلى اليمين', placement: 'end', children: el('Choice', { mark: 'radio', value: 'ltr' }) }),
      el('Field', { label: 'من اليمين إلى اليسار', placement: 'end', children: el('Choice', { mark: 'radio', value: 'rtl', checked: true }) }),
    ] }, { document: AR }],
  ]),

  ...group('Input', { frame: { width: 320, height: 68 } }, [
    ['placeholder', { placeholder: 'e.g. Kanuri Bible' }],
    ['value', { defaultValue: T.project }],
    ['leading', { placeholder: 'Search books', leading: el('svg', { width: 15, height: 15, viewBox: '0 0 16 16', fill: 'none', stroke: 'currentColor', strokeWidth: 1.5, strokeLinecap: 'round', children: [el('circle', { cx: 7, cy: 7, r: 4.5 }), el('path', { d: 'M10.5 10.5L14 14' })] }) }],
    ['trailing', { defaultValue: 'es-419', trailing: el('Text', { role: 'label', children: 'BCP 47' }) }],
    ['invalid', { defaultValue: 'english', invalid: true }],
    ['disabled', { defaultValue: 'es-419', disabled: true }],
    ['size-sm', { defaultValue: 'es-419', size: 'sm' }],
    ['size-lg', { defaultValue: 'es-419', size: 'lg' }, { frame: { height: 76 } }],
    ['textarea', { as: 'textarea', rows: 3, placeholder: 'Type a comment…' }, { frame: { height: 120 } }],
    ['scripture', { variant: 'scripture', defaultValue: 'Paul, a servant of God' }, { frame: { height: 68 } }],
    ['scripture-textarea', { as: 'textarea', variant: 'scripture', rows: 3, defaultValue: T.en1 }, { frame: { width: 420, height: 168 } }],
    ['focused', { placeholder: 'e.g. Kanuri Bible' }, { steps: [focus('input')] }],
    ['rtl', { defaultValue: 'الكتاب المقدس', leading: el('Text', { role: 'label', children: '#' }) }, { document: AR }],
  ]),

  ...group('Layer', { frame: { width: 640, height: 400 } }, [
    ['center-modal-scrim', { open: true, scrim: 'modal', placement: 'center', role: 'dialog', label: 'Layer', children: panel('Centred layer', 'A modal scrim behind a centred panel.') }],
    ['center-no-scrim', { open: true, placement: 'center', role: 'dialog', label: 'Layer', children: panel('Centred layer', 'No scrim: the page behind stays live.') }],
    ['end-drawer-scrim', { open: true, scrim: 'drawer', placement: 'end', role: 'dialog', label: 'Layer', children: sidePanel('End panel') }],
    ['start-drawer-scrim', { open: true, scrim: 'drawer', placement: 'start', role: 'dialog', label: 'Layer', children: sidePanel('Start panel') }],
    ['bottom-start', { open: true, level: 'toast', placement: 'bottom-start', children: toastPanel }],
    ['bottom-end', { open: true, level: 'toast', placement: 'bottom-end', children: toastPanel }],
    ['end-drawer-rtl', { open: true, scrim: 'drawer', placement: 'end', role: 'dialog', label: 'طبقة', children: sidePanel('لوحة جانبية') }, { document: AR }],
  ]),

  ...group('Progress', { frame: { width: 280, height: 40 } }, [
    ['bar-0', { value: 0 }],
    ['bar-50', { value: 50 }],
    ['bar-100', { value: 100 }],
    ['bar-indeterminate', {}, { note: 'Captured with animations off: the 35% bar sits at its start position.' }],
    ...['valid', 'warn', 'invalid'].map((tone) => [`bar-${tone}`, { value: 70, tone }]),
    ['bar-height-8', { value: 50, height: 8 }],
    ...[0, 25, 75, 100].map((v) => [`ring-${v}`, { shape: 'ring', value: v }, { frame: { width: 64, height: 60 } }]),
    ['ring-32', { shape: 'ring', value: 60, size: 32 }, { frame: { width: 72, height: 72 } }],
    ['ring-indeterminate', { shape: 'ring' }, { frame: { width: 64, height: 60 }, note: 'Captured with animations off: the arc sits at the top.' }],
    ['ring-label', { shape: 'ring', value: 40, label: 'Downloading resources…' }, { frame: { height: 60 } }],
    ['ring-indeterminate-label', { shape: 'ring', label: 'Opening the project…' }, { frame: { height: 60 } }],
    ['ring-valid', { shape: 'ring', value: 100, tone: 'valid' }, { frame: { width: 64, height: 60 } }],
    ['bar-rtl', { value: 50 }, { document: AR }],
  ]),

  ...group('Rule', { frame: { width: 320, height: 52 } }, [
    ['hair', {}],
    ['line', { weight: 'line' }],
    ['labelled', { label: 'Or' }],
    ['vertical', { orientation: 'vertical' }, { frame: { width: 220, height: 64 }, wrap: (n) => row([el('Text', { role: 'ui', children: 'Titus' }), n, el('Text', { role: 'ui', children: 'Jonah' })]) }],
    ['on-dark', { label: 'Or' }, { frame: { height: 80 }, wrap: (n) => onDark(n) }],
  ]),

  ...group('Stack', { frame: { width: 320, height: 120 } }, [
    ['row-gap-12', { direction: 'row', gap: 12, children: BOXES }, { frame: { height: 80 } }],
    ['column-gap-8', { gap: 8, children: BOXES }, { frame: { height: 220 } }],
    ['row-align-center', { direction: 'row', gap: 8, align: 'center', children: BOXES }, { frame: { height: 80 } }],
    ['row-justify-between', { direction: 'row', align: 'end', justify: 'between', children: BOXES }, { frame: { height: 80 } }],
    ['row-wrap', { direction: 'row', gap: 10, wrap: true, children: [...BOXES, ...BOXES] }, { frame: { width: 240, height: 196 } }],
    ['columns-3', { columns: 3, gap: 10, children: [...BOXES, ...BOXES].map((b) => ({ ...b, props: { ...b.props, style: { ...b.props.style, width: 'auto' } } })) }, { frame: { height: 196 } }],
    ['columns-template', { columns: '80px 1fr', gap: 10, children: [el('Text', { role: 'label', children: 'Code' }), el('Text', { role: 'ui', children: 'es-419' }), el('Text', { role: 'label', children: 'Books' }), el('Text', { role: 'ui', children: '2' })] }, { frame: { height: 80 } }],
    ['row-rtl', { direction: 'row', gap: 12, children: BOXES }, { document: AR, frame: { height: 80 } }],
  ]),

  ...group('Surface', { frame: { width: 220, height: 96 } }, [
    ...['none', 'solid', 'soft', 'card', 'app', 'muted', 'quiet', 'paper'].map((fill) =>
      [`fill-${fill}`, { fill, tone: 'accent', pad: 'md', children: el('Text', { role: 'ui', tone: fill === 'solid' ? 'inverse' : 'default', children: 'Titus' }) }]),
    ...['hair', 'line', 'input', 'strong', 'tone', 'choice', 'dashed', 'dashed-tone'].map((border) =>
      [`border-${border}`, { fill: 'card', border, tone: 'accent', pad: 'md', children: el('Text', { role: 'ui', children: 'Titus' }) }]),
    ['border-choice-selected', { fill: 'card', border: 'choice', tone: 'accent', selected: true, pad: 'md', children: el('Text', { role: 'ui', tone: 'tone', children: 'Titus' }) }],
    ...['none', 'xs', 'chip', 'sm', 'input', 'md', 'lg', 'xl', '2xl', 'pill'].map((radius) =>
      [`radius-${radius}`, { fill: 'muted', radius, pad: 'md', children: el('Text', { role: 'ui', children: 'Titus' }) }]),
    ...['chip', 'raised', 'card', 'hover', 'hero', 'page', 'modal', 'drawer', 'focus'].map((elevation) =>
      [`elevation-${elevation}`, { fill: 'card', elevation, pad: 'md', children: el('Text', { role: 'ui', children: 'Titus' }) }, { frame: { width: 300, height: 220, padding: 40 } }]),
    ...['ocean', 'valid', 'invalid', 'warn', 'cultivate', 'neutral'].map((tone) =>
      [`solid-${tone}`, { fill: 'solid', tone, pad: 'md', children: el('Text', { role: 'ui', style: { color: 'var(--tone-on-fill)' }, children: 'Titus' }) }]),
    ...['ocean', 'valid', 'invalid', 'warn'].map((tone) =>
      [`soft-${tone}`, { fill: 'soft', tone, border: 'tone', pad: 'md', children: el('Text', { role: 'caption', tone: 'tone', children: 'The verse changed after this check.' }) }, { frame: { width: 280 } }]),
    ['disabled', { as: 'button', fill: 'card', border: 'choice', tone: 'accent', interactive: 'choice', disabled: true, pad: 'md', onClick: noop, children: el('Text', { role: 'ui', children: 'Titus' }) }],
    ...['choice', 'card', 'row', 'quiet'].map((interactive) =>
      [`interactive-${interactive}-hover`, { as: 'button', fill: interactive === 'row' ? 'none' : 'card', border: interactive === 'row' ? 'none' : 'line', tone: 'accent', interactive, pad: 'md', onClick: noop, style: { font: 'inherit', cursor: 'pointer' }, children: el('Text', { role: 'ui', children: 'Titus' }) }, { steps: [hover('button')] }]),
    ['clickable-div-focus-visible', { fill: 'card', border: 'line', interactive: 'card', pad: 'md', onClick: noop, children: el('Text', { role: 'ui', children: 'Titus' }) }, { steps: [focus('[role="button"]')] }],
    ['concentric-inner', { fill: 'card', border: 'line', radius: '2xl', pad: 'xs', children: el('Surface', { fill: 'soft', tone: 'accent', radius: 'inner', pad: 'md', children: el('Text', { role: 'ui', tone: 'tone', children: 'radius="inner"' }) }) }, { frame: { width: 260, height: 112 } }],
    ['concentric-elliptical', { fill: 'card', border: 'line', radius: '2xl', pad: '6px 10px', children: el('Surface', { fill: 'soft', tone: 'accent', radius: 'inner', pad: 'md', children: el('Text', { role: 'ui', tone: 'tone', children: 'rx ≠ ry' }) }) }, { frame: { width: 260, height: 112 } }],
    ['on-dark-text', { fill: 'solid', tone: 'ocean', pad: 'md', children: el('Stack', { gap: 4, children: [el('Text', { role: 'ui', children: 'Titus' }), el('Text', { role: 'caption', children: '12% drafted' })] }) }, { frame: { height: 112 } }],
  ]),

  ...group('Switcher', { frame: { width: 420, height: 84 } }, [
    ['underline', { options: MODES, value: 'draft', onChange: noop }],
    ['pill', { options: MODES, value: 'draft', indicator: 'pill', onChange: noop }],
    ['none', { options: MODES, value: 'draft', indicator: 'none', onChange: noop }],
    ['counts', { options: HELPS, value: 'notes', onChange: noop }],
    ['disabled-option', { options: [...MODES.slice(0, 2), { ...MODES[2], disabled: true }], value: 'draft', onChange: noop }],
    ['no-selection', { options: MODES, value: null, onChange: noop }],
    ...['sm', 'lg'].flatMap((size) => [
      [`pill-${size}`, { options: MODES, value: 'draft', indicator: 'pill', size, onChange: noop }],
      [`underline-${size}`, { options: MODES, value: 'draft', size, onChange: noop }],
    ]),
    ['grow', { options: MODES, value: 'draft', grow: true, onChange: noop }],
    ['tone-ocean', { options: MODES, value: 'draft', indicator: 'pill', tone: 'ocean', onChange: noop }],
    ['tab-hover', { options: MODES, value: 'draft', onChange: noop }, { steps: [hover('[role="tab"][aria-selected="false"]')] }],
    ['pill-hover', { options: MODES, value: 'draft', indicator: 'pill', onChange: noop }, { steps: [hover('[role="tab"][aria-selected="false"]')] }],
    ['focus-visible', { options: MODES, value: 'draft', onChange: noop }, { steps: [focus('[role="tab"][aria-selected="true"]')] }],
    ['rtl', { options: [{ value: 'read', label: 'فهم' }, { value: 'draft', label: 'ترجمة' }, { value: 'check', label: 'مراجعة' }], value: 'read', onChange: noop }, { document: AR }],
    ['pill-rtl', { options: [{ value: 'read', label: 'فهم' }, { value: 'draft', label: 'ترجمة' }, { value: 'check', label: 'مراجعة' }], value: 'read', indicator: 'pill', onChange: noop }, { document: AR }],
  ]),

  ...group('Table', { frame: { width: 520, height: 200 } }, [
    ['default', { columns: COLUMNS, rows: ROWS, caption: 'Books in this project' }],
    ['caption-visible', { columns: COLUMNS, rows: ROWS, caption: 'Books in this project', captionVisible: true }, { frame: { height: 232 } }],
    ['selected-row', { columns: COLUMNS, rows: ROWS, selectedKey: 'jon' }],
    ['row-hover', { columns: COLUMNS, rows: ROWS, onRowClick: noop }, { steps: [hover('tbody tr:nth-child(2)')] }],
    ['row-focus-visible', { columns: COLUMNS, rows: ROWS, onRowClick: noop }, { steps: [focus('tbody tr')] }],
    ['empty', { columns: COLUMNS, rows: [], empty: el('Text', { role: 'caption', children: 'No books yet.' }) }, { frame: { height: 140 } }],
    ['wrap-column', { columns: [{ key: 'ref', header: 'Ref', width: 60 }, { key: 'note', header: 'Note', wrap: true }], rows: [{ id: 1, ref: '1:1', note: T.note }, { id: 2, ref: '1:2', note: 'Eternal life is life that never ends.' }] }, { frame: { height: 220 } }],
    ['rtl', { columns: [{ key: 'book', header: 'السفر' }, { key: 'verses', header: 'الآيات', align: 'numeric' }], rows: [{ id: 'tit', book: 'تيطس', verses: 46 }, { id: 'jon', book: 'يونان', verses: 48 }], selectedKey: 'tit' }, { document: AR, frame: { height: 160 } }],
  ]),

  ...group('Text', { frame: { width: 360, height: 60 } }, [
    ...Object.entries(ROLE_TEXT).map(([role, children]) => [`role-${role.replace(/[A-Z]/g, (c) => '-' + c.toLowerCase())}`, { role, children }, {
      frame: { width: ROLE_WIDTH[role] ?? 360, height: ROLE_HEIGHT[role] ?? 60 },
    }]),
    ...['strong', 'muted', 'faint', 'accent', 'scripture'].map((tone) => [`tone-${tone}`, { role: 'ui', tone, children: 'Show all 66 books' }]),
    ['tone-tone-valid', { role: 'captionStrong', tone: 'tone', 'data-tone': 'valid', children: 'Ready' }],
    ['tone-inverse-on-dark', { role: 'ui', tone: 'inverse', children: 'Show all 66 books' }, { frame: { height: 84 }, wrap: (n) => onDark(n) }],
    ['muted-on-dark', { role: 'caption', children: 'es-419 · 2 books' }, { frame: { height: 84 }, wrap: (n) => onDark(n) }],
    ['script-greek', { role: 'scripture', script: 'greek', children: T.el1 }, { frame: { width: 560, height: 140 } }],
    ['script-hebrew', { role: 'scripture', script: 'hebrew', children: T.he1 }, { frame: { width: 560, height: 80 } }],
    ['script-arabic', { role: 'scripture', script: 'arabic', children: T.ar1 }, { frame: { width: 560, height: 140 } }],
    ['script-nastaliq', { role: 'scripture', script: 'nastaliq', children: T.urdu }, { frame: { width: 560, height: 168 }, note: 'Awami Nastaliq is a Graphite font. Chromium has no Graphite, so it draws the letters unjoined; tC4 ships in Electronite, which joins them.' }],
    ['truncate', { role: 'ui', truncate: true, children: 'Biblia del Equipo Ejemplo — Tito, Jonás, Rut y Ester', style: { display: 'block' } }, { frame: { width: 220 } }],
    ['measure-narrow', { role: 'caption', measure: 'narrow', children: 'The pinned Translation Notes do not cover this book. Choose another book, or get a newer version of the notes when one covers it.' }, { frame: { width: 520, height: 88 } }],
    ['align-center', { role: 'caption', align: 'center', as: 'p', children: 'Select + Add a project to start.' }],
    ['h2-untrimmed', { role: 'h2', trim: false, children: 'Project settings' }],
    ['as-h1', { role: 'h1', as: 'h1', children: 'Your projects' }, { frame: { width: 460, height: 72 } }],
    ['rtl-body', { role: 'body', children: T.noteAr }, { document: AR, frame: { width: 420, height: 120 } }],
    ['rtl-overline', { role: 'overline', children: 'اسم الكتاب المقدس' }, { document: AR }],
    ['rtl-h2', { role: 'h2', children: 'إعدادات المشروع' }, { document: AR }],
  ]),
];
