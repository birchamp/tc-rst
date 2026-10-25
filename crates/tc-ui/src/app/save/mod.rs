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
//! - **A restart sends nothing**: the new run has no reply cache, and the entries wait for the
//!   offer at their project's next open. A field with an editor still on screen stays, held,
//!   so what is typed into it is still buffered; every other field is dropped.
//! - **`StaleScope` never goes back to the open that refused it** (A-19): the field waits until
//!   the page knows its project's new open, and asks for the sessions again.

pub mod classify;
pub mod indicator;
pub mod rules;

use std::collections::{BTreeMap, HashMap};

use tc_api::API_VERSION;
use tc_api::app::OpenSession;
use tc_api::buffer::{BufferEntry, BufferKey, FieldKind, SentSave};
use tc_api::event::SessionMode;
use tc_api::ids::{Epoch, RequestId, Scope, ServerInstance, ViewSeq, WallMs};
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

/// A session view and the open it belongs to. `ViewSeq` counts per open (3.2), so two views
/// compare by their open first: a folder's epochs only grow in one server run, and a restart
/// drops every field that could hold a view of the run before. A field of the app (a test
/// field) has no open, and its views compare by their count alone.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct View {
    pub epoch: Option<Epoch>,
    pub seq: ViewSeq,
}

impl View {
    /// A view of the open `at`, or of no open.
    pub fn of(at: Option<&Scope>, seq: ViewSeq) -> Self {
        Self {
            epoch: at.map(|at| at.epoch),
            seq,
        }
    }
}

/// The value an editor shows and the token it stands for, taken together (AD37).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pair {
    pub value: String,
    pub token: ShownToken,
    /// The session view the read was made at, with its open; `None` for a read that has none
    /// (a test field).
    pub view: Option<View>,
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
    /// The server started again under the field, whose editor is still on screen: its text is
    /// buffered and waits for the offer at its project's next open, never saved by this page
    /// (AD4). Closing the editor forgets the field.
    Restarted,
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
    /// Remove the field's entry of the run `instance` if its text is `if_text` in NFC (any text
    /// for `None`), and send the removal to the mirror when `mirror`.
    Remove {
        key: BufferKey,
        instance: ServerInstance,
        if_text: Option<String>,
        mirror: bool,
    },
    /// Send a save with a new id; the entry naming it was written by the `Write` before.
    Send {
        key: BufferKey,
        sent: SentSave,
        base: Vec<ShownToken>,
    },
    /// Send a save whose reply was lost, with the same id and body, to the run `instance` it
    /// went to. The glue first asks `app/hello` and sends only on
    /// [`SaveMachine::resend_check`]'s word: a run that started again has no reply cache, and
    /// the resend would store the text twice.
    Resend {
        key: BufferKey,
        sent: SentSave,
        base: Vec<ShownToken>,
        instance: ServerInstance,
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
    /// Read the open sessions again (`app/hello`): a save was refused as `StaleScope` by the
    /// open the page still holds, so the page has not heard of its project's new open yet.
    ReadSessions,
}

