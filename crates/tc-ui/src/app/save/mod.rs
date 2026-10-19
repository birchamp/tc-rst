//! The save machine (APP-DESIGN 6.5, AD4, AD20, AD37): a port of tC4's `SaveScheduler`
//! (`tC4:src/data/saveScheduler.ts`), one state per field, decided without touching anything.
//!
//! Every method takes the time and returns the [`Command`]s to carry out: buffer writes, the
//! mirror's lines, saves, retries, drafts, and a value to show in an editor. The browser glue
//! (`web::saves`) executes them, and tc-app's tests execute them against the real
//! `test/fieldStore` with reloads and crashes at every point.
//!
//! The rules that keep a translator's text:
//!
//! - **One save in flight per field**, its id and whole body in the buffer entry before it is
//!   sent. The next save waits for the reply and names its tokens.
//! - **Clean only on the stored text**: a field is clean when `Ack.saved` equals the NFC form
//!   of its current text, never because the text that was sent was answered (A-2).
//! - **A lost reply is sent again with the same id and body**, which the reply cache answers.
//!   Everything else that sends again (Retry, the gate's retry, a reattach, a reissue) mints a
//!   new id, because the cache keeps refusals too and would answer the old id with them for
//!   ever.
//! - **A failure stops automatic saves** until Retry (FR-32); typing goes on into the buffer.
//! - **A value and its token are one pair** (AD37): a refresh replaces both or neither, never
//!   while the field has had input since it took its pair, and never into a focused field.
//! - **A restart drops every field and sends nothing**: the new run has no reply cache, and
//!   the entries wait for the offer at their project's next open.

pub mod classify;
pub mod indicator;
pub mod rules;

use std::collections::BTreeMap;

use tc_api::API_VERSION;
use tc_api::app::OpenSession;
use tc_api::buffer::{BufferEntry, BufferKey, SentSave};
use tc_api::event::SessionMode;
use tc_api::ids::{RequestId, Scope, ServerInstance, ViewSeq, WallMs};
use tc_api::shown::PersonLabel;
use tc_api::token::ShownToken;
use tc_api::write::ReadOnlyReason;
use tc_api::{ApiError, NotSaved};

use self::classify::{Answered, SaveError};
use self::indicator::Worst;
use self::rules::{DraftRule, FieldRules, SaveRule, rules};
use crate::app::buffer::same_text;

/// Milliseconds of `performance.now()`.
pub type Ms = f64;

/// The first wait before a lost reply is sent again; it doubles up to [`RETRY_MAX_MS`].
pub const RETRY_FIRST_MS: Ms = 250.0;
pub const RETRY_MAX_MS: Ms = 5000.0;
/// A request id that keeps coming back as reused is a bug: after this many new ids the field
/// shows the failure instead of sending for ever.
const MAX_REISSUES: u8 = 3;
/// How many of a field's own answered requests it remembers, to know its own `Changed`.
const OWN_KEPT: usize = 8;

/// The value an editor shows and the token it stands for, taken together (AD37).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pair {
    pub value: String,
    pub token: ShownToken,
    /// The session view the read was made at; `None` for a read that has none (a test field).
    pub view: Option<ViewSeq>,
}

/// A field's save state (6.5).
#[derive(Debug, Clone, PartialEq)]
pub enum FieldSave {
    Clean,
    /// Unsaved text, since this time.
    Dirty {
        since: Ms,
    },
    /// One request in flight, or a lost reply that is being sent again.
    Saving {
        sent: SentSave,
    },
    /// Nothing saves until Retry or the navigation gate's one retry (FR-32).
    Failed(SaveError),
    /// The text stays and cannot be saved now.
    Held(Hold),
    /// Clean, and the disk reported an error after the rename (STORAGE:1424-1428).
    SavedWithWarning(String),
}

/// Why a field cannot save now.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Hold {
    /// Its project has no open in this run: it was closed, or reopened under another id.
    NoOpen,
    /// Its project is open read-only (A-10): the text stays for the offered copy.
    ReadOnly(ReadOnlyReason),
}

/// "Juan changed this verse while you were typing. Your text will be kept as a second
/// version." (STORAGE:2978-2981)
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Notice {
    pub by: Option<PersonLabel>,
}

/// Where a text change came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Source {
    /// The person typed in an editor that `boot.js` tracks: its capture listener already wrote
    /// the entry and its mirror line before the WebAssembly saw the event.
    Input,
    /// The program changed the text: the machine writes the entry itself.
    Program,
}

/// Which open a field's saves go to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FieldScope {
    /// A field of the app, which needs no open (the test fields).
    App,
    /// A field of a project, at its current open; `None` while it has none.
    Project(Option<Scope>),
}

