//! The story screens' decisions (unit B7), the parts of tC4's `StoryRail.jsx`,
//! `StoryUnderstand.jsx` and `StoryDraft.jsx` that are not drawing. Pure, so they are tested on
//! the host.
//!
//! Frame 0 is the story's title. A frame is drafted exactly when its paragraph is not empty
//! (#289): the one rule the rail's marker and the target cell share.

use tc_api::ids::StoryNo;
use tc_api::story::{
    FrameComment, ImageNote, PackReport, PackVia, SourceNotice, StoryRow, StoryView,
};

use crate::app::shell::js_space;
use crate::i18n::{keys, t};

/// tC4's `isFrameDrafted`: the paragraph is not empty once JavaScript's `trim` has run.
pub fn is_frame_drafted(text: &str) -> bool {
    text.chars().any(|c| !js_space(c))
}

/// The story an open project shows: its number is kept where tC4 keeps a chapter (the place a
/// story is remembered under, and the helps panel's chapter).
pub fn story_of(chapter: u16) -> Option<StoryNo> {
    u8::try_from(chapter)
        .ok()
        .and_then(|number| StoryNo::new(number).ok())
}

/// "Story 3".
pub fn story_label(story: StoryNo) -> String {
    t(
        keys::story_draft::STORY_NUMBER,
        &[("n", &story.get().to_string())],
    )
}

/// A rail row's name (tC4's `rowName`): the open story's own title first (it is live), else
/// the one Home's summary knows, else the number alone.
pub fn row_name(number: StoryNo, open: Option<&StoryView>, row: Option<&StoryRow>) -> String {
    let live = open
        .filter(|story| story.story == number)
        .map(|story| story.title.as_str())
        .filter(|title| !title.is_empty());
    let known = row
        .map(|row| row.title.as_str())
        .filter(|title| !title.is_empty());
    match live.or(known) {
        Some(title) => format!("{} · {title}", number.get()),
        None => story_label(number),
    }
}

/// One frame's marker in the rail's grid.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FrameMarker {
    /// 1 to the story's last frame.
    pub frame: u16,
    pub drafted: bool,
}

/// The open story's frame grid.
pub fn frame_markers(view: &StoryView) -> Vec<FrameMarker> {
    view.frames
        .iter()
        .enumerate()
        .map(|(at, frame)| FrameMarker {
            frame: u16::try_from(at + 1).unwrap_or(u16::MAX),
            drafted: is_frame_drafted(&frame.text),
        })
        .collect()
}

/// A unit of the story screens: the title (frame 0), a frame, or the reference line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoryUnit {
    /// The unit's identity on its screen: `title`, `f3` or `ref`.
    pub key: String,
    /// The test id of its row (`story-title`, `story-frame-3`, `story-ref`).
    pub test_id: String,
    /// The frame it is (0 the title); none for the reference line.
    pub frame: Option<u16>,
    pub label: String,
    /// The "Draft frame 3" pill of an undrafted unit.
    pub pill: String,
    /// The gateway story's text; none when the gateway story is not there.
    pub gateway: Option<String>,
    /// The project's text.
    pub draft: String,
    /// Where the frame's picture is served.
    pub image: Option<String>,
}

/// The key a focus on `frame` has (`title`, or `f3`).
pub fn focus_key(frame: u16) -> String {
    if frame == 0 {
        "title".to_owned()
    } else {
        format!("f{frame}")
    }
}

fn title_unit(view: &StoryView) -> StoryUnit {
    StoryUnit {
        key: focus_key(0),
        test_id: "story-title".to_owned(),
        frame: Some(0),
        label: t(keys::story_draft::TITLE, &[]),
        pill: t(keys::story_draft::DRAFT_TITLE, &[]),
        gateway: view.gateway.as_ref().map(|g| g.title.clone()),
        draft: view.title.clone(),
        image: None,
    }
}

fn frame_units(view: &StoryView) -> Vec<StoryUnit> {
    view.frames
        .iter()
        .enumerate()
        .map(|(at, frame)| {
            let n = at + 1;
            let number = n.to_string();
            StoryUnit {
                key: focus_key(u16::try_from(n).unwrap_or(u16::MAX)),
                test_id: format!("story-frame-{n}"),
                frame: u16::try_from(n).ok(),
                label: t(keys::story_draft::FRAME_LABEL, &[("n", &number)]),
                pill: t(keys::story_draft::DRAFT_FRAME, &[("n", &number)]),
                gateway: view
                    .gateway
                    .as_ref()
                    .and_then(|g| g.frames.get(at))
                    .cloned(),
                draft: frame.text.clone(),
                image: frame.image.clone(),
            }
        })
        .collect()
}

