//! Mutations of the tC4 samples, each with the exact problems it must produce.
//!
//! Cases 00 to 47 are the 48 controls of the burrito-validity research probe, in its order.
//! Their schema verdicts were cross-checked against ajv running tC4's own loader (48 of 48
//! agree). Where that schema accepts a document the rebuild must refuse, the expected
//! problem comes from the code checks. Later cases cover the remaining code checks and
//! the file checks.

// Helpers outside #[test] functions unwrap too; in a test, a panic is the failure report.
#![allow(
    clippy::unwrap_used,
    reason = "test code: a failed unwrap is a failed test"
)]

mod common;

use serde_json::{Map, Value, json};
use tc_burrito::{
    CopyrightBasis, MemProject, PathRule, Problem, TimestampError, TimestampField,
    validate_metadata, validate_metadata_bytes, validate_project,
};

const TIT: &str = "ingredients/TIT.usfm";
const TIT_POINTER: &str = "/ingredients/ingredients~1TIT.usfm";

/// A problem as a test expects it. Schema problems are compared by place and keyword,
/// because their messages come from the schema library.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
enum Want {
    Schema(String, String),
    Code(String),
}

fn schema(pointer: &str, keyword: &str) -> Want {
    Want::Schema(pointer.to_owned(), keyword.to_owned())
}

fn code(problem: Problem) -> Want {
    Want::Code(format!("{problem:?}"))
}

fn summarize(problems: Vec<Problem>) -> Vec<Want> {
    let mut wants: Vec<Want> = problems
        .into_iter()
        .map(|problem| match problem {
            Problem::Schema {
                pointer, keyword, ..
            } => Want::Schema(pointer, keyword),
            other => code(other),
        })
        .collect();
    wants.sort();
    wants
}

fn ingredients(m: &mut Value) -> &mut Map<String, Value> {
    m["ingredients"].as_object_mut().unwrap()
}

fn add_ingredient_copy(m: &mut Value, path: &str) {
    let entry = m["ingredients"][TIT].clone();
    ingredients(m).insert(path.to_owned(), entry);
}

fn unsafe_path(path: &str, rule: PathRule) -> Want {
    code(Problem::UnsafePath {
        path: path.to_owned(),
        rule,
    })
}

fn uncovered(book: &str) -> Want {
    code(Problem::UncoveredBook {
        book: book.to_owned(),
    })
}

fn date_created(value: &str, reason: TimestampError) -> Want {
    code(Problem::InvalidTimestamp {
        field: TimestampField::DateCreated,
        value: value.to_owned(),
        reason,
    })
}

fn copyright(found: &[CopyrightBasis]) -> Want {
    code(Problem::CopyrightNotExactlyOne {
        found: found.to_vec(),
    })
}

type Mutation = fn(&mut Value);

struct Case {
    label: &'static str,
    mutate: Mutation,
    want: Vec<Want>,
}

fn case(label: &'static str, mutate: Mutation, want: Vec<Want>) -> Case {
    Case {
        label,
        mutate,
        want,
    }
}