/// One thing for the glue to do, in order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    /// Write the field's buffer entry, and send its line to the shell's mirror when `mirror`.
    /// The glue stamps `at` with the wall clock and `draft` with the writer's draft call.
    Write {
        key: BufferKey,
        entry: BufferEntry,
        mirror: bool,
    },
    /// Remove the field's entry if its text is `if_text` in NFC (any text for `None`), and send
    /// the removal to the mirror when `mirror`.
    Remove {
        key: BufferKey,
        if_text: Option<String>,
        mirror: bool,
    },
    /// Send a save with a new id; the entry naming it was written by the `Write` before.
    Send {
        key: BufferKey,
        sent: SentSave,
        base: Vec<ShownToken>,
    },
    /// Send a save whose reply was lost, with the same id and body. The glue first checks with
    /// `app/hello` that the server run is the one the field sent to: a run that started again
    /// has no reply cache, and the resend would store the text twice.
    Resend {
        key: BufferKey,
        sent: SentSave,
        base: Vec<ShownToken>,
    },
    /// Send the field's draft (STORAGE §5.14). Best effort: its answer changes nothing here.
    Draft {
        key: BufferKey,
        text: String,
        base: Vec<ShownToken>,
    },
    /// Discard the field's server draft (Cancel of a section card).
    DiscardDraft { key: BufferKey },
    /// Show this text in the field's editor, which is not focused.
    Show { key: BufferKey, text: String },
}

/// Where the navigation gate stands (6.5, D9 of the A4b plan).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GateState {
    /// A save is in flight: wait for its reply.
    Waiting,
    /// A field may save now: flush again.
    Again,
    /// Every field that may save is saved.
    Pass,
    /// A failure stands after its one retry: stay where you are (FR-32).
    Blocked,
}

#[derive(Debug, Clone, Copy)]
struct Editor {
    focused: bool,
}

#[derive(Debug, Clone)]
struct Field {
    rules: FieldRules,
    scope: FieldScope,
    current: String,
    /// The stored text that `base` stands for, when known. A field restored after a reload
    /// does not know it until a read or an `Ack` says.
    persisted: Option<String>,
    base: Vec<ShownToken>,
    /// The view of the pair or `Ack` the field last took.
    pair_view: Option<ViewSeq>,
    /// The newest view of a `Changed` that its own request caused.
    own_view: Option<ViewSeq>,
    own: Vec<RequestId>,
    state: FieldSave,
    editor: Option<Editor>,
    /// Input or composition since the field last took its pair (AD37).
    touched: bool,
    /// When the next save is due.
    due: Option<Ms>,
    /// When a lost reply is sent again.
    retry_at: Option<Ms>,
    retry_gap: Ms,
    reissues: u8,
    draft_due: Option<Ms>,
    draft_text: Option<String>,
    /// A line for this field went to the mirror and no removal followed.
    mirrored: bool,
    notice: Option<Notice>,
    /// The newest refresh the field has not taken.
    waiting: Option<(Pair, Option<PersonLabel>)>,
    /// The navigation gate has retried this field's failure, or the failure came during it.
    gate_retried: bool,
    /// A save was asked for and the field had no open: send it once an open is listed.
    send_on_open: bool,
}

impl Field {
    fn new(kind_rules: FieldRules, scope: FieldScope, pair: Pair) -> Self {
        Self {
            rules: kind_rules,
            scope,
            current: pair.value.clone(),
            persisted: Some(pair.value),
            base: vec![pair.token],
            pair_view: pair.view,
            own_view: None,
            own: Vec::new(),
            state: FieldSave::Clean,
            editor: None,
            touched: false,
            due: None,
            retry_at: None,
            retry_gap: RETRY_FIRST_MS,
            reissues: 0,
            draft_due: None,
            draft_text: None,
            mirrored: false,
            notice: None,
            waiting: None,
            gate_retried: false,
            send_on_open: false,
        }
    }

    fn focused(&self) -> bool {
        self.editor.is_some_and(|editor| editor.focused)
    }

    /// The text differs from the stored text in NFC, or the stored text is not known.
    fn is_dirty(&self) -> bool {
        self.persisted
            .as_deref()
            .is_none_or(|stored| !same_text(&self.current, stored))
    }

    fn is_clean_state(&self) -> bool {
        matches!(
            self.state,
            FieldSave::Clean | FieldSave::SavedWithWarning(_)
        )
    }

    fn is_saving(&self) -> bool {
        matches!(self.state, FieldSave::Saving { .. })
    }

    /// Whether a refresh is newer than what the field holds.
    fn is_newer(&self, pair: &Pair) -> bool {
        let held = self.pair_view.max(self.own_view);
        match (pair.view, held) {
            (Some(view), Some(held)) => view > held,
            (Some(_), None) => true,
            (None, _) => self.base.first() != Some(&pair.token),
        }
    }

    /// Whether a refresh shows another version than the one the field is based on.
    fn differs_from_base(&self, pair: &Pair) -> bool {
        match &self.persisted {
            Some(stored) => !same_text(&pair.value, stored),
            None => self.base.first() != Some(&pair.token),
        }
    }

    /// A read of the version the field is based on says what that version's text is.
    fn learn(&mut self, pair: &Pair) {
        if self.persisted.is_none() && self.base.first() == Some(&pair.token) {
            self.persisted = Some(pair.value.clone());
        }
    }

    fn note_waiting(&mut self) {
        if self.is_dirty()
            && let Some((pair, by)) = &self.waiting
            && self.differs_from_base(pair)
        {
            self.notice = Some(Notice { by: by.clone() });
        }
    }

    fn remember_own(&mut self, request: RequestId) {
        self.own.push(request);
        if self.own.len() > OWN_KEPT {
            self.own.remove(0);
        }
    }

