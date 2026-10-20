//! Release upgrades, the gateway change and the guided fix (unit C8), through `App::request` on
//! a project written by tc-store, with a fake Door43 serving the releases (journeys J3, J12 and
//! J13 of tC4's `docs/JOURNEYS.md`). The project pins the English notes and links at v86 and
//! holds notes decisions made against v86's Titus; v91 is the release on offer.

use std::collections::BTreeMap;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use tc_api::ApiError;
use tc_api::check::{CheckUnit, Tool};
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
use tc_check::{CheckItem, CheckKey, DecisionRecord, ToolId};
use tc_core::BookId;
use tc_door43_fake::FakeDoor43;
use tc_helps::tsv::{TsvScope, parse_notes};
use tc_journal::command::{PlannedChange, UpgradePlan};
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
                    tc_core::VerseSpan::single(verse.as_str().parse().ok()?),
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

struct World {
    _dir: TempDir,
    t: TestApp,
    fake: FakeDoor43,
    fs: FaultFs,
    at: Scope,
    tn: ResourcePin,
    twl: ResourcePin,
    decided: Vec<(CheckKey, DecisionRecord)>,
}

/// The sample's Titus on tc-store's in-memory file system with the English notes and links
/// pinned at v86 on both rungs and `decided` notes decisions, an app over it, and a fake Door43
/// that serves v86 and v91.
async fn world(decided: usize, orphans: usize) -> World {
    let fake = FakeDoor43::start_seeded().await.unwrap();
    let tn = release(&fake, "en_tn", "v86").await;
    let twl = release(&fake, "en_twl", "v86").await;
    let titus = fixture("tc4/sample-burrito/ingredients/TIT.usfm");
    let text = String::from_utf8(titus.clone()).unwrap();
    let mut decided = decisions(&tn, &text, decided);
    for n in 0..orphans {
        let made = orphan(&decided[0], &format!("zz{n}z"));
        decided.push(made);
    }
    let mut records = sample_records(&BTreeMap::from([(BookId::Titus, titus)]), None);
    let pins = ProjectPins {
        primary: Some(english(&tn, &twl)),
        fallback: Some(english(&tn, &twl)),
        ..ProjectPins::default()
    };
    for group in [PinGroup::Primary, PinGroup::Fallback] {
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
    let backend = backend_over(&fs, APP, FakePlatform::default(), Arc::default());
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
    let dir = tempfile::tempdir().unwrap();
    let projects = dir.path().join("projects");
    std::fs::create_dir_all(&projects).unwrap();
    let url = fake.url().to_owned();
    let t = TestApp::start_with(dir.path(), &Parts::new(), |config, deps| {
        config.projects = ProjectsFolder::Given(projects.clone());
        config.door43 = url.parse().unwrap();
        deps.store = StoreChoice::Given(Arc::new(backend));
    })
    .await;
    let at = open(&t, PROJECT).await;
    internet(&t, true, 1).await;
    World {
        _dir: dir,
        t,
        fake,
        fs,
        at,
        tn,
        twl,
        decided,
    }
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
    .expect("the job ends")
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

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn smoke_upgrade_a_set() {
    let w = world(12, 2).await;
    let updates = ok(&w.t.app, &CheckUpdates { at: w.at.clone() }).await;
    println!("{updates:#?}");
    let view = plan_set(&w, SetRung::Primary, 50).await;
    println!("{view:#?}");
    let applied = ok(
        &w.t.app,
        &ApplyPlan {
            request: rid(51),
            at: w.at.clone(),
            plan: view.plan,
        },
    )
    .await;
    assert_eq!(applied, PlanApplied::Applied);
    println!(
        "{:#?}",
        pins_of(&w).primary.unwrap().pin(SetSlot::TranslationNotes)
    );
    for (key, _) in &w.decided {
        println!("{} {:?}", key.check_id, resource_of(&w, key).sha.to_hex());
    }
    let _ = (
        &w.fake,
        &w.fs,
        &w.tn,
        &w.twl,
        PlanId::new(0),
        NotSaved::PlanStale,
        PlanWhat::Gateway as fn(_) -> _,
        Tool::TranslationNotes,
        CheckUnit::Stories,
        ToolId::TranslationNotes,
        StoreCommand::apply_upgrade as fn(_) -> _,
        UpgradePlan { changes: vec![] },
        PlannedChange {
            record: Record::Pins {
                group: PinGroup::Primary,
                value: Default::default(),
            },
            parents: vec![],
            cause: Cause::Edit,
        },
    );
    let _: Option<&dyn ProjectHandle> = None;
    let _ = err::<CheckUpdates>;
}
