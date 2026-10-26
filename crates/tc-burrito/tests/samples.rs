//! The two tC4 reference projects validate clean, read into the model without loss, and
//! write back deterministically; a project built from scratch validates on disk.

// Helpers outside #[test] functions unwrap too; in a test, a panic is the failure report.
#![allow(
    clippy::unwrap_used,
    reason = "test code: a failed unwrap is a failed test"
)]

mod common;

use common::{bible_dir, fixture, obs_dir, read_metadata};
use serde_json::Value;
use std::path::Path;
use tc_burrito::{
    Copyright, Error, Flavor, FlavorType, Generator, IngredientPath, Language, LocalizedText,
    MemProject, Metadata, NewProject, PrimaryIdError, ProjectKind, Role, Scope, ScopeBook,
    ScriptDirection, TextTranslation, Timestamp, to_canonical_json, validate_dir,
    validate_metadata_bytes, validate_project,
};
use tc_core::BookId;

#[test]
fn both_tc4_samples_validate_clean() {
    for dir in [bible_dir(), obs_dir()] {
        assert_eq!(validate_dir(&dir), [], "{}", dir.display());
    }
}

#[test]
fn both_samples_round_trip_through_the_model() {
    for dir in [bible_dir(), obs_dir()] {
        let bytes = std::fs::read(dir.join("metadata.json")).unwrap();
        let original: Value = serde_json::from_slice(&bytes).unwrap();
        let model = Metadata::from_slice(&bytes).unwrap();
        assert_eq!(model.to_value().unwrap(), original, "{}", dir.display());
    }
}

#[test]
fn the_model_reads_what_the_app_needs() {
    let bible =
        Metadata::from_slice(&std::fs::read(bible_dir().join("metadata.json")).unwrap()).unwrap();
    assert_eq!(bible.kind(), Some(ProjectKind::BibleText));
    let primary = bible.primary().unwrap();
    assert_eq!((primary.authority, primary.id), ("local", "ejemplo_tj"));
    let scope = bible.current_scope().unwrap();
    let books: Vec<_> = scope.books().filter_map(ScopeBook::book).collect();
    assert_eq!(books, [BookId::Jonah, BookId::Titus]);
    let tit = &bible.ingredients[&IngredientPath::parse("ingredients/TIT.usfm").unwrap()];
    assert_eq!(tit.size, 1166);
    assert_eq!(
        tit.checksum.as_ref().unwrap().md5.to_string(),
        "1ce94f44dd0f1c4e87fa88a7a80f9216"
    );
    assert_eq!(
        bible.languages.as_ref().unwrap()[0].script_direction,
        Some(ScriptDirection::Ltr)
    );

    let obs =
        Metadata::from_slice(&std::fs::read(obs_dir().join("metadata.json")).unwrap()).unwrap();
    assert_eq!(obs.kind(), Some(ProjectKind::OpenBibleStories));
    let matthew = obs.current_scope().unwrap().get(BookId::Matthew).unwrap();
    assert_eq!(matthew[13].to_string(), "44-46");
}

fn bible() -> Metadata {
    Metadata::from_slice(&std::fs::read(bible_dir().join("metadata.json")).unwrap()).unwrap()
}

/// Where writing `model` finds a shadowed field, if it does.
fn shadowed(model: &Metadata) -> Option<String> {
    assert!(matches!(
        model.to_canonical_json(),
        Err(Error::ShadowedField { .. })
    ));
    match model.to_value() {
        Err(Error::ShadowedField { pointer }) => Some(pointer),
        _ => None,
    }
}

