//! tc-obs against what tC4's own code answers (`fixtures/obs/obs-vectors.json`, made by
//! `crates/tc-obs/tools/gen-obs-vectors.mjs` from tC4 at `d310bf3`).

// Helpers outside #[test] functions read fixtures and may fail loudly.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use std::collections::BTreeMap;
use std::sync::OnceLock;

use common::{fixture, md5_hex, stories};
use serde_json::{Value, json};
use tc_helps::obs::{ObsFrame, ObsImage, ObsStory};
use tc_obs::frame_set::frame_set_mismatch;
use tc_obs::images::{
    DEFAULT_PACK_REPO, DEFAULT_PACK_SHA, ImageAsset, ImagePack, ImagePins, ImageResolution,
    PackPin, Rung, default_pack_pin, image_file_name, pack_from_paths, resolve_image,
};
use tc_obs::print::{PrintedFrame, PrintedStory, printed_stories, story_drafted};
use tc_obs::progress::first_story;
use tc_obs::questions::{parse_story_questions, story_questions};
use tc_obs::recency::{RecentStory, record_recent_story};
use tc_obs::story::{
    StoryState, apply_story_state, frame_text_problem, parse_story, reference_text_problem,
    seed_story, title_text_problem, write_frame, write_reference, write_title,
};
use tc_obs::template::{FormRefusal, NewObsForm, with_localized_names};
use tc_obs::{FrameNo, StoryError, StoryNo, TextProblem, WriteError};

fn vectors() -> &'static Value {
    static V: OnceLock<Value> = OnceLock::new();
    V.get_or_init(|| serde_json::from_str(&fixture("obs/obs-vectors.json")).unwrap())
}

fn sets() -> &'static BTreeMap<&'static str, Vec<String>> {
    static S: OnceLock<BTreeMap<&'static str, Vec<String>>> = OnceLock::new();
    S.get_or_init(|| {
        common::SETS
            .iter()
            .map(|set| (*set, stories(set)))
            .collect()
    })
}

fn fixture_story(case: &Value) -> &'static str {
    let set = case["set"].as_str().unwrap();
    let n = case["n"].as_u64().unwrap() as usize;
    &sets()[set][n - 1]
}

fn story_json(story: &ObsStory) -> Value {
    json!({
        "number": story.number,
        "title": story.title,
        "frames": story.frames.iter().map(|f| json!({"image": f.image.to_string(), "text": f.text})).collect::<Vec<_>>(),
        "ref": story.reference,
    })
}

fn image(line: &str) -> ObsImage {
    let inner = &line[2..line.len() - 1];
    let (alt, url) = inner.split_once("](").unwrap();
    ObsImage {
        alt: alt.into(),
        url: url.into(),
    }
}

fn story_of(v: &Value) -> ObsStory {
    ObsStory {
        number: v["number"].as_u64().unwrap() as u8,
        title: v["title"].as_str().unwrap().into(),
        frames: v["frames"]
            .as_array()
            .unwrap()
            .iter()
            .map(|f| ObsFrame {
                image: image(f["image"].as_str().unwrap()),
                text: f["text"].as_str().unwrap().into(),
            })
            .collect(),
        reference: v["ref"].as_str().map(str::to_owned),
    }
}

/// tC4 names a story number by `Number(digits)`, which a 20-digit number prints rounded.
fn same_error(ours: &str, theirs: &str) -> bool {
    ours == theirs
        || (ours.starts_with("story title line names story ")
            && theirs.starts_with("story title line names story "))
}

#[test]
fn every_story_of_the_three_sets_reads_as_tc4_reads_it() {
    let cases = vectors()["parse"]["fixtures"].as_array().unwrap();
    assert_eq!(cases.len(), 150);
    for case in cases {
        let text = fixture_story(case);
        let story = parse_story(text).unwrap_or_else(|e| panic!("{case}: {e}"));
        let d = &case["digest"];
        let frames_md5 = md5_hex(
            &story
                .frames
                .iter()
                .map(|f| format!("{}\u{0}{}", f.image, f.text))
                .collect::<Vec<_>>()
                .join("\u{1}"),
        );
        assert_eq!(
            json!({"number": story.number, "title": story.title, "ref": story.reference, "frames": story.frames.len(), "framesMd5": frames_md5}),
            *d,
            "{} {}",
            case["set"],
            case["n"]
        );
    }
}

