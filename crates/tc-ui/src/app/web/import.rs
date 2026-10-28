//! The actions of Import and of Update a book from USFM (unit F1; tC4 `importPickKind`,
//! `importAddFiles`, `importReview`, `importGoOnline`, `importUseInstalled`, `importRun`). The
//! forms hold the rules (`app::forms::ImportForm`, `app::update_book`); these carry them out.

use leptos::prelude::*;
use tc_api::import::{Review, Run, UpdateBook, Versions};
use tc_api::jobs::Cancel;
use tc_api::refusal::Message;
use tc_api::upload::UploadFlow;
use tc_api::{ApiError, Refusal};
use tc_core::BookId;
use wasm_bindgen_futures::spawn_local;

use super::app_ctx::{Actions, Dialog, ImportToast};
use super::js;
use crate::app::client::CallError;
use crate::app::forms::{ImportFailure, ImportKind, ImportState, ImportUpload};
use crate::app::internet::Pending;
use crate::app::jobs::JobEnd;
use crate::app::net::NetEnd;
use crate::app::update_book::{UpdateBookForm, UpdateState};
use crate::app::uploads::Slot;
use crate::i18n::{keys, t};

/// A refusal as the damaged panel and the failed step show it: every refusal through
/// `refusal_text` (INV-EVERY-REFUSAL-RENDERS), never its code alone (#833.3).
fn failure_of(refusal: &Refusal) -> ImportFailure {
    ImportFailure {
        code: refusal.code().map(str::to_owned),
        sentence: crate::i18n::refusal_text(refusal),
        detail: refusal.detail().map(str::to_owned),
    }
}

/// An error that is not a refusal, as the failed step shows it: tC4's general sentence, and
/// the error after it.
fn failure_of_error(error: &ApiError) -> ImportFailure {
    match error {
        ApiError::Refused(refusal) => failure_of(refusal),
        other => ImportFailure {
            code: None,
            sentence: t(keys::importer::failed::BODY, &[]),
            detail: Some(other.to_string()),
        },
    }
}

impl Actions {
    // ---- Import ----------------------------------------------------------------------------

    /// A card of the kind step (tC4 `importPickKind`).
    pub fn import_pick_kind(&self, kind: ImportKind) {
        self.patch_im(|im| im.pick_kind(kind));
    }

    pub fn import_back_to_kind(&self) {
        self.patch_im(|im| im.back_to_kind());
    }

    pub fn import_back_to_files(&self) {
        self.patch_im(|im| im.back_to_files());
    }