fn probe_controls() -> Vec<Case> {
    use PathRule::*;
    use TimestampError::*;
    let tit = |field: &str| format!("{TIT_POINTER}/{field}");
    vec![
        case(
            "00 USFM mimeType text/x-usfm",
            |m| m["ingredients"][TIT]["mimeType"] = json!("text/x-usfm"),
            vec![],
        ),
        case(
            "01 mimeType with a dot",
            |m| m["ingredients"][TIT]["mimeType"] = json!("application/vnd.foo+json"),
            vec![
                schema(&tit("mimeType"), "pattern"),
                code(Problem::InvalidMimeType {
                    path: TIT.to_owned(),
                    value: "application/vnd.foo+json".to_owned(),
                }),
            ],
        ),
        case(
            "02 mimeType with a charset parameter",
            |m| m["ingredients"][TIT]["mimeType"] = json!("text/plain; charset=utf-8"),
            vec![
                schema(&tit("mimeType"), "pattern"),
                code(Problem::InvalidMimeType {
                    path: TIT.to_owned(),
                    value: "text/plain; charset=utf-8".to_owned(),
                }),
            ],
        ),
        case(
            "03 upper-case md5",
            |m| {
                m["ingredients"][TIT]["checksum"]["md5"] = json!("1CE94F44DD0F1C4E87FA88A7A80F9216")
            },
            vec![schema(&tit("checksum/md5"), "pattern")],
        ),
        case(
            "04 checksum without md5",
            |m| m["ingredients"][TIT]["checksum"] = json!({"sha3-256": "0".repeat(64)}),
            vec![
                schema(&tit("checksum"), "required"),
                code(Problem::MissingChecksum {
                    path: TIT.to_owned(),
                }),
            ],
        ),
        case(
            "05 ingredient without checksum (schema allows)",
            |m| {
                m["ingredients"][TIT]
                    .as_object_mut()
                    .unwrap()
                    .remove("checksum");
            },
            vec![code(Problem::MissingChecksum {
                path: TIT.to_owned(),
            })],
        ),
        case(
            "06 ingredient without size",
            |m| {
                m["ingredients"][TIT]
                    .as_object_mut()
                    .unwrap()
                    .remove("size");
            },
            vec![schema(TIT_POINTER, "required")],
        ),
        case(
            "07 ingredient with an extra field",
            |m| m["ingredients"][TIT]["x-foo"] = json!(1),
            vec![schema(TIT_POINTER, "additionalProperties")],
        ),
        case(
            "08 role x-alignment",
            |m| m["ingredients"][TIT]["role"] = json!("x-alignment"),
            vec![],
        ),
        case(
            "09 role alignment without x-",
            |m| m["ingredients"][TIT]["role"] = json!("alignment"),
            vec![schema(&tit("role"), "oneOf")],
        ),
        case(
            "10 empty ingredient scope",
            |m| m["ingredients"][TIT]["scope"] = json!({}),
            vec![schema(&tit("scope"), "minProperties"), uncovered("TIT")],
        ),
        case(
            "11 scope key FRT",
            |m| m["ingredients"][TIT]["scope"] = json!({"FRT": []}),
            vec![schema(&tit("scope"), "propertyNames"), uncovered("TIT")],
        ),
        case(
            "12 scope key in lower case",
            |m| m["ingredients"][TIT]["scope"] = json!({"tit": []}),
            vec![
                schema(&tit("scope"), "propertyNames"),
                schema(&tit("scope"), "propertyNames"),
                uncovered("TIT"),
            ],
        ),
        case(
            "13 scope range 1:1-2:5",
            |m| m["type"]["flavorType"]["currentScope"]["TIT"] = json!(["1:1-2:5"]),
            vec![],
        ),
        case(
            "14 scope range 0",
            |m| m["type"]["flavorType"]["currentScope"]["TIT"] = json!(["0"]),
            vec![schema("/type/flavorType/currentScope/TIT", "oneOf")],
        ),
        case(
            "15 duplicate scope range",
            |m| m["type"]["flavorType"]["currentScope"]["TIT"] = json!(["1", "1"]),
            vec![schema("/type/flavorType/currentScope/TIT", "oneOf")],
        ),
        case(
            "16 path with a backslash (schema allows)",
            |m| add_ingredient_copy(m, "ingredients\\x.usfm"),
            vec![unsafe_path("ingredients\\x.usfm", Backslash)],
        ),
        case(
            "17 path ../escape.usfm (schema allows)",
            |m| add_ingredient_copy(m, "../escape.usfm"),
            vec![unsafe_path("../escape.usfm", ParentDir)],
        ),
        case(
            "18 path with a colon",
            |m| add_ingredient_copy(m, "c:/x.usfm"),
            vec![
                schema("/ingredients", "propertyNames"),
                unsafe_path("c:/x.usfm", ForbiddenChar(':')),
            ],
        ),
        case(
            "19 path with a leading slash",
            |m| add_ingredient_copy(m, "/abs.usfm"),
            vec![
                schema("/ingredients", "propertyNames"),
                unsafe_path("/abs.usfm", Absolute),
            ],
        ),
        case(
            "20 meta.version 1.1.0",
            |m| m["meta"]["version"] = json!("1.1.0"),
            vec![schema("/meta/version", "enum")],
        ),
        case(
            "21 dateCreated as a date only",
            |m| m["meta"]["dateCreated"] = json!("2026-07-02"),
            vec![
                schema("/meta/dateCreated", "format"),
                date_created("2026-07-02", Malformed),
            ],
        ),
        case(
            "22 dateCreated without a timezone",
            |m| m["meta"]["dateCreated"] = json!("2026-07-02T12:00:00"),
            vec![
                schema("/meta/dateCreated", "format"),
                date_created("2026-07-02T12:00:00", MissingTimezone),
            ],
        ),
        case(
            "23 dateCreated with an offset",
            |m| m["meta"]["dateCreated"] = json!("2026-07-02T12:00:00+01:00"),
            vec![],
        ),
        case(
            "24 normalization in lower case",
            |m| m["meta"]["normalization"] = json!("nfc"),
            vec![schema("/meta/normalization", "enum")],
        ),
        case(
            "25 path with a space (schema allows)",
            |m| add_ingredient_copy(m, "ingredients/my file.usfm"),
            vec![unsafe_path("ingredients/my file.usfm", Whitespace)],
        ),
        case(
            "26 empty currentScope",
            |m| m["type"]["flavorType"]["currentScope"] = json!({}),
            vec![schema("/type/flavorType/currentScope", "minProperties")],
        ),
        case(
            "27 gloss/textStories with an extra field",
            |m| {
                m["type"]["flavorType"] = json!({"name": "gloss",
                    "flavor": {"name": "textStories", "x-a": 1}, "currentScope": {"GEN": []}})
            },
            vec![schema("/type/flavorType/flavor", "oneOf")],
        ),
        case(
            "28 gloss/textStories as tC4 writes OBS",
            |m| {
                m["type"]["flavorType"] = json!({"name": "gloss",
                    "flavor": {"name": "textStories"}, "currentScope": {"GEN": []}})
            },
            vec![],
        ),
        case(
            "29 gloss/x-textStories",
            |m| {
                m["type"]["flavorType"] = json!({"name": "gloss",
                    "flavor": {"name": "x-textStories"}, "currentScope": {"GEN": []}})
            },
            vec![],
        ),
        case(
            "30 scripture/x-tc4Project",
            |m| m["type"]["flavorType"]["flavor"] = json!({"name": "x-tc4Project"}),
            vec![],
        ),
        case(
            "31 dateCreated yesterday",
            |m| m["meta"]["dateCreated"] = json!("yesterday"),
            vec![
                schema("/meta/dateCreated", "format"),
                date_created("yesterday", Malformed),
            ],
        ),
        case(
            "32 defaultLocale missing",
            |m| {
                m["meta"].as_object_mut().unwrap().remove("defaultLocale");
            },
            vec![schema("/meta", "required")],
        ),
        case(
            "33 languages missing (schema allows, bug #343)",
            |m| {
                m.as_object_mut().unwrap().remove("languages");
            },
            vec![code(Problem::MissingLanguages)],
        ),
        case(
            "34 copyright with statements and licenses",
            |m| {
                m["copyright"]["licenses"] =
                    json!([{"url": "https://creativecommons.org/licenses/by-sa/4.0/"}])
            },
            vec![
                schema("/copyright", "oneOf"),
                copyright(&[CopyrightBasis::ShortStatements, CopyrightBasis::Licenses]),
            ],
        ),
        case(
            "35 copyright public domain and statements",
            |m| m["copyright"]["publicDomain"] = json!(true),
            vec![
                schema("/copyright", "oneOf"),
                copyright(&[
                    CopyrightBasis::PublicDomain,
                    CopyrightBasis::ShortStatements,
                ]),
            ],
        ),
        case(
            "36 agencies without a rights holder",
            |m| {
                m["agencies"] = json!([{"id": "dcs::x", "name": {"en": "X"}, "roles": ["content"]}])
            },
            vec![schema("/agencies", "contains")],
        ),
        case(
            "37 agencies with a rights holder",
            |m| {
                m["agencies"] =
                    json!([{"id": "dcs::x", "name": {"en": "X"}, "roles": ["rightsHolder"]}])
            },
            vec![],
        ),
        case(
            "38 identification.primary with two authorities",
            |m| {
                m["identification"]["primary"]["dcs"] =
                    json!({"a/b": {"revision": "1", "timestamp": "2026"}})
            },
            vec![schema("/identification/primary", "maxProperties")],
        ),
        case(
            "39 primary timestamp nonsense (schema bug: unchecked)",
            |m| {
                m["identification"]["primary"]["local"]["ejemplo_tj"]["timestamp"] =
                    json!("not a date")
            },
            vec![code(Problem::InvalidTimestamp {
                field: TimestampField::PrimaryTimestamp,
                value: "not a date".to_owned(),
                reason: Malformed,
            })],
        ),
        case(
            "40 idAuthorities label Local (schema bug: unchecked)",
            |m| {
                let authority = m["idAuthorities"]["local"].clone();
                m["idAuthorities"]["Local"] = authority;
            },
            vec![code(Problem::InvalidAuthorityLabel {
                label: "Local".to_owned(),
            })],
        ),
        case(
            "41 usfmVersion missing",
            |m| {
                let flavor = m["type"]["flavorType"]["flavor"].as_object_mut().unwrap();
                flavor.remove("usfmVersion");
            },
            vec![schema("/type/flavorType/flavor", "required")],
        ),
        case(
            "42 expression relationship with an x- flavor",
            |m| m["relationships"][4]["relationType"] = json!("expression"),
            vec![schema("/relationships/4", "oneOf")],
        ),
        case(
            "43 target relationship with an x- flavor",
            |m| m["relationships"][4]["relationType"] = json!("target"),
            vec![schema("/relationships/4", "oneOf")],
        ),
        case(
            "44 unknown top-level field",
            |m| m["x-tc4"] = json!({}),
            vec![schema("", "additionalProperties")],
        ),
        case(
            "45 language tag es-419",
            |m| m["languages"][0]["tag"] = json!("es-419"),
            vec![],
        ),
        case(
            "46 language tag Spanish!",
            |m| m["languages"][0]["tag"] = json!("Spanish!"),
            vec![schema("/languages/0/tag", "pattern")],
        ),
        case(
            "47 localized text padded with spaces",
            |m| m["identification"]["name"]["en"] = json!(" padded "),
            vec![schema("/identification/name/en", "pattern")],
        ),
    ]
}