#[test]
fn generated_and_hand_written_files_read_or_fail_as_in_tc4() {
    let cases = vectors()["parse"]["cases"].as_array().unwrap();
    let (mut read, mut refused) = (0, 0);
    for case in cases {
        let text = case["text"].as_str().unwrap();
        match (parse_story(text), case.get("story")) {
            (Ok(story), Some(expected)) => {
                assert_eq!(story_json(&story), *expected, "{}", case["name"]);
                read += 1;
            }
            (Err(error), None) => {
                let theirs = case["error"].as_str().unwrap();
                assert!(
                    same_error(&error.to_string(), theirs),
                    "{}: {error} / {theirs}",
                    case["name"]
                );
                refused += 1;
            }
            (ours, _) => panic!("{}: {text:?}: ours {ours:?}, tC4 {case}", case["name"]),
        }
    }
    assert!(
        read > 250 && refused > 250,
        "{read} read, {refused} refused"
    );
}

/// Runs one op of the vectors; `None` for an argument Rust cannot be given (a negative or
/// fractional frame number), which tC4 refuses before anything else.
fn run(text: &str, op: &Value) -> Option<Result<String, WriteError>> {
    let args = op["args"].as_array().unwrap();
    let s = |i: usize| args[i].as_str().unwrap();
    Some(match op["op"].as_str().unwrap() {
        "seed" => seed_story(text),
        "frame" => {
            let frame = args[0].as_f64().unwrap();
            if frame < 0.0 || frame.fract() != 0.0 {
                return None;
            }
            write_frame(text, FrameNo(frame as u16), s(1))
        }
        "title" => write_title(text, s(0)),
        "ref" => write_reference(text, s(0)),
        "state" => {
            let state = &args[0];
            let frames = state["frames"]
                .as_object()
                .unwrap()
                .iter()
                .map(|(k, v)| (FrameNo(k.parse().unwrap()), v.as_str().unwrap().to_owned()))
                .collect();
            apply_story_state(
                text,
                &StoryState {
                    frames,
                    reference: state.get("ref").and_then(Value::as_str).map(str::to_owned),
                },
            )
        }
        other => panic!("op {other}"),
    })
}

/// A title write whose title holds U+2028 or U+2029 and no other line break.
fn separator_title(op: &Value) -> bool {
    let is_title = op["op"] == "title" || (op["op"] == "frame" && op["args"][0] == 0);
    let title = op["args"]
        .as_array()
        .unwrap()
        .last()
        .unwrap()
        .as_str()
        .unwrap_or_default();
    is_title && title.contains(['\u{2028}', '\u{2029}']) && !title.contains(['\r', '\n'])
}

/// The ops checked, and of them the titles tC4 writes and then cannot read back.
#[derive(Default)]
struct Checked {
    ops: usize,
    unreadable_titles: usize,
}

fn check_ops(text: &str, ops: &[Value], label: &str, checked: &mut Checked) {
    for op in ops {
        let Some(ours) = run(text, op) else {
            let error = op["error"].as_str().unwrap();
            assert!(error.contains("frame"), "{label}: {op}");
            continue;
        };
        checked.ops += 1;
        match ours {
            Ok(out) => {
                if let Some(expected) = op.get("text") {
                    assert_eq!(out, expected.as_str().unwrap(), "{label}: {op}");
                } else {
                    assert_eq!(
                        Some(md5_hex(&out).as_str()),
                        op["md5"].as_str(),
                        "{label}: {op}: {out:?}"
                    );
                    assert_eq!(
                        out.encode_utf16().count() as u64,
                        op["length"].as_u64().unwrap()
                    );
                }
            }
            Err(WriteError::TitleText(TextProblem::NotOneLine)) if separator_title(op) => {
                // tC4 writes a title holding a line separator, and its reader then refuses
                // the story; tc-obs refuses the title instead. Where tC4 refused the write
                // for another reason, both refuse.
                if let Some(written) = op["text"].as_str() {
                    assert!(parse_story(written).is_err(), "{label}: {op}");
                }
                if op["error"].is_null() {
                    checked.unreadable_titles += 1;
                }
            }
            Err(error) => {
                let theirs = op["error"]
                    .as_str()
                    .unwrap_or_else(|| panic!("{label}: {op}: ours {error}"));
                assert!(
                    same_error(&error.to_string(), theirs),
                    "{label}: {op}: {error}"
                );
            }
        }
    }
}

