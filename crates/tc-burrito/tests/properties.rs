//! Property tests: the validator never panics, the model keeps what it does not know, the
//! writer is canonical, written projects validate, and the code checks agree with the
//! schema wherever both judge the same value.

// Helpers outside #[test] functions unwrap too; in a test, a panic is the failure report.
#![allow(clippy::unwrap_used)]

mod common;

use proptest::prelude::*;
use serde_json::{Value, json};
use std::sync::LazyLock;
use tc_burrito::{
    Copyright, FlavorType, Generator, IngredientPath, Language, LocalizedText, Md5, MemProject,
    Metadata, MimeType, NewProject, Problem, Scope, ScopeBook, ScopeRange, TextTranslation,
    Timestamp, to_canonical_json, validate_metadata, validate_metadata_bytes, validate_project,
};
use tc_core::BookId;

static BIBLE: LazyLock<Value> = LazyLock::new(|| common::read_metadata(&common::bible_dir()));
static OBS: LazyLock<Value> = LazyLock::new(|| common::read_metadata(&common::obs_dir()));

/// Keys the burrito uses, so random documents reach deep into the checks.
const KEYS: &[&str] = &[
    "format",
    "meta",
    "version",
    "category",
    "dateCreated",
    "defaultLocale",
    "idAuthorities",
    "identification",
    "primary",
    "revision",
    "timestamp",
    "name",
    "languages",
    "type",
    "flavorType",
    "flavor",
    "currentScope",
    "confidential",
    "ingredients",
    "checksum",
    "md5",
    "size",
    "mimeType",
    "scope",
    "role",
    "copyright",
    "publicDomain",
    "shortStatements",
    "licenses",
    "relationships",
    "agencies",
    "local",
    "TIT",
    "ingredients/TIT.usfm",
];

fn key() -> impl Strategy<Value = String> {
    prop_oneof![
        3 => proptest::sample::select(KEYS).prop_map(str::to_owned),
        1 => any::<String>(),
    ]
}

fn json_value() -> impl Strategy<Value = Value> {
    let leaf = prop_oneof![
        Just(Value::Null),
        any::<bool>().prop_map(Value::Bool),
        any::<i64>().prop_map(Value::from),
        any::<u64>().prop_map(Value::from),
        any::<f64>().prop_map(Value::from),
        // Every bit pattern, so floats with 17 significant digits come up often.
        any::<u64>().prop_map(|bits| Value::from(f64::from_bits(bits))),
        prop_oneof![
            any::<String>(),
            proptest::sample::select(
                &[
                    "scripture",
                    "gloss",
                    "textTranslation",
                    "textStories",
                    "source",
                    "1.0.0",
                    "scripture burrito",
                    "text/plain",
                    "2026-07-02T12:00:00Z",
                    "TIT",
                    "x-alignment",
                ][..]
            )
            .prop_map(str::to_owned),
        ]
        .prop_map(Value::String),
    ];
    leaf.prop_recursive(5, 64, 6, |inner| {
        prop_oneof![
            prop::collection::vec(inner.clone(), 0..6).prop_map(Value::Array),
            prop::collection::vec((key(), inner), 0..6)
                .prop_map(|entries| Value::Object(entries.into_iter().collect())),
        ]
    })
}

/// Every JSON Pointer into `value`, the root included.
fn pointers(value: &Value) -> Vec<String> {
    fn walk(value: &Value, at: String, out: &mut Vec<String>) {
        out.push(at.clone());
        let token = |text: &str| text.replace('~', "~0").replace('/', "~1");
        match value {
            Value::Object(map) => {
                for (key, child) in map {
                    walk(child, format!("{at}/{}", token(key)), out);
                }
            }
            Value::Array(items) => {
                for (index, child) in items.iter().enumerate() {
                    walk(child, format!("{at}/{index}"), out);
                }
            }
            _ => {}
        }
    }
    let mut out = Vec::new();
    walk(value, String::new(), &mut out);
    out
}

/// Objects in which the model keeps unknown keys in an `extra` map.
fn extensible_objects(document: &Value) -> Vec<String> {
    let mut out = vec![
        String::new(),
        "/meta".to_owned(),
        "/meta/generator".to_owned(),
        "/identification".to_owned(),
        "/type".to_owned(),
        "/type/flavorType".to_owned(),
        "/type/flavorType/flavor".to_owned(),
        "/copyright".to_owned(),
    ];
    let token = |text: &str| text.replace('~', "~0").replace('/', "~1");
    for (label, _) in document["idAuthorities"].as_object().unwrap() {
        out.push(format!("/idAuthorities/{}", token(label)));
    }
    for (authority, ids) in document["identification"]["primary"].as_object().unwrap() {
        for (id, _) in ids.as_object().unwrap() {
            out.push(format!(
                "/identification/primary/{}/{}",
                token(authority),
                token(id)
            ));
        }
    }
    for (path, entry) in document["ingredients"].as_object().unwrap() {
        out.push(format!("/ingredients/{}", token(path)));
        if entry.get("checksum").is_some() {
            out.push(format!("/ingredients/{}/checksum", token(path)));
        }
    }
    for (array, _) in [("languages", ()), ("relationships", ())] {
        if let Some(items) = document.get(array).and_then(Value::as_array) {
            for index in 0..items.len() {
                out.push(format!("/{array}/{index}"));
            }
        }
    }
    if let Some(statements) = document["copyright"]["shortStatements"].as_array() {
        for index in 0..statements.len() {
            out.push(format!("/copyright/shortStatements/{index}"));
        }
    }
    out
}

