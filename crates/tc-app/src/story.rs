//! Open Bible Stories (unit B7): the story rail, one story with its gateway story and its
//! pictures, a story's helps, and C3's displayed-token saves. A story
//! is the file the fold derives, read once per open (`story_open_touches_one_story`), and the
//! gateway story, the helps and the picture packs are read from the resources installed here.
//!
//! The resources of a story project are read once per open, as a book's are (`LoadedObs`,
//! kept in the session's [`crate::understand::BookCache`]): the helps tables, the articles, the
//! gateway stories and the picture packs' file names.

use std::sync::Arc;

use crate::backend::StoreCommand;
use crate::wire;
use tc_api::helps::{
    ArticleView, NoteCard, PassageView, QuestionCard, Slot, Story as HelpsStory, StoryArticle,
    WordCard,
};
use tc_api::ids::{RefPart, StoryNo, WallMs};
use tc_api::shown::Known;
use tc_api::story::{
    FrameComment, FrameView, GatewayStory, ImageNote, List, Open, PackReport, PackVia,
    SourceNotice, StoryList, StoryRow, StoryView,
};
use tc_api::{ApiError, NotSaved};
use tc_helps::obs::ObsStory;
use tc_helps::{HelpRef, HelpsPart};
use tc_journal::fold::DerivedFile;
use tc_journal::record::CommentTarget;
use tc_journal::{ChangeId, Record, RecordKey};
use tc_obs::images::{ImagePack, ImagePins, PackPin};
use tc_obs::presentation::{self, SourceInput};
use tc_resources::{Missing, ObsResources, PartStatus, ProjectPins, ResourcePin, Rung, SetSlot};

use crate::app::App;
use crate::counters::AppCounter;
use crate::dispatch::Handle;
use crate::session::ProjectSession;
use crate::understand::{
    BookResourcesRef, blocks, item_id, occurrence, project_pins,
    resources_article_view as article_view, resources_stamp, slot, word_slug,
};

/// The fifty stories' numbers.
pub(crate) fn story_numbers() -> impl Iterator<Item = u8> {
    1..=StoryNo::COUNT
}

/// A picture pack's identity in a URL: its repository path with `/` as `~`, then its commit,
/// so `git.door43.org~uW~obs_images_360~7146…`. One path segment, and never a path.
pub(crate) fn pack_id(pin: &ResourcePin) -> String {
    format!(
        "{}~{}~{}~{}",
        pin.repo.host(),
        pin.repo.owner(),
        pin.repo.name(),
        pin.sha.to_hex()
    )
}

/// The pin a pack id names, or none for a text that is not one.
pub(crate) fn pin_of_pack_id(id: &str) -> Option<ResourcePin> {
    let mut parts = id.split('~');
    let (host, owner, name, sha) = (parts.next()?, parts.next()?, parts.next()?, parts.next()?);
    if parts.next().is_some() || host != tc_resources::DCS_HOST {
        return None;
    }
    let repo: tc_resources::RepoPath = format!("{host}/{owner}/{name}").parse().ok()?;
    Some(ResourcePin::new(repo, sha.parse().ok()?))
}

/// `repo@commit` with a commit of twelve digits: tC4's `shortPin`.
fn short_pin(pin: &ResourcePin) -> String {
    let sha = pin.sha.to_hex();
    format!("{}@{}", pin.repo, sha.get(..12).unwrap_or(&sha))
}

fn pack_pin(pin: &ResourcePin) -> PackPin {
    PackPin {
        repo_path: pin.repo.to_string(),
        sha: pin.sha.to_hex(),
    }
}

/// A URL path segment: what is not plain stays percent-encoded.
fn encode_path(path: &str) -> String {
    let mut out = String::with_capacity(path.len());
    for byte in path.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b'/' | b'~') {
            out.push(char::from(byte));
        } else {
            out.push_str(&format!("%{byte:02X}"));
        }
    }
    out
}

