//! Ports of tC4's OBS data-layer tests, where they test this crate's rules:
//! `test/obsStory.test.ts`, `obsStoryState.test.ts`, `obsRecency.test.ts`,
//! `obsResources.test.ts`, `deriveObs.test.ts` (Translation Questions; tc-check owns the
//! tN and tW lists) and `obsImages.integration.test.ts` (the parts that need no server).
//! The stories and helps are the real ones: the template, and the en_obs v9, en_obs-tn v13,
//! en_obs-twl v3 and en_obs-tq v10 exports in `bundle/en`.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use common::{bundle_file, stories};
use serde_json::json;
use tc_helps::obs::ObsStory;
use tc_helps::tsv::{TsvScope, parse_notes, parse_word_links};
use tc_helps::{HelpRef, ObsRef};
use tc_obs::frame_set::frame_set_mismatch;
use tc_obs::images::{
    DEFAULT_PACK_REPO, DEFAULT_PACK_SHA, ImageAsset, ImagePack, ImagePins, ImageResolution,
    PackPin, Rung, default_pack_pin, image_file_name, pack_from_ingredients, pack_from_paths,
    resolve_image,
};
use tc_obs::presentation::{
    PackVia, SourceInput, SourceState, pack_not_installed, pinned_packs, present_story, read_pack,
};
use tc_obs::progress::{first_story, obs_progress, story_percent, story_to_open};
use tc_obs::questions::{parse_story_questions, story_questions};
use tc_obs::recency::{RECENT_STORIES, RecentStory, record_recent_story};
use tc_obs::story::{parse_story, write_frame, write_reference, write_title};
use tc_obs::template::{NewObsProject, create_obs_project};
use tc_obs::{FrameNo, StoryNo};

/// The fifty stories of a new project, in their seed form.
fn new_project() -> Vec<String> {
    let project = NewObsProject {
        name: "Historias con fotos",
        abbreviation: "fotos",
        language_code: "es",
        language_name: None,
    };
    create_obs_project(&project, "2026-09-16T09:00:00.000Z", &json!({"tag": "es"}))
        .unwrap()
        .stories
        .into_iter()
        .map(|(_, text)| text)
        .collect()
}

fn gateway_pin() -> PackPin {
    PackPin {
        repo_path: "git.door43.org/unfoldingWord/en_obs".into(),
        sha: "0123456789abcdef0123456789abcdef01234567".into(),
    }
}

fn uri_for(ipath: &str) -> String {
    format!("http://rig.test/api/burrito/ingredient/bytes/default?ipath={ipath}")
}

/// The default pack as read from a listing that holds one file per frame of `story`.
fn seeded_pack(story: &ObsStory) -> (ImagePack, tc_obs::presentation::PackReport) {
    let listing = story
        .frames
        .iter()
        .map(|f| format!("360px/{}", image_file_name(&f.image.to_string()).unwrap()))
        .collect();
    read_pack(
        default_pack_pin(),
        "default",
        Ok(listing),
        || Ok(vec![]),
        uri_for,
    )
}

fn present(
    story: &ObsStory,
    source: SourceInput<'_, PackPin>,
    default: (ImagePack, tc_obs::presentation::PackReport),
) -> tc_obs::presentation::StoryPresentation<PackPin> {
    present_story(
        story.clone(),
        source,
        &ImagePins::default(),
        vec![],
        default,
        &[],
    )
}

fn empty_listing() -> (ImagePack, tc_obs::presentation::PackReport) {
    read_pack(
        default_pack_pin(),
        "default",
        Ok(vec![]),
        || Ok(vec![]),
        uri_for,
    )
}

// ---------------------------------------------------------------------------------------
// test/obsStory.test.ts
// ---------------------------------------------------------------------------------------

#[test]
fn the_source_states_no_pin_not_installed_and_a_read_error_as_states() {
    let story = parse_story(&new_project()[0]).unwrap();
    assert!(!story.frames.is_empty());
    assert_eq!(
        present(&story, SourceInput::NoPin, empty_listing()).source,
        Some(SourceState::NoPin)
    );
    let missing = present(
        &story,
        SourceInput::NotInstalled(gateway_pin()),
        empty_listing(),
    );
    assert_eq!(
        missing.source,
        Some(SourceState::NotInstalled(gateway_pin()))
    );
    assert_eq!(missing.source_story, None);
    let unreadable = present(
        &story,
        SourceInput::Read(Err("no such file".into())),
        empty_listing(),
    );
    assert!(matches!(unreadable.source, Some(SourceState::Error(_))));
    assert_eq!(unreadable.source_story, None);
    assert_eq!(unreadable.story.frames.len(), story.frames.len());
    // While the pins are unknown no source condition is stated (#312).
    assert_eq!(
        present(&story, SourceInput::PinsUnknown, empty_listing()).source,
        None
    );
}