/// What a lost reply's resend does once `app/hello` has named the server's run (6.5).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ResendCheck {
    /// The run the request went to answers, and the field still waits for this request.
    Send,
    /// Another run answers: the server started again (AD4). The runtime's one restart path
    /// takes it, which changes nothing when the page already knows.
    Restarted(ServerInstance),
    /// The field no longer waits for this request, or the page has moved on to another run.
    #[default]
    Nothing,
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
    /// The server run the field's entry belongs to: the machine's, or, for a field kept over a
    /// restart, the run before, whose unsaved edit it goes on with (AD4).
    instance: ServerInstance,
    /// The save in flight when the server started again, kept in the entry of a field kept over
    /// the restart: its answer never comes, and the offer at the next open needs it (6.6).
    unanswered: Option<SentSave>,
    current: String,
    /// The stored text that `base` stands for, when known. A field restored after a reload
    /// does not know it until a read or an `Ack` says.
    persisted: Option<String>,
    base: Vec<ShownToken>,
    /// The view of the pair or `Ack` the field last took.
    pair_view: Option<View>,
    /// The newest view of a `Changed` that its own request caused.
    own_view: Option<View>,
    own: Vec<RequestId>,
    /// The open that last answered a save of this field with `StaleScope`. An epoch is never
    /// reused, so no save goes to it again (A-19).
    refused: Option<Scope>,
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
    /// The newest refresh the field has not taken, with the tokens of the other records it
    /// read (a section card's other verses, as `open_over`'s `rest`).
    waiting: Option<(Pair, Vec<ShownToken>, Option<PersonLabel>)>,
    /// The navigation gate has retried this field's failure, or the failure came during it.
    gate_retried: bool,
    /// A save was asked for and the field had no open: send it once an open is listed.
    send_on_open: bool,
}

impl Field {
    fn new(
        kind_rules: FieldRules,
        scope: FieldScope,
        instance: ServerInstance,
        pair: Pair,
    ) -> Self {
        Self::over(
            kind_rules,
            scope,
            instance,
            pair.value,
            vec![pair.token],
            pair.view,
        )
    }

