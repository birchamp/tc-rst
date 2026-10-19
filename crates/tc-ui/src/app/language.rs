//! The App language dialog's rules (M16, tC4 #522, `tC4:src/views/AccountMenu.jsx:340-399`).
//! A selection previews the language across the whole app, the dialog included; Apply saves it
//! and the dialog closes once the save succeeded; Cancel, Escape, the ✕ and the scrim show the
//! applied language again and save nothing. A failed save keeps the preview and the dialog with
//! an error. While the save runs, nothing can be selected, applied or cancelled.
//!
//! Each method returns the language to show, if it changes; the caller shows it.

use tc_api::ids::LocaleId;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LanguageDialog {
    applied: LocaleId,
    preview: LocaleId,
    saving: bool,
    error: bool,
}

impl LanguageDialog {
    /// Opening is the person's selection of the applied language: tC4 shows it at once.
    pub fn open(applied: LocaleId) -> (Self, LocaleId) {
        (
            Self {
                applied,
                preview: applied,
                saving: false,
                error: false,
            },
            applied,
        )
    }

    /// A choice in the list: preview it. Nothing while the save runs, or for the language the
    /// preview already shows.
    pub fn select(&mut self, id: LocaleId) -> Option<LocaleId> {
        if self.saving || id == self.preview {
            return None;
        }
        self.preview = id;
        self.error = false;
        Some(id)
    }

    /// Cancel, Escape, the ✕ or the scrim: show the applied language; the caller closes the
    /// dialog. `None` while the save runs, when nothing closes.
    pub fn cancel(&self) -> Option<LocaleId> {
        (!self.saving).then_some(self.applied)
    }

    /// Apply: the language to save, which the caller sends with a new request id. Nothing while
    /// the save runs or at the applied language.
    pub fn start_apply(&mut self) -> Option<LocaleId> {
        if self.saving || self.preview == self.applied {
            return None;
        }
        self.saving = true;
        self.error = false;
        Some(self.preview)
    }

    /// The save failed: the preview stays, with the error; Apply and Cancel work again.
    pub fn apply_failed(&mut self) {
        self.saving = false;
        self.error = true;
    }

    /// The select, Apply, Cancel, the ✕, Escape and the scrim are off.
    pub fn locked(&self) -> bool {
        self.saving
    }

    pub fn apply_enabled(&self) -> bool {
        !self.saving && self.preview != self.applied
    }

    pub fn error(&self) -> bool {
        self.error
    }

    pub fn preview(&self) -> LocaleId {
        self.preview
    }

    pub fn applied(&self) -> LocaleId {
        self.applied
    }
}
