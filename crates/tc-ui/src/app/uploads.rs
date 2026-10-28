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

/// Which opening of a dialog an upload was started in: an upload that finishes after its
/// dialog closed is dropped as it lands.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Opening(u32);

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

    /// An upload stored for `slot`, started in `opening`. Returns what to drop: the picked
    /// file it replaces, or this one when its dialog closed while it was sent.
    pub fn pick(&mut self, opening: Opening, slot: Slot, id: UploadId) -> Vec<UploadId> {
        if opening != self.opening() {
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

    /// The dialog's request that reads its files was sent: they are the server's now.
    pub fn take(&mut self) {
        for held in self.held.values_mut() {
            if let Held::Picked(id) = held {
                *held = Held::Taken(id.clone());
            }
        }
    }

    /// The request failed and the dialog stays with the same files, which the person may try
    /// again with or leave.
    pub fn give_back(&mut self) {
        for held in self.held.values_mut() {
            if let Held::Taken(id) = held {
                *held = Held::Picked(id.clone());
            }
        }
    }

    /// The job ended and deleted its files: nothing is left to drop.
    pub fn finish(&mut self) {
        self.held.retain(|_, held| matches!(held, Held::Picked(_)));
    }

    /// The dialog closed, whichever way: every picked file is dropped, a taken one is left to
    /// its job, and an upload still on its way is dropped when it lands.
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
        uploads.take();
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
        uploads.take();
        assert!(uploads.remove(Slot::Import(1)).is_empty());
        assert!(uploads.close().is_empty());
    }

    #[test]
    fn a_failed_request_gives_its_files_back_and_closing_then_drops_them() {
        let mut uploads = picked(&[(Slot::Import(1), 1), (Slot::Import(2), 2)]);
        uploads.take();
        uploads.give_back();
        assert_eq!(uploads.close(), [id(1), id(2)]);
    }

    #[test]
    fn a_finished_job_leaves_nothing_to_drop() {
        let mut uploads = picked(&[(Slot::Usfm(1), 1)]);
        uploads.take();
        uploads.finish();
        uploads.give_back();
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
}