/// tC4's helper: drop the last frame (its image line and paragraph), keeping the
/// reference line. Image lines are found with `image_file_name`, as tC4 finds them.
fn drop_last_frame(bytes: &str) -> String {
    let lines: Vec<&str> = bytes.split('\n').collect();
    let images: Vec<usize> = (0..lines.len())
        .filter(|&i| image_file_name(lines[i]).is_some())
        .collect();
    assert!(images.len() > 1);
    let last_image = *images.last().unwrap();
    let mut last = lines.len() - 1;
    while last > 0 && lines[last].trim().is_empty() {
        last -= 1;
    }
    let is_ref =
        lines[last].len() > 1 && lines[last].starts_with('_') && lines[last].ends_with('_');
    let end = if last > 0 && is_ref && image_file_name(lines[last]).is_none() {
        last
    } else {
        lines.len()
    };
    let mut out = lines[..last_image].to_vec();
    out.extend_from_slice(&lines[end..]);
    out.join("\n")
}

#[test]
fn a_gateway_story_with_another_frame_count_is_a_mismatch_that_keeps_the_gateway_text() {
    for raw in [new_project()[0].clone(), stories("en_obs")[0].clone()] {
        let story = parse_story(&new_project()[0]).unwrap();
        let mismatched = drop_last_frame(&raw);
        assert_ne!(mismatched, raw);
        let p = present(&story, SourceInput::Read(Ok(&mismatched)), empty_listing());
        let source = p.source_story.as_ref().unwrap();
        assert_eq!(source.frames.len(), story.frames.len() - 1);
        let expected = frame_set_mismatch([&story], [source]).unwrap();
        assert_eq!(p.source, Some(SourceState::Mismatch(expected)));
    }
}

#[test]
fn a_gateway_story_numbered_differently_is_a_mismatch() {
    let raw = &new_project()[0];
    let story = parse_story(raw).unwrap();
    let renumbered = raw.replacen("# 1.", "# 2.", 1);
    let p = present(&story, SourceInput::Read(Ok(&renumbered)), empty_listing());
    assert_eq!(
        p.source,
        Some(SourceState::Mismatch(
            "source story 2 does not match project story 1".into()
        ))
    );
    assert_eq!(p.source_story.unwrap().number, 2);
}

#[test]
fn malformed_gateway_bytes_give_an_error_and_no_gateway_text() {
    let story = parse_story(&new_project()[0]).unwrap();
    let p = present(
        &story,
        SourceInput::Read(Ok("definitely not a story file")),
        empty_listing(),
    );
    assert!(matches!(p.source, Some(SourceState::Error(_))));
    assert_eq!(p.source_story, None);
    // The matching gateway story, the en_obs v9 one, is read and states nothing.
    let gateway = &stories("en_obs")[0];
    let p = present(&story, SourceInput::Read(Ok(gateway)), empty_listing());
    assert_eq!(p.source, None);
    assert_eq!(p.source_story.unwrap().title, "The Creation");
}

#[test]
fn with_no_pack_every_frame_is_missing_and_the_note_names_the_pack() {
    let story = parse_story(&new_project()[0]).unwrap();
    let p = present(&story, SourceInput::NoPin, empty_listing());
    assert!(
        p.images
            .iter()
            .all(|i| matches!(i, ImageResolution::Missing { file_name: Some(_) }))
    );
    assert_eq!(p.images.len(), story.frames.len());
    assert_eq!(p.image_packs.len(), 1);
    let report = &p.image_packs[0];
    assert_eq!(
        (
            report.pin.clone(),
            report.local_path.as_deref(),
            report.via,
            report.files
        ),
        (default_pack_pin(), Some("default"), Some(PackVia::Paths), 0)
    );
    let note = p.image_note.unwrap();
    assert_eq!(note.wanted, story.frames.len());
    assert_eq!(note.packs, p.image_packs);
}

