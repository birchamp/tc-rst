//! The seed's private steps, on the samples of `fixtures/tc4`.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use super::check::{alignment_differences, decision_differences};
use super::records::{carried_record, meta_field, meta_fields, migrate_pins_v1, pins_records};
use super::*;
use serde_json::{Map, json};
use tc_journal::{Md5, MetaField, PinGroup, ResourcesKey, ResourcesLevel};

fn sample(dir: &str) -> Tc4Folder {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/tc4")
        .join(dir);
    Tc4Folder::read(&path).unwrap()
}

fn object(value: Value) -> Map<String, Value> {
    match value {
        Value::Object(map) => map,
        other => panic!("not an object: {other}"),
    }
}

#[test]
fn meta_split_covers_every_non_derived_field_once() {
    let meta = object(json!({
        "format": "scripture burrito",
        "meta": {
            "version": "1.0.0",
            "category": "source",
            "generator": {"softwareName": "x"},
            "normalization": "NFC",
            "defaultLocale": "en",
            "dateCreated": "2026-07-02T12:00:00.000Z",
            "comments": ["kept"]
        },
        "idAuthorities": {
            "local": {"id": "http://_local_"},
            "dcs": {"id": "https://git.door43.org"},
            "ebible": {"id": "https://ebible.org", "name": {"en": "eBible"}}
        },
        "identification": {
            "primary": {"local": {"x": {}}, "ebible": {"abc": {"revision": "1"}}},
            "upstream": {"dcs": [{"x": {}}]},
            "name": {"en": "N"},
            "abbreviation": {"en": "A"},
            "description": {"en": "D"}
        },
        "languages": [{"tag": "es-419"}],
        "copyright": {"shortStatements": [{"statement": "S"}]},
        "type": {"flavorType": {"name": "scripture"}},
        "confidential": false,
        "localizedNames": {"book-tit": {"short": {"en": "Tit"}}},
        "agencies": [{"id": "dcs::x"}],
        "relationships": [],
        "ingredients": {}
    }));
    let fields = meta_fields(&meta).unwrap();
    let paths: Vec<&str> = fields.iter().map(|(p, _)| p.as_str()).collect();
    assert_eq!(
        paths,
        [
            "meta.defaultLocale",
            "meta.dateCreated",
            "meta.comments",
            "idAuthorities.ebible",
            "identification.primary.ebible",
            "identification.upstream",
            "identification.name",
            "identification.abbreviation",
            "identification.description",
            "languages",
            "copyright",
            "confidential",
            "localizedNames",
            "agencies",
        ]
    );
    let records: Vec<MetaField> = paths.iter().map(|p| meta_field(p).unwrap()).collect();
    assert_eq!(records[6], MetaField::Name);
    assert_eq!(records[7], MetaField::Abbreviation);
    assert_eq!(records[8], MetaField::Description);
    assert_eq!(records[9], MetaField::Languages);
    assert_eq!(records[10], MetaField::Copyright);
    assert!(matches!(&records[3], MetaField::Extra(p) if p.as_str() == "idAuthorities.ebible"));
    // Each value once, at its own path: no field is split twice or left out.
    for (path, value) in &fields {
        let mut at = &Value::Object(meta.clone());
        for segment in path.split('.') {
            at = &at[segment];
        }
        assert_eq!(at, value, "{path}");
    }
    // A key with a dot would be stored at a path that names another place.
    let dotted = object(json!({"a.b": 1, "meta": {"defaultLocale": "en"}}));
    assert!(matches!(
        meta_fields(&dotted),
        Err(SeedRefusal::BadValue { .. })
    ));
    let not_object = object(json!({"identification": "x"}));
    assert!(matches!(
        meta_fields(&not_object),
        Err(SeedRefusal::BadValue { .. })
    ));
}

