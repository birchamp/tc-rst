//! Project settings (M7) and Get source texts (M8 steps 1 and 2), unit B9a: tC4's `st` and
//! `src` forms (`tC4:src/views/modals/ProjectSettings.jsx`, `SourceTexts.jsx`). Pure; the reads
//! and the download are the server's.

use tc_api::home::{HomeCard, ProjectKind};
use tc_api::ids::{JobId, ProjectRef, RequestId};
use tc_api::resources::{GatewayPackage, PackageRow, SourcesView};
use tc_api::settings::{GatewayCard, Save, SettingsView, ShownValues};
use tc_api::shown::{Direction, Known};
use tc_core::BookId;

use crate::app::forms::SCRIPT_FONTS;

/// The Checking language card while its read is out, or as it came.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GatewayState {
    /// `settings.gatewayLoading`.
    Loading,
    Read(GatewayCard),
    /// The read failed with the rest of the form: the card leaves its loading text.
    Failed,
}

/// tC4's `st`: what the dialog shows of one project, and the direction and font being chosen.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SettingsForm {
    pub project: ProjectRef,
    pub name: String,
    pub lang_name: String,
    pub code: String,
    pub dir: Direction,
    pub font: String,
    pub book_count: usize,
    pub kind: ProjectKind,
    /// The project's values have come: until then Save is disabled, so a save can never write
    /// the defaults over the project's real direction and font (tC4's D30 sweep).
    pub loaded: bool,
    pub busy: bool,
    pub error: Option<String>,
    pub gateway: GatewayState,
    /// The direction and font the project held when they were read: what Save says the dialog
    /// showed (AD14).
    pub shown: Option<ShownValues>,
    /// The id of the save in flight or lost (A-27); an edit or an answer ends it.
    pub request: Option<RequestId>,
}

impl SettingsForm {
    /// The dialog as a card's Settings opens it (tC4 `performOpenSettings`): the card's name,
    /// code, direction and books at once, the rest when the read lands.
    pub fn of(card: &HomeCard) -> Self {
        let book_count = match &card.books {
            Known::Known(books) => books.len(),
            Known::Unknown => 0,
        };
        Self {
            project: card.project.clone(),
            name: card.name.clone(),
            lang_name: String::new(),
            code: match &card.tag {
                Known::Known(tag) => tag.clone(),
                Known::Unknown => String::new(),
            },
            dir: card.dir,
            font: SCRIPT_FONTS[0].to_owned(),
            book_count,
            kind: card.kind,
            loaded: false,
            busy: false,
            error: None,
            gateway: GatewayState::Loading,
            shown: None,
            request: None,
        }
    }

    /// The read of `project` landed. A read for a project whose Settings are no longer the
    /// ones open changes nothing (tC4 #641: a reply never lands on another project's form).
    pub fn read(&mut self, project: &ProjectRef, view: SettingsView) {
        if *project != self.project {
            return;
        }
        self.shown = Some(ShownValues {
            direction: view.direction,
            font: view.font.clone(),
        });
        self.dir = view.direction;
        self.font = view.font.unwrap_or_else(|| SCRIPT_FONTS[0].to_owned());
        self.lang_name = view.language_name;
        self.gateway = GatewayState::Read(view.gateway);
        self.loaded = true;
        self.error = None;
    }

    /// The read of `project` failed: the reason shows and Save stays disabled.
    pub fn failed(&mut self, project: &ProjectRef, reason: String) {
        if *project != self.project {
            return;
        }
        self.loaded = false;
        self.error = Some(reason);
        self.gateway = GatewayState::Failed;
    }

    pub fn save_disabled(&self) -> bool {
        self.busy || !self.loaded
    }

    /// The press of Save changes (tC4 `saveSettings`): nothing while a save runs or before the
    /// project's values came; else the request, with the id of the attempt.
    pub fn press_save(&mut self, fresh: impl FnOnce() -> RequestId) -> Option<Save> {
        if self.save_disabled() {
            return None;
        }
        let shown = self.shown.clone()?;
        self.busy = true;
        self.error = None;
        Some(Save {
            request: *self.request.get_or_insert_with(fresh),
            project: self.project.clone(),
            shown,
            direction: self.dir,
            font: self.font.clone(),
        })
    }

    /// The server answered a save of `project`: a refusal's words stay in the dialog. A reply
    /// for another project's dialog changes nothing (tC4 #641).
    pub fn answered(&mut self, project: &ProjectRef, error: Option<String>) {
        if *project != self.project {
            return;
        }
        self.busy = false;
        self.request = None;
        self.error = error;
    }

    /// No answer came: the save may have been carried out, and the next press sends its id.
    pub fn lost(&mut self, project: &ProjectRef, error: String) {
        if *project == self.project {
            self.busy = false;
            self.error = Some(error);
        }
    }

    /// The direction or the font changed: a save of the new values is another request.
    pub fn edited(&mut self) {
        self.request = None;
    }