#[test]
fn every_frame_resolves_from_the_real_listing_even_when_the_metadata_lists_nothing() {
    let story = parse_story(&new_project()[0]).unwrap();
    let p = present(&story, SourceInput::NoPin, seeded_pack(&story));
    assert_eq!(p.image_note, None);
    let report = &p.image_packs[0];
    assert_eq!(
        (report.via, report.files, report.error.as_deref()),
        (Some(PackVia::Paths), story.frames.len(), None)
    );
    for (i, image) in p.images.iter().enumerate() {
        let name = format!("obs-en-01-{:02}.jpg", i + 1);
        assert_eq!(
            image,
            &ImageResolution::Default {
                uri: uri_for(&format!("360px/{name}")),
                file_name: name,
                pin: default_pack_pin(),
            }
        );
    }
}

#[test]
fn the_ingredient_table_stands_in_when_the_listing_cannot_be_read() {
    let story = parse_story(&new_project()[0]).unwrap();
    let keys: Vec<String> = story
        .frames
        .iter()
        .map(|f| {
            format!(
                "ingredients/360px/{}",
                image_file_name(&f.image.to_string()).unwrap()
            )
        })
        .collect();
    let pack = read_pack(
        default_pack_pin(),
        "default",
        Err("injected failure".into()),
        || Ok(keys),
        uri_for,
    );
    let p = present(&story, SourceInput::NoPin, pack);
    assert_eq!(
        (p.image_packs[0].via, p.image_packs[0].files),
        (Some(PackVia::Metadata), story.frames.len())
    );
    assert!(
        p.image_packs[0]
            .error
            .as_deref()
            .unwrap()
            .contains("injected failure")
    );
    assert_eq!(p.image_note, None);
    assert!(
        p.images
            .iter()
            .all(|i| matches!(i, ImageResolution::Default { .. }))
    );
    // Neither readable: no candidate, and both reasons are reported.
    let none = read_pack(
        default_pack_pin(),
        "default",
        Err("listing".into()),
        || Err("metadata".into()),
        uri_for,
    );
    assert_eq!(
        (none.1.via, none.1.files, none.1.error.as_deref()),
        (None, 0, Some("listing; metadata"))
    );
}

#[test]
fn a_pack_with_no_picture_files_is_reported_not_silent() {
    let story = parse_story(&new_project()[0]).unwrap();
    let pack = read_pack(
        default_pack_pin(),
        "default",
        Ok(vec!["metadata.json".into(), "README.md".into()]),
        || Ok(vec![]),
        uri_for,
    );
    let p = present(&story, SourceInput::NoPin, pack);
    let note = p.image_note.unwrap();
    assert_eq!(
        (
            note.packs[0].local_path.as_deref(),
            note.packs[0].via,
            note.packs[0].files
        ),
        (Some("default"), Some(PackVia::Paths), 0)
    );
}

#[test]
fn a_pack_pinned_by_both_rungs_is_read_once() {
    let pin = PackPin {
        repo_path: "git.door43.org/es-419_gl/obs_images".into(),
        sha: "1".repeat(40),
    };
    let both = ImagePins {
        primary: Some(pin.clone()),
        fallback: Some(pin.clone()),
    };
    assert_eq!(pinned_packs(&both), vec![pin.clone()]);
    let two = ImagePins {
        primary: Some(pin.clone()),
        fallback: Some(default_pack_pin()),
    };
    assert_eq!(pinned_packs(&two), [pin.clone(), default_pack_pin()]);
    assert_eq!(
        pack_not_installed(pin).error.as_deref(),
        Some("not installed")
    );
}

// ---------------------------------------------------------------------------------------
// test/obsStoryState.test.ts
// ---------------------------------------------------------------------------------------

fn numbers() -> Vec<u8> {
    (1..=50).collect()
}

#[test]
fn a_project_resumes_on_its_remembered_story() {
    assert_eq!(first_story(&numbers(), None, Some(&json!(50))), Some(50));
    // A Home tile names its story; it wins over the remembered one.
    assert_eq!(
        first_story(&numbers(), Some("12"), Some(&json!(50))),
        Some(12)
    );
    assert_eq!(first_story(&[], None, None), None);
}

