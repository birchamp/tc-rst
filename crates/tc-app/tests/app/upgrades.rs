//! Release upgrades, the gateway change and the guided fix (unit C8), through `App::request` on
//! a project written by tc-store, with a fake Door43 serving the releases (journeys J3, J12 and
//! J13 of tC4's `docs/JOURNEYS.md`). The project pins the English notes and links at v86 and
//! holds notes decisions made against v86's Titus; v91 is the release on offer.

use std::collections::BTreeMap;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use tc_api::ApiError;
use tc_api::check::{CheckCollection, CheckUnit, Tool};
use tc_api::ids::{JobId, PlanId, Scope};
use tc_api::jobs::{JobOutcome, JobState};
use tc_api::net::Set;
use tc_api::resources::{
    ApplyPlan, CheckUpdates, OfferRef, PlanApplied, PlanUpgrade, PlanView, PlanWhat, SetRung,
};
use tc_api::session::{Open, OpenUnit};
use tc_api::{Endpoint, NotSaved, Reply};
use tc_app::StoreChoice;
use tc_app::backend::{ProjectHandle, StoreCommand};
use tc_app::config::ProjectsFolder;
use tc_check::derive::{BookSpec, notes_for_book};
use tc_check::status::{Edit, Triage, decision_edit};
use tc_check::{CheckItem, CheckKey, DecisionRecord};
use tc_core::BookId;
use tc_door43_fake::{CommitSeed, FakeDoor43, ReleaseKind, RepoSeed, SeedFile};
use tc_helps::tsv::{TsvScope, parse_notes};
use tc_journal::command::{Command, DecisionChange, DecisionEdit, PlannedChange, UpgradePlan};
use tc_journal::fold::ProjectKind;
use tc_journal::keys::PinGroup;
use tc_journal::record::{CheckValue, Record};
use tc_journal::{Cause, RecordKey};
use tc_resources::{BookCoverage, GatewayLanguage, LanguageSet, ProjectPins, ResourcePin, SetSlot};
use tc_store::fault::FaultFs;
use tc_store::{FakePlatform, NewProject};
use tempfile::TempDir;

use super::common::{Parts, TestApp, rid};
use crate::store_world::{APP, assert_valid_burrito, backend_over, sample_records};

const PROJECT: &str = "upgrade_burrito";

fn repo_root() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn fixture(rel: &str) -> Vec<u8> {
    std::fs::read(repo_root().join("fixtures").join(rel)).unwrap()
}

/// The English set the project pins at `version`: Door43's notes and links, which record Titus.
fn english(tn: &ResourcePin, twl: &ResourcePin) -> LanguageSet {
    LanguageSet::new(GatewayLanguage {
        language: "en".to_owned(),
        owner: "unfoldingWord".parse().unwrap(),
    })
    .with(SetSlot::TranslationNotes, tn.clone())
    .with(SetSlot::TranslationWordsLinks, twl.clone())
}

async fn release(fake: &FakeDoor43, repo: &str, tag: &str) -> ResourcePin {
    let sha = fake.resolve("unfoldingWord", repo, tag).await.unwrap();
    let flavor = if repo == "en_tn" {
        "parascriptural/x-bcvnotes"
    } else {
        "parascriptural/x-bcvarticles"
    };
    ResourcePin::new(
        format!("git.door43.org/unfoldingWord/{repo}")
            .parse()
            .unwrap(),
        sha.parse().unwrap(),
    )
    .with_version(tag.parse().unwrap())
    .with_flavor(flavor)
    .with_books(BookCoverage::Books(vec![BookId::Titus]))
}

/// The checks of Titus in the notes at v86, each decided valid: a decision made against the
/// release the project pins, as the Check screen writes it.
fn decisions(tn: &ResourcePin, titus: &str, only: usize) -> Vec<(CheckKey, DecisionRecord)> {
    let table =
        String::from_utf8(fixture("tc4/test-fixtures/resources/en_tn@v86/TIT.tsv")).unwrap();
    let notes = parse_notes(&table, TsvScope::Book(BookId::Titus))
        .unwrap()
        .notes;
    let eng = tc_versification::StandardScheme::Eng.scheme();
    let list = notes_for_book(
        &notes,
        &BookSpec {
            book: BookId::Titus,
            resource_frame: eng,
            project_frame: Some(eng),
            scope: &[],
        },
    );
    let book = tc_usfm::Book::parse(titus);
    list.items
        .iter()
        .filter_map(|item: &CheckItem| {
            let (chapter, verse) = item.key.target.parts();
            let content = book
                .verse(
                    chapter.as_str().parse().ok()?,
                    tc_core::VerseSpan::single(tc_core::VerseNo::new(
                        verse.as_str().parse().ok()?,
                    )?),
                )
                .ok()?
                .body()
                .to_owned();
            let record = decision_edit(
                item,
                Edit::Triage(Triage::Valid(Vec::new())),
                &tn.resource(),
            )
            .apply(None, Some(&content))
            .ok()?;
            Some((item.key.clone(), record))
        })
        .take(only)
        .collect()
}

/// A decision on a check no release carries: made against v86, found in no later list.
fn orphan(of: &(CheckKey, DecisionRecord), id: &str) -> (CheckKey, DecisionRecord) {
    let mut key = of.0.clone();
    key.check_id = tc_check::CheckId::parse(id).unwrap();
    let mut record = of.1.clone();
    record
        .context
        .insert("checkId".into(), tc_journal::Json::from(id));
    (key, record)
}

/// Two releases of the Greek New Testament, made of a fixture's files under its name: the text
/// the project's alignments point at, and the one that replaces it.
async fn add_texts(fake: &FakeDoor43) {
    let release = |tag: &str| CommitSeed {
        message: format!("Release {tag}"),
        files: export("en_ult@v89", "unfoldingWord/el-x-koine_ugnt"),
        tag: Some(tag.to_owned()),
        release: Some(ReleaseKind::Published),
    };
    fake.add_repo(RepoSeed {
        owner: "unfoldingWord".into(),
        name: "el-x-koine_ugnt".into(),
        subject: "Greek New Testament".into(),
        language: "el-x-koine".into(),
        topics: vec!["tc-ready".into()],
        private: false,
        commits: vec![release("v0.34"), release("v0.35")],
    })
    .await
    .unwrap();
}

async fn text_pin(fake: &FakeDoor43, repo: &str, tag: &str) -> ResourcePin {
    let sha = fake.resolve("unfoldingWord", repo, tag).await.unwrap();
    ResourcePin::new(
        format!("git.door43.org/unfoldingWord/{repo}")
            .parse()
            .unwrap(),
        sha.parse().unwrap(),
    )
    .with_version(tag.parse().unwrap())
    .with_flavor("scripture/textTranslation")
}

/// The translation words at two releases of one repository: v1 holds the article `kt/god`, and v2
/// is the same without it.
async fn add_words(fake: &FakeDoor43) -> ResourcePin {
    let v1 = export("en_tw@v87", "unfoldingWord/en_tw");
    let v2: Vec<SeedFile> = v1
        .iter()
        .filter(|f| f.path != "ingredients/payload/kt/god.md")
        .map(|f| {
            if f.path != "metadata.json" {
                return f.clone();
            }
            let mut metadata: serde_json::Value = serde_json::from_slice(&f.bytes).unwrap();
            metadata["ingredients"]
                .as_object_mut()
                .unwrap()
                .remove("ingredients/payload/kt/god.md");
            SeedFile::regular(
                "metadata.json",
                serde_json::to_vec_pretty(&metadata).unwrap(),
            )
        })
        .collect();
    let release = |tag: &str, files: Vec<SeedFile>| CommitSeed {
        message: format!("Release {tag}"),
        files,
        tag: Some(tag.to_owned()),
        release: Some(ReleaseKind::Published),
    };
    fake.add_repo(RepoSeed {
        owner: "unfoldingWord".into(),
        name: "en_tw".into(),
        subject: "Translation Words".into(),
        language: "en".into(),
        topics: vec!["tc-ready".into()],
        private: false,
        commits: vec![release("v1", v1), release("v2", v2)],
    })
    .await
    .unwrap();
    let sha = fake.resolve("unfoldingWord", "en_tw", "v1").await.unwrap();
    ResourcePin::new(
        "git.door43.org/unfoldingWord/en_tw".parse().unwrap(),
        sha.parse().unwrap(),
    )
    .with_version("v1".parse().unwrap())
    .with_flavor("parascriptural/x-bcvarticles")
}

/// Words checks of Titus, each decided valid against the links the project pins: the first
/// `only` of its list, which holds the articles the pinned words carry.
fn word_decisions(
    twl: &ResourcePin,
    tw: &ResourcePin,
    titus: &str,
    only: usize,
) -> Vec<(CheckKey, DecisionRecord)> {
    let table =
        String::from_utf8(fixture("tc4/test-fixtures/resources/en_twl@v86/TIT.tsv")).unwrap();
    let links = tc_helps::tsv::parse_word_links(&table, TsvScope::Book(BookId::Titus)).unwrap();
    let metadata: String = String::from_utf8(
        export("en_tw@v87", "unfoldingWord/en_tw")
            .into_iter()
            .find(|f| f.path == "metadata.json")
            .unwrap()
            .bytes,
    )
    .unwrap();
    let articles = tc_helps::metadata::parse_burrito_metadata(&metadata)
        .unwrap()
        .word_articles();
    let eng = tc_versification::StandardScheme::Eng.scheme();
    let list = tc_check::derive::word_links_for_book(
        &links,
        Some(&articles),
        &BookSpec {
            book: BookId::Titus,
            resource_frame: eng,
            project_frame: Some(eng),
            scope: &[],
        },
    );
    let _ = tw;
    let book = tc_usfm::Book::parse(titus);
    list.items
        .iter()
        .filter_map(|item: &CheckItem| {
            let (chapter, verse) = item.key.target.parts();
            let content = book
                .verse(
                    chapter.as_str().parse().ok()?,
                    tc_core::VerseSpan::single(tc_core::VerseNo::new(
                        verse.as_str().parse().ok()?,
                    )?),
                )
                .ok()?
                .body()
                .to_owned();
            let record = decision_edit(
                item,
                Edit::Triage(Triage::Valid(Vec::new())),
                &twl.resource(),
            )
            .apply(None, Some(&content))
            .ok()?;
            Some((item.key.clone(), record))
        })
        .take(only)
        .collect()
}