/// The pictures one project's packs hold, as read: what each pack consulted listed, in rung
/// order, then the default pack.
struct Packs {
    image_pins: ImagePins,
    pinned: Vec<(Option<ImagePack>, presentation::PackReport)>,
    default: (ImagePack, presentation::PackReport),
}

fn read_one_pack(
    app: &App,
    resource: &ResourcePin,
) -> (Option<ImagePack>, presentation::PackReport) {
    let pin = pack_pin(resource);
    let installed = match app.inner.resources.dcs(resource) {
        Ok(Some(installed)) => installed,
        Ok(None) => return (None, presentation::pack_not_installed(pin)),
        Err(error) => {
            let (pack, report) = presentation::read_pack(
                pin,
                "",
                Err(error.to_string()),
                || Err("the ingredient table is not read".to_owned()),
                |_| String::new(),
            );
            return (Some(pack), report);
        }
    };
    let paths: Vec<String> = installed
        .files()
        .filter_map(|path| path.strip_prefix("ingredients/"))
        .map(str::to_owned)
        .collect();
    let id = pack_id(resource);
    let (pack, report) = presentation::read_pack(
        pin,
        &installed.dir().to_string_lossy(),
        Ok(paths),
        || Err("the ingredient table is not read".to_owned()),
        |ipath| format!("/media/obs/{id}/{}", encode_path(ipath)),
    );
    (Some(pack), report)
}

fn read_packs(app: &App, pins: &ProjectPins) -> Packs {
    let slot_of = |rung| pins.set(rung)?.pin(SetSlot::ObsImages).cloned();
    let (primary, fallback) = (slot_of(Rung::Primary), slot_of(Rung::Fallback));
    let image_pins = ImagePins {
        primary: primary.as_ref().map(pack_pin),
        fallback: fallback.as_ref().map(pack_pin),
    };
    let mut seen = Vec::new();
    let mut pinned = Vec::new();
    for resource in [&primary, &fallback].into_iter().flatten() {
        if seen.contains(&pack_pin(resource)) {
            continue;
        }
        seen.push(pack_pin(resource));
        pinned.push(read_one_pack(app, resource));
    }
    let (default, report) = read_one_pack(app, tc_resources::suite::obs_images());
    let default = (
        default.unwrap_or_else(|| ImagePack::empty(tc_obs::images::default_pack_pin())),
        report,
    );
    Packs {
        image_pins,
        pinned,
        default,
    }
}

/// What a session read for its story project: the helps tables, the articles, the gateway
/// stories and the picture packs.
pub(crate) struct LoadedObs {
    stamp: Vec<Vec<ChangeId>>,
    pub(crate) pins: Option<ProjectPins>,
    pub(crate) resources: ObsResources,
    packs: Packs,
}

impl LoadedObs {
    pub(crate) fn resources_ref(&self) -> BookResourcesRef<'_> {
        BookResourcesRef {
            helps: &self.resources.helps,
            parts: &self.resources.parts,
            pins: self.pins.as_ref(),
        }
    }

    /// Each frame's picture address, as the story screens show it: from the pinned picture
    /// packs, else the default pack, in frame order.
    pub(crate) fn frame_pictures(&self, story: &ObsStory) -> Vec<Option<String>> {
        let packs = &self.packs;
        presentation::present_story(
            story.clone(),
            SourceInput::<()>::PinsUnknown,
            &packs.image_pins,
            packs.pinned.clone(),
            packs.default.clone(),
            &[],
        )
        .images
        .iter()
        .map(|image| image.uri().map(str::to_owned))
        .collect()
    }

    /// The project's `obs` pin: the first rung that pins it.
    fn source_pin(&self) -> Option<&ResourcePin> {
        let pins = self.pins.as_ref()?;
        Rung::LADDER
            .into_iter()
            .find_map(|rung| pins.set(rung)?.pin(SetSlot::Obs))
    }
}

/// The session's story resources, read at the first need and again when the pins change.
#[derive(Default)]
pub(crate) struct ObsCache {
    loaded: tokio::sync::Mutex<Option<Arc<LoadedObs>>>,
}