fn more_document_cases() -> Vec<Case> {
    use PathRule::*;
    use TimestampError::*;
    vec![
        case(
            "48 currentScope book with no ingredient",
            |m| m["type"]["flavorType"]["currentScope"]["MAT"] = json!([]),
            vec![uncovered("MAT")],
        ),
        case(
            "49 alignment JSON does not cover a book without USFM",
            |m| {
                ingredients(m).remove(TIT);
            },
            vec![uncovered("TIT")],
        ),
        case(
            "50 empty copyright",
            |m| m["copyright"] = json!({}),
            vec![schema("/copyright", "oneOf"), copyright(&[])],
        ),
        case(
            "51 copyright publicDomain false alone",
            |m| m["copyright"] = json!({"publicDomain": false}),
            vec![schema("/copyright", "oneOf"), copyright(&[])],
        ),
        case(
            "52 copyright public domain alone",
            |m| m["copyright"] = json!({"publicDomain": true}),
            vec![],
        ),
        case(
            "53 primary authority not declared",
            |m| {
                let primary = m["identification"]["primary"].as_object_mut().unwrap();
                let entry = primary.remove("local").unwrap();
                primary.insert("door43".to_owned(), entry);
            },
            vec![code(Problem::UndeclaredAuthority {
                authority: "door43".to_owned(),
            })],
        ),
        case(
            "54 primary revision with a leading dash",
            |m| m["identification"]["primary"]["local"]["ejemplo_tj"]["revision"] = json!("-1"),
            vec![code(Problem::InvalidRevision {
                id: "ejemplo_tj".to_owned(),
                value: "-1".to_owned(),
            })],
        ),
        case(
            "55 primary timestamp as the date the schema means",
            |m| {
                m["identification"]["primary"]["local"]["ejemplo_tj"]["timestamp"] =
                    json!("2026-07")
            },
            vec![],
        ),
        case(
            "56 primary timestamp without a timezone",
            |m| {
                m["identification"]["primary"]["local"]["ejemplo_tj"]["timestamp"] =
                    json!("2026-07-02T12:00:00")
            },
            vec![code(Problem::InvalidTimestamp {
                field: TimestampField::PrimaryTimestamp,
                value: "2026-07-02T12:00:00".to_owned(),
                reason: MissingTimezone,
            })],
        ),
        case(
            "57 two paths that differ only in case",
            |m| add_ingredient_copy(m, "ingredients/tit.usfm"),
            vec![code(Problem::PathCaseCollision {
                first: TIT.to_owned(),
                second: "ingredients/tit.usfm".to_owned(),
            })],
        ),
        case(
            "58 hidden path",
            |m| add_ingredient_copy(m, ".git/config"),
            vec![unsafe_path(".git/config", Hidden)],
        ),
        case(
            "59 metadata.json listed as an ingredient",
            |m| add_ingredient_copy(m, "metadata.json"),
            vec![unsafe_path("metadata.json", MetadataFile)],
        ),
        case(
            "60 leap second away from 23:59 UTC",
            |m| m["meta"]["dateCreated"] = json!("2026-07-02T12:00:60Z"),
            vec![
                schema("/meta/dateCreated", "format"),
                date_created("2026-07-02T12:00:60Z", Malformed),
            ],
        ),
        case(
            "61 size written as a float the model cannot keep",
            |m| m["ingredients"][TIT]["size"] = json!(1166.0),
            vec![code(Problem::ModelMismatch {
                message: "invalid type: floating point `1166.0`, expected u64".to_owned(),
            })],
        ),
        case(
            "62 an audience value SB 1.0 does not list",
            |m| m["type"]["flavorType"]["flavor"]["audience"] = json!("youth"),
            vec![schema("/type/flavorType/flavor/audience", "enum")],
        ),
        case(
            "63 dateCreated with a space separator (tC4's ajv-formats accepts it)",
            |m| m["meta"]["dateCreated"] = json!("2026-07-02 12:00:00Z"),
            vec![
                schema("/meta/dateCreated", "format"),
                date_created("2026-07-02 12:00:00Z", SpaceSeparator),
            ],
        ),
        case(
            "64 metadata.json in another letter case (schema allows)",
            |m| add_ingredient_copy(m, "Metadata.json"),
            vec![unsafe_path("Metadata.json", MetadataFile)],
        ),
        case(
            "65 metadata.json in capitals (schema allows)",
            |m| add_ingredient_copy(m, "METADATA.JSON"),
            vec![unsafe_path("METADATA.JSON", MetadataFile)],
        ),
        case(
            "66 one path in NFC and in NFD",
            |m| {
                add_ingredient_copy(m, "ingredients/Jon\u{e1}s.usfm");
                add_ingredient_copy(m, "ingredients/Jona\u{301}s.usfm");
            },
            vec![code(Problem::PathCaseCollision {
                first: "ingredients/Jon\u{e1}s.usfm".to_owned(),
                second: "ingredients/Jona\u{301}s.usfm".to_owned(),
            })],
        ),
        case(
            "67 a TSV of notes does not cover a book without USFM",
            |m| {
                let mut notes = ingredients(m).remove(TIT).unwrap();
                notes["mimeType"] = json!("text/tab-separated-values");
                ingredients(m).insert("ingredients/notes/TIT.tsv".to_owned(), notes);
            },
            vec![uncovered("TIT")],
        ),
        case(
            "68 a USX file covers its book",
            |m| {
                let mut usx = ingredients(m).remove(TIT).unwrap();
                usx["mimeType"] = json!("text/xml");
                ingredients(m).insert("ingredients/TIT.usx".to_owned(), usx);
            },
            vec![],
        ),
        case(
            "69 a file and a folder at one path (schema allows)",
            |m| {
                add_ingredient_copy(m, "ingredients/extra.json");
                add_ingredient_copy(m, "ingredients/extra.json/inner.json");
            },
            vec![code(Problem::PathIsAlsoFolder {
                file: "ingredients/extra.json".to_owned(),
                inside: "ingredients/extra.json/inner.json".to_owned(),
            })],
        ),
        case(
            "70 a file and a folder at one path in another letter case",
            |m| {
                add_ingredient_copy(m, "ingredients/Extra");
                add_ingredient_copy(m, "ingredients/extra/a/b.json");
            },
            vec![code(Problem::PathIsAlsoFolder {
                file: "ingredients/Extra".to_owned(),
                inside: "ingredients/extra/a/b.json".to_owned(),
            })],
        ),
        case(
            "71 a Windows short name (schema allows)",
            |m| add_ingredient_copy(m, "INGRED~1/TIT.usfm"),
            vec![unsafe_path("INGRED~1/TIT.usfm", ShortName)],
        ),
        case(
            "72 a right-to-left override in a path (schema allows)",
            |m| add_ingredient_copy(m, "ingredients/TIT\u{202e}nosj.usfm"),
            vec![unsafe_path("ingredients/TIT\u{202e}nosj.usfm", FormatChar)],
        ),
        case(
            "73 a device name with a superscript digit (schema allows)",
            |m| add_ingredient_copy(m, "ingredients/COM\u{b9}.usfm"),
            vec![unsafe_path("ingredients/COM\u{b9}.usfm", ReservedName)],
        ),
    ]
}

