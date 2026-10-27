//! `t()` and the shown language, against the catalogs the app ships. tC4's own checks of `t()`
//! (`test/i18n-catalogs.test.ts`, `test/i18n-keys.test.ts` at d310bf3) are ported here, with
//! the cases the rebuild adds: the rebuild's key files, and the keys as constants.
//!
//! The language state is per thread and each test runs on its own thread, so a test starts in
//! English with only English installed.
#![cfg(not(target_arch = "wasm32"))]
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    reason = "test code: a failed unwrap is a failed test"
)]

use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;

use serde_json::{Map, Value};
use tc_api::ids::{LOCALE_IDS, LocaleId};
use tc_ui::i18n::{
    Catalog, CatalogError, ENGLISH_VENDORED, LOCALES, REBUILD, TextDir, html_lang_dir,
    install_catalog, keys, locale_of, set_missing_key_sink, show_locale, shown_locale, t, t_dyn,
};

fn assets() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("assets")
}

fn read(path: PathBuf) -> String {
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("cannot read {}: {e}", path.display()))
}

fn vendored_text(id: &str) -> String {
    read(assets().join(format!("i18n/{id}.json")))
}

fn rebuild_text(id: &str) -> String {
    read(assets().join(format!("i18n-rebuild/{id}.json")))
}

fn locale(id: &str) -> LocaleId {
    LocaleId::parse(id).unwrap()
}

/// A language's catalog as the app builds it from the files on disk.
fn catalog(id: &str) -> Catalog {
    Catalog::parse(&vendored_text(id), &rebuild_text(id)).unwrap()
}

/// The shipped catalog of `id` after `edit` changed its vendored entries.
fn edited(id: &str, edit: impl FnOnce(&mut Map<String, Value>)) -> Catalog {
    let Value::Object(mut map) = serde_json::from_str(&vendored_text(id)).unwrap() else {
        panic!("{id}.json is not an object");
    };
    edit(&mut map);
    Catalog::parse(&Value::Object(map).to_string(), &rebuild_text(id)).unwrap()
}

/// Installs every language, as boot does.
fn install_all() {
    for id in LocaleId::all().filter(|id| *id != LocaleId::ENGLISH) {
        install_catalog(id, catalog(id.as_str()));
    }
}

/// Collects what the missing-key sink is given.
fn collect_warnings() -> Rc<RefCell<Vec<String>>> {
    let log = Rc::new(RefCell::new(Vec::new()));
    let sink = Rc::clone(&log);
    set_missing_key_sink(move |message| sink.borrow_mut().push(message.to_owned()));
    log
}

#[test]
fn t_reads_the_shown_catalog_and_a_selection_shows_at_once() {
    install_all();
    let english = t(keys::account::LANGUAGE, &[]);
    assert_eq!(english, "App language");
    assert!(show_locale(locale("fr")));
    let french = t(keys::account::LANGUAGE, &[]);
    assert_eq!(french, "Langue de l’application");
    assert_ne!(french, english);
    assert!(show_locale(locale("hi")));
    assert_eq!(t(keys::account::LANGUAGE, &[]), "ऐप की भाषा");
    assert_eq!(shown_locale(), locale("hi"));
}

#[test]
fn t_falls_back_to_english_per_key_when_a_target_entry_is_absent_or_empty() {
    let es = locale("es-419");
    install_catalog(
        es,
        edited("es-419", |map| {
            map.remove("common.close");
        }),
    );
    assert!(show_locale(es));
    assert_eq!(t(keys::common::CLOSE, &[]), "Close");
    // Another key of the same catalog stays Spanish.
    assert_eq!(t(keys::app::SAVED, &[]), "Guardado");

    install_catalog(
        es,
        edited("es-419", |map| {
            map.insert("common.close".into(), Value::String(String::new()));
        }),
    );
    assert_eq!(t(keys::common::CLOSE, &[]), "Close");
    assert_eq!(t(keys::app::SAVED, &[]), "Guardado");

    // An entry that is not a string reads as absent too.
    install_catalog(
        es,
        edited("es-419", |map| {
            map.insert("common.close".into(), Value::from(7));
        }),
    );
    assert_eq!(t(keys::common::CLOSE, &[]), "Close");
}

#[test]
fn t_warns_once_per_missing_key_and_shows_the_key() {
    let warnings = collect_warnings();
    assert_eq!(
        t_dyn("no.such.key.for.issue12", &[], None),
        "no.such.key.for.issue12"
    );
    assert_eq!(
        t_dyn("no.such.key.for.issue12", &[], None),
        "no.such.key.for.issue12"
    );
    assert_eq!(
        *warnings.borrow(),
        vec!["[i18n] missing key: no.such.key.for.issue12".to_owned()],
        "once per key, not once per call"
    );
    // A present key stays silent.
    assert_eq!(t(keys::app::NAME, &[]), "translationCore4");
    assert_eq!(warnings.borrow().len(), 1);
    // Another missing key warns for itself.
    t_dyn("no.such.other.key", &[], None);
    assert_eq!(warnings.borrow().len(), 2);
}

