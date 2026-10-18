//! The first-launch window, `app/prepare.html` (APP-DESIGN §5.3, unit A9): tC4's preparation
//! page, `scripts/desktop-preparing.html`, with its words in the ten interface languages and
//! one script of its own. The page is one self-contained file, so these tests keep it equal to
//! what it is built from: the rebuild text files and tC4's markup.
//!
//! After a change to the script or to a `prepare.*` key, run
//! `UPDATE_PREPARE=1 cargo test -p tc-ui --test prepare_page`, which rewrites the page's text
//! block and the hash its policy allows its script by, then run the tests without it.
#![cfg(not(target_arch = "wasm32"))]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Once;

use base64::Engine as _;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use tc_ui::i18n::{LOCALES, placeholders};

const TEXT_OPEN: &str = r#"<script type="application/json" id="tc4-text">"#;
const SCRIPT_CLOSE: &str = "</script>";
const HASH_OPEN: &str = "script-src 'sha256-";

fn crate_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn page_path() -> PathBuf {
    crate_dir().join("app/prepare.html")
}

/// The `prepare.*` keys of a rebuild file.
fn prepare_keys(file: &str) -> BTreeMap<String, String> {
    let path = crate_dir().join("assets/i18n-rebuild").join(file);
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    let all: BTreeMap<String, String> = serde_json::from_str(&text).unwrap();
    all.into_iter()
        .filter(|(key, _)| key.starts_with("prepare."))
        .collect()
}

/// What the page's text block holds: for each language its direction and the `prepare.*` text
/// it has translated (English: all of it; the others are pending, Q-10), keys in order, and no
/// `<` so that nothing in it can close the block.
fn expected_text() -> String {
    let mut locales = serde_json::Map::new();
    let mut ids: Vec<_> = LOCALES.iter().collect();
    ids.sort_by_key(|locale| locale.id);
    for locale in ids {
        let file = format!("{}.json", locale.id);
        let text: serde_json::Map<String, Value> = prepare_keys(&file)
            .into_iter()
            .map(|(key, text)| (key, Value::String(text)))
            .collect();
        locales.insert(
            locale.id.to_owned(),
            json!({"dir": locale.dir.as_str(), "text": text}),
        );
    }
    serde_json::to_string(&json!({"locales": locales}))
        .unwrap()
        .replace('<', "\\u003c")
}

/// The text of the page's one executable script, between `<script>` and `</script>`.
fn inline_script(html: &str) -> &str {
    let start = html.rfind("<script>").expect("an executable script") + "<script>".len();
    let end = html[start..].find(SCRIPT_CLOSE).expect("its end") + start;
    &html[start..end]
}

/// The source a policy allows that script by.
fn script_hash(script: &str) -> String {
    let digest = Sha256::digest(script.as_bytes());
    base64::engine::general_purpose::STANDARD.encode(digest)
}

/// The page, after `UPDATE_PREPARE=1` rewrote what is built from other files.
fn page() -> String {
    static UPDATE: Once = Once::new();
    UPDATE.call_once(|| {
        if std::env::var_os("UPDATE_PREPARE").is_none() {
            return;
        }
        let mut html = std::fs::read_to_string(page_path()).unwrap();
        let text_start = html.find(TEXT_OPEN).unwrap() + TEXT_OPEN.len();
        let text_end = html[text_start..].find(SCRIPT_CLOSE).unwrap() + text_start;
        html.replace_range(text_start..text_end, &expected_text());
        let hash = script_hash(inline_script(&html));
        let hash_start = html.find(HASH_OPEN).unwrap() + HASH_OPEN.len();
        let hash_end = html[hash_start..].find('\'').unwrap() + hash_start;
        html.replace_range(hash_start..hash_end, &hash);
        std::fs::write(page_path(), html).unwrap();
    });
    std::fs::read_to_string(page_path()).unwrap()
}

/// The text between two markers, which must each occur once.
fn between<'a>(html: &'a str, open: &str, close: &str) -> &'a str {
    assert_eq!(html.matches(open).count(), 1, "{open}");
    let start = html.find(open).unwrap() + open.len();
    let end = html[start..]
        .find(close)
        .unwrap_or_else(|| panic!("{close}"))
        + start;
    &html[start..end]
}

