//! The files the person picked in a dialog, from their upload to the end of their use (review
//! 2026-10-28 #7). An upload stays on the server until a job takes it or the page drops it, so
//! the page drops each one as soon as nothing in it can use the file: its dialog closed (in any
//! way, including leaving the project), its row was removed, or another file took its slot.
//! A file a job was started with is the job's: closing the dialog then drops nothing, and the
//! server leaves a taken upload alone anyway. Pure; the page sends `uploads/drop` for what a
//! transition returns.

use std::collections::BTreeMap;

use tc_api::ids::UploadId;

/// Where a picked file sits in the open dialog. A file picked into a slot that holds one
/// replaces it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Slot {
    /// Receive work from a file: the one exchange file.
    Receive,
    /// Update a book from USFM: the file, by the id the form gave it. A file chosen again is
    /// a new row; the one it replaces is removed.
    UpdateBook(u32),
    /// The guided fix's sideload: the last file sent.
    Fix,
    /// Import: a file row, by its id.
    Import(u32),
    /// Add a book from USFM: a file row, by its id.
    Usfm(u32),
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Held {
    /// Uploaded, and the dialog may still use it.
    Picked(UploadId),
    /// A request that reads it was sent: the job or the server deletes it, not the page.
    Taken(UploadId),
}

/// Which opening of a dialog an upload or a request was started in. Every answer is checked
/// against it ([`PickedUploads::is_current`]): one that comes after its dialog closed never
/// changes the dialog open now.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Opening(u32);

/// The files one request was sent with, and the dialog that sent it: its answer acts on these
/// files only, and only while that dialog is open.
#[derive(Debug, Clone, PartialEq, Eq)]
#[must_use = "the request's answer needs it"]
pub struct Sent {
    opening: Opening,
    ids: Vec<UploadId>,
}