fn run(base: &Value, cases: Vec<Case>) {
    let mut failures = Vec::new();
    for case in cases {
        let mut document = base.clone();
        (case.mutate)(&mut document);
        let got = summarize(validate_metadata(&document));
        let mut want = case.want;
        want.sort();
        if got != want {
            failures.push(format!("{}\n  got  {got:?}\n  want {want:?}", case.label));
        }
        let bytes = serde_json::to_vec(&document).unwrap();
        assert_eq!(
            summarize(validate_metadata_bytes(&bytes)),
            got,
            "{}: bytes and value disagree",
            case.label
        );
    }
    assert!(failures.is_empty(), "\n{}", failures.join("\n"));
}

#[test]
fn the_48_probe_controls_give_the_expected_problems() {
    let cases = probe_controls();
    assert_eq!(cases.len(), 48);
    run(&common::read_metadata(&common::bible_dir()), cases);
}

#[test]
fn the_other_document_rules_give_the_expected_problems() {
    run(
        &common::read_metadata(&common::bible_dir()),
        more_document_cases(),
    );
}

#[test]
fn obs_without_languages_is_refused() {
    let mut document = common::read_metadata(&common::obs_dir());
    document.as_object_mut().unwrap().remove("languages");
    assert_eq!(validate_metadata(&document), [Problem::MissingLanguages]);
}