impl ObsCache {
    pub(crate) async fn forget(&self) {
        *self.loaded.lock().await = None;
    }
}

fn not_a_story_project() -> ApiError {
    ApiError::BadRequest {
        field: "story".to_owned(),
        reason: "the project holds no stories".to_owned(),
    }
}

impl App {
    /// The open story project's resources.
    pub(crate) async fn loaded_obs(
        &self,
        session: &Arc<ProjectSession>,
    ) -> Result<Arc<LoadedObs>, ApiError> {
        let read = session.read();
        let stamp = resources_stamp(&read);
        let mut cached = session.books().obs().loaded.lock().await;
        if let Some(found) = cached.as_ref().filter(|l| l.stamp == stamp) {
            return Ok(found.clone());
        }
        let pins = project_pins(&read).map_err(|reason| ApiError::BadRequest {
            field: "pins".to_owned(),
            reason,
        })?;
        let app = self.clone();
        let loaded = tokio::task::spawn_blocking(move || {
            let read_pins = pins.clone().unwrap_or_default();
            let resources = tc_resources::load_obs(&app.inner.resources, &read_pins);
            app.inner.counters.add(AppCounter::BookResourceLoads, 1);
            let packs = read_packs(&app, &read_pins);
            LoadedObs {
                stamp,
                pins,
                resources,
                packs,
            }
        })
        .await
        .map_err(|stopped| {
            self.internal(
                "a story project's resources could not be read",
                crate::reports::facts(serde_json::json!({
                    "stopped": crate::jobs::stop_reason(stopped),
                })),
            )
        })?;
        let loaded = Arc::new(loaded);
        *cached = Some(loaded.clone());
        Ok(loaded)
    }
}

/// A story of the project as its file, parsed: tC4's `readStory`. Costs one derived file.
pub(crate) fn read_story(
    read: &crate::counters::CountingSnapshot,
    story: StoryNo,
) -> Result<ObsStory, ApiError> {
    let file = DerivedFile::Story(story);
    let bytes = read.render(&file).ok_or_else(|| ApiError::BadRequest {
        field: "story".to_owned(),
        reason: format!("story {} is not in the project", story.get()),
    })?;
    let text = String::from_utf8(bytes).map_err(|_| ApiError::BadRequest {
        field: "story".to_owned(),
        reason: format!("story {} is not text", story.get()),
    })?;
    tc_obs::story::parse_story(&text).map_err(|error| ApiError::BadRequest {
        field: "story".to_owned(),
        reason: format!("story {} cannot be read: {error}", story.get()),
    })
}

impl Handle<List> for App {
    async fn handle(&self, req: List) -> Result<StoryList, ApiError> {
        let session = self.read(&req.at)?;
        let read = session.read();
        let summary = self.inner.home.last_known(&session.scope().project);
        let known = summary
            .as_deref()
            .and_then(|s| s.stories.as_ref())
            .map(|s| s.stories.as_slice())
            .unwrap_or_default();
        let mut stories = Vec::new();
        for number in story_numbers() {
            let Some(story) = StoryNo::new(number) else {
                continue;
            };
            let key = RecordKey::StoryBase(story);
            if read.key_state(&key).is_none() {
                continue;
            }
            let summary = known.iter().find(|s| s.number == number);
            stories.push(StoryRow {
                story,
                title: summary.map(|s| s.title.clone()).unwrap_or_default(),
                drafted: summary
                    .and_then(|s| s.percent)
                    .map_or(Known::Unknown, Known::Known),
            });
        }
        if stories.is_empty() {
            return Err(not_a_story_project());
        }
        Ok(StoryList { stories })
    }
}

/// The user comments on `story`'s frames, oldest first. A comment on a check is not a story
/// note. Costs the story's own comments.
fn comments_of(
    app: &App,
    read: &crate::counters::CountingSnapshot,
    story: StoryNo,
) -> Result<Vec<FrameComment>, ApiError> {
    let mut out = Vec::new();
    for key in read.story_comments_of(story) {
        let Some(Record::Comment { value, .. }) = read.projected(&key) else {
            continue;
        };
        let CommentTarget::Frame { frame, .. } = value.target else {
            continue;
        };
        out.push(FrameComment {
            token: app.shown_token(read, &key)?,
            frame: frame.get(),
            text: value.text,
            at: WallMs(read.written_at(&key).map_or(0, |at| at.0)),
        });
    }
    out.sort_by_key(|c| c.at);
    Ok(out)
}

