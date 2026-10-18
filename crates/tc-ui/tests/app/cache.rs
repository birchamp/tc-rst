//! The query cache's bookkeeping (APP-DESIGN 6.2, 6.3): which reads a change makes stale, and
//! which replies are shown. The browser glue only does what the registry says, so these
//! tests are the cache's tests.

use proptest::prelude::*;
use tc_api::check::{CheckListRef, CheckUnit, Tool};
use tc_api::event::{Aspect, ChangeParts, ChangeScope, ChapterRef};
use tc_api::ids::{Epoch, JobId, ProjectRef, Scope, ViewSeq};
use tc_api::jobs::Get;
use tc_api::query::QueryKey;
use tc_core::BookId;
use tc_ui::app::cache::{
    AppTopic, Held, Landed, Landing, Outcome, QueryId, ReadOf, Registry, Restarted, Start, Ticket,
};

fn scope(folder: &str, epoch: u64) -> Scope {
    Scope {
        project: ProjectRef::parse(folder).unwrap(),
        epoch: Epoch::new(epoch),
    }
}

fn id(name: &str) -> QueryId {
    QueryId {
        path: "test/read",
        body: name.to_owned(),
    }
}

fn chapter(book: BookId, chapter: u16) -> QueryKey {
    QueryKey::Chapter(ChapterRef { book, chapter })
}

fn project(at: &Scope, keys: Vec<QueryKey>) -> ReadOf {
    ReadOf::Project {
        at: at.clone(),
        keys,
    }
}

fn chapters_changed(book: BookId, chapter: u16) -> ChangeScope {
    ChangeScope::Parts(ChangeParts {
        chapters: vec![ChapterRef { book, chapter }],
        ..ChangeParts::default()
    })
}

/// Holds a read and returns the ticket of its first fetch.
fn hold(registry: &mut Registry, id: &QueryId, of: ReadOf) -> Ticket {
    match registry.hold(id, of) {
        Held::Fetch(ticket) => ticket,
        other => panic!("{other:?}"),
    }
}

fn value(registry: &mut Registry, id: &QueryId, ticket: Ticket) -> Landing {
    registry.landed(id, ticket, Landed::Value(None))
}

/// The ids of some starts, in a fixed order.
fn ids(starts: &[Start]) -> Vec<QueryId> {
    let mut ids: Vec<_> = starts.iter().map(|s| s.id.clone()).collect();
    ids.sort_by(|a, b| a.body.cmp(&b.body));
    ids
}

fn names(list: &[&str]) -> Vec<QueryId> {
    list.iter().map(|name| id(name)).collect()
}