fn sample() -> impl Strategy<Value = &'static Value> {
    prop_oneof![Just(&*BIBLE), Just(&*OBS)]
}

fn sample_files(document: &Value) -> MemProject {
    let dir = if std::ptr::eq(document, &*BIBLE) {
        common::bible_dir()
    } else {
        common::obs_dir()
    };
    let mut files = MemProject::new();
    for path in document["ingredients"].as_object().unwrap().keys() {
        files.insert(path.clone(), std::fs::read(dir.join(path)).unwrap());
    }
    files
}

fn sort_keys(value: &Value) -> Value {
    match value {
        Value::Object(map) => {
            let mut entries: Vec<(&String, &Value)> = map.iter().collect();
            entries.sort_by(|a, b| a.0.cmp(b.0));
            Value::Object(
                entries
                    .into_iter()
                    .map(|(key, child)| (key.clone(), sort_keys(child)))
                    .collect(),
            )
        }
        Value::Array(items) => Value::Array(items.iter().map(sort_keys).collect()),
        other => other.clone(),
    }
}

/// Rebuilds every object of `value` with its keys in a shuffled order.
fn shuffle_keys(value: &Value, seed: &mut u64) -> Value {
    match value {
        Value::Object(map) => {
            let mut entries: Vec<(&String, &Value)> = map.iter().collect();
            for index in (1..entries.len()).rev() {
                *seed = seed.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1);
                let other = usize::try_from(*seed >> 33).unwrap() % (index + 1);
                entries.swap(index, other);
            }
            Value::Object(
                entries
                    .into_iter()
                    .map(|(key, child)| (key.clone(), shuffle_keys(child, seed)))
                    .collect(),
            )
        }
        Value::Array(items) => {
            Value::Array(items.iter().map(|item| shuffle_keys(item, seed)).collect())
        }
        other => other.clone(),
    }
}

