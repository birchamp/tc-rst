//! A page around the save machine for the tests: it carries the machine's commands out on a
//! storage in memory as the browser glue does, plays `boot.js`'s capture listener on each
//! keystroke into a focused editor, and keeps the saves that are waiting for a reply.
#![allow(dead_code)]

use std::collections::{BTreeSet, VecDeque};
use std::sync::atomic::{AtomicU64, Ordering};

use serde_json::json;
use tc_api::buffer::{BufferEntry, BufferKey, FieldKey, FieldKind, FieldPlace, SentSave};
use tc_api::ids::{
    Epoch, ProjectId, ProjectRef, RecordRef, RequestId, Scope, ServerInstance, ViewSeq, WallMs,
};
use tc_api::token::ShownToken;
use tc_api::write::{Ack, SavedValue, Stored, TokenUpdate};
use tc_ui::app::buffer::{self, MemStore};
use tc_ui::app::save::classify::Answered;
use tc_ui::app::save::{
    Command, FieldSave, FieldScope, GateState, Ms, Pair, SaveMachine, Source, View,
};

pub const PROJECT_ID: &str = "6f1c0e9a-2b7d-4c3e-8a5f-0b1d2c3e4f50";

pub fn instance() -> ServerInstance {
    ServerInstance::from_bytes([1; 16])
}

pub fn tok(n: u64) -> ShownToken {
    serde_json::from_value(json!(format!("st1.{n:0>24}"))).unwrap()
}

pub fn key_of(kind: FieldKind, place: &str) -> BufferKey {
    BufferKey {
        project: ProjectRef::parse("en_ult").unwrap(),
        id: ProjectId::parse(PROJECT_ID).unwrap(),
        field: FieldKey {
            kind,
            place: FieldPlace::parse(place).unwrap(),
        },
    }
}

/// A test field: blur or two seconds idle, and no open needed.
pub fn key(place: &str) -> BufferKey {
    key_of(FieldKind::Test, place)
}

pub fn scope(epoch: u64) -> Scope {
    Scope {
        project: ProjectRef::parse("en_ult").unwrap(),
        epoch: Epoch::new(epoch),
    }
}

/// A view of no open (a test field), or of the open at `epoch` with [`pair_at`].
pub fn view(seq: u64) -> View {
    View {
        epoch: None,
        seq: ViewSeq::new(seq),
    }
}

pub fn view_at(epoch: u64, seq: u64) -> View {
    View::of(Some(&scope(epoch)), ViewSeq::new(seq))
}

pub fn pair(value: &str, token: u64, seq: Option<u64>) -> Pair {
    Pair {
        value: value.to_owned(),
        token: tok(token),
        view: seq.map(view),
    }
}

/// A pair read from the open at `epoch`, at view `seq`.
pub fn pair_at(value: &str, token: u64, epoch: u64, seq: u64) -> Pair {
    Pair {
        value: value.to_owned(),
        token: tok(token),
        view: Some(view_at(epoch, seq)),
    }
}

