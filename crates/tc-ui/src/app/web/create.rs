//! The actions of New Bible, New Open Bible Stories, Add a book and Project settings' Save
//! (unit C6; tC4 `createBible`, `createObs`, `addBooks`, `abAddUsfmFiles`, `addUsfmBooks`,
//! `saveSettings`).
//!
//! Each confirming button is disabled while its request is in flight, and a press sends the id
//! of its attempt: a press after a reply that never came sends the same id, so the server answers
//! from its reply cache and nothing is made twice (APP-DESIGN A-27). The forms hold that rule
//! (`press`, `answered`, `lost`); these actions only carry it out.

use leptos::prelude::*;
use tc_api::ApiError;
use tc_api::Refusal;
use tc_api::create::{AddBlankBooks, AddUsfmBooks, Bible, Obs, ReviewUsfm, UsfmFile, UsfmOutcome};
use tc_api::refusal::{CreateRefusal, Message};
use tc_api::upload::UploadFlow;
use tc_core::BookId;
use wasm_bindgen_futures::spawn_local;

use super::app_ctx::{Actions, Dialog, OpenTarget};
use super::js;
use crate::app::client::CallError;
use crate::app::forms::{AddBookForm, AddBookStep, NewKind, UsfmState};
use crate::app::uploads::{Opening, Slot};
use crate::i18n::{keys, t, t_dyn};

/// What a refusal says: its sentence, with the diagnosis after it where it has one. A create
/// that failed for its own reason says that reason alone, as tC4 does.
fn words(refusal: &Refusal) -> String {
    if let Refusal::Create(CreateRefusal::Failed { reason }) = refusal {
        return reason.clone();
    }
    let sentence = crate::i18n::refusal_text(refusal);
    match refusal.detail() {
        Some(detail) => format!("{sentence} {detail}"),
        None => sentence,
    }
}

/// What the person reads of an error that came back: an answer's words, or why none came.
enum Said {
    /// The server answered: the attempt is over.
    Answered(String),
    /// No answer: the attempt may have been carried out.
    Lost(String),
}

fn said(error: CallError) -> Said {
    match error {
        CallError::Api(ApiError::Refused(refusal)) => Said::Answered(words(&refusal)),
        CallError::Api(other) => Said::Answered(other.to_string()),
        other => Said::Lost(other.to_string()),
    }
}

impl Actions {
    /// New Bible's Create (tC4 `createBible`).
    pub fn create_bible(&self) {
        self.create_project(NewKind::Bible);
    }

    /// New Open Bible Stories' Create (tC4 `createObs`).
    pub fn create_obs(&self) {
        self.create_project(NewKind::Stories);
    }

    fn create_project(&self, kind: NewKind) {
        let pressed = self
            .s()
            .np_slot()
            .try_update(|np| {
                let np = np.as_mut()?;
                let versification = np.versification.clone();
                Some(
                    np.press(js::random_request_id)?
                        .map(|(request, details)| (request, details, versification)),
                )
            })
            .flatten();
        let (request, details, versification) = match pressed {
            None => return,
            Some(Err(problem)) => {
                let error = words(&Refusal::Create(problem));
                self.s().np_slot().try_update(|np| {
                    if let Some(np) = np {
                        np.error = Some(error);
                    }
                });
                return;
            }
            Some(Ok(pressed)) => pressed,
        };
        let this = self.clone();
        spawn_local(async move {
            let name = details.name.trim().to_owned();
            let api = this.runtime().api();
            let made = match kind {
                NewKind::Bible => {
                    api.call(&Bible {
                        request,
                        details,
                        versification,
                    })
                    .await
                }
                NewKind::Stories => api.call(&Obs { request, details }).await,
            };
            match made {
                Ok(created) => {
                    let open = this.s().np_slot().with_untracked(|np| {
                        np.as_ref().is_some_and(|np| np.awaits(kind, request))
                    });
                    this.refresh_projects();
                    if open && kind == NewKind::Bible {
                        // "You'll add books next" (tC4): Add a book opens for the new project.
                        let form = AddBookForm::of_new(created.project, name);
                        this.s().layers.dialog.try_set(Some(Dialog::AddBook(form)));
                    } else if open {
                        this.close_modal();
                    }
                }
                Err(error) => {
                    this.s().np_slot().try_update(|np| {
                        let Some(np) = np.as_mut().filter(|np| np.awaits(kind, request)) else {
                            return;
                        };
                        match said(error) {
                            Said::Answered(error) => np.answered(Some(error)),
                            Said::Lost(error) => np.lost(error),
                        }
                    });
                }
            }
        });
    }

