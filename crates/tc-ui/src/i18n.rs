//! The interface languages: tC4's locale registry (`src/i18n/locales.js`), its message
//! catalogs, and `t()`, which looks a message up as tC4's `src/i18n/index.js` does.
//!
//! The catalogs in `assets/i18n/<id>.json` are tC4's `src/i18n/<id>.json` at commit `d310bf3`,
//! copied byte for byte: flat objects from a dotted key to a message with `{name}`
//! placeholders, 1,029 keys each. tC4 has no other source of interface text, so a screen that
//! uses the same key shows the same words. To update them, run `crates/tc-ui/vendor-tc4.sh` on a
//! tC4 checkout, then `cargo test -p tc-ui`, which checks every catalog against English and
//! every vendored file against tC4's blob id in `vendored-from-tc4.txt`. Text that only the
//! rebuild shows is in `assets/i18n-rebuild/<id>.json`, read after the vendored catalog of the
//! same language and never overriding it.
//!
//! A message is named by a [`Key`], and only [`keys`] makes one from text: `build.rs` writes a
//! constant for every key of the two English files, so a misspelled key does not compile.
//! [`t_dyn`] takes a key as text for the few that a `tc-api` type names (`NetKind::ask_key`)
//! or a screen builds at run time; `tests/i18n_keys.rs` scans the source for those.
//!
//! The state is per thread: a browser runs the UI on one thread, and each host test gets its
//! own copy. With the `app` feature in the browser, [`show_locale`] sets `<html lang dir>` and
//! tells every view that has read [`t`] or [`shown_locale`] to render again.

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::fmt::Display;
use std::rc::Rc;

use tc_api::ids::LocaleId;

/// The direction an interface reads in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextDir {
    Ltr,
    Rtl,
}

impl TextDir {
    /// The value of the `dir` attribute.
    pub fn as_str(self) -> &'static str {
        match self {
            TextDir::Ltr => "ltr",
            TextDir::Rtl => "rtl",
        }
    }
}

/// One installed interface language.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Locale {
    /// The catalog's id, which is also its file name.
    pub id: &'static str,
    /// The language's own name, as the picker shows it.
    pub label: &'static str,
    /// The direction the interface reads in (#657). Scripture sets its own from the project.
    pub dir: TextDir,
}

/// The installed catalogs, in picker order (#522, #622; Arabic #649). The picker offers exactly
/// these.
pub const LOCALES: [Locale; 10] = [
    Locale {
        id: "en",
        label: "English",
        dir: TextDir::Ltr,
    },
    Locale {
        id: "es-419",
        label: "Español (Latinoamérica)",
        dir: TextDir::Ltr,
    },
    Locale {
        id: "fr",
        label: "Français",
        dir: TextDir::Ltr,
    },
    Locale {
        id: "hi",
        label: "हिन्दी",
        dir: TextDir::Ltr,
    },
    Locale {
        id: "pt-BR",
        label: "Português (Brasil)",
        dir: TextDir::Ltr,
    },
    Locale {
        id: "id",
        label: "Bahasa Indonesia",
        dir: TextDir::Ltr,
    },
    Locale {
        id: "uk",
        label: "Українська",
        dir: TextDir::Ltr,
    },
    Locale {
        id: "ru",
        label: "Русский",
        dir: TextDir::Ltr,
    },
    Locale {
        id: "vi",
        label: "Tiếng Việt",
        dir: TextDir::Ltr,
    },
    Locale {
        id: "ar",
        label: "العربية",
        dir: TextDir::Rtl,
    },
];

/// The language of a first start, and of a saved choice that is not installed.
pub const DEFAULT_LOCALE: &str = "en";

/// The installed locale with this id. Ids are compared exactly: `EN` and `hi-IN` are not
/// installed.
pub fn locale(id: &str) -> Option<&'static Locale> {
    LOCALES.iter().find(|l| l.id == id)
}

