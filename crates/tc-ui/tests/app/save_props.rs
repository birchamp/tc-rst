//! Property tests of the save machine against a scripted, adversarial server (APP-DESIGN
//! 6.5): answers in any order, late, lost before or after the store, refused, reused; teammates'
//! changes; session changes; reloads at any point; restarts.
//!
//! The server keeps every field's versions, answers a request id it has seen with its first
//! answer (the reply cache), and stores a text over an older version as the store does,
//! beside it. The properties are checked after every step.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use proptest::prelude::*;
use tc_api::NotSaved;
use tc_api::app::{OpenSession, SessionRole};
use tc_api::buffer::{BufferKey, FieldKind};
use tc_api::event::SessionMode;
use tc_api::ids::{ProjectId, RequestId, ServerInstance, ViewSeq};
use tc_api::shown::{PersonLabel, Who};
use tc_api::token::ShownToken;
use tc_api::write::{Ack, SavedValue, Stored, TokenUpdate};
use tc_ui::app::buffer::{self, MemStore};
use tc_ui::app::save::classify::{Answered, SaveError};
use tc_ui::app::save::{Command, FieldSave, FieldScope, Pair, SaveMachine, View};

use super::save_kit::{InFlight, PROJECT_ID, Page, ids, key, key_of, scope, tok, view};

const TEXTS: [&str; 8] = [
    "",
    "a",
    "e\u{301}",
    "\u{e9}",
    "Paul, a servant",
    "\u{5d1}\u{5bc}\u{5b0}\u{5e8}\u{5b5}",
    "\u{643}\u{64e}\u{62a}\u{64e}\u{628}\u{64e}",
    "b",
];

fn fields() -> [BufferKey; 4] {
    [
        key("t"),
        key_of(FieldKind::Comment, "c"),
        key_of(FieldKind::Section, "s"),
        key_of(FieldKind::Verse, "v"),
    ]
}

fn is_project(k: &BufferKey) -> bool {
    k.field.kind == FieldKind::Verse
}

#[derive(Debug, Clone, Copy)]
enum Reply {
    Ack,
    /// The store ran; the reply was lost.
    LostAfter,
    /// The request never reached the store.
    LostBefore,
    StaleScope,
    Reused,
    Fail,
    /// A refusal no retry in this open can fix (tC4 #801): a newer change under a merged verse.
    LookAgain,
}

#[derive(Debug, Clone)]
enum Step {
    Type(usize, usize),
    Focus(usize),
    Blur(usize),
    Save(usize),
    Tick(u16),
    Deliver(usize, Reply),
    Retry,
    Teammate(usize, usize),
    /// The project closed (`None`), its open announced again (`Some(0 | 1)`), or reopened under
    /// a new epoch (`Some(2..)`): epochs only grow (3.2).
    Session(Option<u8>),
    Gate,
    Reload(bool),
    Restart,
}

fn reply() -> impl Strategy<Value = Reply> {
    prop_oneof![
        4 => Just(Reply::Ack),
        2 => Just(Reply::LostAfter),
        1 => Just(Reply::LostBefore),
        1 => Just(Reply::StaleScope),
        1 => Just(Reply::Reused),
        1 => Just(Reply::Fail),
        1 => Just(Reply::LookAgain),
    ]
}

fn step() -> impl Strategy<Value = Step> {
    let field = 0usize..4;
    prop_oneof![
        6 => (field.clone(), 0..TEXTS.len()).prop_map(|(f, t)| Step::Type(f, t)),
        2 => field.clone().prop_map(Step::Focus),
        3 => field.clone().prop_map(Step::Blur),
        1 => field.clone().prop_map(Step::Save),
        3 => (0u16..3000).prop_map(Step::Tick),
        5 => (0usize..4, reply()).prop_map(|(i, r)| Step::Deliver(i, r)),
        1 => Just(Step::Retry),
        1 => (field.clone(), 0..TEXTS.len()).prop_map(|(f, t)| Step::Teammate(f, t)),
        1 => proptest::option::of(0u8..4).prop_map(Step::Session),
        1 => Just(Step::Gate),
        2 => any::<bool>().prop_map(Step::Reload),
        1 => Just(Step::Restart),
    ]
}