#[test]
fn a_book_code_is_never_read_as_a_story() {
    assert_eq!(first_story(&numbers(), Some("GEN"), None), Some(1));
    assert_eq!(first_story(&numbers(), None, Some(&json!("GEN"))), Some(1));
    assert_eq!(story_to_open(&numbers(), &json!("GEN")), None);
    assert_eq!(story_to_open(&numbers(), &json!(51)), None);
    assert_eq!(story_to_open(&numbers(), &json!(2)), Some(2));
    assert_eq!(story_to_open(&numbers(), &json!("2")), Some(2));
    assert_eq!(story_to_open(&numbers(), &json!(2.5)), None);
}

fn progress_of(
    project: &[String],
    gateway: impl FnMut(u8) -> Option<String>,
) -> tc_obs::progress::ObsProgress {
    let stories: Vec<ObsStory> = project.iter().map(|t| parse_story(t).unwrap()).collect();
    obs_progress(stories.iter().map(|s| (s.number, Some(s))), gateway)
}

#[test]
fn the_draft_percent_counts_frames_over_the_fifty_stories_and_ignores_title_and_reference() {
    let mut project = new_project();
    assert_eq!(progress_of(&project, |_| None).percent, Some(0));
    let total: usize = project
        .iter()
        .map(|t| parse_story(t).unwrap().frames.len())
        .sum();
    assert!(total > 0);
    project[0] = write_title(&project[0], "La creación").unwrap();
    project[0] = write_reference(&project[0], "Génesis 1-2").unwrap();
    assert_eq!(progress_of(&project, |_| None).percent, Some(0));
    project[0] = write_frame(&project[0], FrameNo(1), "Al principio.").unwrap();
    let percent =
        |drafted: usize| Some(((drafted as f64 / total as f64 * 100.0).round() as u8).max(1));
    assert_eq!(progress_of(&project, |_| None).percent, percent(1));
    let half = total.div_ceil(2);
    let mut drafted = 1;
    'stories: for (i, text) in project.iter_mut().enumerate() {
        let frames = parse_story(text).unwrap().frames.len();
        for f in 1..=frames {
            if drafted >= half {
                break 'stories;
            }
            if i == 0 && f == 1 {
                continue;
            }
            *text = write_frame(text, FrameNo(f as u16), &format!("Marco {}:{f}", i + 1)).unwrap();
            drafted += 1;
        }
    }
    assert_eq!(progress_of(&project, |_| None).percent, percent(half));
}

#[test]
fn fifty_story_tiles_carry_their_own_percent_and_title_or_the_gateway_title() {
    // The template with its own titles; stories 2 and 12 are blanked to test the fallback.
    let mut project = stories("template");
    project[0] = write_title(&project[0], "La creación").unwrap();
    project[1] = write_title(&project[1], "").unwrap();
    project[11] = write_title(&project[11], "").unwrap();
    project[0] = write_frame(&project[0], FrameNo(1), "Al principio.").unwrap();
    project[11] = write_frame(&project[11], FrameNo(1), "Moisés.").unwrap();
    project[11] = write_frame(&project[11], FrameNo(2), "El faraón.").unwrap();
    let seven = parse_story(&project[6]).unwrap().frames.len();
    for f in 1..=seven {
        project[6] = write_frame(&project[6], FrameNo(f as u16), &format!("Marco {f}")).unwrap();
    }
    let progress = progress_of(&project, |n| Some(format!("Gateway {n}")));
    assert_eq!(progress.stories.len(), 50);
    let tile = |n: u8| progress.stories.iter().find(|s| s.number == n).unwrap();
    assert_eq!(tile(1).title, "La creación");
    assert_eq!(tile(1).percent, Some(story_percent(1, tile(1).frames)));
    assert_eq!(
        (tile(12).title.as_str(), tile(12).drafted),
        ("Gateway 12", 2)
    );
    assert_eq!(tile(12).percent, Some(story_percent(2, tile(12).frames)));
    assert_eq!(tile(7).percent, Some(100));
    assert_eq!(
        (tile(2).title.as_str(), tile(2).percent, tile(2).drafted),
        ("Gateway 2", Some(0), 0)
    );
    let frames: usize = progress.stories.iter().map(|s| s.frames).sum();
    assert_eq!(progress.percent, Some(story_percent(1 + 2 + seven, frames)));
}