#[test]
fn an_extra_key_never_overrides_a_modelled_field() {
    let mut model = bible();
    model
        .extra
        .insert("ingredients".to_owned(), serde_json::json!({}));
    assert_eq!(shadowed(&model).as_deref(), Some("/ingredients"));

    let mut model = bible();
    model
        .extra
        .insert("confidential".to_owned(), serde_json::json!(true));
    assert_eq!(shadowed(&model).as_deref(), Some("/confidential"));

    let mut model = bible();
    let tit = IngredientPath::parse("ingredients/TIT.usfm").unwrap();
    let ingredient = model.ingredients.get_mut(&tit).unwrap();
    ingredient
        .extra
        .insert("size".to_owned(), serde_json::json!(999));
    assert_eq!(
        shadowed(&model).as_deref(),
        Some("/ingredients/ingredients~1TIT.usfm/size")
    );

    let mut model = bible();
    let Flavor::TextTranslation(flavor) = &mut model.burrito_type.flavor_type.flavor else {
        panic!("the Bible sample is textTranslation");
    };
    flavor
        .extra
        .insert("name".to_owned(), serde_json::json!("textStories"));
    assert_eq!(
        shadowed(&model).as_deref(),
        Some("/type/flavorType/flavor/name")
    );

    // A key the model does not know is written as given.
    let mut model = bible();
    let ingredient = model.ingredients.get_mut(&tit).unwrap();
    ingredient
        .extra
        .insert("x-note".to_owned(), serde_json::json!(1));
    let value = model.to_value().unwrap();
    assert_eq!(value["ingredients"]["ingredients/TIT.usfm"]["x-note"], 1);
    assert_eq!(value["ingredients"]["ingredients/TIT.usfm"]["size"], 1166);
}

#[test]
fn the_primary_id_is_missing_one_or_ambiguous() {
    let mut model = bible();
    let ids = model.identification.primary.get_mut("local").unwrap();
    let revision = ids["ejemplo_tj"].clone();
    ids.insert("otro_tj".to_owned(), revision);
    assert_eq!(model.primary(), Err(PrimaryIdError::Ambiguous));
    model.identification.primary.clear();
    assert_eq!(model.primary(), Err(PrimaryIdError::Missing));
}

#[test]
fn canonical_bytes_are_stable_and_read_back_equal() {
    for dir in [bible_dir(), obs_dir()] {
        let model =
            Metadata::from_slice(&std::fs::read(dir.join("metadata.json")).unwrap()).unwrap();
        let bytes = model.to_canonical_json().unwrap();
        assert_eq!(bytes, model.clone().to_canonical_json().unwrap());
        assert_eq!(bytes.last(), Some(&b'\n'));
        assert_eq!(Metadata::from_slice(&bytes).unwrap(), model);
        assert_eq!(bytes, to_canonical_json(&read_metadata(&dir)));
        assert_eq!(
            validate_project(&bytes, &tc_burrito::DirProject::new(&dir)),
            []
        );
    }
}

#[test]
fn canonical_bytes_are_pretty_and_sorted() {
    let model =
        Metadata::from_slice(&std::fs::read(obs_dir().join("metadata.json")).unwrap()).unwrap();
    let text = String::from_utf8(model.to_canonical_json().unwrap()).unwrap();
    assert!(text.starts_with("{\n  \"confidential\": false,\n  \"copyright\": {\n"));
    assert!(text.contains(
        "\"name\": {\n      \"en\": \"Equipo Ejemplo — Historias Bíblicas Abiertas\"\n    }"
    ));
}

#[test]
fn vendored_schema_matches_the_tc4_fixture() {
    let vendored = Path::new(env!("CARGO_MANIFEST_DIR")).join("schema/sb-1.0.0");
    let fixture = fixture("sb-schema");
    let list = |root: &Path| {
        let mut files = Vec::new();
        let mut pending = vec![root.to_path_buf()];
        while let Some(dir) = pending.pop() {
            for entry in std::fs::read_dir(dir).unwrap() {
                let path = entry.unwrap().path();
                if path.is_dir() {
                    pending.push(path);
                } else {
                    files.push(path.strip_prefix(root).unwrap().to_path_buf());
                }
            }
        }
        files.sort();
        files
    };
    let files = list(&vendored);
    assert_eq!(files, list(&fixture));
    for file in files {
        assert_eq!(
            std::fs::read(vendored.join(&file)).unwrap(),
            std::fs::read(fixture.join(&file)).unwrap(),
            "{}",
            file.display()
        );
    }
}

