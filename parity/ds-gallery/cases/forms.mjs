// components/forms: Checkbox, DropZone, FilterChip, OptionCard, Radio + RadioGroup, SearchField,
// Select, SuggestField, TextArea, TextField, Toggle.
import { noop, group, focus, hover, click, key, AR, T } from './lib.mjs';

const FONTS = ['Noto Sans (default)', 'Charis SIL', 'Scheherazade New', 'Padauk — Myanmar', 'Awami Nastaliq'];
const BOOKS = [
  ['Ruth', 'RUT', 'Old Testament'], ['Esther', 'EST', 'Old Testament'], ['Jonah', 'JON', 'Old Testament'],
  ['Micah', 'MIC', 'Old Testament'], ['Matthew', 'MAT', 'New Testament'], ['Mark', 'MRK', 'New Testament'],
  ['Luke', 'LUK', 'New Testament'], ['John', 'JHN', 'New Testament'], ['Galatians', 'GAL', 'New Testament'],
  ['Titus', 'TIT', 'New Testament'], ['Philemon', 'PHM', 'New Testament'], ['Jude', 'JUD', 'New Testament'],
];
const bookOptions = (grouped) => BOOKS.map(([label, code, g]) => ({ value: code, label, code, ...(grouped ? { group: g } : null) }));
const LICENSES = [
  { value: 'cc-by-sa', label: 'CC BY-SA 4.0', badge: 'Recommended' },
  { value: 'cc-by', label: 'CC BY 4.0', meta: 'Attribution only' },
  { value: 'cc0', label: 'CC0 1.0', meta: 'Public domain dedication' },
  { value: 'custom', label: 'Custom licence', meta: 'Not available yet', disabled: true },
];
const LANGS = [
  { value: 'ha', label: 'Hausa', meta: 'Harshen Hausa', code: 'ha' },
  { value: 'haw', label: 'Hawaiian', meta: 'ʻŌlelo Hawaiʻi', code: 'haw' },
  { value: 'hak', label: 'Hakka Chinese', code: 'hak' },
];
const LANGS_AR = [
  { value: 'ar', label: 'العربية', code: 'ar' },
  { value: 'arq', label: 'العربية الجزائرية', code: 'arq' },
];
const DIRECTIONS = [
  { value: 'ltr', label: 'Left to right', description: 'English, Español, Kiswahili' },
  { value: 'rtl', label: 'Right to left', description: 'العربية, עברית, اردو' },
];

const select = (props) => ({ label: 'Script font', options: FONTS, value: FONTS[0], onChange: noop, ...props });
const OPEN = [click('[role="combobox"]')];

