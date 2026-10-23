//! A port of tC4's `test/i18n-keys.test.ts` (issue #12): every key the UI source names exists
//! in the catalog, with floors so the scan cannot pass vacuously.
//!
//! tC4 scans its source with regular expressions for three call shapes. The rebuild has the
//! same three, and the compiler covers the first:
//!
//! - `t(keys::app::SAVED, …)`: the key is a generated constant (`build.rs`), so a key that is
//!   not in the catalog does not compile. [`every_key_constant_names_a_catalog_key_and_every_key_has_one`]
//!   proves the generator saw both English files.
//! - `t_dyn("home.title.x", …)` and a role table's `name: "sources.roleNotes"` read later
//!   through `t_dyn(role.name, …)`: any string literal whose first segment is a catalog
//!   namespace must be a catalog key. An `t_dyn` literal is checked even when its namespace is
//!   misspelled.
//! - `format!("net.ask.{kind}")`: the literal becomes a pattern, which must match a key.
//!
//! The scan reads the source's tokens, not its text, so a quote in a comment, a character
//! literal or a raw string cannot pair with the wrong quote. It reads `crates/tc-ui/src`; the
//! generated file is in `OUT_DIR` and is not scanned.
#![cfg(not(target_arch = "wasm32"))]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use regex::Regex;
use tc_ui::i18n::keys;

/// What the scan finds in the source today. Each screen unit raises them to its new counts
/// and never lowers one without a reason in its commit message: a floor that falls hides a
/// scan that stopped seeing a call shape. tC4's own floors are 100 literal keys and 100
/// namespaced strings; they are the end state, once every screen is ported.
const KEY_CONSTANT_USES_FLOOR: usize = 82;
const NAMESPACED_FLOOR: usize = 0;
const TEMPLATE_FLOOR: usize = 0;

/// Strings in the source that begin with a catalog namespace but are not keys, each with its
/// reason. The scan fails on an entry it no longer finds, so this list cannot go stale.
const ALLOWED: &[(&str, &str)] = &[];

fn crate_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

/// The merged English catalog: tC4's `en.json` and the rebuild's.
fn catalog_keys() -> BTreeSet<String> {
    let mut keys = BTreeSet::new();
    for file in ["assets/i18n/en.json", "assets/i18n-rebuild/en.json"] {
        let text = std::fs::read_to_string(crate_dir().join(file)).unwrap();
        let serde_json::Value::Object(map) = serde_json::from_str(&text).unwrap() else {
            panic!("{file} is not an object");
        };
        keys.extend(map.into_iter().map(|(key, _)| key));
    }
    keys
}

/// A first segment of a catalog key.
fn namespaces(catalog: &BTreeSet<String>) -> BTreeSet<&str> {
    catalog
        .iter()
        .filter_map(|key| key.split('.').next())
        .collect()
}

#[derive(Debug, Clone, PartialEq)]
enum Token {
    /// The text between the quotes, as written (escapes are not resolved).
    Str(String),
    Ident(String),
    Punct(char),
}

/// The tokens of Rust source that matter to the scan. Comments, character literals and
/// lifetimes produce nothing; a string, a raw string or a byte string is one `Str`.
fn tokens(src: &str) -> Vec<Token> {
    let chars: Vec<char> = src.chars().collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        let next = chars.get(i + 1).copied();
        if c == '/' && next == Some('/') {
            while i < chars.len() && chars[i] != '\n' {
                i += 1;
            }
        } else if c == '/' && next == Some('*') {
            let mut depth = 0;
            while i < chars.len() {
                if chars[i] == '/' && chars.get(i + 1) == Some(&'*') {
                    depth += 1;
                    i += 2;
                } else if chars[i] == '*' && chars.get(i + 1) == Some(&'/') {
                    depth -= 1;
                    i += 2;
                    if depth == 0 {
                        break;
                    }
                } else {
                    i += 1;
                }
            }
        } else if c == '"' {
            let (text, end) = string_from(&chars, i + 1);
            out.push(Token::Str(text));
            i = end;
        } else if c == '\'' {
            // `'x'` and `'\n'` are characters; `'a` in `&'a str` is a lifetime.
            if next == Some('\\') {
                // Skip the backslash and the character it escapes, which may be a quote.
                i += 3;
                while i < chars.len() && chars[i] != '\'' {
                    i += 1;
                }
                i += 1;
            } else if chars.get(i + 2) == Some(&'\'') {
                i += 3;
            } else {
                i += 1;
            }
        } else if c.is_alphanumeric() || c == '_' {
            let start = i;
            while i < chars.len() && (chars[i].is_alphanumeric() || chars[i] == '_') {
                i += 1;
            }
            let word: String = chars[start..i].iter().collect();
            let raw_prefix = word == "r" || word == "br";
            let raw_start = matches!(chars.get(i), Some('"' | '#'));
            if raw_prefix
                && raw_start
                && let Some((text, end)) = raw_string_from(&chars, i)
            {
                out.push(Token::Str(text));
                i = end;
            } else {
                out.push(Token::Ident(word));
            }
        } else if c.is_whitespace() {
            i += 1;
        } else {
            out.push(Token::Punct(c));
            i += 1;
        }
    }
    out
}