/// The schema's verdict on one value placed at `pointer` in the Bible sample, as the set
/// of schema problems at that place.
fn schema_refuses_at(pointer: &str, mutate: impl FnOnce(&mut Value)) -> bool {
    let mut document = BIBLE.clone();
    mutate(&mut document);
    validate_metadata(&document)
        .iter()
        .any(|problem| matches!(problem, Problem::Schema { pointer: at, .. } if at == pointer))
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    #[test]
    fn arbitrary_json_never_panics_the_validator(value in json_value()) {
        let problems = validate_metadata(&value);
        let bytes = serde_json::to_vec(&value).unwrap();
        prop_assert_eq!(validate_metadata_bytes(&bytes), problems.clone());
        let mut files = MemProject::new();
        files.insert("ingredients/TIT.usfm", b"\\id TIT\n".to_vec());
        files.insert("ingredients/x/y.json", b"{}".to_vec());
        let _ = validate_project(&bytes, &files);
        let _ = Metadata::from_value(&value);
        prop_assert!(!problems.is_empty(), "random JSON is not a burrito: {value}");
    }

    #[test]
    fn arbitrary_bytes_never_panic_the_validator(bytes in prop::collection::vec(any::<u8>(), 0..512)) {
        let problems = validate_metadata_bytes(&bytes);
        prop_assert!(!problems.is_empty());
        let _ = validate_project(&bytes, &MemProject::new());
        let _ = Metadata::from_slice(&bytes);
    }

    #[test]
    fn a_sample_with_any_part_replaced_never_panics(
        document in sample(),
        pick in any::<prop::sample::Index>(),
        replacement in json_value(),
    ) {
        let files = sample_files(document);
        let all = pointers(document);
        let at = pick.get(&all);
        let mut changed = document.clone();
        *changed.pointer_mut(at).unwrap() = replacement;
        let problems = validate_project(&serde_json::to_vec(&changed).unwrap(), &files);
        if problems.is_empty() {
            // A document the validator accepts must open in the app and write back as read.
            let model = Metadata::from_value(&changed).unwrap();
            prop_assert_eq!(model.to_value().unwrap(), changed);
        }
    }

    #[test]
    fn unknown_fields_survive_read_modify_write(
        document in sample(),
        injections in prop::collection::vec(
            (any::<prop::sample::Index>(), "x-[a-zA-Z0-9]{1,8}", json_value()),
            1..6,
        ),
    ) {
        let mut changed = document.clone();
        let targets = extensible_objects(document);
        for (pick, key, value) in injections {
            let target = pick.get(&targets);
            changed.pointer_mut(target).unwrap().as_object_mut().unwrap().insert(key, value);
        }
        let mut model = Metadata::from_value(&changed).unwrap();
        prop_assert_eq!(model.to_value().unwrap(), changed.clone());
        // A modification elsewhere leaves every unknown field in place.
        model.confidential = true;
        let mut expected = changed;
        expected["confidential"] = json!(true);
        let written = Metadata::from_slice(&model.to_canonical_json().unwrap()).unwrap();
        prop_assert_eq!(written.to_value().unwrap(), expected);
    }

    #[test]
    fn canonical_bytes_ignore_key_order(document in sample(), seed in any::<u64>()) {
        let mut state = seed;
        let shuffled = shuffle_keys(document, &mut state);
        prop_assert_eq!(to_canonical_json(&shuffled), to_canonical_json(document));
        let a = Metadata::from_value(document).unwrap();
        let b = Metadata::from_value(&shuffled).unwrap();
        prop_assert_eq!(&a, &b);
        prop_assert_eq!(a.to_canonical_json().unwrap(), b.to_canonical_json().unwrap());
    }

    #[test]
    fn canonical_json_reads_back_equal(value in json_value()) {
        let bytes = to_canonical_json(&value);
        prop_assert_eq!(bytes.last(), Some(&b'\n'));
        let back: Value = serde_json::from_slice(&bytes).unwrap();
        prop_assert_eq!(&back, &value);
        prop_assert_eq!(to_canonical_json(&back), bytes.clone());
        // The same text serde_json writes for the value with sorted keys, so numbers and
        // escapes look the way other tools expect.
        let pretty = serde_json::to_string_pretty(&sort_keys(&value)).unwrap() + "\n";
        prop_assert_eq!(String::from_utf8(bytes).unwrap(), pretty);
    }

    #[test]
    fn recorded_projects_always_validate(
        books in prop::collection::btree_set(0..66usize, 1..5),
        extra_files in prop::collection::btree_map(
            "[a-z][a-z0-9_-]{0,7}(/[a-z][a-z0-9_-]{0,7}){0,2}\\.(json|md|tsv|txt|bin)",
            prop::collection::vec(any::<u8>(), 0..64),
            0..5,
        ),
        texts in prop::collection::vec(prop::collection::vec(any::<u8>(), 0..256), 5),
    ) {
        let books: Vec<BookId> = books.into_iter().map(|index| BookId::ALL[index]).collect();
        let mut metadata = new_project(Scope::whole_books(books.iter().copied()));
        let mut files = MemProject::new();
        for (index, book) in books.iter().enumerate() {
            let path = IngredientPath::parse(&format!("ingredients/{}.usfm", book.code())).unwrap();
            let bytes = &texts[index];
            metadata.record_ingredient(path.clone(), bytes).scope =
                Some(Scope::whole_books([*book]));
            files.insert(path, bytes.clone());
        }
        for (relative, bytes) in extra_files {
            // The generator can spell a Windows device name such as `con.txt`.
            let Ok(path) = IngredientPath::parse(&format!("ingredients/extra/{relative}")) else {
                continue;
            };
            metadata.record_ingredient(path.clone(), &bytes);
            files.insert(path, bytes);
        }
        let written = metadata.to_canonical_json().unwrap();
        prop_assert_eq!(validate_project(&written, &files), vec![]);

        // Re-recording changed bytes keeps it valid; forgetting to record breaks it.
        let first = metadata.ingredients.keys().next().unwrap().clone();
        let mut changed = files.get(first.as_str()).unwrap().to_vec();
        changed.push(b'\n');
        files.insert(first.clone(), changed.clone());
        prop_assert!(!validate_project(&written, &files).is_empty());
        metadata.record_ingredient(first, &changed);
        prop_assert_eq!(validate_project(&metadata.to_canonical_json().unwrap(), &files), vec![]);
    }

    #[test]
    fn scope_ranges_round_trip_through_text(
        parts in prop::collection::vec(1u32..2000, 4),
        form in 0usize..5,
    ) {
        let n = |index: usize| std::num::NonZeroU32::new(parts[index]).unwrap();
        let range = match form {
            0 => ScopeRange::Chapter(n(0)),
            1 => ScopeRange::Chapters(n(0), n(1)),
            2 => ScopeRange::Verse(n(0), n(1)),
            3 => ScopeRange::Verses(n(0), n(1), n(2)),
            _ => ScopeRange::Span(n(0), n(1), n(2), n(3)),
        };
        let text = range.to_string();
        prop_assert_eq!(text.parse::<ScopeRange>().unwrap(), range);
    }

    #[test]
    fn md5_round_trips_through_text(bytes in any::<[u8; 16]>(), data in prop::collection::vec(any::<u8>(), 0..64)) {
        let digest = Md5::of(&data);
        prop_assert_eq!(Md5::parse(&digest.to_string()).unwrap(), digest);
        let hex: String = bytes.iter().map(|b| format!("{b:02x}")).collect();
        prop_assert_eq!(Md5::parse(&hex).unwrap().bytes(), bytes);
    }

    #[test]
    fn scope_text_agrees_with_the_schema(text in "[0-9:-]{0,9}|[1-9][0-9]{0,2}(:[1-9][0-9]?)?(-[1-9][0-9]?(:[0-9]{1,2})?)?") {
        let refused = schema_refuses_at("/type/flavorType/currentScope/TIT", |m| {
            m["type"]["flavorType"]["currentScope"]["TIT"] = json!([text.clone()]);
        });
        prop_assert_eq!(text.parse::<ScopeRange>().is_ok(), !refused, "{:?}", text);
    }

    #[test]
    fn book_codes_agree_with_the_schema(code in "[A-Z0-9]{3}|[A-Za-z0-9]{2,4}") {
        let accepted = ScopeBook::parse(&code).is_ok();
        let refused = schema_refuses_at("/ingredients/ingredients~1TIT.usfm/scope", |m| {
            m["ingredients"]["ingredients/TIT.usfm"]["scope"] = json!({ code.clone(): [] });
        });
        prop_assert_eq!(accepted, !refused, "{:?}", code);
    }

    #[test]
    fn media_types_agree_with_the_schema(text in "[a-zA-Z0-9.+;= /-]{0,16}") {
        let refused = schema_refuses_at("/ingredients/ingredients~1TIT.usfm/mimeType", |m| {
            m["ingredients"]["ingredients/TIT.usfm"]["mimeType"] = json!(text.clone());
        });
        prop_assert_eq!(MimeType::parse(&text).is_ok(), !refused, "{:?}", text);
    }

    #[test]
    fn md5_text_agrees_with_the_schema(text in "[0-9a-fA-Fg]{30,34}") {
        let refused = schema_refuses_at("/ingredients/ingredients~1TIT.usfm/checksum/md5", |m| {
            m["ingredients"]["ingredients/TIT.usfm"]["checksum"]["md5"] = json!(text.clone());
        });
        prop_assert_eq!(Md5::parse(&text).is_ok(), !refused, "{:?}", text);
    }

    #[test]
    fn date_times_agree_with_the_schema(
        text in "[0-9]{4}-[0-1][0-9]-[0-3][0-9][Tt ][0-2][0-9]:[0-5][0-9]:[0-6][0-9](\\.[0-9]{1,4})?(Z|z|[+-][0-2][0-9]:[0-5][0-9])?",
    ) {
        let refused = schema_refuses_at("/meta/dateCreated", |m| {
            m["meta"]["dateCreated"] = json!(text.clone());
        });
        prop_assert_eq!(Timestamp::from_text(text.clone()).check().is_ok(), !refused, "{:?}", text);
    }

    #[test]
    fn every_path_the_model_accepts_the_schema_accepts(text in any::<String>()) {
        if let Ok(path) = IngredientPath::parse(&text) {
            prop_assert_eq!(path.as_str(), text.as_str());
            let refused = schema_refuses_at("/ingredients", |m| {
                let entry = m["ingredients"]["ingredients/TIT.usfm"].clone();
                m["ingredients"].as_object_mut().unwrap().insert(text.clone(), entry);
            });
            prop_assert!(!refused, "{:?}", text);
        }
    }
}

fn new_project(scope: Scope) -> Metadata {
    Metadata::new(NewProject {
        project_id: "prop".to_owned(),
        name: LocalizedText::single("en", "Property project"),
        language: Language {
            tag: "en".to_owned(),
            name: LocalizedText::single("en", "English"),
            script_direction: None,
            rod: None,
            numbering_system: None,
            extra: Default::default(),
        },
        default_locale: "en".to_owned(),
        flavor_type: FlavorType::text_translation(TextTranslation::new("3.0"), scope),
        copyright: Copyright::public_domain(),
        generator: Generator {
            software_name: "tc-rst".to_owned(),
            software_version: "0.1.0".to_owned(),
            user_id: None,
            user_name: None,
            extra: Default::default(),
        },
        created: Timestamp::now(),
    })
}