#[test]
fn resources_v1_migration_matches_burrito_spec_5_3() {
    let v1 = object(json!({
        "schemaVersion": 1,
        "gatewayLanguage": {"languageId": "en", "owner": "unfoldingWord"},
        "resources": {
            "translationNotes": {"repoPath": "git.door43.org/unfoldingWord/en_tn", "version": "v91", "sha": "e586762e330f482a60c52aedd1c7b3a2f155df8a", "flavor": "parascriptural/x-bcvnotes"},
            "translationWords": {"repoPath": "git.door43.org/unfoldingWord/en_tw", "version": "v91", "sha": "ff5b3852c27c3a0d01b109e482eb26047dcd20e2", "flavor": "parascriptural/x-bcvarticles"},
            "translationAcademy": {"repoPath": "git.door43.org/unfoldingWord/en_ta", "version": "v91", "sha": "ce9a1bb9431317ca888e8c1f9620caa7f5fe45fd", "flavor": "peripheral/x-peripheralArticles"},
            "originalLanguage": {"nt": {"repoPath": "git.door43.org/unfoldingWord/el-x-koine_ugnt", "version": "v0.34", "sha": "fc95b2b8aad08bb65ab54628ab685413a1139e97", "flavor": "scripture/textTranslation"}},
            "lexicon": {"nt": {"repoPath": "git.door43.org/unfoldingWord/en_ugl", "sha": "8fa6eb60c0fe7afa61a80264c7326d63db5f1e70", "flavor": "peripheral/x-lexicon"}}
        },
        "extraScripture": [{"id": "ult", "repoPath": "git.door43.org/unfoldingWord/en_ult", "sha": "35d215957f3203fd2e2fac5702ce14902d417f9d", "flavor": "scripture/textTranslation"}],
        "x-note": "kept"
    }));
    let set = json!({
        "gatewayLanguage": {"languageId": "en", "owner": "unfoldingWord"},
        "translationNotes": {"repoPath": "git.door43.org/unfoldingWord/en_tn", "version": "v91", "sha": "e586762e330f482a60c52aedd1c7b3a2f155df8a", "flavor": "parascriptural/x-bcvnotes"},
        "translationWordsLinks": {"repoPath": "git.door43.org/unfoldingWord/en_twl", "version": "v91", "sha": "7cbb3419f79780ce84b92ad7d42bd476cc5d8a64", "flavor": "parascriptural/x-bcvarticles"},
        "translationWords": {"repoPath": "git.door43.org/unfoldingWord/en_tw", "version": "v91", "sha": "ff5b3852c27c3a0d01b109e482eb26047dcd20e2", "flavor": "parascriptural/x-bcvarticles"},
        "translationAcademy": {"repoPath": "git.door43.org/unfoldingWord/en_ta", "version": "v91", "sha": "ce9a1bb9431317ca888e8c1f9620caa7f5fe45fd", "flavor": "peripheral/x-peripheralArticles"}
    });
    let expected = json!({
        "schemaVersion": 2,
        "languageSets": {"primary": set, "fallback": set},
        "resources": {
            "originalLanguage": {"nt": {"repoPath": "git.door43.org/unfoldingWord/el-x-koine_ugnt", "version": "v0.34", "sha": "fc95b2b8aad08bb65ab54628ab685413a1139e97", "flavor": "scripture/textTranslation"}},
            "lexicon": {"nt": {"repoPath": "git.door43.org/unfoldingWord/en_ugl", "sha": "8fa6eb60c0fe7afa61a80264c7326d63db5f1e70", "flavor": "peripheral/x-lexicon"}}
        },
        "extraScripture": [{"id": "ult", "repoPath": "git.door43.org/unfoldingWord/en_ult", "sha": "35d215957f3203fd2e2fac5702ce14902d417f9d", "flavor": "scripture/textTranslation"}],
        "x-note": "kept"
    });
    let path = "ingredients/checking/resources.json";
    assert_eq!(migrate_pins_v1(path, &v1).unwrap(), expected);
    // A links pin the file has is kept as it is, whatever it pins.
    let mut with_links = v1.clone();
    let links = json!({"repoPath": "git.door43.org/unfoldingWord/en_tw", "sha": "ff5b3852c27c3a0d01b109e482eb26047dcd20e2"});
    with_links["resources"]["translationWordsLinks"] = links.clone();
    let migrated = migrate_pins_v1(path, &with_links).unwrap();
    assert_eq!(
        migrated["languageSets"]["fallback"]["translationWordsLinks"],
        links
    );
    // The gateway language may stand under `resources`; two different ones, or none,
    // refuse: no v1 file exists to say which one a writer meant.
    let mut inner = v1.clone();
    let gateway = inner.remove("gatewayLanguage").unwrap();
    inner["resources"]["gatewayLanguage"] = gateway;
    assert_eq!(migrate_pins_v1(path, &inner).unwrap(), expected);
    let mut two = v1.clone();
    two["resources"]["gatewayLanguage"] = json!({"languageId": "fr"});
    assert!(matches!(
        migrate_pins_v1(path, &two),
        Err(SeedRefusal::BadValue { .. })
    ));
    let mut none = v1.clone();
    none.remove("gatewayLanguage");
    assert!(matches!(
        migrate_pins_v1(path, &none),
        Err(SeedRefusal::BadValue { .. })
    ));
    // The migrated document seeds the pin groups and keeps the rest.
    let seeded = pins_records(path, &Value::Object(v1)).unwrap();
    assert_eq!(seeded.expected, expected);
    assert_eq!(
        seeded.declared,
        Some(DeclaredDifference::ResourcesMigrated { from: 1 })
    );
    let keys: Vec<RecordKey> = seeded.records.iter().map(Record::key).collect();
    assert!(keys.contains(&RecordKey::Pins(PinGroup::Primary)));
    assert!(keys.contains(&RecordKey::Pins(PinGroup::Fallback)));
    assert!(keys.contains(&RecordKey::Pins(PinGroup::ExtraScripture)));
    assert!(keys.contains(&RecordKey::ResourcesExtra(
        ResourcesKey::new(ResourcesLevel::Top, "x-note").unwrap()
    )));
}

