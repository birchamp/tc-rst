// components/surfaces and components/translation: BookTile, Callout, Card, Drawer, HelpCard,
// Modal, ScripturePassage, VerseMarker, WordChip.
import { el, noop, cell, set, setTo, when, prevent, group, focus, hover, click, AR, T } from './lib.mjs';

const conflicts = el('Text', { role: 'labelNum', tone: 'tone', 'data-tone': 'warn', as: 'div', style: { marginTop: 7 }, children: '2 conflicts' });
const cardBody = [
  el('Text', { role: 'title', as: 'div', children: 'Equipo Ejemplo' }),
  el('Text', { role: 'caption', style: { marginTop: 8 }, children: 'es-419 · 2 books · 1 in progress' }),
];
const drawerBody = [
  el('Text', { role: 'body', children: 'An abstract noun refers to an attitude, quality, event or situation. These are things that cannot be seen or touched in a physical sense, such as happiness, weight, unity, friendship, health and reason.' }),
  el('Text', { role: 'body', children: 'Some languages may express an idea with an abstract noun, and others would need a different way to express it.' }),
];
const modalBody = [
  el('TextField', { label: 'Bible name', defaultValue: T.project }),
  el('Callout', { tone: 'info', children: 'Renaming arrives with a later increment.' }),
];
const modalFooter = [
  el('Button', { variant: 'secondary', children: T.cancel }),
  el('Button', { variant: 'primary', children: T.save }),
];
const modal = { title: 'Project settings', subtitle: `${T.project} · 2 books`, closeLabel: 'Close', onClose: noop, children: modalBody, footer: modalFooter };
const verses = [{ n: 1, text: T.en1 }, { n: 2, text: T.en2 }];
/* #698: tC4's book rail turns every tile off while an editing card is open
   (src/views/BookRail.jsx:19, 28-31). */
const railRow = { name: 'Titus', percent: 45, meta: '45%', metaTitle: '45% of this book\'s verses are drafted', layout: 'row', onClick: noop };
const railOff = { disabled: true, onPointerDownCapture: prevent, style: { cursor: 'default' } };
const ROW = { width: 280, height: 88 };
/* Open from Door43 (src/views/modals/OpenFromDoor43.jsx:71-78, #692): no ✕, Escape or scrim
   while the open runs. */
const openDoor43 = {
  title: 'Open a project from Door43', subtitle: 'Get a full copy of a project your team shares on Door43.', width: 520,
  closeLabel: 'Close', onClose: when('busy', undefined, noop), 'data-testid': 'open-door43-modal',
  children: [
    el('Callout', { tone: 'info', children: 'Paste the address of a project on git.door43.org.' }),
    el('TextField', { label: 'Door43 repository', defaultValue: 'https://git.door43.org/equipo/es-419_tit' }),
  ],
  footer: [
    el('Button', { variant: 'secondary', disabled: cell('busy'), onClick: noop, 'data-testid': 'open-door43-cancel', children: 'Cancel' }),
    el('Button', { disabled: cell('busy'), 'aria-busy': when('busy', true), onClick: setTo('busy', true), 'data-testid': 'open-door43-submit', children: when('busy', 'Opening…', 'Open project') }),
  ],
};