#[test]
fn a_story_that_cannot_be_read_is_unknown_and_so_is_the_project() {
    let mut project = stories("template");
    project[2] = write_title(&project[2], "").unwrap();
    let parsed: Vec<ObsStory> = project.iter().map(|t| parse_story(t).unwrap()).collect();
    let progress = obs_progress(
        parsed
            .iter()
            .map(|s| (s.number, (s.number != 2).then_some(s))),
        |_| None,
    );
    let tile = |n: u8| progress.stories.iter().find(|s| s.number == n).unwrap();
    assert_eq!((tile(2).percent, tile(2).title.as_str()), (None, ""));
    assert_eq!((tile(3).percent, tile(3).title.as_str()), (Some(0), ""));
    assert_eq!(
        (tile(1).percent, tile(1).title.as_str()),
        (Some(0), "The Creation")
    );
    assert_eq!(progress.percent, None);
}

// ---------------------------------------------------------------------------------------
// test/obsRecency.test.ts
// ---------------------------------------------------------------------------------------

fn order(list: &[RecentStory]) -> Vec<u32> {
    list.iter().map(|e| e.story).collect()
}

#[test]
fn an_edit_puts_its_story_first_without_a_duplicate() {
    let mut list = record_recent_story(&[], 3, 1000);
    list = record_recent_story(&list, 7, 2000);
    assert_eq!(order(&list), [7, 3]);
    list = record_recent_story(&list, 3, 3000);
    assert_eq!(order(&list), [3, 7]);
}

#[test]
fn a_fourth_distinct_story_pushes_the_oldest_out() {
    let list = [(1, 100), (2, 200), (3, 300)].map(|(story, at)| RecentStory { story, at });
    let list = record_recent_story(&list, 12, 400);
    assert_eq!(order(&list), [12, 3, 2]);
    assert_eq!((list.len(), RECENT_STORIES), (3, 3));
}

#[test]
fn the_list_is_ordered_by_time_and_story_zero_changes_nothing() {
    let list = record_recent_story(&[RecentStory { story: 5, at: 500 }], 4, 400);
    assert_eq!(order(&list), [5, 4]);
    assert_eq!(record_recent_story(&list, 0, 900), list);
}

// ---------------------------------------------------------------------------------------
// test/obsResources.test.ts (the picture ladder and the frame set; tc-resources owns the
// language-set rules)
// ---------------------------------------------------------------------------------------

fn template_line() -> String {
    stories("template")[0]
        .split('\n')
        .find(|l| l.starts_with("!["))
        .unwrap()
        .to_owned()
}

#[test]
fn a_line_that_is_not_an_image_never_resolves() {
    assert_eq!(image_file_name("not an image"), None);
    let default = ImagePack::empty(default_pack_pin());
    assert_eq!(
        resolve_image("not an image", &ImagePins::default(), &[], &[], &default),
        ImageResolution::Missing { file_name: None }
    );
}

#[test]
fn pictures_resolve_project_then_primary_then_fallback_then_default_and_the_line_never_changes() {
    let line = template_line();
    let name = image_file_name(&line).unwrap();
    let pin = default_pack_pin();
    let pinned = ImagePins {
        primary: Some(pin.clone()),
        fallback: Some(pin.clone()),
    };
    let one = |uri: &str| ImagePack {
        pin: pin.clone(),
        images: [(name.clone(), vec![ImageAsset::at(uri)])].into(),
    };
    let project = [ImageAsset {
        uri: format!("local://{name}"),
        role: Some("x-obsimages".into()),
        decodable: None,
    }];
    assert!(matches!(
        resolve_image(
            &line,
            &pinned,
            &project,
            &[one("local://override")],
            &one("local://default")
        ),
        ImageResolution::Project { .. }
    ));
    assert_eq!(
        resolve_image(
            &line,
            &pinned,
            &[],
            &[one("local://override")],
            &one("local://default")
        ),
        ImageResolution::Pin {
            uri: "local://override".into(),
            file_name: name.clone(),
            pin: pin.clone(),
            rung: Rung::Primary
        }
    );
    assert!(matches!(
        resolve_image(
            &line,
            &ImagePins::default(),
            &[],
            &[],
            &one("local://default")
        ),
        ImageResolution::Default { .. }
    ));
    assert_eq!(line, template_line());
}