    /// The subtitle's book count is one book or many (`settings.subtitleOne`).
    pub fn one_book(&self) -> bool {
        self.book_count == 1
    }
}

/// Where a download stands (tC4's `src.dl`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Dl {
    /// `sources.progress` names the repository being fetched; before the first report,
    /// `sources.downloading`.
    Run {
        job: JobId,
        repo: Option<String>,
    },
    Done,
}

/// tC4's `src`: kept across the dialog's openings, as tC4 keeps it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourcesForm {
    /// Step 1's packages for the open project.
    pub packages: Option<SourcesView>,
    /// Step 2's package, once one is chosen.
    pub gateway: Option<GatewayPackage>,
    pub book: BookId,
    pub rows: Vec<PackageRow>,
    /// The row keys the person turned off.
    pub exclude: std::collections::BTreeSet<String>,
    pub loading: bool,
    pub error: Option<String>,
    /// The person answered Not now to the internet question: the catalog was not read.
    pub needs_internet: bool,
    pub dl: Option<Dl>,
}

impl Default for SourcesForm {
    fn default() -> Self {
        Self {
            packages: None,
            gateway: None,
            book: BookId::Titus,
            rows: Vec::new(),
            exclude: std::collections::BTreeSet::new(),
            loading: false,
            error: None,
            needs_internet: false,
            dl: None,
        }
    }
}

impl SourcesForm {
    /// Whether a row is in the download: a fixed row always is.
    pub fn on(&self, row: &PackageRow) -> bool {
        row.role.fixed() || !self.exclude.contains(&row.key())
    }

    /// A row's card was clicked (tC4 `toggleSourceRow`). A fixed row is not a choice.
    pub fn toggle(&mut self, row: &PackageRow) {
        if row.role.fixed() {
            return;
        }
        let key = row.key();
        if !self.exclude.remove(&key) {
            self.exclude.insert(key);
        }
        self.dl = None;
    }

    /// The repositories a download fetches, in the rows' order.
    pub fn chosen(&self) -> Vec<String> {
        self.rows
            .iter()
            .filter(|row| self.on(row))
            .map(|row| row.repo.clone())
            .collect()
    }

    /// A package was chosen (tC4 `pickGateway`).
    pub fn pick(&mut self, gateway: GatewayPackage) {
        self.gateway = Some(gateway);
        self.dl = None;
        self.reading();
    }

    /// "Change language" (tC4 `changeGateway`): back to step 1.
    pub fn change_gateway(&mut self) {
        self.gateway = None;
        self.rows.clear();
        self.error = None;
        self.dl = None;
    }

    /// Another book (tC4 `setSourceBook`).
    pub fn set_book(&mut self, book: BookId) {
        self.book = book;
        self.dl = None;
        self.reading();
    }

    fn reading(&mut self) {
        self.loading = true;
        self.error = None;
        self.rows.clear();
        self.needs_internet = false;
    }

    /// The rows of `gateway` and `book` came. Rows for a package or book the person has left
    /// meanwhile are not shown.
    pub fn rows_read(&mut self, gateway: &GatewayPackage, book: BookId, rows: Vec<PackageRow>) {
        if self.gateway.as_ref() != Some(gateway) || self.book != book {
            return;
        }
        self.loading = false;
        self.rows = rows;
    }

    /// The catalog could not be read: `sources.catalogError`'s reason.
    pub fn rows_failed(&mut self, gateway: &GatewayPackage, book: BookId, reason: String) {
        if self.gateway.as_ref() != Some(gateway) || self.book != book {
            return;
        }
        self.loading = false;
        self.error = Some(reason);
    }

    /// Not now to the internet question: nothing was read (tC4's `needsInternet`).
    pub fn not_read(&mut self) {
        self.loading = false;
        self.rows.clear();
        self.needs_internet = true;
    }

    /// Whether the footer offers Download: a package with rows and no download under way or
    /// finished.
    pub fn can_download(&self) -> bool {
        self.dl.is_none() && !self.rows.is_empty()
    }

    /// Whether `gateway` is the project's checking language (tC4 `isCurrentGateway`: the
    /// owner without letter case).
    pub fn is_current(&self, gateway: &GatewayPackage) -> bool {
        self.packages
            .as_ref()
            .and_then(|p| p.current.as_ref())
            .is_some_and(|current| {
                current.language == gateway.gateway.language
                    && current.owner.eq_ignore_ascii_case(&gateway.gateway.owner)
            })
    }

    /// Whether this computer holds `gateway`'s whole suite (tC4 `isCheckableGateway`).
    pub fn is_checkable(&self, gateway: &GatewayPackage) -> bool {
        self.packages.as_ref().is_some_and(|p| match &p.checkable {
            tc_api::resources::FromInstalls::Listed(keys) => keys.contains(&gateway.gateway),
            tc_api::resources::FromInstalls::Failed { .. } => false,
        })
    }
}