#[test]
fn query_ids_are_the_endpoint_and_the_request() {
    let a = QueryId::of(&Get { job: JobId::new(4) }).unwrap();
    let again = QueryId::of(&Get { job: JobId::new(4) }).unwrap();
    let other = QueryId::of(&Get { job: JobId::new(5) }).unwrap();
    assert_eq!(a, again);
    assert_ne!(a, other);
    assert_eq!(a.path, "jobs/get");
    assert_eq!(a.body, r#"{"job":4}"#);
}

#[test]
fn two_holders_of_one_read_share_one_fetch() {
    let mut registry = Registry::new();
    let at = scope("TIT", 1);
    let read = id("a");
    let first = registry.hold(&read, project(&at, vec![chapter(BookId::Titus, 1)]));
    assert!(matches!(first, Held::Fetch(_)));
    let second = registry.hold(&read, project(&at, vec![chapter(BookId::Titus, 1)]));
    assert_eq!(second, Held::Shared);
    assert_eq!(registry.len(), 1);
    assert!(!registry.release(&read), "one holder is left");
    assert!(registry.release(&read), "the last release forgets the read");
    assert!(registry.is_empty());
}

#[test]
fn a_change_refetches_only_the_reads_that_depend_on_it() {
    let mut registry = Registry::new();
    let tit = scope("TIT", 1);
    let psa = scope("PSA", 1);
    let reads = [
        ("tit1", project(&tit, vec![chapter(BookId::Titus, 1)])),
        (
            "titbook",
            project(&tit, vec![QueryKey::Book(BookId::Titus)]),
        ),
        ("tit2", project(&tit, vec![chapter(BookId::Titus, 2)])),
        (
            "psabook",
            project(&psa, vec![QueryKey::Book(BookId::Psalms)]),
        ),
        ("psa1", project(&psa, vec![chapter(BookId::Titus, 1)])),
    ];
    for (name, of) in reads {
        let id = id(name);
        let ticket = hold(&mut registry, &id, of);
        value(&mut registry, &id, ticket);
    }
    // Titus 1 changed in the Titus open: its chapter and its book, nothing else. A read of
    // another folder that happens to name Titus 1 is not in this open.
    let starts = registry.changed(&tit, &chapters_changed(BookId::Titus, 1));
    assert_eq!(ids(&starts), names(&["tit1", "titbook"]));
    // And nothing changed gives nothing.
    for start in starts {
        value(&mut registry, &start.id, start.ticket);
    }
    assert!(
        registry
            .changed(&tit, &ChangeScope::Parts(ChangeParts::default()))
            .is_empty()
    );
    // An aspect of the project reaches only reads that name it.
    assert!(
        registry
            .changed(
                &tit,
                &ChangeScope::Parts(ChangeParts {
                    aspects: vec![Aspect::Pins],
                    ..ChangeParts::default()
                })
            )
            .is_empty()
    );
}

#[test]
fn a_change_in_one_open_does_not_reach_a_read_of_another_epoch() {
    let mut registry = Registry::new();
    let old = scope("TIT", 1);
    let new = scope("TIT", 2);
    let read = id("new");
    let ticket = hold(
        &mut registry,
        &read,
        project(&new, vec![QueryKey::Book(BookId::Titus)]),
    );
    value(&mut registry, &read, ticket);
    // A late change of the closed open, delivered after the new one began.
    assert!(
        registry
            .changed(&old, &chapters_changed(BookId::Titus, 1))
            .is_empty()
    );
    assert_eq!(
        ids(&registry.changed(&new, &chapters_changed(BookId::Titus, 1))),
        vec![read]
    );
}

#[test]
fn everything_refetches_every_read_of_that_open_only() {
    let mut registry = Registry::new();
    let tit = scope("TIT", 1);
    let psa = scope("PSA", 1);
    for (name, at) in [("a", &tit), ("b", &tit), ("c", &psa)] {
        let read = id(name);
        let ticket = hold(
            &mut registry,
            &read,
            project(at, vec![QueryKey::Aspect(Aspect::Meta)]),
        );
        value(&mut registry, &read, ticket);
    }
    let home = id("home");
    let ticket = hold(&mut registry, &home, ReadOf::App(vec![AppTopic::Projects]));
    value(&mut registry, &home, ticket);
    assert_eq!(
        ids(&registry.changed(&tit, &ChangeScope::Everything)),
        names(&["a", "b"])
    );
}

#[test]
fn a_change_during_a_fetch_fetches_again_after_it_lands() {
    let mut registry = Registry::new();
    let at = scope("TIT", 1);
    let read = id("a");
    let first = hold(
        &mut registry,
        &read,
        project(&at, vec![chapter(BookId::Titus, 1)]),
    );
    // The change arrives while the first fetch runs: no second fetch at once, because the
    // reply of the first may have been computed before the change.
    assert!(
        registry
            .changed(&at, &chapters_changed(BookId::Titus, 1))
            .is_empty()
    );
    let landing = value(&mut registry, &read, first);
    assert_eq!(landing.outcome, Outcome::Apply);
    let second = landing.then.expect("a fetch after the change");
    assert_ne!(second, first);
    let landing = value(&mut registry, &read, second);
    assert_eq!(landing.outcome, Outcome::Apply);
    assert_eq!(landing.then, None);
}

#[test]
fn many_changes_during_one_fetch_cost_one_more_fetch() {
    let mut registry = Registry::new();
    let at = scope("TIT", 1);
    let read = id("a");
    let first = hold(
        &mut registry,
        &read,
        project(&at, vec![QueryKey::Book(BookId::Titus)]),
    );
    for chapter in 1..=40 {
        assert!(
            registry
                .changed(&at, &chapters_changed(BookId::Titus, chapter))
                .is_empty()
        );
    }
    assert!(registry.resync().is_empty());
    let second = value(&mut registry, &read, first).then.unwrap();
    let after = value(&mut registry, &read, second);
    assert_eq!(after.then, None, "the 41 changes cost one fetch, not 41");
}

#[test]
fn a_late_reply_of_an_earlier_attempt_does_nothing() {
    let mut registry = Registry::new();
    let at = scope("TIT", 1);
    let read = id("a");
    let first = hold(
        &mut registry,
        &read,
        project(&at, vec![QueryKey::Book(BookId::Titus)]),
    );
    registry.changed(&at, &chapters_changed(BookId::Titus, 1));
    let second = value(&mut registry, &read, first).then.unwrap();
    // The first attempt's reply arrives again (a duplicate), after the second began.
    let landing = value(&mut registry, &read, first);
    assert_eq!((landing.outcome, landing.then), (Outcome::Ignore, None));
    assert_eq!(value(&mut registry, &read, second).outcome, Outcome::Apply);
}

#[test]
fn resync_refetches_every_live_read() {
    let mut registry = Registry::new();
    let at = scope("TIT", 1);
    let ended = scope("PSA", 1);
    let reads = [
        ("project", project(&at, vec![chapter(BookId::Titus, 1)])),
        (
            "other",
            project(&at, vec![QueryKey::Aspect(Aspect::Settings)]),
        ),
        (
            "ended",
            project(&ended, vec![QueryKey::Book(BookId::Psalms)]),
        ),
        ("home", ReadOf::App(vec![AppTopic::Projects])),
    ];
    for (name, of) in reads {
        let read = id(name);
        let ticket = hold(&mut registry, &read, of);
        value(&mut registry, &read, ticket);
    }
    registry.closed(&ended);
    assert_eq!(
        ids(&registry.resync()),
        names(&["home", "other", "project"]),
        "an ended open's read is not fetched again"
    );
}

#[test]
fn topics_refetch_their_reads() {
    let mut registry = Registry::new();
    let p = ProjectRef::parse("TIT").unwrap();
    let q = ProjectRef::parse("PSA").unwrap();
    let reads = [
        ("list", ReadOf::App(vec![AppTopic::Projects])),
        ("card-p", ReadOf::App(vec![AppTopic::HomeCard(p.clone())])),
        ("card-q", ReadOf::App(vec![AppTopic::HomeCard(q.clone())])),
        (
            "both",
            ReadOf::App(vec![AppTopic::Projects, AppTopic::HomeCard(q.clone())]),
        ),
        (
            "project",
            project(&scope("TIT", 1), vec![QueryKey::Aspect(Aspect::Meta)]),
        ),
    ];
    for (name, of) in reads {
        let read = id(name);
        let ticket = hold(&mut registry, &read, of);
        value(&mut registry, &read, ticket);
    }
    assert_eq!(
        ids(&registry.topic(&AppTopic::Projects)),
        names(&["both", "list"])
    );
    // Start the cards' next fetches to completion, then ask for p's card alone.
    let starts = registry.topic(&AppTopic::HomeCard(p));
    assert_eq!(ids(&starts), names(&["card-p"]));
}

#[test]
fn a_reply_for_a_closed_open_is_dropped() {
    let mut registry = Registry::new();
    let at = scope("TIT", 1);
    let read = id("a");
    let ticket = hold(
        &mut registry,
        &read,
        project(&at, vec![chapter(BookId::Titus, 1)]),
    );
    assert_eq!(registry.closed(&at), vec![read.clone()]);
    assert!(!registry.is_current(&at));
    // The reply that was in flight is dropped, not shown.
    let landing = value(&mut registry, &read, ticket);
    assert_eq!((landing.outcome, landing.then), (Outcome::Drop, None));
    // Nothing fetches it again, whatever happens.
    assert!(registry.changed(&at, &ChangeScope::Everything).is_empty());
    assert!(registry.resync().is_empty());
    assert_eq!(registry.refresh(&read), None);
    // Closing again says nothing new.
    assert!(registry.closed(&at).is_empty());
}

#[test]
fn a_read_of_an_ended_open_is_gone_when_held() {
    let mut registry = Registry::new();
    let at = scope("TIT", 1);
    registry.closed(&at);
    let read = id("a");
    assert_eq!(
        registry.hold(&read, project(&at, vec![QueryKey::Book(BookId::Titus)])),
        Held::Gone
    );
    assert!(registry.resync().is_empty());
    assert!(registry.release(&read));
}

#[test]
fn a_newer_epoch_forgets_the_reads_of_the_older_one() {
    let mut registry = Registry::new();
    let old = scope("TIT", 1);
    let other_folder = scope("PSA", 1);
    let ticket_old = {
        let read = id("old");
        let ticket = hold(
            &mut registry,
            &read,
            project(&old, vec![QueryKey::Book(BookId::Titus)]),
        );
        let read2 = id("psa");
        let t2 = hold(
            &mut registry,
            &read2,
            project(&other_folder, vec![QueryKey::Book(BookId::Psalms)]),
        );
        value(&mut registry, &read2, t2);
        ticket
    };
    let ended = registry.opened(scope("TIT", 2));
    assert_eq!(ended, vec![id("old")], "only the older open of that folder");
    assert!(!registry.is_current(&old));
    assert!(registry.is_current(&scope("TIT", 2)));
    assert!(registry.is_current(&other_folder));
    let landing = value(&mut registry, &id("old"), ticket_old);
    assert_eq!(landing.outcome, Outcome::Drop);
    assert!(registry.changed(&old, &ChangeScope::Everything).is_empty());
    // The same epoch announced again ends nothing.
    assert!(registry.opened(scope("TIT", 2)).is_empty());
    // An older epoch announced late does not become the newest.
    assert!(registry.opened(scope("TIT", 1)).is_empty());
    assert!(!registry.is_current(&old));
}

#[test]
fn an_open_older_than_the_newest_is_not_current_even_with_no_read_of_it() {
    let mut registry = Registry::new();
    assert!(registry.opened(scope("TIT", 2)).is_empty());
    assert!(!registry.is_current(&scope("TIT", 1)));
    assert!(registry.is_current(&scope("TIT", 2)));
    // An open newer than any announced one is current: the registry may simply not have been
    // told yet.
    assert!(registry.is_current(&scope("TIT", 3)));
    assert!(registry.is_current(&scope("PSA", 1)));
    // A read of the older open is gone as soon as it is held.
    let read = id("a");
    let keys = vec![QueryKey::Book(BookId::Titus)];
    assert_eq!(
        registry.hold(&read, project(&scope("TIT", 1), keys)),
        Held::Gone
    );
}

#[test]
fn an_open_nobody_announced_still_gets_its_replies() {
    let mut registry = Registry::new();
    let at = scope("TIT", 7);
    let read = id("a");
    let ticket = hold(
        &mut registry,
        &read,
        project(&at, vec![QueryKey::Book(BookId::Titus)]),
    );
    assert!(registry.is_current(&at));
    assert_eq!(value(&mut registry, &read, ticket).outcome, Outcome::Apply);
}

#[test]
fn a_stale_scope_reply_is_dropped_and_kills_the_scope() {
    let mut registry = Registry::new();
    let at = scope("TIT", 1);
    let keys = vec![QueryKey::Book(BookId::Titus)];
    let (a, b) = (id("a"), id("b"));
    let ta = hold(&mut registry, &a, project(&at, keys.clone()));
    let tb = hold(&mut registry, &b, project(&at, keys));
    // The server says the open is gone before any event did.
    let landing = registry.landed(&a, ta, Landed::StaleScope);
    assert_eq!(landing.outcome, Outcome::Drop);
    assert_eq!(landing.died, vec![b.clone()], "the other read of that open");
    assert!(!registry.is_current(&at));
    assert!(registry.changed(&at, &ChangeScope::Everything).is_empty());
    assert_eq!(value(&mut registry, &b, tb).outcome, Outcome::Drop);
}

#[test]
fn a_stale_scope_for_a_read_of_the_app_is_an_error_to_show() {
    let mut registry = Registry::new();
    let read = id("home");
    let ticket = hold(&mut registry, &read, ReadOf::App(vec![AppTopic::Projects]));
    let landing = registry.landed(&read, ticket, Landed::StaleScope);
    assert_eq!(landing.outcome, Outcome::Apply);
}

#[test]
fn an_older_view_never_replaces_a_newer_one() {
    let mut registry = Registry::new();
    let at = scope("TIT", 1);
    let read = id("a");
    let view = |n| Landed::Value(Some(ViewSeq::new(n)));
    let t1 = hold(
        &mut registry,
        &read,
        project(&at, vec![QueryKey::Book(BookId::Titus)]),
    );
    assert_eq!(registry.landed(&read, t1, view(5)).outcome, Outcome::Apply);
    let t2 = registry.refresh(&read).unwrap();
    assert_eq!(registry.landed(&read, t2, view(3)).outcome, Outcome::Ignore);
    let t3 = registry.refresh(&read).unwrap();
    assert_eq!(registry.landed(&read, t3, view(5)).outcome, Outcome::Apply);
    let t4 = registry.refresh(&read).unwrap();
    assert_eq!(registry.landed(&read, t4, view(6)).outcome, Outcome::Apply);
    // A reply with no snapshot number does not lower the one shown.
    let t5 = registry.refresh(&read).unwrap();
    assert_eq!(
        registry.landed(&read, t5, Landed::Value(None)).outcome,
        Outcome::Apply
    );
    let t6 = registry.refresh(&read).unwrap();
    assert_eq!(registry.landed(&read, t6, view(5)).outcome, Outcome::Ignore);
    // An ignored reply still fetches again if a change came meanwhile.
    let t7 = registry.refresh(&read).unwrap();
    registry.changed(&at, &ChangeScope::Everything);
    let landing = registry.landed(&read, t7, view(1));
    assert_eq!(landing.outcome, Outcome::Ignore);
    assert!(landing.then.is_some());
}

#[test]
fn an_error_is_shown_and_a_retry_fetches_again() {
    let mut registry = Registry::new();
    let read = id("home");
    let ticket = hold(&mut registry, &read, ReadOf::App(vec![AppTopic::Projects]));
    let landing = registry.landed(&read, ticket, Landed::Error);
    assert_eq!((landing.outcome, landing.then), (Outcome::Apply, None));
    let retry = registry
        .refresh(&read)
        .expect("an idle read starts a fetch");
    // A retry while it runs starts nothing more, and is followed by one fetch.
    assert_eq!(registry.refresh(&read), None);
    assert!(value(&mut registry, &read, retry).then.is_some());
}

#[test]
fn the_last_release_forgets_the_read_and_its_late_reply() {
    let mut registry = Registry::new();
    let at = scope("TIT", 1);
    let read = id("a");
    let old = hold(
        &mut registry,
        &read,
        project(&at, vec![QueryKey::Book(BookId::Titus)]),
    );
    assert!(registry.release(&read));
    assert_eq!(value(&mut registry, &read, old).outcome, Outcome::Ignore);
    // The same read is held again while the old reply is still on its way: the old reply is
    // not the new read's attempt.
    let new = hold(
        &mut registry,
        &read,
        project(&at, vec![QueryKey::Book(BookId::Titus)]),
    );
    assert_ne!(old, new);
    assert_eq!(value(&mut registry, &read, old).outcome, Outcome::Ignore);
    assert_eq!(value(&mut registry, &read, new).outcome, Outcome::Apply);
    // Releasing what is not held says nothing is forgotten.
    assert!(registry.release(&read));
    assert!(!registry.release(&read));
}

#[test]
fn restart_forgets_every_read_and_open() {
    let mut registry = Registry::new();
    let at = scope("TIT", 1);
    let (project_read, home) = (id("project"), id("home"));
    let keys = vec![QueryKey::Book(BookId::Titus)];
    let tp = hold(&mut registry, &project_read, project(&at, keys.clone()));
    value(&mut registry, &project_read, tp);
    let th = hold(&mut registry, &home, ReadOf::App(vec![AppTopic::Projects]));
    value(&mut registry, &home, th);

    let Restarted { dead, starts } = registry.restarted();
    assert_eq!(dead, vec![project_read.clone()]);
    assert_eq!(ids(&starts), vec![home.clone()], "Home reads the new run");

    // The old run's read stays gone even though the new run counts epochs from 1 again.
    assert!(registry.is_current(&at));
    assert!(registry.changed(&at, &ChangeScope::Everything).is_empty());
    assert!(registry.resync().iter().all(|s| s.id != project_read));
    assert_eq!(registry.refresh(&project_read), None);
    // The new run's first open of the same folder is a new read.
    assert!(matches!(
        registry.hold(&id("fresh"), project(&at, keys)),
        Held::Fetch(_)
    ));
}

#[test]
fn a_reply_in_flight_at_a_restart_does_not_count() {
    let mut registry = Registry::new();
    let at = scope("TIT", 1);
    let (project_read, home) = (id("project"), id("home"));
    let tp = hold(
        &mut registry,
        &project_read,
        project(&at, vec![QueryKey::Book(BookId::Titus)]),
    );
    let th = hold(&mut registry, &home, ReadOf::App(vec![AppTopic::Projects]));
    // Both fetches are running when the server starts again.
    let Restarted { dead, starts } = registry.restarted();
    assert_eq!(dead, vec![project_read.clone()]);
    assert!(
        starts.is_empty(),
        "a read that is fetching is not started twice"
    );
    // The project read's reply is of the old run: dropped.
    let landing = value(&mut registry, &project_read, tp);
    assert_eq!((landing.outcome, landing.then), (Outcome::Drop, None));
    // The app read's reply may be the old server's, so it is shown and then fetched again.
    let landing = value(&mut registry, &home, th);
    assert_eq!(landing.outcome, Outcome::Apply);
    assert!(landing.then.is_some());
}

// --- The property: no change is hidden behind an earlier reply ------------------------------

/// Four reads over two opens and the app, as a screen would hold them.
fn universe() -> Vec<(QueryId, ReadOf)> {
    let tit = scope("TIT", 1);
    let psa = scope("PSA", 1);
    vec![
        (
            id("tit-chapter"),
            project(&tit, vec![chapter(BookId::Titus, 1)]),
        ),
        (
            id("tit-book"),
            project(&tit, vec![QueryKey::Book(BookId::Titus)]),
        ),
        (
            id("psa-checks"),
            project(
                &psa,
                vec![QueryKey::CheckList(CheckListRef {
                    tool: Tool::TranslationNotes,
                    unit: CheckUnit::Book(BookId::Psalms),
                })],
            ),
        ),
        (id("home"), ReadOf::App(vec![AppTopic::Projects])),
    ]
}

/// What a change does to the universe: the open it is in and its parts.
fn changes() -> Vec<(Scope, ChangeScope)> {
    let tit = scope("TIT", 1);
    let psa = scope("PSA", 1);
    vec![
        (tit.clone(), chapters_changed(BookId::Titus, 1)),
        (tit.clone(), chapters_changed(BookId::Titus, 2)),
        (tit, ChangeScope::Everything),
        (psa.clone(), chapters_changed(BookId::Psalms, 3)),
        (
            psa,
            ChangeScope::Parts(ChangeParts {
                books: vec![BookId::Psalms],
                ..ChangeParts::default()
            }),
        ),
    ]
}

#[derive(Debug, Clone)]
enum Op {
    Hold(usize),
    Release(usize),
    Change(usize),
    Topic,
    Resync,
    /// Land the outstanding fetch at this position (modulo how many there are).
    Land(usize),
}

fn op() -> impl Strategy<Value = Op> {
    prop_oneof![
        3 => (0usize..4).prop_map(Op::Hold),
        2 => (0usize..4).prop_map(Op::Release),
        4 => (0usize..5).prop_map(Op::Change),
        1 => Just(Op::Topic),
        1 => Just(Op::Resync),
        6 => any::<usize>().prop_map(Op::Land),
    ]
}

struct Outstanding {
    read: usize,
    ticket: Ticket,
}

/// A second account of the same events, kept without looking inside the registry.
struct Model {
    reads: Vec<(QueryId, ReadOf)>,
    held: Vec<usize>,
    /// The attempt the registry has running for each read.
    running: Vec<Option<Ticket>>,
    /// The order number at which each held read's latest fetch started, and the latest
    /// change or resync that touched it.
    last_start: Vec<Option<u64>>,
    last_touch: Vec<Option<u64>>,
    /// The start order of the fetch whose value was last shown.
    shown_start: Vec<Option<u64>>,
    started_at: std::collections::HashMap<(usize, Ticket), u64>,
    outstanding: Vec<Outstanding>,
    clock: u64,
}

impl Model {
    fn new() -> Self {
        let reads = universe();
        let n = reads.len();
        Model {
            reads,
            held: vec![0; n],
            running: vec![None; n],
            last_start: vec![None; n],
            last_touch: vec![None; n],
            shown_start: vec![None; n],
            started_at: Default::default(),
            outstanding: Vec::new(),
            clock: 0,
        }
    }

    fn tick(&mut self) -> u64 {
        self.clock += 1;
        self.clock
    }

    fn index(&self, id: &QueryId) -> usize {
        self.reads.iter().position(|(read, _)| read == id).unwrap()
    }

    fn began(&mut self, read: usize, ticket: Ticket) -> Result<(), TestCaseError> {
        prop_assert!(self.held[read] > 0, "a fetch for a read nobody holds");
        prop_assert!(
            self.running[read].is_none(),
            "a second fetch while one runs"
        );
        let at = self.tick();
        self.running[read] = Some(ticket);
        self.last_start[read] = Some(at);
        self.started_at.insert((read, ticket), at);
        self.outstanding.push(Outstanding { read, ticket });
        Ok(())
    }

    fn started(&mut self, starts: Vec<Start>) -> Result<(), TestCaseError> {
        for start in starts {
            let read = self.index(&start.id);
            self.began(read, start.ticket)?;
        }
        Ok(())
    }

    /// Marks every held read for which `touches` says yes as touched now.
    fn touch(&mut self, touches: impl Fn(&ReadOf) -> bool) {
        let at = self.tick();
        for read in 0..self.reads.len() {
            if self.held[read] > 0 && touches(&self.reads[read].1) {
                self.last_touch[read] = Some(at);
            }
        }
    }

    fn land(&mut self, registry: &mut Registry, which: usize) -> Result<(), TestCaseError> {
        if self.outstanding.is_empty() {
            return Ok(());
        }
        let Outstanding { read, ticket } = self.outstanding.remove(which % self.outstanding.len());
        let id = self.reads[read].0.clone();
        let landing = registry.landed(&id, ticket, Landed::Value(None));
        let current = self.held[read] > 0 && self.running[read] == Some(ticket);
        if current {
            prop_assert_eq!(landing.outcome, Outcome::Apply);
            self.running[read] = None;
            self.shown_start[read] = self.started_at.get(&(read, ticket)).copied();
            if let Some(next) = landing.then {
                self.began(read, next)?;
            }
        } else {
            prop_assert_eq!(
                landing.outcome,
                Outcome::Ignore,
                "a reply that is not current"
            );
            prop_assert_eq!(landing.then, None);
        }
        Ok(())
    }
}

fn run(ops: Vec<Op>) -> Result<(), TestCaseError> {
    let mut registry = Registry::new();
    let mut model = Model::new();
    let changes = changes();
    for op in ops {
        match op {
            Op::Hold(read) => {
                let (id, of) = model.reads[read].clone();
                match registry.hold(&id, of) {
                    Held::Fetch(ticket) => {
                        prop_assert_eq!(model.held[read], 0);
                        model.held[read] = 1;
                        model.last_touch[read] = None;
                        model.shown_start[read] = None;
                        model.began(read, ticket)?;
                    }
                    Held::Shared => {
                        prop_assert!(model.held[read] > 0);
                        model.held[read] += 1;
                    }
                    Held::Gone => prop_assert!(false, "no open ends in this property"),
                }
            }
            Op::Release(read) => {
                if model.held[read] > 0 {
                    let forgotten = registry.release(&model.reads[read].0);
                    model.held[read] -= 1;
                    prop_assert_eq!(forgotten, model.held[read] == 0);
                    if forgotten {
                        model.running[read] = None;
                        model.last_start[read] = None;
                        model.last_touch[read] = None;
                    }
                }
            }
            Op::Change(which) => {
                let (at, scope) = changes[which % changes.len()].clone();
                let starts = registry.changed(&at, &scope);
                model.touch(|of| match of {
                    ReadOf::Project { at: read_at, keys } => {
                        *read_at == at && keys.iter().any(|key| key.depends_on(&scope))
                    }
                    ReadOf::App(_) => false,
                });
                model.started(starts)?;
            }
            Op::Topic => {
                let starts = registry.topic(&AppTopic::Projects);
                model.touch(
                    |of| matches!(of, ReadOf::App(topics) if topics.contains(&AppTopic::Projects)),
                );
                model.started(starts)?;
            }
            Op::Resync => {
                let starts = registry.resync();
                model.touch(|_| true);
                model.started(starts)?;
            }
            Op::Land(which) => model.land(&mut registry, which)?,
        }
    }
    // Every fetch lands, in the order they were sent, and fetches that followed run too.
    while !model.outstanding.is_empty() {
        model.land(&mut registry, 0)?;
    }
    for read in 0..model.reads.len() {
        if model.held[read] == 0 {
            continue;
        }
        prop_assert_eq!(model.running[read], None);
        let started = model.last_start[read];
        prop_assert!(started.is_some(), "a held read was never fetched");
        if let Some(touched) = model.last_touch[read] {
            prop_assert!(
                started > Some(touched),
                "read {} was touched at {touched}, but its last fetch started at {started:?}",
                model.reads[read].0.body
            );
            prop_assert!(
                model.shown_start[read] > Some(touched),
                "read {} shows a value fetched before the change at {touched}",
                model.reads[read].0.body
            );
        }
    }
    Ok(())
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 512, ..ProptestConfig::default() })]

    #[test]
    fn no_change_is_hidden_behind_an_earlier_reply(ops in proptest::collection::vec(op(), 0..80)) {
        run(ops)?;
    }
}