#[test]
fn a_project_ingredient_is_matched_by_the_file_its_url_names_in_ipath() {
    let line = template_line();
    let name = image_file_name(&line).unwrap();
    let uri = format!(
        "https://rig.test/api/burrito/ingredient/bytes/_local_/_local_/project?ipath=360px/{name}"
    );
    let asset = ImageAsset {
        uri: uri.clone(),
        role: Some("x-obsimages".into()),
        decodable: None,
    };
    let empty = ImagePack::empty(default_pack_pin());
    assert_eq!(
        resolve_image(&line, &ImagePins::default(), &[asset], &[], &empty),
        ImageResolution::Project {
            uri,
            file_name: name
        }
    );
    // The query is decoded once: a literal percent sign in the file name still matches.
    let percent_uri = "https://rig.test/api/burrito/ingredient/bytes/_local_/_local_/project?ipath=360px/obs-50%25.jpg";
    let asset = ImageAsset {
        uri: percent_uri.into(),
        role: Some("x-obsimages".into()),
        decodable: None,
    };
    assert_eq!(
        resolve_image(
            "![x](https://cdn.door43.org/obs/jpg/360px/obs-50%25.jpg)",
            &ImagePins::default(),
            &[asset],
            &[],
            &empty
        ),
        ImageResolution::Project {
            uri: percent_uri.into(),
            file_name: "obs-50%.jpg".into()
        }
    );
}

#[test]
fn corrupt_and_ambiguous_candidates_fall_through_and_a_wrong_sha_is_refused() {
    let line = template_line();
    let name = image_file_name(&line).unwrap();
    let wanted = PackPin {
        sha: "1".repeat(40),
        ..default_pack_pin()
    };
    let pins = ImagePins {
        primary: Some(wanted.clone()),
        fallback: None,
    };
    let pack = |pin: &PackPin, assets: Vec<ImageAsset>| ImagePack {
        pin: pin.clone(),
        images: [(name.clone(), assets)].into(),
    };
    let wrong_sha = pack(
        &default_pack_pin(),
        vec![ImageAsset::at("local://wrong-sha.jpg")],
    );
    let ambiguous = pack(
        &wanted,
        vec![
            ImageAsset::at("local://one.jpg"),
            ImageAsset::at("local://two.jpg"),
        ],
    );
    let corrupt = pack(
        &wanted,
        vec![ImageAsset {
            decodable: Some(false),
            ..ImageAsset::at("local://bad.jpg")
        }],
    );
    let bundled = pack(
        &default_pack_pin(),
        vec![ImageAsset::at("local://default.jpg")],
    );
    let is_default = |r: ImageResolution| matches!(r, ImageResolution::Default { .. });
    assert!(is_default(resolve_image(
        &line,
        &pins,
        &[],
        &[wrong_sha, ambiguous],
        &bundled
    )));
    assert!(is_default(resolve_image(
        &line,
        &pins,
        &[],
        &[corrupt],
        &bundled
    )));
    let two = [format!("local://a/{name}"), format!("local://b/{name}")].map(|uri| ImageAsset {
        uri,
        role: Some("x-obsimages".into()),
        decodable: None,
    });
    assert!(is_default(resolve_image(&line, &pins, &two, &[], &bundled)));
}

#[test]
fn the_ingredient_table_maps_file_names_stably_whatever_its_order() {
    let line = template_line();
    let name = image_file_name(&line).unwrap();
    let keys = [
        "ingredients/360px/obs-en-01-02.jpg".to_owned(),
        format!("ingredients/360px/{name}"),
    ];
    let a = pack_from_ingredients(default_pack_pin(), keys.iter().map(String::as_str), |p| {
        format!("local://{p}")
    });
    let b = pack_from_ingredients(
        default_pack_pin(),
        keys.iter().rev().map(String::as_str),
        |p| format!("local://{p}"),
    );
    assert_eq!(a, b);
    assert_eq!(
        resolve_image(&line, &ImagePins::default(), &[], &[], &a).uri(),
        Some(format!("local://360px/{name}").as_str())
    );
}