/// The body of a string whose opening quote is before `start`, and the index after its
/// closing quote.
fn string_from(chars: &[char], start: usize) -> (String, usize) {
    let mut i = start;
    let mut text = String::new();
    while i < chars.len() && chars[i] != '"' {
        if chars[i] == '\\' {
            text.push(chars[i]);
            i += 1;
        }
        if let Some(c) = chars.get(i) {
            text.push(*c);
        }
        i += 1;
    }
    (text, i + 1)
}

/// A raw string whose `#`s or opening quote begin at `start`, or `None` for `r#ident`.
fn raw_string_from(chars: &[char], start: usize) -> Option<(String, usize)> {
    let mut i = start;
    let mut hashes = 0;
    while chars.get(i) == Some(&'#') {
        hashes += 1;
        i += 1;
    }
    if chars.get(i) != Some(&'"') {
        return None;
    }
    i += 1;
    let body = i;
    while i < chars.len() {
        if chars[i] == '"' && (0..hashes).all(|k| chars.get(i + 1 + k) == Some(&'#')) {
            return Some((chars[body..i].iter().collect(), i + 1 + hashes));
        }
        i += 1;
    }
    None
}

/// A string that could be a key: dotted segments of letters, digits, `_` and `-`.
fn key_shaped() -> Regex {
    Regex::new(r"^[A-Za-z][\w-]*(?:\.[\w-]+)+$").unwrap()
}

/// Every string literal that could be a key and starts with a catalog namespace.
fn namespaced_strings(src: &str, namespaces: &BTreeSet<&str>) -> Vec<String> {
    let shape = key_shaped();
    tokens(src)
        .into_iter()
        .filter_map(|token| match token {
            Token::Str(text)
                if shape.is_match(&text)
                    && text
                        .split('.')
                        .next()
                        .is_some_and(|first| namespaces.contains(first)) =>
            {
                Some(text)
            }
            _ => None,
        })
        .collect()
}

/// The literal first argument of each `t_dyn(` call.
fn t_dyn_literals(src: &str) -> Vec<String> {
    let all = tokens(src);
    all.windows(3)
        .filter_map(|w| match w {
            [Token::Ident(call), Token::Punct('('), Token::Str(text)] if call == "t_dyn" => {
                Some(text.clone())
            }
            _ => None,
        })
        .collect()
}

/// The literal of each `format!(` that makes a key family: with each `{…}` hole filled by a
/// word, it is a dotted key. `"{name}. {title}"` and `"spec.json: {e}"` are text, not keys.
fn template_families(src: &str) -> Vec<String> {
    let shape = key_shaped();
    let hole = Regex::new(r"\{[^}]*\}").unwrap();
    let all = tokens(src);
    all.windows(4)
        .filter_map(|w| match w {
            [
                Token::Ident(call),
                Token::Punct('!'),
                Token::Punct('('),
                Token::Str(text),
            ] if call == "format"
                && text.contains('{')
                && shape.is_match(&hole.replace_all(text, "x")) =>
            {
                Some(text.clone())
            }
            _ => None,
        })
        .collect()
}

/// The paths `keys::a::b::NAME` the source names.
fn key_constant_uses(src: &str) -> Vec<String> {
    let all = tokens(src);
    let mut found = Vec::new();
    let mut i = 0;
    while i < all.len() {
        if all[i] == Token::Ident("keys".into()) {
            let mut path = vec!["keys".to_owned()];
            let mut j = i + 1;
            while let (Some(Token::Punct(':')), Some(Token::Punct(':')), Some(Token::Ident(seg))) =
                (all.get(j), all.get(j + 1), all.get(j + 2))
            {
                path.push(seg.clone());
                j += 3;
            }
            let ends_in_constant = path.len() > 1
                && path.last().is_some_and(|last| {
                    last.chars()
                        .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_')
                });
            if ends_in_constant {
                found.push(path.join("::"));
            }
            i = j.max(i + 1);
        } else {
            i += 1;
        }
    }
    found
}