/// Why the gateway story is not shown, and the story when it is (tC4's `ObsSourceState`).
fn gateway_of(
    loaded: &LoadedObs,
    story: &ObsStory,
) -> (Option<GatewayStory>, Option<SourceNotice>) {
    match &loaded.resources.stories_status {
        PartStatus::Missing(Missing::NotPinned) => (None, Some(SourceNotice::NoPin)),
        PartStatus::Missing(Missing::NotInstalled(pin)) => (
            None,
            Some(SourceNotice::NotInstalled {
                pin: short_pin(pin),
            }),
        ),
        PartStatus::Missing(other) => (
            None,
            Some(SourceNotice::Error {
                message: format!("{other:?}"),
            }),
        ),
        PartStatus::Loaded { .. } => {
            let Some(gateway) = loaded
                .resources
                .stories
                .iter()
                .find(|g| g.number == story.number)
            else {
                return (
                    None,
                    Some(SourceNotice::Error {
                        message: format!("the gateway has no story {}", story.number),
                    }),
                );
            };
            let mismatch = tc_obs::frame_set::frame_set_mismatch([story], [gateway])
                .map(|message| SourceNotice::Mismatch { message });
            (
                Some(GatewayStory {
                    title: gateway.title.clone(),
                    reference: gateway.reference.clone().unwrap_or_default(),
                    frames: gateway.frames.iter().map(|f| f.text.clone()).collect(),
                }),
                mismatch,
            )
        }
    }
}

fn pack_report(report: presentation::PackReport) -> PackReport {
    PackReport {
        pin: {
            let sha = report.pin.sha.get(..12).unwrap_or(&report.pin.sha);
            format!("{}@{sha}", report.pin.repo_path)
        },
        installed_at: report.local_path,
        via: report.via.map(|via| match via {
            presentation::PackVia::Paths => PackVia::Paths,
            presentation::PackVia::Metadata => PackVia::Metadata,
        }),
        files: u32::try_from(report.files).unwrap_or(u32::MAX),
        error: report.error,
    }
}

impl Handle<Open> for App {
    async fn handle(&self, req: Open) -> Result<StoryView, ApiError> {
        let session = self.read(&req.at)?;
        let read = session.read();
        let story = read_story(&read, req.story)?;
        let loaded = self.loaded_obs(&session).await?;
        let (gateway, source) = gateway_of(&loaded, &story);
        let source_version = loaded
            .source_pin()
            .and_then(|pin| pin.version.as_ref())
            .map(|v| v.as_str().to_owned());
        let packs = &loaded.packs;
        let shown = presentation::present_story(
            story.clone(),
            SourceInput::<()>::PinsUnknown,
            &packs.image_pins,
            packs.pinned.clone(),
            packs.default.clone(),
            &[],
        );
        let frames = story
            .frames
            .iter()
            .zip(&shown.images)
            .enumerate()
            .map(|(index, (frame, image))| {
                Ok(FrameView {
                    token: self.shown_token(
                        &read,
                        &RecordKey::Frame(
                            req.story,
                            tc_journal::FrameNo::new(
                                u16::try_from(index + 1).map_err(|_| not_a_story_project())?,
                            ),
                        ),
                    )?,
                    text: frame.text.clone(),
                    image: image.uri().map(str::to_owned),
                })
            })
            .collect::<Result<Vec<_>, ApiError>>()?;
        Ok(StoryView {
            view: wire::scope::view(read.seq()),
            title_token: self.shown_token(
                &read,
                &RecordKey::Frame(req.story, tc_journal::FrameNo::TITLE),
            )?,
            reference_token: self.shown_token(&read, &RecordKey::StoryRef(req.story))?,
            story: req.story,
            title: story.title.clone(),
            reference: story.reference.clone().unwrap_or_default(),
            frames,
            gateway,
            source,
            source_version,
            image_note: shown.image_note.map(|note| ImageNote {
                wanted: u32::try_from(note.wanted).unwrap_or(u32::MAX),
                packs: note.packs.into_iter().map(pack_report).collect(),
            }),
            comments: comments_of(self, &read, req.story)?,
        })
    }
}