#[test]
fn a_sink_may_read_messages_itself() {
    // The sink runs after the state's borrow is released, or this call would panic.
    set_missing_key_sink(|_| {
        assert_eq!(t(keys::app::SAVED, &[]), "Saved");
    });
    assert_eq!(t_dyn("no.such.key", &[], None), "no.such.key");
}

#[test]
fn an_explicit_fallback_wins_only_over_a_missing_key_and_is_not_filled() {
    let warnings = collect_warnings();
    let n = 3;
    assert_eq!(
        t_dyn("no.such.key", &[("n", &n)], Some("Fallback {n}")),
        "Fallback {n}"
    );
    assert!(warnings.borrow().is_empty(), "a fallback is not a warning");
    // A key a catalog has ignores the fallback.
    assert_eq!(
        t_dyn("app.saved", &[], Some("Fallback")),
        "Saved",
        "the catalog's text wins when it has the key"
    );
}

#[test]
fn vars_replace_every_occurrence_in_the_order_given() {
    // Not a catalog key: a message with `{n}` twice is made from a fallback-free dynamic key
    // that does exist in English.
    let en = Catalog::parse(r#"{"x.twice":"{n} of {n}","x.name":"Hi {name}"}"#, "{}").unwrap();
    install_catalog(LocaleId::ENGLISH, en);
    let n = 2;
    assert_eq!(t_dyn("x.twice", &[("n", &n)], None), "2 of 2");
    // tC4 applies the variables one after another: a value that holds a later variable's
    // `{name}` is replaced again.
    assert_eq!(
        t_dyn("x.name", &[("name", &"{n}"), ("n", &"3")], None),
        "Hi 3"
    );
    assert_eq!(
        t_dyn("x.name", &[("n", &"3"), ("name", &"{n}")], None),
        "Hi {n}"
    );
    // A value is put in as it is: `$&` and `$$` are not patterns here.
    assert_eq!(t_dyn("x.name", &[("name", &"$&$$")], None), "Hi $&$$");
    // A variable the message does not name changes nothing.
    assert_eq!(t_dyn("x.name", &[("other", &1)], None), "Hi {name}");
}

#[test]
fn a_rebuild_key_reads_in_english_until_it_is_translated() {
    let ar = locale("ar");
    install_catalog(ar, Catalog::parse(&vendored_text("ar"), "{}").unwrap());
    assert!(show_locale(ar));
    let reason = "disk full";
    assert_eq!(
        t(keys::upload::NOT_WRITTEN, &[("reason", &reason)]),
        "The file could not be copied into translationCore: disk full. Nothing was changed."
    );
}

#[test]
fn a_translated_rebuild_key_reads_in_its_language() {
    let fr = locale("fr");
    install_catalog(
        fr,
        Catalog::parse(
            &vendored_text("fr"),
            r#"{"upload.notWritten":"Copie impossible : {reason}"}"#,
        )
        .unwrap(),
    );
    assert!(show_locale(fr));
    let reason = "disque plein";
    assert_eq!(
        t(keys::upload::NOT_WRITTEN, &[("reason", &reason)]),
        "Copie impossible : disque plein"
    );
}

#[test]
fn a_rebuild_key_never_overrides_a_vendored_key() {
    let merged = Catalog::parse(&vendored_text("fr"), r#"{"common.close":"X"}"#).unwrap();
    assert_eq!(merged.get("common.close"), Some("Fermer"));
    let fr = locale("fr");
    install_catalog(fr, merged);
    assert!(show_locale(fr));
    assert_eq!(t(keys::common::CLOSE, &[]), "Fermer");
}

#[test]
fn showing_the_shown_locale_again_changes_nothing() {
    install_all();
    assert!(!show_locale(LocaleId::ENGLISH), "English is shown at first");
    assert!(show_locale(locale("fr")));
    assert!(!show_locale(locale("fr")));
    assert_eq!(shown_locale(), locale("fr"));
}

#[test]
fn a_language_that_is_not_installed_is_not_shown() {
    // Only English is installed in a fresh state.
    assert!(!show_locale(locale("ar")));
    assert_eq!(shown_locale(), LocaleId::ENGLISH);
    assert_eq!(t(keys::app::SAVED, &[]), "Saved");
}

#[test]
fn every_ui_locale_is_an_api_locale_in_the_same_order() {
    let ui: Vec<&str> = LOCALES.iter().map(|l| l.id).collect();
    assert_eq!(ui, LOCALE_IDS);
    for (index, id) in LocaleId::all().enumerate() {
        let found = locale_of(id);
        assert_eq!(found.id, id.as_str());
        assert_eq!(LOCALES.get(index).map(|l| l.id), Some(found.id));
    }
}

#[test]
fn lang_and_dir_follow_the_locale() {
    for id in LocaleId::all() {
        let (lang, dir) = html_lang_dir(id);
        assert_eq!(lang, id.as_str());
        assert_eq!(dir, if id.as_str() == "ar" { "rtl" } else { "ltr" });
        assert_eq!(
            locale_of(id).dir,
            if dir == "rtl" {
                TextDir::Rtl
            } else {
                TextDir::Ltr
            }
        );
    }
}

#[test]
fn every_catalog_parses_with_every_english_key() {
    let english: Value = serde_json::from_str(ENGLISH_VENDORED).unwrap();
    let Value::Object(english) = english else {
        panic!("en.json is not an object");
    };
    assert!(english.len() >= 1_029);
    for id in LocaleId::all() {
        let parsed = catalog(id.as_str());
        assert!(parsed.len() >= 1_029, "{id:?} has {} entries", parsed.len());
        for key in english.keys() {
            assert!(parsed.get(key).is_some(), "{} lacks {key}", id.as_str());
        }
    }
}

#[test]
fn the_compiled_rebuild_files_are_the_files_on_disk() {
    assert_eq!(ENGLISH_VENDORED, vendored_text("en"));
    assert_eq!(REBUILD.len(), LOCALES.len());
    for ((id, compiled), locale) in REBUILD.iter().zip(LOCALES) {
        assert_eq!(*id, locale.id, "the table follows the locale order");
        assert_eq!(*compiled, rebuild_text(id), "{id}");
    }
}

#[test]
fn a_catalog_that_is_not_an_object_is_refused() {
    for text in ["[]", "\"x\"", "3", "null"] {
        assert!(
            matches!(
                Catalog::parse(text, "{}"),
                Err(CatalogError::NotAnObject { .. })
            ),
            "{text}"
        );
        assert!(
            matches!(
                Catalog::parse("{}", text),
                Err(CatalogError::NotAnObject { .. })
            ),
            "{text}"
        );
    }
    assert!(matches!(
        Catalog::parse("{", "{}"),
        Err(CatalogError::Json { .. })
    ));
}

#[test]
fn a_key_constant_and_its_text_give_the_same_message() {
    assert_eq!(
        t(keys::net::ask::SIGN_IN, &[]),
        t_dyn("net.ask.signIn", &[], None)
    );
    assert_eq!(keys::net::ask::SIGN_IN.as_str(), "net.ask.signIn");
    assert_eq!(
        keys::refusal::open::LOCK_HELD.as_str(),
        "refusal.open.lock-held"
    );
}

/// #833.3: every refusal of an import or of Update a book reads as a sentence that names the
/// file, never as tC4's code or the catalog key in its place.
#[test]
fn an_import_refusal_shows_a_sentence_not_a_code() {
    use tc_api::refusal::{ImportRefusal as R, Message, Refusal};
    show_locale(LocaleId::ENGLISH);
    let file = "cfm_fbt_tit_book.zip";
    let refusals = [
        R::WriteFailed,
        R::NameExists,
        R::NotZip { file: file.into() },
        R::CorruptEntry {
            file: file.into(),
            entry: "manifest.json".into(),
        },
        R::NoManifest { file: file.into() },
        R::NoText { file: file.into() },
        R::Languages {
            languages: vec!["cfm".into(), "en".into()],
        },
        R::Twice { code: "TIT".into() },
        R::Unreadable {
            file: file.into(),
            problem: "chapter 2".into(),
        },
        R::TextChanged { book: "TIT".into() },
        R::UpdateUnreadable {
            file: file.into(),
            line: 12,
            problem: "a verse marker with no number".into(),
        },
        R::UpdateOtherBook {
            file: file.into(),
            named: "JUD".into(),
            chosen: "TIT".into(),
        },
        R::UpdateNotWritten {
            reason: "the disk is full".into(),
        },
        R::UpdateReviewNotWritten {
            reason: "the disk is full".into(),
        },
    ];
    for refusal in refusals {
        let names_file = refusal.vars().iter().any(|(k, _)| *k == "file");
        let refusal = Refusal::Import(refusal);
        let text = tc_ui::i18n::refusal_text(&refusal);
        assert_ne!(Some(text.as_str()), refusal.code(), "{refusal:?}");
        assert_ne!(text, refusal.key(), "{refusal:?}");
        assert!(
            text.contains(' ') && !text.contains('{'),
            "not a sentence: {refusal:?}: {text:?}"
        );
        if names_file {
            assert!(text.contains(file), "{refusal:?}: {text:?}");
        }
    }
}

proptest::proptest! {
    #![proptest_config(proptest::prelude::ProptestConfig::with_cases(256))]

    /// INV-EVERY-REFUSAL-RENDERS (UI-3): every refusal reads as a whole sentence in every
    /// shipped language, with no `{placeholder}` left, whatever its values.
    #[test]
    fn every_refusal_renders_with_no_placeholder_left(refusal in proptest::prelude::any::<tc_api::refusal::Refusal>()) {
        use tc_api::refusal::Message;
        proptest::prop_assume!(refusal.vars().iter().all(|(_, value)| !value.contains('{')));
        install_all();
        for id in LOCALE_IDS {
            show_locale(locale(id));
            let text = tc_ui::i18n::refusal_text(&refusal);
            proptest::prop_assert!(
                !text.contains('{'),
                "INV-EVERY-REFUSAL-RENDERS: {} in {id}: {text:?}",
                refusal.key()
            );
        }
    }
}