#[test]
fn a_structural_frame_set_change_is_refused_but_changed_text_is_not() {
    let story = |number: u8, frames: usize, text: &str| {
        let lines: Vec<String> = (0..frames)
            .map(|i| format!("![x](obs-{number}-{i}.jpg)\n\n{text}\n"))
            .collect();
        parse_story(&format!("# {number}. {text}\n\n{}", lines.join("\n"))).unwrap()
    };
    assert_eq!(
        frame_set_mismatch([&story(1, 2, "old")], [&story(1, 2, "new")]),
        None
    );
    assert!(
        frame_set_mismatch([&story(1, 2, "x")], [&story(1, 3, "x")])
            .unwrap()
            .contains("story 1 has 3 source frames")
    );
    assert!(
        frame_set_mismatch([&story(1, 2, "x")], [&story(2, 2, "x")])
            .unwrap()
            .contains("does not match")
    );
}

#[test]
fn the_default_pack_is_the_pin_tc_resources_ships() {
    let pin = tc_resources::suite::obs_images();
    assert_eq!(pin.repo.to_string(), DEFAULT_PACK_REPO);
    assert_eq!(pin.sha.to_string(), DEFAULT_PACK_SHA);
}

// ---------------------------------------------------------------------------------------
// test/deriveObs.test.ts: OBS Translation Questions (en_obs-tq v10)
// ---------------------------------------------------------------------------------------

fn obs_tq() -> Vec<tc_helps::Question> {
    parse_story_questions(&bundle_file(
        "en_obs-tq_v10.sb.zip",
        "en_obs-tq/ingredients/OBS.tsv",
    ))
    .unwrap()
}

#[test]
fn every_question_row_is_story_keyed_with_its_question_and_response() {
    let questions = obs_tq();
    let all = story_questions(&questions, None);
    assert_eq!(all.len(), 672);
    assert!(all.iter().all(|q| matches!(q.reference, HelpRef::Obs(_))
        && !q.question.is_empty()
        && !q.response.is_empty()));
    let stories: std::collections::BTreeSet<u8> = all
        .iter()
        .filter_map(|q| match q.reference {
            HelpRef::Obs(at) => Some(at.story()),
            HelpRef::Bible(_) => None,
        })
        .collect();
    assert_eq!(stories.len(), 50);
}

#[test]
fn one_storys_questions_are_its_rows_with_the_creation_question_first() {
    let questions = obs_tq();
    let one = story_questions(&questions, StoryNo::new(1));
    assert_eq!(one.len(), 22);
    assert_eq!(
        one[0]
            .id
            .upstream_row_id()
            .map(|id| id.as_str().to_owned())
            .as_deref(),
        Some("es4e")
    );
    assert_eq!(one[0].reference, HelpRef::Obs(ObsRef::new(1, 1).unwrap()));
    assert_eq!(
        one[0].question,
        "Where did everything in the universe come from?"
    );
    assert_eq!(one[0].response, "God created everything.");
    assert_eq!(StoryNo::new(51), None);
}

const TQ_HEADER: &str = "Reference\tID\tTags\tQuote\tOccurrence\tQuestion\tResponse";

fn tq_table(rows: &[(&str, &str)]) -> String {
    let mut tsv = format!("{TQ_HEADER}\n");
    for (reference, id) in rows {
        tsv.push_str(&format!("{reference}\t{id}\t\t\t1\tQ?\tR.\n"));
    }
    tsv
}

fn ids_and_frames(questions: &[tc_helps::Question]) -> Vec<(String, u8, u16)> {
    story_questions(questions, None)
        .into_iter()
        .map(|q| {
            let HelpRef::Obs(at) = q.reference else {
                panic!("an OBS question")
            };
            let id = q.id.upstream_row_id().unwrap().as_str().to_owned();
            (id, at.story(), at.frame())
        })
        .collect()
}

#[test]
fn a_question_row_whose_locator_is_not_two_integers_is_dropped_and_the_rest_kept() {
    let tsv = tq_table(&[
        ("1:1", "aaaa"),
        ("front:intro", "bbbb"),
        ("1:x", "cccc"),
        ("2:3", "dddd"),
    ]);
    let questions = parse_story_questions(&tsv).unwrap();
    assert_eq!(
        ids_and_frames(&questions),
        [("aaaa".to_owned(), 1, 1), ("dddd".to_owned(), 2, 3)]
    );
    // A row that is broken in another way still refuses the table, at its line.
    let broken = format!("{tsv}front:intro\tbbbb\n2:4\tdddd\n");
    let refused = parse_story_questions(&broken).unwrap_err().to_string();
    assert!(refused.contains("line 7"), "{refused}");
}

