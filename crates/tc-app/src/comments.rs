//! The Understand comment box commits only on blur; pauses use the editor draft path.

use tc_api::editor::{AddComment, CommentTarget};
use tc_api::ids::WallMs;
use tc_api::write::Ack;
use tc_api::{ApiError, NotSaved};
use tc_journal::{ChapterNo, FrameNo, Record, RecordKey, VerseNo};

use crate::app::App;
use crate::backend::{EditToken, StoreCommand};
use crate::counters::CountingSnapshot;
use crate::dispatch::Handle;
use crate::session::ProjectSession;
use crate::wire;

pub(crate) fn target(target: CommentTarget) -> Result<tc_journal::record::CommentTarget, ApiError> {
    let invalid = || ApiError::BadRequest {
        field: "target".into(),
        reason: "a comment needs a verse or story frame anchor".into(),
    };
    Ok(match target {
        CommentTarget::Verse {
            book,
            chapter,
            verse,
        } => tc_journal::record::CommentTarget::Verse {
            book,
            chapter: ChapterNo::new(chapter).ok_or_else(invalid)?,
            verse: VerseNo::new(verse).ok_or_else(invalid)?,
        },
        CommentTarget::Frame { story, frame } => tc_journal::record::CommentTarget::Frame {
            story: tc_journal::StoryNo::new(story.get()).ok_or_else(invalid)?,
            frame: FrameNo::new(frame),
        },
    })
}

pub(crate) fn matches_target(
    read: &CountingSnapshot,
    shown: &EditToken,
    target: &tc_journal::record::CommentTarget,
) -> bool {
    matches!(shown.key(), RecordKey::Comment(_)) && shown.heads().first()
        .and_then(|head| read.body(head))
        .is_some_and(|body| matches!(body.record, Record::Comment { value, .. } if value.target == *target))
}

impl App {
    pub(crate) fn shown_token(
        &self,
        read: &CountingSnapshot,
        key: &RecordKey,
    ) -> Result<tc_api::ShownToken, ApiError> {
        read.mint(key, None)
            .map_err(|error| error.to_string())
            .and_then(|token| wire::token::shown(&token).map_err(|error| error.to_string()))
            .map_err(|reason| {
                self.internal(
                    "an editor token could not be made",
                    crate::reports::facts(
                        serde_json::json!({"record": format!("{key:?}"), "reason": reason}),
                    ),
                )
            })
    }
    pub(crate) fn record_comment_edit(
        &self,
        session: &ProjectSession,
        target: &tc_journal::record::CommentTarget,
        text: &str,
        at: WallMs,
    ) {
        let unit = match target {
            tc_journal::record::CommentTarget::Verse {
                book,
                chapter,
                verse,
            } => {
                let Ok(verse) = tc_api::ids::RefPart::parse(&verse.get().to_string()) else {
                    return;
                };
                crate::prefs::EditUnit::Book {
                    book: *book,
                    chapter: chapter.get(),
                    verse,
                }
            }
            tc_journal::record::CommentTarget::Frame { story, frame } => {
                crate::prefs::EditUnit::Story {
                    story: *story,
                    frame: frame.get(),
                }
            }
            tc_journal::record::CommentTarget::Check(_) => return,
        };
        let folder = session.scope().project.clone();
        let snippet = text
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
            .chars()
            .take(160)
            .collect();
        self.inner.prefs.record_last_used(folder.clone(), at);
        self.inner.prefs.record_last_edit(
            session.id().clone(),
            crate::prefs::LastEdit {
                project: folder,
                unit,
                mode: tc_api::prefs::PlaceMode::Read,
                tool: None,
                snippet,
                at,
            },
        );
    }
}

impl Handle<AddComment> for App {
    async fn handle(&self, req: AddComment) -> Result<Ack, ApiError> {
        let session = self
            .inner
            .sessions
            .reader(&req.at)
            .map_err(|_| ApiError::NotSaved(NotSaved::StaleScope))?;
        let handle = session.handle();
        let target = target(req.target)?;
        let shown = req
            .shown
            .as_ref()
            .map(|token| wire::token::edit_token(handle.as_ref(), token))
            .transpose()?;
        let command = match &shown {
            Some(token) if matches_target(&session.read(), token, &target) => {
                StoreCommand::revise_comment(token.clone(), req.text.clone())
            }
            Some(_) => return Err(ApiError::NotSaved(NotSaved::BadToken)),
            None => StoreCommand::add_comment(target.clone(), req.text.clone()),
        }
        .requested_by(req.request);
        let reply =
            handle.submit(command).await.map_err(|error| {
                match wire::error::command_error(error) {
                    Ok(api) => api,
                    Err(error) => self.internal(
                        "the store refused a user comment",
                        crate::reports::facts(serde_json::json!({"reason": error.0})),
                    ),
                }
            })?;
        self.record_comment_edit(&session, &target, &req.text, self.inner.clock.now());
        wire::write::record_ack(
            &reply,
            &session.read(),
            shown.as_ref(),
            &req.text,
            handle.facts().device,
        )
        .map_err(|error| {
            self.internal(
                "a comment save reply could not be written",
                crate::reports::facts(serde_json::json!({"reason": error.to_string()})),
            )
        })
    }
}