    fn sendable_by_gate(&self) -> bool {
        self.rules.save != SaveRule::OnSaveOnly
    }

    /// A save is due now by the field's rule, after a blur or once an open is listed again.
    fn due_now_by_rule(&self, now: Ms) -> Option<Ms> {
        match self.rules.save {
            SaveRule::BlurOrIdle { .. } => Some(now),
            SaveRule::OnBlur => (!self.focused()).then_some(now),
            SaveRule::OnLeave | SaveRule::OnSaveOnly => None,
        }
    }
}

/// The context a field's transitions need from the machine.
struct Ctx<'a> {
    instance: ServerInstance,
    sessions: &'a [OpenSession],
    gating: bool,
}

/// The save machine: every field the UI holds unsaved text for, or has an editor open on.
pub struct SaveMachine {
    instance: ServerInstance,
    ids: Box<dyn FnMut() -> RequestId>,
    fields: BTreeMap<BufferKey, Field>,
    sessions: Vec<OpenSession>,
    gating: bool,
}

impl std::fmt::Debug for SaveMachine {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SaveMachine")
            .field("instance", &self.instance)
            .field("fields", &self.fields.len())
            .finish_non_exhaustive()
    }
}

impl SaveMachine {
    /// A machine for the server run `instance`; `ids` makes a fresh random request id.
    pub fn new(instance: ServerInstance, ids: Box<dyn FnMut() -> RequestId>) -> Self {
        Self {
            instance,
            ids,
            fields: BTreeMap::new(),
            sessions: Vec::new(),
            gating: false,
        }
    }

    pub fn instance(&self) -> ServerInstance {
        self.instance
    }

    /// An editor mounts on a field. A field the machine holds with unsaved text keeps its text
    /// and base, and the pair is a refresh (AD37); a clean or new one takes the pair. Returns
    /// the text the editor shows.
    pub fn open(
        &mut self,
        key: BufferKey,
        scope: FieldScope,
        pair: Pair,
        now: Ms,
    ) -> (String, Vec<Command>) {
        let mut out = Vec::new();
        if let Some(field) = self.fields.get_mut(&key) {
            field.editor = Some(Editor { focused: false });
            field.scope = scope;
            let shown = field.current.clone();
            out.extend(self.refresh(&key, pair, None));
            self.pump(now, &mut out);
            let shown = self.fields.get(&key).map_or(shown, |f| f.current.clone());
            // `Show` is for an editor already on screen; this one shows what is returned.
            out.retain(|command| !matches!(command, Command::Show { key: shown_key, .. } if *shown_key == key));
            return (shown, out);
        }
        let mut field = Field::new(rules(key.field.kind), scope, pair);
        field.editor = Some(Editor { focused: false });
        let shown = field.current.clone();
        self.fields.insert(key, field);
        (shown, out)
    }

    /// After a reload in the same server run (AD22): the entry becomes a field with no editor.
    /// A save that was in flight is sent again with its own id and body; other unsaved text
    /// waits for its rule. The glue restores only entries of this run whose project is open.
    pub fn restore(
        &mut self,
        key: BufferKey,
        entry: BufferEntry,
        scope: FieldScope,
        now: Ms,
    ) -> Vec<Command> {
        let mut out = Vec::new();
        if self.fields.contains_key(&key) || entry.instance != self.instance {
            return out;
        }
        let Some(token) = entry.base.first().cloned() else {
            return out;
        };
        let kind_rules = rules(key.field.kind);
        let mut field = Field::new(
            kind_rules,
            scope,
            Pair {
                value: entry.text.clone(),
                token,
                view: None,
            },
        );
        field.base = entry.base.clone();
        field.persisted = None;
        field.pair_view = None;
        field.touched = true;
        field.mirrored = true;
        // The usual timer, which needs no editor (6.6): a text newer than the save in flight
        // saves after it. Only a field that saves on a pause has one (trap 30 of the plan).
        field.due = match kind_rules.save {
            SaveRule::BlurOrIdle { idle_ms } => Some(now + f64::from(idle_ms)),
            SaveRule::OnBlur | SaveRule::OnLeave | SaveRule::OnSaveOnly => None,
        };
        field.draft_due = Some(match kind_rules.draft {
            DraftRule::Throttled { gap_ms } => now + f64::from(gap_ms),
            DraftRule::OnBlurAndIdle { idle_ms } => now + f64::from(idle_ms),
        });
        match entry.sent {
            Some(sent) => {
                field.state = FieldSave::Saving { sent: sent.clone() };
                out.push(Command::Send {
                    key: key.clone(),
                    sent,
                    base: entry.base,
                });
            }
            None => field.state = FieldSave::Dirty { since: now },
        }
        self.fields.insert(key, field);
        out
    }