/// tC4 keeps these three rows (as frame 2 of story 1, story 0 and story 51); no project
/// has those stories, no session shows them, and a question here cannot address them.
#[test]
fn a_question_for_no_story_of_a_project_is_dropped() {
    let tsv = tq_table(&[
        ("1:2:3", "aaaa"),
        ("0:1", "bbbb"),
        ("51:1", "cccc"),
        ("50:1", "dddd"),
    ]);
    assert_eq!(
        ids_and_frames(&parse_story_questions(&tsv).unwrap()),
        [("dddd".to_owned(), 50, 1)]
    );
}

/// The OBS helps the app ships address frames the en_obs v9 stories have: frame 0 is a
/// title, and every other frame exists in its story.
#[test]
fn the_shipped_obs_helps_address_frames_the_shipped_stories_have() {
    let frames: Vec<usize> = stories("en_obs")
        .iter()
        .map(|t| parse_story(t).unwrap().frames.len())
        .collect();
    let tn = parse_notes(
        &bundle_file("en_obs-tn_v13.sb.zip", "en_obs-tn/ingredients/OBS.tsv"),
        TsvScope::Obs,
    )
    .unwrap()
    .notes;
    let twl = parse_word_links(
        &bundle_file("en_obs-twl_v3.sb.zip", "en_obs-twl/ingredients/OBS.tsv"),
        TsvScope::Obs,
    )
    .unwrap();
    let tq = obs_tq();
    let refs = tn
        .iter()
        .map(|n| &n.reference)
        .chain(twl.iter().map(|l| &l.reference))
        .chain(tq.iter().map(|q| &q.reference));
    let mut count = 0;
    for reference in refs {
        let HelpRef::Obs(at) = reference else {
            panic!("{reference:?}")
        };
        assert!(
            usize::from(at.frame()) <= frames[usize::from(at.story()) - 1],
            "{at}"
        );
        count += 1;
    }
    assert_eq!(count, tn.len() + twl.len() + tq.len());
    assert_eq!(tn.len(), 2325);
}

// ---------------------------------------------------------------------------------------
// test/obsImages.integration.test.ts, without the server
// ---------------------------------------------------------------------------------------

#[test]
fn an_invented_file_name_resolves_to_nothing() {
    let empty = ImagePack::empty(default_pack_pin());
    assert!(matches!(
        resolve_image("![x](obs-xx-99-99.jpg)", &ImagePins::default(), &[], &[], &empty),
        ImageResolution::Missing { file_name: Some(name) } if name == "obs-xx-99-99.jpg"
    ));
}

/// The default pack holds `360px/obs-en-NN-FF.jpg` for frame FF of story NN. Every frame
/// of every story, in the template and in the en_obs v9 export, resolves to that file, and
/// the file name is the one tc-helps reads the story and frame from.
#[test]
fn every_frame_of_every_story_resolves_to_its_file_in_the_default_pack() {
    let template: Vec<ObsStory> = stories("template")
        .iter()
        .map(|t| parse_story(t).unwrap())
        .collect();
    let listing: Vec<String> = template
        .iter()
        .flat_map(|s| {
            s.frames
                .iter()
                .map(|f| format!("360px/{}", image_file_name(&f.image.to_string()).unwrap()))
        })
        .collect();
    assert_eq!(
        listing.len(),
        template.iter().map(|s| s.frames.len()).sum::<usize>()
    );
    let pack = pack_from_paths(default_pack_pin(), &listing, |p| format!("pack://{p}"));
    for set in ["template", "en_obs"] {
        for story in stories(set).iter().map(|t| parse_story(t).unwrap()) {
            for (i, frame) in story.frames.iter().enumerate() {
                let resolved = resolve_image(
                    &frame.image.to_string(),
                    &ImagePins::default(),
                    &[],
                    &[],
                    &pack,
                );
                let name = format!("obs-en-{:02}-{:02}.jpg", story.number, i + 1);
                assert_eq!(
                    resolved.uri(),
                    Some(format!("pack://360px/{name}").as_str()),
                    "{set} {} {}",
                    story.number,
                    i + 1
                );
                assert_eq!(frame.image.picture(), Some((story.number, i as u16 + 1)));
            }
        }
    }
}