export default [
  ...group('BookTile', { frame: { width: 200, height: 92 } }, [
    ['tile-0', { name: 'Ruth', percent: 0 }],
    ['tile-45', { name: 'Titus', percent: 45 }],
    ['tile-100', { name: 'Jonah', percent: 100 }],
    ['tile-active', { name: 'Titus', percent: 45, active: true }],
    ['tile-meta', { name: 'Titus', meta: '—', metaTitle: 'Progress unknown' }],
    ['tile-children', { name: 'Titus', percent: 45, children: conflicts }, { frame: { height: 112 } }],
    ['tile-hover', { name: 'Titus', percent: 45, onClick: noop }, { steps: [hover('button')] }],
    ['tile-focus-visible', { name: 'Titus', percent: 45, onClick: noop }, { steps: [focus('button')] }],
    ['tile-long-name', { name: '1 Thessalonians and the Pastoral Epistles', percent: 8, style: { width: '100%' } }],
    ['row-45', { name: 'Titus', percent: 45, layout: 'row' }, { frame: { width: 280, height: 88 } }],
    ['row-active', { name: 'Titus', percent: 45, layout: 'row', active: true }, { frame: { width: 280, height: 88 } }],
    ['row-hover', { name: 'Titus', percent: 45, layout: 'row', onClick: noop }, { frame: { width: 280, height: 88 }, steps: [hover('button')] }],
    ['tile-rtl', { name: 'تيطس', percent: 45 }, { document: AR }],
    ['tile-disabled', { name: 'Titus', percent: 45, onClick: noop, disabled: true }],
    ['row-disabled', { ...railRow, ...railOff }, { frame: ROW }],
    ['row-active-disabled', { ...railRow, active: true, ...railOff }, { frame: ROW }],
    ['row-disabled-hover', { ...railRow, ...railOff }, { frame: ROW, steps: [hover('button')] }],
    ['row-editing', { ...railRow, disabled: cell('editing'), onPointerDownCapture: when('editing', prevent), style: when('editing', { cursor: 'default' }) }, {
      frame: { width: 280, height: 176 }, cells: { editing: false },
      wrap: (node) => el('div', { style: { display: 'flex', flexDirection: 'column', alignItems: 'flex-start', gap: 10 }, children: [
        node,
        el('Input', { defaultValue: 'In the beginning God created', 'aria-label': 'Verse 1' }),
        el('Button', { variant: 'secondary', size: 'sm', 'data-testid': 'edit', onClick: setTo('editing', true), children: 'Edit this verse' })] }),
      steps: [click('[data-testid="edit"]')],
      note: 'Opening an editing card turns the tile off: the same button takes disabled, the press guard and the default cursor.',
    }],
  ]),

  ...group('Callout', { frame: { width: 420, height: 104 } }, [
    ...['info', 'warn', 'kindle', 'success'].map((tone) => [tone, { tone, children: 'The pinned Translation Notes are older than the ones on this computer.' }]),
    ['flex-action', { tone: 'warn', style: { display: 'flex', alignItems: 'center', justifyContent: 'space-between', gap: 12 }, children: [
      el('span', { children: 'Source texts are not on this computer.' }),
      el('Button', { variant: 'secondary', size: 'sm', children: 'Get them' }),
    ] }],
    ['long-text', { tone: 'info', children: 'This project checks in Español (Latinoamérica) · es-419_gl. Notes, words and questions come from that package; when it has no Bible of its own, the English ULT and UST stand in for it, and the panes say so.' }, { frame: { height: 148 } }],
    ['rtl', { tone: 'warn', children: 'ملاحظات الترجمة المثبّتة أقدم من الموجودة على هذا الكمبيوتر.' }, { document: AR }],
  ]),

  ...group('Card', { frame: { width: 360, height: 150 } }, [
    ...['default', 'flat', 'muted', 'paper'].map((variant) => [variant, { variant, children: cardBody }]),
    ['ocean', { variant: 'ocean', children: [
      el('Text', { role: 'title', as: 'div', children: 'Continue where you left off' }),
      el('Text', { role: 'caption', style: { marginTop: 8 }, children: 'Titus 2:1 · Translate' }),
    ] }],
    ['padding-sm', { padding: 'sm', children: cardBody }, { frame: { height: 120 } }],
    ['interactive-hover', { interactive: true, onClick: noop, children: cardBody }, { steps: [hover('[role="button"]')] }],
    ['interactive-focus-visible', { interactive: true, onClick: noop, children: cardBody }, { steps: [focus('[role="button"]')] }],
    ['rtl', { children: [el('Text', { role: 'title', as: 'div', children: T.projectAr }), el('Text', { role: 'caption', style: { marginTop: 8 }, children: 'ar · سفران' })] }, { document: AR }],
  ]),

  ...group('Drawer', { frame: { width: 720, height: 480 } }, [
    ['default', { eyebrow: 'Translation Academy', title: 'Abstract Nouns', onClose: noop, closeLabel: 'Close', children: drawerBody }],
    ['no-eyebrow', { title: 'Abstract Nouns', onClose: noop, children: drawerBody }],
    ['no-close', { eyebrow: 'Translation Academy', title: 'Abstract Nouns', children: drawerBody }],
    ['width-360', { eyebrow: 'Translation Academy', title: 'Abstract Nouns', width: 360, onClose: noop, children: drawerBody }],
    ['long-content', { eyebrow: 'Translation Academy', title: 'Abstract Nouns', onClose: noop, children: [...drawerBody, ...drawerBody, ...drawerBody] }],
    ['title-rtl', { eyebrow: 'Translation Academy', title: 'الأسماء المجردة', titleDir: 'rtl', onClose: noop, children: drawerBody }, { note: 'A content title in a gateway language written right to left, under an English interface (#650).' }],
    ['rtl', { eyebrow: 'أكاديمية الترجمة', title: 'الأسماء المجردة', onClose: noop, closeLabel: 'إغلاق', children: [el('Text', { role: 'body', children: T.noteAr })] }, { document: AR }],
    ['element-title', { eyebrow: 'Translation Academy', title: el('span', { dir: 'rtl', children: 'الأسماء المجردة' }), onClose: noop, children: drawerBody },
      { note: 'A title that is an element: the dialog is labelled "Panel".' }],
  ]),

  ...group('HelpCard', { frame: { width: 380, height: 208 } }, [
    ['note', { verse: 1, title: 'servant of God', body: T.note, onAction: noop }],
    ['word', { kind: 'word', title: 'apostle', body: 'An apostle is a man sent by God to tell people about God and his kingdom.', onAction: noop, actionLabel: 'Read the article →' }],
    ['note-active', { verse: 1, title: 'servant of God', body: T.note, active: true, onAction: noop }],
    ['word-active', { kind: 'word', title: 'apostle', body: 'An apostle is a man sent by God to tell people about God and his kingdom.', active: true }, { frame: { height: 160 } }],
    ['no-action', { verse: 2, title: 'eternal life', body: 'Eternal life is life that never ends.' }, { frame: { height: 140 } }],
    ['chapter-intro', { body: 'Titus 1 general notes: Paul begins the letter with a formal greeting.' }, { frame: { height: 128 } }],
    ['hover', { verse: 1, title: 'servant of God', body: T.note, onClick: noop }, { frame: { height: 180 }, steps: [hover('[role="button"]')] }],
    ['focus-visible', { verse: 1, title: 'servant of God', body: T.note, onClick: noop }, { frame: { height: 180 }, steps: [focus('[role="button"]')] }],
    ['long-body', { verse: 1, title: 'for the faith of the chosen people of God and the knowledge of the truth', body: `${T.note} ${T.note}`, onAction: noop }, { frame: { height: 272 } }],
    ['helps-rtl', { verse: 1, title: 'عبد الله', body: T.noteAr, dir: 'rtl', onAction: noop, actionLabel: 'أكاديمية الترجمة ←' }, { note: 'Helps follow their gateway package, not the interface: tC4 passes dir to the card (#650).' }],
  ]),

  ...group('Modal', { frame: { width: 720, height: 520 } }, [
    ['default', modal],
    ['no-subtitle', { ...modal, subtitle: undefined }],
    ['no-close', { ...modal, onClose: undefined }],
    ['no-footer', { ...modal, footer: undefined }],
    ['width-400', { ...modal, width: 400 }],
    ['long-content', { ...modal, children: [...modalBody, ...drawerBody, ...drawerBody, ...drawerBody] }, { note: 'The panel stops at 82vh and scrolls.' }],
    ['rtl', { title: 'إعدادات المشروع', subtitle: `${T.projectAr} · سفران`, closeLabel: 'إغلاق', onClose: noop,
      children: [el('TextField', { label: 'اسم الكتاب المقدس', defaultValue: T.projectAr }), el('Callout', { tone: 'info', children: 'ستتوفر إعادة التسمية في تحديث لاحق.' })],
      footer: [el('Button', { variant: 'secondary', children: T.cancelAr }), el('Button', { variant: 'primary', children: T.saveAr })] }, { document: AR }],
    ['element-subtitle', { ...modal, title: 'Add a book', subtitle: ['To ', el('strong', { style: { color: 'var(--uw-ocean)' }, children: T.project })] },
      { note: 'A subtitle of text and an element, as tC4\'s AddBook passes it.' }],
    ['nested-select', { ...modal, open: cell('open'), onClose: setTo('open', false), children: [
      el('Select', { label: 'Script font', options: ['Noto Sans (default)', 'Charis SIL', 'Scheherazade New'], value: cell('font'), onChange: set('font', '0.target.value') }),
      el('Callout', { tone: 'info', children: 'The font applies to every book of the project.' }),
    ] }, { cells: { open: true, font: 'Charis SIL' }, steps: [click('[role="combobox"]')], note: 'A Select open inside a dialog: its list is a layer one level deeper.' }],
    ['nested-menu', { ...modal, open: cell('open'), onClose: setTo('open', false), children: [
      el('Menu', { trigger: el('Button', { variant: 'secondary', size: 'sm', 'data-testid': 'export', children: 'Export ▾' }), items: [{ label: 'USFM', meta: '.usfm', onClick: noop }, { label: 'Scripture Burrito', meta: '.zip', onClick: noop }] }),
    ] }, { cells: { open: true }, steps: [click('[data-testid="export"]')], note: 'A Menu open inside a dialog: its layer sits one level deeper.' }],
    ['busy', openDoor43, { cells: { busy: true } }],
    ['busy-later', { ...openDoor43, open: cell('open'), onClose: when('busy', undefined, setTo('open', false)) },
      { cells: { busy: false, open: true }, steps: [click('[data-testid="open-door43-submit"]')], note: 'The open starts: the ✕ goes, Cancel and Open turn off, Open is busy. Until then the ✕, Escape and the scrim close the dialog.' }],
  ]),

  ...group('ScripturePassage', { frame: { width: 560, height: 212 } }, [
    ['latin-lg', { verses }, { frame: { height: 236 } }],
    ['latin-md', { verses, step: 'md' }],
    ['latin-sm', { verses, step: 'sm' }, { frame: { height: 168 } }],
    ['highlight', { verses, highlight: 'servant of God' }, { frame: { height: 236 } }],
    ['highlight-many', { verses, step: 'md', highlight: ['servant of God', 'eternal life', 'the truth'] }],
    ['highlight-wraps', { verses: [{ n: 1, text: T.en1 }], step: 'md', highlight: 'the chosen people of God and the knowledge of the truth that agrees' }, { frame: { width: 360, height: 232 } }],
    ['three-verses', { verses: [...verses, { n: 3, text: T.en3 }], step: 'md' }, { frame: { height: 264 } }],
    ['greek', { verses: [{ n: 1, text: T.el1 }, { n: 2, text: T.el2 }], script: 'greek', step: 'md' }],
    ['hebrew', { verses: [{ n: 1, text: T.he1 }, { n: 2, text: T.he2 }], script: 'hebrew', step: 'md' }, { frame: { height: 136 } }],
    ['hebrew-highlight', { verses: [{ n: 1, text: T.he1 }], script: 'hebrew', step: 'md', highlight: 'אֶל־יוֹנָ֥ה' }, { frame: { height: 104 } }],
    ['arabic', { verses: [{ n: 1, text: T.ar1 }, { n: 2, text: T.ar2 }], script: 'arabic', step: 'md' }],
    ['nastaliq', { verses: [{ n: 1, text: T.urdu }], script: 'nastaliq' }, { frame: { height: 240 }, note: 'Awami Nastaliq is a Graphite font. Chromium has no Graphite, so it draws the letters unjoined; tC4 ships in Electronite, which joins them.' }],
    ['size-override', { verses, size: '18px', style: { lineHeight: '28px' } }, { frame: { height: 176 } }],
    ['ltr-forced-on-hebrew', { verses: [{ n: 1, text: T.he1 }], script: 'hebrew', dir: 'ltr', step: 'md' }, { frame: { height: 104 } }],
  ]),

  ...group('VerseMarker', { frame: { width: 72, height: 72 } }, [
    ['n-1', { n: 1 }],
    ['n-12', { n: 12 }],
    ['n-123', { n: 123 }],
    ['size-24', { n: 9, size: 24 }],
    ['size-48', { n: 9, size: 48 }, { frame: { height: 88 } }],
    ['dragging', { n: 9, state: 'dragging' }],
    ['focus-visible', { n: 9, label: 'Move where verse 9 begins' }, { steps: [focus('[role="button"]')] }],
    ['inline-in-text', { n: 10 }, {
      frame: { width: 420, height: 104 },
      wrap: (n) => el('p', { style: { fontFamily: 'var(--font-scripture)', fontSize: 'var(--fs-verse-md)', lineHeight: 'var(--lh-verse-md)', margin: 0 }, children: ['in hope of eternal life ', n, ' that God, who does not lie, promised'] }),
    }],
  ]),

  ...group('WordChip', { frame: { width: 180, height: 64 } }, [
    ...['default', 'selected', 'suggested', 'placed'].map((state) => [state, { state, children: 'servant' }]),
    ['long-word', { children: 'righteousness-giving' }, { frame: { width: 240 } }],
    ['bank-row', { children: 'servant' }, {
      frame: { width: 420, height: 72 },
      wrap: (n) => el('div', { style: { display: 'flex', flexWrap: 'wrap', gap: 8 }, children: [n, el('WordChip', { state: 'selected', children: 'of' }), el('WordChip', { state: 'suggested', children: 'God' }), el('WordChip', { state: 'placed', children: 'Paul' })] }),
    }],
    /* #735 (#750): a word that occurs more than once carries its occurrence number in a
       <sup> with line-height 0, so the chip keeps the height of a plain one
       (src/views/Align.jsx:45-47, 143-149). Top-aligned, so a taller chip would show. */
    ['suggested-occurrence', { state: 'suggested', draggable: false, title: 'Confirm this link', 'aria-label': 'de 2', 'data-word': 'de',
      children: ['de', el('sup', { style: { lineHeight: 0 }, children: 2 }), el('span', { 'aria-hidden': 'true', style: { fontWeight: 'var(--fw-heavy)' }, children: '✓' })] }, {
      frame: { width: 240, height: 72 },
      wrap: (n) => el('div', { style: { display: 'flex', alignItems: 'flex-start', gap: 8 }, children: [
        el('WordChip', { state: 'suggested', draggable: false, title: 'Confirm this link', 'data-word': 'Dios',
          children: ['Dios', el('span', { 'aria-hidden': 'true', style: { fontWeight: 'var(--fw-heavy)' }, children: '✓' })] }), n] }),
    }],
  ]),
];