struct World {
    _dir: TempDir,
    t: TestApp,
    fake: FakeDoor43,
    fs: FaultFs,
    at: Scope,
    tn: ResourcePin,
    decided: Vec<(CheckKey, DecisionRecord)>,
    /// The store's hold points.
    hooks: Arc<tc_store::Hooks>,
}

/// The sample's Titus on tc-store's in-memory file system with the English notes and links
/// pinned at v86 on both rungs and `decided` notes decisions, an app over it, and a fake Door43
/// that serves v86 and v91.
async fn world(decided: usize, orphans: usize) -> World {
    world_with(decided, orphans, None).await
}

async fn world_with(decided: usize, orphans: usize, vrs: Option<&[u8]>) -> World {
    build(decided, orphans, vrs, Opts::default()).await
}

/// What a world is made with beyond the sample's Titus and notes.
#[derive(Default)]
struct Opts {
    /// The project pins an original-language text and a gateway Bible pane, and holds aligned
    /// verses.
    texts: bool,
    /// The bundled store this launch seeds from.
    bundle: Option<std::path::PathBuf>,
    /// The notes release the project pins is installed before the app starts, as an earlier
    /// version of the app left it.
    pinned_installed: bool,
    /// The project pins the translation words too, at a release a newer one of which no longer
    /// has the article of some of its decided words checks; the notes are at their newest.
    words: bool,
    /// With `texts`: Titus 1:1's alignment is marked done and vouches for its verse under the
    /// pinned original text, as Mark valid leaves it.
    first_valid: bool,
    trouble: Trouble,
    /// Door43 is an address nothing listens on: the internet is on, the server gone.
    dead_door43: bool,
}

/// What is wrong with the project's records beyond the sample's: what a teammate's sync, a
/// Door43 release or an earlier version of the app can leave there.
#[derive(Default, Clone, Copy, PartialEq, Eq)]
enum Trouble {
    #[default]
    Nothing,
    /// The first decision is two decisions with check ids v91 does not have, each recording
    /// the quote of the one v91 note that both come to.
    TwoOnOneCheck,
    /// The second decision has two heads: this computer's judgement and a teammate's.
    ConflictedDecision,
    /// The primary pins have two heads: this computer's, and a teammate's with other coverage.
    ConflictedPins,
    /// The primary pins have two heads that agree: this computer's, and a teammate's that
    /// removes the links pin and changes the coverage of the notes.
    MergedPins,
}

/// A project that pins an original-language text and a gateway Bible pane as well, and holds
/// aligned verses.
async fn world_with_texts(decided: usize) -> World {
    build(
        decided,
        0,
        None,
        Opts {
            texts: true,
            ..Opts::default()
        },
    )
    .await
}