#[test]
fn obs_currentscope_needs_no_ingredient_coverage() {
    // The OBS template's currentScope lists the Bible passages the stories retell; no
    // ingredient is scoped to them.
    assert_eq!(
        validate_metadata(&common::read_metadata(&common::obs_dir())),
        []
    );
}

/// The Bible sample's files, read into memory.
fn bible_files() -> MemProject {
    let dir = common::bible_dir();
    let mut files = MemProject::new();
    let metadata = common::read_metadata(&dir);
    for path in metadata["ingredients"].as_object().unwrap().keys() {
        files.insert(path.clone(), std::fs::read(dir.join(path)).unwrap());
    }
    files
}

fn project_problems(document: &Value, files: &MemProject) -> Vec<Want> {
    summarize(validate_project(
        &serde_json::to_vec(document).unwrap(),
        files,
    ))
}

#[test]
fn file_checks_compare_every_listed_ingredient_with_its_bytes() {
    let base = common::read_metadata(&common::bible_dir());
    let files = bible_files();
    assert_eq!(project_problems(&base, &files), []);

    let mut wrong_md5 = base.clone();
    wrong_md5["ingredients"][TIT]["checksum"]["md5"] = json!("d41d8cd98f00b204e9800998ecf8427e");
    assert_eq!(
        project_problems(&wrong_md5, &files),
        [code(Problem::ChecksumMismatch {
            path: TIT.to_owned(),
            listed: "d41d8cd98f00b204e9800998ecf8427e".to_owned(),
            actual: "1ce94f44dd0f1c4e87fa88a7a80f9216".to_owned(),
        })]
    );

    let mut wrong_size = base.clone();
    wrong_size["ingredients"][TIT]["size"] = json!(1);
    assert_eq!(
        project_problems(&wrong_size, &files),
        [code(Problem::SizeMismatch {
            path: TIT.to_owned(),
            listed: 1,
            actual: 1166,
        })]
    );

    let mut edited = files.clone();
    let mut bytes = edited.get(TIT).unwrap().to_vec();
    bytes.extend_from_slice(b"\\v 16 nuevo\n");
    let actual = tc_burrito::Md5::of(&bytes).to_string();
    edited.insert(TIT, bytes);
    assert_eq!(
        project_problems(&base, &edited),
        summarize(vec![
            Problem::ChecksumMismatch {
                path: TIT.to_owned(),
                listed: "1ce94f44dd0f1c4e87fa88a7a80f9216".to_owned(),
                actual,
            },
            Problem::SizeMismatch {
                path: TIT.to_owned(),
                listed: 1166,
                actual: 1178,
            },
        ])
    );

    let mut missing = files.clone();
    missing.remove(TIT);
    assert_eq!(
        project_problems(&base, &missing),
        [code(Problem::MissingIngredient {
            path: TIT.to_owned(),
        })]
    );
}