/// The server: every field's versions (version 0 is ""), the token of each version, and the
/// answers it gave, by request id.
#[derive(Default)]
struct Server {
    versions: BTreeMap<BufferKey, Vec<String>>,
    tokens: HashMap<ShownToken, (BufferKey, usize)>,
    token_of: BTreeMap<(BufferKey, usize), ShownToken>,
    next_token: u64,
    view: u64,
    answered: HashMap<RequestId, Ack>,
}

impl Server {
    fn new() -> Self {
        let mut server = Server {
            next_token: 1000,
            view: 1,
            ..Server::default()
        };
        for k in fields() {
            server.versions.insert(k.clone(), Vec::new());
            server.mint(&k, 0);
        }
        server
    }

    fn mint(&mut self, k: &BufferKey, version: usize) -> ShownToken {
        self.next_token += 1;
        let token = tok(self.next_token);
        self.tokens.insert(token.clone(), (k.clone(), version));
        self.token_of.insert((k.clone(), version), token.clone());
        token
    }

    fn value(&self, k: &BufferKey, version: usize) -> String {
        version
            .checked_sub(1)
            .and_then(|i| self.versions[k].get(i).cloned())
            .unwrap_or_default()
    }

    fn latest(&self, k: &BufferKey) -> (String, ShownToken, usize) {
        let version = self.versions[k].len();
        (
            self.value(k, version),
            self.token_of[&(k.clone(), version)].clone(),
            version,
        )
    }

    /// The current pair, read from the open at `epoch` (`None` for a field of the app).
    fn pair(&self, k: &BufferKey, epoch: Option<u64>) -> Pair {
        let (value, token, _) = self.latest(k);
        Pair {
            value,
            token,
            view: Some(self.view_at(epoch)),
        }
    }

    fn view_at(&self, epoch: Option<u64>) -> View {
        View::of(epoch.map(scope).as_ref(), ViewSeq::new(self.view))
    }

    /// The text a token stands for.
    fn text_of(&self, token: &ShownToken) -> Option<String> {
        let (k, version) = self.tokens.get(token)?;
        Some(self.value(k, *version))
    }

