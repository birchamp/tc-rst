//! A file the person chose, as a request reads it (#858). Holding the upload is the one way a
//! request gets its file: no drop or sweep deletes a held upload, so a dialog that closes, or a
//! late answer that drops its files, never deletes a file a running job reads.

use std::path::Path;

use tc_api::error::ApiError;
use tc_api::ids::UploadId;

use crate::app::App;
use crate::fixes::{UploadedFile, Uploads};

/// An upload a request holds. Dropped without [`HeldUpload::used_up`], it is given back, so a
/// failed, refused or cancelled request leaves the person's file for another try.
pub(crate) struct HeldUpload {
    id: UploadId,
    file: UploadedFile,
    /// None once used up.
    uploads: Option<Uploads>,
}

impl HeldUpload {
    pub(crate) fn path(&self) -> &Path {
        &self.file.path
    }

    /// The request is done with the file: it is deleted. Blocking: one unlink.
    pub(crate) fn used_up(mut self) {
        if let Some(uploads) = self.uploads.take() {
            uploads.used_up(&self.id);
        }
    }
}

impl Drop for HeldUpload {
    fn drop(&mut self) {
        if let Some(uploads) = self.uploads.take() {
            uploads.give_back(&self.id);
        }
    }
}

fn no_such_upload() -> ApiError {
    ApiError::BadRequest {
        field: "upload".to_owned(),
        reason: "no such upload".to_owned(),
    }
}

impl App {
    /// Holds the upload before its first read.
    pub(crate) fn hold_upload(&self, id: &UploadId) -> Result<HeldUpload, ApiError> {
        let uploads = self.inner.uploads.get().ok_or_else(no_such_upload)?;
        let file = uploads.take(id).ok_or_else(no_such_upload)?;
        Ok(HeldUpload {
            id: id.clone(),
            file,
            uploads: Some(uploads.clone()),
        })
    }

    /// Holds every upload, or none: the ones held before a missing one are given back.
    pub(crate) fn hold_uploads<'a>(
        &self,
        ids: impl IntoIterator<Item = &'a UploadId>,
    ) -> Result<Vec<HeldUpload>, ApiError> {
        ids.into_iter().map(|id| self.hold_upload(id)).collect()
    }
}

/// Every held upload is used up.
pub(crate) fn used_up(held: Vec<HeldUpload>) {
    held.into_iter().for_each(HeldUpload::used_up);
}
