//! One open aligner: a task that owns the `Aligner`, its save machine and its timer, so an
//! edit, the 2 s timer, a flush and the store's answer never run at once (APP-DESIGN §4.7: one
//! edit at a time per aligner). The pure parts are `save::next` and `view::build`; this file
//! carries out their effects.

use std::future::pending;
use std::pin::Pin;
use std::sync::Arc;

use tc_align::{Aligner, Click};
use tc_api::ApiError;
use tc_api::align::{
    AlignEdit, AlignPlace, AlignSave, AlignerView, BankClick, SuggestStatus, Word,
};
use tc_api::event::{Event, SessionEvent, SessionMode};
use tc_api::ids::{AlignerId, Scope};
use tc_api::write::NotSaved;
use tc_core::BookId;
use tc_journal::command::AlignmentEdit;
use tc_journal::record::TargetWord;
use tokio::sync::{mpsc, oneshot};
use tokio::time::{Instant, Sleep, sleep_until};

use super::open::{Ready, Tokens};
use super::save::{Effect, Event as SaveEvent, Machine, SAVE_AFTER, State, next};
use super::view::{Facts, Moment, build};
use crate::backend::{BoxFuture, CommandError, ProjectHandle, StoreCommand, StoreReply};
use crate::events::EventBus;

/// What the session around the aligner says now: the project's suggestions switch and what the
/// engine knows. It comes with each request, so the view never shows an old switch.
#[derive(Debug, Clone)]
pub(super) struct Ctx {
    pub suggest_on: bool,
    pub suggest: SuggestStatus,
}

pub(super) enum Cmd {
    View {
        ctx: Ctx,
        reply: oneshot::Sender<AlignerView>,
    },
    Edit {
        ctx: Ctx,
        seq: u64,
        edit: AlignEdit,
        reply: oneshot::Sender<Result<AlignerView, ApiError>>,
    },
    KeepReflow {
        ctx: Ctx,
        seq: u64,
        reply: oneshot::Sender<Result<AlignerView, ApiError>>,
    },
    /// Save now and answer how it went.
    Flush { reply: oneshot::Sender<AlignSave> },
    /// Save, and end the session if that worked.
    Close { reply: oneshot::Sender<AlignSave> },
}

/// The way to an open aligner's task.
pub(super) struct Handle {
    pub id: AlignerId,
    pub book: BookId,
    pub place: AlignPlace,
    tx: mpsc::UnboundedSender<Cmd>,
}

impl Handle {
    /// Sends a command and waits for its answer. A task that ended answers `StaleSession`: the
    /// aligner the caller means is gone.
    pub(super) async fn ask<T>(
        &self,
        make: impl FnOnce(oneshot::Sender<T>) -> Cmd,
    ) -> Result<T, ApiError> {
        let (reply, answer) = oneshot::channel();
        self.tx
            .send(make(reply))
            .map_err(|_| ApiError::StaleSession)?;
        answer.await.map_err(|_| ApiError::StaleSession)
    }
}

pub(super) struct Env {
    pub bus: EventBus,
    pub scope: Scope,
    pub handle: Arc<dyn ProjectHandle>,
}

struct Actor {
    env: Env,
    facts: Facts,
    aligner: Aligner,
    tokens: Tokens,
    /// One more for every change of the view.
    seq: u64,
    /// One more for every change of the links: what the machine counts.
    version: u64,
    machine: Machine,
    mark_refused: bool,
    timer: Option<Pin<Box<Sleep>>>,
    /// The generation the running timer was armed with.
    armed: u64,
    saving: Option<BoxFuture<Result<StoreReply, CommandError>>>,
    /// Flushes and closes waiting for the links to be saved.
    flushing: Vec<oneshot::Sender<AlignSave>>,
    closing: Vec<oneshot::Sender<AlignSave>>,
    done: bool,
}

/// Starts the task of a verse that opened.
pub(super) fn spawn(env: Env, ready: Ready) -> Handle {
    let Ready {
        aligner,
        facts,
        tokens,
        book,
    } = ready;
    let (tx, rx) = mpsc::unbounded_channel();
    let handle = Handle {
        id: facts.id,
        book,
        place: facts.place,
        tx,
    };
    // A lossless carry-over is saved when the verse is left (AD19); until then nothing shows
    // as unsaved.
    let machine = if aligner.is_changed() {
        Machine::carried(0)
    } else {
        Machine::clean()
    };
    let actor = Actor {
        env,
        facts,
        aligner,
        tokens,
        seq: 0,
        version: 0,
        machine,
        mark_refused: false,
        timer: None,
        armed: 0,
        saving: None,
        flushing: Vec::new(),
        closing: Vec::new(),
        done: false,
    };
    tokio::spawn(actor.run(rx));
    handle
}

enum Wake {
    Cmd(Option<Cmd>),
    Timer,
    Saved(Result<StoreReply, CommandError>),
}

async fn wait_saving(
    saving: &mut Option<BoxFuture<Result<StoreReply, CommandError>>>,
) -> Result<StoreReply, CommandError> {
    match saving {
        Some(save) => save.await,
        None => pending().await,
    }
}