impl Sent {
    /// The dialog that sent the request: its answer changes that dialog only, checked with
    /// [`PickedUploads::is_current`].
    pub fn opening(&self) -> Opening {
        self.opening
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PickedUploads {
    opening: u32,
    held: BTreeMap<Slot, Held>,
}

impl PickedUploads {
    /// The dialog open now; pass it to [`PickedUploads::pick`] when its upload lands.
    pub fn opening(&self) -> Opening {
        Opening(self.opening)
    }

    /// Whether `opening` is the dialog open now: the one check every late answer goes
    /// through.
    pub fn is_current(&self, opening: Opening) -> bool {
        opening == self.opening()
    }

    /// An upload stored for `slot`, started in `opening`. Returns what to drop: the picked
    /// file it replaces, or this one when its dialog closed while it was sent.
    pub fn pick(&mut self, opening: Opening, slot: Slot, id: UploadId) -> Vec<UploadId> {
        if !self.is_current(opening) {
            return vec![id];
        }
        match self.held.insert(slot, Held::Picked(id)) {
            Some(Held::Picked(old)) => vec![old],
            Some(Held::Taken(_)) | None => Vec::new(),
        }
    }

    /// The row's ✕: its picked file is dropped.
    pub fn remove(&mut self, slot: Slot) -> Vec<UploadId> {
        match self.held.remove(&slot) {
            Some(Held::Picked(id)) => vec![id],
            Some(Held::Taken(_)) | None => Vec::new(),
        }
    }

    /// The dialog's request that reads its picked files was sent: they are the server's now.
    pub fn take(&mut self) -> Sent {
        let mut ids = Vec::new();
        for held in self.held.values_mut() {
            if let Held::Picked(id) = held {
                ids.push(id.clone());
                *held = Held::Taken(id.clone());
            }
        }
        Sent {
            opening: self.opening(),
            ids,
        }
    }

    /// The request failed, and the server gave its files back. The dialog that sent it keeps
    /// them, for another try or for Close. If that dialog closed meanwhile, nothing can use
    /// them any more: they are returned, to drop.
    pub fn give_back(&mut self, sent: &Sent) -> Vec<UploadId> {
        if !self.is_current(sent.opening) {
            return sent.ids.clone();
        }
        for held in self.held.values_mut() {
            if let Held::Taken(id) = held
                && sent.ids.contains(id)
            {
                *held = Held::Picked(id.clone());
            }
        }
        Vec::new()
    }

    /// The job ended and deleted its files: nothing is left to drop.
    pub fn finish(&mut self, sent: &Sent) {
        if self.is_current(sent.opening) {
            self.held
                .retain(|_, held| !matches!(held, Held::Taken(id) if sent.ids.contains(id)));
        }
    }

    /// The dialog closed, whichever way: every picked file is dropped, a taken one is left to
    /// its job, and an upload or answer still on its way is for a closed dialog.
    pub fn close(&mut self) -> Vec<UploadId> {
        self.opening = self.opening.wrapping_add(1);
        std::mem::take(&mut self.held)
            .into_values()
            .filter_map(|held| match held {
                Held::Picked(id) => Some(id),
                Held::Taken(_) => None,
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn id(n: u8) -> UploadId {
        UploadId::parse(&format!("{n:032x}")).unwrap()
    }

    fn picked(slots: &[(Slot, u8)]) -> PickedUploads {
        let mut uploads = PickedUploads::default();
        for (slot, n) in slots {
            assert!(uploads.pick(uploads.opening(), *slot, id(*n)).is_empty());
        }
        uploads
    }

    #[test]
    fn a_picked_file_is_kept_until_something_ends_its_use() {
        let mut uploads = picked(&[(Slot::Receive, 1)]);
        assert_eq!(uploads.close(), [id(1)]);
    }

    #[test]
    fn a_file_picked_into_a_full_slot_drops_the_one_it_replaces() {
        let mut uploads = picked(&[(Slot::Receive, 1)]);
        let opening = uploads.opening();
        assert_eq!(uploads.pick(opening, Slot::Receive, id(2)), [id(1)]);
        assert_eq!(uploads.close(), [id(2)]);
    }

    #[test]
    fn a_file_picked_over_a_taken_one_leaves_the_taken_one_to_its_job() {
        let mut uploads = picked(&[(Slot::Receive, 1)]);
        let _ = uploads.take();
        let opening = uploads.opening();
        assert!(uploads.pick(opening, Slot::Receive, id(2)).is_empty());
        assert_eq!(uploads.close(), [id(2)]);
    }

    #[test]
    fn a_removed_row_drops_its_file_and_only_its_file() {
        let mut uploads = picked(&[(Slot::Usfm(1), 1), (Slot::Usfm(2), 2)]);
        assert_eq!(uploads.remove(Slot::Usfm(1)), [id(1)]);
        assert!(uploads.remove(Slot::Usfm(1)).is_empty(), "removed once");
        assert!(
            uploads.remove(Slot::Usfm(9)).is_empty(),
            "a row with no file"
        );
        assert_eq!(uploads.close(), [id(2)]);
    }

    #[test]
    fn a_taken_file_is_never_dropped() {
        let mut uploads = picked(&[(Slot::Import(1), 1), (Slot::Import(2), 2)]);
        let _ = uploads.take();
        assert!(uploads.remove(Slot::Import(1)).is_empty());
        assert!(uploads.close().is_empty());
    }

    #[test]
    fn a_failed_request_gives_its_files_back_and_closing_then_drops_them() {
        let mut uploads = picked(&[(Slot::Import(1), 1), (Slot::Import(2), 2)]);
        let sent = uploads.take();
        assert!(uploads.give_back(&sent).is_empty());
        assert_eq!(uploads.close(), [id(1), id(2)]);
    }

    #[test]
    fn a_finished_job_leaves_nothing_to_drop() {
        let mut uploads = picked(&[(Slot::Usfm(1), 1)]);
        let sent = uploads.take();
        uploads.finish(&sent);
        assert!(uploads.give_back(&sent).is_empty());
        assert!(uploads.close().is_empty());
    }

    #[test]
    fn an_upload_that_lands_after_its_dialog_closed_is_dropped_at_once() {
        let mut uploads = PickedUploads::default();
        let opening = uploads.opening();
        assert!(uploads.close().is_empty());
        assert_eq!(uploads.pick(opening, Slot::Receive, id(1)), [id(1)]);
        assert!(uploads.close().is_empty(), "never held");
    }

    /// #858: a failed import that answers after a new dialog picked files leaves the new
    /// dialog's files picked, and registered: nothing of the new dialog is dropped, and the
    /// old request's own files, which nothing can use now, are.
    #[test]
    fn a_late_failure_leaves_a_newer_dialogs_picked_files_alone() {
        let mut uploads = picked(&[(Slot::Import(1), 1)]);
        let sent = uploads.take();
        assert!(uploads.close().is_empty(), "the run holds its file");
        let opening = uploads.opening();
        assert!(uploads.pick(opening, Slot::Import(1), id(2)).is_empty());
        assert_eq!(uploads.give_back(&sent), [id(1)]);
        assert_eq!(
            uploads.close(),
            [id(2)],
            "still picked, so still the dialog's"
        );
    }

    /// The case review 2026-10-28 traced: the newer dialog's files are taken by its own
    /// request; a late failure of the older request must not hand them back to be dropped.
    #[test]
    fn a_late_failure_leaves_a_newer_dialogs_taken_files_to_their_job() {
        let mut uploads = picked(&[(Slot::Import(1), 1)]);
        let old = uploads.take();
        assert!(uploads.close().is_empty());
        let opening = uploads.opening();
        assert!(uploads.pick(opening, Slot::Import(1), id(2)).is_empty());
        let new = uploads.take();
        assert_eq!(uploads.give_back(&old), [id(1)]);
        assert!(
            uploads.close().is_empty(),
            "the newer job still holds its file"
        );
        assert!(uploads.give_back(&new).contains(&id(2)));
    }

    /// #858: a request's answer names the dialog that sent it, so the page can tell an answer
    /// for a dialog closed and opened again from one for the dialog open now.
    #[test]
    fn a_requests_answer_is_current_only_while_the_dialog_that_sent_it_is_open() {
        let mut uploads = picked(&[(Slot::Usfm(0), 1)]);
        let sent = uploads.take();
        assert!(uploads.is_current(sent.opening()));
        assert!(uploads.close().is_empty());
        assert!(!uploads.is_current(sent.opening()), "the dialog closed");
        let opening = uploads.opening();
        assert!(uploads.pick(opening, Slot::Usfm(0), id(2)).is_empty());
        assert!(
            !uploads.is_current(sent.opening()),
            "a newer opening is not the one that sent it"
        );
        let request = PickedUploads::default().take();
        assert!(
            PickedUploads::default().is_current(request.opening()),
            "a request with no files still names its dialog"
        );
    }

    #[test]
    fn a_late_success_leaves_a_newer_dialogs_files_alone() {
        let mut uploads = picked(&[(Slot::Usfm(0), 1)]);
        let old = uploads.take();
        assert!(uploads.close().is_empty());
        let opening = uploads.opening();
        assert!(uploads.pick(opening, Slot::Usfm(0), id(2)).is_empty());
        let _ = uploads.take();
        uploads.finish(&old);
        assert!(
            uploads.close().is_empty(),
            "the newer file is still taken, not forgotten"
        );
    }

    /// In one dialog, a request's answer acts on its own files only: a file picked after the
    /// request was sent stays picked when the request ends.
    #[test]
    fn an_answer_acts_on_the_files_its_request_was_sent_with() {
        let mut uploads = picked(&[(Slot::Usfm(0), 1)]);
        let sent = uploads.take();
        let opening = uploads.opening();
        assert!(uploads.pick(opening, Slot::Usfm(1), id(2)).is_empty());
        uploads.finish(&sent);
        assert_eq!(uploads.close(), [id(2)]);

        let mut uploads = picked(&[(Slot::Usfm(0), 1)]);
        let first = uploads.take();
        assert!(uploads.give_back(&first).is_empty());
        let second = uploads.take();
        uploads.finish(&second);
        assert!(uploads.give_back(&first).is_empty());
        assert!(
            uploads.close().is_empty(),
            "a late first answer gives back nothing"
        );
    }

    #[test]
    fn only_the_open_dialog_is_current() {
        let mut uploads = PickedUploads::default();
        let first = uploads.opening();
        assert!(uploads.is_current(first));
        let _ = uploads.close();
        assert!(!uploads.is_current(first));
        assert!(uploads.is_current(uploads.opening()));
    }
}