/// Understand's units: the title, then every frame.
pub fn understand_units(view: &StoryView) -> Vec<StoryUnit> {
    let mut units = vec![title_unit(view)];
    units.extend(frame_units(view));
    units
}

/// Translate's units, in file order: the title, every frame, then the reference line.
pub fn draft_units(view: &StoryView) -> Vec<StoryUnit> {
    let mut units = understand_units(view);
    units.push(StoryUnit {
        key: "ref".to_owned(),
        test_id: "story-ref".to_owned(),
        frame: None,
        label: t(keys::story_draft::REFERENCE, &[]),
        pill: t(keys::story_draft::DRAFT_REFERENCE, &[]),
        gateway: view.gateway.as_ref().map(|g| g.reference.clone()),
        draft: view.reference.clone(),
        image: None,
    });
    units
}

/// Why the gateway story is absent or unusable, in tC4's words (`describeSource`).
pub fn source_notice(notice: &SourceNotice) -> String {
    match notice {
        SourceNotice::NoPin => t(keys::story_draft::SOURCE_NO_PIN, &[]),
        SourceNotice::NotInstalled { pin } => t(
            keys::story_draft::SOURCE_NOT_INSTALLED,
            &[("pin", pin as &dyn std::fmt::Display)],
        ),
        SourceNotice::Mismatch { message } => t(
            keys::story_draft::SOURCE_MISMATCH,
            &[("message", message as &dyn std::fmt::Display)],
        ),
        SourceNotice::Error { message } => t(
            keys::story_draft::SOURCE_ERROR,
            &[("message", message as &dyn std::fmt::Display)],
        ),
    }
}

/// Only a gateway story that is not on this computer earns the offer to get it.
pub fn offers_download(notice: &SourceNotice) -> bool {
    matches!(notice, SourceNotice::NotInstalled { .. })
}

/// One clause per picture pack consulted: where it was read and what it held (`describePack`).
pub fn pack_sentence(report: &PackReport) -> String {
    let pin = &report.pin as &dyn std::fmt::Display;
    let Some(path) = &report.installed_at else {
        return t(keys::story_draft::PACK_NOT_INSTALLED, &[("pin", pin)]);
    };
    let path = path as &dyn std::fmt::Display;
    let Some(via) = report.via else {
        let error = report.error.clone().unwrap_or_default();
        return t(
            keys::story_draft::PACK_UNREADABLE,
            &[("pin", pin), ("path", path), ("error", &error)],
        );
    };
    let via = match via {
        PackVia::Paths => "paths",
        PackVia::Metadata => "metadata",
    };
    let n = report.files.to_string();
    t(
        if report.files == 1 {
            keys::story_draft::PACK_FILES_ONE
        } else {
            keys::story_draft::PACK_FILES
        },
        &[("pin", pin), ("path", path), ("n", &n), ("via", &via)],
    )
}

/// Why no frame has a picture: the count of image lines, then each pack's clause.
pub fn image_note(note: &ImageNote) -> String {
    let n = note.wanted.to_string();
    let head = t(
        if note.wanted == 1 {
            keys::story_draft::NO_PICTURES_ONE
        } else {
            keys::story_draft::NO_PICTURES
        },
        &[("n", &n)],
    );
    let packs: Vec<String> = note.packs.iter().map(pack_sentence).collect();
    format!("{head} {}", packs.join("; "))
}

/// The comment a frame's box shows: its newest.
pub fn frame_comment(comments: &[FrameComment], frame: u16) -> Option<&FrameComment> {
    comments
        .iter()
        .filter(|c| c.frame == frame)
        .max_by_key(|c| c.at)
}

/// Whether a help card at `verse` (its frame) is the focused frame's (tC4's `isActive` in story
/// mode: the card of the frame in focus is marked).
pub fn card_in_focus(verse: &str, focus_frame: Option<u16>) -> bool {
    focus_frame.is_some_and(|frame| verse.parse::<u16>().ok() == Some(frame))
}

/// The label of a comment's frame in the helps panel's Comments tab.
pub fn comment_label(frame: u16) -> String {
    if frame == 0 {
        t(keys::story_draft::TITLE, &[])
    } else {
        t(keys::story_draft::FRAME_LABEL, &[("n", &frame.to_string())])
    }
}