#[test]
fn file_checks_find_unlisted_files_but_skip_hidden_and_backup_files() {
    let base = common::read_metadata(&common::bible_dir());
    let mut files = bible_files();
    files.insert("ingredients/MAT.usfm", b"\\id MAT\n".to_vec());
    files.insert("ingredients/.DS_Store", b"x".to_vec());
    files.insert("ingredients/TIT.usfm.bak", b"x".to_vec());
    files.insert("ingredients/.tmp/TIT.usfm", b"x".to_vec());
    files.insert("README.md", b"outside the ingredients folder".to_vec());
    assert_eq!(
        project_problems(&base, &files),
        [code(Problem::UnlistedFile {
            path: "ingredients/MAT.usfm".to_owned(),
        })]
    );
}

#[test]
fn an_unsafe_path_is_never_looked_up() {
    let mut document = common::read_metadata(&common::bible_dir());
    add_ingredient_copy(&mut document, "../escape.usfm");
    let mut files = bible_files();
    files.insert("../escape.usfm", files.get(TIT).unwrap().to_vec());
    assert_eq!(
        project_problems(&document, &files),
        [unsafe_path("../escape.usfm", PathRule::ParentDir)]
    );
}

#[test]
fn bytes_that_are_not_one_json_document_are_refused() {
    assert!(matches!(
        validate_metadata_bytes(b"{\"format\": ").as_slice(),
        [Problem::NotJson { .. }]
    ));
    let mut text =
        String::from_utf8(std::fs::read(common::bible_dir().join("metadata.json")).unwrap())
            .unwrap();
    text = text.replacen(
        "\"confidential\": false,",
        "\"confidential\": false, \"confidential\": true,",
        1,
    );
    assert_eq!(
        validate_metadata_bytes(text.as_bytes()),
        [Problem::DuplicateKey {
            pointer: "/confidential".to_owned(),
        }]
    );
    assert!(matches!(
        tc_burrito::Metadata::from_slice(text.as_bytes()),
        Err(tc_burrito::Error::DuplicateKey { .. })
    ));
}

fn bible_text() -> String {
    String::from_utf8(std::fs::read(common::bible_dir().join("metadata.json")).unwrap()).unwrap()
}

#[test]
fn a_byte_order_mark_is_named_and_the_rest_still_checked() {
    let text = bible_text();
    let mut bytes = b"\xef\xbb\xbf".to_vec();
    bytes.extend_from_slice(text.as_bytes());
    assert_eq!(
        validate_project(&bytes, &bible_files()),
        [Problem::ByteOrderMark]
    );
    let broken = bytes.len() - text.len() + text.find("\"confidential\": false").unwrap();
    bytes.splice(
        broken..broken + 21,
        b"\"confidential\": 0    ".iter().copied(),
    );
    assert_eq!(
        summarize(validate_metadata_bytes(&bytes)),
        summarize(vec![
            Problem::ByteOrderMark,
            Problem::Schema {
                pointer: "/confidential".to_owned(),
                keyword: "type".to_owned(),
                schema_path: String::new(),
                message: String::new(),
            },
        ])
    );
    // The reader skips the mark, and the writer never writes one.
    let mut with_mark = b"\xef\xbb\xbf".to_vec();
    with_mark.extend_from_slice(text.as_bytes());
    let model = tc_burrito::Metadata::from_slice(&with_mark).unwrap();
    assert_eq!(
        model,
        tc_burrito::Metadata::from_slice(text.as_bytes()).unwrap()
    );
    assert!(model.to_canonical_json().unwrap().starts_with(b"{"));
}