    // ---- Add a book -------------------------------------------------------------------------

    /// The method step's two cards, and Back (tC4 `patchAb({ step })`).
    pub fn ab_choose(&self, step: AddBookStep) {
        self.patch_ab(|ab| {
            ab.step = step;
            ab.error = None;
        });
    }

    /// The single pick of the pick step.
    pub fn ab_pick_book(&self, book: BookId) {
        self.patch_ab(|ab| {
            ab.book = book;
            ab.request = None;
        });
    }

    /// "Choose several at once" and "Pick one book".
    pub fn ab_toggle_multi(&self) {
        self.patch_ab(|ab| {
            ab.multi = !ab.multi;
            ab.request = None;
        });
    }

    /// A chip of the several-at-once grid.
    pub fn ab_toggle_book(&self, book: BookId) {
        self.patch_ab(|ab| {
            ab.toggle(book);
            ab.request = None;
        });
    }

    /// "Create book" (tC4 `addBooks`): each picked book the project does not hold is a blank
    /// book, and the project opens at the first.
    pub fn add_books(&self) {
        let pressed = self
            .s()
            .ab_slot()
            .try_update(|ab| {
                let ab = ab.as_mut()?;
                let project = ab.project.clone();
                Some(
                    ab.press_blank(js::random_request_id)?
                        .map(|ok| (project, ok)),
                )
            })
            .flatten();
        let (project, request, books) = match pressed {
            None => return,
            Some(Err(())) => {
                self.patch_ab(|ab| ab.error = Some(t(keys::add_book::PICK_ONE, &[])));
                return;
            }
            Some(Ok((project, (request, books)))) => (project, request, books),
        };
        let this = self.clone();
        spawn_local(async move {
            let asked = AddBlankBooks {
                request,
                project: project.clone(),
                books,
            };
            let mine = |ab: &AddBookForm| ab.project == project;
            match this.runtime().api().call(&asked).await {
                Ok(added) => {
                    let open = this
                        .s()
                        .ab_slot()
                        .with_untracked(|ab| ab.as_ref().is_some_and(mine));
                    this.refresh_projects();
                    if open {
                        this.close_modal();
                        if let Some(first) = added.added.first() {
                            this.open_project_at(project, OpenTarget::Book(*first));
                        }
                    }
                }
                Err(error) => {
                    this.uploads_given_back();
                    this.patch_ab(|ab| {
                        if mine(ab) {
                            match said(error) {
                                Said::Answered(error) => ab.added(&[], Some(error)),
                                Said::Lost(error) => ab.lost(error),
                            }
                        }
                    });
                }
            }
        });
    }

    /// Files picked or dropped (tC4 `abAddUsfmFiles`): each file goes to the server at once and
    /// its row says what it is when the server has read it.
    pub fn ab_add_files(&self, list: web_sys::FileList) {
        let files: Vec<web_sys::File> = (0..list.length()).filter_map(|n| list.get(n)).collect();
        for file in files {
            let name = file.name();
            let added = self
                .s()
                .ab_slot()
                .try_update(|ab| ab.as_mut().map(|ab| ab.add_file(name.clone())))
                .flatten();
            let Some(id) = added else { return };
            let opening = self.upload_opening();
            let this = self.clone();
            spawn_local(async move {
                let state = this.review_file(&file, &name, opening, id).await;
                this.patch_ab(|ab| ab.file_read(id, state));
            });
        }
    }

    /// Sends one file and asks the server what it is.
    async fn review_file(
        &self,
        file: &web_sys::File,
        name: &str,
        opening: Opening,
        row: u32,
    ) -> UsfmState {
        let api = self.runtime().api();
        if file.size() > UploadFlow::Usfm.cap() as f64 {
            return UsfmState::Unreadable(t(keys::wizard::ERROR, &[]));
        }
        let upload = match api.upload(file, UploadFlow::Usfm).await {
            Ok(upload) => upload,
            Err(error) => return UsfmState::Unreadable(unreadable(error)),
        };
        if !self.picked_upload(opening, Slot::Usfm(row), &upload.id) {
            return UsfmState::Unreadable(String::new());
        }
        // Its row was removed while it was sent.
        let listed = self.s().ab_slot().with_untracked(|ab| {
            ab.as_ref()
                .is_some_and(|ab| ab.files.iter().any(|f| f.id == row))
        });
        if !listed {
            self.removed_upload(Slot::Usfm(row));
            return UsfmState::Unreadable(String::new());
        }
        let asked = ReviewUsfm {
            file: UsfmFile {
                upload: upload.id.clone(),
                name: name.to_owned(),
            },
        };
        match api.call(&asked).await {
            Ok(verdict) => UsfmState::Read {
                upload: upload.id,
                verdict,
            },
            Err(error) => UsfmState::Unreadable(unreadable(error)),
        }
    }