#[test]
fn obs_stories_are_kept_as_the_files_are() {
    let folder = sample("sample-burrito-obs");
    let built = build(&folder, fixture_project_id(folder.name())).unwrap();
    assert_eq!(built.kind, ProjectKind::Stories);
    let mut bases = 0;
    for batch in &built.batches {
        for change in &batch.changes {
            match &change.body().record {
                Record::StoryBase { story, value } => {
                    bases += 1;
                    let path = DerivedFile::Story(*story).path();
                    assert_eq!(value.as_bytes(), folder.files()[&path].as_slice(), "{path}");
                    assert_eq!(
                        built.fold.render(&DerivedFile::Story(*story)).unwrap(),
                        folder.files()[&path],
                        "{path}"
                    );
                    assert_eq!(batch.label, BatchLabel::Story(*story));
                }
                Record::Frame { .. } | Record::StoryRef { .. } => {
                    panic!("a story is seeded as its file, never as frames")
                }
                _ => {}
            }
        }
    }
    assert_eq!(bases, 50);
    // The licence and the front and back matter are carried, by their git blob ids.
    let placements = built.fold.carried_files();
    let mut carried: Vec<&str> = placements.iter().map(|p| p.path.as_str()).collect();
    carried.sort_unstable();
    assert_eq!(
        carried,
        [
            "ingredients/LICENSE.md",
            "ingredients/content/back/intro.md",
            "ingredients/content/front/intro.md",
            "ingredients/content/front/title.md",
        ]
    );
    for placement in &placements {
        let bytes = &folder.files()[&placement.path];
        assert_eq!(placement.size, bytes.len() as u64);
        assert_eq!(placement.md5, Md5::of(bytes));
        assert_eq!(built.carried[&placement.oid], *bytes);
    }
    // `git hash-object ingredients/content/front/title.md` in the sample.
    let title = placements
        .iter()
        .find(|p| p.path.ends_with("front/title.md"))
        .unwrap();
    assert_eq!(
        title.oid.to_hex(),
        git_blob(&folder.files()["ingredients/content/front/title.md"])
    );
    let changes: usize = built.batches.iter().map(|b| b.changes.len()).sum();
    assert_eq!(
        changes, 66,
        "9 fields, 2 pin groups, 1 setting, 4 carried, 50 stories"
    );
}