/// The Bible sample with an `x-` flavor, whose schema allows any field, holding `raw` as
/// the JSON text of the field `value`.
fn with_raw_flavor_field(raw: &str) -> Vec<u8> {
    let mut document = common::read_metadata(&common::bible_dir());
    document["type"]["flavorType"]["flavor"] = json!({"name": "x-tc4Project", "value": "@@"});
    serde_json::to_string(&document)
        .unwrap()
        .replacen("\"@@\"", raw, 1)
        .into_bytes()
}

#[test]
fn numbers_are_kept_exactly_or_refused() {
    let pointer = "/type/flavorType/flavor/value";
    for kept in [
        "0",
        "-0",
        "1E2",
        "0.1",
        "1.50",
        "1e23",
        "-2.5e-8",
        "18446744073709551615",
        "-9223372036854775808",
        "1.7976931348623157e308",
    ] {
        let bytes = with_raw_flavor_field(kept);
        assert_eq!(validate_metadata_bytes(&bytes), [], "{kept}");
        // The writer may spell the number differently, never as another value.
        let model = tc_burrito::Metadata::from_slice(&bytes).unwrap();
        let written = model.to_canonical_json().unwrap();
        let reread = tc_burrito::Metadata::from_slice(&written).unwrap();
        assert_eq!(reread, model, "{kept}");
        assert_eq!(validate_metadata_bytes(&written), [], "{kept}");
    }
    let huge = format!("1{}", "0".repeat(400));
    for changed in [
        "123456789012345678901234567890",
        "18446744073709551616",
        "-9223372036854775809",
        "0.30000000000000000000001",
        "1e-400",
        "4.9e-324",
        "1e400",
        "-1e400",
        "1e99999999999999999999",
        huge.as_str(),
    ] {
        let bytes = with_raw_flavor_field(changed);
        assert_eq!(
            validate_metadata_bytes(&bytes),
            [Problem::UnrepresentableNumber {
                pointer: pointer.to_owned()
            }],
            "{changed}"
        );
        assert!(
            matches!(
                tc_burrito::Metadata::from_slice(&bytes),
                Err(tc_burrito::Error::UnrepresentableNumber { pointer: at }) if at == pointer
            ),
            "{changed}"
        );
    }
}