/// The `{name}` placeholders of a message, sorted, each as often as it occurs. A translation
/// keeps those of its English source.
pub fn placeholders(message: &str) -> Vec<&str> {
    let mut found = Vec::new();
    let mut rest = message;
    while let Some(open) = rest.find('{') {
        let after = &rest[open + 1..];
        let name_len = after
            .find(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
            .unwrap_or(after.len());
        if name_len > 0 && after[name_len..].starts_with('}') {
            found.push(&rest[open..open + name_len + 2]);
            rest = &after[name_len + 1..];
        } else {
            rest = after;
        }
    }
    found.sort_unstable();
    found
}

/// The locale of an API locale id. A test proves `LOCALES` and `tc_api::ids::LOCALE_IDS` hold
/// the same ten ids in the same order, so the search always finds it.
pub fn locale_of(id: LocaleId) -> &'static Locale {
    LOCALES
        .iter()
        .find(|l| l.id == id.as_str())
        .unwrap_or(&LOCALES[0])
}

/// `<html lang>` and `<html dir>` for the interface language: the id, and `ltr` or `rtl`.
pub fn html_lang_dir(id: LocaleId) -> (&'static str, &'static str) {
    (id.as_str(), locale_of(id).dir.as_str())
}

/// A catalog key that exists in English. Only [`keys`] makes one from text.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Key(&'static str);

impl Key {
    pub const fn as_str(self) -> &'static str {
        self.0
    }
}

impl Display for Key {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.0)
    }
}

/// One constant per key of `assets/i18n/en.json` and `assets/i18n-rebuild/en.json`, in nested
/// modules: `keys::app::SAVED` is `app.saved`, `keys::refusal::open::LOCK_HELD` is
/// `refusal.open.lock-held`. Each constant's documentation is its English text.
pub mod keys {
    include!(concat!(env!("OUT_DIR"), "/i18n_keys.rs"));
}

/// tC4's English catalog, compiled in: the fallback of every key.
pub const ENGLISH_VENDORED: &str = include_str!("../assets/i18n/en.json");

/// The rebuild's own text for each language, compiled in: (id, the file's text). A file is a
/// few hundred bytes; English must be here anyway, because `build.rs` reads it.
pub const REBUILD: [(&str, &str); 10] = [
    ("en", include_str!("../assets/i18n-rebuild/en.json")),
    ("es-419", include_str!("../assets/i18n-rebuild/es-419.json")),
    ("fr", include_str!("../assets/i18n-rebuild/fr.json")),
    ("hi", include_str!("../assets/i18n-rebuild/hi.json")),
    ("pt-BR", include_str!("../assets/i18n-rebuild/pt-BR.json")),
    ("id", include_str!("../assets/i18n-rebuild/id.json")),
    ("uk", include_str!("../assets/i18n-rebuild/uk.json")),
    ("ru", include_str!("../assets/i18n-rebuild/ru.json")),
    ("vi", include_str!("../assets/i18n-rebuild/vi.json")),
    ("ar", include_str!("../assets/i18n-rebuild/ar.json")),
];

/// The rebuild's file for a language, compiled in.
pub fn rebuild_text(id: LocaleId) -> &'static str {
    REBUILD
        .iter()
        .find(|(known, _)| *known == id.as_str())
        .map_or("{}", |(_, text)| text)
}