    /// Runs a request as the store does, or answers it from the cache.
    fn store(&mut self, flight: &InFlight) -> Ack {
        if let Some(ack) = self.answered.get(&flight.sent.request) {
            return ack.clone();
        }
        let k = &flight.key;
        let named = match flight.base.first().and_then(|t| self.tokens.get(t)) {
            Some((field, version)) if field == k => *version,
            other => panic!("a save named a token of another field or none: {other:?}"),
        };
        let text = buffer::nfc(&flight.sent.text);
        let (stored, token) = if text == self.value(k, named) {
            (Stored::Unchanged, flight.base[0].clone())
        } else {
            self.versions.get_mut(k).unwrap().push(text.clone());
            self.view += 1;
            let version = self.versions[k].len();
            (Stored::Written, self.mint(k, version))
        };
        let record = tc_api::ids::RecordRef::parse(r#"{"kind":"test"}"#).unwrap();
        let ack = Ack {
            view: ViewSeq::new(self.view),
            stored,
            tokens: vec![TokenUpdate {
                record: record.clone(),
                token,
            }],
            new_conflicts: Vec::new(),
            saved: vec![SavedValue { record, text }],
            rows: Vec::new(),
            disk_warning: None,
        };
        self.answered.insert(flight.sent.request, ack.clone());
        ack
    }

    fn teammate(&mut self, k: &BufferKey, text: &str, epoch: Option<u64>) -> Pair {
        self.versions.get_mut(k).unwrap().push(buffer::nfc(text));
        self.view += 1;
        let version = self.versions[k].len();
        let token = self.mint(k, version);
        Pair {
            value: buffer::nfc(text),
            token,
            view: Some(self.view_at(epoch)),
        }
    }
}

fn session(epoch: u64) -> OpenSession {
    OpenSession {
        at: scope(epoch),
        id: ProjectId::parse(PROJECT_ID).unwrap(),
        mode: SessionMode::Writable,
        role: SessionRole::Screen,
        outside: Default::default(),
    }
}

struct World {
    page: Page,
    server: Server,
    sessions: Vec<OpenSession>,
    editors: BTreeSet<BufferKey>,
    /// Every body each request id was sent with.
    bodies: HashMap<RequestId, (String, Vec<ShownToken>, Option<tc_api::ids::Scope>)>,
    /// Ids answered with anything but a lost reply.
    settled: BTreeSet<RequestId>,
    /// Ids made before the last restart.
    old_run: BTreeSet<RequestId>,
    /// Section fields whose Save or Retry was pressed and not yet followed by a clean state.
    save_pressed: BTreeSet<BufferKey>,
    /// The last text the person typed per field, and texts that went to the next open's offer.
    last_typed: BTreeMap<BufferKey, String>,
    offered: Vec<(BufferKey, String)>,
    run: u8,
    /// The newest epoch of the project's opens.
    epoch: u64,
}

impl World {
    fn new() -> Self {
        let sessions = vec![session(1)];
        let mut page = Page::new();
        let commands = page.m.sessions(&sessions, 0.0);
        page.run(commands);
        World {
            page,
            server: Server::new(),
            sessions,
            editors: BTreeSet::new(),
            bodies: HashMap::new(),
            settled: BTreeSet::new(),
            old_run: BTreeSet::new(),
            save_pressed: BTreeSet::new(),
            last_typed: BTreeMap::new(),
            offered: Vec::new(),
            run: 1,
            epoch: 1,
        }
    }

    /// The open a read of `k` is made against: the project's current open, or none for a
    /// field of the app.
    fn read_epoch(&self, k: &BufferKey) -> Option<u64> {
        is_project(k).then_some(self.epoch)
    }

    fn scope_of(&self, k: &BufferKey) -> FieldScope {
        if is_project(k) {
            FieldScope::Project(self.sessions.first().map(|s| s.at.clone()))
        } else {
            FieldScope::App
        }
    }

    /// The field's editor, opened on the server's current pair when it is not open.
    fn editor(&mut self, k: &BufferKey) {
        if self.editors.insert(k.clone()) {
            let scope = self.scope_of(k);
            let pair = self.server.pair(k, self.read_epoch(k));
            self.page.open_with(k, scope, pair);
        }
    }

    fn check_sends(&mut self, from: usize) -> Result<(), TestCaseError> {
        let commands: Vec<Command> = self.page.log[from..].to_vec();
        for command in commands {
            let (k, sent, base, resend) = match command {
                Command::Send { key, sent, base } => (key, sent, base, false),
                Command::Resend {
                    key, sent, base, ..
                } => (key, sent, base, true),
                _ => continue,
            };
            let body = (sent.text.clone(), base.clone(), sent.scope.clone());
            prop_assert!(
                !self.old_run.contains(&sent.request),
                "a request of a dead run was sent again"
            );
            if let Some(first) = self.bodies.get(&sent.request) {
                prop_assert_eq!(first, &body, "one body per request id");
                prop_assert!(
                    !self.settled.contains(&sent.request),
                    "an answered id was sent again"
                );
            } else {
                prop_assert!(!resend, "a resend names an id never sent");
                // One save of a field in flight: every earlier id of the field was answered.
                let open_ids = self
                    .page
                    .inflight
                    .iter()
                    .filter(|f| {
                        f.key == k
                            && f.sent.request != sent.request
                            && !self.settled.contains(&f.sent.request)
                    })
                    .count();
                prop_assert!(open_ids == 0, "two saves of {:?} in flight", k.field.place);
                if k.field.kind == FieldKind::Section {
                    prop_assert!(
                        self.save_pressed.contains(&k),
                        "a section saved without Save"
                    );
                }
                self.bodies.insert(sent.request, body);
            }
        }
        Ok(())
    }