/// The git blob id of `bytes`, as `git hash-object` gives it, computed here independently
/// of the seed with a hand-written SHA-1 so the test does not share the seed's code.
fn git_blob(bytes: &[u8]) -> String {
    let mut message = format!("blob {}\0", bytes.len()).into_bytes();
    message.extend_from_slice(bytes);
    let bit_len = (message.len() as u64) * 8;
    message.push(0x80);
    while message.len() % 64 != 56 {
        message.push(0);
    }
    message.extend_from_slice(&bit_len.to_be_bytes());
    let mut h: [u32; 5] = [
        0x6745_2301,
        0xEFCD_AB89,
        0x98BA_DCFE,
        0x1032_5476,
        0xC3D2_E1F0,
    ];
    for chunk in message.chunks(64) {
        let mut w = [0u32; 80];
        for i in 0..16 {
            w[i] = u32::from_be_bytes([
                chunk[4 * i],
                chunk[4 * i + 1],
                chunk[4 * i + 2],
                chunk[4 * i + 3],
            ]);
        }
        for i in 16..80 {
            w[i] = (w[i - 3] ^ w[i - 8] ^ w[i - 14] ^ w[i - 16]).rotate_left(1);
        }
        let [mut a, mut b, mut c, mut d, mut e] = h;
        for (i, word) in w.iter().enumerate() {
            let (f, k) = match i {
                0..=19 => ((b & c) | (!b & d), 0x5A82_7999),
                20..=39 => (b ^ c ^ d, 0x6ED9_EBA1),
                40..=59 => ((b & c) | (b & d) | (c & d), 0x8F1B_BCDC),
                _ => (b ^ c ^ d, 0xCA62_C1D6),
            };
            let t = a
                .rotate_left(5)
                .wrapping_add(f)
                .wrapping_add(e)
                .wrapping_add(k)
                .wrapping_add(*word);
            e = d;
            d = c;
            c = b.rotate_left(30);
            b = a;
            a = t;
        }
        for (x, y) in h.iter_mut().zip([a, b, c, d, e]) {
            *x = x.wrapping_add(y);
        }
    }
    h.iter().map(|x| format!("{x:08x}")).collect()
}

#[test]
fn git_blob_is_git_s_hash() {
    // `printf 'hello\n' | git hash-object --stdin`.
    assert_eq!(
        git_blob(b"hello\n"),
        "ce013625030ba8dba906f756967f9e9ca394464a"
    );
    let (_, oid) = carried_record("ingredients/a.txt", b"hello\n").unwrap();
    assert_eq!(oid.to_hex(), "ce013625030ba8dba906f756967f9e9ca394464a");
}