    /// The text of a field changed.
    pub fn typed(&mut self, key: &BufferKey, text: &str, source: Source, now: Ms) -> Vec<Command> {
        let mut out = Vec::new();
        let instance = self.instance;
        let Some(field) = self.fields.get_mut(key) else {
            return out;
        };
        field.current = text.to_owned();
        field.touched = true;
        field.note_waiting();
        let dirty = field.is_dirty();
        match field.state {
            FieldSave::Clean | FieldSave::SavedWithWarning(_) | FieldSave::Dirty { .. } => {
                if !dirty {
                    field.state = FieldSave::Clean;
                    field.due = None;
                } else if !matches!(field.state, FieldSave::Dirty { .. }) {
                    field.state = FieldSave::Dirty { since: now };
                }
            }
            FieldSave::Saving { .. } | FieldSave::Failed(_) | FieldSave::Held(_) => {}
        }
        let tracked = source == Source::Input && field.focused();
        if tracked {
            field.mirrored = true;
        } else if field.is_clean_state() && !field.focused() {
            out.push(Command::Remove {
                key: key.clone(),
                if_text: Some(field.current.clone()),
                mirror: field.mirrored,
            });
            field.mirrored = false;
        } else {
            out.push(Command::Write {
                key: key.clone(),
                entry: entry_of(instance, field),
                mirror: true,
            });
            field.mirrored = true;
        }
        if dirty {
            if let SaveRule::BlurOrIdle { idle_ms } = field.rules.save {
                field.due = Some(now + f64::from(idle_ms));
            }
            match field.rules.draft {
                DraftRule::Throttled { gap_ms } => {
                    if field.draft_due.is_none() {
                        field.draft_due = Some(now + f64::from(gap_ms));
                    }
                }
                DraftRule::OnBlurAndIdle { idle_ms } => {
                    field.draft_due = Some(now + f64::from(idle_ms));
                }
            }
        }
        self.pump(now, &mut out);
        out
    }

    /// A composition started (AD37): it counts as input, though the text has not changed yet.
    pub fn composition(&mut self, key: &BufferKey) {
        if let Some(field) = self.fields.get_mut(key) {
            field.touched = true;
            field.note_waiting();
        }
    }

    pub fn focus(&mut self, key: &BufferKey, _now: Ms) -> Vec<Command> {
        if let Some(editor) = self.fields.get_mut(key).and_then(|f| f.editor.as_mut()) {
            editor.focused = true;
        }
        Vec::new()
    }

    pub fn blur(&mut self, key: &BufferKey, now: Ms) -> Vec<Command> {
        let mut out = Vec::new();
        let Some(field) = self.fields.get_mut(key) else {
            return out;
        };
        if let Some(editor) = field.editor.as_mut() {
            editor.focused = false;
        }
        match field.state {
            FieldSave::Clean | FieldSave::SavedWithWarning(_) => {
                if field.touched || field.mirrored {
                    out.push(Command::Remove {
                        key: key.clone(),
                        if_text: Some(field.current.clone()),
                        mirror: field.mirrored,
                    });
                    field.mirrored = false;
                    field.touched = false;
                }
                // The stored NFC form is shown once the person has left the field (6.3).
                if let Some(stored) = &field.persisted
                    && *stored != field.current
                {
                    field.current = stored.clone();
                    out.push(Command::Show {
                        key: key.clone(),
                        text: stored.clone(),
                    });
                }
                take_waiting(key, field, &mut out);
            }
            FieldSave::Dirty { .. } | FieldSave::Saving { .. } => {
                if let Some(due) = field.due_now_by_rule(now) {
                    field.due = Some(due);
                }
            }
            FieldSave::Failed(_) | FieldSave::Held(_) => {}
        }
        if field.is_dirty() && matches!(field.rules.draft, DraftRule::OnBlurAndIdle { .. }) {
            field.draft_due = Some(now);
        }
        self.pump(now, &mut out);
        out
    }

    /// The Save button or Done: the field saves now, whatever its rule. A failed field is
    /// retried with a new id.
    pub fn save(&mut self, key: &BufferKey, now: Ms) -> Vec<Command> {
        let mut out = Vec::new();
        let ctx = Ctx {
            instance: self.instance,
            sessions: &self.sessions,
            gating: self.gating,
        };
        let Some(field) = self.fields.get_mut(key) else {
            return out;
        };
        match field.state {
            FieldSave::Dirty { .. } | FieldSave::Saving { .. } => field.due = Some(now),
            FieldSave::Failed(_) => retry_field(key, field, &mut self.ids, &ctx, now, &mut out),
            FieldSave::Held(_) => field.send_on_open = true,
            FieldSave::Clean | FieldSave::SavedWithWarning(_) => {}
        }
        self.pump(now, &mut out);
        out
    }

    /// Cancel of a section card: the text, its entry and its server draft are discarded.
    pub fn discard(&mut self, key: &BufferKey) -> Vec<Command> {
        if self.fields.remove(key).is_none() {
            return Vec::new();
        }
        vec![
            Command::Remove {
                key: key.clone(),
                if_text: None,
                mirror: true,
            },
            Command::DiscardDraft { key: key.clone() },
        ]
    }

    /// The editor unmounts. Leaving a field saves it as a blur does, and a check comment as
    /// Done does (A-10); a section card's text stays a draft. A clean field is forgotten.
    pub fn close(&mut self, key: &BufferKey, now: Ms) -> Vec<Command> {
        let mut out = self.blur(key, now);
        let Some(field) = self.fields.get_mut(key) else {
            return out;
        };
        field.editor = None;
        field.notice = None;
        if field.is_clean_state() {
            self.fields.remove(key);
            return out;
        }
        if field.rules.save == SaveRule::OnLeave
            && matches!(
                field.state,
                FieldSave::Dirty { .. } | FieldSave::Saving { .. }
            )
        {
            field.due = Some(now);
        }
        self.pump(now, &mut out);
        out
    }

