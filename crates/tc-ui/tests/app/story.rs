//! The story screens' decisions (B7): the ports of tC4's `StoryRail`, `StoryUnderstand` and
//! `StoryDraft` rules that are not drawing.

use tc_api::ids::{StoryNo, WallMs};
use tc_api::shown::Known;
use tc_api::story::{
    FrameComment, FrameView, GatewayStory, ImageNote, PackReport, PackVia, SourceNotice, StoryRow,
    StoryView,
};
use tc_ui::app::story::{
    FrameMarker, card_in_focus, comment_label, draft_units, focus_key, frame_comment,
    frame_markers, image_note, is_frame_drafted, offers_download, pack_sentence, row_name,
    source_notice, story_label, story_of, understand_units,
};
use tc_ui::app::understand::HelpsTab;

fn story(n: u8) -> StoryNo {
    StoryNo::new(n).unwrap()
}

fn view() -> StoryView {
    StoryView {
        story: story(1),
        title: "La Creación".into(),
        reference: "Una historia de: Génesis 1-2".into(),
        frames: vec![
            FrameView {
                text: "Así fue.".into(),
                image: Some("/media/obs/p/a.jpg".into()),
            },
            FrameView {
                text: "  \n ".into(),
                image: None,
            },
            FrameView {
                text: String::new(),
                image: None,
            },
        ],
        gateway: Some(GatewayStory {
            title: "The Creation".into(),
            reference: "A Bible story from: Genesis 1-2".into(),
            frames: vec!["This is how.".into(), "Then God said.".into()],
        }),
        source: None,
        source_version: Some("v9".into()),
        image_note: None,
        comments: Vec::new(),
    }
}

#[test]
fn a_frame_is_drafted_when_its_paragraph_is_not_blank_as_javascript_trims_it() {
    // tC4 #289: `String(text ?? '').trim() !== ''`, the one rule the marker and the cell share.
    assert!(is_frame_drafted("Así fue."));
    for blank in ["", " ", "\n\n", "\t \u{a0} \u{feff}"] {
        assert!(!is_frame_drafted(blank), "{blank:?}");
    }
    assert!(is_frame_drafted("\u{2003}x\u{2003}"));
}

#[test]
fn a_story_number_is_kept_where_a_chapter_is() {
    assert_eq!(story_of(1), Some(story(1)));
    assert_eq!(story_of(50), Some(story(50)));
    assert_eq!(story_of(0), None);
    assert_eq!(story_of(51), None);
    assert_eq!(story_of(300), None);
    assert_eq!(story_label(story(7)), "Story 7");
}

/// tC4's `rowName`: the open story's own title first (it is live), else Home's, else the number.
#[test]
fn a_rail_row_is_named_by_the_live_title_then_homes_then_its_number() {
    let open = view();
    let known = |title: &str| StoryRow {
        story: story(1),
        title: title.into(),
        drafted: Known::Known(13),
    };
    assert_eq!(
        row_name(story(1), Some(&open), Some(&known("old"))),
        "1 · La Creación"
    );
    assert_eq!(
        row_name(story(1), None, Some(&known("La Creación"))),
        "1 · La Creación"
    );
    assert_eq!(row_name(story(1), None, Some(&known(""))), "Story 1");
    assert_eq!(
        row_name(story(2), Some(&open), None),
        "Story 2",
        "another story's title is not 2's"
    );
    let untitled = StoryView {
        title: String::new(),
        ..open
    };
    assert_eq!(
        row_name(story(1), Some(&untitled), Some(&known("Home's"))),
        "1 · Home's"
    );
}

#[test]
fn the_frame_grid_marks_the_drafted_frames_with_the_same_rule_as_the_cells() {
    assert_eq!(
        frame_markers(&view()),
        [
            FrameMarker {
                frame: 1,
                drafted: true
            },
            FrameMarker {
                frame: 2,
                drafted: false
            },
            FrameMarker {
                frame: 3,
                drafted: false
            },
        ]
    );
}

#[test]
fn understand_lists_the_title_then_every_frame_with_its_gateway_text_and_picture() {
    let units = understand_units(&view());
    let keys: Vec<&str> = units.iter().map(|u| u.key.as_str()).collect();
    assert_eq!(keys, ["title", "f1", "f2", "f3"]);
    assert_eq!(units[0].label, "Story title");
    assert_eq!(units[0].gateway.as_deref(), Some("The Creation"));
    assert_eq!(units[0].image, None);
    assert_eq!(units[1].label, "Frame 1");
    assert_eq!(units[1].gateway.as_deref(), Some("This is how."));
    assert_eq!(units[1].image.as_deref(), Some("/media/obs/p/a.jpg"));
    assert_eq!(units[3].gateway, None, "the gateway has no third frame");
    assert_eq!(units[1].frame, Some(1));
    assert_eq!(units[0].frame, Some(0));
}

#[test]
fn translate_lists_the_units_in_file_order_ending_with_the_reference_line() {
    let units = draft_units(&view());
    let tests: Vec<&str> = units.iter().map(|u| u.test_id.as_str()).collect();
    assert_eq!(
        tests,
        [
            "story-title",
            "story-frame-1",
            "story-frame-2",
            "story-frame-3",
            "story-ref"
        ]
    );
    let reference = &units[4];
    assert_eq!(reference.frame, None);
    assert_eq!(reference.draft, "Una historia de: Génesis 1-2");
    assert_eq!(
        reference.gateway.as_deref(),
        Some("A Bible story from: Genesis 1-2")
    );
    assert_eq!(reference.pill, "Draft the reference");
    assert_eq!(units[0].pill, "Draft the title");
    assert_eq!(units[2].pill, "Draft frame 2");
    // Without a gateway story no unit has gateway text.
    let alone = StoryView {
        gateway: None,
        ..view()
    };
    assert!(draft_units(&alone).iter().all(|u| u.gateway.is_none()));
}

