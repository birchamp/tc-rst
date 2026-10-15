// Helpers for the case files. A case file exports an array of cases built with `group`.

/** A node of the render tree: a DS export (capitalised) or an HTML tag (lower case). */
export const el = (name, props = {}) => ({ $el: name, props });
/** A callback that does nothing. Some components draw a control only when one is passed. */
export const noop = { $fn: 'noop' };
/** The current value of the case's cell `name` (the case option `cells` holds the first). */
export const cell = (name) => ({ $cell: name });
/** A callback that writes cell `name`: the value at `from` in its arguments ("0" is the first). */
export const set = (name, from) => ({ $fn: 'set', cell: name, from });
/** A callback that writes `value` into cell `name`. */
export const setTo = (name, value) => ({ $fn: 'set', cell: name, value });
/** A file of the tC4 checkout, by its path there. */
export const asset = (path) => ({ $asset: path });

/** The Arabic interface: tC4 sets <html lang="ar" dir="rtl"> when the app language is Arabic. */
export const AR = { lang: 'ar', dir: 'rtl' };

/** Interaction steps, run in order before the screenshot (see the spec's `protocol`). */
export const focus = (target) => ({ do: 'focus', target });
export const hover = (target) => ({ do: 'hover', target });
export const press = (target) => ({ do: 'press', target });
export const click = (target) => ({ do: 'click', target });
export const key = (k) => ({ do: 'key', key: k });

/** The Ocean title-bar ground: a solid Ocean Surface sets data-on="dark" for its children. */
export const onDark = (node, pad = 'md') => el('Surface', { tone: 'ocean', fill: 'solid', radius: 'lg', pad, children: node });
/** Centres a node in the frame, for things that open beside or around themselves. */
export const centred = (node) => el('div', {
  style: { display: 'flex', alignItems: 'center', justifyContent: 'center', height: '100%' }, children: node,
});
export const row = (children, gap = 12) => el('div', {
  style: { display: 'flex', alignItems: 'center', gap }, children,
});

/**
 * Cases of one component. `defaults` holds `frame` ({ width, height }) and any other case
 * option; each entry is [state, props, options?]. Options: frame, document, steps, wrap (a
 * function from the component's node to the rendered tree), cells (the first value of each
 * cell that `cell` reads and `set` writes, which is how a case owns state), note.
 */
export function group(component, defaults, entries) {
  return entries.map(([state, props, opts = {}]) => ({
    component,
    state,
    props,
    ...defaults,
    ...opts,
    frame: { ...defaults.frame, ...opts.frame },
  }));
}

/** Shared copy. Interface strings come from tC4's English and Arabic catalogs where they exist there;
 * scripture lines are illustrative samples in each script (Latin, Greek, Hebrew, Arabic, Urdu). */
export const T = {
  save: 'Save changes',
  cancel: 'Cancel',
  saveAr: 'حفظ التغييرات',
  cancelAr: 'إلغاء',
  project: 'Equipo Ejemplo — Tito y Jonás',
  projectAr: 'مشروع الكتاب المقدس بالعربية',
  long: 'Save changes and continue to the next chapter',
  en1: 'Paul, a servant of God and an apostle of Jesus Christ, for the faith of the chosen people of God and the knowledge of the truth that agrees with godliness,',
  en2: 'in hope of eternal life that God, who does not lie, promised before all the ages of time.',
  en3: 'At the right time, he revealed his word by the message that he entrusted me to deliver.',
  el1: 'Παῦλος, δοῦλος Θεοῦ, ἀπόστολος δὲ Ἰησοῦ Χριστοῦ, κατὰ πίστιν ἐκλεκτῶν Θεοῦ καὶ ἐπίγνωσιν ἀληθείας, τῆς κατ’ εὐσέβειαν',
  el2: 'ἐπ’ ἐλπίδι ζωῆς αἰωνίου, ἣν ἐπηγγείλατο ὁ ἀψευδὴς Θεὸς πρὸ χρόνων αἰωνίων,',
  he1: 'וַֽיְהִי֙ דְּבַר־יְהוָ֔ה אֶל־יוֹנָ֥ה בֶן־אֲמִתַּ֖י לֵאמֹֽר׃',
  he2: 'ק֠וּם לֵ֧ךְ אֶל־נִֽינְוֵ֛ה הָעִ֥יר הַגְּדוֹלָ֖ה וּקְרָ֣א עָלֶ֑יהָ',
  ar1: 'بُولُسُ، عَبْدُ اللهِ، وَرَسُولُ يَسُوعَ الْمَسِيحِ، لأَجْلِ إِيمَانِ مُخْتَارِي اللهِ وَمَعْرِفَةِ الْحَقِّ، الَّذِي هُوَ حَسَبُ التَّقْوَى،',
  ar2: 'عَلَى رَجَاءِ الْحَيَاةِ الأَبَدِيَّةِ، الَّتِي وَعَدَ بِهَا اللهُ الْمُنَزَّهُ عَنِ الْكَذِبِ، قَبْلَ الأَزْمِنَةِ الأَزَلِيَّةِ،',
  urdu: 'خدا کا کلام ہمیشہ تک قائم رہے گا اور اُس کی سچائی پشت در پشت ہے',
  note: 'Paul speaks of himself as a servant of God. Here the word servant shows that Paul obeys God in everything he does.',
  noteAr: 'يتحدث بولس عن نفسه بوصفه عبدًا لله. وتدل كلمة عبد هنا على أن بولس يطيع الله في كل ما يفعله.',
};