/// A template's `{…}` holes as a pattern that matches one segment's text.
fn family_pattern(template: &str) -> Regex {
    let hole = Regex::new(r"\{[^}]*\}").unwrap();
    let pattern: String = hole
        .split(template)
        .map(regex::escape)
        .collect::<Vec<_>>()
        .join(r"[\w-]+");
    Regex::new(&format!("^{pattern}$")).unwrap()
}

fn source_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let mut entries: Vec<_> = std::fs::read_dir(dir)
        .unwrap()
        .map(|e| e.unwrap().path())
        .collect();
    entries.sort();
    for path in entries {
        if path.is_dir() {
            source_files(&path, out);
        } else if path.extension().is_some_and(|ext| ext == "rs") {
            out.push(path);
        }
    }
}

/// Every source file of the crate with its text.
fn sources() -> Vec<(String, String)> {
    let root = crate_dir().join("src");
    let mut files = Vec::new();
    source_files(&root, &mut files);
    assert!(!files.is_empty(), "no source under {}", root.display());
    files
        .into_iter()
        .map(|path| {
            let text = std::fs::read_to_string(&path).unwrap();
            let name = path.strip_prefix(&root).unwrap().display().to_string();
            (name, text)
        })
        .collect()
}

/// (file, string) for each scanned string over all sources.
fn scan(find: impl Fn(&str) -> Vec<String>) -> Vec<(String, String)> {
    sources()
        .into_iter()
        .flat_map(|(file, text)| {
            find(&text)
                .into_iter()
                .map(move |found| (file.clone(), found))
        })
        .collect()
}

#[test]
fn every_key_constant_names_a_catalog_key_and_every_key_has_one() {
    let generated: BTreeSet<String> = keys::ALL.iter().map(|k| k.as_str().to_owned()).collect();
    let catalog = catalog_keys();
    assert_eq!(generated.len(), keys::ALL.len(), "ALL holds no key twice");
    assert_eq!(
        generated.difference(&catalog).collect::<Vec<_>>(),
        Vec::<&String>::new(),
        "constants for keys no catalog has"
    );
    assert_eq!(
        catalog.difference(&generated).collect::<Vec<_>>(),
        Vec::<&String>::new(),
        "catalog keys with no constant"
    );
    assert!(catalog.len() >= 1_045);
    // ALL is in key order.
    assert!(keys::ALL.windows(2).all(|w| w[0] < w[1]));
}

#[test]
fn every_catalog_namespaced_string_in_the_ui_source_is_a_catalog_key() {
    let catalog = catalog_keys();
    let namespaces = namespaces(&catalog);
    let found = scan(|src| namespaced_strings(src, &namespaces));
    let missing: Vec<_> = found
        .iter()
        .filter(|(_, key)| !catalog.contains(key) && !ALLOWED.iter().any(|(a, _)| a == key))
        .collect();
    assert!(
        missing.is_empty(),
        "strings in a catalog namespace that are not catalog keys (a typo, or an entry for \
         ALLOWED with its reason): {missing:?}"
    );
    for (allowed, reason) in ALLOWED {
        assert!(!reason.is_empty());
        assert!(
            found.iter().any(|(_, key)| key == allowed),
            "ALLOWED names {allowed}, which the source no longer holds"
        );
    }
}

#[test]
fn every_key_literal_in_a_t_dyn_call_is_a_catalog_key() {
    let catalog = catalog_keys();
    let missing: Vec<_> = scan(t_dyn_literals)
        .into_iter()
        .filter(|(_, key)| !catalog.contains(key))
        .collect();
    assert!(
        missing.is_empty(),
        "t_dyn names keys no catalog has: {missing:?}"
    );
}

#[test]
fn every_template_key_family_matches_a_catalog_key() {
    let catalog = catalog_keys();
    let unmatched: Vec<_> = scan(template_families)
        .into_iter()
        .filter(|(_, template)| {
            let pattern = family_pattern(template);
            !catalog.iter().any(|key| pattern.is_match(key))
        })
        .collect();
    assert!(
        unmatched.is_empty(),
        "key families with no catalog key: {unmatched:?}"
    );
}

