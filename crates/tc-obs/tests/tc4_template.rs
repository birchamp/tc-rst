//! The OBS template and a new project: ports of tC4's `test/obsTemplateServed.test.ts`
//! (#347) and `test/obsCreate.test.ts` (#287, J20), and the rules behind `NewObs.jsx`.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use std::collections::BTreeMap;

use common::{fixture, repo_path, stories};
use serde_json::{Value, json};
use tc_obs::ids::is_obs_metadata;
use tc_obs::story::parse_story;
use tc_obs::template::{
    NewObsProject, TEMPLATE_FILES, create_obs_project, template_current_scope, template_metadata,
    validate_template, with_localized_names,
};
use tc_obs::{Error, NewProjectError, StoryNo, TemplateError};

/// tC4's pinned copy of the template, by path below `text_stories/`.
fn reference() -> BTreeMap<String, Vec<u8>> {
    let root = repo_path("fixtures/tc4/conformance-fixtures/text_stories");
    let mut out = BTreeMap::new();
    let mut dirs = vec![root.clone()];
    while let Some(dir) = dirs.pop() {
        for entry in std::fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                dirs.push(path);
            } else {
                let rel = path
                    .strip_prefix(&root)
                    .unwrap()
                    .to_str()
                    .unwrap()
                    .replace('\\', "/");
                out.insert(rel, std::fs::read(&path).unwrap());
            }
        }
    }
    out
}

/// The template this crate ships, prepared as tC4 serves it: `localizedNames` added.
fn served() -> BTreeMap<String, Vec<u8>> {
    let mut files: BTreeMap<String, Vec<u8>> = TEMPLATE_FILES
        .iter()
        .map(|(path, text)| ((*path).to_owned(), text.as_bytes().to_vec()))
        .collect();
    let metadata = with_localized_names(template_metadata()).unwrap();
    files.insert("metadata.json".into(), metadata.into_bytes());
    files
}

#[test]
fn the_shipped_template_is_the_pinned_one() {
    let reference = reference();
    assert_eq!(validate_template(&served(), &reference), Ok(50));
    // Every shipped file, metadata and README aside, is tC4's byte for byte.
    for (path, text) in TEMPLATE_FILES {
        assert_eq!(
            reference.get(path).map(Vec::as_slice),
            Some(text.as_bytes()),
            "{path}"
        );
    }
    assert_eq!(
        reference.len(),
        TEMPLATE_FILES.len() + 1,
        "the fixture has a README besides"
    );
}

#[test]
fn a_prepared_template_with_crlf_a_lone_cr_a_changed_byte_or_another_story_is_refused() {
    let reference = reference();
    let story = "ingredients/content/01.md".to_owned();
    let bytes = served()[&story].clone();

    let mut crlf = served();
    let text = String::from_utf8(bytes.clone())
        .unwrap()
        .replace('\n', "\r\n");
    crlf.insert(story.clone(), text.clone().into_bytes());
    assert!(matches!(
        validate_template(&crlf, &reference),
        Err(TemplateError::CarriageReturn { what: "story", .. })
    ));
    assert!(
        parse_story(&text)
            .unwrap_err()
            .to_string()
            .contains("carriage return")
    );

    let mut lone = served();
    let mut cr = bytes[..1].to_vec();
    cr.push(b'\r');
    cr.extend_from_slice(&bytes[1..]);
    lone.insert(story.clone(), cr.clone());
    assert!(matches!(
        validate_template(&lone, &reference),
        Err(TemplateError::CarriageReturn { .. })
    ));
    assert!(parse_story(&String::from_utf8(cr).unwrap()).is_err());

    let mut flipped = served();
    let mut changed = bytes.clone();
    *changed.last_mut().unwrap() ^= 1;
    flipped.insert(story.clone(), changed);
    assert!(matches!(
        validate_template(&flipped, &reference),
        Err(TemplateError::BytesDiffer { what: "story", .. })
    ));

    let mut front = served();
    let intro = "ingredients/content/front/intro.md".to_owned();
    front.get_mut(&intro).unwrap().push(b' ');
    let error = validate_template(&front, &reference).unwrap_err();
    assert!(
        error.to_string().contains("content bytes differ"),
        "{error}"
    );

    let mut extra = served();
    extra.insert("ingredients/content/51.md".into(), bytes.clone());
    assert!(
        validate_template(&extra, &reference)
            .unwrap_err()
            .to_string()
            .contains("content set mismatch")
    );

    let mut unfixed = served();
    unfixed.insert(
        "metadata.json".into(),
        template_metadata().as_bytes().to_vec(),
    );
    assert!(matches!(
        validate_template(&unfixed, &reference),
        Err(TemplateError::LocalizedNamesMissing(_))
    ));

    let mut no_back = served();
    no_back.remove("ingredients/content/back/intro.md");
    assert!(validate_template(&no_back, &reference).is_err());
}

const LANGUAGE: fn() -> Value = || json!({"tag": "es", "name": {"en": "Spanish"}});

fn params(name: &str) -> NewObsProject<'_> {
    NewObsProject {
        name,
        abbreviation: "historias",
        language_code: "es",
        language_name: None,
    }
}