#[test]
fn the_writers_give_tc4s_bytes_and_refusals() {
    let v = vectors();
    let parse_cases = v["parse"]["cases"].as_array().unwrap();
    let mut checked = Checked::default();
    for case in v["writes"]["cases"].as_array().unwrap() {
        let source = &parse_cases[case["case"].as_u64().unwrap() as usize];
        check_ops(
            source["text"].as_str().unwrap(),
            case["ops"].as_array().unwrap(),
            source["name"].as_str().unwrap(),
            &mut checked,
        );
    }
    for case in v["writes"]["fixtures"].as_array().unwrap() {
        let label = format!("{} {}", case["set"], case["n"]);
        check_ops(
            fixture_story(case),
            case["ops"].as_array().unwrap(),
            &label,
            &mut checked,
        );
    }
    assert!(checked.ops > 3500, "{}", checked.ops);
    // The one place tc-obs departs from tC4.
    assert!(
        (1..100).contains(&checked.unreadable_titles),
        "{}",
        checked.unreadable_titles
    );
}

#[test]
fn the_text_rules_are_tc4s() {
    for case in vectors()["textRules"].as_array().unwrap() {
        let input = case["input"].as_str().unwrap();
        let text = |p: Option<tc_obs::TextProblem>| p.map(|p| p.to_string());
        assert_eq!(
            text(frame_text_problem(input)).as_deref(),
            case["frame"].as_str(),
            "frame {input:?}"
        );
        // Titles with a line separator: see `the_writers_give_tc4s_bytes_and_refusals`.
        let title = if input.contains(['\u{2028}', '\u{2029}']) && !input.contains(['\r', '\n']) {
            Some(TextProblem::NotOneLine.to_string())
        } else {
            case["title"].as_str().map(str::to_owned)
        };
        assert_eq!(text(title_text_problem(input)), title, "title {input:?}");
        assert_eq!(
            text(reference_text_problem(input)).as_deref(),
            case["ref"].as_str(),
            "ref {input:?}"
        );
    }
}

#[test]
fn image_file_names_are_tc4s() {
    let images = &vectors()["images"];
    for case in images["fileNames"].as_array().unwrap() {
        let line = case["line"].as_str().unwrap();
        assert_eq!(
            image_file_name(line).as_deref(),
            case["fileName"].as_str(),
            "{line:?}"
        );
    }
    assert_eq!(images["default"]["repoPath"], DEFAULT_PACK_REPO);
    assert_eq!(images["default"]["sha"], DEFAULT_PACK_SHA);
}

fn pin_of(v: &Value) -> PackPin {
    PackPin {
        repo_path: v["repoPath"].as_str().unwrap().into(),
        sha: v["sha"].as_str().unwrap().into(),
    }
}

fn asset_of(v: &Value) -> ImageAsset {
    match v {
        Value::String(uri) => ImageAsset::at(uri.clone()),
        _ => ImageAsset {
            uri: v["uri"].as_str().unwrap().into(),
            role: v["role"].as_str().map(str::to_owned),
            decodable: v["decodable"].as_bool(),
        },
    }
}

fn pack_of(v: &Value) -> ImagePack {
    ImagePack {
        pin: pin_of(&v["pin"]),
        images: v["images"]
            .as_object()
            .unwrap()
            .iter()
            .map(|(name, a)| {
                let assets = match a {
                    Value::Array(all) => all.iter().map(asset_of).collect(),
                    one => vec![asset_of(one)],
                };
                (name.clone(), assets)
            })
            .collect(),
    }
}

fn pack_json(pack: &ImagePack) -> Value {
    let images: serde_json::Map<String, Value> = pack
        .images
        .iter()
        .map(|(name, assets)| {
            let list = assets
                .iter()
                .map(|a| json!({"uri": a.uri, "role": a.role}))
                .collect();
            (name.clone(), Value::Array(list))
        })
        .collect();
    Value::Object(images)
}