#[derive(Debug, thiserror::Error)]
pub enum CatalogError {
    #[error("{which} is not a JSON object")]
    NotAnObject { which: &'static str },
    #[error("{which} is not JSON: {source}")]
    Json {
        which: &'static str,
        source: serde_json::Error,
    },
}

/// One language's messages: tC4's catalog, then the rebuild's file, which never overrides it.
/// Only string entries are kept, so an entry that is not a string reads as absent (tC4 treats
/// it so); an empty string is kept, and [`t`] decides what it means.
#[derive(Debug, Clone, Default)]
pub struct Catalog {
    entries: HashMap<Box<str>, Box<str>>,
}

impl Catalog {
    pub fn parse(vendored: &str, rebuild: &str) -> Result<Catalog, CatalogError> {
        let mut entries = HashMap::new();
        for (which, text) in [
            ("the vendored catalog", vendored),
            ("the rebuild's file", rebuild),
        ] {
            let value: serde_json::Value = serde_json::from_str(text)
                .map_err(|source| CatalogError::Json { which, source })?;
            let serde_json::Value::Object(map) = value else {
                return Err(CatalogError::NotAnObject { which });
            };
            for (key, value) in map {
                if let serde_json::Value::String(message) = value {
                    // The first file to name a key keeps it.
                    entries
                        .entry(key.into_boxed_str())
                        .or_insert(message.into_boxed_str());
                }
            }
        }
        Ok(Catalog { entries })
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn get(&self, key: &str) -> Option<&str> {
        self.entries.get(key).map(|message| &**message)
    }
}

/// The values `t()` puts into `{name}` placeholders, in the order they are applied.
pub type Vars<'a> = &'a [(&'a str, &'a dyn Display)];

/// What the missing-key warning goes to; shared so it can run after the state's borrow ends.
type MissingKeySink = Rc<dyn Fn(&str)>;

struct State {
    shown: LocaleId,
    /// Every installed language, English included.
    installed: HashMap<LocaleId, Catalog>,
    warned: HashSet<String>,
    sink: Option<MissingKeySink>,
}

impl State {
    fn new() -> State {
        // `build.rs` read these same bytes and refused anything but a JSON object of strings,
        // so the parse cannot fail; an empty catalog would only make every key show itself.
        let english =
            Catalog::parse(ENGLISH_VENDORED, rebuild_text(LocaleId::ENGLISH)).unwrap_or_default();
        State {
            shown: LocaleId::ENGLISH,
            installed: HashMap::from([(LocaleId::ENGLISH, english)]),
            warned: HashSet::new(),
            sink: None,
        }
    }