    fn over(
        kind_rules: FieldRules,
        scope: FieldScope,
        instance: ServerInstance,
        value: String,
        base: Vec<ShownToken>,
        view: Option<View>,
    ) -> Self {
        Self {
            rules: kind_rules,
            scope,
            instance,
            unanswered: None,
            current: value.clone(),
            persisted: Some(value),
            base,
            pair_view: view,
            own_view: None,
            own: Vec::new(),
            refused: None,
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

    fn is_kept_over_restart(&self) -> bool {
        self.state == FieldSave::Held(Hold::Restarted)
    }

    fn is_saving(&self) -> bool {
        matches!(self.state, FieldSave::Saving { .. })
    }

    /// A save was refused as `StaleScope` by the open the page still holds for the project: the
    /// field waits for the sessions to name another open, or none (A-19).
    fn awaits_open(&self) -> bool {
        self.send_on_open
            && matches!(self.state, FieldSave::Dirty { .. })
            && matches!(&self.scope, FieldScope::Project(Some(at)) if self.refused.as_ref() == Some(at))
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
            && let Some((pair, _, by)) = &self.waiting
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
    sessions: &'a [OpenSession],
    gating: bool,
}

/// The save machine: every field the UI holds unsaved text for, or has an editor open on.
pub struct SaveMachine {
    instance: ServerInstance,
    ids: Box<dyn FnMut() -> RequestId>,
    fields: BTreeMap<BufferKey, Field>,
    /// A blur can be acknowledged before the read cache learns a newly minted comment.
    /// Keep its proved pair outside active fields so timers, gates and indicators never
    /// scan comments visited earlier in this open. The outer map has one entry per open.
    comment_pairs: HashMap<Option<Scope>, HashMap<BufferKey, Pair>>,
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
            comment_pairs: HashMap::new(),
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
        self.open_over(key, scope, pair, Vec::new(), now)
    }

    /// A new user comment has no record yet. Its first Ack supplies the token that later
    /// edits revise; remounting an unread/new box must retain that learned identity.
    pub fn open_comment(&mut self, key: BufferKey, scope: FieldScope) -> String {
        self.reopen_comment(&key, &scope);
        let field = self.fields.entry(key).or_insert_with(|| {
            Field::over(
                rules(FieldKind::Comment),
                scope.clone(),
                self.instance,
                String::new(),
                Vec::new(),
                None,
            )
        });
        field.scope = scope;
        field.editor = Some(Editor { focused: false });
        field.current.clone()
    }

    /// A saved comment cannot be retracted by clearing its box. Prefer the save already
    /// on its way over an older render's text, so a clear cannot revert that save.
    pub fn revert_comment(&mut self, key: &BufferKey, now: Ms) -> (String, Vec<Command>) {
        let value = self
            .fields
            .get(key)
            .map(|field| match &field.state {
                FieldSave::Saving { sent } => sent.text.clone(),
                _ => field.persisted.clone().unwrap_or_default(),
            })
            .unwrap_or_default();
        let commands = self.typed(key, &value, Source::Program, now);
        (value, commands)
    }

    /// As [`open`](Self::open) for a field that displays several records, a section card: its
    /// saves and drafts are based on the tokens of all of them, `pair.token` first and `rest`
    /// after, in the order the card shows the verses. A field the machine already holds keeps
    /// the base it has, as `open` does, unless it takes the read whole.
    pub fn open_over(
        &mut self,
        key: BufferKey,
        scope: FieldScope,
        pair: Pair,
        rest: Vec<ShownToken>,
        now: Ms,
    ) -> (String, Vec<Command>) {
        let mut out = Vec::new();
        self.reopen_comment(&key, &scope);
        if let Some(field) = self.fields.get_mut(&key) {
            field.editor = Some(Editor { focused: false });
            field.scope = scope;
            let shown = field.current.clone();
            out.extend(self.refresh_over(&key, pair, rest, None));
            self.pump(now, &mut out);
            let shown = self.fields.get(&key).map_or(shown, |f| f.current.clone());
            // `Show` is for an editor already on screen; this one shows what is returned.
            out.retain(|command| !matches!(command, Command::Show { key: shown_key, .. } if *shown_key == key));
            return (shown, out);
        }
        let mut field = Field::new(rules(key.field.kind), scope, self.instance, pair);
        field.base.extend(rest);
        field.editor = Some(Editor { focused: false });
        let shown = field.current.clone();
        self.fields.insert(key, field);
        (shown, out)
    }

    fn reopen_comment(&mut self, key: &BufferKey, scope: &FieldScope) {
        if key.field.kind != FieldKind::Comment || self.fields.contains_key(key) {
            return;
        }
        let at = match scope {
            FieldScope::App => None,
            FieldScope::Project(Some(at)) => Some(at.clone()),
            FieldScope::Project(None) => return,
        };
        if let Some(pair) = self
            .comment_pairs
            .get_mut(&at)
            .and_then(|pairs| pairs.remove(key))
        {
            self.fields.insert(
                key.clone(),
                Field::new(
                    rules(FieldKind::Comment),
                    scope.clone(),
                    self.instance,
                    pair,
                ),
            );
        }
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
        if entry.base.is_empty() && key.field.kind != FieldKind::Comment {
            return out;
        }
        let kind_rules = rules(key.field.kind);
        let mut field = Field::over(
            kind_rules,
            scope,
            self.instance,
            entry.text.clone(),
            entry.base.clone(),
            None,
        );
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
            out.push(remove_of(key, field));
            field.mirrored = false;
        } else {
            out.push(Command::Write {
                key: key.clone(),
                entry: entry_of(field),
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
                    out.push(remove_of(key, field));
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
        let Some(field) = self.fields.get_mut(key) else {
            return out;
        };
        match field.state {
            FieldSave::Dirty { .. } | FieldSave::Saving { .. } => field.due = Some(now),
            FieldSave::Failed(_) => retry_field(key, field, &mut self.ids, now, &mut out),
            FieldSave::Held(_) => field.send_on_open = true,
            FieldSave::Clean | FieldSave::SavedWithWarning(_) => {}
        }
        self.pump(now, &mut out);
        out
    }

    /// Cancel of a section card: the text, its entry and its server draft are discarded.
    pub fn discard(&mut self, key: &BufferKey) -> Vec<Command> {
        let Some(field) = self.fields.remove(key) else {
            return Vec::new();
        };
        vec![
            Command::Remove {
                key: key.clone(),
                instance: field.instance,
                if_text: None,
                mirror: true,
            },
            Command::DiscardDraft { key: key.clone() },
        ]
    }

    /// The editor unmounts. Leaving a field saves it as a blur does, and a check comment as
    /// Done does (A-10); a section card's text stays a draft. A clean field is forgotten, and so
    /// is one kept over a restart, whose entry waits for the offer.
    pub fn close(&mut self, key: &BufferKey, now: Ms) -> Vec<Command> {
        let mut out = self.blur(key, now);
        let Some(field) = self.fields.get_mut(key) else {
            return out;
        };
        field.editor = None;
        field.notice = None;
        if field.is_clean_state() || field.is_kept_over_restart() {
            if key.field.kind == FieldKind::Comment
                && field.is_clean_state()
                && let Some(token) = field.base.first().cloned()
            {
                let at = match &field.scope {
                    FieldScope::App => Some(None),
                    FieldScope::Project(Some(at)) => Some(Some(at.clone())),
                    FieldScope::Project(None) => None,
                };
                if let Some(at) = at {
                    self.comment_pairs.entry(at).or_default().insert(
                        key.clone(),
                        Pair {
                            value: field
                                .persisted
                                .clone()
                                .unwrap_or_else(|| field.current.clone()),
                            token,
                            view: field.pair_view,
                        },
                    );
                }
            }
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
            sessions: &self.sessions,
            gating: self.gating,
        };
        let Some(field) = self.fields.get_mut(key) else {
            return out;
        };
        let sent = match &field.state {
            FieldSave::Saving { sent } if sent.request == request => sent.clone(),
            _ => return out,
        };
        field.retry_at = None;
        match answered {
            Answered::Acked(ack) => {
                // A verse save also writes the verse's reflowed alignment (R25, Q-6), whose
                // token comes back beside the verse's: the field's token is its value's. A
                // section card is based on every verse it displays, and its `Ack` answers each,
                // in the order of the base (`section_ack`): cutting the base to the first verse
                // would have the next save read the whole text as that verse.
                let own = match ack.saved.as_slice() {
                    [saved] => ack
                        .tokens
                        .iter()
                        .find(|t| t.record == saved.record)
                        .and_then(|token| match field.base.len() {
                            0 | 1 => Some(vec![token.token.clone()]),
                            n => ack
                                .tokens
                                .get(..n)
                                .filter(|all| all.first() == Some(token))
                                .map(|all| all.iter().map(|t| t.token.clone()).collect()),
                        })
                        .map(|base| (saved, base)),
                    _ => None,
                };
                let Some((saved, base)) = own else {
                    fail(
                        key,
                        field,
                        SaveError::BadAck("an Ack for one field holds one value and its token"),
                        &ctx,
                        &mut out,
                    );
                    self.pump(now, &mut out);
                    return out;
                };
                field.reissues = 0;
                field.base = base;
                field.persisted = Some(saved.text.clone());
                field.pair_view = field
                    .pair_view
                    .max(Some(View::of(sent.scope.as_ref(), ack.view)));
                field.remember_own(request);
                if field
                    .waiting
                    .as_ref()
                    .is_some_and(|(pair, ..)| !field.is_newer(pair))
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
                    out.push(remove_of(key, field));
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
                    // The remaining text now has the Ack's tokens. Refresh its server draft
                    // immediately even when the prior draft had the same text (R26).
                    field.draft_due = Some(now);
                    field.draft_text = None;
                    out.push(Command::Write {
                        key: key.clone(),
                        entry: entry_of(field),
                        mirror: true,
                    });
                    field.mirrored = true;
                    pump_draft(key, field, now, &mut out);
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
                    // The reply and the session event of a reopen come in either order (5.4):
                    // while the page still holds the open that refused, the field waits for the
                    // sessions to name another one, and asks for them.
                    field.refused = sent.scope;
                    field.send_on_open = true;
                    attach(key, field, &mut self.ids, &ctx, now, &mut out);
                    if field.awaits_open() {
                        out.push(Command::ReadSessions);
                    }
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
                    send(key, field, &mut self.ids, now, &mut out);
                }
            }
            Answered::Failed(SaveError::NotSaved(NotSaved::ReadOnly(reason))) => {
                field.state = FieldSave::Held(Hold::ReadOnly(reason));
                field.send_on_open = true;
                out.push(Command::Write {
                    key: key.clone(),
                    entry: entry_of(field),
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
        for (key, field) in &mut self.fields {
            if matches!(field.state, FieldSave::Failed(_)) {
                retry_field(key, field, &mut self.ids, now, &mut out);
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
        self.refresh_over(key, pair, Vec::new(), by)
    }

    /// As [`refresh`](Self::refresh) for a read of several records, a section card's.
    fn refresh_over(
        &mut self,
        key: &BufferKey,
        pair: Pair,
        rest: Vec<ShownToken>,
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
            out.push(remove_of(key, field));
            field.mirrored = false;
        }
        if !field.is_newer(&pair) {
            return out;
        }
        let newer_than_waiting =
            field
                .waiting
                .as_ref()
                .is_none_or(|(held, ..)| match (pair.view, held.view) {
                    (Some(view), Some(held)) => view >= held,
                    _ => true,
                });
        if !newer_than_waiting {
            return out;
        }
        field.waiting = Some((pair, rest, by));
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
    pub fn local_change(&mut self, request: RequestId, view: View) {
        for field in self.fields.values_mut() {
            let in_flight =
                matches!(&field.state, FieldSave::Saving { sent } if sent.request == request);
            if in_flight || field.own.contains(&request) {
                field.own_view = field.own_view.max(Some(view));
            }
        }
    }

    /// The open sessions, from `hello` and the session events: fields reattach to the current
    /// open of their folder and project id, and hold while it is read-only or absent. A clean
    /// field with no editor is forgotten once its open has ended: a read of another open is
    /// newer than anything it holds.
    pub fn sessions(&mut self, open: &[OpenSession], now: Ms) -> Vec<Command> {
        let mut out = Vec::new();
        self.comment_pairs.retain(|at, _| {
            at.as_ref()
                .is_none_or(|at| open.iter().any(|session| session.at == *at))
        });
        self.sessions = open.to_vec();
        let ctx = Ctx {
            sessions: &self.sessions,
            gating: self.gating,
        };
        let mut ended = Vec::new();
        for (key, field) in &mut self.fields {
            if !matches!(field.scope, FieldScope::Project(_)) {
                continue;
            }
            let current = FieldScope::Project(session_of(key, ctx.sessions).map(|s| s.at.clone()));
            match field.state {
                FieldSave::Held(Hold::Restarted) => {}
                FieldSave::Clean | FieldSave::SavedWithWarning(_)
                    if field.editor.is_none() && field.scope != current =>
                {
                    ended.push(key.clone());
                }
                // A save in flight: the reply decides, and the next save goes to the current
                // open.
                FieldSave::Saving { .. }
                | FieldSave::Clean
                | FieldSave::SavedWithWarning(_)
                | FieldSave::Failed(_) => field.scope = current,
                FieldSave::Dirty { .. } | FieldSave::Held(_) => {
                    attach(key, field, &mut self.ids, &ctx, now, &mut out);
                }
            }
        }
        for key in ended {
            self.fields.remove(&key);
        }
        self.pump(now, &mut out);
        out
    }

    /// The server started again (AD4): nothing is sent, the entries stay for the offer at
    /// their project's next open, and a reply that comes later changes nothing. A field whose
    /// editor is still on screen stays, held, as the unsaved edit of the run before: what is
    /// typed into it goes on into that run's entry, which this run never restores or sends, so
    /// the offer shows it with the save that run never answered. Every other field is dropped.
    /// A second report of the same run changes nothing, so whichever part of the page finds the
    /// restart first, the others can tell it again.
    pub fn restarted(&mut self, instance: ServerInstance) {
        if instance == self.instance {
            return;
        }
        self.instance = instance;
        self.comment_pairs.clear();
        self.fields.retain(|_, field| field.editor.is_some());
        for field in self.fields.values_mut() {
            if let FieldSave::Saving { sent } = &field.state {
                field.unanswered = Some(sent.clone());
            }
            if !field.is_kept_over_restart() {
                field.state = FieldSave::Held(Hold::Restarted);
            }
            if let FieldScope::Project(_) = field.scope {
                field.scope = FieldScope::Project(None);
            }
            field.due = None;
            field.retry_at = None;
            field.draft_due = None;
            field.send_on_open = false;
            field.waiting = None;
            field.notice = None;
            field.own.clear();
            field.own_view = None;
            field.pair_view = None;
        }
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
        for (key, field) in &mut self.fields {
            if !field.sendable_by_gate() {
                continue;
            }
            match field.state {
                FieldSave::Failed(_) if !field.gate_retried => {
                    field.gate_retried = true;
                    retry_field(key, field, &mut self.ids, now, &mut out);
                }
                FieldSave::Dirty { .. } => send(key, field, &mut self.ids, now, &mut out),
                _ => {}
            }
        }
        out
    }

    /// The page is hidden (`pagehide`): every field that may save now is sent, best effort, and
    /// each standing failure is retried once with a new id, as tC4's `drain` does for each of
    /// its schedulers then (`tC4:src/state.jsx:3699-3704`, `saveScheduler.ts:139-146`): the
    /// cause may be gone, and the page is leaving. Each entry names its save before the request
    /// goes, so the next load of the same run sends the same request again if no answer came.
    pub fn hide(&mut self, now: Ms) -> Vec<Command> {
        let mut out = Vec::new();
        for (key, field) in &mut self.fields {
            if !field.sendable_by_gate() {
                continue;
            }
            match field.state {
                FieldSave::Dirty { .. } => send(key, field, &mut self.ids, now, &mut out),
                FieldSave::Failed(_) => retry_field(key, field, &mut self.ids, now, &mut out),
                _ => {}
            }
        }
        out
    }

    /// Where the gate stands. A held field and a section buffer never block it: the person
    /// cannot save the one, and the other is saved only by its Save (D9 of the A4b plan). A
    /// field that waits for its project's new open after `StaleScope` keeps it waiting, as a
    /// save in flight does.
    pub fn gate(&self) -> GateState {
        let fields = || self.fields.values().filter(|f| f.sendable_by_gate());
        // A failure no retry in this open can fix (a newer change arrived under a merged
        // verse) keeps its text and its reason, and does not keep the person from Home
        // (INV-A-REFUSED-SAVE-NEVER-TRAPS-THE-TRANSLATOR, tC4 #801).
        if fields().any(|f| match &f.state {
            FieldSave::Failed(error) => f.gate_retried && classify::save_error_can_be_fixed(error),
            _ => false,
        }) {
            return GateState::Blocked;
        }
        if self.fields.values().any(Field::is_saving) || fields().any(Field::awaits_open) {
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

    /// Whether a lost reply's resend goes, now that `app/hello` named the run `hello`. It goes
    /// only to the run it was sent to, and only while that is still this machine's run and the
    /// field still waits for this very request: another part of the page (the event stream's
    /// `Resync`, a read of the sessions) may have found a restart while the `hello` was on its
    /// way, and the new run, which has no reply cache, would store the text a second time.
    pub fn resend_check(
        &self,
        key: &BufferKey,
        request: RequestId,
        sent_to: ServerInstance,
        hello: ServerInstance,
    ) -> ResendCheck {
        if hello != sent_to {
            return ResendCheck::Restarted(hello);
        }
        let waits = self.fields.get(key).is_some_and(
            |field| matches!(&field.state, FieldSave::Saving { sent } if sent.request == request),
        );
        if self.instance == sent_to && waits {
            ResendCheck::Send
        } else {
            ResendCheck::Nothing
        }
    }

    /// The fields of `kind` whose save has not landed: typed, on its way, failed or held.
    pub fn unsaved_keys(&self, kind: FieldKind) -> Vec<BufferKey> {
        self.fields
            .iter()
            .filter(|(key, field)| {
                key.field.kind == kind
                    && !matches!(
                        field.state,
                        FieldSave::Clean | FieldSave::SavedWithWarning(_)
                    )
            })
            .map(|(key, _)| key.clone())
            .collect()
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

    /// The entry `boot.js` writes on a keystroke into the field's editor: the field's entry as
    /// this page's machine holds it (its run, its tokens and its save in flight), with `text`
    /// and `at` filled in by `boot.js`. A keystroke therefore never leaves another tab's save or
    /// tokens under this page's text.
    pub fn template(&self, key: &BufferKey) -> Option<BufferEntry> {
        let field = self.fields.get(key)?;
        Some(BufferEntry {
            text: String::new(),
            ..entry_of(field)
        })
    }

    /// Another write of the field's record (a mark under a typed check comment) moved the
    /// token its draft names: the unsaved text is drafted again at once, so a *Keep* after a
    /// crash continues from that write instead of standing beside it as a conflict.
    pub fn redraft(&mut self, key: &BufferKey, now: Ms) -> Vec<Command> {
        let mut out = Vec::new();
        if let Some(field) = self.fields.get_mut(key) {
            field.draft_due = Some(now);
            field.draft_text = None;
            pump_draft(key, field, now, &mut out);
        }
        out
    }

    /// The worst state of all fields: an error, a save in flight, unsaved or held text, or
    /// saved. A field kept over a restart counts while its text differs from the stored text.
    pub fn worst(&self) -> Worst {
        self.fields
            .values()
            .map(|field| match field.state {
                FieldSave::Failed(_) => Worst::Error,
                FieldSave::Saving { .. } => Worst::Saving,
                FieldSave::Held(Hold::Restarted) if !field.is_dirty() => Worst::Saved,
                FieldSave::Dirty { .. } | FieldSave::Held(_) => Worst::Dirty,
                FieldSave::Clean | FieldSave::SavedWithWarning(_) => Worst::Saved,
            })
            .max()
            .unwrap_or(Worst::Saved)
    }

    /// The kinds of the fields whose save failed, for the indicator's Retry.
    pub fn failed_kinds(&self) -> Vec<FieldKind> {
        self.fields
            .iter()
            .filter(|(_, field)| matches!(field.state, FieldSave::Failed(_)))
            .map(|(key, _)| key.field.kind)
            .collect()
    }

    pub fn invalid_text_reason(&self) -> Option<String> {
        self.fields.values().find_map(|field| match &field.state {
            FieldSave::Failed(error) => error.invalid_text_reason().map(str::to_owned),
            _ => None,
        })
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
                    || field.awaits_open()
                    || matches!(field.state, FieldSave::Dirty { .. }) && field.due.is_some()
            })
            .count()
    }

    /// Runs what is due at `now`: lost replies sent again, due saves, drafts.
    fn pump(&mut self, now: Ms, out: &mut Vec<Command>) {
        for (key, field) in &mut self.fields {
            match &field.state {
                FieldSave::Saving { sent } if field.retry_at.is_some_and(|at| at <= now) => {
                    let sent = sent.clone();
                    field.retry_at = None;
                    out.push(Command::Resend {
                        key: key.clone(),
                        sent,
                        base: field.base.clone(),
                        instance: field.instance,
                    });
                }
                FieldSave::Dirty { .. } if field.due.is_some_and(|due| due <= now) => {
                    send(key, field, &mut self.ids, now, out);
                }
                _ => {}
            }
            pump_draft(key, field, now, out);
        }
    }
}

/// The entry of a field as it is now, with the save in flight if there is one.
fn entry_of(field: &Field) -> BufferEntry {
    BufferEntry {
        api: API_VERSION,
        instance: field.instance,
        base: field.base.clone(),
        text: field.current.clone(),
        sent: match &field.state {
            FieldSave::Saving { sent } => Some(sent.clone()),
            FieldSave::Held(Hold::Restarted) => field.unanswered.clone(),
            _ => None,
        },
        at: WallMs(0),
        draft: None,
    }
}

/// The removal of a field's entry once it is clean, while it still holds the text now.
fn remove_of(key: &BufferKey, field: &Field) -> Command {
    Command::Remove {
        key: key.clone(),
        instance: field.instance,
        if_text: Some(field.current.clone()),
        mirror: field.mirrored,
    }
}

fn session_of<'a>(key: &BufferKey, sessions: &'a [OpenSession]) -> Option<&'a OpenSession> {
    sessions
        .iter()
        .find(|session| session.at.project == key.project && session.id == key.id)
}

/// Sends the field's current text with a new id: the entry with the save first (so a reload
/// sends the same request again), then the request. Never to an open that refused the field
/// with `StaleScope`: the field then waits, unsaved, for the page to hear of the new open.
fn send(
    key: &BufferKey,
    field: &mut Field,
    ids: &mut Box<dyn FnMut() -> RequestId>,
    now: Ms,
    out: &mut Vec<Command>,
) {
    let scope = match &field.scope {
        FieldScope::App => None,
        FieldScope::Project(Some(at)) if field.refused.as_ref() == Some(at) => {
            field.send_on_open = true;
            field.due = None;
            if !matches!(field.state, FieldSave::Dirty { .. }) {
                // The entry no longer names the refused request: a reload must not repeat it.
                field.state = FieldSave::Dirty { since: now };
                out.push(Command::Write {
                    key: key.clone(),
                    entry: entry_of(field),
                    mirror: true,
                });
                field.mirrored = true;
            }
            return;
        }
        FieldScope::Project(Some(at)) => Some(at.clone()),
        FieldScope::Project(None) => {
            let named_a_save = field.is_saving();
            field.state = FieldSave::Held(Hold::NoOpen);
            field.send_on_open = true;
            field.due = None;
            if named_a_save {
                // A reissue after `RequestReused`: the entry no longer names the answered
                // request, so a reload never sends it again.
                out.push(Command::Write {
                    key: key.clone(),
                    entry: entry_of(field),
                    mirror: true,
                });
                field.mirrored = true;
            }
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
        entry: entry_of(field),
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
    now: Ms,
    out: &mut Vec<Command>,
) {
    if field.is_dirty() {
        send(key, field, ids, now, out);
    } else {
        field.state = FieldSave::Clean;
        out.push(remove_of(key, field));
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
                    entry: entry_of(field),
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
                            entry: entry_of(field),
                            mirror: true,
                        });
                    }
                }
                SessionMode::Writable => {
                    if field.send_on_open {
                        // Base token, new id, the current open (A-19).
                        send(key, field, ids, now, out);
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
        entry: entry_of(field),
        mirror: true,
    });
    field.mirrored = true;
}

/// A clean field that is not focused takes the newest refresh it has not taken.
fn take_waiting(key: &BufferKey, field: &mut Field, out: &mut Vec<Command>) {
    if field.is_saving() || !field.is_clean_state() || field.focused() {
        return;
    }
    let Some((pair, rest, _)) = field.waiting.take() else {
        return;
    };
    if !field.is_newer(&pair) {
        return;
    }
    field.current = pair.value.clone();
    field.persisted = Some(pair.value.clone());
    field.base = std::iter::once(pair.token).chain(rest).collect();
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
    // A field kept over a restart sends nothing to the new run, its draft included (AD4).
    let unsaved = field.is_dirty()
        && matches!(
            field.state,
            FieldSave::Dirty { .. }
                | FieldSave::Failed(_)
                | FieldSave::Held(Hold::NoOpen | Hold::ReadOnly(_))
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