#[test]
fn decision_reading_applies_tc4_defaults() {
    let path = "ingredients/checking/translationNotes/TIT.json";
    let base = json!({
        "contextId": {"checkId": "x", "reference": {"bookId": "tit", "chapter": 1, "verse": 1}, "occurrence": 1},
        "category": "translate",
        "selections": [{"text": "fe", "occurrence": 1, "occurrences": 1}],
        "comments": false,
        "reminders": false,
        "nothingToSelect": false,
        "verseEdits": false,
        "invalidated": false,
        "modifiedTimestamp": "2026-07-02T15:02:11.000Z"
    });
    let read_equal = |a: Value, b: Value| {
        let (differences, _) = decision_differences(path, "x", &object(a), &object(b));
        differences
    };
    let with = |edit: &dyn Fn(&mut Map<String, Value>)| {
        let mut m = object(base.clone());
        edit(&mut m);
        Value::Object(m)
    };
    // Pairs tC4 reads alike.
    let alike = [
        (
            with(&|m| {
                m.remove("comments");
            }),
            base.clone(),
        ),
        (
            with(&|m| {
                m.remove("reminders");
                m.remove("nothingToSelect");
                m.remove("invalidated");
            }),
            base.clone(),
        ),
        (
            with(&|m| {
                m.insert("selections".into(), json!([]));
            }),
            with(&|m| {
                m.insert("selections".into(), json!(false));
            }),
        ),
        // An absent status is derived: selections make it valid.
        (
            base.clone(),
            with(&|m| {
                m.insert("status".into(), json!("valid"));
            }),
        ),
        // Invalidated and no status reads invalid; tC4 reads `valid` there as `invalid`.
        (
            with(&|m| {
                m.insert("invalidated".into(), json!(true));
            }),
            with(&|m| {
                m.insert("invalidated".into(), json!(true));
                m.insert("status".into(), json!("invalid"));
            }),
        ),
        (
            with(&|m| {
                m.insert("invalidated".into(), json!(true));
                m.insert("status".into(), json!("valid"));
            }),
            with(&|m| {
                m.insert("invalidated".into(), json!(true));
                m.insert("status".into(), json!("invalid"));
            }),
        ),
        (
            with(&|m| {
                m.remove("userInvalid");
            }),
            with(&|m| {
                m.insert("userInvalid".into(), json!(false));
            }),
        ),
        // The journal's own fields count only where the source has them.
        (
            base.clone(),
            with(&|m| {
                m.insert("userStatus".into(), json!("valid"));
                m.insert(
                    "verseTextMd5".into(),
                    json!("00000000000000000000000000000000"),
                );
                m.insert("importedInvalid".into(), json!(true));
            }),
        ),
    ];
    for (a, b) in alike {
        assert_eq!(read_equal(a.clone(), b.clone()), vec![], "{a} / {b}");
    }
    // Pairs tC4 reads apart.
    let apart: [(Value, Value, &str); 7] = [
        (
            base.clone(),
            with(&|m| {
                m.insert("selections".into(), json!(false));
            }),
            "selections",
        ),
        // An absent `selections` is a selection to tC4's `isDecided`; `false` is none.
        (
            with(&|m| {
                m.remove("selections");
            }),
            with(&|m| {
                m.insert("selections".into(), json!(false));
            }),
            "selections",
        ),
        (
            with(&|m| {
                m.insert("selections".into(), json!(false));
            }),
            with(&|m| {
                m.insert("selections".into(), json!(false));
                m.insert("status".into(), json!("valid"));
            }),
            "status",
        ),
        (
            base.clone(),
            with(&|m| {
                m.insert("comments".into(), json!("una nota"));
            }),
            "comments",
        ),
        (
            base.clone(),
            with(&|m| {
                m.insert("invalidated".into(), json!(true));
            }),
            "invalidated",
        ),
        (
            with(&|m| {
                m.insert("x-note".into(), json!(1));
            }),
            base.clone(),
            "x-note",
        ),
        (
            with(&|m| {
                m.insert(
                    "modifiedTimestamp".into(),
                    json!("2026-07-02T15:02:12.000Z"),
                );
            }),
            base.clone(),
            "modifiedTimestamp",
        ),
    ];
    for (a, b, field) in apart {
        let differences = read_equal(a.clone(), b.clone());
        assert!(
            differences.iter().any(|(f, ..)| f == field),
            "{field}: {a} / {b} gave {differences:?}"
        );
    }
    // The same instant in another form, and a recomputed verseEdits, are declared.
    let (differences, declared) = decision_differences(
        path,
        "x",
        &object(with(&|m| {
            m.insert("modifiedTimestamp".into(), json!("2026-07-02T15:02:11Z"));
            m.insert("verseEdits".into(), json!(true));
        })),
        &object(base.clone()),
    );
    assert_eq!(differences, vec![]);
    assert_eq!(declared.len(), 2, "{declared:?}");
}