    /// Time passed: due saves, lost replies and drafts go.
    pub fn tick(&mut self, now: Ms) -> Vec<Command> {
        let mut out = Vec::new();
        self.pump(now, &mut out);
        out
    }

    /// A write's answer. One for a request the field is not waiting for is ignored.
    pub fn reply(
        &mut self,
        key: &BufferKey,
        request: RequestId,
        answered: Answered,
        now: Ms,
    ) -> Vec<Command> {
        let mut out = Vec::new();
        let ctx = Ctx {
            instance: self.instance,
            sessions: &self.sessions,
            gating: self.gating,
        };
        let Some(field) = self.fields.get_mut(key) else {
            return out;
        };
        match &field.state {
            FieldSave::Saving { sent } if sent.request == request => {}
            _ => return out,
        }
        field.retry_at = None;
        match answered {
            Answered::Acked(ack) => {
                let ([saved], [token]) = (ack.saved.as_slice(), ack.tokens.as_slice()) else {
                    fail(
                        key,
                        field,
                        SaveError::BadAck("an Ack for one field holds one value and one token"),
                        &ctx,
                        &mut out,
                    );
                    self.pump(now, &mut out);
                    return out;
                };
                field.reissues = 0;
                field.base = vec![token.token.clone()];
                field.persisted = Some(saved.text.clone());
                field.pair_view = field.pair_view.max(Some(ack.view));
                field.remember_own(request);
                if field
                    .waiting
                    .as_ref()
                    .is_some_and(|(pair, _)| !field.is_newer(pair))
                {
                    field.waiting = None;
                }
                if same_text(&saved.text, &field.current) {
                    field.state = match ack.disk_warning {
                        Some(warning) => FieldSave::SavedWithWarning(warning),
                        None => FieldSave::Clean,
                    };
                    field.due = None;
                    field.draft_due = None;
                    field.draft_text = None;
                    field.touched = false;
                    out.push(Command::Remove {
                        key: key.clone(),
                        if_text: Some(field.current.clone()),
                        mirror: field.mirrored,
                    });
                    field.mirrored = false;
                    if !field.focused() {
                        // A focused field keeps the text as typed; the stored NFC form shows
                        // once the person has left it (6.3).
                        if saved.text != field.current {
                            field.current = saved.text.clone();
                            out.push(Command::Show {
                                key: key.clone(),
                                text: saved.text.clone(),
                            });
                        }
                        take_waiting(key, field, &mut out);
                    }
                } else {
                    field.state = FieldSave::Dirty { since: now };
                    field.note_waiting();
                    out.push(Command::Write {
                        key: key.clone(),
                        entry: entry_of(ctx.instance, field),
                        mirror: true,
                    });
                    field.mirrored = true;
                }
            }
            Answered::Lost => {
                field.retry_at = Some(now + field.retry_gap);
                field.retry_gap = (field.retry_gap * 2.0).min(RETRY_MAX_MS);
            }
            Answered::Reattach => match field.scope {
                FieldScope::App => fail(
                    key,
                    field,
                    SaveError::NotSaved(NotSaved::StaleScope),
                    &ctx,
                    &mut out,
                ),
                FieldScope::Project(_) => {
                    field.send_on_open = true;
                    attach(key, field, &mut self.ids, &ctx, now, &mut out);
                }
            },
            Answered::Reissue => {
                field.reissues += 1;
                if field.reissues > MAX_REISSUES {
                    fail(
                        key,
                        field,
                        SaveError::Api(ApiError::RequestReused),
                        &ctx,
                        &mut out,
                    );
                } else {
                    send(key, field, &mut self.ids, &ctx, &mut out);
                }
            }
            Answered::Failed(SaveError::NotSaved(NotSaved::ReadOnly(reason))) => {
                field.state = FieldSave::Held(Hold::ReadOnly(reason));
                field.send_on_open = true;
                out.push(Command::Write {
                    key: key.clone(),
                    entry: entry_of(ctx.instance, field),
                    mirror: true,
                });
            }
            Answered::Failed(error) => fail(key, field, error, &ctx, &mut out),
        }
        self.pump(now, &mut out);
        out
    }

    /// The Retry button: every failed field sends its current text with a new id. A field
    /// already saving is left alone, so ten presses send one request (tC4 #356).
    pub fn retry(&mut self, now: Ms) -> Vec<Command> {
        let mut out = Vec::new();
        let ctx = Ctx {
            instance: self.instance,
            sessions: &self.sessions,
            gating: self.gating,
        };
        for (key, field) in &mut self.fields {
            if matches!(field.state, FieldSave::Failed(_)) {
                retry_field(key, field, &mut self.ids, &ctx, now, &mut out);
            }
        }
        self.pump(now, &mut out);
        out
    }