// ---------------------------------------------------------------------------------------
// helps/story, helps/storyArticle
// ---------------------------------------------------------------------------------------

/// Whether a help belongs to `story`'s frame, and which frame (0 is the title).
fn frame_of(reference: &HelpRef, story: StoryNo) -> Option<u16> {
    match reference {
        HelpRef::Obs(at) if at.story() == story.get() => Some(at.frame()),
        _ => None,
    }
}

fn frame_part(frame: u16) -> Option<RefPart> {
    RefPart::parse(&frame.to_string()).ok()
}

impl LoadedObs {
    /// The helps of `story`: every frame's cards in the resources' order, as one passage with
    /// the story as the chapter and the frame as the verse.
    fn passage(&self, story: StoryNo) -> PassageView {
        let helps = &self.resources.helps;
        let status = |part| self.resources.parts.get(&part);
        let pins = self.pins.as_ref();
        let chapter = RefPart::parse(&story.get().to_string()).ok();
        let at = |reference: &HelpRef| -> Option<(RefPart, RefPart)> {
            Some((chapter.clone()?, frame_part(frame_of(reference, story)?)?))
        };
        let notes = slot(status(HelpsPart::ObsNotes), pins, || {
            helps
                .notes()
                .iter()
                .filter_map(|note| {
                    let (chapter, verse) = at(&note.reference)?;
                    Some(NoteCard {
                        id: item_id(&note.id),
                        chapter,
                        verse,
                        quote: note.quote.as_str().to_owned(),
                        occurrence: occurrence(note.occurrence),
                        note: blocks(&note.text),
                        academy: note
                            .support_reference
                            .as_ref()
                            .and_then(|a| a.slug())
                            .map(str::to_owned),
                        panes: Vec::new(),
                    })
                })
                .collect()
        });
        let words = slot(status(HelpsPart::ObsWordLinks), pins, || {
            helps
                .word_links()
                .iter()
                .filter_map(|link| {
                    let (chapter, verse) = at(&link.reference)?;
                    Some(WordCard {
                        id: item_id(&link.id),
                        chapter,
                        verse,
                        quote: link.orig_words.as_str().to_owned(),
                        occurrence: occurrence(link.occurrence),
                        category: link.article.category().map(|c| c.folder().to_owned()),
                        slug: word_slug(&link.article),
                        panes: Vec::new(),
                    })
                })
                .collect()
        });
        let questions = slot(status(HelpsPart::ObsQuestions), pins, || {
            helps
                .questions()
                .iter()
                .filter_map(|q| {
                    let (chapter, verse) = at(&q.reference)?;
                    Some(QuestionCard {
                        id: item_id(&q.id),
                        chapter,
                        verse,
                        question: q.question.clone(),
                        response: q.response.clone(),
                    })
                })
                .collect()
        });
        PassageView {
            notes,
            words,
            questions,
            simplified: Slot::None,
            simplified_pin: None,
            other_book: None,
        }
    }
}

impl Handle<HelpsStory> for App {
    async fn handle(&self, req: HelpsStory) -> Result<PassageView, ApiError> {
        let session = self.read(&req.at)?;
        let loaded = self.loaded_obs(&session).await?;
        Ok(loaded.passage(req.story))
    }
}

impl Handle<StoryArticle> for App {
    async fn handle(&self, req: StoryArticle) -> Result<ArticleView, ApiError> {
        let session = self.read(&req.at)?;
        let loaded = self.loaded_obs(&session).await?;
        Ok(article_view(
            &loaded.resources_ref(),
            &req.link,
            (HelpsPart::ObsNotes, HelpsPart::ObsWordLinks),
        ))
    }
}