#[test]
fn json_the_app_cannot_hold_is_refused_with_its_cause() {
    let pointer = "/type/flavorType/flavor/value";
    assert_eq!(
        validate_metadata_bytes(&with_raw_flavor_field(r#""broken \ud800 text""#)),
        [Problem::LoneSurrogate {
            pointer: pointer.to_owned()
        }]
    );
    assert_eq!(
        validate_metadata_bytes(&with_raw_flavor_field(r#""a pair \ud83d\ude00 is fine""#)),
        []
    );
    // The flavor object is four levels deep, so 96 more fit within the limit of 100.
    let nested = |levels: usize| format!("{}{}", "[".repeat(levels), "]".repeat(levels));
    assert_eq!(
        validate_metadata_bytes(&with_raw_flavor_field(&nested(96))),
        []
    );
    let deep = format!("{pointer}{}", "/0".repeat(96));
    for levels in [97, 300] {
        assert_eq!(
            validate_metadata_bytes(&with_raw_flavor_field(&nested(levels))),
            [Problem::TooDeep {
                pointer: deep.clone()
            }],
            "{levels}"
        );
    }
}

#[test]
fn hidden_and_backup_files_on_disk_are_not_unlisted_ingredients() {
    let root = tempfile::tempdir().unwrap();
    common::copy_dir(&common::bible_dir(), root.path());
    let ingredients = root.path().join("ingredients");
    std::fs::write(ingredients.join(".tmp-metadata.json"), b"partial").unwrap();
    std::fs::create_dir(ingredients.join(".cache")).unwrap();
    std::fs::write(ingredients.join(".cache/a"), b"x").unwrap();
    std::fs::write(ingredients.join("TIT.usfm.bak"), b"old").unwrap();
    std::fs::write(ingredients.join("checking/.DS_Store"), b"x").unwrap();
    assert_eq!(tc_burrito::validate_dir(root.path()), []);

    std::fs::write(ingredients.join("MAT.usfm"), b"\\id MAT\n").unwrap();
    assert_eq!(
        tc_burrito::validate_dir(root.path()),
        [Problem::UnlistedFile {
            path: "ingredients/MAT.usfm".to_owned()
        }]
    );
}

#[test]
fn a_file_and_a_folder_at_one_path_are_refused_in_memory() {
    let mut document = common::read_metadata(&common::bible_dir());
    add_ingredient_copy(&mut document, "ingredients/extra.json");
    add_ingredient_copy(&mut document, "ingredients/extra.json/inner.json");
    let mut files = bible_files();
    let tit = files.get(TIT).unwrap().to_vec();
    files.insert("ingredients/extra.json", tit.clone());
    files.insert("ingredients/extra.json/inner.json", tit);
    assert_eq!(
        project_problems(&document, &files),
        [code(Problem::PathIsAlsoFolder {
            file: "ingredients/extra.json".to_owned(),
            inside: "ingredients/extra.json/inner.json".to_owned(),
        })]
    );
}

#[test]
fn problems_read_back_from_json() {
    let problems = vec![
        Problem::MissingMetadata,
        Problem::ByteOrderMark,
        Problem::UnrepresentableNumber {
            pointer: "/x".to_owned(),
        },
        Problem::UnsafePath {
            path: "c:/x".to_owned(),
            rule: PathRule::ForbiddenChar(':'),
        },
        Problem::InvalidTimestamp {
            field: TimestampField::DateCreated,
            value: "2026-07-02 12:00:00Z".to_owned(),
            reason: TimestampError::SpaceSeparator,
        },
        Problem::CopyrightNotExactlyOne {
            found: vec![CopyrightBasis::PublicDomain, CopyrightBasis::Licenses],
        },
        Problem::PathIsAlsoFolder {
            file: "a".to_owned(),
            inside: "a/b".to_owned(),
        },
        Problem::SizeMismatch {
            path: TIT.to_owned(),
            listed: 1,
            actual: u64::MAX,
        },
    ];
    let text = serde_json::to_string(&problems).unwrap();
    assert_eq!(
        serde_json::from_str::<Vec<Problem>>(&text).unwrap(),
        problems
    );
}

#[cfg(unix)]
#[test]
fn a_linked_metadata_file_is_not_followed() {
    let root = tempfile::tempdir().unwrap();
    common::copy_dir(&common::bible_dir(), root.path());
    let outside = tempfile::tempdir().unwrap();
    let metadata = root.path().join("metadata.json");
    std::fs::rename(&metadata, outside.path().join("metadata.json")).unwrap();
    std::os::unix::fs::symlink(outside.path().join("metadata.json"), &metadata).unwrap();
    assert_eq!(
        tc_burrito::validate_dir(root.path()),
        [Problem::NotAFile {
            path: "metadata.json".to_owned()
        }]
    );
    std::fs::remove_file(&metadata).unwrap();
    std::fs::create_dir(&metadata).unwrap();
    assert_eq!(
        tc_burrito::validate_dir(root.path()),
        [Problem::NotAFile {
            path: "metadata.json".to_owned()
        }]
    );
}

#[cfg(unix)]
#[test]
fn links_and_folders_are_not_ingredients_on_disk() {
    let root = tempfile::tempdir().unwrap();
    common::copy_dir(&common::bible_dir(), root.path());
    let outside = tempfile::tempdir().unwrap();
    let tit = std::fs::read(root.path().join(TIT)).unwrap();
    std::fs::write(outside.path().join("TIT.usfm"), &tit).unwrap();

    // The listed file is a link to an identical file outside the project.
    std::fs::remove_file(root.path().join(TIT)).unwrap();
    std::os::unix::fs::symlink(outside.path().join("TIT.usfm"), root.path().join(TIT)).unwrap();
    assert_eq!(
        tc_burrito::validate_dir(root.path()),
        [Problem::NotAFile {
            path: TIT.to_owned()
        }]
    );

    // A folder where the file should be.
    std::fs::remove_file(root.path().join(TIT)).unwrap();
    std::fs::create_dir(root.path().join(TIT)).unwrap();
    assert_eq!(
        tc_burrito::validate_dir(root.path()),
        [Problem::NotAFile {
            path: TIT.to_owned()
        }]
    );
    std::fs::remove_dir(root.path().join(TIT)).unwrap();
    std::fs::write(root.path().join(TIT), &tit).unwrap();
    assert_eq!(tc_burrito::validate_dir(root.path()), []);

    // A linked folder on the way to listed files: none of them is read through the link.
    let checking = root.path().join("ingredients/checking");
    std::fs::rename(&checking, outside.path().join("checking")).unwrap();
    std::os::unix::fs::symlink(outside.path().join("checking"), &checking).unwrap();
    let mut want: Vec<Problem> = [
        "alignments/TIT.json",
        "resources.json",
        "settings.json",
        "translationNotes/TIT.json",
        "translationWords/TIT.json",
    ]
    .iter()
    .map(|file| Problem::NotAFile {
        path: format!("ingredients/checking/{file}"),
    })
    .collect();
    want.push(Problem::UnlistedFile {
        path: "ingredients/checking".to_owned(),
    });
    assert_eq!(tc_burrito::validate_dir(root.path()), want);
}

#[test]
fn a_missing_metadata_file_is_reported() {
    let root = tempfile::tempdir().unwrap();
    assert_eq!(
        tc_burrito::validate_dir(root.path()),
        [Problem::MissingMetadata]
    );
}