// --- The property: an ended open is never fetched or shown ---------------------------------

#[derive(Debug, Clone)]
enum EndOp {
    Hold(usize),
    Change(usize),
    Resync,
    Land(usize),
    /// Close the open at this index of the scopes below.
    Close(usize),
    /// Announce a newer open of this folder.
    Open(usize),
    StaleScope(usize),
}

fn end_op() -> impl Strategy<Value = EndOp> {
    prop_oneof![
        4 => (0usize..3).prop_map(EndOp::Hold),
        3 => (0usize..2).prop_map(EndOp::Change),
        1 => Just(EndOp::Resync),
        5 => any::<usize>().prop_map(EndOp::Land),
        1 => (0usize..2).prop_map(EndOp::Close),
        1 => (0usize..2).prop_map(EndOp::Open),
        1 => any::<usize>().prop_map(EndOp::StaleScope),
    ]
}

proptest! {
    #[test]
    fn an_ended_open_is_never_fetched_or_shown(ops in proptest::collection::vec(end_op(), 0..80)) {
        // Folders TIT and PSA, their current epoch, and which epochs have ended, as the events
        // said so.
        let folders = ["TIT", "PSA"];
        let mut epoch = [1u64, 1u64];
        let mut ended: std::collections::HashSet<(usize, u64)> = Default::default();
        let mut registry = Registry::new();
        // Three reads: two in the Titus open and one in the Psalms open, each remembered with
        // the epoch it was made at.
        let reads: Vec<(QueryId, usize)> = vec![(id("a"), 0), (id("b"), 0), (id("c"), 1)];
        let mut made_at: Vec<Option<u64>> = vec![None; 3];
        let mut outstanding: Vec<(usize, Ticket)> = Vec::new();
        let keys = |folder: usize| vec![if folder == 0 { QueryKey::Book(BookId::Titus) } else { QueryKey::Book(BookId::Psalms) }];
        for op in ops {
            match op {
                EndOp::Hold(read) => {
                    if made_at[read].is_none() {
                        let (id, folder) = &reads[read];
                        let at = scope(folders[*folder], epoch[*folder]);
                        match registry.hold(id, project(&at, keys(*folder))) {
                            Held::Fetch(ticket) => {
                                made_at[read] = Some(epoch[*folder]);
                                outstanding.push((read, ticket));
                            }
                            Held::Gone => made_at[read] = Some(epoch[*folder]),
                            Held::Shared => prop_assert!(false, "a new read is not shared"),
                        }
                    }
                }
                EndOp::Change(folder) => {
                    let at = scope(folders[folder], epoch[folder]);
                    for start in registry.changed(&at, &ChangeScope::Everything) {
                        let read = reads.iter().position(|(id, _)| *id == start.id).unwrap();
                        let made = made_at[read].unwrap();
                        prop_assert!(!ended.contains(&(folder, made)), "fetched a read of an ended open");
                        outstanding.push((read, start.ticket));
                    }
                }
                EndOp::Resync => {
                    for start in registry.resync() {
                        let read = reads.iter().position(|(id, _)| *id == start.id).unwrap();
                        let folder = reads[read].1;
                        prop_assert!(!ended.contains(&(folder, made_at[read].unwrap())), "fetched a read of an ended open");
                        outstanding.push((read, start.ticket));
                    }
                }
                EndOp::Land(which) => {
                    if !outstanding.is_empty() {
                        let (read, ticket) = outstanding.remove(which % outstanding.len());
                        let folder = reads[read].1;
                        let made = made_at[read].unwrap();
                        let landing = registry.landed(&reads[read].0, ticket, Landed::Value(None));
                        if landing.outcome == Outcome::Apply {
                            prop_assert!(!ended.contains(&(folder, made)), "showed a reply of an ended open");
                        }
                        if let Some(next) = landing.then {
                            prop_assert!(!ended.contains(&(folder, made)));
                            outstanding.push((read, next));
                        }
                    }
                }
                EndOp::Close(folder) => {
                    registry.closed(&scope(folders[folder], epoch[folder]));
                    ended.insert((folder, epoch[folder]));
                }
                EndOp::Open(folder) => {
                    epoch[folder] += 1;
                    registry.opened(scope(folders[folder], epoch[folder]));
                    for earlier in 1..epoch[folder] {
                        ended.insert((folder, earlier));
                    }
                }
                EndOp::StaleScope(which) => {
                    if !outstanding.is_empty() {
                        let (read, ticket) = outstanding.remove(which % outstanding.len());
                        let folder = reads[read].1;
                        let made = made_at[read].unwrap();
                        let landing = registry.landed(&reads[read].0, ticket, Landed::StaleScope);
                        // The server said the open it was made at is gone, if the read was
                        // its current attempt.
                        if landing.outcome == Outcome::Drop {
                            ended.insert((folder, made));
                        }
                    }
                }
            }
        }
    }
}