#[test]
fn packs_are_built_from_paths_as_tc4_builds_them() {
    for case in vectors()["images"]["packs"].as_array().unwrap() {
        let paths: Vec<&str> = case["paths"]
            .as_array()
            .unwrap()
            .iter()
            .map(|p| p.as_str().unwrap())
            .collect();
        let pack = pack_from_paths(default_pack_pin(), &paths, |ipath| {
            format!("local://{ipath}")
        });
        assert_eq!(pack_json(&pack), case["pack"]["images"], "{paths:?}");
    }
}

fn resolution_json(r: &ImageResolution) -> Value {
    let pin = |p: &PackPin| json!({"repoPath": p.repo_path, "sha": p.sha});
    match r {
        ImageResolution::Project { uri, file_name } => {
            json!({"source": "project", "uri": uri, "fileName": file_name})
        }
        ImageResolution::Pin {
            uri,
            file_name,
            pin: p,
            rung,
        } => json!({
            "source": "pin", "uri": uri, "fileName": file_name, "pin": pin(p),
            "rung": match rung { Rung::Primary => "primary", Rung::Fallback => "fallback" },
        }),
        ImageResolution::Default {
            uri,
            file_name,
            pin: p,
        } => json!({"source": "default", "uri": uri, "fileName": file_name, "pin": pin(p)}),
        ImageResolution::Missing { file_name } => {
            json!({"source": "missing", "fileName": file_name})
        }
    }
}

#[test]
fn pictures_resolve_through_the_same_ladder_as_tc4() {
    for case in vectors()["images"]["resolve"].as_array().unwrap() {
        let sets = &case["pins"]["languageSets"];
        let rung = |name: &str| sets[name].get("obs-images").map(pin_of);
        let pins = ImagePins {
            primary: rung("primary"),
            fallback: rung("fallback"),
        };
        let project: Vec<ImageAsset> = case["project"]
            .as_object()
            .unwrap()
            .values()
            .map(asset_of)
            .collect();
        let packs: Vec<ImagePack> = case["packs"]
            .as_array()
            .unwrap()
            .iter()
            .map(pack_of)
            .collect();
        let result = resolve_image(
            case["line"].as_str().unwrap(),
            &pins,
            &project,
            &packs,
            &pack_of(&case["fallbackPack"]),
        );
        let theirs = &case["result"];
        let mut expected = json!({"source": theirs["source"], "fileName": theirs["fileName"]});
        if !theirs["uri"].is_null() {
            expected["uri"] = theirs["uri"].clone();
        }
        if !theirs["pin"].is_null() {
            expected["pin"] =
                json!({"repoPath": theirs["pin"]["repoPath"], "sha": theirs["pin"]["sha"]});
        }
        if !theirs["rung"].is_null() {
            expected["rung"] = theirs["rung"].clone();
        }
        assert_eq!(resolution_json(&result), expected, "{}", case["name"]);
    }
}

#[test]
fn frame_set_mismatches_read_as_in_tc4() {
    for case in vectors()["frameSet"].as_array().unwrap() {
        let current: Vec<ObsStory> = case["current"]
            .as_array()
            .unwrap()
            .iter()
            .map(story_of)
            .collect();
        let incoming: Vec<ObsStory> = case["incoming"]
            .as_array()
            .unwrap()
            .iter()
            .map(story_of)
            .collect();
        assert_eq!(
            frame_set_mismatch(&current, &incoming).as_deref(),
            case["mismatch"].as_str(),
            "{case}"
        );
    }
}

