//! A story project's own pictures (tC4 #549): found by their path in `metadata.json`, which
//! lists a carried file with no role.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use std::collections::BTreeMap;

use common::stories;
use serde_json::Value;
use tc_burrito::{
    IngredientEntry, IngredientPath, Md5, MetadataSource, ProjectKind, build_metadata,
};
use tc_obs::images::{
    ImageAsset, ImagePack, ImagePins, ImageResolution, PackPin, default_pack_pin, image_file_name,
    is_project_picture, project_image_assets, resolve_image,
};

fn uri_for(ipath: &str) -> String {
    format!("local://project/{ipath}")
}

/// The first image line of story 1 in the new-project template.
fn template_line() -> String {
    stories("template")[0]
        .split('\n')
        .find(|l| l.starts_with("!["))
        .unwrap()
        .to_owned()
}

fn entry(path: &str, role: Option<&str>) -> IngredientEntry {
    let bytes = path.as_bytes();
    IngredientEntry {
        path: IngredientPath::parse(path).unwrap(),
        md5: Md5::of(bytes),
        size: bytes.len() as u64,
        role: role.map(str::to_owned),
        scope: None,
    }
}

/// The `metadata.json` the rebuild's own writer builds for a story project that carries
/// `carried` (each with no role, as tc-store lists a carried file), and its ingredient keys
/// in document order.
fn metadata_keys(carried: &[&str]) -> (Value, Vec<String>) {
    let source = MetadataSource {
        kind: ProjectKind::OpenBibleStories,
        format: (1, 0),
        fields: BTreeMap::new(),
        scope: tc_obs::template::template_current_scope(),
        relationships: vec![],
        nfc: true,
    };
    let mut entries = vec![
        entry("ingredients/content/01.md", None),
        entry(
            "ingredients/checking/settings.json",
            Some("x-check-settings"),
        ),
    ];
    entries.extend(carried.iter().map(|path| entry(path, None)));
    let bytes = build_metadata(&source, &entries).unwrap();
    let doc: Value = serde_json::from_slice(&bytes).unwrap();
    let keys = doc["ingredients"]
        .as_object()
        .unwrap()
        .keys()
        .cloned()
        .collect();
    (doc, keys)
}

fn pack(pin: PackPin, name: &str, uri: &str) -> ImagePack {
    ImagePack {
        pin,
        images: [(name.to_owned(), vec![ImageAsset::at(uri)])].into(),
    }
}

#[test]
fn a_project_picture_without_a_role_is_the_project_rung() {
    let line = template_line();
    let name = image_file_name(&line).unwrap();
    assert_eq!(name, "obs-en-01-01.jpg");
    let picture = format!("ingredients/360px/{name}");
    let (doc, keys) = metadata_keys(&[&picture, "ingredients/LICENSE.md"]);
    // The premise: the rebuild's metadata.json gives a carried picture no role.
    assert_eq!(doc["ingredients"][&picture]["mimeType"], "image/jpeg");
    assert_eq!(doc["ingredients"][&picture].get("role"), None);

    let project = project_image_assets(keys.iter().map(String::as_str), uri_for);
    assert_eq!(project.len(), 1, "{project:?}");

    let pin = PackPin {
        sha: "1".repeat(40),
        ..default_pack_pin()
    };
    let pinned = pack(pin.clone(), &name, "local://pinned-pack");
    let default = pack(default_pack_pin(), &name, "local://default-pack");
    assert_eq!(
        resolve_image(
            &line,
            &ImagePins {
                primary: Some(pin),
                fallback: None,
            },
            &project,
            &[pinned],
            &default,
        ),
        ImageResolution::Project {
            uri: uri_for(&format!("360px/{name}")),
            file_name: name,
        }
    );
}

#[test]
fn only_pictures_outside_checking_tc_and_content_are_the_projects() {
    let table = [
        ("ingredients/checking/obs-en-01-01.jpg", false),
        ("ingredients/360px/obs-en-01-01.jpg", true),
        ("ingredients/tc/obs-en-01-01.jpg", false),
        ("ingredients/obs-en-01-02.JPG", true),
        ("ingredients/content/obs-en-01-01.jpg", false),
        ("ingredients/pictures/content/obs-en-02-01.webp", true),
        ("ingredients/content/front/title.jpg", false),
        ("ingredients/art/cover.png", true),
        ("ingredients/audio/01-01.mp3", false),
        ("ingredients/art/cover.jpeg", true),
        ("ingredients/LICENSE.md", false),
        ("metadata.json", false),
        ("360px/obs-en-01-01.jpg", false),
    ];
    for (key, is_picture) in table {
        assert_eq!(is_project_picture(key), is_picture, "{key}");
    }
    let uris: Vec<String> = project_image_assets(table.iter().map(|(key, _)| *key), uri_for)
        .into_iter()
        .map(|asset| asset.uri)
        .collect();
    let expected: Vec<String> = table
        .iter()
        .filter(|(_, is_picture)| *is_picture)
        .map(|(key, _)| uri_for(key.strip_prefix("ingredients/").unwrap()))
        .collect();
    assert_eq!(uris, expected);
}

#[test]
fn a_conflict_copy_keeps_the_picture_and_two_folders_make_it_ambiguous() {
    let line = template_line();
    let name = image_file_name(&line).unwrap();
    let default = pack(default_pack_pin(), &name, "local://default-pack");
    let resolve = |keys: &[String]| {
        let project = project_image_assets(keys.iter().map(String::as_str), uri_for);
        resolve_image(&line, &ImagePins::default(), &project, &[], &default)
    };

    let stem = name.strip_suffix(".jpg").unwrap();
    let (_, keys) = metadata_keys(&[
        &format!("ingredients/360px/{name}"),
        &format!("ingredients/360px/{stem}.conflict-1a2b3c4d.jpg"),
    ]);
    assert_eq!(
        resolve(&keys),
        ImageResolution::Project {
            uri: uri_for(&format!("360px/{name}")),
            file_name: name.clone(),
        }
    );

    let (_, keys) = metadata_keys(&[
        &format!("ingredients/360px/{name}"),
        &format!("ingredients/images/{name}"),
    ]);
    assert!(
        matches!(resolve(&keys), ImageResolution::Default { .. }),
        "two project pictures named {name} are ambiguous, as in tC4"
    );
}