pub fn ack(text: &str, token: u64, view: u64) -> Ack {
    let record = RecordRef::parse(r#"{"kind":"test"}"#).unwrap();
    Ack {
        view: ViewSeq::new(view),
        stored: Stored::Written,
        tokens: vec![TokenUpdate {
            record: record.clone(),
            token: tok(token),
        }],
        new_conflicts: Vec::new(),
        saved: vec![SavedValue {
            record,
            text: buffer::nfc(text),
        }],
        rows: Vec::new(),
        disk_warning: None,
    }
}

/// A request in flight: the field, the request, and the tokens it named.
#[derive(Debug, Clone)]
pub struct InFlight {
    pub key: BufferKey,
    pub sent: SentSave,
    pub base: Vec<ShownToken>,
    pub resend: bool,
}

pub struct Page {
    pub m: SaveMachine,
    pub now: Ms,
    pub store: MemStore,
    /// Every command, in order.
    pub log: Vec<Command>,
    /// Saves waiting for a reply, oldest first.
    pub inflight: VecDeque<InFlight>,
    pub focused: BTreeSet<BufferKey>,
    /// The mirror's lines, as the shell would append them.
    pub mirror: Vec<String>,
    /// How many times the machine asked for the sessions again.
    pub session_reads: usize,
    next_token: u64,
    next_view: u64,
}

/// Fresh ids for every page of the test binary, as the browser's random ids are: a page after
/// a reload never makes an id the page before it made.
pub fn ids() -> Box<dyn FnMut() -> RequestId> {
    static NEXT: AtomicU64 = AtomicU64::new(1);
    Box::new(|| {
        let mut bytes = [0u8; 16];
        bytes[8..].copy_from_slice(&NEXT.fetch_add(1, Ordering::Relaxed).to_be_bytes());
        RequestId::from_bytes(bytes)
    })
}

impl Page {
    pub fn new() -> Self {
        Self::on(MemStore::new())
    }

    /// A page over a storage a page before it left, as after a reload.
    pub fn on(store: MemStore) -> Self {
        Self {
            m: SaveMachine::new(instance(), ids()),
            now: 0.0,
            store,
            log: Vec::new(),
            inflight: VecDeque::new(),
            focused: BTreeSet::new(),
            mirror: Vec::new(),
            session_reads: 0,
            next_token: 100,
            next_view: 100,
        }
    }

    /// Carries the commands out as the glue does, and returns them.
    pub fn run(&mut self, commands: Vec<Command>) -> Vec<Command> {
        for command in &commands {
            match command {
                // What the glue does: `write_entry`, and the line to the mirror in any case.
                Command::Write { key, entry, mirror } => {
                    let entry = BufferEntry {
                        at: WallMs(self.now as i64),
                        ..entry.clone()
                    };
                    let written = buffer::write_entry(&mut self.store, key, &entry);
                    if *mirror && let Some(line) = written.line {
                        self.mirror.push(line);
                    }
                }
                Command::Remove {
                    key,
                    instance,
                    if_text,
                    mirror,
                } => {
                    let _ = buffer::remove(&mut self.store, key, if_text.as_deref(), *instance);
                    if *mirror {
                        self.mirror
                            .push(buffer::removed_line(key, *instance).unwrap());
                    }
                }
                Command::ReadSessions => self.session_reads += 1,
                Command::Send { key, sent, base } => self.inflight.push_back(InFlight {
                    key: key.clone(),
                    sent: sent.clone(),
                    base: base.clone(),
                    resend: false,
                }),
                Command::Resend {
                    key, sent, base, ..
                } => self.inflight.push_back(InFlight {
                    key: key.clone(),
                    sent: sent.clone(),
                    base: base.clone(),
                    resend: true,
                }),
                Command::Draft { .. } | Command::DiscardDraft { .. } | Command::Show { .. } => {}
            }
        }
        self.log.extend(commands.iter().cloned());
        commands
    }

    pub fn open(&mut self, key: &BufferKey, value: &str, token: u64) -> String {
        self.open_with(key, FieldScope::App, pair(value, token, None))
    }

    pub fn open_with(&mut self, key: &BufferKey, scope: FieldScope, pair: Pair) -> String {
        let (shown, commands) = self.m.open(key.clone(), scope, pair, self.now);
        self.run(commands);
        shown
    }

    pub fn focus(&mut self, key: &BufferKey) {
        self.focused.insert(key.clone());
        let commands = self.m.focus(key, self.now);
        self.run(commands);
    }

    pub fn blur(&mut self, key: &BufferKey) -> Vec<Command> {
        self.focused.remove(key);
        let commands = self.m.blur(key, self.now);
        self.run(commands)
    }

    pub fn close(&mut self, key: &BufferKey) -> Vec<Command> {
        self.focused.remove(key);
        let commands = self.m.close(key, self.now);
        self.run(commands)
    }

    /// A keystroke: `boot.js` writes the entry first when the editor is focused, then the
    /// machine hears of it, as `Source::Input` only when `boot.js` says its item took the text
    /// (`tc4Boot.wrote`), as the glue decides.
    pub fn type_text(&mut self, key: &BufferKey, text: &str) -> Vec<Command> {
        let mut source = Source::Program;
        if self.focused.contains(key)
            && let Some(template) = self.m.template(key)
        {
            let input = buffer::input_to_entry(
                &mut self.store,
                key,
                text,
                WallMs(self.now as i64),
                &template,
            );
            if let Some(line) = input.line {
                self.mirror.push(line);
            }
            if input.stored {
                source = Source::Input;
            }
        }
        let commands = self.m.typed(key, text, source, self.now);
        self.run(commands)
    }

    pub fn save(&mut self, key: &BufferKey) -> Vec<Command> {
        let commands = self.m.save(key, self.now);
        self.run(commands)
    }

    pub fn retry(&mut self) -> Vec<Command> {
        let commands = self.m.retry(self.now);
        self.run(commands)
    }

    /// Lets `ms` pass, running every deadline on the way at its own time.
    pub fn advance(&mut self, ms: Ms) -> Vec<Command> {
        let until = self.now + ms;
        let mut all = Vec::new();
        while let Some(deadline) = self.m.next_deadline().filter(|d| *d <= until) {
            self.now = deadline.max(self.now);
            let commands = self.m.tick(self.now);
            all.extend(self.run(commands));
        }
        self.now = until;
        let commands = self.m.tick(self.now);
        all.extend(self.run(commands));
        all
    }

    pub fn answer(&mut self, flight: &InFlight, answered: Answered) -> Vec<Command> {
        let commands = self
            .m
            .reply(&flight.key, flight.sent.request, answered, self.now);
        self.run(commands)
    }

    /// The oldest save answered as the store answers: its text stored in NFC with a new token.
    pub fn deliver_ok(&mut self) -> InFlight {
        let flight = self.inflight.pop_front().expect("a save in flight");
        self.next_token += 1;
        self.next_view += 1;
        let answered = Answered::Acked(ack(&flight.sent.text, self.next_token, self.next_view));
        self.answer(&flight, answered);
        flight
    }

    pub fn deliver(&mut self, answered: Answered) -> InFlight {
        let flight = self.inflight.pop_front().expect("a save in flight");
        self.answer(&flight, answered);
        flight
    }

    /// The saves sent so far, resends included, in order.
    pub fn sends(&self) -> Vec<(SentSave, Vec<ShownToken>, bool)> {
        self.log
            .iter()
            .filter_map(|command| match command {
                Command::Send { sent, base, .. } => Some((sent.clone(), base.clone(), false)),
                Command::Resend { sent, base, .. } => Some((sent.clone(), base.clone(), true)),
                _ => None,
            })
            .collect()
    }

    pub fn sent_texts(&self) -> Vec<String> {
        self.sends()
            .into_iter()
            .map(|(sent, _, _)| sent.text)
            .collect()
    }

    pub fn drafts(&self) -> Vec<String> {
        self.log
            .iter()
            .filter_map(|command| match command {
                Command::Draft { text, .. } => Some(text.clone()),
                _ => None,
            })
            .collect()
    }

    pub fn shows(&self) -> Vec<String> {
        self.log
            .iter()
            .filter_map(|command| match command {
                Command::Show { text, .. } => Some(text.clone()),
                _ => None,
            })
            .collect()
    }

    pub fn state(&self, key: &BufferKey) -> Option<FieldSave> {
        self.m.state(key).cloned()
    }

    pub fn entry(&self, key: &BufferKey) -> Option<BufferEntry> {
        buffer::read(&self.store, key)
    }

    /// The navigation gate as the glue runs it, with `answer` playing the server for each
    /// save it waits for. `during` runs once, the first time the gate waits.
    pub fn gate(
        &mut self,
        mut answer: impl FnMut(&mut Page, InFlight) -> Answered,
        mut during: impl FnMut(&mut Page),
    ) -> bool {
        self.m.begin_gate();
        let mut waited = false;
        let passed = loop {
            let commands = self.m.flush_for_gate(self.now);
            self.run(commands);
            match self.m.gate() {
                GateState::Blocked => break false,
                GateState::Pass => break true,
                GateState::Again => {}
                GateState::Waiting => {
                    if !waited {
                        waited = true;
                        during(self);
                    }
                    let flight = self
                        .inflight
                        .pop_front()
                        .expect("the gate waits for a save");
                    let answered = answer(self, flight.clone());
                    self.answer(&flight, answered);
                }
            }
        };
        self.m.end_gate();
        passed
    }

    /// A store's answer for `flight`: its text in NFC with a fresh token.
    pub fn stored(&mut self, flight: &InFlight) -> Answered {
        self.next_token += 1;
        self.next_view += 1;
        Answered::Acked(ack(&flight.sent.text, self.next_token, self.next_view))
    }
}