#[test]
fn a_focus_has_the_key_of_its_unit() {
    assert_eq!(focus_key(0), "title");
    assert_eq!(focus_key(12), "f12");
}

/// tC4's `describeSource`, in its catalog's words.
#[test]
fn the_gateway_notice_says_why_the_story_is_absent_and_offers_the_download_only_when_it_helps() {
    assert_eq!(
        source_notice(&SourceNotice::NoPin),
        "No gateway story is pinned for this project."
    );
    let pin = SourceNotice::NotInstalled {
        pin: "git.door43.org/es-419_gl/es-419_obs@4a239590d543".into(),
    };
    assert_eq!(
        source_notice(&pin),
        "The pinned gateway story git.door43.org/es-419_gl/es-419_obs@4a239590d543 is not on this machine."
    );
    assert!(offers_download(&pin));
    for other in [
        SourceNotice::NoPin,
        SourceNotice::Mismatch {
            message: "m".into(),
        },
        SourceNotice::Error {
            message: "e".into(),
        },
    ] {
        assert!(!offers_download(&other));
    }
    assert_eq!(
        source_notice(&SourceNotice::Mismatch {
            message: "15 frames, not 16".into()
        }),
        "The gateway story does not match this project: 15 frames, not 16"
    );
    assert_eq!(
        source_notice(&SourceNotice::Error {
            message: "bad".into()
        }),
        "The gateway story could not be read: bad"
    );
}

/// tC4's `describePack` and the image note's sentence.
#[test]
fn the_picture_note_names_each_pack_consulted() {
    let pack =
        |installed: Option<&str>, via: Option<PackVia>, files, error: Option<&str>| PackReport {
            pin: "git.door43.org/uW/obs_images_360@7146d5b504f6".into(),
            installed_at: installed.map(str::to_owned),
            via,
            files,
            error: error.map(str::to_owned),
        };
    assert_eq!(
        pack_sentence(&pack(None, None, 0, Some("not installed"))),
        "git.door43.org/uW/obs_images_360@7146d5b504f6 is not installed"
    );
    assert_eq!(
        pack_sentence(&pack(Some("/r/p"), None, 0, Some("denied"))),
        "git.door43.org/uW/obs_images_360@7146d5b504f6 at /r/p could not be read (denied)"
    );
    assert_eq!(
        pack_sentence(&pack(Some("/r/p"), Some(PackVia::Paths), 1, None)),
        "git.door43.org/uW/obs_images_360@7146d5b504f6 at /r/p lists 1 image file (paths)"
    );
    assert_eq!(
        pack_sentence(&pack(Some("/r/p"), Some(PackVia::Metadata), 0, None)),
        "git.door43.org/uW/obs_images_360@7146d5b504f6 at /r/p lists 0 image files (metadata)"
    );
    let note = |wanted| ImageNote {
        wanted,
        packs: vec![
            pack(None, None, 0, Some("not installed")),
            pack(Some("/r/p"), Some(PackVia::Paths), 2, None),
        ],
    };
    assert_eq!(
        image_note(&note(16)),
        "No picture on this machine matched the 16 image lines of this story: \
         git.door43.org/uW/obs_images_360@7146d5b504f6 is not installed; \
         git.door43.org/uW/obs_images_360@7146d5b504f6 at /r/p lists 2 image files (paths)"
    );
    assert!(
        image_note(&note(1))
            .starts_with("No picture on this machine matched the image line of this story:")
    );
}

#[test]
fn a_frames_box_shows_its_newest_comment() {
    let comment = |frame, at, text: &str| FrameComment {
        frame,
        text: text.into(),
        at: WallMs(at),
    };
    let comments = [
        comment(1, 10, "first"),
        comment(2, 5, "other frame"),
        comment(1, 30, "newest"),
        comment(1, 20, "second"),
    ];
    assert_eq!(
        frame_comment(&comments, 1).map(|c| c.text.as_str()),
        Some("newest")
    );
    assert_eq!(
        frame_comment(&comments, 2).map(|c| c.text.as_str()),
        Some("other frame")
    );
    assert_eq!(frame_comment(&comments, 3), None);
    assert_eq!(comment_label(0), "Story title");
    assert_eq!(comment_label(4), "Frame 4");
}

/// tC4's `isActive` in story mode: a card of the frame in focus is marked, whatever its id.
#[test]
fn the_cards_of_the_frame_in_focus_are_marked() {
    assert!(card_in_focus("3", Some(3)));
    assert!(
        card_in_focus("0", Some(0)),
        "the title's cards lead on the title"
    );
    assert!(!card_in_focus("3", Some(4)));
    assert!(!card_in_focus("3", None));
    assert!(!card_in_focus("intro", Some(3)));
}

/// tC4's `shownTab`: a story has no simplified text, and Comments is Translate's.
#[test]
fn a_stories_panel_shows_only_its_own_tabs() {
    assert_eq!(HelpsTab::Simplified.shown(false, true), HelpsTab::Notes);
    assert_eq!(
        HelpsTab::Simplified.shown(false, false),
        HelpsTab::Simplified
    );
    assert_eq!(HelpsTab::Comments.shown(false, true), HelpsTab::Notes);
    assert_eq!(HelpsTab::Comments.shown(true, true), HelpsTab::Comments);
    assert_eq!(HelpsTab::Words.shown(false, true), HelpsTab::Words);
    for tab in [
        HelpsTab::Notes,
        HelpsTab::Words,
        HelpsTab::Questions,
        HelpsTab::Simplified,
        HelpsTab::Comments,
    ] {
        assert_eq!(HelpsTab::parse(tab.as_str()), Some(tab));
    }
}