    /// A refresh of a field's value under an open editor or a held field (AD37, 6.3): taken
    /// whole by a field with no input since it took its pair (on blur when focused), kept
    /// back with the notice from a field the person has typed in.
    pub fn refresh(
        &mut self,
        key: &BufferKey,
        pair: Pair,
        by: Option<PersonLabel>,
    ) -> Vec<Command> {
        let mut out = Vec::new();
        let Some(field) = self.fields.get_mut(key) else {
            return out;
        };
        field.learn(&pair);
        if matches!(field.state, FieldSave::Dirty { .. }) && !field.is_dirty() {
            // The read showed that the text is the stored text.
            field.state = FieldSave::Clean;
            field.due = None;
            out.push(Command::Remove {
                key: key.clone(),
                if_text: Some(field.current.clone()),
                mirror: field.mirrored,
            });
            field.mirrored = false;
        }
        if !field.is_newer(&pair) {
            return out;
        }
        let newer_than_waiting =
            field
                .waiting
                .as_ref()
                .is_none_or(|(held, _)| match (pair.view, held.view) {
                    (Some(view), Some(held)) => view >= held,
                    _ => true,
                });
        if !newer_than_waiting {
            return out;
        }
        field.waiting = Some((pair, by));
        if field.is_saving() {
            // Judged when the reply has come: it may be this field's own save.
            return out;
        }
        if !field.is_clean_state() {
            field.note_waiting();
            return out;
        }
        if !field.focused() {
            take_waiting(key, field, &mut out);
        }
        out
    }

    /// `Changed { cause: LocalSave { request } }`: a refresh up to `view` is this field's own.
    pub fn local_change(&mut self, request: RequestId, view: ViewSeq) {
        for field in self.fields.values_mut() {
            let in_flight =
                matches!(&field.state, FieldSave::Saving { sent } if sent.request == request);
            if in_flight || field.own.contains(&request) {
                field.own_view = field.own_view.max(Some(view));
            }
        }
    }

    /// The open sessions, from `hello` and the session events: fields reattach to the current
    /// open of their folder and project id, and hold while it is read-only or absent.
    pub fn sessions(&mut self, open: &[OpenSession], now: Ms) -> Vec<Command> {
        let mut out = Vec::new();
        self.sessions = open.to_vec();
        let ctx = Ctx {
            instance: self.instance,
            sessions: &self.sessions,
            gating: self.gating,
        };
        for (key, field) in &mut self.fields {
            if !matches!(field.scope, FieldScope::Project(_)) {
                continue;
            }
            match field.state {
                FieldSave::Saving { .. } => {
                    // The reply decides; the next save goes to the current open.
                    field.scope =
                        FieldScope::Project(session_of(key, ctx.sessions).map(|s| s.at.clone()));
                }
                FieldSave::Clean | FieldSave::SavedWithWarning(_) => {
                    field.scope =
                        FieldScope::Project(session_of(key, ctx.sessions).map(|s| s.at.clone()));
                }
                FieldSave::Dirty { .. } | FieldSave::Held(_) => {
                    attach(key, field, &mut self.ids, &ctx, now, &mut out);
                }
                FieldSave::Failed(_) => {
                    field.scope =
                        FieldScope::Project(session_of(key, ctx.sessions).map(|s| s.at.clone()));
                }
            }
        }
        self.pump(now, &mut out);
        out
    }

    /// The server started again: every field is dropped and nothing is sent (AD4). The
    /// entries stay for the offer at their project's next open, and a reply that comes later
    /// finds no field.
    pub fn restarted(&mut self, instance: ServerInstance) {
        self.instance = instance;
        self.fields.clear();
        self.sessions.clear();
        self.gating = false;
    }

    /// The navigation gate starts: each standing failure gets one retry in it.
    pub fn begin_gate(&mut self) {
        self.gating = true;
        for field in self.fields.values_mut() {
            field.gate_retried = false;
        }
    }

    pub fn end_gate(&mut self) {
        self.gating = false;
    }

    /// Sends every field that may save now, and retries each failure once (tC4's `drain`). A
    /// section buffer is never saved by the gate (A-1).
    pub fn flush_for_gate(&mut self, now: Ms) -> Vec<Command> {
        let mut out = Vec::new();
        let ctx = Ctx {
            instance: self.instance,
            sessions: &self.sessions,
            gating: self.gating,
        };
        for (key, field) in &mut self.fields {
            if !field.sendable_by_gate() {
                continue;
            }
            match field.state {
                FieldSave::Failed(_) if !field.gate_retried => {
                    field.gate_retried = true;
                    retry_field(key, field, &mut self.ids, &ctx, now, &mut out);
                }
                FieldSave::Dirty { .. } => send(key, field, &mut self.ids, &ctx, &mut out),
                _ => {}
            }
        }
        out
    }

    /// Where the gate stands. A held field and a section buffer never block it: the person
    /// cannot save the one, and the other is saved only by its Save (D9 of the A4b plan).
    pub fn gate(&self) -> GateState {
        let fields = || self.fields.values().filter(|f| f.sendable_by_gate());
        if fields().any(|f| matches!(f.state, FieldSave::Failed(_)) && f.gate_retried) {
            return GateState::Blocked;
        }
        if self.fields.values().any(Field::is_saving) {
            return GateState::Waiting;
        }
        if fields().any(|f| {
            matches!(f.state, FieldSave::Dirty { .. })
                || matches!(f.state, FieldSave::Failed(_)) && !f.gate_retried
        }) {
            return GateState::Again;
        }
        GateState::Pass
    }