async fn wait_timer(timer: &mut Option<Pin<Box<Sleep>>>) {
    match timer {
        Some(sleep) => sleep.as_mut().await,
        None => pending().await,
    }
}

/// Why a save did not store, in the words of `NotSaved`.
fn not_saved(error: CommandError) -> NotSaved {
    match crate::wire::error::command_error(error) {
        Ok(ApiError::NotSaved(reason)) => reason,
        Ok(other) => NotSaved::Disk(other.to_string()),
        Err(refusal) => NotSaved::Disk(refusal.0),
    }
}

/// What an edit did.
enum Applied {
    /// Nothing changed: the view stays as it was.
    Nothing,
    /// The view changed, the links did not.
    View,
    Links,
    /// Mark valid was refused because suggestions stand.
    Refused,
}

impl Actor {
    async fn run(mut self, mut rx: mpsc::UnboundedReceiver<Cmd>) {
        while !self.done {
            let wake = tokio::select! {
                biased;
                saved = wait_saving(&mut self.saving) => Wake::Saved(saved),
                () = wait_timer(&mut self.timer) => Wake::Timer,
                cmd = rx.recv() => Wake::Cmd(cmd),
            };
            match wake {
                Wake::Saved(outcome) => {
                    self.saving = None;
                    let result = match outcome {
                        Ok(reply) => {
                            if let Some((_, token)) = reply
                                .tokens
                                .into_iter()
                                .find(|(key, _)| key == self.tokens.alignment.key())
                            {
                                self.tokens.alignment = token;
                            }
                            Ok(())
                        }
                        Err(error) => Err(not_saved(error)),
                    };
                    self.apply(SaveEvent::Done(result));
                }
                Wake::Timer => {
                    self.timer = None;
                    self.apply(SaveEvent::Tick { timer: self.armed });
                }
                Wake::Cmd(Some(cmd)) => self.command(cmd),
                // Every sender is gone: the session ended without a close.
                Wake::Cmd(None) => break,
            }
        }
    }

    fn command(&mut self, cmd: Cmd) {
        match cmd {
            Cmd::View { ctx, reply } => {
                let _ = reply.send(self.view(&ctx));
            }
            Cmd::Edit {
                ctx,
                seq,
                edit,
                reply,
            } => {
                let _ = reply.send(self.edit(&ctx, seq, edit));
            }
            Cmd::KeepReflow { ctx, seq, reply } => {
                let answer = self.checked(seq).map(|()| {
                    self.aligner.keep_reflow();
                    self.seq += 1;
                    self.links_changed();
                    self.view(&ctx)
                });
                let _ = reply.send(answer);
            }
            Cmd::Flush { reply } => {
                self.flushing.push(reply);
                self.apply(SaveEvent::Flush);
            }
            Cmd::Close { reply } => {
                self.closing.push(reply);
                self.apply(SaveEvent::Flush);
            }
        }
    }

    fn view(&self, ctx: &Ctx) -> AlignerView {
        build(
            &self.aligner,
            &self.facts,
            &Moment {
                seq: self.seq,
                save: self.machine.visible(),
                mark_refused: self.mark_refused,
                suggest_on: ctx.suggest_on,
                suggest: ctx.suggest.clone(),
                asked: false,
                suggesting: false,
            },
        )
    }

    /// An edit names the view it was made on; any other is for an aligner the person no
    /// longer sees.
    fn checked(&self, seq: u64) -> Result<(), ApiError> {
        if seq != self.seq {
            return Err(ApiError::StaleSession);
        }
        if let SessionMode::ReadOnly(reason) = self.env.handle.facts().mode {
            return Err(ApiError::NotSaved(NotSaved::ReadOnly(reason)));
        }
        Ok(())
    }

    fn edit(&mut self, ctx: &Ctx, seq: u64, edit: AlignEdit) -> Result<AlignerView, ApiError> {
        self.checked(seq)?;
        match self.apply_edit(edit) {
            Applied::Nothing => {}
            Applied::View => {
                self.seq += 1;
                self.mark_refused = false;
            }
            Applied::Links => {
                self.seq += 1;
                self.mark_refused = false;
                self.links_changed();
            }
            Applied::Refused => {
                self.seq += 1;
                self.mark_refused = true;
            }
        }
        Ok(self.view(ctx))
    }

    fn links_changed(&mut self) {
        self.version += 1;
        self.apply(SaveEvent::Changed {
            version: self.version,
        });
    }

    /// The word of the bank that the person's chip names, with every field it carries.
    fn banked(&self, word: &Word) -> Option<TargetWord> {
        self.aligner
            .value()
            .word_bank
            .iter()
            .find(|w| w.word == word.word && w.occurrence == word.occurrence)
            .cloned()
    }

    fn placed(&self, card: u32, word: &Word) -> Option<TargetWord> {
        self.aligner
            .value()
            .alignments
            .get(usize::try_from(card).ok()?)?
            .bottom_words
            .iter()
            .find(|w| w.word == word.word && w.occurrence == word.occurrence)
            .cloned()
    }

