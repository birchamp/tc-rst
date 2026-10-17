//! Opening what is installed for a book or for Open Bible Stories: the helps as one
//! tc-helps [`Helps`], and the source texts. Nothing here touches the network.
//!
//! Every part the screens show comes back with where it came from or why it is missing
//! ([`Missing`]): absence is always stated, never a blank tab. Each part comes from one
//! pinned resource:
//!
//! | Part | Door43 | Aquifer |
//! |---|---|---|
//! | notes | `translationNotes`, resolved per book over the ladder | the `notes` asset |
//! | words links | `translationWordsLinks`, resolved per book | the `words` asset's passage lists |
//! | words articles | `translationWords` of the rung that answered the links (D97) | the `words` asset |
//! | academy articles | `translationAcademy` of the rung that answered the notes | the `academy` asset |
//! | questions | `translationQuestions`, resolved per book | the `questions` asset |
//!
//! An Aquifer note, question or words link goes with the book of its passage, whatever
//! file the release filed it in: the install indexes which files hold each book.
//!
//! The texts are the original-language text of the book's testament, every source pane,
//! and the simplified text resolved over the ladder; the USFM comes back as its bytes.
//! Open Bible Stories helps come from Door43 only: Aquifer mirrors none of them.

// A `Missing` is built at most once per part and carries the pin it names, so its size
// costs nothing; boxing it would only make every match harder to read.
#![allow(clippy::result_large_err)]

use std::collections::BTreeMap;

use tc_core::BookId;
use tc_helps::aquifer::{
    academy_from_aquifer, notes_from_aquifer, parse_aquifer_articles, questions_from_aquifer,
    words_from_aquifer,
};
use tc_helps::articles::{academy_article, parse_word_article, word_article_id};
use tc_helps::metadata::{AquiferPin as HelpsAquiferPin, DcsPin, HelpsSource};
use tc_helps::obs::{ObsStory, parse_obs_story};
use tc_helps::tsv::{TsvScope, parse_notes, parse_questions, parse_word_links};
use tc_helps::{HelpRef, Helps, HelpsPart};
use tc_journal::pins::{Collection, UnusablePin};

use crate::aquifer::{bible_book, canonical_book, is_article_file};
use crate::choice::{AquiferHelps, HelpsChoice};
use crate::layout;
use crate::pin::{AquiferPin, ResourcePin};
use crate::resolve::{CoverageSource, Resolution, Resolved, coverage_for, resolve_slot};
use crate::sets::{PaneId, ProjectPins, Rung, SetSlot};
use crate::store::{Installed, ResourceStore};

/// Why a part or a text is not there.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Missing {
    /// Nothing names it: no language set pins the slot, or no Aquifer asset is pinned for
    /// the tab.
    NotPinned,
    /// Pinned, but no pinned resource holds this book, by its recorded or installed
    /// coverage.
    NotCovered { pinned: Vec<ResourcePin> },
    /// The pinned Door43 commit is not on this computer: it can be fetched while the
    /// internet is on (D30.4), and until then this part is unavailable (D30.5).
    NotInstalled(ResourcePin),
    /// The pinned Aquifer asset is not on this computer.
    AquiferNotInstalled(AquiferPin),
    /// The installed resource does not hold this book.
    BookNotInResource { source: String },
    /// The words-links pin is the pre-release one-repository form (D97 ruling 6): it is
    /// never read, and the project must be created again.
    OldTwl(ResourcePin),
    /// Installed, but it cannot be read: damaged on disk, or not in a form tc-helps reads.
    Unreadable { source: String, problem: String },
    /// Aquifer mirrors no Open Bible Stories helps, so these come from Door43 only.
    NotInAquifer,
    /// The rung that answers holds a value that is not a usable pin, such as a pin with a
    /// tag and no commit (STORAGE §7.7): nothing can be read for it until the pins are
    /// repaired, and the next rung is not used in its place.
    UnusablePin { rung: Rung, pin: UnusablePin },
}