    fn check_fields(&self) -> Result<(), TestCaseError> {
        for k in self.page.m.keys() {
            let text = self.page.m.text(k).unwrap_or_default();
            match self.page.m.state(k) {
                Some(FieldSave::Clean | FieldSave::SavedWithWarning(_)) => {
                    let base = self.page.m.base(k).and_then(|b| b.first()).cloned();
                    let stored = base.and_then(|t| self.server.text_of(&t));
                    prop_assert_eq!(
                        stored.map(|s| buffer::nfc(&s)),
                        Some(buffer::nfc(text)),
                        "a clean field holds exactly the text its token stands for"
                    );
                }
                Some(_) => {
                    let entry = buffer::read(&self.page.store, k);
                    prop_assert_eq!(
                        entry.map(|e| e.text),
                        Some(text.to_owned()),
                        "unsaved text is in the buffer"
                    );
                }
                None => {}
            }
        }
        Ok(())
    }

    fn unsaved(&self) -> BTreeMap<BufferKey, String> {
        self.page
            .m
            .keys()
            .filter(|k| {
                !matches!(
                    self.page.m.state(k),
                    Some(FieldSave::Clean | FieldSave::SavedWithWarning(_)) | None
                )
            })
            .map(|k| {
                (
                    k.clone(),
                    self.page.m.text(k).unwrap_or_default().to_owned(),
                )
            })
            .collect()
    }

    fn deliver(&mut self, index: usize, reply: Reply) {
        if self.page.inflight.is_empty() {
            return;
        }
        let index = index % self.page.inflight.len();
        let flight = self.page.inflight.remove(index).unwrap();
        let cached = self.server.answered.contains_key(&flight.sent.request);
        let answered = match reply {
            Reply::Ack => Answered::Acked(self.server.store(&flight)),
            Reply::LostAfter => {
                self.server.store(&flight);
                Answered::Lost
            }
            Reply::LostBefore => Answered::Lost,
            _ if cached => Answered::Acked(self.server.store(&flight)),
            Reply::StaleScope => Answered::Reattach,
            Reply::Reused => Answered::Reissue,
            Reply::Fail => Answered::Failed(SaveError::NotSaved(NotSaved::Disk("full".into()))),
            Reply::LookAgain => Answered::Failed(SaveError::NotSaved(NotSaved::LookAgain)),
        };
        if !matches!(answered, Answered::Lost) {
            self.settled.insert(flight.sent.request);
        }
        self.page.answer(&flight, answered);
    }

    fn apply(&mut self, step: &Step) -> Result<(), TestCaseError> {
        let keys = fields();
        let from = self.page.log.len();
        match step {
            Step::Type(f, t) => {
                let k = &keys[*f];
                self.editor(k);
                if !self.page.focused.contains(k) {
                    self.page.focus(k);
                }
                self.page.type_text(k, TEXTS[*t]);
                self.last_typed.insert(k.clone(), TEXTS[*t].to_owned());
            }
            Step::Focus(f) => {
                let k = &keys[*f];
                self.editor(k);
                self.page.focus(k);
            }
            Step::Blur(f) => {
                let k = &keys[*f];
                if self.page.focused.contains(k) {
                    self.page.blur(k);
                }
            }
            Step::Save(f) => {
                let k = &keys[*f];
                self.editor(k);
                self.save_pressed.insert(k.clone());
                self.page.save(k);
            }
            Step::Tick(ms) => {
                self.page.advance(f64::from(*ms));
            }
            Step::Deliver(i, r) => self.deliver(*i, *r),
            Step::Retry => {
                for k in self.page.m.keys().cloned().collect::<Vec<_>>() {
                    if matches!(self.page.m.state(&k), Some(FieldSave::Failed(_))) {
                        self.save_pressed.insert(k);
                    }
                }
                self.page.retry();
            }
            Step::Teammate(f, t) => {
                let k = &keys[*f];
                let epoch = self.read_epoch(k);
                let pair = self.server.teammate(k, TEXTS[*t], epoch);
                let by = Some(PersonLabel {
                    who: Who::Teammate(2),
                    computer: None,
                });
                let commands = self.page.m.refresh(k, pair, by);
                self.page.run(commands);
            }
            Step::Session(change) => {
                self.sessions = match change {
                    None => Vec::new(),
                    Some(0 | 1) if !self.sessions.is_empty() => self.sessions.clone(),
                    Some(_) => {
                        self.epoch += 1;
                        vec![session(self.epoch)]
                    }
                };
                let commands = self.page.m.sessions(&self.sessions, self.page.now);
                self.page.run(commands);
            }
            Step::Gate => {
                self.page.m.begin_gate();
                let commands = self.page.m.flush_for_gate(self.page.now);
                self.page.run(commands);
                self.page.m.end_gate();
            }
            Step::Reload(server_runs_pending) => self.reload(*server_runs_pending)?,
            Step::Restart => self.restart()?,
        }
        self.check_sends(from)?;
        // A section field that is clean no longer has a Save pressed.
        for k in &keys {
            if matches!(self.page.m.state(k), Some(FieldSave::Clean) | None) {
                self.save_pressed.remove(k);
            }
        }
        self.check_fields()
    }