    /// The edit, as tC4's actions apply it. An edit that does not apply (a card that is gone,
    /// a word already placed) changes nothing, as tC4's `applyAlignEdit` returns when the
    /// record comes back the same.
    fn apply_edit(&mut self, edit: AlignEdit) -> Applied {
        let at = |n: u32| usize::try_from(n).unwrap_or(usize::MAX);
        let links = |applied: bool| {
            if applied {
                Applied::Links
            } else {
                Applied::Nothing
            }
        };
        match edit {
            AlignEdit::ClickBank { word, click } => {
                // A word that is not in the shown bank any more (placed meanwhile) is no click.
                let Some(index) = self
                    .aligner
                    .shown_bank()
                    .iter()
                    .position(|w| w.word == word.word && w.occurrence == word.occurrence)
                else {
                    return Applied::Nothing;
                };
                self.aligner.click_bank_word(
                    index,
                    match click {
                        BankClick::Plain => Click::Plain,
                        BankClick::Toggle => Click::Toggle,
                        BankClick::Range => Click::Range,
                    },
                );
                Applied::View
            }
            AlignEdit::PlaceSelection { card } => {
                links(self.aligner.place_selection(at(card)).is_ok())
            }
            AlignEdit::PlaceWords { card, words } => {
                let words: Vec<TargetWord> = words.iter().filter_map(|w| self.banked(w)).collect();
                links(self.aligner.place_many(at(card), &words).is_ok())
            }
            AlignEdit::Unplace { card, word } => match self.placed(card, &word) {
                Some(w) => links(self.aligner.unplace(at(card), &w).is_ok()),
                None => Applied::Nothing,
            },
            AlignEdit::Move { from, to, word } => match self.placed(from, &word) {
                Some(w) => links(self.aligner.move_word(at(from), at(to), &w).is_ok()),
                None => Applied::Nothing,
            },
            AlignEdit::Merge { from, to } => links(self.aligner.merge(at(from), at(to)).is_ok()),
            AlignEdit::Split { card } => links(self.aligner.split(at(card)).is_ok()),
            AlignEdit::MarkValid => match self.aligner.mark_valid() {
                Ok(()) => Applied::Links,
                Err(_) => Applied::Refused,
            },
            AlignEdit::Accept { card, word } => {
                let found = self.aligner.proposals().iter().position(|p| {
                    p.card == at(card)
                        && p.word.word == word.word
                        && p.word.occurrence == word.occurrence
                });
                match found {
                    Some(i) => links(self.aligner.accept(i).is_ok()),
                    None => Applied::Nothing,
                }
            }
            AlignEdit::AcceptAll => links(self.aligner.accept_all() > 0),
            AlignEdit::RejectAll => {
                self.aligner.reject_all();
                Applied::View
            }
        }
    }

    // ---- the save machine --------------------------------------------------------------

    fn waiting(&self) -> bool {
        !self.flushing.is_empty() || !self.closing.is_empty()
    }

    /// One event of the machine and its effects; what the person or a waiting flush sees of it
    /// follows.
    fn apply(&mut self, event: SaveEvent) {
        let before = self.machine.visible();
        self.step(event);
        // Changed while a flush waited: save those too, until nothing waits or a save fails.
        while self.waiting() && matches!(self.machine.state, State::Dirty { .. }) {
            self.step(SaveEvent::Flush);
        }
        let after = self.machine.visible();
        if after != before {
            self.env.bus.publish(Event::Session {
                at: self.env.scope.clone(),
                event: SessionEvent::AlignSave {
                    aligner: self.facts.id,
                    state: after,
                },
            });
        }
        if let Some(answer) = self.machine.settled() {
            for waiting in self.flushing.drain(..) {
                let _ = waiting.send(answer.clone());
            }
            // A close that found everything saved ends the session; one that did not leaves
            // it open and unsaved, for the person to see.
            if answer == AlignSave::Saved && !self.closing.is_empty() {
                self.done = true;
            }
            for waiting in self.closing.drain(..) {
                let _ = waiting.send(answer.clone());
            }
        }
    }

    fn step(&mut self, event: SaveEvent) {
        let machine = std::mem::replace(&mut self.machine, Machine::clean());
        let (machine, effects) = next(machine, event);
        self.machine = machine;
        for effect in effects {
            match effect {
                Effect::Arm { timer } => {
                    self.armed = timer;
                    self.timer = Some(Box::pin(sleep_until(Instant::now() + SAVE_AFTER)));
                }
                Effect::Disarm => self.timer = None,
                Effect::Start { .. } => self.start(),
                Effect::MarkSaved => self.aligner.mark_saved(),
            }
        }
    }

    fn start(&mut self) {
        let value = self.aligner.value();
        let edit = AlignmentEdit {
            alignments: value.alignments.clone(),
            word_bank: value.word_bank.clone(),
            done: value.done,
        };
        let command = StoreCommand::save_alignment(
            self.tokens.alignment.clone(),
            self.tokens.verse.clone(),
            edit,
        );
        self.saving = Some(self.env.handle.submit(command));
    }
}