    pub fn state(&self, key: &BufferKey) -> Option<&FieldSave> {
        self.fields.get(key).map(|f| &f.state)
    }

    pub fn text(&self, key: &BufferKey) -> Option<&str> {
        self.fields.get(key).map(|f| f.current.as_str())
    }

    pub fn notice(&self, key: &BufferKey) -> Option<&Notice> {
        self.fields.get(key).and_then(|f| f.notice.as_ref())
    }

    /// The tokens the field's next save names.
    pub fn base(&self, key: &BufferKey) -> Option<&[ShownToken]> {
        self.fields.get(key).map(|f| f.base.as_slice())
    }

    pub fn keys(&self) -> impl Iterator<Item = &BufferKey> {
        self.fields.keys()
    }

    /// The entry `boot.js` writes when the person types into an editor whose field has none:
    /// the first keystroke after a clean save, or the first of all (`text` and `at` are
    /// filled in by `boot.js`).
    pub fn template(&self, key: &BufferKey) -> Option<BufferEntry> {
        let field = self.fields.get(key)?;
        Some(BufferEntry {
            api: API_VERSION,
            instance: self.instance,
            base: field.base.clone(),
            text: String::new(),
            sent: None,
            at: WallMs(0),
            draft: None,
        })
    }

    /// The worst state of all fields: an error, a save in flight, unsaved or held text, or
    /// saved.
    pub fn worst(&self) -> Worst {
        self.fields
            .values()
            .map(|field| match field.state {
                FieldSave::Failed(_) => Worst::Error,
                FieldSave::Saving { .. } => Worst::Saving,
                FieldSave::Dirty { .. } | FieldSave::Held(_) => Worst::Dirty,
                FieldSave::Clean | FieldSave::SavedWithWarning(_) => Worst::Saved,
            })
            .max()
            .unwrap_or(Worst::Saved)
    }

    /// The disk warnings of fields saved with one.
    pub fn warnings(&self) -> Vec<String> {
        self.fields
            .values()
            .filter_map(|field| match &field.state {
                FieldSave::SavedWithWarning(warning) => Some(warning.clone()),
                _ => None,
            })
            .collect()
    }

    /// When [`tick`](Self::tick) has work next.
    pub fn next_deadline(&self) -> Option<Ms> {
        self.fields
            .values()
            .flat_map(|field| {
                let save = match field.state {
                    FieldSave::Dirty { .. } => field.due,
                    FieldSave::Saving { .. } => field.retry_at,
                    _ => None,
                };
                let draft = (!field.is_saving()).then_some(field.draft_due).flatten();
                [save, draft]
            })
            .flatten()
            .reduce(f64::min)
    }

    /// The saves in flight or armed, for `window.__tcIdle` (D11 of the A4b plan). A section
    /// buffer, which saves only on Save, does not keep the page busy.
    pub fn busy(&self) -> usize {
        self.fields
            .values()
            .filter(|field| {
                field.is_saving()
                    || matches!(field.state, FieldSave::Dirty { .. }) && field.due.is_some()
            })
            .count()
    }

    /// Runs what is due at `now`: lost replies sent again, due saves, drafts.
    fn pump(&mut self, now: Ms, out: &mut Vec<Command>) {
        let ctx = Ctx {
            instance: self.instance,
            sessions: &self.sessions,
            gating: self.gating,
        };
        for (key, field) in &mut self.fields {
            match &field.state {
                FieldSave::Saving { sent } if field.retry_at.is_some_and(|at| at <= now) => {
                    let sent = sent.clone();
                    field.retry_at = None;
                    out.push(Command::Resend {
                        key: key.clone(),
                        sent,
                        base: field.base.clone(),
                    });
                }
                FieldSave::Dirty { .. } if field.due.is_some_and(|due| due <= now) => {
                    send(key, field, &mut self.ids, &ctx, out);
                }
                _ => {}
            }
            pump_draft(key, field, now, out);
        }
    }
}

/// The entry of a field as it is now, with the save in flight if there is one.
fn entry_of(instance: ServerInstance, field: &Field) -> BufferEntry {
    BufferEntry {
        api: API_VERSION,
        instance,
        base: field.base.clone(),
        text: field.current.clone(),
        sent: match &field.state {
            FieldSave::Saving { sent } => Some(sent.clone()),
            _ => None,
        },
        at: WallMs(0),
        draft: None,
    }
}

fn session_of<'a>(key: &BufferKey, sessions: &'a [OpenSession]) -> Option<&'a OpenSession> {
    sessions
        .iter()
        .find(|session| session.at.project == key.project && session.id == key.id)
}