    fn reload(&mut self, server_runs_pending: bool) -> Result<(), TestCaseError> {
        let before = self.unsaved();
        // The old page's requests are on their way; the server runs them or not, and their
        // replies reach nobody.
        for flight in std::mem::take(&mut self.page.inflight) {
            if server_runs_pending {
                self.server.store(&flight);
            }
        }
        let instance = self.page.m.instance();
        let mut page = Page::on(self.page.store.clone());
        page.m = SaveMachine::new(instance, ids());
        page.now = self.page.now;
        let commands = page.m.sessions(&self.sessions, page.now);
        page.run(commands);
        for (k, entry) in buffer::entries(&page.store) {
            if entry.instance != instance {
                continue;
            }
            let scope = if is_project(&k) {
                match self.sessions.first() {
                    Some(s) => FieldScope::Project(Some(s.at.clone())),
                    // No open of the project: the entry waits for the offer.
                    None => continue,
                }
            } else {
                FieldScope::App
            };
            let commands = page.m.restore(k, entry, scope, page.now);
            page.run(commands);
        }
        // One log across the reload, so the checks of this step see what the new page sent.
        page.log.splice(0..0, std::mem::take(&mut self.page.log));
        page.mirror = std::mem::take(&mut self.page.mirror);
        self.page = page;
        self.editors.clear();
        for (k, text) in before {
            match self.page.m.text(&k) {
                Some(restored) => {
                    prop_assert_eq!(restored, text.as_str(), "a reload restores the text")
                }
                None => {
                    let entry = buffer::read(&self.page.store, &k);
                    prop_assert_eq!(
                        entry.map(|e| e.text),
                        Some(text.clone()),
                        "kept for the offer"
                    );
                    self.offered.push((k, text));
                }
            }
        }
        Ok(())
    }

    fn restart(&mut self) -> Result<(), TestCaseError> {
        let before = self.unsaved();
        for (k, text) in &before {
            let entry = buffer::read(&self.page.store, k);
            prop_assert_eq!(
                entry.map(|e| e.text),
                Some(text.clone()),
                "kept for the offer"
            );
            self.offered.push((k.clone(), text.clone()));
        }
        self.run += 1;
        let instance = ServerInstance::from_bytes([self.run; 16]);
        for id in self.bodies.keys() {
            self.old_run.insert(*id);
        }
        self.page.m.restarted(instance);
        // A5 goes Home: every editor closes, and the fields kept for it go with it.
        for k in std::mem::take(&mut self.editors) {
            self.page.close(&k);
        }
        prop_assert!(
            self.page.m.keys().next().is_none(),
            "a field outlived the restart and its editor"
        );
        // Late replies of the old run find no field.
        for flight in std::mem::take(&mut self.page.inflight) {
            let ack = self.server.store(&flight);
            let commands = self.page.m.reply(
                &flight.key,
                flight.sent.request,
                Answered::Acked(ack),
                self.page.now,
            );
            prop_assert!(commands.is_empty(), "a reply of a dead run did something");
        }
        self.server.answered.clear();
        // The restarted server is at a new port: a new origin, with storage of its own.
        self.page.store = MemStore::new();
        self.page.focused.clear();
        self.editors.clear();
        self.save_pressed.clear();
        let commands = self.page.m.sessions(&self.sessions, self.page.now);
        self.page.run(commands);
        Ok(())
    }