export default [
  ...group('Checkbox', { frame: { width: 64, height: 60 } }, [
    ['unchecked', {}],
    ['checked', { checked: true }],
    ['indeterminate', { indeterminate: true }],
    ['disabled', { disabled: true }],
    ['disabled-checked', { disabled: true, checked: true }],
    ['label', { label: 'Stay signed in' }, { frame: { width: 300 } }],
    ['label-checked', { label: 'Stay signed in', checked: true }, { frame: { width: 300 } }],
    ['label-description', { label: 'Stay signed in', description: 'Keeps a token on this computer. Your password is never stored.' }, { frame: { width: 360, height: 96 } }],
    ['label-disabled', { label: 'Stay signed in', disabled: true }, { frame: { width: 300 } }],
    ['focus-visible', { label: 'Stay signed in', onChange: noop }, { frame: { width: 300 }, steps: [focus('input')] }],
    ['long-label', { label: 'Include the alignment data with the exported USFM file', checked: true }, { frame: { width: 260, height: 76 } }],
    ['rtl', { label: 'البقاء مسجلاً للدخول', checked: true }, { document: AR, frame: { width: 300 } }],
  ]),

  ...group('DropZone', { frame: { width: 440, height: 170 } }, [
    ['default', {}],
    ['hint', { title: 'Drop a project here, or browse', hint: 'USFM files, a Scripture Burrito zip or a translationCore 3 project' }],
    ['hover', { hint: 'USFM files or a Scripture Burrito zip', onClick: noop }, { steps: [hover('button')] }],
    ['focus-visible', { onClick: noop }, { steps: [focus('button')] }],
    ['long-title', { title: 'Drop the project files you want to bring into this computer here, or browse for them' }, { frame: { width: 360, height: 190 } }],
    ['rtl', { title: 'أفلت الملف هنا، أو تصفح', hint: 'ملفات USFM أو ملف zip' }, { document: AR }],
  ]),

  ...group('FilterChip', { frame: { width: 160, height: 68 } }, [
    ['accent', { children: 'Notes' }],
    ['accent-selected', { children: 'Notes', selected: true }],
    ['ocean', { tone: 'ocean', children: 'Notes' }],
    ['ocean-selected', { tone: 'ocean', children: 'Notes', selected: true }],
    ['valid-selected', { tone: 'valid', children: 'Valid', selected: true }],
    ['mark', { children: 'Bookmarked', mark: '★', selected: true }, { frame: { width: 200 } }],
    ['hover', { children: 'Notes', onClick: noop }, { steps: [hover('button')] }],
    ['selected-hover', { children: 'Notes', selected: true, onClick: noop }, { steps: [hover('button')] }],
    ['focus-visible', { children: 'Notes', onClick: noop }, { steps: [focus('button')] }],
    ['long-text', { children: 'Invalidated by an edit to the verse', selected: true }, { frame: { width: 300 } }],
    ['rtl', { children: 'ملاحظات', selected: true }, { document: AR }],
  ]),

  ...group('OptionCard', { frame: { width: 460, height: 124 } }, [
    ['default', { title: 'English', description: 'unfoldingWord', meta: 'en_tn v86' }],
    ['selected', { title: 'English', description: 'unfoldingWord', meta: 'en_tn v86', selected: true }],
    ['radio', { control: 'radio', title: 'Literal text (ULT)', description: 'Follows the original word order closely.' }],
    ['radio-selected', { control: 'radio', title: 'Literal text (ULT)', description: 'Follows the original word order closely.', selected: true }],
    ['icon', { icon: 'USFM', title: 'USFM files', description: 'One file per book, from Paratext or another editor.' }],
    ['recommended', { title: 'Español (Latinoamérica)', description: 'unfoldingWord · es-419_gl', recommended: true, recommendedLabel: 'Recommended' }],
    ['trailing', { title: 'Translation Notes', description: 'en_tn', trailing: 'Always included' }],
    ['disabled', { title: 'Hindi', description: 'Not on this computer', disabled: true }],
    ['hover', { title: 'English', description: 'unfoldingWord', onClick: noop }, { steps: [hover('button')] }],
    ['focus-visible', { title: 'English', description: 'unfoldingWord', onClick: noop }, { steps: [focus('button')] }],
    ['long-text', { control: 'radio', title: 'Create the project from a translationCore 3 backup with its check decisions', description: 'Every alignment and check decision is carried over. Comments and bookmarks come too, and invalidated checks stay marked for review.', recommended: true }, { frame: { width: 380, height: 192 } }],
    ['rtl', { control: 'radio', title: 'العربية', description: 'unfoldingWord · ar_gl', selected: true, recommended: true, recommendedLabel: 'موصى به' }, { document: AR }],
  ]),

  ...group('Radio', { frame: { width: 64, height: 56 } }, [
    ['unchecked', {}],
    ['checked', { checked: true }],
    ['disabled', { disabled: true }],
    ['disabled-checked', { disabled: true, checked: true }],
    ['label', { label: 'Left to right', name: 'dir', value: 'ltr' }, { frame: { width: 260 } }],
    ['label-description', { label: 'Left to right', description: 'English, Español, Kiswahili', checked: true }, { frame: { width: 300, height: 76 } }],
    ['focus-visible', { label: 'Left to right', onChange: noop }, { frame: { width: 260 }, steps: [focus('input')] }],
    ['rtl', { label: 'من اليمين إلى اليسار', checked: true }, { document: AR, frame: { width: 260 } }],
  ]),

  ...group('RadioGroup', { frame: { width: 380, height: 150 } }, [
    ['legend', { legend: 'Text direction', options: DIRECTIONS.map(({ value, label }) => ({ value, label })), value: 'ltr', onChange: noop }, { frame: { height: 120 } }],
    ['descriptions', { legend: 'Text direction', options: DIRECTIONS, value: 'rtl', onChange: noop }, { frame: { height: 170 } }],
    ['strings', { legend: 'Line spacing', options: ['Single', 'Double'], value: 'Single', onChange: noop }, { frame: { height: 120 } }],
    ['no-legend', { 'aria-label': 'Paper', options: ['A4', 'Letter'], value: 'Letter', onChange: noop }, { frame: { height: 96 } }],
    ['focus-visible', { legend: 'Text direction', options: DIRECTIONS.map(({ value, label }) => ({ value, label })), value: 'ltr', onChange: noop }, { frame: { height: 120 }, steps: [focus('input[value="ltr"]')] }],
    ['rtl', { legend: 'اتجاه النص', options: [{ value: 'ltr', label: 'من اليسار إلى اليمين' }, { value: 'rtl', label: 'من اليمين إلى اليسار' }], value: 'rtl', onChange: noop }, { document: AR, frame: { height: 120 } }],
  ]),

  ...group('SearchField', { frame: { width: 320, height: 68 } }, [
    ['placeholder', { placeholder: 'Search books' }],
    ['value-clear', { value: 'Tit', onChange: noop, onClear: noop, clearLabel: 'Clear' }],
    ['value-no-clear', { value: 'Tit', onChange: noop }],
    ['focused', { placeholder: 'Search books' }, { steps: [focus('input')] }],
    ['disabled', { placeholder: 'Search books', disabled: true }],
    ['rtl', { value: 'تيطس', onChange: noop, onClear: noop, clearLabel: 'مسح' }, { document: AR }],
  ]),

  ...group('Select', { frame: { width: 340, height: 100 } }, [
    ['value', select()],
    ['value-code', select({ label: 'Book', options: bookOptions(false), value: 'TIT' })],
    ['no-value', select({ value: undefined })],
    ['hint', select({ hint: 'How the project’s own text is drawn.' }), { frame: { height: 120 } }],
    ['no-label', select({ label: undefined })],
    ['disabled', select({ disabled: true })],
    ['hover', select(), { steps: [hover('[role="combobox"]')] }],
    ['focus-visible', select(), { steps: [focus('[role="combobox"]')] }],
    ['open', select(), { frame: { height: 330 }, steps: OPEN }],
    ['open-search', select({ label: 'Book', options: bookOptions(false), value: 'TIT', searchPlaceholder: 'Search books', clearLabel: 'Clear' }), { frame: { height: 500 }, steps: OPEN }],
    ['open-groups', select({ label: 'Book', options: bookOptions(true), value: 'JON', searchPlaceholder: 'Search books' }), { frame: { height: 500 }, steps: OPEN }],
    ['open-no-matches', select({ label: 'Book', options: bookOptions(false), value: 'TIT', searchPlaceholder: 'Search books', noMatchesLabel: 'No matches' }), { frame: { height: 220 }, steps: [...OPEN, { do: 'type', text: 'zz' }] }],
    ['open-rows', select({ label: 'Licence', options: LICENSES, value: 'cc-by-sa' }), { frame: { height: 300 }, steps: OPEN }],
    ['open-keyboard', select(), { frame: { height: 330 }, steps: [focus('[role="combobox"]'), key('ArrowDown'), key('ArrowDown')] }],
    ['long-value', select({ label: 'Checking language', options: ['English — unfoldingWord Translation Notes, Words and Questions'], value: 'English — unfoldingWord Translation Notes, Words and Questions' }), { frame: { width: 280 } }],
    ['rtl', select({ label: 'خط النص', options: ['Noto Sans (افتراضي)', 'Awami Nastaliq'], value: 'Noto Sans (افتراضي)' }), { document: AR }],
    ['rtl-open', select({ label: 'السفر', options: BOOKS.slice(0, 5).map(([, code], i) => ({ value: code, label: ['راعوث', 'أستير', 'يونان', 'ميخا', 'متى'][i], code })), value: 'JON' }), { document: AR, frame: { height: 330 }, steps: OPEN }],
  ]),

  ...group('SuggestField', { frame: { width: 340, height: 100 } }, [
    ['value', { label: 'Language name', value: 'Ha', onChange: noop, onChoose: noop, suggestions: LANGS }],
    ['hint', { label: 'Language name', hint: 'Type the name, then choose it from the list.', placeholder: 'e.g. Kanuri', value: '', onChange: noop, suggestions: [] }, { frame: { height: 120 } }],
    ['open', { label: 'Language name', value: 'Ha', onChange: noop, onChoose: noop, suggestions: LANGS }, { frame: { height: 260 }, steps: [focus('input'), key('ArrowDown')] }],
    ['open-typed', { label: 'Language name', value: 'Ha', onChange: noop, onChoose: noop, suggestions: LANGS }, { frame: { height: 260 }, steps: [click('input'), { do: 'type', text: 'u' }] }],
    ['no-suggestions-focused', { label: 'Language name', value: 'Zzz', onChange: noop, suggestions: [] }, { steps: [focus('input')] }],
    ['rtl-open', { label: 'اسم اللغة', value: 'العر', onChange: noop, onChoose: noop, suggestions: LANGS_AR }, { document: AR, frame: { height: 220 }, steps: [focus('input'), key('ArrowDown')] }],
  ]),

  ...group('TextArea', { frame: { width: 420, height: 150 } }, [
    ['placeholder', { label: 'Comment', placeholder: 'Type your translation of this verse…' }],
    ['value', { label: 'Comment', defaultValue: 'The word “servant” here could also be “slave”; we chose servant for the reading in Titus 1:1.' }],
    ['no-label', { placeholder: 'Type your translation of this verse…', 'aria-label': 'Verse 1' }, { frame: { height: 128 } }],
    ['rows-6', { label: 'Comment', rows: 6, placeholder: 'Type a comment…' }, { frame: { height: 220 } }],
    ['scripture', { variant: 'scripture', 'aria-label': 'Verse 1', defaultValue: T.en1 }, { frame: { height: 190 } }],
    ['scripture-rtl', { variant: 'scripture', dir: 'rtl', 'aria-label': 'Verse 1', defaultValue: 'بولس، عبد الله ورسول يسوع المسيح', inputStyle: { fontFamily: 'var(--font-arabic)' } }, { frame: { height: 190 }, note: 'TextArea sets only dir; the Arabic face reaches the native field through Input’s inputStyle, as tC4’s targetType gives an Arabic project the --font-arabic family.' }],
    ['disabled', { label: 'Comment', disabled: true, defaultValue: 'Saved comments cannot be emptied.' }],
    ['invalid', { label: 'Comment', invalid: true, defaultValue: 'A comment that failed to save' }],
    ['focused', { label: 'Comment', placeholder: 'Type a comment…' }, { steps: [focus('textarea')] }],
    ['rtl', { label: 'تعليق', placeholder: 'اكتب ترجمتك لهذه الآية…' }, { document: AR }],
  ]),

  ...group('TextField', { frame: { width: 340, height: 96 } }, [
    ['placeholder', { label: 'Bible name', placeholder: 'e.g. Kanuri Bible' }],
    ['value', { label: 'Bible name', defaultValue: T.project }],
    ['hint', { label: 'Bible name', defaultValue: T.project, hint: 'Renaming arrives with a later increment.' }, { frame: { height: 120 } }],
    ['invalid', { label: 'Code', defaultValue: 'english', invalid: true }],
    ['disabled', { label: 'Code', defaultValue: 'es-419', disabled: true }],
    ['focused', { label: 'Bible name', placeholder: 'e.g. Kanuri Bible' }, { steps: [focus('input')] }],
    ['focused-invalid', { label: 'Code', defaultValue: 'english', invalid: true }, { steps: [focus('input')] }],
    ['size-sm', { label: 'Code', defaultValue: 'es-419', size: 'sm' }],
    ['size-lg', { label: 'Bible name', defaultValue: T.project, size: 'lg' }, { frame: { height: 104 } }],
    ['no-label', { placeholder: 'e.g. Kanuri Bible', 'aria-label': 'Bible name' }, { frame: { height: 68 } }],
    ['long-value', { label: 'Bible name', defaultValue: 'Biblia del Equipo Ejemplo — Tito, Jonás, Rut y los demás libros del proyecto' }],
    ['rtl', { label: 'اسم الكتاب المقدس', defaultValue: T.projectAr }, { document: AR }],
  ]),

  ...group('Toggle', { frame: { width: 72, height: 60 } }, [
    ['off', {}],
    ['on', { checked: true }],
    ['disabled', { disabled: true }],
    ['disabled-on', { disabled: true, checked: true }],
    ['label', { label: 'Verse numbers' }, { frame: { width: 280 } }],
    ['label-on', { label: 'Verse numbers', checked: true }, { frame: { width: 280 } }],
    ['label-disabled', { label: 'Verse numbers', disabled: true, checked: true }, { frame: { width: 280 } }],
    ['focus-visible', { label: 'Verse numbers', onChange: noop }, { frame: { width: 280 }, steps: [focus('input')] }],
    ['rtl', { label: 'أرقام الآيات', checked: true }, { document: AR, frame: { width: 280 } }],
  ]),
];
