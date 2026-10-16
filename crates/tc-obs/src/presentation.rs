//! One story as the story screen shows it: the project's story, the pinned gateway story
//! beside it, and each frame's picture (tC4 `readObsStoryPresentation`,
//! `src/data/obsStory.ts:164-254`, #289, #312, #313).
//!
//! The reads are the caller's: it reads the project's story, the gateway story when the
//! gateway pin is installed, and each picture pack's file listing, and hands the results
//! here. Nothing is written into the project's story: neither gateway text nor pictures.

use std::collections::BTreeSet;

use tc_helps::obs::ObsStory;

use crate::frame_set::frame_set_mismatch;
use crate::images::{ImageAsset, ImagePack, ImagePins, ImageResolution, PackPin, resolve_image};
use crate::story::parse_story;

/// What the caller knows of the gateway story.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SourceInput<'a, P> {
    /// The project's pins have not been read (the read failed or is pending): no source
    /// condition is stated, because none has been decided (#312).
    PinsUnknown,
    /// The pins are known and have no `obs` member.
    NoPin,
    /// The `obs` pin is not installed on this computer.
    NotInstalled(P),
    /// The gateway story's bytes, or why they could not be read.
    Read(Result<&'a str, String>),
}

/// Why the gateway story is absent or unusable (`ObsSourceState`, `obsStory.ts:44-48`).
/// Only `NotInstalled` earns a download offer; a mismatch keeps the gateway story, so each
/// frame keeps its gateway text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SourceState<P> {
    NoPin,
    NotInstalled(P),
    Mismatch(String),
    Error(String),
}

/// Where a pack's file names came from (`via`, `obsStory.ts:55-57`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PackVia {
    /// The real file listing.
    Paths,
    /// The `metadata.json` ingredient table, read because the listing could not be.
    Metadata,
}

/// One picture pack consulted (`ObsImagePackReport`, `obsStory.ts:50-61`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackReport {
    pub pin: PackPin,
    /// Where the pack was read from; `None` when the pin is not installed here.
    pub local_path: Option<String>,
    pub via: Option<PackVia>,
    /// Picture names found in the pack.
    pub files: usize,
    pub error: Option<String>,
}

/// The pinned packs to read, in rung order, each pack once (`obsStory.ts:218-231`).
pub fn pinned_packs(pins: &ImagePins) -> Vec<PackPin> {
    let mut seen = BTreeSet::new();
    [pins.primary.as_ref(), pins.fallback.as_ref()]
        .into_iter()
        .flatten()
        .filter(|pin| seen.insert((pin.repo_path.clone(), pin.sha.clone())))
        .cloned()
        .collect()
}

/// The report of a pinned pack that is not installed (`pinnedPack`, `obsStory.ts:124-133`).
pub fn pack_not_installed(pin: PackPin) -> PackReport {
    PackReport {
        pin,
        local_path: None,
        via: None,
        files: 0,
        error: Some("not installed".to_owned()),
    }
}

/// Reads one pack's file names: the real listing first, the ingredient table when the
/// listing cannot be read (`readPackUncached`, `obsStory.ts:98-122`). Never fails: a pack
/// that cannot be read reports why and holds nothing, so its frames fall through.
pub fn read_pack(
    pin: PackPin,
    local_path: &str,
    listing: Result<Vec<String>, String>,
    ingredient_keys: impl FnOnce() -> Result<Vec<String>, String>,
    uri_for: impl Fn(&str) -> String,
) -> (ImagePack, PackReport) {
    let report = |pack: &ImagePack, via, error| PackReport {
        pin: pack.pin.clone(),
        local_path: Some(local_path.to_owned()),
        via,
        files: pack.files(),
        error,
    };
    let listing_error = match listing {
        Ok(paths) => {
            let pack = crate::images::pack_from_paths(pin, &paths, uri_for);
            let r = report(&pack, Some(PackVia::Paths), None);
            return (pack, r);
        }
        Err(error) => error,
    };
    match ingredient_keys() {
        Ok(keys) => {
            let pack =
                crate::images::pack_from_ingredients(pin, keys.iter().map(String::as_str), uri_for);
            let r = report(&pack, Some(PackVia::Metadata), Some(listing_error));
            (pack, r)
        }
        Err(metadata_error) => {
            let pack = ImagePack::empty(pin);
            let r = report(
                &pack,
                None,
                Some(format!("{listing_error}; {metadata_error}")),
            );
            (pack, r)
        }
    }
}

/// Why no frame of a story has a picture: how many frames wanted one, and the packs
/// consulted (`ObsImageNote`, `obsStory.ts:39-42`). A missing picture is never a frame
/// error (#289).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImageNote {
    pub wanted: usize,
    pub packs: Vec<PackReport>,
}

/// One story ready for the story screen (`ObsStoryPresentation`, `obsStory.ts:18-37`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoryPresentation<P> {
    pub story: ObsStory,
    pub source_story: Option<ObsStory>,
    /// `None` when the gateway story was read and matches, or the pins are unknown.
    pub source: Option<SourceState<P>>,
    /// Frame by frame, frame 1 first.
    pub images: Vec<ImageResolution>,
    /// One report per pack consulted, the default pack last.
    pub image_packs: Vec<PackReport>,
    /// Set only when the story has image lines and none resolved.
    pub image_note: Option<ImageNote>,
}

/// Puts one story together for the screen. `pinned` holds what reading each pin of
/// [`pinned_packs`] gave (its pack when installed, and its report), `default` the default
/// pack read at its installed path, `project` the project's own picture ingredients.
pub fn present_story<P>(
    story: ObsStory,
    source: SourceInput<'_, P>,
    pins: &ImagePins,
    pinned: Vec<(Option<ImagePack>, PackReport)>,
    default: (ImagePack, PackReport),
    project: &[ImageAsset],
) -> StoryPresentation<P> {
    let (source_story, source) = match source {
        SourceInput::PinsUnknown => (None, None),
        SourceInput::NoPin => (None, Some(SourceState::NoPin)),
        SourceInput::NotInstalled(pin) => (None, Some(SourceState::NotInstalled(pin))),
        SourceInput::Read(Err(error)) => (None, Some(SourceState::Error(error))),
        SourceInput::Read(Ok(bytes)) => match parse_story(bytes) {
            Ok(gateway) => {
                let mismatch = frame_set_mismatch([&story], [&gateway]).map(SourceState::Mismatch);
                (Some(gateway), mismatch)
            }
            Err(error) => (None, Some(SourceState::Error(error.to_string()))),
        },
    };
    let mut packs = Vec::new();
    let mut reports = Vec::new();
    for (pack, report) in pinned {
        reports.push(report);
        packs.extend(pack);
    }
    let (default_pack, default_report) = default;
    reports.push(default_report);
    let images: Vec<ImageResolution> = story
        .frames
        .iter()
        .map(|frame| {
            resolve_image(
                &frame.image.to_string(),
                pins,
                project,
                &packs,
                &default_pack,
            )
        })
        .collect();
    let wanted = story.frames.len();
    let image_note =
        (wanted > 0 && images.iter().all(|image| image.uri().is_none())).then(|| ImageNote {
            wanted,
            packs: reports.clone(),
        });
    StoryPresentation {
        story,
        source_story,
        source,
        images,
        image_packs: reports,
        image_note,
    }
}