async fn build(decided: usize, orphans: usize, vrs: Option<&[u8]>, opts: Opts) -> World {
    let texts = opts.texts;
    let decided_count = decided;
    let fake = FakeDoor43::start_seeded().await.unwrap();
    // A project that pins the words has its notes at the newest, so the words are all an upgrade moves.
    let tn = release(&fake, "en_tn", if opts.words { "v91" } else { "v86" }).await;
    let twl = release(&fake, "en_twl", "v86").await;
    let titus = fixture("tc4/sample-burrito/ingredients/TIT.usfm");
    let text = String::from_utf8(titus.clone()).unwrap();
    let mut decided = if opts.words {
        Vec::new()
    } else {
        decisions(&tn, &text, decided)
    };
    for n in 0..orphans {
        let made = orphan(&decided[0], &format!("zz{n}z"));
        decided.push(made);
    }
    if opts.trouble == Trouble::TwoOnOneCheck {
        let quote = v91_quote_of(&decided[0].0);
        let twin = |id: &str| {
            let mut twin = orphan(&decided[0], id);
            twin.1
                .context
                .insert("quoteString".into(), tc_journal::Json::from(quote.as_str()));
            twin
        };
        let twins = [twin("aa1a"), twin("bb2b")];
        decided.splice(0..1, twins);
    }
    let mut records = sample_records(&BTreeMap::from([(BookId::Titus, titus)]), vrs);
    let mut pins = ProjectPins {
        primary: Some(english(&tn, &twl)),
        fallback: Some(english(&tn, &twl)),
        ..ProjectPins::default()
    };
    if opts.words {
        let tw = add_words(&fake).await;
        for set in [pins.primary.as_mut(), pins.fallback.as_mut()]
            .into_iter()
            .flatten()
        {
            set.pins.insert(SetSlot::TranslationWords, tw.clone());
        }
        decided = word_decisions(&twl, &tw, &text, decided_count);
    }
    if texts {
        add_texts(&fake).await;
        let ugnt = text_pin(&fake, "el-x-koine_ugnt", "v0.34").await;
        pins.original_language.nt = Some(ugnt.clone());
        let old_ult = ResourcePin::new(
            "git.door43.org/unfoldingWord/en_ult".parse().unwrap(),
            "5".repeat(40).parse().unwrap(),
        )
        .with_version("v88".parse().unwrap())
        .with_flavor("scripture/textTranslation");
        pins.extra_scripture = vec![tc_resources::ScripturePane {
            id: "ult".parse().unwrap(),
            pin: old_ult,
        }];
        let verse_text = |n: u16| {
            records.iter().find_map(|r| match r {
                Record::Verse {
                    book: BookId::Titus,
                    chapter,
                    verse,
                    value: tc_journal::record::VerseValue::Text { words, layout, .. },
                } if chapter.get() == 1 && verse.get() == n => Some(format!("{words}{layout}")),
                _ => None,
            })
        };
        let first_md5 = verse_text(1).map(|text| tc_align::TargetVerse::new(&text).md5());
        for verse in 1..=3u16 {
            let valid = opts.first_valid && verse == 1;
            records.push(Record::Alignment {
                book: BookId::Titus,
                chapter: tc_journal::ChapterNo::new(1).unwrap(),
                verse: tc_journal::VerseNo::new(verse).unwrap(),
                value: tc_journal::record::AlignmentValue {
                    span_end: tc_journal::VerseNo::new(verse).unwrap(),
                    alignments: Vec::new(),
                    word_bank: Vec::new(),
                    target_verse_md5: match first_md5 {
                        Some(md5) if valid => md5,
                        _ => tc_journal::Md5::of(b"Paul"),
                    },
                    source_version: "dcs::unfoldingWord/el-x-koine_ugnt@v0.34".into(),
                    source: Some(ugnt.resource()),
                    done: valid,
                    marked_invalid: false,
                    dismissed: false,
                    extra: tc_journal::json::JsonMap::default(),
                },
            });
        }
    }
    let groups: &[PinGroup] = if texts {
        &PinGroup::ALL
    } else {
        &[PinGroup::Primary, PinGroup::Fallback]
    };
    for &group in groups {
        records.push(Record::Pins {
            group,
            value: pins.group_value(group).unwrap(),
        });
    }
    for (key, record) in &decided {
        records.push(Record::Check {
            key: key.clone(),
            value: CheckValue::Decision(record.clone()),
        });
    }
    let fs = FaultFs::new();
    let hooks = Arc::<tc_store::Hooks>::default();
    let (backend, watch) = backend_over(&fs, APP, FakePlatform::default(), hooks.clone());
    let folder = backend
        .store()
        .create(&NewProject {
            name: PROJECT.to_owned(),
            kind: ProjectKind::Bible,
            records,
            books: Vec::new(),
        })
        .unwrap();
    assert_valid_burrito(&fs, &folder);
    match opts.trouble {
        Trouble::ConflictedDecision => two_heads(&backend, &folder, 0, |handle| {
            decision_edits(handle, &decided[1].0, &tn)
        }),
        Trouble::ConflictedPins => two_heads(&backend, &folder, 0, |handle| {
            pins_edits(handle, &pins, SetSlot::TranslationWordsLinks)
        }),
        Trouble::MergedPins => two_heads(&backend, &folder, 0, |handle| {
            pins_edits(handle, &pins, SetSlot::TranslationNotes)
        }),
        Trouble::Nothing | Trouble::TwoOnOneCheck => {}
    }
    let dir = tempfile::tempdir().unwrap();
    let projects = dir.path().join("projects");
    std::fs::create_dir_all(&projects).unwrap();
    let url = if opts.dead_door43 {
        "http://127.0.0.1:9/".to_owned()
    } else {
        fake.url().to_owned()
    };
    if opts.pinned_installed {
        let store = tc_resources::ResourceStore::open(
            dir.path().join("app-data").join(tc_app::RESOURCES_FOLDER),
        )
        .unwrap();
        let zip = zip_declaring("en_tn@v86", "unfoldingWord/en_tn", &tn.sha.to_hex());
        tc_resources::install_dcs_zip(&store, &tn, &zip, 1 << 30).unwrap();
    }
    if opts.words {
        // The links the words checks are listed from are on this computer, as they are for any
        // project that can check its words.
        let store = tc_resources::ResourceStore::open(
            dir.path().join("app-data").join(tc_app::RESOURCES_FOLDER),
        )
        .unwrap();
        let zip = zip_declaring("en_twl@v86", "unfoldingWord/en_twl", &twl.sha.to_hex());
        tc_resources::install_dcs_zip(&store, &twl, &zip, 1 << 30).unwrap();
    }
    let bundle = opts.bundle.clone();
    let mut t = TestApp::start_with(dir.path(), &Parts::new(), |config, deps| {
        config.projects = ProjectsFolder::Given(projects.clone());
        config.door43 = url.parse().unwrap();
        config.bundle = bundle;
        deps.store = StoreChoice::Given(Arc::new(backend));
    })
    .await;
    if opts.bundle.is_some() {
        while matches!(t.app.startup(), tc_api::event::StartupState::Preparing(_)) {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    }
    let at = open(&t, PROJECT).await;
    internet(&t, true, 1).await;
    t.watch = Some(watch);
    World {
        _dir: dir,
        t,
        fake,
        fs,
        at,
        tn,
        decided,
        hooks,
    }
}

/// The quote v91 gives the note of the check `key`, which v86 (the decisions') words differently.
fn v91_quote_of(key: &CheckKey) -> String {
    let table =
        String::from_utf8(fixture("tc4/test-fixtures/resources/en_tn@v91/TIT.tsv")).unwrap();
    let notes = parse_notes(&table, TsvScope::Book(BookId::Titus))
        .unwrap()
        .notes;
    let eng = tc_versification::StandardScheme::Eng.scheme();
    notes_for_book(
        &notes,
        &BookSpec {
            book: BookId::Titus,
            resource_frame: eng,
            project_frame: Some(eng),
            scope: &[],
        },
    )
    .items
    .into_iter()
    .find(|item| item.key == *key)
    .unwrap()
    .quote
    .to_string()
}

/// Two heads on a record, from two edits of one token: the second did not see the first, as when
/// a teammate's sync brings a different version of what this computer also changed.
fn two_heads(
    backend: &tc_app::backend::store::StoreBackend,
    folder: &Path,
    conflicts: usize,
    edits: impl FnOnce(&tc_store::ProjectHandle) -> [Command; 2],
) {
    let handle = backend
        .store()
        .open(folder, &tc_store::OpenOptions::default())
        .unwrap();
    for edit in edits(&handle) {
        handle.submit(edit).wait().unwrap();
    }
    assert_eq!(handle.snapshot().fold().conflict_count(), conflicts);
    handle.close().unwrap();
}

/// This computer's judgement of a check, and one that disagrees with it.
fn decision_edits(
    handle: &tc_store::ProjectHandle,
    check: &CheckKey,
    tn: &ResourcePin,
) -> [Command; 2] {
    let (chapter, verse) = check.target.parts();
    let verse = super::common::tit(
        chapter.as_str().parse().unwrap(),
        verse.as_str().parse().unwrap(),
    );
    let shown = handle
        .snapshot()
        .token(&RecordKey::Check(check.clone()), None)
        .unwrap();
    let verse_shown = handle.snapshot().token(&verse, None).unwrap();
    [
        (tc_check::UserStatus::Todo, "mine"),
        (tc_check::UserStatus::Invalid, "theirs"),
    ]
    .map(|(status, comment)| Command::SaveDecision {
        shown: shown.clone(),
        verse_shown: verse_shown.clone(),
        value: DecisionEdit {
            basis: None,
            resource: tn.resource(),
            changes: vec![
                DecisionChange::Status(Some(status)),
                DecisionChange::Comment(Some(comment.to_owned())),
            ],
        },
    })
}

/// The primary pins without the links, and with other coverage for the pin in `slot`: a
/// disagreement when that is the links the first removes, none when it is the notes.
fn pins_edits(handle: &tc_store::ProjectHandle, pins: &ProjectPins, slot: SetSlot) -> [Command; 2] {
    let shown = handle
        .snapshot()
        .token(&RecordKey::Pins(PinGroup::Primary), None)
        .unwrap();
    let mut without_links = pins.clone();
    without_links
        .primary
        .as_mut()
        .unwrap()
        .pins
        .remove(&SetSlot::TranslationWordsLinks);
    let mut with_more_books = pins.clone();
    let set = with_more_books.primary.as_mut().unwrap();
    let pin = set.pin(slot).unwrap().clone();
    set.pins
        .insert(slot, pin.with_books(BookCoverage::WholeCollection));
    [without_links, with_more_books].map(|pins| Command::SetPins {
        shown: shown.clone(),
        value: pins.group_value(PinGroup::Primary).unwrap(),
    })
}

async fn ok<E: Endpoint>(app: &tc_app::App, req: &E) -> E::Ok {
    match app.request(req).await.unwrap() {
        Reply::Ok(ok) => ok,
        Reply::Err(e) => panic!("{}: {e:?}", E::PATH),
    }
}

async fn err<E: Endpoint>(app: &tc_app::App, req: &E) -> ApiError
where
    E::Ok: std::fmt::Debug,
{
    match app.request(req).await.unwrap() {
        Reply::Ok(ok) => panic!("{}: {ok:?}", E::PATH),
        Reply::Err(e) => e,
    }
}

async fn ended(app: &tc_app::App, job: JobId) -> JobState {
    let mut watch = app.jobs().watch(job).unwrap();
    tokio::time::timeout(Duration::from_secs(60), async {
        watch
            .wait_for(tc_app::jobs::JobRegistry::is_ended)
            .await
            .unwrap()
            .clone()
    })
    .await
    .expect("hung: the job ends")
}

async fn open(t: &TestApp, project: &str) -> Scope {
    let job = ok(
        &t.app,
        &Open {
            request: rid(40),
            project: super::common::folder(project),
            unit: Some(OpenUnit::Book(BookId::Titus)),
        },
    )
    .await;
    match ended(&t.app, job).await {
        JobState::Done(JobOutcome::Opened(opened)) => opened.session.at,
        other => panic!("{other:?}"),
    }
}

async fn internet(t: &TestApp, on: bool, id: u8) {
    ok(
        &t.app,
        &Set {
            request: rid(id),
            on,
        },
    )
    .await;
}

async fn plan_set(w: &World, rung: SetRung, id: u8) -> PlanView {
    let job = ok(
        &w.t.app,
        &PlanUpgrade {
            request: rid(id),
            at: w.at.clone(),
            offer: OfferRef::Set { rung },
        },
    )
    .await;
    match ended(&w.t.app, job).await {
        JobState::Done(JobOutcome::PlanReady(view)) => *view,
        other => panic!("{other:?}"),
    }
}

fn pins_of(w: &World) -> ProjectPins {
    let session = w.t.app.read(&w.at).unwrap();
    let read = session.read();
    let mut pins = ProjectPins::default();
    for group in PinGroup::ALL {
        if let Some(Record::Pins { value, .. }) = read.projected(&RecordKey::Pins(group)) {
            pins.set_group_value(group, &value).unwrap();
        }
    }
    pins
}

fn resource_of(w: &World, key: &CheckKey) -> tc_check::ResourceRef {
    let session = w.t.app.read(&w.at).unwrap();
    match session.read().projected(&RecordKey::Check(key.clone())) {
        Some(Record::Check {
            value: CheckValue::Decision(d),
            ..
        }) => d.resource,
        other => panic!("{other:?}"),
    }
}

fn heads(w: &World, key: &RecordKey) -> Vec<tc_journal::ChangeId> {
    w.t.app.read(&w.at).unwrap().read().heads(key)
}

fn handle(w: &World) -> Arc<dyn ProjectHandle> {
    w.t.app.read(&w.at).unwrap().handle().clone()
}

/// How many changes the project's journal holds.
fn changes(w: &World) -> usize {
    handle(w).snapshot().fold().len()
}

fn commands(w: &World) -> u64 {
    handle(w).counters().commands
}

fn confirm(w: &World, plan: PlanId, id: u8) -> ApplyPlan {
    ApplyPlan {
        request: rid(id),
        at: w.at.clone(),
        plan,
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn checking_for_updates_names_what_moved_and_writes_nothing() {
    let w = world(6, 0).await;
    let (before, commands_before) = (changes(&w), commands(&w));
    let found = ok(&w.t.app, &CheckUpdates { at: w.at.clone() }).await;
    let moves = |rung: SetRung| {
        found
            .sets
            .iter()
            .find(|s| s.rung == rung)
            .map(|s| {
                s.moves
                    .iter()
                    .map(|m| (m.repo.as_str(), m.from.as_str(), m.to.as_str()))
                    .collect::<Vec<_>>()
            })
            .unwrap()
    };
    // Notes have a newer release; the links are at the newest, so only the notes are offered.
    assert_eq!(moves(SetRung::Primary), [("en_tn", "v86", "v91")]);
    assert_eq!(moves(SetRung::Fallback), [("en_tn", "v86", "v91")]);
    assert_eq!(found.sets[0].language, "en");
    assert!(found.texts.is_empty() && found.texts_error.is_none());
    assert_eq!((changes(&w), commands(&w)), (before, commands_before));
    assert!(
        w.t.app.read(&w.at).is_ok() && installed_tn(&w).is_empty(),
        "asking installs nothing"
    );
}

fn installed_tn(w: &World) -> Vec<String> {
    let store =
        tc_resources::ResourceStore::open(w.t.app_data.join(tc_app::RESOURCES_FOLDER)).unwrap();
    store
        .installed_dcs_pins()
        .unwrap()
        .iter()
        .map(|p| format!("{}", p.repo.repo().name))
        .collect()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn j12_an_upgrade_moves_one_sets_pins_and_carries_decisions_in_one_batch() {
    let w = world(12, 2).await;
    let fallback_before = heads(&w, &RecordKey::Pins(PinGroup::Fallback));
    let verse_heads = heads(&w, &super::common::tit(1, 1));
    let view = plan_set(&w, SetRung::Primary, 50).await;
    // The release is installed before the plan is read, and no pin has moved yet.
    assert_eq!(installed_tn(&w), ["en_tn"]);
    assert_eq!(
        pins_of(&w)
            .primary
            .unwrap()
            .pin(SetSlot::TranslationNotes)
            .unwrap()
            .sha,
        w.tn.sha
    );
    let PlanWhat::Set {
        rung,
        language,
        moves,
    } = &view.what
    else {
        panic!("{:?}", view.what)
    };
    assert_eq!((*rung, language.as_str()), (SetRung::Primary, "en"));
    assert_eq!(moves.len(), 1);
    // 14 decisions were made against v86: 12 find their check in v91 (four of them under a
    // quote v91 punctuates differently) and two are on checks v91 does not have.
    assert_eq!(view.decisions_at_risk, 14);
    let [row] = &view.rows[..] else {
        panic!("{:?}", view.rows)
    };
    assert_eq!(
        (row.tool, row.unit),
        (Tool::TranslationNotes, CheckCollection::Book(BookId::Titus))
    );
    assert_eq!((row.decisions, row.against.as_str()), (14, "en_tn"));
    let carry = row.carry.unwrap();
    assert_eq!((carry.carried, carry.check_again), (12, 2));
    assert!(view.blocked.is_empty() && view.alignments.is_none());

    let (changes_before, commands_before) = (changes(&w), commands(&w));
    let applied = ok(&w.t.app, &confirm(&w, view.plan, 51)).await;
    assert_eq!(applied, PlanApplied::Applied);
    handle(&w).wait_idle();
    // One command wrote the whole plan: the primary pins and the twelve decisions.
    assert_eq!(commands(&w) - commands_before, 1);
    assert_eq!(changes(&w) - changes_before, 13);
    let pins = pins_of(&w);
    let moved = pins
        .primary
        .as_ref()
        .unwrap()
        .pin(SetSlot::TranslationNotes)
        .unwrap();
    assert_eq!(
        moved.version.as_ref().map(ToString::to_string).as_deref(),
        Some("v91")
    );
    assert_ne!(moved.sha, w.tn.sha);
    assert!(
        moved.books.is_some(),
        "coverage is recorded for the new pin (D41)"
    );
    // The other language set, the text and the other pins are as they were.
    assert_eq!(
        heads(&w, &RecordKey::Pins(PinGroup::Fallback)),
        fallback_before
    );
    assert_eq!(
        pins.fallback
            .unwrap()
            .pin(SetSlot::TranslationNotes)
            .unwrap()
            .sha,
        w.tn.sha
    );
    assert_eq!(heads(&w, &super::common::tit(1, 1)), verse_heads);
    // Each carried decision now records v91; the two with no check there are kept as they were.
    let (carried, orphans) = w.decided.split_at(12);
    for (key, was) in carried {
        let now = resource_of(&w, key);
        assert_eq!(now.sha, moved.sha, "{}", key.check_id);
        assert_ne!(now.sha, was.resource.sha);
    }
    for (key, was) in orphans {
        assert_eq!(
            resource_of(&w, key),
            was.resource,
            "{} is kept",
            key.check_id
        );
    }
    assert_valid_burrito(
        &w.fs,
        &Path::new(crate::store_world::PROJECTS).join(PROJECT),
    );
    // Nothing is left to confirm: the plan is spent.
    assert_eq!(
        err(&w.t.app, &confirm(&w, view.plan, 52)).await,
        ApiError::NotSaved(NotSaved::PlanStale)
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_download_that_fails_or_cannot_be_trusted_moves_no_pin() {
    use tc_door43_fake::{Fault, FaultRule};
    for (fault, id) in [
        (Fault::BreakOff { after: 400 }, 60),
        (Fault::SbRevision("0".repeat(40)), 61),
    ] {
        let w = world(6, 0).await;
        w.fake
            .inject(FaultRule::once("GET", "/unfoldingWord/en_tn/sb/v91", fault));
        let (before, commands_before) = (changes(&w), commands(&w));
        let job = ok(
            &w.t.app,
            &PlanUpgrade {
                request: rid(id),
                at: w.at.clone(),
                offer: OfferRef::Set {
                    rung: SetRung::Primary,
                },
            },
        )
        .await;
        match ended(&w.t.app, job).await {
            JobState::Failed(_) => {}
            other => panic!("{other:?}"),
        }
        assert_eq!((changes(&w), commands(&w)), (before, commands_before));
        assert_eq!(
            pins_of(&w)
                .primary
                .unwrap()
                .pin(SetSlot::TranslationNotes)
                .unwrap()
                .sha,
            w.tn.sha
        );
        assert!(installed_tn(&w).is_empty(), "nothing half installed");
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_stale_confirm_reshows_the_dialog_with_new_counts() {
    // tC4 #595: the preview is read once; a teammate's work that arrives before the confirm
    // makes the plan stale, and the person reads the plan made again before anything is written.
    let w = world(12, 0).await;
    let view = plan_set(&w, SetRung::Primary, 50).await;
    assert_eq!(view.rows[0].carry.unwrap().carried, 12);
    // A teammate marks one of the decisions To do.
    let (key, was) = w.decided[0].clone();
    let session = w.t.app.read(&w.at).unwrap();
    let mut todo = was.clone();
    todo.judgement.status = Some(tc_check::UserStatus::Todo);
    todo.judgement.nothing_to_select = false;
    let record = RecordKey::Check(key.clone());
    let teammate = UpgradePlan {
        changes: vec![PlannedChange {
            parents: session.read().heads(&record),
            record: Record::Check {
                key: key.clone(),
                value: CheckValue::Decision(todo),
            },
            cause: Cause::Edit,
        }],
    };
    session
        .handle()
        .submit(StoreCommand::apply_upgrade(teammate))
        .await
        .unwrap();
    let before = changes(&w);
    let again = ok(&w.t.app, &confirm(&w, view.plan, 51)).await;
    let PlanApplied::Replanned(fresh) = again else {
        panic!("{again:?}")
    };
    // Nothing was written, and the figures are of the project as it is now.
    assert_eq!(changes(&w), before);
    assert_ne!(fresh.plan, view.plan);
    assert_eq!(
        fresh.rows[0].carry.unwrap().carried,
        11,
        "one decision is no longer decided"
    );
    assert_eq!(
        err(&w.t.app, &confirm(&w, view.plan, 52)).await,
        ApiError::NotSaved(NotSaved::PlanStale),
        "the plan that was read is gone"
    );
    // Confirming what was read last writes it, and the teammate's To do stands.
    assert_eq!(
        ok(&w.t.app, &confirm(&w, fresh.plan, 53)).await,
        PlanApplied::Applied
    );
    match session.read().projected(&record) {
        Some(Record::Check {
            value: CheckValue::Decision(d),
            ..
        }) => {
            assert_eq!(d.judgement.status, Some(tc_check::UserStatus::Todo));
            assert_ne!(d.resource.sha, w.tn.sha, "and it moved with the release");
        }
        other => panic!("{other:?}"),
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_failed_save_keeps_the_plan_for_the_retry() {
    let w = world(6, 0).await;
    let view = plan_set(&w, SetRung::Primary, 50).await;
    let before = changes(&w);
    w.fs.fail_writes(Some(std::io::ErrorKind::StorageFull));
    match err(&w.t.app, &confirm(&w, view.plan, 51)).await {
        ApiError::NotSaved(NotSaved::Disk(_)) => {}
        other => panic!("{other:?}"),
    }
    w.fs.fail_writes(None);
    assert_eq!(
        changes(&w),
        before,
        "a failed save changes no pin and no decision"
    );
    assert_eq!(
        pins_of(&w)
            .primary
            .unwrap()
            .pin(SetSlot::TranslationNotes)
            .unwrap()
            .sha,
        w.tn.sha
    );
    // The plan is still there: the same dialog confirms it when the disk has room.
    assert_eq!(
        ok(&w.t.app, &confirm(&w, view.plan, 52)).await,
        PlanApplied::Applied
    );
    assert_eq!(changes(&w) - before, 7);
}

// ---- J13: a new checking language ------------------------------------------------------------

/// A resource fixture as a Door43 repository's files: its `metadata.json` naming `repo`, and each
/// fixture file at the ingredient path the metadata lists it under.
fn export(fixture_dir: &str, repo: &str) -> Vec<SeedFile> {
    let dir = repo_root()
        .join("fixtures/tc4/test-fixtures/resources")
        .join(fixture_dir);
    let mut metadata: serde_json::Value =
        serde_json::from_slice(&std::fs::read(dir.join("metadata.json")).unwrap()).unwrap();
    let dcs = metadata["identification"]["primary"]["dcs"]
        .as_object_mut()
        .unwrap();
    let revision = dcs.values().next().unwrap().clone();
    dcs.clear();
    dcs.insert(repo.to_owned(), revision);
    let listed: Vec<String> = metadata["ingredients"]
        .as_object()
        .unwrap()
        .keys()
        .cloned()
        .collect();
    let mut files = vec![SeedFile::regular(
        "metadata.json",
        serde_json::to_vec_pretty(&metadata).unwrap(),
    )];
    for entry in std::fs::read_dir(&dir).unwrap() {
        let path = entry.unwrap().path();
        let name = path.file_name().unwrap().to_string_lossy().into_owned();
        if name == "metadata.json" || name == "README.md" {
            continue;
        }
        let ingredient = listed
            .iter()
            .find(|i| i.trim_start_matches("ingredients/").replace('/', "-") == name)
            .unwrap_or_else(|| panic!("{name} is not an ingredient of {fixture_dir}"));
        files.push(SeedFile::regular(ingredient, std::fs::read(&path).unwrap()));
    }
    files
}

/// The Spanish suite beside the seeded `es-419_tn`: links, words and academy made of the English
/// fixtures under their Spanish names.
async fn add_spanish_suite(fake: &FakeDoor43) {
    for (name, subject, fixture_dir) in [
        ("es-419_twl", "TSV Translation Words Links", "en_twl@v86"),
        ("es-419_tw", "Translation Words", "en_tw@v87"),
        ("es-419_ta", "Translation Academy", "en_ta@v86"),
    ] {
        fake.add_repo(RepoSeed {
            owner: "es-419_gl".into(),
            name: name.into(),
            subject: subject.into(),
            language: "es-419".into(),
            topics: vec!["tc-ready".into()],
            private: false,
            commits: vec![CommitSeed {
                message: "Release v1".into(),
                files: export(fixture_dir, &format!("es-419_gl/{name}")),
                tag: Some("v1".into()),
                release: Some(ReleaseKind::Published),
            }],
        })
        .await
        .unwrap();
    }
}

fn spanish() -> tc_api::resources::GatewayRef {
    tc_api::resources::GatewayRef {
        language: "es-419".into(),
        owner: "es-419_gl".into(),
    }
}

async fn install_spanish_suite(w: &World) {
    add_spanish_suite(&w.fake).await;
    let job = ok(
        &w.t.app,
        &tc_api::resources::Download {
            request: rid(70),
            at: w.at.clone(),
            gateway: spanish(),
            book: BookId::Titus,
            repos: ["es-419_tn", "es-419_twl", "es-419_tw", "es-419_ta"]
                .map(str::to_owned)
                .to_vec(),
        },
    )
    .await;
    assert_eq!(
        ended(&w.t.app, job).await,
        JobState::Done(JobOutcome::Finished)
    );
}

async fn plan_gateway(w: &World, id: u8) -> PlanView {
    let job = ok(
        &w.t.app,
        &tc_api::resources::PlanGateway {
            request: rid(id),
            at: w.at.clone(),
            gateway: spanish(),
        },
    )
    .await;
    match ended(&w.t.app, job).await {
        JobState::Done(JobOutcome::PlanReady(view)) => *view,
        other => panic!("{other:?}"),
    }
}

fn spanish_tn(w: &World) -> tc_check::ResourceRef {
    let primary = pins_of(w).primary.unwrap();
    primary.pin(SetSlot::TranslationNotes).unwrap().resource()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn j13_a_new_checking_language_moves_the_primary_set_and_keeps_the_english_fallback() {
    let w = world(12, 2).await;
    install_spanish_suite(&w).await;
    let fallback_before = heads(&w, &RecordKey::Pins(PinGroup::Fallback));
    let (changes_before, commands_before) = (changes(&w), commands(&w));
    let view = plan_gateway(&w, 71).await;
    // Planning wrote nothing, and both languages are named as the person knows them (#579).
    assert_eq!(
        (changes(&w), commands(&w)),
        (changes_before, commands_before)
    );
    let PlanWhat::Gateway(change) = &view.what else {
        panic!("{:?}", view.what)
    };
    assert_eq!(change.from.as_ref().unwrap().autonym, "English");
    assert_eq!(change.to.autonym, "Español (Latinoamérica)");
    assert!(change.images.is_none());
    // 14 notes decisions were made against English: seven find their check in the Spanish
    // notes and seven (the two with no check anywhere among them) come back as work.
    assert_eq!(view.decisions_at_risk, 14);
    let [row] = &view.rows[..] else {
        panic!("{:?}", view.rows)
    };
    assert_eq!(
        (row.tool, row.unit),
        (Tool::TranslationNotes, CheckCollection::Book(BookId::Titus))
    );
    let carry = row.carry.unwrap();
    assert_eq!((carry.carried, carry.check_again), (7, 7));
    assert!(view.blocked.is_empty());

    assert_eq!(
        ok(&w.t.app, &confirm(&w, view.plan, 72)).await,
        PlanApplied::Applied
    );
    handle(&w).wait_idle();
    assert_eq!(commands(&w) - commands_before, 1, "one batch");
    let pins = pins_of(&w);
    let primary = pins.primary.as_ref().unwrap();
    assert_eq!(primary.gateway.named().unwrap().language, "es-419");
    for slot in [
        SetSlot::TranslationNotes,
        SetSlot::TranslationWordsLinks,
        SetSlot::TranslationWords,
        SetSlot::TranslationAcademy,
    ] {
        let pin = primary.pin(slot).unwrap();
        assert!(
            pin.repo.repo().name.to_string().starts_with("es-419_"),
            "{slot:?}"
        );
    }
    // The English fallback is as it was: the same record, untouched.
    assert_eq!(
        heads(&w, &RecordKey::Pins(PinGroup::Fallback)),
        fallback_before
    );
    assert_eq!(
        pins.fallback
            .unwrap()
            .pin(SetSlot::TranslationNotes)
            .unwrap()
            .sha,
        w.tn.sha
    );
    // The seven carried decisions record the Spanish notes now; every other decision is kept
    // exactly as it was, and none is deleted.
    let spanish = spanish_tn(&w);
    let on_spanish = w
        .decided
        .iter()
        .filter(|(key, _)| resource_of(&w, key) == spanish)
        .count();
    assert_eq!(on_spanish, 7);
    for (key, was) in &w.decided {
        let now = resource_of(&w, key);
        assert!(now == spanish || now == was.resource, "{}", key.check_id);
    }
}

/// Going Home closes the session behind the person, and its leave writes the derived files
/// after `session/close` has answered: Settings opened from the card meanwhile names the
/// checking language just chosen, never the one the files held before.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn settings_from_home_right_after_a_change_name_the_new_checking_language() {
    let w = world(0, 0).await;
    install_spanish_suite(&w).await;
    let view = plan_gateway(&w, 71).await;
    assert_eq!(
        ok(&w.t.app, &confirm(&w, view.plan, 72)).await,
        PlanApplied::Applied
    );
    w.hooks.hold(tc_store::Point::BeforeCheckpoint);
    ok(
        &w.t.app,
        &tc_api::session::Close {
            request: rid(73),
            at: w.at.clone(),
        },
    )
    .await;
    let app = w.t.app.clone();
    let mut settings = tokio::spawn(async move {
        ok(
            &app,
            &tc_api::settings::ViewFromHome {
                project: super::common::folder(PROJECT),
            },
        )
        .await
    });
    if let Ok(early) = tokio::time::timeout(Duration::from_millis(300), &mut settings).await {
        panic!("Settings answered before the leave wrote the pins: {early:?}");
    }
    w.hooks.release(tc_store::Point::BeforeCheckpoint);
    let current = settings.await.unwrap().gateway.current.unwrap();
    assert_eq!(current.gateway, spanish());
    assert_eq!(current.autonym, "Español (Latinoamérica)");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn gateway_change_dialog_names_both_languages_by_autonym() {
    // tC4 #579: the languages are named as their own people name them, never by a code, in the
    // title, both buttons and the card behind.
    let w = world(6, 0).await;
    install_spanish_suite(&w).await;
    let view = plan_gateway(&w, 71).await;
    let PlanWhat::Gateway(change) = &view.what else {
        panic!("{:?}", view.what)
    };
    assert_eq!(change.from.as_ref().unwrap().autonym, "English");
    assert_eq!(change.to.autonym, "Español (Latinoamérica)");
    assert_ne!(
        change.to.autonym, change.to.gateway.language,
        "not the code"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_gateway_plan_that_cannot_read_a_resource_says_so_and_changes_nothing() {
    // tC4's D30 sweep: a count made over a resource that could not be read would understate the
    // change, so the preview fails, with Close and no Change button, instead.
    let w = world(12, 0).await;
    install_spanish_suite(&w).await;
    let store =
        tc_resources::ResourceStore::open(w.t.app_data.join(tc_app::RESOURCES_FOLDER)).unwrap();
    let notes = store
        .installed_dcs_pins()
        .unwrap()
        .into_iter()
        .find(|p| p.repo.repo().name.to_string() == "es-419_tn")
        .unwrap();
    let installed = store.dcs(&notes).unwrap().unwrap();
    std::fs::remove_file(installed.dir().join("content/ingredients/TIT.tsv")).unwrap();
    let before = changes(&w);
    let job = ok(
        &w.t.app,
        &tc_api::resources::PlanGateway {
            request: rid(71),
            at: w.at.clone(),
            gateway: spanish(),
        },
    )
    .await;
    match ended(&w.t.app, job).await {
        JobState::Failed(ApiError::Refused(tc_api::Refusal::Resources(
            tc_api::refusal::ResourceRefusal::GatewayFailed { reason },
        ))) => assert!(reason.contains("es-419_tn"), "{reason}"),
        other => panic!("{other:?}"),
    }
    assert_eq!(changes(&w), before);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn gateway_preview_appears_while_the_open_verse_keeps_changing() {
    // tC4 #595: the preview was read again for as long as the project kept changing. It is read
    // once, from one snapshot; typing in the open verse touches no record the plan names, so
    // the confirm that follows applies.
    let w = world(12, 2).await;
    install_spanish_suite(&w).await;
    let session = w.t.app.read(&w.at).unwrap();
    let key = super::common::tit(1, 1);
    let typing = {
        let session = session.clone();
        tokio::spawn(async move {
            for n in 0..40 {
                let token = session.read().mint(&key, None).unwrap();
                let words = format!("Paul, a servant of God {n}");
                let _ = session
                    .handle()
                    .submit(StoreCommand::save_verse(token, words))
                    .await;
            }
        })
    };
    let view = plan_gateway(&w, 71).await;
    typing.await.unwrap();
    assert_eq!(view.rows[0].carry.unwrap().carried, 7);
    assert_eq!(
        ok(&w.t.app, &confirm(&w, view.plan, 72)).await,
        PlanApplied::Applied,
        "the verse edits moved no record the plan wrote"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_project_whose_numbering_is_unknown_cannot_carry_its_decisions_over() {
    // The decisions cannot be placed under verse numbers the project may not have, so the
    // change is blocked and says why, and nothing is written.
    let mut eng: serde_json::Value =
        serde_json::from_str(tc_versification::StandardScheme::Eng.json()).unwrap();
    eng["maxVerses"]["TIT"][0] = serde_json::json!(17);
    let vrs = serde_json::to_vec(&eng).unwrap();
    let w = world_with(6, 0, Some(&vrs)).await;
    install_spanish_suite(&w).await;
    let view = plan_gateway(&w, 71).await;
    // The unit is listed with the decisions it holds and no carry-over.
    let [row] = &view.rows[..] else {
        panic!("{:?}", view.rows)
    };
    assert_eq!((row.decisions, row.carry), (6, None));
    let [blocked] = &view.blocked[..] else {
        panic!("{:?}", view.blocked)
    };
    assert_eq!(
        (blocked.tool, blocked.unit, blocked.reason),
        (
            Tool::TranslationNotes,
            CheckCollection::Book(BookId::Titus),
            tc_api::resources::BlockReason::VersificationUnknown
        )
    );
    let before = changes(&w);
    match err(&w.t.app, &confirm(&w, view.plan, 72)).await {
        ApiError::Refused(tc_api::Refusal::Resources(
            tc_api::refusal::ResourceRefusal::GatewayBlocked { books },
        )) => assert_eq!(books, "TIT"),
        other => panic!("{other:?}"),
    }
    assert_eq!(changes(&w), before);
}

// ---- J3: the guided fix ----------------------------------------------------------------------

use tc_api::resources::{FixFetch, FixRepin, FixSideload, FixState};

fn fix_state(w: &World) -> FixState {
    FixState {
        at: w.at.clone(),
        tool: Tool::TranslationNotes,
        unit: CheckUnit::Book(BookId::Titus),
    }
}

/// A burrito zip of the fixture's export that declares `commit` as its revision.
fn zip_declaring(fixture_dir: &str, repo: &str, commit: &str) -> Vec<u8> {
    use std::io::Write;
    let mut writer = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    let options = zip::write::SimpleFileOptions::default();
    for file in export(fixture_dir, repo) {
        let bytes = if file.path == "metadata.json" {
            let mut metadata: serde_json::Value = serde_json::from_slice(&file.bytes).unwrap();
            let dcs = metadata["identification"]["primary"]["dcs"]
                .as_object_mut()
                .unwrap();
            for value in dcs.values_mut() {
                value["revision"] = serde_json::json!(commit);
            }
            serde_json::to_vec(&metadata).unwrap()
        } else {
            file.bytes
        };
        writer.start_file(&file.path, options).unwrap();
        writer.write_all(&bytes).unwrap();
    }
    writer.finish().unwrap().into_inner()
}

/// The server's uploaded files, as a test keeps them.
#[derive(Default)]
struct Uploaded(std::sync::Mutex<BTreeMap<tc_api::ids::UploadId, tc_app::UploadedFile>>);

impl tc_app::UploadedFiles for Uploaded {
    fn take(&self, id: &tc_api::ids::UploadId) -> Option<tc_app::UploadedFile> {
        self.0.lock().unwrap().remove(id)
    }
}

impl Uploaded {
    fn put(&self, dir: &Path, bytes: &[u8]) -> tc_api::ids::UploadId {
        let id = tc_api::ids::UploadId::parse("0123456789abcdef0123456789abcdef").unwrap();
        let path = dir.join("upload.zip");
        std::fs::write(&path, bytes).unwrap();
        self.0.lock().unwrap().insert(
            id.clone(),
            tc_app::UploadedFile {
                name: "en_tn.zip".into(),
                path,
            },
        );
        id
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_guided_fix_names_the_missing_pin_and_the_other_versions_installed() {
    let w = world(6, 0).await;
    let view = ok(&w.t.app, &fix_state(&w))
        .await
        .expect("the notes are not installed");
    assert_eq!(view.pin.repo_path, "git.door43.org/unfoldingWord/en_tn");
    assert_eq!(view.pin.version.as_deref(), Some("v86"));
    assert_eq!(view.pin.sha, w.tn.sha.to_hex());
    assert_eq!(view.rung, SetRung::Primary);
    assert!(view.candidates.is_empty() && view.candidates_error.is_none());
    // Planning a release upgrade installs v91 and moves no pin: v91 is now a version to move to.
    plan_set(&w, SetRung::Primary, 50).await;
    let view = ok(&w.t.app, &fix_state(&w)).await.unwrap();
    let versions: Vec<_> = view
        .candidates
        .iter()
        .map(|c| c.version.as_deref())
        .collect();
    assert_eq!(versions, [Some("v91")]);
    // The Words tool of this project pins links only at v86, which is not here either.
    let words = FixState {
        tool: Tool::TranslationWords,
        ..fix_state(&w)
    };
    assert_eq!(
        ok(&w.t.app, &words).await.unwrap().pin.repo_path,
        "git.door43.org/unfoldingWord/en_twl"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn j3_fetching_the_pinned_version_installs_that_commit_and_moves_no_pin() {
    let w = world(6, 0).await;
    let before = changes(&w);
    let job = ok(
        &w.t.app,
        &FixFetch {
            request: rid(80),
            at: w.at.clone(),
            tool: Tool::TranslationNotes,
            unit: CheckUnit::Book(BookId::Titus),
        },
    )
    .await;
    assert_eq!(
        ended(&w.t.app, job).await,
        JobState::Done(JobOutcome::Finished)
    );
    let store =
        tc_resources::ResourceStore::open(w.t.app_data.join(tc_app::RESOURCES_FOLDER)).unwrap();
    let installed = store.installed_dcs_pins().unwrap();
    assert_eq!(installed.len(), 1);
    assert_eq!(installed[0].sha, w.tn.sha, "exactly the pinned commit");
    assert_eq!(changes(&w), before, "no pin and no decision moved");
    assert!(
        ok(&w.t.app, &fix_state(&w)).await.is_none(),
        "the tool can open now: nothing is left to fix"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_download_of_another_commit_than_the_pinned_one_is_refused() {
    use tc_door43_fake::{Fault, FaultRule};
    let w = world(6, 0).await;
    w.fake.inject(FaultRule::once(
        "GET",
        "/unfoldingWord/en_tn/sb/",
        Fault::SbRevision("0".repeat(40)),
    ));
    let job = ok(
        &w.t.app,
        &FixFetch {
            request: rid(80),
            at: w.at.clone(),
            tool: Tool::TranslationNotes,
            unit: CheckUnit::Book(BookId::Titus),
        },
    )
    .await;
    match ended(&w.t.app, job).await {
        JobState::Failed(ApiError::Refused(tc_api::Refusal::Resources(
            tc_api::refusal::ResourceRefusal::FixFailed { .. },
        ))) => {}
        other => panic!("{other:?}"),
    }
    assert!(installed_tn(&w).is_empty());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_file_is_installed_only_when_it_declares_the_pinned_commit() {
    let w = world(6, 0).await;
    let uploads = Arc::new(Uploaded::default());
    w.t.app.provide_uploads(uploads.clone());
    let dir = w._dir.path();
    let sideload = |upload, request| FixSideload {
        request: rid(request),
        at: w.at.clone(),
        tool: Tool::TranslationNotes,
        unit: CheckUnit::Book(BookId::Titus),
        upload,
    };
    // A release of the same repository with another commit: refused, nothing installed.
    let other = zip_declaring("en_tn@v86", "unfoldingWord/en_tn", &"1".repeat(40));
    let id = uploads.put(dir, &other);
    match err(&w.t.app, &sideload(id, 81)).await {
        ApiError::Refused(tc_api::Refusal::Resources(
            tc_api::refusal::ResourceRefusal::FixFailed { .. },
        )) => {}
        other => panic!("{other:?}"),
    }
    assert!(installed_tn(&w).is_empty());
    // The file that declares exactly the pinned commit is installed.
    let right = zip_declaring("en_tn@v86", "unfoldingWord/en_tn", &w.tn.sha.to_hex());
    let id = uploads.put(dir, &right);
    ok(&w.t.app, &sideload(id, 82)).await;
    assert_eq!(installed_tn(&w), ["en_tn"]);
    assert!(ok(&w.t.app, &fix_state(&w)).await.is_none());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_re_pin_is_planned_and_confirmed_like_an_upgrade() {
    let w = world(12, 2).await;
    plan_set(&w, SetRung::Primary, 50).await; // installs v91, moves nothing
    let candidate = ok(&w.t.app, &fix_state(&w))
        .await
        .unwrap()
        .candidates
        .remove(0);
    let view = ok(
        &w.t.app,
        &FixRepin {
            request: rid(82),
            at: w.at.clone(),
            tool: Tool::TranslationNotes,
            unit: CheckUnit::Book(BookId::Titus),
            sha: candidate.sha.clone(),
        },
    )
    .await;
    let PlanWhat::Repin { rung, moves, .. } = &view.what else {
        panic!("{:?}", view.what)
    };
    assert_eq!(*rung, SetRung::Primary);
    assert_eq!(
        (moves[0].from.as_str(), moves[0].to.as_str()),
        ("v86", "v91")
    );
    let carry = view.rows[0].carry.unwrap();
    assert_eq!((carry.carried, carry.check_again), (12, 2));
    assert_eq!(
        pins_of(&w)
            .primary
            .unwrap()
            .pin(SetSlot::TranslationNotes)
            .unwrap()
            .sha,
        w.tn.sha,
        "the pin moves when the person confirms"
    );
    assert_eq!(
        ok(&w.t.app, &confirm(&w, view.plan, 83)).await,
        PlanApplied::Applied
    );
    assert_eq!(
        pins_of(&w)
            .primary
            .unwrap()
            .pin(SetSlot::TranslationNotes)
            .unwrap()
            .sha
            .to_hex(),
        candidate.sha
    );
}

// ---- scripture texts ---------------------------------------------------------------------------

async fn plan_text(w: &World, repo: &str, id: u8) -> PlanView {
    let job = ok(
        &w.t.app,
        &PlanUpgrade {
            request: rid(id),
            at: w.at.clone(),
            offer: OfferRef::Text {
                repo: repo.to_owned(),
            },
        },
    )
    .await;
    match ended(&w.t.app, job).await {
        JobState::Done(JobOutcome::PlanReady(view)) => *view,
        other => panic!("{other:?}"),
    }
}

fn alignment_heads(w: &World) -> Vec<Vec<tc_journal::ChangeId>> {
    (1..=3)
        .map(|verse| {
            heads(
                w,
                &RecordKey::Alignment(
                    BookId::Titus,
                    tc_journal::ChapterNo::new(1).unwrap(),
                    tc_journal::VerseNo::new(verse).unwrap(),
                ),
            )
        })
        .collect()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn updates_offer_each_scripture_text_with_a_newer_release() {
    let w = world_with_texts(3).await;
    let found = ok(&w.t.app, &CheckUpdates { at: w.at.clone() }).await;
    let texts: Vec<_> = found
        .texts
        .iter()
        .map(|t| {
            (
                t.kind,
                t.change.repo.as_str(),
                t.change.from.as_str(),
                t.change.to.as_str(),
            )
        })
        .collect();
    use tc_api::resources::TextKind;
    assert_eq!(
        texts,
        [
            (TextKind::Original, "el-x-koine_ugnt", "v0.34", "v0.35"),
            (TextKind::Gateway, "en_ult", "v88", "v89"),
        ]
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn ugnt_upgrade_counts_the_verses_whose_alignments_it_marks_and_keeps_every_alignment() {
    // tC4 #722 and J12: the pin moves only after the text is installed and the person accepts;
    // an original-language text marks every alignment of the books it covers for review and
    // deletes none, and the confirmation states the verse count.
    let w = world_with_texts(3).await;
    let aligned = alignment_heads(&w);
    let view = plan_text(&w, "el-x-koine_ugnt", 50).await;
    assert_eq!(view.alignments, Some(3));
    assert_eq!(
        view.decisions_at_risk, 0,
        "no check decision depends on the original text"
    );
    assert!(view.rows.is_empty() && view.blocked.is_empty());
    assert_eq!(
        pins_of(&w)
            .original_language
            .nt
            .unwrap()
            .version
            .unwrap()
            .to_string(),
        "v0.34",
        "nothing moved before the accept"
    );
    assert_eq!(
        ok(&w.t.app, &confirm(&w, view.plan, 51)).await,
        PlanApplied::Applied
    );
    let pins = pins_of(&w);
    assert_eq!(
        pins.original_language
            .nt
            .unwrap()
            .version
            .unwrap()
            .to_string(),
        "v0.35"
    );
    assert_eq!(
        alignment_heads(&w),
        aligned,
        "every alignment is kept as it was"
    );
    assert_eq!(
        pins.extra_scripture[0]
            .pin
            .version
            .as_ref()
            .unwrap()
            .to_string(),
        "v88",
        "the gateway Bible is not touched"
    );
}

/// The state of Titus 1:1's row on the Align rail.
async fn rail_state_of_1_1(w: &World) -> tc_api::align::RailState {
    let rail = ok(
        &w.t.app,
        &tc_api::align::Index {
            at: w.at.clone(),
            book: BookId::Titus,
        },
    )
    .await;
    rail.items
        .iter()
        .find(|i| (i.place.chapter, i.place.verse) == (1, 1))
        .map(|i| i.status.state)
        .expect("Titus 1:1 has a row")
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn ugnt_upgrade_marks_1_1_invalid_in_align() {
    // tC4 #722: after the UGNT upgrade a verse aligned under the old text lost its verse
    // button. The rail and the picker's Align card read the pin's sha, so 1:1 shows as needing
    // review, and its links are still there to review.
    let w = build(
        0,
        0,
        None,
        Opts {
            texts: true,
            first_valid: true,
            ..Opts::default()
        },
    )
    .await;
    use tc_api::align::RailState;
    assert_eq!(
        rail_state_of_1_1(&w).await,
        RailState::Valid,
        "aligned under v0.34"
    );
    let aligned = alignment_heads(&w);
    let view = plan_text(&w, "el-x-koine_ugnt", 50).await;
    assert_eq!(
        ok(&w.t.app, &confirm(&w, view.plan, 51)).await,
        PlanApplied::Applied
    );
    assert_eq!(
        rail_state_of_1_1(&w).await,
        RailState::Invalid,
        "the verse needs review against v0.35"
    );
    assert_eq!(alignment_heads(&w), aligned, "its links are kept");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_gateway_bible_upgrade_moves_its_pane_and_nothing_else() {
    let w = world_with_texts(3).await;
    let aligned = alignment_heads(&w);
    let view = plan_text(&w, "en_ult", 50).await;
    assert_eq!(
        view.alignments, None,
        "a gateway Bible changes no alignment"
    );
    assert!(view.harmless());
    assert_eq!(
        ok(&w.t.app, &confirm(&w, view.plan, 51)).await,
        PlanApplied::Applied
    );
    let pins = pins_of(&w);
    assert_eq!(
        pins.extra_scripture[0]
            .pin
            .version
            .as_ref()
            .unwrap()
            .to_string(),
        "v89"
    );
    assert_eq!(
        pins.original_language
            .nt
            .unwrap()
            .version
            .unwrap()
            .to_string(),
        "v0.34"
    );
    assert_eq!(alignment_heads(&w), aligned);
    for (key, was) in &w.decided {
        assert_eq!(resource_of(&w, key), was.resource, "no decision changed");
    }
}

// ---- bundled-upgrade ---------------------------------------------------------------------------

use super::seed::bundle_build;

/// A bundle of the one release the project could be offered from the installer: `en_tn` v91.
fn bundle_of_notes() -> &'static Path {
    static BUNDLE: std::sync::OnceLock<std::path::PathBuf> = std::sync::OnceLock::new();
    BUNDLE.get_or_init(|| {
        let out = std::path::PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("c8-bundle-notes");
        let _ = std::fs::remove_dir_all(&out);
        let from = repo_root().join("bundle/en");
        let built =
            bundle_build::build_bundle(&from, &out, &|pin| pin.repo.name().as_str() == "en_tn")
                .unwrap_or_else(|e| panic!("the bundle could not be built: {e}"));
        assert_eq!(built.installed.len(), 1, "{built:?}");
        out
    })
}

/// Every file of the install folder of `en_tn`, by path, with its bytes.
fn installed_files(w: &World, sha: &tc_core::GitOid) -> BTreeMap<String, Vec<u8>> {
    let store =
        tc_resources::ResourceStore::open(w.t.app_data.join(tc_app::RESOURCES_FOLDER)).unwrap();
    let pin = store
        .installed_dcs_pins()
        .unwrap()
        .into_iter()
        .find(|p| p.sha == *sha)
        .unwrap_or_else(|| panic!("{sha} is not installed"));
    let install = store.dcs(&pin).unwrap().unwrap();
    install
        .files()
        .map(|path| (path.to_owned(), install.read(path).unwrap()))
        .collect()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn bundled_upgrade_leaves_the_pinned_release_and_the_projects_pins_as_they_were() {
    // tC4 #528: an app upgrade brings a newer bundled release. The release the project pins
    // stays on this computer, byte for byte, and no pin moves of its own accord (#259): the
    // newer release is an offer the person accepts or leaves.
    let w = build(
        6,
        0,
        None,
        Opts {
            bundle: Some(bundle_of_notes().to_owned()),
            pinned_installed: true,
            ..Opts::default()
        },
    )
    .await;
    let bundled: tc_core::GitOid = "e586762e330f482a60c52aedd1c7b3a2f155df8a".parse().unwrap();
    // Both releases are installed: the pinned v86 as it was, and the bundle's v91 beside it.
    let pinned = installed_files(&w, &w.tn.sha);
    assert!(!installed_files(&w, &bundled).is_empty());
    assert_eq!(
        pinned,
        {
            let zip = zip_declaring("en_tn@v86", "unfoldingWord/en_tn", &w.tn.sha.to_hex());
            let expected = tempfile::tempdir().unwrap();
            let store = tc_resources::ResourceStore::open(expected.path().join("r")).unwrap();
            tc_resources::install_dcs_zip(&store, &w.tn, &zip, 1 << 30).unwrap();
            let install = store.dcs(&w.tn).unwrap().unwrap();
            install
                .files()
                .map(|p| (p.to_owned(), install.read(p).unwrap()))
                .collect::<BTreeMap<_, _>>()
        },
        "the pinned release is untouched by the seed"
    );
    // No pin and no decision moved, and the pinned notes can still open.
    assert_eq!(
        pins_of(&w)
            .primary
            .unwrap()
            .pin(SetSlot::TranslationNotes)
            .unwrap()
            .sha,
        w.tn.sha
    );
    assert_eq!(
        pins_of(&w)
            .fallback
            .unwrap()
            .pin(SetSlot::TranslationNotes)
            .unwrap()
            .sha,
        w.tn.sha
    );
    for (key, was) in &w.decided {
        assert_eq!(resource_of(&w, key), was.resource);
    }
    assert!(
        ok(&w.t.app, &fix_state(&w)).await.is_none(),
        "nothing to fix: the pinned notes are here"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_tw_only_change_that_drops_a_decided_article_is_counted() {
    // tC4 #661: a newer translation words release without an article drops the checks that cite
    // it from the list. tC4 says nothing changes; the rebuild counts those decisions, keeps them
    // as they are, and lists them to check again.
    let w = build(
        20,
        0,
        None,
        Opts {
            words: true,
            ..Opts::default()
        },
    )
    .await;
    let citing_god = w
        .decided
        .iter()
        .filter(|(_, d)| d.context.get("groupId").and_then(tc_journal::Json::as_str) == Some("god"))
        .count();
    assert!(
        citing_god > 0 && citing_god < w.decided.len(),
        "{citing_god} of {}",
        w.decided.len()
    );
    let view = plan_set(&w, SetRung::Primary, 50).await;
    let PlanWhat::Set { moves, .. } = &view.what else {
        panic!("{:?}", view.what)
    };
    assert_eq!(
        moves
            .iter()
            .map(|m| (m.repo.as_str(), m.from.as_str(), m.to.as_str()))
            .collect::<Vec<_>>(),
        [("en_tw", "v1", "v2")],
        "the words are all the upgrade moves"
    );
    let [row] = &view.rows[..] else {
        panic!("{:?}", view.rows)
    };
    assert_eq!(
        (row.tool, row.unit),
        (Tool::TranslationWords, CheckCollection::Book(BookId::Titus))
    );
    assert_eq!(
        row.decisions as usize, citing_god,
        "only the decisions whose article left are at risk"
    );
    let carry = row.carry.unwrap();
    assert_eq!(carry.check_again as usize, citing_god);
    assert_eq!(carry.carried as usize, w.decided.len() - citing_god);
    let heads_before: Vec<_> = w
        .decided
        .iter()
        .map(|(k, _)| heads(&w, &RecordKey::Check(k.clone())))
        .collect();
    assert_eq!(
        ok(&w.t.app, &confirm(&w, view.plan, 51)).await,
        PlanApplied::Applied
    );
    let after: Vec<_> = w
        .decided
        .iter()
        .map(|(k, _)| heads(&w, &RecordKey::Check(k.clone())))
        .collect();
    assert_eq!(
        after, heads_before,
        "no decision was written: each is kept as it was"
    );
    assert_eq!(
        pins_of(&w)
            .primary
            .unwrap()
            .pin(SetSlot::TranslationWords)
            .unwrap()
            .version
            .as_ref()
            .unwrap()
            .to_string(),
        "v2"
    );
}

// ---- J3: what a download brings joins the sets that name its package -----------------------------

async fn add_repo(fake: &FakeDoor43, owner: &str, name: &str, fixture_dir: &str) {
    fake.add_repo(RepoSeed {
        owner: owner.into(),
        name: name.into(),
        subject: "Resource".into(),
        language: "en".into(),
        topics: vec!["tc-ready".into()],
        private: false,
        commits: vec![CommitSeed {
            message: "Release v1".into(),
            files: export(fixture_dir, &format!("{owner}/{name}")),
            tag: Some("v1".into()),
            release: Some(ReleaseKind::Published),
        }],
    })
    .await
    .unwrap();
}

async fn download(w: &World, gateway: tc_api::resources::GatewayRef, repos: &[&str], id: u8) {
    let job = ok(
        &w.t.app,
        &tc_api::resources::Download {
            request: rid(id),
            at: w.at.clone(),
            gateway,
            book: BookId::Titus,
            repos: repos.iter().map(|r| (*r).to_owned()).collect(),
        },
    )
    .await;
    assert_eq!(
        ended(&w.t.app, job).await,
        JobState::Done(JobOutcome::Finished)
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_download_adds_what_it_brought_to_the_sets_of_its_package_and_moves_no_pin() {
    // tC4 `adoptDownloadedPins`, J3's "pins with sha": the simplified Bible of the English
    // package joins both English sets, pinned by its commit and release; the notes and links
    // they held stay as they were, and a package of another language adds nothing.
    let w = world(6, 0).await;
    add_repo(&w.fake, "unfoldingWord", "en_ust", "en_ult@v89").await;
    add_repo(&w.fake, "es-419_gl", "es-419_tq", "en_tn@v86").await;
    let english = tc_api::resources::GatewayRef {
        language: "en".into(),
        owner: "unfoldingWord".into(),
    };
    let before = pins_of(&w);
    let (changes_before, commands_before) = (changes(&w), commands(&w));

    download(&w, spanish(), &["es-419_tq"], 80).await;
    assert_eq!(
        (changes(&w), commands(&w)),
        (changes_before, commands_before),
        "the Spanish questions belong to no set of this project"
    );

    download(&w, english.clone(), &["en_ust"], 81).await;
    handle(&w).wait_idle();
    assert_eq!(commands(&w) - commands_before, 1, "one batch");
    let after = pins_of(&w);
    let sha = w
        .fake
        .resolve("unfoldingWord", "en_ust", "v1")
        .await
        .unwrap();
    for rung in [tc_resources::Rung::Primary, tc_resources::Rung::Fallback] {
        let (was, now) = (before.set(rung).unwrap(), after.set(rung).unwrap());
        let adopted = now.pin(SetSlot::SimplifiedText).unwrap();
        assert_eq!(adopted.sha.to_hex(), sha);
        assert_eq!(
            adopted.version.as_ref().map(ToString::to_string).as_deref(),
            Some("v1")
        );
        for (slot, pin) in &was.pins {
            assert_eq!(now.pin(*slot), Some(pin), "{rung:?} {slot:?} is as it was");
        }
    }
    // Downloading it again changes nothing.
    download(&w, english, &["en_ust"], 82).await;
    assert_eq!(commands(&w) - commands_before, 1);
}

// ---- records in conflict, and decisions that cannot move ---------------------------------------

/// What the dialog said about the notes of Titus, and what the project holds of them afterwards:
/// how many decisions record `new` and how many still record `old`.
struct Outcome {
    carried: u32,
    check_again: u32,
    on_new: usize,
    on_old: usize,
}

async fn upgrade_titus(w: &World, id: u8) -> Outcome {
    let view = plan_set(w, SetRung::Primary, id).await;
    let carry = view.rows[0].carry.unwrap();
    assert_eq!(
        ok(&w.t.app, &confirm(w, view.plan, id + 1)).await,
        PlanApplied::Applied
    );
    handle(w).wait_idle();
    let new = pins_of(w)
        .primary
        .unwrap()
        .pin(SetSlot::TranslationNotes)
        .unwrap()
        .resource();
    let held = |expected: &tc_check::ResourceRef| {
        w.decided
            .iter()
            .filter(|(key, _)| resource_of(w, key) == *expected)
            .count()
    };
    Outcome {
        carried: carry.carried,
        check_again: carry.check_again,
        on_new: held(&new),
        on_old: held(&w.tn.resource()),
    }
}

// Ruling H5: two versions of a decision settle on the newest with no question, so the upgrade
// carries the newest over, and the other stays in History.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_upgrade_carries_the_newest_of_two_decisions_and_keeps_the_other_in_history() {
    let w = build(
        6,
        0,
        None,
        Opts {
            trouble: Trouble::ConflictedDecision,
            ..Opts::default()
        },
    )
    .await;
    let key = RecordKey::Check(w.decided[1].0.clone());
    let heads_before = heads(&w, &key);
    assert_eq!(heads_before.len(), 2);
    assert_eq!(handle(&w).snapshot().fold().conflict_count(), 0);

    let done = upgrade_titus(&w, 50).await;

    assert_eq!((done.carried, done.check_again), (6, 0));
    let snapshot = handle(&w).snapshot();
    assert_eq!(snapshot.fold().conflict_count(), 0);
    let history = snapshot.fold().history(&key);
    for head in &heads_before {
        assert!(history.contains(head), "{head:?} is in History");
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn two_decisions_that_come_to_one_check_are_counted_as_checks_to_do_again() {
    let w = build(
        6,
        0,
        None,
        Opts {
            trouble: Trouble::TwoOnOneCheck,
            ..Opts::default()
        },
    )
    .await;
    assert_eq!(w.decided.len(), 7);

    let done = upgrade_titus(&w, 50).await;

    // The dialog's figures are what the project holds afterwards: five decisions on v91, and
    // the two that could not move still on v86, kept as they were.
    assert_eq!((done.carried, done.check_again), (5, 2));
    assert_eq!((done.on_new, done.on_old), (5, 2));
    for (key, was) in &w.decided[..2] {
        assert_eq!(resource_of(&w, key), was.resource);
    }
}

// Ruling H5: two versions of the pins settle on the newest, so an upgrade starts from it and
// replaces both; the other stays in History.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_upgrade_over_two_pin_versions_starts_from_the_newest() {
    let w = build(
        6,
        0,
        None,
        Opts {
            trouble: Trouble::ConflictedPins,
            ..Opts::default()
        },
    )
    .await;
    let key = RecordKey::Pins(PinGroup::Primary);
    let heads_before = heads(&w, &key);
    assert_eq!(heads_before.len(), 2);

    upgrade_titus(&w, 50).await;

    assert_eq!(heads(&w, &key).len(), 1);
    let history = handle(&w).snapshot().fold().history(&key);
    for head in &heads_before {
        assert!(history.contains(head), "{head:?} is in History");
    }
}

// Ruling H5: the pins' two versions settle on the newest, so a download adopts over it; the
// other stays in History.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_download_adopts_pins_over_two_versions_and_keeps_the_other_in_history() {
    let w = build(
        6,
        0,
        None,
        Opts {
            trouble: Trouble::ConflictedPins,
            ..Opts::default()
        },
    )
    .await;
    add_repo(&w.fake, "unfoldingWord", "en_ust", "en_ult@v89").await;
    let key = RecordKey::Pins(PinGroup::Primary);
    let (heads_before, changes_before) = (heads(&w, &key), changes(&w));

    let job = ok(
        &w.t.app,
        &tc_api::resources::Download {
            request: rid(80),
            at: w.at.clone(),
            gateway: tc_api::resources::GatewayRef {
                language: "en".into(),
                owner: "unfoldingWord".into(),
            },
            book: BookId::Titus,
            repos: vec!["en_ust".to_owned()],
        },
    )
    .await;
    match ended(&w.t.app, job).await {
        JobState::Failed(error) => panic!("{error:?}"),
        _ => {}
    }
    handle(&w).wait_idle();
    assert_eq!(heads(&w, &key).len(), 1);
    assert!(changes(&w) > changes_before);
    let history = handle(&w).snapshot().fold().history(&key);
    for head in &heads_before {
        assert!(history.contains(head), "{head:?} is in History");
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_upgrade_over_pins_with_two_heads_that_agree_keeps_both_teammates_changes() {
    let w = build(
        6,
        0,
        None,
        Opts {
            trouble: Trouble::MergedPins,
            ..Opts::default()
        },
    )
    .await;
    let key = RecordKey::Pins(PinGroup::Primary);
    assert_eq!(heads(&w, &key).len(), 2);

    let done = upgrade_titus(&w, 50).await;

    assert_eq!((done.carried, done.check_again), (6, 0));
    assert_eq!(heads(&w, &key).len(), 1, "the change is over both heads");
    let primary = pins_of(&w).primary.unwrap();
    assert!(
        primary.pin(SetSlot::TranslationWordsLinks).is_none(),
        "the teammate's removal stands"
    );
    assert_eq!(
        primary
            .pin(SetSlot::TranslationNotes)
            .unwrap()
            .version
            .as_ref()
            .map(ToString::to_string)
            .as_deref(),
        Some("v91")
    );
}

/// tC4 #833.1: a check for updates while Door43 cannot be reached answers one plain sentence,
/// never the client's own error text.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_offline_update_check_shows_one_plain_sentence() {
    use tc_api::Refusal;
    use tc_api::refusal::{ResourceRefusal, UpdateCheckFailure};
    let w = build(
        1,
        0,
        None,
        Opts {
            dead_door43: true,
            ..Opts::default()
        },
    )
    .await;
    let error = super::common::post(&w.t.app, &CheckUpdates { at: w.at.clone() })
        .await
        .unwrap_err();
    assert_eq!(
        error,
        ApiError::Refused(Refusal::Resources(ResourceRefusal::UpdateCheckFailed {
            why: UpdateCheckFailure::Unreachable,
        }))
    );
}
