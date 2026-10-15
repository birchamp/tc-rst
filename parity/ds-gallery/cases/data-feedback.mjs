// components/data and components/feedback: Accordion, Avatar, KeyValueGrid, ConfirmDialog,
// EmptyState, InlineError, Spinner, Toast, Tooltip.
import { el, noop, asset, group, onDark, centred, focus, hover, AR, T } from './lib.mjs';

const ACCORDION = [
  { key: 'what', title: 'What this check covers', meta: '3', content: 'Translation notes explain words and phrases that may be hard to translate. Each note names the issue and suggests ways to express the meaning.' },
  { key: 'how', title: 'How alignment works', content: 'Drag each word of your translation onto the original word it translates. A word can be aligned to more than one original word.' },
  { key: 'sources', title: 'Source texts', meta: 'v86', content: 'The source texts are pinned to the versions this project was created with.' },
];
const ACCORDION_AR = [
  { key: 'what', title: 'ما الذي تغطيه هذه المراجعة', meta: '3', content: 'تشرح ملاحظات الترجمة الكلمات والعبارات التي قد تصعب ترجمتها.' },
  { key: 'how', title: 'كيف تعمل المحاذاة', content: 'اسحب كل كلمة من ترجمتك إلى الكلمة الأصلية التي تترجمها.' },
];
const FACTS = [
  { k: 'Language', v: 'Español (Latinoamérica)' },
  { k: 'Code', v: 'es-419' },
  { k: 'Books', v: '2' },
  { k: 'Direction', v: 'Left to right' },
];

const CONFIRM = {
  open: true,
  title: 'Remove all the text of section 9-10?',
  description: 'The verses go back to empty and the book’s progress drops.',
  confirmLabel: 'Remove the text',
  cancelLabel: 'Keep editing',
  onConfirm: noop,
  onCancel: noop,
};

const TOOLTIP_TRIGGER = el('Button', { variant: 'secondary', size: 'sm', children: 'Settings' });