    /// The ✕ of a row (tC4 `abRemoveUsfmFile`).
    pub fn ab_remove_file(&self, id: u32) {
        self.patch_ab(|ab| ab.remove_file(id));
        self.removed_upload(Slot::Usfm(id));
    }

    /// "Add book" and "Add n books" (tC4 `addUsfmBooks`): the valid files are added, each as
    /// one book whole or not at all; a file that was not added stays in its row.
    pub fn add_usfm_books(&self) {
        let pressed = self
            .s()
            .ab_slot()
            .try_update(|ab| {
                let ab = ab.as_mut()?;
                let project = ab.project.clone();
                ab.press_usfm(js::random_request_id)
                    .map(|(request, files)| (project, request, files))
            })
            .flatten();
        let Some((project, request, files)) = pressed else {
            return;
        };
        self.uploads_taken();
        let this = self.clone();
        spawn_local(async move {
            let asked = AddUsfmBooks {
                request,
                project: project.clone(),
                files,
            };
            let mine = |ab: &AddBookForm| ab.project == project;
            match this.runtime().api().call(&asked).await {
                Ok(done) => {
                    // Each file was read and deleted, added or not.
                    this.uploads_finished();
                    this.refresh_projects();
                    let mut added = Vec::new();
                    let mut problems = Vec::new();
                    for row in done.rows {
                        match row.outcome {
                            UsfmOutcome::Added { book } => added.push((row.name, book)),
                            UsfmOutcome::NotAdded { reason } => problems.push(t(
                                keys::add_book::NOT_ADDED,
                                &[("file", &row.name), ("reason", &reason)],
                            )),
                            UsfmOutcome::Refused(problem) => {
                                let owned = problem.vars(&row.name);
                                let vars: Vec<(&str, &dyn std::fmt::Display)> = owned
                                    .iter()
                                    .map(|(k, v)| (*k, v as &dyn std::fmt::Display))
                                    .collect();
                                problems.push(t_dyn(problem.key(), &vars, None));
                            }
                        }
                    }
                    let open = this
                        .s()
                        .ab_slot()
                        .with_untracked(|ab| ab.as_ref().is_some_and(mine));
                    if open && problems.is_empty() {
                        this.close_modal();
                    } else {
                        let error = (!problems.is_empty()).then(|| problems.join(" "));
                        this.patch_ab(|ab| {
                            if mine(ab) {
                                ab.added(&added, error);
                            }
                        });
                    }
                }
                Err(error) => this.patch_ab(|ab| {
                    if mine(ab) {
                        match said(error) {
                            Said::Answered(error) => ab.added(&[], Some(error)),
                            Said::Lost(error) => ab.lost(error),
                        }
                    }
                }),
            }
        });
    }

    // ---- Project settings -------------------------------------------------------------------

    /// Save changes (tC4 `saveSettings`): the project opens through the registry's one open
    /// (AD14), and only what changed is written. The dialog closes when it is saved.
    pub fn save_settings(&self) {
        let pressed = self
            .s()
            .st_slot()
            .try_update(|st| st.as_mut()?.press_save(js::random_request_id))
            .flatten();
        let Some(save) = pressed else { return };
        let this = self.clone();
        spawn_local(async move {
            let project = save.project.clone();
            match this.runtime().api().call(&save).await {
                Ok(()) => {
                    this.refresh_projects();
                    // Another project's dialog opened meanwhile stays (tC4 #641).
                    let same = this
                        .s()
                        .st_slot()
                        .with_untracked(|st| st.as_ref().is_some_and(|st| st.project == project));
                    if same {
                        this.close_modal();
                    }
                }
                Err(error) => {
                    this.s().st_slot().try_update(|st| {
                        let Some(st) = st else { return };
                        match said(error) {
                            Said::Answered(error) => st.answered(&project, Some(error)),
                            Said::Lost(error) => st.lost(&project, error),
                        }
                    });
                }
            }
        });
    }
}

/// Why a file could not be sent or read, in words.
fn unreadable(error: CallError) -> String {
    match said(error) {
        Said::Answered(words) | Said::Lost(words) => words,
    }
}