// ---------------------------------------------------------------------------------------
// /media/obs/{pack}/{path}
// ---------------------------------------------------------------------------------------

/// A picture the page fetches by URL.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MediaFile {
    pub bytes: Vec<u8>,
    pub content_type: &'static str,
}

/// The content type of a picture file name; none for anything else, which is never served.
fn picture_type(name: &str) -> Option<&'static str> {
    let (_, extension) = name.rsplit_once('.')?;
    match extension.to_ascii_lowercase().as_str() {
        "jpg" | "jpeg" => Some("image/jpeg"),
        "png" => Some("image/png"),
        "webp" => Some("image/webp"),
        _ => None,
    }
}

/// A path below a pack's `ingredients/` that names a picture and cannot leave it.
fn picture_path(path: &str) -> Option<String> {
    let mut clean = Vec::new();
    for segment in path.split('/') {
        if segment.is_empty() || segment == "." || segment == ".." || segment.contains(['\\', '\0'])
        {
            return None;
        }
        clean.push(segment);
    }
    picture_type(clean.last()?)?;
    Some(format!("ingredients/{}", clean.join("/")))
}

impl App {
    /// `GET /media/obs/{pack}/{path}`: a picture of an installed pack. Only a file the pack's
    /// receipt lists, and only a picture, is served.
    pub async fn obs_picture(&self, pack: &str, path: &str) -> Option<MediaFile> {
        let content_type = picture_type(path)?;
        let file = picture_path(path)?;
        let pin = pin_of_pack_id(pack)?;
        let app = self.clone();
        tokio::task::spawn_blocking(move || {
            let installed = app.inner.resources.dcs(&pin).ok()??;
            installed
                .has_file(&file)
                .then(|| installed.read(&file).ok())?
                .map(|bytes| MediaFile {
                    bytes,
                    content_type,
                })
        })
        .await
        .ok()?
    }
}

impl App {
    pub(crate) fn record_story_edit(
        &self,
        session: &ProjectSession,
        story: tc_journal::StoryNo,
        frame: u16,
        text: &str,
        at: WallMs,
    ) {
        let folder = session.scope().project.clone();
        self.inner.prefs.record_last_used(folder.clone(), at);
        self.inner.prefs.record_last_edit(
            session.id().clone(),
            crate::prefs::LastEdit {
                project: folder,
                unit: crate::prefs::EditUnit::Story { story, frame },
                mode: tc_api::prefs::PlaceMode::Draft,
                tool: None,
                snippet: text
                    .split_whitespace()
                    .collect::<Vec<_>>()
                    .join(" ")
                    .chars()
                    .take(160)
                    .collect(),
                at,
            },
        );
    }

    async fn save_story_text(
        &self,
        at: tc_api::ids::Scope,
        request: tc_api::ids::RequestId,
        token: tc_api::ShownToken,
        text: String,
        reference: bool,
    ) -> Result<tc_api::write::Ack, ApiError> {
        let session = self.write_session(&at)?;
        let handle = session.handle();
        let shown = wire::token::edit_token(handle.as_ref(), &token)?;
        if !matches!(
            (reference, shown.key()),
            (true, RecordKey::StoryRef(_)) | (false, RecordKey::Frame(..))
        ) {
            return Err(ApiError::NotSaved(NotSaved::BadToken));
        }
        if let tc_api::event::SessionMode::ReadOnly(reason) = handle.facts().mode {
            return Err(ApiError::NotSaved(NotSaved::ReadOnly(reason)));
        }
        let line = match shown.key() {
            RecordKey::StoryRef(_) => tc_api::story::StoryLine::Reference,
            RecordKey::Frame(_, tc_journal::FrameNo::TITLE) => tc_api::story::StoryLine::Title,
            _ => tc_api::story::StoryLine::Frame,
        };
        let text = tc_api::story::stored_text(line, &text);
        let problem = match line {
            tc_api::story::StoryLine::Reference => tc_obs::story::reference_text_problem(&text),
            tc_api::story::StoryLine::Title => tc_obs::story::title_text_problem(&text),
            tc_api::story::StoryLine::Frame => tc_obs::story::frame_text_problem(&text),
        };
        if let Some(problem) = problem {
            return Err(ApiError::NotSaved(NotSaved::Refused(
                tc_api::write::SaveRefusal::InvalidText {
                    reason: story_text_problem(problem, reference).to_owned(),
                },
            )));
        }
        let reply = handle
            .submit(StoreCommand::save_frame(shown.clone(), text.clone()).requested_by(request))
            .await
            .map_err(|error| match wire::error::command_error(error) {
                Ok(api) => api,
                Err(error) => self.internal(
                    "the store refused a story save",
                    crate::reports::facts(serde_json::json!({"reason": error.0})),
                ),
            })?;
        self.record_translate_edit(&session, &shown, &text, self.inner.clock.now());
        wire::write::record_ack(
            &reply,
            &session.read(),
            Some(&shown),
            &text,
            handle.facts().device,
        )
        .map_err(|error| {
            self.internal(
                "a story save reply could not be written",
                crate::reports::facts(serde_json::json!({"reason": error.to_string()})),
            )
        })
    }
}