    /// tC4's lookup: the shown language's entry if it is a non-empty string, else English's
    /// as it is, even when empty.
    fn resolve(&self, key: &str) -> Option<String> {
        let own = self
            .installed
            .get(&self.shown)
            .and_then(|catalog| catalog.get(key))
            .filter(|message| !message.is_empty());
        own.or_else(|| {
            self.installed
                .get(&LocaleId::ENGLISH)
                .and_then(|catalog| catalog.get(key))
        })
        .map(str::to_owned)
    }
}

thread_local! {
    static STATE: RefCell<State> = RefCell::new(State::new());
}

/// Makes `catalog` the messages of `locale`. Boot installs the nine that it fetched; a call
/// for a language already installed replaces it.
pub fn install_catalog(locale: LocaleId, catalog: Catalog) {
    STATE.with(|state| {
        state.borrow_mut().installed.insert(locale, catalog);
    });
}

/// tC4's `t(key, vars)` in the shown language: the entry, else English's, else (after one
/// warning per key) the key itself, with each `{name}` of `vars` replaced in the order given.
pub fn t(key: Key, vars: Vars<'_>) -> String {
    lookup(key.as_str(), vars, None)
}

/// tC4's `t(key, vars, fallback)` for a key that is not a constant: one a `tc-api` type names
/// (`NetKind::ask_key`, `Message::key`) or one made at run time. A key that no catalog has gives
/// `fallback` as it is, with no warning and no substitution, or the key after one warning when
/// there is none.
pub fn t_dyn(key: &str, vars: Vars<'_>, fallback: Option<&str>) -> String {
    lookup(key, vars, fallback)
}

fn lookup(key: &str, vars: Vars<'_>, fallback: Option<&str>) -> String {
    reactive::track();
    // The borrow ends before the sink runs: a sink may call `t` itself.
    let (found, warning) = STATE.with(|state| {
        let mut state = state.borrow_mut();
        if let Some(message) = state.resolve(key) {
            return (Some(message), None);
        }
        if fallback.is_some() {
            return (None, None);
        }
        let first = state.warned.insert(key.to_owned());
        (None, first.then(|| state.sink.clone()))
    });
    if let Some(sink) = warning {
        let message = format!("[i18n] missing key: {key}");
        match sink {
            Some(sink) => sink(&message),
            None => warn_in_console(&message),
        }
    }
    let message = match (found, fallback) {
        (Some(message), _) => message,
        (None, Some(fallback)) => return fallback.to_owned(),
        (None, None) => key.to_owned(),
    };
    tc_api::model::fill(&message, vars)
}

#[cfg(target_arch = "wasm32")]
fn warn_in_console(message: &str) {
    web_sys::console::warn_1(&wasm_bindgen::JsValue::from_str(message));
}

#[cfg(not(target_arch = "wasm32"))]
fn warn_in_console(_message: &str) {}

/// Where `[i18n] missing key: <key>` goes: the console in a browser by default, nowhere on the
/// host until a test sets a sink.
pub fn set_missing_key_sink(sink: impl Fn(&str) + 'static) {
    STATE.with(|state| state.borrow_mut().sink = Some(Rc::new(sink)));
}

/// The shown language. A view that reads it renders again when it changes.
pub fn shown_locale() -> LocaleId {
    reactive::track();
    STATE.with(|state| state.borrow().shown)
}

/// tC4's `setLocale`: shows `locale` and returns true. A language that is not installed, or
/// the one already shown, changes nothing and returns false. With the `app` feature in a
/// browser, `<html lang>` and `<html dir>` change first and then the views that read [`t`]
/// render again, so nothing paints a new text in the old direction.
pub fn show_locale(locale: LocaleId) -> bool {
    let changed = STATE.with(|state| {
        let mut state = state.borrow_mut();
        if state.shown == locale || !state.installed.contains_key(&locale) {
            return false;
        }
        state.shown = locale;
        true
    });
    if changed {
        reactive::announce(locale);
    }
    changed
}

/// The tracking of the shown language and the attributes of `<html>`: browser and `app` only.
#[cfg(all(target_arch = "wasm32", feature = "app"))]
mod reactive {
    use leptos::prelude::*;
    use tc_api::ids::LocaleId;

    thread_local! {
        static SHOWN: ArcRwSignal<LocaleId> = ArcRwSignal::new(LocaleId::ENGLISH);
    }

    pub(super) fn track() {
        SHOWN.with(|shown| shown.track());
    }

    pub(super) fn announce(locale: LocaleId) {
        let (lang, dir) = super::html_lang_dir(locale);
        if let Some(root) = document().document_element() {
            // A refused attribute leaves the old direction; the views still change language.
            let _ = root.set_attribute("lang", lang);
            let _ = root.set_attribute("dir", dir);
        }
        SHOWN.with(|shown| shown.set(locale));
    }
}

#[cfg(not(all(target_arch = "wasm32", feature = "app")))]
mod reactive {
    use tc_api::ids::LocaleId;

    pub(super) fn track() {}

    pub(super) fn announce(_locale: LocaleId) {}
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn placeholders_are_named_braces_sorted_with_repeats() {
        assert_eq!(
            placeholders("{n} of {total} · {n}"),
            vec!["{n}", "{n}", "{total}"]
        );
        assert_eq!(placeholders("{ n } {} {a-b} {x"), Vec::<&str>::new());
        assert_eq!(placeholders("{{span}}"), vec!["{span}"]);
    }

    #[test]
    fn a_locale_is_found_by_its_exact_id() {
        assert_eq!(locale("ar").map(|l| l.dir), Some(TextDir::Rtl));
        for bad in ["es", "de", "", "EN", "hi-IN", "__proto__", "toString"] {
            assert!(locale(bad).is_none(), "{bad}");
        }
        assert!(locale(DEFAULT_LOCALE).is_some());
    }
}