fn new_bible_project() -> Metadata {
    Metadata::new(NewProject {
        project_id: "nuevo_tit".to_owned(),
        name: LocalizedText::single("es-419", "Tito"),
        language: Language {
            tag: "es-419".to_owned(),
            name: LocalizedText::single("es-419", "Español"),
            script_direction: Some(ScriptDirection::Ltr),
            rod: None,
            numbering_system: None,
            extra: Default::default(),
        },
        default_locale: "en".to_owned(),
        flavor_type: FlavorType::text_translation(
            TextTranslation::new("3.0"),
            Scope::whole_books([BookId::Titus]),
        ),
        copyright: Copyright::statement("CC BY-SA 4.0, Equipo Nuevo"),
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

#[test]
fn a_new_project_with_recorded_ingredients_is_valid_on_disk() {
    let mut metadata = new_bible_project();
    let usfm = b"\\id TIT\n\\c 1\n\\v 1 Pablo\n".to_vec();
    let alignment = b"{}".to_vec();
    let tit = IngredientPath::parse("ingredients/TIT.usfm").unwrap();
    let aligned = IngredientPath::parse("ingredients/checking/alignments/TIT.json").unwrap();
    metadata.record_ingredient(tit.clone(), &usfm).scope =
        Some(Scope::whole_books([BookId::Titus]));
    let entry = metadata.record_ingredient(aligned.clone(), &alignment);
    entry.role = Some(Role::new("x-alignment"));
    entry.scope = Some(Scope::whole_books([BookId::Titus]));

    let root = tempfile::tempdir().unwrap();
    for (path, bytes) in [(&tit, &usfm), (&aligned, &alignment)] {
        let target = root.path().join(path.as_str());
        std::fs::create_dir_all(target.parent().unwrap()).unwrap();
        std::fs::write(target, bytes).unwrap();
    }
    std::fs::write(
        root.path().join("metadata.json"),
        metadata.to_canonical_json().unwrap(),
    )
    .unwrap();
    assert_eq!(validate_dir(root.path()), []);

    // An edit recorded with its new bytes keeps the burrito valid; one not recorded breaks it.
    let edited = b"\\id TIT\n\\c 1\n\\v 1 Pablo, siervo\n".to_vec();
    std::fs::write(root.path().join(tit.as_str()), &edited).unwrap();
    assert!(!validate_dir(root.path()).is_empty());
    metadata.record_ingredient(tit.clone(), &edited);
    std::fs::write(
        root.path().join("metadata.json"),
        metadata.to_canonical_json().unwrap(),
    )
    .unwrap();
    assert_eq!(validate_dir(root.path()), []);
    let tit_entry = &metadata.ingredients[&tit];
    assert_eq!(tit_entry.size, edited.len() as u64);
    assert!(
        tit_entry.scope.is_some(),
        "recording new bytes keeps the scope"
    );
}

#[test]
fn a_new_project_without_ingredients_is_refused() {
    let bytes = new_bible_project().to_canonical_json().unwrap();
    let problems = validate_metadata_bytes(&bytes);
    assert!(
        problems.iter().any(|problem| matches!(
            problem,
            tc_burrito::Problem::Schema { pointer, keyword, .. }
                if pointer == "/ingredients" && keyword == "minProperties"
        )),
        "{problems:?}"
    );
    assert!(problems.contains(&tc_burrito::Problem::UncoveredBook {
        book: "TIT".to_owned()
    }));
}

#[test]
fn an_obs_project_checks_in_memory_before_writing() {
    let mut metadata = new_bible_project();
    metadata.burrito_type.flavor_type =
        FlavorType::text_stories(Scope::whole_books([BookId::Genesis]));
    let mut files = MemProject::new();
    for (path, bytes) in [
        ("ingredients/content/01.md", "# 1. La creación\n".as_bytes()),
        ("ingredients/LICENSE.md", "CC BY-SA 4.0\n".as_bytes()),
    ] {
        let path = IngredientPath::parse(path).unwrap();
        let entry = metadata.record_ingredient(path.clone(), bytes);
        assert_eq!(entry.mime_type.as_str(), "text/markdown");
        files.insert(path, bytes);
    }
    let bytes = metadata.to_canonical_json().unwrap();
    assert_eq!(validate_project(&bytes, &files), []);
    assert_eq!(metadata.kind(), Some(ProjectKind::OpenBibleStories));
}