/// Sends the field's current text with a new id: the entry with the save first (so a reload
/// sends the same request again), then the request.
fn send(
    key: &BufferKey,
    field: &mut Field,
    ids: &mut Box<dyn FnMut() -> RequestId>,
    ctx: &Ctx<'_>,
    out: &mut Vec<Command>,
) {
    let scope = match &field.scope {
        FieldScope::App => None,
        FieldScope::Project(Some(at)) => Some(at.clone()),
        FieldScope::Project(None) => {
            field.state = FieldSave::Held(Hold::NoOpen);
            field.send_on_open = true;
            field.due = None;
            return;
        }
    };
    let sent = SentSave {
        request: ids(),
        text: field.current.clone(),
        scope,
    };
    field.state = FieldSave::Saving { sent: sent.clone() };
    field.due = None;
    field.retry_at = None;
    field.retry_gap = RETRY_FIRST_MS;
    field.send_on_open = false;
    out.push(Command::Write {
        key: key.clone(),
        entry: entry_of(ctx.instance, field),
        mirror: true,
    });
    field.mirrored = true;
    out.push(Command::Send {
        key: key.clone(),
        sent,
        base: field.base.clone(),
    });
}

/// Retry of a failed field: its text, if still unsaved, with a new id.
fn retry_field(
    key: &BufferKey,
    field: &mut Field,
    ids: &mut Box<dyn FnMut() -> RequestId>,
    ctx: &Ctx<'_>,
    _now: Ms,
    out: &mut Vec<Command>,
) {
    if field.is_dirty() {
        send(key, field, ids, ctx, out);
    } else {
        field.state = FieldSave::Clean;
        out.push(Command::Remove {
            key: key.clone(),
            if_text: Some(field.current.clone()),
            mirror: field.mirrored,
        });
        field.mirrored = false;
    }
}

/// A field without a save in flight meets the current sessions: it goes to its folder's open
/// with the same project id, holds while that open is read-only or absent, and sends at once
/// if a save was asked for while it had none.
fn attach(
    key: &BufferKey,
    field: &mut Field,
    ids: &mut Box<dyn FnMut() -> RequestId>,
    ctx: &Ctx<'_>,
    now: Ms,
    out: &mut Vec<Command>,
) {
    let held_before = matches!(field.state, FieldSave::Held(_));
    match session_of(key, ctx.sessions) {
        None => {
            field.scope = FieldScope::Project(None);
            if !matches!(field.state, FieldSave::Held(Hold::NoOpen)) {
                field.state = FieldSave::Held(Hold::NoOpen);
                field.due = None;
                out.push(Command::Write {
                    key: key.clone(),
                    entry: entry_of(ctx.instance, field),
                    mirror: true,
                });
            }
        }
        Some(session) => {
            field.scope = FieldScope::Project(Some(session.at.clone()));
            match session.mode {
                SessionMode::ReadOnly(reason) => {
                    if field.state != FieldSave::Held(Hold::ReadOnly(reason)) {
                        field.state = FieldSave::Held(Hold::ReadOnly(reason));
                        field.due = None;
                        out.push(Command::Write {
                            key: key.clone(),
                            entry: entry_of(ctx.instance, field),
                            mirror: true,
                        });
                    }
                }
                SessionMode::Writable => {
                    if field.send_on_open {
                        // Base token, new id, the current open (A-19).
                        send(key, field, ids, ctx, out);
                    } else if held_before {
                        field.state = FieldSave::Dirty { since: now };
                        field.due = field.due_now_by_rule(now);
                    }
                }
            }
        }
    }
}

fn fail(
    key: &BufferKey,
    field: &mut Field,
    error: SaveError,
    ctx: &Ctx<'_>,
    out: &mut Vec<Command>,
) {
    field.state = FieldSave::Failed(error);
    field.due = None;
    // A failure that comes during the gate is not retried by it (tC4's `drain`).
    if ctx.gating {
        field.gate_retried = true;
    }
    // The entry no longer names a save in flight: a reload must not repeat a refused request.
    out.push(Command::Write {
        key: key.clone(),
        entry: entry_of(ctx.instance, field),
        mirror: true,
    });
    field.mirrored = true;
}

/// A clean field that is not focused takes the newest refresh it has not taken.
fn take_waiting(key: &BufferKey, field: &mut Field, out: &mut Vec<Command>) {
    if field.is_saving() || !field.is_clean_state() || field.focused() {
        return;
    }
    let Some((pair, _)) = field.waiting.take() else {
        return;
    };
    if !field.is_newer(&pair) {
        return;
    }
    field.current = pair.value.clone();
    field.persisted = Some(pair.value.clone());
    field.base = vec![pair.token];
    field.pair_view = field.pair_view.max(pair.view);
    field.touched = false;
    field.notice = None;
    out.push(Command::Show {
        key: key.clone(),
        text: pair.value,
    });
}

fn pump_draft(key: &BufferKey, field: &mut Field, now: Ms, out: &mut Vec<Command>) {
    let Some(due) = field.draft_due else {
        return;
    };
    if due > now || field.is_saving() {
        return;
    }
    let unsaved = field.is_dirty()
        && matches!(
            field.state,
            FieldSave::Dirty { .. } | FieldSave::Failed(_) | FieldSave::Held(_)
        );
    if !unsaved || field.draft_text.as_deref() == Some(field.current.as_str()) {
        field.draft_due = None;
        return;
    }
    out.push(Command::Draft {
        key: key.clone(),
        text: field.current.clone(),
        base: field.base.clone(),
    });
    field.draft_text = Some(field.current.clone());
    field.draft_due = match field.rules.draft {
        DraftRule::Throttled { gap_ms } => Some(now + f64::from(gap_ms)),
        DraftRule::OnBlurAndIdle { .. } => None,
    };
}