#[test]
fn a_name_the_template_would_splice_unescaped_is_refused_before_anything_is_made() {
    for name in ["Historias \"de\" Prueba", "Historias\\Prueba", "Tab\there"] {
        let refused = create_obs_project(&params(name), "2026-09-16T09:00:00.000Z", &LANGUAGE());
        assert!(
            matches!(
                refused,
                Err(Error::NewProject(NewProjectError::UnsafeName(_)))
            ),
            "{name}"
        );
    }
    let quote = params("Historias \"de\" Prueba")
        .check()
        .unwrap_err()
        .to_string();
    assert!(quote.contains("quote"), "{quote}");
    let backslash = params("Historias\\Prueba").check().unwrap_err().to_string();
    assert!(backslash.contains("backslash"), "{backslash}");
    for abbreviation in [
        "", ".hidden", "a b", "a:b", "a/b?", "a#b", "a%b", "a*b", "a|b",
    ] {
        let p = NewObsProject {
            abbreviation,
            ..params("Historias")
        };
        assert!(
            matches!(p.check(), Err(NewProjectError::UnsafeAbbreviation(_))),
            "{abbreviation:?}"
        );
    }
    let private = NewObsProject {
        language_code: "x-abc",
        ..params("Historias")
    };
    assert_eq!(
        private.check(),
        Err(NewProjectError::PrivateLanguageNeedsName)
    );
    let named = NewObsProject {
        language_name: Some("Abc Language"),
        ..private
    };
    assert_eq!(named.check(), Ok(()));
}

#[test]
fn a_new_project_is_the_template_in_seed_form_with_its_scope_kept_verbatim() {
    let created = create_obs_project(
        &params("Historias de Prueba"),
        "2026-09-16T09:00:00.000Z",
        &LANGUAGE(),
    )
    .unwrap();
    // The seed forms are the sample project's stories 2 to 50, which tC4 made the same way.
    let sample = stories("sample");
    let template = stories("template");
    assert_eq!(created.stories.len(), 50);
    for (story, text) in &created.stories {
        let n = usize::from(story.get());
        let parsed = parse_story(text).unwrap();
        assert_eq!(parsed.title, "", "story {n}");
        assert!(parsed.frames.iter().all(|f| f.text.is_empty()));
        assert_eq!(
            parsed.frames.len(),
            parse_story(&template[n - 1]).unwrap().frames.len()
        );
        if n > 1 {
            assert_eq!(text, &sample[n - 1], "story {n}");
        }
    }
    let base: Vec<&str> = created
        .base_ingredients
        .iter()
        .map(|(path, _)| *path)
        .collect();
    assert_eq!(
        base,
        [
            "LICENSE.md",
            "content/front/title.md",
            "content/front/intro.md",
            "content/back/intro.md"
        ]
    );
    for (path, text) in &created.base_ingredients {
        assert!(tc_obs::ids::is_obs_base_ingredient(path), "{path}");
        assert_eq!(
            *text,
            fixture(&format!(
                "tc4/conformance-fixtures/text_stories/ingredients/{path}"
            ))
        );
    }

    let meta: Value = serde_json::from_str(&created.metadata).unwrap();
    assert!(is_obs_metadata(&meta));
    assert_eq!(meta["localizedNames"], json!({}));
    assert_eq!(meta["identification"]["name"]["en"], "Historias de Prueba");
    assert_eq!(meta["identification"]["abbreviation"]["en"], "historias");
    assert_eq!(
        meta["identification"]["primary"]["local"]["historias"]["timestamp"],
        "2026-09-16T09:00:00.000Z"
    );
    assert_eq!(meta["meta"]["dateCreated"], "2026-09-16T09:00:00.000Z");
    assert_eq!(meta["languages"], json!([LANGUAGE()]));
    // R-10.2.3: the scope table is the template's, with its 33 books, never rebuilt; the
    // sample project carries the same table.
    let scope = &meta["type"]["flavorType"]["currentScope"];
    assert_eq!(scope.as_object().unwrap().len(), 33);
    assert_eq!(*scope, template_current_scope());
    let sample_meta: Value =
        serde_json::from_str(&fixture("tc4/sample-burrito-obs/metadata.json")).unwrap();
    assert_eq!(*scope, sample_meta["type"]["flavorType"]["currentScope"]);
    assert_eq!(
        serde_json::to_string(scope).unwrap(),
        serde_json::to_string(&sample_meta["type"]["flavorType"]["currentScope"]).unwrap(),
        "the order of the books is kept too"
    );
    assert!(!is_obs_metadata(
        &json!({"type": {"flavorType": {"name": "scripture", "flavor": {"name": "textTranslation"}}}})
    ));
}

#[test]
fn each_story_number_has_its_template_story() {
    for story in StoryNo::all() {
        let parsed = parse_story(tc_obs::template::template_story(story)).unwrap();
        assert_eq!(parsed.number, story.get());
        assert!(
            !parsed.title.is_empty(),
            "the template carries the gateway title"
        );
    }
}