#[test]
fn alignment_reading_applies_i3() {
    let now = "11111111111111111111111111111111";
    let other = "22222222222222222222222222222222";
    let record = |md5: &str, invalid: Option<bool>| {
        let mut m = object(json!({
            "alignments": [], "wordBank": [], "targetVerseMd5": md5, "sourceVersion": "v"
        }));
        if let Some(invalid) = invalid {
            m.insert("invalid".into(), Value::Bool(invalid));
        }
        m
    };
    let fields = |a: &Map<String, Value>, b: &Map<String, Value>, now: Option<&str>| {
        alignment_differences(a, b, now)
            .into_iter()
            .map(|(f, ..)| f)
            .collect::<Vec<_>>()
    };
    // A hash that is not the verse's is invalid, whatever the flag says.
    assert_eq!(
        fields(
            &record(other, Some(false)),
            &record(other, Some(true)),
            Some(now)
        ),
        Vec::<String>::new()
    );
    assert_eq!(
        fields(&record(now, None), &record(now, Some(false)), Some(now)),
        Vec::<String>::new()
    );
    assert_eq!(
        fields(
            &record(now, Some(false)),
            &record(now, Some(true)),
            Some(now)
        ),
        ["invalid"]
    );
    // With no verse text to hash, the flags compare as written.
    assert_eq!(
        fields(
            &record(other, Some(false)),
            &record(other, Some(true)),
            None
        ),
        ["invalid"]
    );
    // `done` reads absent as false; the journal's `markedInvalid` counts only where the
    // source has it; any other field counts both ways.
    let mut done = record(now, Some(false));
    done.insert("done".into(), json!(false));
    assert_eq!(
        fields(&record(now, None), &done, Some(now)),
        Vec::<String>::new()
    );
    let mut marked = record(now, Some(true));
    marked.insert("markedInvalid".into(), json!(true));
    assert_eq!(
        fields(&record(now, Some(true)), &marked, Some(now)),
        Vec::<String>::new()
    );
    let mut extra = record(now, None);
    extra.insert("x-note".into(), json!("n"));
    assert_eq!(fields(&record(now, None), &extra, Some(now)), ["x-note"]);
    // Word order is the translator's work.
    let mut a = record(now, None);
    a.insert("wordBank".into(), json!([{"word": "a"}, {"word": "b"}]));
    let mut b = record(now, None);
    b.insert("wordBank".into(), json!([{"word": "b"}, {"word": "a"}]));
    assert_eq!(fields(&a, &b, Some(now)), ["wordBank"]);
}

/// The Bible sample's built seed, and its check before any change.
fn bible() -> (Tc4Folder, Built) {
    let folder = sample("sample-burrito");
    let built = build(&folder, fixture_project_id("sample_burrito")).unwrap();
    assert!(check::check(&folder, &built).mismatches.is_empty());
    (folder, built)
}

#[test]
fn round_trip_rule_finds_a_record_the_projection_does_not_give_back() {
    // A seeded alignment the fold never saw: decoding the projected sidecar gives the record
    // the fold holds, so the check names the difference.
    let (folder, mut built) = bible();
    let path = "ingredients/checking/alignments/TIT.json";
    let records = built.sidecar_records.get_mut(path).unwrap();
    let Record::Alignment { value, .. } = &mut records[0] else {
        panic!("not an alignment");
    };
    value.dismissed = true;
    let found = check::check(&folder, &built).mismatches;
    let rules: Vec<(&str, CheckRule)> = found.iter().map(|m| (m.path.as_str(), m.rule)).collect();
    assert_eq!(rules, [(path, CheckRule::RoundTrip)], "{found:?}");
    assert_eq!(found[0].record.as_deref(), Some("alignment TIT 1:1"));
}

#[test]
fn a_seeded_change_the_fold_does_not_show_is_a_mismatch() {
    let (folder, mut built) = bible();
    let batch = built
        .batches
        .iter_mut()
        .find(|b| b.label == BatchLabel::Book(BookId::Titus))
        .unwrap();
    let at = batch
        .changes
        .iter()
        .position(|c| matches!(c.body().record, Record::Verse { .. }))
        .unwrap();
    let mut body = batch.changes[at].body().clone();
    if let Record::Verse {
        value: tc_journal::record::VerseValue::Text { words, .. },
        ..
    } = &mut body.record
    {
        words.push_str(" (never folded)");
    }
    batch.changes[at] = Change::new(body).unwrap();
    let found = check::check(&folder, &built).mismatches;
    let rules: Vec<CheckRule> = found.iter().map(|m| m.rule).collect();
    assert_eq!(rules, [CheckRule::NotProvisional], "{found:?}");
    assert_eq!(found[0].path, "ingredients/TIT.usfm");
}