/// One helps part of a screen.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PartStatus {
    Loaded {
        source: HelpsSource,
        /// The rung that answered, for a Door43 part.
        rung: Option<Rung>,
        /// Set when English answered only because the primary pin, whose coverage is
        /// unknown, is not installed: the screen must say the part is a substitute (B20).
        substitute_for: Option<ResourcePin>,
        /// Items the reader refused alone, each named; the rest loaded.
        refused: Vec<String>,
    },
    Missing(Missing),
}

impl PartStatus {
    pub fn is_loaded(&self) -> bool {
        matches!(self, PartStatus::Loaded { .. })
    }

    pub fn missing(&self) -> Option<&Missing> {
        match self {
            PartStatus::Missing(m) => Some(m),
            PartStatus::Loaded { .. } => None,
        }
    }
}

/// Which text a [`SourceText`] is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TextRole {
    /// The UGNT or UHB of the book's testament.
    OriginalLanguage,
    /// A source pane of `extraScripture`, such as `ult`.
    Pane(PaneId),
    /// The simplified Bible of the language sets, resolved over the ladder.
    Simplified,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TextStatus {
    Loaded {
        pin: ResourcePin,
        rung: Option<Rung>,
        /// The book's USFM, byte for byte.
        usfm: String,
    },
    Missing(Missing),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceText {
    pub role: TextRole,
    pub status: TextStatus,
}

/// What is installed for one book.
#[derive(Debug, Clone)]
pub struct BookResources {
    pub book: BookId,
    pub helps: Helps,
    /// Every Bible helps part, each loaded or missing.
    pub parts: BTreeMap<HelpsPart, PartStatus>,
    /// The original-language text, then each pane in order, then the simplified text.
    pub texts: Vec<SourceText>,
}

impl BookResources {
    pub fn text(&self, role: &TextRole) -> Option<&TextStatus> {
        self.texts
            .iter()
            .find(|t| t.role == *role)
            .map(|t| &t.status)
    }
}

/// What is installed for Open Bible Stories.
#[derive(Debug, Clone)]
pub struct ObsResources {
    pub helps: Helps,
    /// Every Open Bible Stories helps part and the articles, each loaded or missing.
    pub parts: BTreeMap<HelpsPart, PartStatus>,
    /// The stories of the `obs` slot, each that parsed; refused files are named in
    /// `stories_status`.
    pub stories: Vec<ObsStory>,
    pub stories_status: PartStatus,
}

/// Opens everything installed for `book` under the project's pins and helps choice.
pub fn load_book(
    store: &ResourceStore,
    pins: &ProjectPins,
    choice: &HelpsChoice,
    book: BookId,
) -> BookResources {
    let mut out = BookResources {
        book,
        helps: Helps::new(),
        parts: BTreeMap::new(),
        texts: Vec::new(),
    };
    match choice {
        HelpsChoice::Dcs => load_dcs_book(store, pins, book, &mut out.helps, &mut out.parts),
        HelpsChoice::Aquifer(aquifer) => {
            load_aquifer_book(store, aquifer, book, &mut out.helps, &mut out.parts);
        }
    }
    out.texts = load_texts(store, pins, book);
    out
}

/// Opens everything installed for Open Bible Stories. The helps choice does not apply:
/// Aquifer mirrors no Open Bible Stories helps.
pub fn load_obs(store: &ResourceStore, pins: &ProjectPins) -> ObsResources {
    let mut helps = Helps::new();
    let mut parts = BTreeMap::new();
    let mut rung_of = BTreeMap::new();
    for (part, slot) in [
        (HelpsPart::ObsNotes, SetSlot::ObsNotes),
        (HelpsPart::ObsWordLinks, SetSlot::ObsWordsLinks),
        (HelpsPart::ObsQuestions, SetSlot::ObsQuestions),
    ] {
        let status = match resolve_slot(pins, slot, Collection::Stories, store) {
            None => PartStatus::Missing(Missing::NotPinned),
            Some(Resolved::Unusable { rung, pin }) => {
                PartStatus::Missing(Missing::UnusablePin { rung, pin })
            }
            Some(Resolved::Pin(resolution)) => {
                rung_of.insert(part, resolution.rung);
                load_table(store, &resolution, None, part, TsvScope::Obs, &mut helps)
            }
        };
        parts.insert(part, status);
    }
    let words_rung = rung_of.get(&HelpsPart::ObsWordLinks).copied();
    let academy_rung = rung_of.get(&HelpsPart::ObsNotes).copied();
    parts.insert(
        HelpsPart::WordArticles,
        load_articles(
            store,
            pins,
            SetSlot::TranslationWords,
            words_rung,
            &mut helps,
        ),
    );
    parts.insert(
        HelpsPart::AcademyArticles,
        load_articles(
            store,
            pins,
            SetSlot::TranslationAcademy,
            academy_rung,
            &mut helps,
        ),
    );
    let (stories, stories_status) =
        match resolve_slot(pins, SetSlot::Obs, Collection::Stories, store) {
            None => (Vec::new(), PartStatus::Missing(Missing::NotPinned)),
            Some(Resolved::Unusable { rung, pin }) => (
                Vec::new(),
                PartStatus::Missing(Missing::UnusablePin { rung, pin }),
            ),
            Some(Resolved::Pin(resolution)) => load_stories(store, &resolution),
        };
    ObsResources {
        helps,
        parts,
        stories,
        stories_status,
    }
}

fn dcs_source(pin: &ResourcePin) -> HelpsSource {
    HelpsSource::Dcs(DcsPin {
        owner: pin.repo.owner().to_string(),
        repo: pin.repo.name().to_string(),
        tag: pin.version.as_ref().map(|v| v.as_str().to_owned()),
        commit: pin.sha,
    })
}

fn aquifer_source(pin: &AquiferPin) -> HelpsSource {
    HelpsSource::Aquifer(HelpsAquiferPin {
        resource_code: pin.repo.name.to_string(),
        language: pin.language.to_string(),
        tag: pin.tag.as_str().to_owned(),
        commit: pin.commit,
        asset_sha256: tc_helps::Sha256Digest::parse(&pin.sha256.to_hex()).ok(),
    })
}

/// Why no rung answered a per-book slot.
fn unresolved(store: &ResourceStore, pins: &ProjectPins, slot: SetSlot) -> Missing {
    let pinned: Vec<ResourcePin> = Rung::LADDER
        .into_iter()
        .filter_map(|rung| pins.set(rung)?.pin(slot).cloned())
        .collect();
    if pinned.is_empty() {
        return Missing::NotPinned;
    }
    // A pin whose coverage is unknown and that is not installed may hold the book.
    match pinned
        .iter()
        .find(|p| coverage_for(p, store).1 == CoverageSource::Unknown && !store.has_dcs(p))
    {
        Some(pin) => Missing::NotInstalled(pin.clone()),
        None => Missing::NotCovered { pinned },
    }
}

/// The primary pin English stands in for, when the primary's coverage is unknown and it
/// is not installed.
fn substitute_for(
    store: &ResourceStore,
    pins: &ProjectPins,
    slot: SetSlot,
    resolution: &Resolution,
) -> Option<ResourcePin> {
    if !resolution.used_fallback() {
        return None;
    }
    let primary = pins.set(Rung::Primary)?.pin(slot)?;
    (coverage_for(primary, store).1 == CoverageSource::Unknown && !store.has_dcs(primary))
        .then(|| primary.clone())
}

fn open_dcs(store: &ResourceStore, pin: &ResourcePin) -> Result<Installed, Missing> {
    match store.dcs(pin) {
        Ok(Some(installed)) => Ok(installed),
        Ok(None) => Err(Missing::NotInstalled(pin.clone())),
        Err(e) => Err(Missing::Unreadable {
            source: pin.to_string(),
            problem: e.to_string(),
        }),
    }
}

fn read_text(installed: &Installed, path: &str, source: &str) -> Result<String, Missing> {
    installed.read_text(path).map_err(|e| Missing::Unreadable {
        source: source.to_owned(),
        problem: e.to_string(),
    })
}

fn load_dcs_book(
    store: &ResourceStore,
    pins: &ProjectPins,
    book: BookId,
    helps: &mut Helps,
    parts: &mut BTreeMap<HelpsPart, PartStatus>,
) {
    let mut rung_of = BTreeMap::new();
    let tables = [
        (HelpsPart::Notes, SetSlot::TranslationNotes),
        (HelpsPart::WordLinks, SetSlot::TranslationWordsLinks),
        (HelpsPart::Questions, SetSlot::TranslationQuestions),
    ];
    for (part, slot) in tables {
        if part == HelpsPart::WordLinks
            && let Some(old) = pins.old_twl_pin()
        {
            parts.insert(part, PartStatus::Missing(Missing::OldTwl(old.clone())));
            continue;
        }
        let status = match resolve_slot(pins, slot, Collection::Book(book), store) {
            None => PartStatus::Missing(unresolved(store, pins, slot)),
            Some(Resolved::Unusable { rung, pin }) => {
                PartStatus::Missing(Missing::UnusablePin { rung, pin })
            }
            Some(Resolved::Pin(resolution)) => {
                rung_of.insert(part, resolution.rung);
                let substitute = substitute_for(store, pins, slot, &resolution);
                load_table(
                    store,
                    &resolution,
                    substitute,
                    part,
                    TsvScope::Book(book),
                    helps,
                )
            }
        };
        parts.insert(part, status);
    }
    let words_rung = rung_of.get(&HelpsPart::WordLinks).copied();
    let academy_rung = rung_of.get(&HelpsPart::Notes).copied();
    parts.insert(
        HelpsPart::WordArticles,
        load_articles(store, pins, SetSlot::TranslationWords, words_rung, helps),
    );
    parts.insert(
        HelpsPart::AcademyArticles,
        load_articles(
            store,
            pins,
            SetSlot::TranslationAcademy,
            academy_rung,
            helps,
        ),
    );
}

/// Reads one table (notes, words links or questions) of a book or of the stories.
fn load_table(
    store: &ResourceStore,
    resolution: &Resolution,
    substitute_for: Option<ResourcePin>,
    part: HelpsPart,
    scope: TsvScope,
    helps: &mut Helps,
) -> PartStatus {
    let pin = &resolution.pin;
    let name = pin.to_string();
    let result = (|| -> Result<(), Missing> {
        let installed = open_dcs(store, pin)?;
        let path = match scope {
            TsvScope::Book(book) => layout::book_file(installed.files(), book, "tsv"),
            TsvScope::Obs => layout::obs_table(installed.files()),
        }
        .map(str::to_owned)
        .ok_or_else(|| Missing::BookNotInResource {
            source: name.clone(),
        })?;
        let text = read_text(&installed, &path, &name)?;
        let unreadable = |e: tc_helps::Error| Missing::Unreadable {
            source: name.clone(),
            problem: e.to_string(),
        };
        let source = dcs_source(pin);
        let added = match part {
            HelpsPart::Notes | HelpsPart::ObsNotes => helps.add_notes(
                &source,
                parse_notes(&text, scope).map_err(unreadable)?.notes,
            ),
            HelpsPart::WordLinks | HelpsPart::ObsWordLinks => {
                helps.add_word_links(&source, parse_word_links(&text, scope).map_err(unreadable)?)
            }
            _ => helps.add_questions(&source, parse_questions(&text, scope).map_err(unreadable)?),
        };
        added.map_err(unreadable)
    })();
    match result {
        Ok(()) => PartStatus::Loaded {
            source: dcs_source(pin),
            rung: Some(resolution.rung),
            substitute_for,
            refused: Vec::new(),
        },
        Err(missing) => PartStatus::Missing(missing),
    }
}

/// Reads the words or academy articles of `rung`'s set, or of the first rung that pins
/// the slot when no rung answered the part that links to them.
fn load_articles(
    store: &ResourceStore,
    pins: &ProjectPins,
    slot: SetSlot,
    rung: Option<Rung>,
    helps: &mut Helps,
) -> PartStatus {
    let pinned = match rung {
        Some(rung) => pins
            .set(rung)
            .and_then(|set| set.pin(slot))
            .map(|pin| (rung, pin))
            .or_else(|| first_pinned(pins, slot)),
        None => first_pinned(pins, slot),
    };
    let Some((rung, pin)) = pinned else {
        return PartStatus::Missing(Missing::NotPinned);
    };
    let name = pin.to_string();
    let result = (|| -> Result<Vec<String>, Missing> {
        let installed = open_dcs(store, pin)?;
        let source = dcs_source(pin);
        let mut refused = Vec::new();
        let added = if slot == SetSlot::TranslationWords {
            let mut articles = Vec::new();
            let paths: Vec<String> = installed
                .files()
                .filter(|p| layout::is_word_article(p))
                .map(str::to_owned)
                .collect();
            for path in paths {
                match word_article_id(&path) {
                    Ok(id) => articles.push(parse_word_article(
                        id,
                        &read_text(&installed, &path, &name)?,
                    )),
                    Err(e) => refused.push(format!("{path}: {e}")),
                }
            }
            if articles.is_empty() {
                return Err(Missing::Unreadable {
                    source: name.clone(),
                    problem: "it holds no translation words articles".to_owned(),
                });
            }
            helps.add_word_articles(&source, articles)
        } else {
            let mut articles = Vec::new();
            for folder in layout::academy_folders(installed.files()) {
                let file = |leaf: &str| format!("{}{leaf}", folder.folder);
                let title = read_text(&installed, &file("title.md"), &name)?;
                let subtitle = if installed.has_file(&file("sub-title.md")) {
                    Some(read_text(&installed, &file("sub-title.md"), &name)?)
                } else {
                    None
                };
                let body = read_text(&installed, &file("01.md"), &name)?;
                articles.push(academy_article(
                    folder.manual,
                    folder.slug,
                    &title,
                    subtitle.as_deref(),
                    &body,
                ));
            }
            if articles.is_empty() {
                return Err(Missing::Unreadable {
                    source: name.clone(),
                    problem: "it holds no Translation Academy articles".to_owned(),
                });
            }
            helps.add_academy_articles(&source, articles)
        };
        added.map_err(|e| Missing::Unreadable {
            source: name.clone(),
            problem: e.to_string(),
        })?;
        Ok(refused)
    })();
    match result {
        Ok(refused) => PartStatus::Loaded {
            source: dcs_source(pin),
            rung: Some(rung),
            substitute_for: None,
            refused,
        },
        Err(missing) => PartStatus::Missing(missing),
    }
}

fn first_pinned(pins: &ProjectPins, slot: SetSlot) -> Option<(Rung, &ResourcePin)> {
    Rung::LADDER
        .into_iter()
        .find_map(|rung| Some((rung, pins.set(rung)?.pin(slot)?)))
}

fn load_stories(store: &ResourceStore, resolution: &Resolution) -> (Vec<ObsStory>, PartStatus) {
    let pin = &resolution.pin;
    let name = pin.to_string();
    let installed = match open_dcs(store, pin) {
        Ok(installed) => installed,
        Err(missing) => return (Vec::new(), PartStatus::Missing(missing)),
    };
    let paths: Vec<String> = installed
        .files()
        .filter(|p| {
            let rel = p.strip_prefix("ingredients/").unwrap_or(p);
            rel.strip_prefix("content/").is_some_and(|f| {
                f.len() == 5 && f.ends_with(".md") && f[..2].bytes().all(|b| b.is_ascii_digit())
            })
        })
        .map(str::to_owned)
        .collect();
    let mut stories = Vec::new();
    let mut refused = Vec::new();
    for path in paths {
        match read_text(&installed, &path, &name) {
            Ok(text) => match parse_obs_story(&text) {
                Ok(story) => stories.push(story),
                Err(e) => refused.push(format!("{path}: {e}")),
            },
            Err(missing) => return (Vec::new(), PartStatus::Missing(missing)),
        }
    }
    if stories.is_empty() {
        return (
            Vec::new(),
            PartStatus::Missing(Missing::Unreadable {
                source: name,
                problem: "it holds no story that can be read".to_owned(),
            }),
        );
    }
    let status = PartStatus::Loaded {
        source: dcs_source(pin),
        rung: Some(resolution.rung),
        substitute_for: None,
        refused,
    };
    (stories, status)
}

fn load_aquifer_book(
    store: &ResourceStore,
    aquifer: &AquiferHelps,
    book: BookId,
    helps: &mut Helps,
    parts: &mut BTreeMap<HelpsPart, PartStatus>,
) {
    let in_book = |reference: &HelpRef| bible_book(reference) == Some(book);
    let notes = aquifer_part(
        store,
        aquifer.notes.as_ref(),
        Some(book),
        |source, articles, helps| {
            let loaded = notes_from_aquifer(articles);
            let notes: Vec<_> = loaded
                .items
                .into_iter()
                .filter(|n| in_book(&n.reference))
                .collect();
            let count = notes.len();
            helps.add_notes(source, notes)?;
            Ok((count, loaded.refused))
        },
        helps,
    );
    parts.insert(HelpsPart::Notes, notes);
    let questions = aquifer_part(
        store,
        aquifer.questions.as_ref(),
        Some(book),
        |source, articles, helps| {
            let loaded = questions_from_aquifer(articles);
            let questions: Vec<_> = loaded
                .items
                .into_iter()
                .filter(|q| in_book(&q.reference))
                .collect();
            let count = questions.len();
            helps.add_questions(source, questions)?;
            Ok((count, loaded.refused))
        },
        helps,
    );
    parts.insert(HelpsPart::Questions, questions);
    let words = aquifer_part(
        store,
        aquifer.words.as_ref(),
        None,
        |source, articles, helps| {
            let words = words_from_aquifer(articles);
            let count = words.articles.len();
            helps.add_word_articles(source, words.articles)?;
            helps.add_word_links(
                source,
                words.links.into_iter().filter(|l| in_book(&l.reference)),
            )?;
            Ok((count, Vec::new()))
        },
        helps,
    );
    // The links and the articles come from the one words asset.
    parts.insert(HelpsPart::WordLinks, words.clone());
    parts.insert(HelpsPart::WordArticles, words);
    let academy = aquifer_part(
        store,
        aquifer.academy.as_ref(),
        None,
        |source, articles, helps| {
            let articles = academy_from_aquifer(articles);
            let count = articles.len();
            helps.add_academy_articles(source, articles)?;
            Ok((count, Vec::new()))
        },
        helps,
    );
    parts.insert(HelpsPart::AcademyArticles, academy);
}

/// Reads an Aquifer asset's article files and hands their articles to `add`, which keeps
/// what belongs on the screen and returns how many items it kept and what the converter
/// refused. For a book (`book` given) the files are those the install's index lists for
/// it, since a release files some notes in another book's file; every file otherwise.
/// A refused article is named with the book whose file holds it.
fn aquifer_part(
    store: &ResourceStore,
    pin: Option<&AquiferPin>,
    book: Option<BookId>,
    add: impl FnOnce(
        &HelpsSource,
        &[tc_helps::aquifer::AquiferArticle],
        &mut Helps,
    ) -> Result<(usize, Vec<tc_helps::Error>), tc_helps::Error>,
    helps: &mut Helps,
) -> PartStatus {
    let Some(pin) = pin else {
        return PartStatus::Missing(Missing::NotPinned);
    };
    let name = pin.to_string();
    let installed = match store.aquifer(pin) {
        Ok(Some(installed)) => installed,
        Ok(None) => return PartStatus::Missing(Missing::AquiferNotInstalled(pin.clone())),
        Err(e) => {
            return PartStatus::Missing(Missing::Unreadable {
                source: name,
                problem: e.to_string(),
            });
        }
    };
    let files: Vec<String> = match book.and_then(|b| installed.aquifer_book_files(b)) {
        Some(listed) => listed.to_vec(),
        None => installed
            .files()
            .filter(|p| is_article_file(p))
            .map(str::to_owned)
            .collect(),
    };
    let not_in_resource = || {
        PartStatus::Missing(Missing::BookNotInResource {
            source: name.clone(),
        })
    };
    if files.is_empty() {
        return not_in_resource();
    }
    // A file's refusals are this book's to name when the file is the book's own, or is
    // not numbered by book at all.
    let names_refusals =
        |path: &str| book.is_none() || canonical_book(path).is_none_or(|b| Some(b) == book);
    let result = (|| -> Result<(usize, Vec<String>), Missing> {
        let unreadable = |problem: String| Missing::Unreadable {
            source: name.clone(),
            problem,
        };
        let mut articles = Vec::new();
        let mut file_of: BTreeMap<String, &str> = BTreeMap::new();
        let mut refused = Vec::new();
        for path in &files {
            let text = read_text(&installed, path, &name)?;
            let loaded =
                parse_aquifer_articles(&text).map_err(|e| unreadable(format!("{path}: {e}")))?;
            for article in &loaded.items {
                file_of.insert(article.content_id.as_str().to_owned(), path);
            }
            articles.extend(loaded.items);
            if names_refusals(path) {
                refused.extend(loaded.refused.iter().map(|e| format!("{path}: {e}")));
            }
        }
        let (kept, more) =
            add(&aquifer_source(pin), &articles, helps).map_err(|e| unreadable(e.to_string()))?;
        for error in more {
            let path = match &error {
                tc_helps::Error::AquiferArticle { content_id, .. } => {
                    file_of.get(content_id).copied()
                }
                _ => None,
            };
            match path {
                Some(path) if names_refusals(path) => refused.push(format!("{path}: {error}")),
                Some(_) => {}
                None => refused.push(error.to_string()),
            }
        }
        Ok((kept, refused))
    })();
    match result {
        Ok((0, refused)) if refused.is_empty() => not_in_resource(),
        Ok((_, refused)) => PartStatus::Loaded {
            source: aquifer_source(pin),
            rung: None,
            substitute_for: None,
            refused,
        },
        Err(missing) => PartStatus::Missing(missing),
    }
}

fn load_texts(store: &ResourceStore, pins: &ProjectPins, book: BookId) -> Vec<SourceText> {
    let mut texts = Vec::new();
    let original = match pins.original_language.for_book(book) {
        None => TextStatus::Missing(Missing::NotPinned),
        Some(pin) => load_usfm(store, pin, None, book),
    };
    texts.push(SourceText {
        role: TextRole::OriginalLanguage,
        status: original,
    });
    for pane in &pins.extra_scripture {
        texts.push(SourceText {
            role: TextRole::Pane(pane.id.clone()),
            status: load_usfm(store, &pane.pin, None, book),
        });
    }
    let simplified =
        match resolve_slot(pins, SetSlot::SimplifiedText, Collection::Book(book), store) {
            Some(Resolved::Pin(resolution)) => {
                load_usfm(store, &resolution.pin, Some(resolution.rung), book)
            }
            Some(Resolved::Unusable { rung, pin }) => {
                TextStatus::Missing(Missing::UnusablePin { rung, pin })
            }
            None => TextStatus::Missing(unresolved(store, pins, SetSlot::SimplifiedText)),
        };
    texts.push(SourceText {
        role: TextRole::Simplified,
        status: simplified,
    });
    texts
}

fn load_usfm(
    store: &ResourceStore,
    pin: &ResourcePin,
    rung: Option<Rung>,
    book: BookId,
) -> TextStatus {
    let name = pin.to_string();
    let result = (|| -> Result<String, Missing> {
        let installed = open_dcs(store, pin)?;
        let path = layout::book_file(installed.files(), book, "usfm")
            .map(str::to_owned)
            .ok_or_else(|| Missing::BookNotInResource {
                source: name.clone(),
            })?;
        read_text(&installed, &path, &name)
    })();
    match result {
        Ok(usfm) => TextStatus::Loaded {
            pin: pin.clone(),
            rung,
            usfm,
        },
        Err(missing) => TextStatus::Missing(missing),
    }
}
