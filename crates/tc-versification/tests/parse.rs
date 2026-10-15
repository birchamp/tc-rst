//! Parsing and writing real scheme files.

// Test helpers fail the test by panicking.
#![allow(clippy::unwrap_used, clippy::panic)]

use serde_json::Value;
use std::path::PathBuf;
use tc_versification::{
    BookCode, StandardScheme, UnappliedForm, VerseRef, Versification, canonical_scheme,
};

fn read(rel: &str) -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures")
        .join(rel);
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

fn book(code: &str) -> BookCode {
    BookCode::new(code).unwrap()
}

const VRS_FIXTURES: [&str; 10] = [
    "proskomma/eng",
    "proskomma/lxx",
    "proskomma/org",
    "proskomma/rsc",
    "proskomma/rso",
    "proskomma/vul",
    "proskomma-tests/douay_rheims",
    "proskomma-tests/rsc",
    "proskomma-tests/truncated_versification",
    "proskomma-tests/webbe",
];

#[test]
fn bundled_schemes_are_the_fixture_bytes() {
    for which in StandardScheme::ALL {
        assert_eq!(
            which.json(),
            read(&format!("tc4/test-fixtures/vrs/{}.json", which.name()))
        );
    }
    // The platform writes eng into each project as ingredients/vrs.json.
    assert_eq!(
        StandardScheme::Eng.json(),
        read("tc4/sample-burrito/ingredients/vrs.json")
    );
    assert_eq!(
        StandardScheme::Eng.json(),
        read("tc4/conformance-fixtures/vrs/eng.json")
    );
    assert_eq!(StandardScheme::DEFAULT, StandardScheme::Eng);
}

#[test]
fn a_project_vrs_json_is_the_eng_scheme() {
    let project =
        Versification::from_json(&read("tc4/sample-burrito/ingredients/vrs.json")).unwrap();
    assert_eq!(&project, StandardScheme::Eng.scheme().versification());
    // The project's scope books are covered.
    for code in ["TIT", "JON"] {
        assert!(project.verse_counts(book(code)).is_some());
    }
}

#[test]
fn verse_count_queries() {
    let eng = StandardScheme::Eng.scheme().versification();
    let rsc = StandardScheme::Rsc.scheme().versification();
    assert_eq!(eng.books().count(), 92);
    assert_eq!(eng.mappings().len(), 265);
    assert_eq!(eng.last_chapter(book("PSA")), Some(150));
    assert_eq!(eng.last_verse(book("PSA"), 119), Some(176));
    // JON 1 tells eng apart from every other scheme.
    assert_eq!(eng.last_verse(book("JON"), 1), Some(17));
    assert_eq!(rsc.last_verse(book("JON"), 1), Some(16));
    assert_eq!(rsc.verse_counts(book("TOB")), None);
    assert!(eng.contains("3JN 1:15".parse::<VerseRef>().unwrap()));

    let syn = Versification::from_json(&read("tc4/test-fixtures/vrs-synthetic/syn.json")).unwrap();
    assert_eq!(syn.books().collect::<Vec<_>>(), vec![book("TIT")]);
    assert_eq!(syn.verse_counts(book("TIT")), Some([15, 16, 14].as_slice()));
    assert_eq!(syn.mappings().len(), 2);
    assert_eq!(syn.mappings()[1].to_string(), "TIT 2:2-16 = TIT 2:1-15");
}

#[test]
fn the_json_writer_reproduces_every_standard_file() {
    for which in StandardScheme::ALL {
        let original: Value = serde_json::from_str(which.json()).unwrap();
        let written = which.scheme().versification().to_json().unwrap();
        assert_eq!(
            serde_json::from_str::<Value>(&written).unwrap(),
            original,
            "{}",
            which.name()
        );
        assert_eq!(canonical_scheme(&written), canonical_scheme(which.json()));
    }
}

#[test]
fn the_json_writer_keeps_what_tc4_fingerprints() {
    // tC4 resolves a project's scheme from canonical_scheme of its vrs.json, so writing a
    // scheme back must not add, drop or change a key.
    let eng: Value = serde_json::from_str(StandardScheme::Eng.json()).unwrap();
    let mut doc = serde_json::Map::new();
    doc.insert("basedOn".into(), "org".into());
    doc.insert("maxVerses".into(), eng["maxVerses"].clone());
    doc.insert("mappedVerses".into(), serde_json::json!({}));
    doc.insert(
        "mergedVerses".into(),
        serde_json::json!({"ACT 19:40-41": "ACT 19:40"}),
    );
    doc.insert("verification".into(), serde_json::json!({"note": [1, 2.5]}));
    let text = Value::Object(doc).to_string();
    let v = Versification::from_json(&text).unwrap();
    let written = v.to_json().unwrap();
    assert_eq!(canonical_scheme(&written), canonical_scheme(&text));
    assert_eq!(Versification::from_json(&written).unwrap(), v);
}