#[test]
fn a_derived_file_the_fold_does_not_list_is_a_mismatch() {
    // The file-set rule alone sees what a materialize would drop: an OBS project's
    // settings.json renders, but the fold does not list it among its files.
    let folder = sample("sample-burrito-obs");
    let built = build(&folder, fixture_project_id("sample_burrito_obs")).unwrap();
    let path = "ingredients/checking/settings.json";
    assert!(built.fold.render(&DerivedFile::Settings).is_some());
    assert!(!built.fold.derived_files().contains(&DerivedFile::Settings));
    let found = check::check(&folder, &built).mismatches;
    let at_settings: Vec<CheckRule> = found
        .iter()
        .filter(|m| m.path == path)
        .map(|m| m.rule)
        .collect();
    assert_eq!(at_settings, [CheckRule::FileNotDerived]);
}

/// The OBS sample's built seed with one edit of story 1 by a person on this computer, and
/// story 1's file as the fold then projects it.
fn obs_story_one_after(frame: u16, text: &str) -> (String, String) {
    let folder = sample("sample-burrito-obs");
    let built = build(&folder, fixture_project_id("sample_burrito_obs")).unwrap();
    let story = StoryNo::new(1).unwrap();
    let base = String::from_utf8(folder.files()["ingredients/content/01.md"].clone()).unwrap();
    let edit = Change::new(ChangeBody {
        v: FORMAT_MINOR,
        record: Record::Frame {
            story,
            frame: tc_journal::FrameNo(frame),
            value: text.to_owned(),
        },
        parents: Vec::new(),
        device: DeviceId::local([7; 15]),
        seq: DeviceSeq(1),
        at: WallTime(1_790_000_000_000),
        batch: BatchRef::new(BatchId::from_bytes([9; 16]), 1).unwrap(),
        cause: Cause::Edit,
    })
    .unwrap();
    let mut fold = built.fold.clone();
    fold.add([FoldInput::from(edit)]);
    let projected = String::from_utf8(fold.render(&DerivedFile::Story(story)).unwrap()).unwrap();
    (base, projected)
}

/// Storage findings outside the seed check (docs/evidence/state-seed-check-2026-10-18.md,
/// part 5), asserted as they stand today so this test fails the day storage changes them.
#[test]
fn obs_frame_edits_project_other_bytes_than_tc4_writes_today() {
    // tC4's title box sends the title without the number; the fold writes `# ` and the text,
    // so the number is lost and tC4's TITLE_RE no longer reads the line as a title.
    let (base, projected) = obs_story_one_after(0, "La Creación");
    let tc4 = tc_obs::story::write_title(&base, "La Creación").unwrap();
    assert_eq!(tc4.lines().next(), Some("# 1. La Creación"));
    assert_eq!(projected.lines().next(), Some("# La Creación"));
    assert!(tc_obs::StoryFile::parse(projected.clone()).is_err());
    // An edit of an empty frame: the fold keeps the template's blank lines and adds the
    // text after them; tC4's writer puts one blank line on each side.
    let (base, projected) = obs_story_one_after(3, "Texto nuevo.");
    let tc4 = tc_obs::story::write_frame(&base, tc_obs::FrameNo(3), "Texto nuevo.").unwrap();
    let image3 = "![OBS Image](https://cdn.door43.org/obs/jpg/360px/obs-en-01-03.jpg)";
    let image4 = "![OBS Image](https://cdn.door43.org/obs/jpg/360px/obs-en-01-04.jpg)";
    let between = |text: &str| -> String {
        let start = text.find(image3).unwrap() + image3.len();
        let end = text.find(image4).unwrap();
        text[start..end].to_owned()
    };
    assert_eq!(between(&tc4), "\n\nTexto nuevo.\n\n");
    assert_eq!(between(&projected), "\n\n\n\nTexto nuevo.\n");
    assert_ne!(projected, tc4);
    // tC4 still reads the frame's text there, but not in its writers' layout: reading the
    // file back, as an outside edit is read, names frame 3 as reformatted.
    let read = tc_obs::StoryFile::parse(projected.clone()).unwrap();
    assert_eq!(read.text(tc_obs::FrameNo(3)), Some("Texto nuevo."));
    let decoded = tc_obs::decode_story(&projected).unwrap();
    assert!(
        decoded
            .reformatted
            .contains(&tc_obs::Reformatted::Frame(tc_obs::FrameNo(3))),
        "{:?}",
        decoded.reformatted
    );
}