#[test]
fn the_page_holds_each_languages_text_from_the_rebuild_files() {
    let html = page();
    let block = between(&html, TEXT_OPEN, SCRIPT_CLOSE);
    assert_eq!(
        block,
        expected_text(),
        "run UPDATE_PREPARE=1 cargo test -p tc-ui --test prepare_page"
    );

    let english = prepare_keys("en.json");
    assert_eq!(english.len(), 7, "{english:?}");
    for (key, text) in &english {
        assert!(!text.is_empty(), "{key}");
    }
    assert_eq!(
        placeholders(&english["prepare.progress"]),
        ["{done}", "{total}"],
        "the count is the page's only substitution"
    );
    // Every language is listed, with its direction, even while it has no text of its own.
    let parsed: Value = serde_json::from_str(block).unwrap();
    for locale in &LOCALES {
        assert_eq!(
            parsed["locales"][locale.id]["dir"],
            locale.dir.as_str(),
            "{}",
            locale.id
        );
    }
    assert_eq!(parsed["locales"]["ar"]["dir"], "rtl");
    assert!(!block.contains('<'), "nothing in the block can close it");
}

#[test]
fn the_page_runs_only_its_own_script() {
    let html = page();
    let policy = between(
        &html,
        "http-equiv=\"Content-Security-Policy\" content=\"",
        "\"",
    );
    let script = inline_script(&html);

    let directives: BTreeMap<&str, &str> = policy
        .split(';')
        .map(str::trim)
        .filter_map(|directive| directive.split_once(' '))
        .collect();
    assert_eq!(
        directives["script-src"],
        format!("'sha256-{}'", script_hash(script)),
        "the policy allows exactly the script that is on the page; \
         run UPDATE_PREPARE=1 cargo test -p tc-ui --test prepare_page"
    );
    assert_eq!(directives["default-src"], "'none'");
    assert_eq!(
        directives["connect-src"], "'self'",
        "it asks the server it came from"
    );
    assert_eq!(directives["style-src"], "'unsafe-inline'");
    for forbidden in ["unsafe-eval", "http:", "https:", "data:", "blob:"] {
        assert!(!policy.contains(forbidden), "{forbidden}");
    }

    // One data block, which does not run, and one script with no source and no attributes.
    assert_eq!(html.matches("<script").count(), 2);
    assert_eq!(html.matches("<script>").count(), 1);
    assert_eq!(html.matches(TEXT_OPEN).count(), 1);
    for attribute in [" src=", "onclick=", "onload=", "onerror=", "javascript:"] {
        assert!(!html.contains(attribute), "{attribute}");
    }
}

#[test]
fn the_page_keeps_tc4s_words_and_markup() {
    let html = page();
    let english = prepare_keys("en.json");

    // `scripts/desktop-preparing.html:2-4` at tC4 d310bf3.
    assert!(html.contains("<title>translationCore4</title>"));
    assert_eq!(
        between(&html, "<style>", "</style>"),
        "body{margin:36px;font:16px system-ui,sans-serif;color:#293847;background:#f5f7fa}\
         h1{font-size:22px}p{line-height:1.5}#progress{color:#526173}"
    );
    assert!(html.contains(
        "<body><h1>Preparing translationCore4</h1>\
         <p>Getting resources ready for offline use.</p>\
         <p id=\"progress\" role=\"status\">Starting preparation…</p>"
    ));
    assert_eq!(english["prepare.title"], between(&html, "<h1>", "</h1>"));
    assert_eq!(
        english["prepare.lead"],
        between(&html, "<h1>", "</p>").split("<p>").nth(1).unwrap()
    );
    assert_eq!(
        english["prepare.starting"],
        between(&html, "<p id=\"progress\" role=\"status\">", "</p>")
    );

    // The words tC4 sets from its main process while it prepares:
    // `scripts/desktop-prepare.cjs:45` (the count), `:49` (the hand-off), and
    // `scripts/desktop-main.cjs:177-178` (the failure).
    assert_eq!(
        english["prepare.progress"],
        "Preparing resources: {done} of {total}"
    );
    assert_eq!(english["prepare.handoff"], "Starting…");
    assert_eq!(
        english["prepare.failedTitle"],
        "translationCore4 could not start"
    );
    assert_eq!(
        english["prepare.failed"],
        "The bundled resources could not be prepared. Please quit and try again."
    );
}