/// Content validation is typed; storage's diagnostic strings are for reports, while the
/// editor gives the person a concrete correction without referring to a file format.
fn story_text_problem(problem: tc_obs::TextProblem, reference: bool) -> &'static str {
    use tc_obs::TextProblem;
    match problem {
        TextProblem::Empty => "The reference needs text.",
        TextProblem::NotOneLine if reference => "Write the reference on one line.",
        TextProblem::NotOneLine => "Write the title on one line.",
        TextProblem::OuterWhitespace if reference => {
            "Remove spaces before and after the reference."
        }
        TextProblem::OuterWhitespace => "Remove spaces before and after the title.",
        TextProblem::CarriageReturn | TextProblem::BlankLine => {
            "Write the frame as one paragraph, without blank lines."
        }
        TextProblem::ImageLine => "Remove the image line from the frame text.",
        TextProblem::ReferenceLine => "Remove the reference line from the frame text.",
        TextProblem::TitleLine => "Remove the story title line from the frame text.",
    }
}

impl Handle<tc_api::story::SaveFrame> for App {
    async fn handle(&self, req: tc_api::story::SaveFrame) -> Result<tc_api::write::Ack, ApiError> {
        self.save_story_text(req.at, req.request, req.shown, req.text, false)
            .await
    }
}
impl Handle<tc_api::story::SaveRef> for App {
    async fn handle(&self, req: tc_api::story::SaveRef) -> Result<tc_api::write::Ack, ApiError> {
        self.save_story_text(req.at, req.request, req.shown, req.text, true)
            .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_pack_id_is_one_segment_that_names_its_pin_and_nothing_else() {
        let pin = tc_resources::suite::obs_images();
        let id = pack_id(pin);
        assert!(!id.contains('/'), "{id}");
        let named = pin_of_pack_id(&id).unwrap();
        assert_eq!((&named.repo, named.sha), (&pin.repo, pin.sha));
        assert_eq!(pin_of_pack_id("git.door43.org~uW~obs_images_360"), None);
        assert_eq!(pin_of_pack_id(&format!("{id}~x")), None);
        assert_eq!(
            pin_of_pack_id(&id.replace("git.door43.org", "example.org")),
            None
        );
    }

    #[test]
    fn only_a_picture_below_ingredients_is_served() {
        assert_eq!(
            picture_path("360px/obs-en-01-01.jpg").as_deref(),
            Some("ingredients/360px/obs-en-01-01.jpg")
        );
        for bad in [
            "../metadata.json",
            "360px/../../x.jpg",
            "/etc/passwd.jpg",
            "360px//a.jpg",
            "360px\\a.jpg",
            "metadata.json",
            "360px/a.jpg/",
            "",
        ] {
            assert_eq!(picture_path(bad), None, "{bad:?}");
        }
    }
}