fn printed_json(items: &[PrintedStory<'_>]) -> Value {
    items
        .iter()
        .map(|item| match item {
            PrintedStory::Gap(a, b) => json!({"gap": [a, b]}),
            PrintedStory::Story { story, frames } => json!({
                "story": story_json(story),
                "frames": frames.iter().map(|f| match f {
                    PrintedFrame::Drafted { n, text } => json!({"n": n, "text": text}),
                    PrintedFrame::Gap(a, b) => json!({"gap": [a, b]}),
                }).collect::<Vec<_>>(),
            }),
        })
        .collect()
}

#[test]
fn the_print_model_is_tc4s() {
    for case in vectors()["print"].as_array().unwrap() {
        let stories: Vec<ObsStory> = case["stories"]
            .as_array()
            .unwrap()
            .iter()
            .map(story_of)
            .collect();
        assert_eq!(
            printed_json(&printed_stories(&stories)),
            case["items"],
            "{case}"
        );
        let drafted: Vec<bool> = stories.iter().map(story_drafted).collect();
        assert_eq!(json!(drafted), case["drafted"]);
    }
}

#[test]
fn recent_stories_follow_tc4() {
    let mut steps = 0;
    for case in vectors()["recency"].as_array().unwrap() {
        for step in case.as_array().unwrap() {
            let story = step["story"].as_i64().unwrap();
            let Ok(story) = u32::try_from(story) else {
                // A negative story leaves tC4's list unchanged; this type cannot hold one.
                assert_eq!(step["output"], step["input"]);
                continue;
            };
            let input: Vec<RecentStory> = step["input"]
                .as_array()
                .map(|list| {
                    list.iter()
                        .map(|e| RecentStory {
                            story: e["story"].as_u64().unwrap() as u32,
                            at: e["at"].as_i64().unwrap(),
                        })
                        .collect()
                })
                .unwrap_or_default();
            let out = record_recent_story(&input, story, step["at"].as_i64().unwrap());
            let out: Vec<Value> = out
                .iter()
                .map(|e| json!({"story": e.story, "at": e.at}))
                .collect();
            assert_eq!(Value::Array(out), step["output"], "{step}");
            steps += 1;
        }
    }
    assert!(steps >= 10);
}

#[test]
fn story_numbers_from_strings_are_javascripts() {
    let all: Vec<u8> = (0..=255).collect();
    for case in vectors()["toNumber"].as_array().unwrap() {
        let input = case["input"].as_str().unwrap();
        // The generator writes -0, NaN and the infinities as strings. `includes` finds
        // story 0 for -0.
        let theirs = match &case["number"] {
            Value::String(s) if s == "-0" => Some(-0.0),
            other => other.as_f64(),
        };
        let expected = match theirs {
            Some(n) if n.fract() == 0.0 && (0.0..=255.0).contains(&n) => n as u8,
            _ => 200,
        };
        assert_eq!(
            first_story(&all, Some(input), Some(&json!(200))),
            Some(expected),
            "{input:?} → {}",
            case["number"]
        );
    }
}

#[test]
fn localized_names_is_added_as_tc4_adds_it() {
    let template = fixture("tc4/conformance-fixtures/text_stories/metadata.json");
    for case in vectors()["localizedNames"].as_array().unwrap() {
        let name = case["name"].as_str().unwrap();
        let input = match name {
            "template" => template.clone(),
            "already carries it" => {
                "{\n  \"localizedNames\": {},\n  \"ingredients\": {}\n}\n".into()
            }
            "tab indent" => "{\n\t\"a\": 1,\n\t\"ingredients\": {}\n}".into(),
            "no ingredients key" => "{\n  \"a\": 1\n}\n".into(),
            "key not at a line start" => "{\"a\": 1, \"ingredients\": {}}".into(),
            "nested first" => {
                "{\n  \"x\": {\n    \"ingredients\": 1\n  },\n  \"ingredients\": {}\n}\n".into()
            }
            other => panic!("case {other}"),
        };
        match with_localized_names(&input) {
            Ok(out) => {
                assert_eq!(Some(md5_hex(&out).as_str()), case["md5"].as_str(), "{name}");
                if let Some(text) = case["text"].as_str() {
                    assert_eq!(out, text);
                }
            }
            Err(error) => assert_eq!(
                Some(error.to_string().as_str()),
                case["error"].as_str(),
                "{name}"
            ),
        }
    }
}

/// Every row tC4 derives is a question here too, with its frame, question and response,
/// except a row whose ID a tc-helps question cannot hold: that row is left out and the
/// rest kept (`parse_story_questions`).
#[test]
fn story_questions_keep_the_rows_tc4_derives() {
    let cases = vectors()["questions"].as_array().unwrap();
    assert!(cases.len() >= 54);
    let mut left_out = Vec::new();
    for case in cases {
        let questions = parse_story_questions(case["tsv"].as_str().unwrap()).unwrap();
        let story = case["story"]
            .as_u64()
            .map(|n| StoryNo::new(u8::try_from(n).unwrap()).unwrap());
        let ours: Vec<Value> = story_questions(&questions, story)
            .into_iter()
            .map(|q| {
                let tc_helps::HelpRef::Obs(at) = q.reference else {
                    panic!("an OBS question")
                };
                json!({
                    "id": q.id.upstream_row_id().unwrap().as_str(),
                    "story": at.story(),
                    "frame": at.frame(),
                    "question": q.question,
                    "response": q.response,
                })
            })
            .collect();
        let (theirs, unheld): (Vec<Value>, Vec<Value>) = case["items"]
            .as_array()
            .unwrap()
            .iter()
            .cloned()
            .partition(|item| {
                item["id"]
                    .as_str()
                    .is_some_and(|id| tc_helps::RowId::parse(id).is_ok())
            });
        left_out.extend(unheld.iter().map(|item| item["id"].clone()));
        assert_eq!(ours, theirs, "{} (story {})", case["name"], case["story"]);
    }
    // The rows left out: tC4's ID `not an id`, an empty ID, `b-b`, and none at all (a row
    // of one cell), each in an LF and a CRLF table, for all stories and for story 1.
    let mut ids: Vec<String> = left_out.iter().map(Value::to_string).collect();
    ids.sort();
    ids.dedup();
    assert_eq!(ids, ["\"\"", "\"b-b\"", "\"not an id\"", "null"]);
    assert_eq!(left_out.len(), 4 * 2 * 2);
}

#[test]
fn the_new_project_form_is_checked_as_tc4_checks_it() {
    let cases = vectors()["newProjectForms"].as_array().unwrap();
    assert!(cases.len() >= 29);
    for case in cases {
        let form = NewObsForm {
            name: case["name"].as_str().unwrap(),
            language_name: "",
            language_code: case["code"].as_str().unwrap(),
        };
        let ours = match form.project([]) {
            Ok(project) => json!({"abbr": project.abbreviation}),
            Err(refusal) => json!({"error": match refusal {
                FormRefusal::NameRequired => "wizard.nameRequired",
                FormRefusal::CodeRequired => "wizard.codeRequired",
                FormRefusal::AbbrRequired => "wizard.abbrRequired",
                FormRefusal::NameInUse { .. } => "wizard.nameInUse",
            }}),
        };
        assert_eq!(
            ours, case["result"],
            "{:?} {:?}",
            form.name, form.language_code
        );
    }
}

#[test]
fn the_sample_story_is_the_template_drafted_by_generate_obs() {
    let draft = &vectors()["draft"];
    let d = &draft["draft"];
    let mut bytes = seed_story(&sets()["template"][0]).unwrap();
    bytes = write_frame(&bytes, FrameNo(0), d["title"].as_str().unwrap()).unwrap();
    for (frame, text) in d["frames"].as_object().unwrap() {
        bytes = write_frame(
            &bytes,
            FrameNo(frame.parse().unwrap()),
            text.as_str().unwrap(),
        )
        .unwrap();
    }
    bytes = write_reference(&bytes, d["ref"].as_str().unwrap()).unwrap();
    assert_eq!(Some(md5_hex(&bytes).as_str()), draft["story1Md5"].as_str());
    assert_eq!(bytes, sets()["sample"][0]);
    // The other forty-nine sample stories are the template's seed forms.
    for n in 2..=50 {
        assert_eq!(
            seed_story(&sets()["template"][n - 1]).unwrap(),
            sets()["sample"][n - 1],
            "story {n}"
        );
    }
}

#[test]
fn a_negative_control_a_changed_byte_is_noticed() {
    // The vectors bind: one wrong byte in an expected text fails the comparison.
    let text = &sets()["en_obs"][0];
    let written = write_frame(text, FrameNo(1), "Otro texto.").unwrap();
    assert_ne!(md5_hex(&written), md5_hex(text));
    assert!(matches!(
        write_frame("# 1.\n\n![a](b)\n", FrameNo(2), "x"),
        Err(WriteError::NoSuchFrame { frames: 1, .. })
    ));
    assert!(matches!(
        parse_story("# 1.\r\n"),
        Err(StoryError::CarriageReturn)
    ));
}