export default [
  ...group('Accordion', { frame: { width: 440, height: 200 } }, [
    ['closed', { items: ACCORDION }],
    ['first-open', { items: ACCORDION, defaultOpen: ['what'] }, { frame: { height: 260 } }],
    ['all-open', { items: ACCORDION, defaultOpen: ['what', 'how', 'sources'] }, { frame: { height: 380 } }],
    ['row-hover', { items: ACCORDION }, { steps: [hover('button[aria-controls="how-p"]')] }],
    ['row-focus-visible', { items: ACCORDION }, { steps: [focus('button[aria-controls="what-p"]')] }],
    ['long-title', { items: [{ key: 'l', title: 'A long section title that has to wrap onto a second line inside the row', meta: '12', content: 'x' }] }, { frame: { width: 320, height: 120 } }],
    ['rtl', { items: ACCORDION_AR, defaultOpen: ['what'] }, { document: AR, frame: { height: 220 } }],
  ]),

  ...group('Avatar', { frame: { width: 80, height: 76 } }, [
    ...['ocean', 'accent', 'cultivate', 'muted'].flatMap((tone) => [
      [`${tone}-44`, { initials: 'EE', tone }],
      [`${tone}-32`, { initials: 'EE', tone, size: 32 }],
    ]),
    ['circle', { initials: 'TJ', shape: 'circle' }],
    ['size-64', { initials: 'TJ', size: 64 }, { frame: { width: 96, height: 96 } }],
    ['image', { src: asset('public/assets/logo-mark-color.png'), tone: 'muted' }, { frame: { height: 84 } }],
    ['rtl-initials', { initials: 'مك' }, { document: AR }],
  ]),

  ...group('KeyValueGrid', { frame: { width: 420, height: 180 } }, [
    ['two-columns', { items: FACTS }],
    ['three-columns', { items: FACTS.slice(0, 3), columns: 3 }, { frame: { height: 120 } }],
    ['one-column', { items: FACTS.slice(0, 2), columns: 1 }, { frame: { width: 260, height: 168 } }],
    ['long-values', { items: [{ k: 'Checking language', v: 'English — unfoldingWord® Translation Notes, Words and Questions' }, { k: 'Pinned', v: 'en_tn v86 · en_tw v86 · en_tq v86' }] }, { frame: { height: 164 } }],
    ['rtl', { items: [{ k: 'اللغة', v: 'العربية' }, { k: 'الرمز', v: 'ar' }, { k: 'الأسفار', v: '2' }, { k: 'الاتجاه', v: 'من اليمين إلى اليسار' }] }, { document: AR }],
  ]),

  ...group('ConfirmDialog', { frame: { width: 600, height: 340 } }, [
    ['default', { ...CONFIRM }],
    ['destructive', { ...CONFIRM, destructive: true }],
    ['consequence', { ...CONFIRM, destructive: true, consequence: '2 verses lose their text. Their alignments and check decisions are kept and marked for review.' }, { frame: { height: 420 } }],
    ['default-labels', { open: true, title: 'Leave without saving?', onConfirm: noop, onCancel: noop }, { frame: { height: 280 } }],
    ['long-title', { ...CONFIRM, title: 'Remove all the text of the section that runs from verse 9 to verse 10 of Titus 2?' }, { frame: { height: 380 } }],
    ['rtl', { ...CONFIRM, destructive: true, title: 'إزالة كل نص المقطع 9-10؟', description: 'تعود الآيات فارغة وتنخفض نسبة تقدم السفر.', confirmLabel: 'إزالة النص', cancelLabel: 'مواصلة التعديل' }, { document: AR }],
  ]),

  ...group('EmptyState', { frame: { width: 460, height: 200 } }, [
    ['block', { title: 'No projects yet', description: 'Select + Add a project to start.' }],
    ['block-action', { title: 'No projects yet', description: 'Select + Add a project to start.', action: el('Button', { children: '+ Add a project' }) }, { frame: { height: 250 } }],
    ['title-only', { title: 'Nothing to check in this book' }, { frame: { height: 160 } }],
    ['description-only', { description: 'Translation Words has no entries for Jonah 4.' }, { frame: { height: 160 } }],
    ['long-description', { title: 'Not available for this book', description: 'The pinned Translation Notes do not cover this book. Choose another book, or get a newer version of the notes when one covers it.' }, { frame: { height: 230 } }],
    ['inline', { variant: 'inline', title: 'No comments yet.' }, { frame: { width: 320, height: 64 } }],
    ['rtl', { title: 'لا توجد مشاريع بعد', description: 'اختر + إضافة مشروع للبدء.' }, { document: AR }],
  ]),

  ...group('InlineError', { frame: { width: 340, height: 52 } }, [
    ['default', { children: 'Enter a language code, such as es-419.' }],
    ['long-text', { children: 'The server refused this language code: it is not a valid BCP 47 tag. Check the code, or use an x- private code.' }, { frame: { height: 88 } }],
    ['rtl', { children: 'أدخل رمز اللغة، مثل es-419.' }, { document: AR }],
  ]),

  ...group('Spinner', { frame: { width: 64, height: 60 } }, [
    ['default', {}],
    ['size-32', { size: 32 }, { frame: { height: 72 } }],
    ['label', { label: 'Loading projects…' }, { frame: { width: 220 } }],
    ...['valid', 'invalid', 'warn', 'ocean'].map((tone) => [`tone-${tone}`, { tone }]),
    ['inverse-on-dark', { tone: 'inverse', label: 'Uploading…' }, { frame: { width: 220, height: 92 }, wrap: (node) => onDark(node) }],
    ['rtl-label', { label: 'جارٍ تحميل المشاريع…' }, { document: AR, frame: { width: 220 } }],
  ]),

  ...group('Toast', { frame: { width: 480, height: 88 } }, [
    ...['success', 'info', 'warn', 'error'].map((tone) => [tone, { tone, message: 'Exported Titus as USFM.' }]),
    ['action', { tone: 'success', message: 'Removed the text of section 9-10.', action: noop, actionLabel: 'Undo' }],
    ['dismiss', { tone: 'info', message: 'Exported Titus as USFM.', onDismiss: noop, dismissLabel: 'Dismiss' }],
    ['action-dismiss', { tone: 'error', message: 'Upload failed.', action: noop, actionLabel: 'Try again', onDismiss: noop }],
    ['long-message', { tone: 'warn', message: 'The pinned Translation Notes are older than the ones on this computer. Your decisions stay with the pinned version until you upgrade.', onDismiss: noop }, { frame: { height: 132 } }],
    ['rtl', { tone: 'success', message: 'تم تصدير سفر تيطس بصيغة USFM.', onDismiss: noop, dismissLabel: 'تجاهل' }, { document: AR }],
  ]),

  ...group('Tooltip', { frame: { width: 320, height: 160 }, wrap: centred }, [
    ['resting', { label: 'Switch project', children: TOOLTIP_TRIGGER }],
    ...['bottom', 'top', 'start', 'end'].map((placement) =>
      [`hover-${placement}`, { label: 'Switch project', placement, children: TOOLTIP_TRIGGER }, { steps: [hover('button')] }]),
    ['focus-visible', { label: 'Switch project', children: TOOLTIP_TRIGGER }, { steps: [focus('button')] }],
    ['long-label', { label: `${T.project}. Switch project`, children: TOOLTIP_TRIGGER }, { frame: { width: 480 }, steps: [hover('button')] }],
    ['rtl-end', { label: 'تبديل المشروع', placement: 'end', children: el('Button', { variant: 'secondary', size: 'sm', children: 'الإعدادات' }) }, { document: AR, steps: [hover('button')] }],
  ]),
];
