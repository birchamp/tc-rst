// components/navigation: AppHeader, Breadcrumb, Menu.
import { el, noop, asset, group, focus, hover, AR, T } from './lib.mjs';

const modeSwitch = (labels, value = 'draft') => el('Switcher', {
  indicator: 'pill', value, onChange: noop,
  options: [{ value: 'read', label: labels[0] }, { value: 'draft', label: labels[1] }, { value: 'check', label: labels[2] }],
});
const saved = (label) => el('StatusDot', { status: 'valid', size: 8, label });
const project = {
  logoSrc: asset('public/assets/translationcore-logo.png'),
  projectInitials: 'EE', projectName: T.project, projectMeta: 'es-419', switchTitle: 'Switch project',
  onBrandClick: noop, onProjectClick: noop,
  center: modeSwitch(['Understand', 'Translate', 'Check']),
  right: saved('Saved'),
};

const CRUMBS = [
  { label: 'Equipo Ejemplo', onClick: noop }, { label: 'Titus', onClick: noop },
  { label: 'Chapter 2', onClick: noop }, { label: 'Translation Notes' },
];
const MENU_ITEMS = [
  { label: 'USFM', meta: '.usfm', onClick: noop },
  { label: 'Scripture Burrito', meta: '.zip', onClick: noop },
  { divider: true },
  { label: 'PDF', meta: '.pdf', disabled: true },
  { label: 'Remove project', destructive: true, onClick: noop },
];
const trigger = (label = 'Export') => el('Button', { variant: 'secondary', size: 'sm', children: label + ' ▾' });

export default [
  ...group('AppHeader', { frame: { width: 1040, height: 88 } }, [
    ['home', { logoSrc: asset('public/assets/translationcore-logo.png'), onBrandClick: noop }],
    ['project', project],
    ['project-default-right', { ...project, right: undefined }],
    ['light', { ...project, tone: 'light' }],
    ['no-logo', { ...project, logoSrc: undefined }],
    ['long-project-name', { ...project, projectName: 'Biblia del Equipo Ejemplo — Tito, Jonás, Rut, Ester y los demás libros', projectMeta: 'es-419 · 12 books' }],
    ['project-hover', project, { steps: [hover('[data-testid="project-switch"]')] }],
    ['project-focus-visible', project, { steps: [focus('[data-testid="project-switch"]')] }],
    ['project-name-rtl', { ...project, projectInitials: 'مك', projectName: T.projectAr, projectNameDir: 'rtl', projectMeta: 'ar' }],
    ['rtl', { ...project, projectInitials: 'مك', projectName: T.projectAr, projectNameDir: 'rtl', projectMeta: 'ar', switchTitle: 'تبديل المشروع', center: modeSwitch(['فهم', 'ترجمة', 'مراجعة']), right: saved('تم الحفظ') }, { document: AR }],
  ]),

  ...group('Breadcrumb', { frame: { width: 440, height: 52 } }, [
    ['default', { items: CRUMBS }],
    ['two-items', { items: CRUMBS.slice(0, 1).concat({ label: 'Titus' }) }],
    ['single', { items: [{ label: 'Your projects' }] }],
    ['wrapping', { items: [...CRUMBS.slice(0, 3), { label: 'Translation Notes · Figures of speech · Abstract nouns' }] }, { frame: { width: 300, height: 96 } }],
    ['link-hover', { items: CRUMBS }, { steps: [hover('button')] }],
    ['link-focus-visible', { items: CRUMBS }, { steps: [focus('button')] }],
    ['rtl', { label: 'مسار التنقل', items: [{ label: 'مشروعي', onClick: noop }, { label: 'تيطس', onClick: noop }, { label: 'الأصحاح 2' }] }, { document: AR }],
  ]),

  ...group('Menu', { frame: { width: 320, height: 280 } }, [
    ['closed', { trigger: trigger(), items: MENU_ITEMS }, { frame: { height: 72 } }],
    ['open', { trigger: trigger(), items: MENU_ITEMS, open: true }],
    ['open-align-end', { trigger: trigger(), items: MENU_ITEMS, open: true, align: 'end' }, {
      wrap: (node) => el('div', { style: { display: 'flex', justifyContent: 'flex-end' }, children: node }),
    }],
    ['open-no-meta', { trigger: trigger('Account'), items: [{ label: 'Sign in to Door43', onClick: noop }, { label: 'App language', onClick: noop }, { label: 'Help and guides', onClick: noop }], open: true }, { frame: { height: 200 } }],
    ['item-hover', { trigger: trigger(), items: MENU_ITEMS, open: true }, { steps: [hover('[role="menuitem"]')] }],
    ['item-focus-visible', { trigger: trigger(), items: MENU_ITEMS, open: true }, { steps: [focus('[role="menuitem"]')] }],
    ['open-rtl', { trigger: trigger('تصدير'), items: [{ label: 'USFM', meta: '.usfm', onClick: noop }, { label: 'حزمة Scripture Burrito', meta: '.zip', onClick: noop }, { divider: true }, { label: 'إزالة المشروع', destructive: true, onClick: noop }], open: true }, { document: AR, frame: { height: 240 } }],
  ]),
];