    /// Everything answered, every field left and saved.
    fn settle(&mut self) -> Result<(), TestCaseError> {
        self.epoch += 1;
        self.sessions = vec![session(self.epoch)];
        let commands = self.page.m.sessions(&self.sessions, self.page.now);
        self.page.run(commands);
        for _ in 0..30 {
            while !self.page.inflight.is_empty() {
                let from = self.page.log.len();
                self.deliver(0, Reply::Ack);
                self.check_sends(from)?;
            }
            let from = self.page.log.len();
            for k in fields() {
                if self.page.focused.contains(&k) {
                    self.page.blur(&k);
                }
                if self.page.m.state(&k).is_some() {
                    self.save_pressed.insert(k.clone());
                    self.page.save(&k);
                }
            }
            self.page.retry();
            self.page.advance(5000.0);
            self.check_sends(from)?;
            self.check_fields()?;
            if self.page.inflight.is_empty() && self.unsaved().is_empty() {
                break;
            }
        }
        prop_assert!(
            self.unsaved().is_empty(),
            "everything saves in the end: {:?}",
            self.unsaved()
        );
        // The last text the person typed in each field is stored, or went to the offer.
        for (k, text) in &self.last_typed {
            let on_server = buffer::nfc(text).is_empty()
                || self.server.versions[k]
                    .iter()
                    .any(|v| *v == buffer::nfc(text));
            let offered = self.offered.iter().any(|(ok, ot)| ok == k && ot == text);
            prop_assert!(
                on_server || offered,
                "the last text of {:?} was lost: {:?}",
                k.field.place,
                text
            );
        }
        Ok(())
    }
}

proptest! {
    // 512 cases missed a history that 40,000 found (the reissue with no open); both properties
    // passed at 60,000 cases on 2026-10-19. 2,048 keeps the run to a few seconds.
    #![proptest_config(ProptestConfig::with_cases(2048))]

    /// The row's proptest: no `current` is dropped. Unsaved text is never clean and always in
    /// the buffer, a clean field holds exactly the text its token stands for, a reload brings
    /// unsaved text back (or keeps it for the offer), and in the end every last text is
    /// stored or offered. Along the way: one body per request id, an answered id never sent
    /// again, one save of a field in flight, nothing sent for a dead run, and a section saved
    /// only after its Save.
    #[test]
    fn no_current_is_dropped(steps in proptest::collection::vec(step(), 1..60)) {
        let mut world = World::new();
        for step in &steps {
            world.apply(step)?;
        }
        world.settle()?;
    }
}

/// The shortest history `no_current_is_dropped` found at 40,000 cases: a reissue after
/// `RequestReused` while the project had no open left the answered request in the entry, and
/// the reload after the project opened again sent it.
#[test]
fn a_reissue_with_no_open_and_two_reloads_send_no_answered_id() {
    for pending in [false, true] {
        let mut world = World::new();
        for step in [
            Step::Type(3, 1),
            Step::Blur(3),
            Step::Session(None),
            Step::Deliver(0, Reply::Reused),
            Step::Reload(pending),
            Step::Session(Some(0)),
            Step::Reload(pending),
        ] {
            world.apply(&step).unwrap();
        }
        world.settle().unwrap();
    }
}

// --- The order of an Ack, its own Changed and the refreshes (5.4) -----------------------

#[derive(Debug, Clone)]
enum Event {
    /// The field's own save's `Changed { LocalSave }`.
    OwnChanged,
    /// The refetch that event causes: the stored text at the Ack's view.
    OwnRefresh,
    Ack,
    /// A teammate's change after the save, with a later view.
    Teammate(usize),
}

fn orders() -> impl Strategy<Value = (Vec<Event>, bool)> {
    let teammates = proptest::collection::vec(0..TEXTS.len(), 0..3);
    (teammates, any::<bool>()).prop_flat_map(|(teammates, typing_after)| {
        let mut events = vec![Event::OwnChanged, Event::OwnRefresh, Event::Ack];
        events.extend(teammates.into_iter().map(Event::Teammate));
        Just(events)
            .prop_shuffle()
            .prop_map(move |events| (events, typing_after))
    })
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(512))]

    /// Whatever order an Ack, its own `Changed`, the refetch it causes and teammates' later
    /// changes arrive in: no notice unless a teammate's text differs from the field's unsaved
    /// text, never a save that names a token older than the field's own Ack (which would
    /// conflict with itself), and a clean field ends on the newest version.
    #[test]
    fn events_reply_order_is_safe((events, typing_after) in orders()) {
        let mut page = Page::new();
        let k = key_of(FieldKind::Verse, "TIT 1:1");
        page.open_with(&k, FieldScope::App, Pair { value: "Paul".into(), token: tok(1), view: Some(view(10)) });
        page.focus(&k);
        page.type_text(&k, "Paul, a servant");
        page.blur(&k);
        let flight = page.inflight.pop_front().unwrap();
        let own_view = 11;
        let own_token = 2;
        let mut teammate_views = 0u64;
        let mut acked = false;
        let mut teammate_differs = false;
        let mut newest = (own_view, own_token);
        for event in &events {
            match event {
                Event::OwnChanged => page.m.local_change(flight.sent.request, view(own_view)),
                Event::OwnRefresh => {
                    let commands = page.m.refresh(&k, Pair { value: "Paul, a servant".into(), token: tok(own_token), view: Some(view(own_view)) }, None);
                    page.run(commands);
                }
                Event::Ack => {
                    acked = true;
                    page.answer(&flight, Answered::Acked(super::save_kit::ack("Paul, a servant", own_token, own_view)));
                    if typing_after {
                        page.focus(&k);
                        page.type_text(&k, "Paul, a servant of God");
                    }
                }
                Event::Teammate(t) => {
                    teammate_views += 1;
                    let view = own_view + teammate_views;
                    let token = 100 + teammate_views;
                    let text = buffer::nfc(TEXTS[*t]);
                    if !buffer::same_text(&text, "Paul, a servant") {
                        teammate_differs = true;
                    }
                    newest = (view, token);
                    let by = Some(PersonLabel { who: Who::Username("juan".into()), computer: None });
                    let commands = page.m.refresh(&k, Pair { value: text, token: tok(token), view: Some(super::save_kit::view(view)) }, by);
                    page.run(commands);
                }
            }
        }
        prop_assert!(acked);
        if page.m.notice(&k).is_some() {
            prop_assert!(teammate_differs, "a notice with no teammate's other text");
        }
        page.focused.remove(&k);
        let commands = page.m.blur(&k, page.now);
        page.run(commands);
        // Every save after the Ack names the Ack's token or a newer one.
        let own_or_newer: Vec<ShownToken> = (0..=teammate_views).map(|n| if n == 0 { tok(own_token) } else { tok(100 + n) }).collect();
        for (sent, base, _) in page.sends().into_iter().skip(1) {
            prop_assert!(own_or_newer.contains(&base[0]), "{:?} names an older token", sent.text);
        }
        if matches!(page.state(&k), Some(FieldSave::Clean)) {
            prop_assert_eq!(page.m.base(&k).map(|b| b[0].clone()), Some(tok(newest.1)), "a clean field ends on the newest version");
        }
    }
}