#[test]
fn real_files_round_trip_through_both_writers() {
    let mut schemes: Vec<(String, Versification)> = StandardScheme::ALL
        .into_iter()
        .map(|s| (s.name().to_owned(), s.scheme().versification().clone()))
        .collect();
    for rel in VRS_FIXTURES {
        let v = Versification::from_vrs(&read(&format!("versification/vrs/{rel}.vrs"))).unwrap();
        schemes.push((rel.to_owned(), v));
    }
    let mut refused = Vec::new();
    for (name, v) in schemes {
        match v.to_json() {
            Ok(text) => assert_eq!(Versification::from_json(&text).unwrap(), v, "{name} json"),
            Err(e) => refused.push(format!("{name} json: {e}")),
        }
        match v.to_vrs() {
            Ok(text) => assert_eq!(Versification::from_vrs(&text).unwrap(), v, "{name} vrs"),
            Err(e) => refused.push(format!("{name} vrs: {e}")),
        }
    }
    // A writer refuses only what its format cannot hold. eng.vrs has a mapping Proskomma
    // does not apply, which vrs.json has no place for. lxx.json's partial-verse segments
    // end in a space ("a "), which a trimmed .vrs line loses.
    assert_eq!(
        refused,
        vec![
            "lxx vrs: partial verse EXO 28:29: a .vrs line cannot hold the segment \"a \" (a comma, a line break, or white space a line loses when it is trimmed)".to_owned(),
            "proskomma/eng json: vrs.json has no place for a mapping Proskomma does not apply: `#! &ACT 19:40-41 = ACT 19:40`".to_owned(),
        ]
    );
}

#[test]
fn hidden_vrs_lines_are_data() {
    // lxx.vrs gives its partial verses behind `#!`, which lxx.json was made from.
    let lxx_vrs = Versification::from_vrs(&read("versification/vrs/proskomma/lxx.vrs")).unwrap();
    let lxx_json = StandardScheme::Lxx.scheme().versification();
    assert_eq!(lxx_vrs.partial_verses().len(), 74);
    // lxx.json keeps the space that ends each .vrs line; a .vrs reader trims it.
    let trimmed: Vec<(VerseRef, Vec<String>)> = lxx_json
        .partial_verses()
        .iter()
        .map(|(verse, parts)| {
            let parts = parts.iter().map(|p| p.trim_end().to_owned()).collect();
            (*verse, parts)
        })
        .collect();
    assert_eq!(lxx_vrs.partial_verses(), trimmed.as_slice());
    assert_eq!(lxx_vrs.excluded_verses().len(), 304);
    assert_eq!(lxx_json.excluded_verses().len(), 0);

    // webbe.vrs excludes seven verses behind `#!`.
    let webbe =
        Versification::from_vrs(&read("versification/vrs/proskomma-tests/webbe.vrs")).unwrap();
    let excluded: Vec<String> = webbe
        .excluded_verses()
        .iter()
        .map(ToString::to_string)
        .collect();
    assert_eq!(
        excluded,
        [
            "ROM 16:26",
            "ROM 16:27",
            "3JN 1:15",
            "S3Y 1:49",
            "ESG 4:6",
            "ESG 9:5",
            "ESG 9:30"
        ]
    );

    // eng.vrs's `#! &ACT 19:40-41 = ACT 19:40` is kept and not applied.
    let eng = Versification::from_vrs(&read("versification/vrs/proskomma/eng.vrs")).unwrap();
    let [act] = eng.unapplied_mappings() else {
        panic!("{:?}", eng.unapplied_mappings())
    };
    assert_eq!(act.form(), UnappliedForm::Ampersand);
    assert_eq!(act.source().to_string(), "ACT 19:40-41");
    assert_eq!(act.target().to_string(), "ACT 19:40");
    assert!(
        eng.mappings()
            .iter()
            .all(|m| m.source().to_string() != "ACT 19:40-41")
    );
    for rel in VRS_FIXTURES {
        let v = Versification::from_vrs(&read(&format!("versification/vrs/{rel}.vrs"))).unwrap();
        let expected = usize::from(rel == "proskomma/eng");
        assert_eq!(v.unapplied_mappings().len(), expected, "{rel}");
    }
}

#[test]
fn proskommas_vrs_copies_differ_from_the_json_as_documented() {
    let vrs = Versification::from_vrs(&read("versification/vrs/proskomma/eng.vrs")).unwrap();
    let json = StandardScheme::Eng.scheme().versification();
    let psa51 = |v: &Versification| {
        v.mappings()
            .iter()
            .find(|m| m.source().to_string() == "PSA 51:0")
            .map(ToString::to_string)
    };
    assert_eq!(
        psa51(&vrs).as_deref(),
        Some("PSA 51:0 = PSA 51:1, PSA 51:2")
    );
    assert_eq!(psa51(json).as_deref(), Some("PSA 51:0 = PSA 51:2"));
}

#[test]
fn malformed_files_are_refused_with_a_reason() {
    let err = Versification::from_json(
        r#"{"maxVerses":{"TIT":["15"]},"mappedVerses":{"TIT 2:1":"TIT 1:x"}}"#,
    )
    .unwrap_err();
    assert!(err.to_string().contains("TIT 1:x"), "{err}");
    let err = Versification::from_vrs("TIT 1:15\nTIT 2:1 = TIT 1:16 extra\n").unwrap_err();
    assert!(err.to_string().starts_with("line 2:"), "{err}");
    assert!(Versification::from_json("not json").is_err());
}