const SNIPPET: &str = r##"
use tc_ui::i18n::{keys, t, t_dyn};
// "app.fromComment" is a comment, not a string.
/* "common.inBlock" /* nested */ is a comment too */
fn view<'a>(kind: &'a str) {
    let a = t(keys::app::SAVED, &[]);
    let b = t_dyn("home.title.missing", &[], None);
    let role = Role { name: "sources.roleNotes" };
    let c = t_dyn(&format!("net.ask.{kind}"), &[], None);
    let d = format!("syncResult.error.{}", kind);
    let decoys = ["app.json", "0.5", "a.b", "no namespace.here", '.', '"', '\\'];
    let raw = r#"quote " and common.close"#;
    let escaped = "esc \" app.escaped";
    let plain = format!("{kind} has no dot");
    let prose = format!("{name}. {switch_title}");
    let file = format!("gallery-spec.json: {e}");
    let e = t(keys::refusal::open::LOCK_HELD, &[("pid", &4)]);
    let not_a_key = other::keys_of(keys::all);
}
"##;

#[test]
fn the_scan_sees_each_call_shape() {
    let catalog = catalog_keys();
    let namespaces = namespaces(&catalog);
    assert_eq!(
        namespaced_strings(SNIPPET, &namespaces),
        ["home.title.missing", "sources.roleNotes", "app.json"],
        "the strings that could be keys: not comments, not characters, not text with spaces, \
         not templates"
    );
    assert_eq!(
        t_dyn_literals(SNIPPET),
        vec!["home.title.missing".to_owned()]
    );
    assert_eq!(
        template_families(SNIPPET),
        vec![
            "net.ask.{kind}".to_owned(),
            "syncResult.error.{}".to_owned()
        ]
    );
    assert_eq!(
        key_constant_uses(SNIPPET),
        vec![
            "keys::app::SAVED".to_owned(),
            "keys::refusal::open::LOCK_HELD".to_owned()
        ],
        "`keys::all` is not a constant"
    );
    // A family matches the keys it names and no others.
    let pattern = family_pattern("net.ask.{kind}");
    assert!(pattern.is_match("net.ask.signIn"));
    assert!(!pattern.is_match("net.ask.signIn.more"));
    assert!(!pattern.is_match("net.askx.signIn"));
    // The raw string and the escaped quote are one string each.
    let all = tokens(SNIPPET);
    assert!(all.contains(&Token::Str("quote \" and common.close".into())));
    assert!(all.contains(&Token::Str("esc \\\" app.escaped".into())));
}

#[test]
fn a_misspelled_key_is_caught() {
    let catalog = catalog_keys();
    let namespaces = namespaces(&catalog);
    let typo = r#"t_dyn("home.titel", &[], None); let r = Role { name: "sources.roleNotess" };"#;
    let flagged: Vec<_> = namespaced_strings(typo, &namespaces)
        .into_iter()
        .filter(|key| !catalog.contains(key))
        .collect();
    assert_eq!(flagged, ["home.titel", "sources.roleNotess"]);
    // A t_dyn literal with a namespace nobody has is caught by its call shape.
    let bad_namespace = r#"t_dyn("hom.title", &[], None)"#;
    assert!(namespaced_strings(bad_namespace, &namespaces).is_empty());
    assert!(
        t_dyn_literals(bad_namespace)
            .iter()
            .any(|key| !catalog.contains(key))
    );
    // A family that matches nothing is caught.
    assert!(
        !catalog
            .iter()
            .any(|key| family_pattern("nosuch.{x}.title").is_match(key))
    );
}

#[test]
fn the_scan_finds_at_least_its_floors() {
    let catalog = catalog_keys();
    let namespaces = namespaces(&catalog);
    let uses = scan(key_constant_uses).len();
    let namespaced = scan(|src| namespaced_strings(src, &namespaces)).len();
    let templates = scan(template_families).len();
    // `checked_sub` rather than `>=`, so a floor of 0 is not an always-true comparison.
    assert!(
        uses.checked_sub(KEY_CONSTANT_USES_FLOOR).is_some(),
        "{uses} key constants named, floor {KEY_CONSTANT_USES_FLOOR}"
    );
    assert!(
        namespaced.checked_sub(NAMESPACED_FLOOR).is_some(),
        "{namespaced} namespaced strings, floor {NAMESPACED_FLOOR}"
    );
    assert!(
        templates.checked_sub(TEMPLATE_FLOOR).is_some(),
        "{templates} key families, floor {TEMPLATE_FLOOR}"
    );
}