    /// Files chosen or dropped (tC4 `importAddFiles`): each goes to the server at once, after
    /// the size check the server makes too (A-28).
    pub fn import_add_files(&self, list: web_sys::FileList) {
        let files: Vec<web_sys::File> = (0..list.length()).filter_map(|n| list.get(n)).collect();
        for file in files {
            let name = file.name();
            #[expect(
                clippy::cast_possible_truncation,
                clippy::cast_sign_loss,
                reason = "a file's size is a whole number of bytes below 2^53"
            )]
            let bytes = file.size() as u64;
            let Some(id) = self
                .s()
                .im_slot()
                .try_update(|im| im.as_mut().map(|im| im.add_file(name.clone(), bytes)))
                .flatten()
            else {
                return;
            };
            let opening = self.upload_opening();
            let this = self.clone();
            spawn_local(async move {
                let upload = this.send_file(&file, &name, bytes).await;
                if let ImportUpload::Sent(upload) = &upload {
                    if !this.picked_upload(opening, Slot::Import(id), upload) {
                        return;
                    }
                    // Its row was removed while it was sent.
                    let listed = this.s().im_slot().with_untracked(|im| {
                        im.as_ref()
                            .is_some_and(|im| im.files.iter().any(|f| f.id == id))
                    });
                    if !listed {
                        this.removed_upload(Slot::Import(id));
                        return;
                    }
                }
                this.patch_im(|im| im.file_sent(id, upload));
            });
        }
    }

    /// One file to the server, with flow `import`.
    async fn send_file(&self, file: &web_sys::File, name: &str, bytes: u64) -> ImportUpload {
        if bytes > UploadFlow::Import.cap() {
            return ImportUpload::Failed(t(keys::importer::files::TOO_BIG, &[("file", &name)]));
        }
        match self.runtime().api().upload(file, UploadFlow::Import).await {
            Ok(view) => ImportUpload::Sent(view.id),
            Err(error) => ImportUpload::Failed(said(error)),
        }
    }

    pub fn import_remove_file(&self, id: u32) {
        self.patch_im(|im| im.remove_file(id));
        self.removed_upload(Slot::Import(id));
    }

    /// "Review files →" (tC4 `importReview`): the files are read on the server, which writes
    /// nothing. A file that cannot be imported shows on the review step, and Import stays off.
    pub fn import_review(&self) {
        let Some(files) = self
            .s()
            .im_slot()
            .try_update(|im| im.as_mut().and_then(|im| im.press_review()))
            .flatten()
        else {
            return;
        };
        let this = self.clone();
        spawn_local(async move {
            let asked = Review {
                files,
                details: None,
                use_installed: None,
                versification: None,
            };
            match this.runtime().api().call(&asked).await {
                Ok(review) => this.patch_im(|im| im.reviewed(review, js::random_request_id())),
                Err(CallError::Api(ApiError::Refused(refusal))) => {
                    let failure = failure_of(&refusal);
                    this.patch_im(|im| im.review_refused(failure));
                }
                Err(error) => {
                    let problem = said(error);
                    this.patch_im(|im| im.review_lost(problem));
                }
            }
        });
    }

    /// The Bible name of the review step.
    pub fn import_type_name(&self, name: String) {
        self.patch_im(|im| im.type_name(name, js::random_request_id()));
    }

    /// The language code of the review step.
    pub fn import_type_code(&self, code: String) {
        self.patch_im(|im| im.type_code(code, js::random_request_id()));
    }

    /// "Look up on Door43" (tC4 `importGoOnline`): the internet question first (D95).
    pub fn import_look_up(&self) {
        let ready = self.s().im_slot().with_untracked(|im| {
            im.as_ref().is_some_and(|im| {
                im.versions == crate::app::forms::VersionLookup::NotLookedUp
                    && matches!(im.reviewed, Some(crate::app::forms::Reviewed::Found(_)))
            })
        });
        if ready {
            self.internet_task(Pending::ImportVersions);
        }
    }

    /// The lookup, once the internet is on: `import/versions`, then its job's end.
    pub(super) fn import_versions_now(&self) {
        let request = js::random_request_id();
        let Some(files) = self
            .s()
            .im_slot()
            .try_update(|im| {
                let im = im.as_mut()?;
                im.look_up(request).then(|| im.sent())
            })
            .flatten()
        else {
            return;
        };
        let this = self.clone();
        spawn_local(async move {
            let asked = Versions { request, files };
            let open = {
                let im = this.s().im_slot();
                move || im.with_untracked(Option::is_some)
            };
            let ended = match this.run_net(Pending::ImportVersions, &asked).await {
                NetEnd::Ok(job) => this.follow::<Versions>(job, |_| {}, open).await,
                // The question or the sign-in step took over, and runs the lookup again after.
                NetEnd::Asked | NetEnd::Dropped => {
                    this.patch_im(|im| im.versions_not_looked_up());
                    return;
                }
                NetEnd::Failed(error) => JobEnd::Failed(error),
                NetEnd::Transport(error) => {
                    let problem = error.to_string();
                    this.patch_im(|im| im.versions_failed(request, problem));
                    return;
                }
            };
            match ended {
                JobEnd::Done(view) => this.patch_im(|im| im.versions_found(request, view)),
                JobEnd::Cancelled | JobEnd::Lost | JobEnd::Left => {
                    this.patch_im(|im| im.versions_not_looked_up());
                }
                JobEnd::Failed(error) => match this.net_refusal(Pending::ImportVersions, error) {
                    None => this.patch_im(|im| im.versions_not_looked_up()),
                    Some(ApiError::Refused(refusal)) => {
                        let problem = crate::i18n::refusal_text(&refusal);
                        this.patch_im(|im| im.versions_failed(request, problem));
                    }
                    Some(other) => {
                        let problem = other.to_string();
                        this.patch_im(|im| im.versions_failed(request, problem));
                    }
                },
            }
        });
    }

    /// "Use installed versions" (tC4 `importUseInstalled`): the server counts what carries
    /// over, and writes nothing.
    pub fn import_use_installed(&self) {
        let request = js::random_request_id();
        let Some(asked) = self
            .s()
            .im_slot()
            .try_update(|im| im.as_mut().and_then(|im| im.use_installed(request)))
            .flatten()
        else {
            return;
        };
        let this = self.clone();
        spawn_local(async move {
            match this.runtime().api().call(&asked).await {
                Ok(review) => this.patch_im(|im| im.carry_counted(request, review.carry_over)),
                Err(error) => {
                    let problem = said(error);
                    this.patch_im(|im| im.carry_failed(request, problem));
                }
            }
        });
    }

    /// The license picker of the review step.
    pub fn import_pick_license(&self, license: Option<String>) {
        self.patch_im(|im| im.pick_license(license, js::random_request_id()));
    }

    /// The versification picker of the review step.
    pub fn import_pick_versification(&self, name: String) {
        self.patch_im(|im| im.pick_versification(name, js::random_request_id()));
    }

    /// "Import as a new Bible" and "Try the import again" (tC4 `importRun`): one job that
    /// makes the whole project or none. `listing` is Home's, for the name check.
    pub fn import_run(&self, listing: Vec<String>) {
        let Some(run) = self
            .s()
            .im_slot()
            .try_update(|im| im.as_mut()?.press_run(listing.iter().map(String::as_str)))
            .flatten()
        else {
            return;
        };
        self.uploads_taken();
        let this = self.clone();
        spawn_local(async move {
            let name = run.details.name.clone();
            let open = {
                let im = this.s().im_slot();
                move || im.with_untracked(Option::is_some)
            };
            let ended = match this.runtime().api().call(&run).await {
                Ok(job) => {
                    this.patch_im(|im| im.run_started(job));
                    let progress = |p: &tc_api::jobs::Progress| {
                        let p = p.clone();
                        this.patch_im(|im| im.run_progress(p));
                    };
                    this.follow::<Run>(job, progress, open).await
                }
                Err(CallError::Api(error)) => JobEnd::Failed(error),
                Err(CallError::Dropped) => JobEnd::Left,
                Err(error @ CallError::Transport(_)) => {
                    let problem = error.to_string();
                    this.patch_im(|im| im.run_lost(problem));
                    return;
                }
            };
            match ended {
                JobEnd::Done(project) => {
                    this.uploads_finished();
                    let books = this.s().im_slot().with_untracked(|im| {
                        match im.as_ref().and_then(|im| im.reviewed.as_ref()) {
                            Some(crate::app::forms::Reviewed::Found(review)) => review
                                .found
                                .books
                                .iter()
                                .map(|b| b.book.english_name())
                                .collect::<Vec<_>>()
                                .join(" · "),
                            _ => String::new(),
                        }
                    });
                    this.close_modal();
                    this.s().import_toast.try_set(Some(ImportToast {
                        message: t(keys::importer::DONE, &[("name", &name), ("books", &books)]),
                    }));
                    this.s().imported_repo.try_set(Some(project));
                    this.refresh_projects();
                }
                JobEnd::Failed(error) => {
                    // An import that did not make its project keeps its files for a retry.
                    this.uploads_given_back();
                    let failure = failure_of_error(&error);
                    this.patch_im(|im| im.run_refused(failure, js::random_request_id()));
                    this.refresh_projects();
                }
                JobEnd::Cancelled | JobEnd::Lost | JobEnd::Left => {
                    this.uploads_given_back();
                    this.patch_im(|im| im.run_stopped(js::random_request_id()));
                    this.refresh_projects();
                }
            }
        });
    }

    /// Cancel on the working step: the job stops at its next check, and leaves no project.
    pub fn import_cancel(&self) {
        let job = self
            .s()
            .im_slot()
            .with_untracked(|im| match im.as_ref().map(|im| &im.state) {
                Some(ImportState::Running { job, .. }) => *job,
                _ => None,
            });
        self.cancel_job(job);
    }

    fn cancel_job(&self, job: Option<tc_api::ids::JobId>) {
        let Some(job) = job else { return };
        let this = self.clone();
        spawn_local(async move {
            let cancel = Cancel {
                request: js::random_request_id(),
                job,
            };
            // The follow sees the job end either way; a failed cancel leaves it running.
            let _ = this.runtime().api().call(&cancel).await;
        });
    }

    // ---- Update a book from USFM -----------------------------------------------------------

    /// The account menu's "Update a book from USFM", on the open Bible, at the book on screen.
    pub fn open_update_book(&self) {
        let s = self.s();
        let Some(at) = s
            .project
            .with_untracked(|p| p.as_ref().map(|p| p.at.clone()))
        else {
            return;
        };
        let book = s.u.book.get_untracked().unwrap_or(BookId::Genesis);
        let form = UpdateBookForm::open(at, book, js::random_request_id());
        s.layers.dialog.try_set(Some(Dialog::UpdateBook(form)));
    }

    pub fn patch_ub(&self, patch: impl FnOnce(&mut UpdateBookForm)) {
        self.s().layers.dialog.try_update(|dialog| {
            if let Some(Dialog::UpdateBook(ub)) = dialog {
                patch(ub);
            }
        });
    }

    /// A file chosen: it replaces the one before and goes to the server at once.
    pub fn update_book_file(&self, list: web_sys::FileList) {
        let Some(file) = list.get(0) else { return };
        let name = file.name();
        #[expect(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "a file's size is a whole number of bytes below 2^53"
        )]
        let bytes = file.size() as u64;
        let chosen = self
            .s()
            .ub_slot()
            .try_update(|ub| {
                ub.as_mut().map(|ub| {
                    let replaced = ub.file.as_ref().map(|f| f.id);
                    (
                        replaced,
                        ub.choose_file(name.clone(), bytes, js::random_request_id()),
                    )
                })
            })
            .flatten();
        let Some((replaced, id)) = chosen else {
            return;
        };
        if let Some(replaced) = replaced {
            self.removed_upload(Slot::UpdateBook(replaced));
        }
        let opening = self.upload_opening();
        let this = self.clone();
        spawn_local(async move {
            let upload = this.send_file(&file, &name, bytes).await;
            if let ImportUpload::Sent(upload) = &upload {
                if !this.picked_upload(opening, Slot::UpdateBook(id), upload) {
                    return;
                }
                // Another file was chosen while this one was sent.
                let current = this.s().ub_slot().with_untracked(|ub| {
                    ub.as_ref().and_then(|ub| ub.file.as_ref()).map(|f| f.id) == Some(id)
                });
                if !current {
                    this.removed_upload(Slot::UpdateBook(id));
                    return;
                }
            }
            this.patch_ub(|ub| ub.file_sent(id, upload));
        });
    }

    pub fn update_book_choose(&self, book: BookId) {
        self.patch_ub(|ub| ub.choose_book(book, js::random_request_id()));
    }

    /// "Update the book": `import/updateBook`, then its job's end and the report.
    pub fn update_book_run(&self) {
        let Some(run) = self
            .s()
            .ub_slot()
            .try_update(|ub| ub.as_mut().and_then(UpdateBookForm::press_run))
            .flatten()
        else {
            return;
        };
        self.uploads_taken();
        let this = self.clone();
        spawn_local(async move {
            let open = {
                let ub = this.s().ub_slot();
                move || ub.with_untracked(Option::is_some)
            };
            let ended = match this.runtime().api().call(&run).await {
                Ok(job) => {
                    this.patch_ub(|ub| ub.run_started(job));
                    let progress = |p: &tc_api::jobs::Progress| {
                        let p = p.clone();
                        this.patch_ub(|ub| ub.run_progress(p));
                    };
                    this.follow::<UpdateBook>(job, progress, open).await
                }
                Err(CallError::Api(error)) => JobEnd::Failed(error),
                Err(CallError::Dropped) => JobEnd::Left,
                Err(error @ CallError::Transport(_)) => {
                    let problem = error.to_string();
                    this.patch_ub(|ub| ub.run_lost(problem));
                    return;
                }
            };
            match ended {
                JobEnd::Done(report) => {
                    this.uploads_finished();
                    this.patch_ub(|ub| ub.run_done(report));
                    this.refresh_projects();
                }
                JobEnd::Failed(error) => {
                    // A refusal before the job leaves the file; a job deleted it already, and
                    // dropping it then is harmless.
                    this.uploads_given_back();
                    let failure = failure_of_error(&error);
                    this.patch_ub(|ub| ub.run_refused(failure, js::random_request_id()));
                }
                JobEnd::Cancelled | JobEnd::Lost | JobEnd::Left => {
                    this.uploads_given_back();
                    this.patch_ub(|ub| ub.run_stopped(js::random_request_id()));
                }
            }
        });
    }

    pub fn update_book_cancel(&self) {
        let job = self
            .s()
            .ub_slot()
            .with_untracked(|ub| match ub.as_ref().map(|ub| &ub.state) {
                Some(UpdateState::Running { job, .. }) => *job,
                _ => None,
            });
        self.cancel_job(job);
    }

    /// The report's "Review the changes": the conflict review over Translate, at the book.
    pub fn update_book_review(&self) {
        let Some(book) =
            self.s()
                .ub_slot()
                .with_untracked(|ub| match ub.as_ref().map(|ub| &ub.state) {
                    Some(UpdateState::Done(report)) => Some(report.book),
                    _ => None,
                })
        else {
            return;
        };
        let Some(project) = self
            .s()
            .project
            .with_untracked(|p| p.as_ref().map(|p| p.at.project.clone()))
        else {
            return;
        };
        self.close_modal();
        self.open_project_conflicts(project, Some(book), tc_api::conflicts::ReviewTab::Text);
    }
}

/// What the person reads of an error with no refusal to render: why no answer came.
fn said(error: CallError) -> String {
    match error {
        CallError::Api(ApiError::Refused(refusal)) => crate::i18n::refusal_text(&refusal),
        other => other.to_string(),
    }
}
