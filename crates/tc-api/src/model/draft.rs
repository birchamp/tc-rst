//! The life of a field's unsaved text (APP-DESIGN §6.6, C1b): one state per field, one event
//! list, and one pure [`next`] that says what each place does. The table in §6.6 is this
//! file's `match`.
//!
//! A field's text can be in four places: the page's buffer item (one per field), the shell's
//! mirror (the last line per field and server run), `.tc/drafts.json` (a *current* entry per
//! field, the page's typing at most a second behind, and *offered* entries, each an unsaved
//! text an open offered, kept until the person answers), and the store's record. [`next`]
//! decides; an executor carries out the [`Action`]s: the browser glue and the shell for the
//! buffer and the mirror, tc-app and tc-store for `drafts.json` and the record, and the
//! property test's world for all four.
//!
//! The rules that keep a translator's text:
//!
//! - **No pair drops a text it has not proved stored.** A text goes from a place only when
//!   the record holds it ([`Verdict::Saved`]), when it is the version it was typed over and no
//!   save of it landed ([`Verdict::Undone`]), when the person discards it, or once another
//!   place holds it durably (an offered entry, a re-keyed slot). Everything else is offered.
//! - **A text equal to a saved one is saved only if that save is the newest for the field.**
//!   An entry whose save landed continues from the stored text: it is offered over the stored
//!   version even when it is the text it was first typed over (typed back, or an undo).
//! - **One place answers an offer.** At an open the offered texts move into `drafts.json` as
//!   offered entries (written before the reply), and only then do the buffer slots go, each
//!   only while it still holds that text. *Keep* and *Discard* then touch one entry by its
//!   number, never the field's other texts or the page's typing since.
//! - **A slot goes only while it holds the text it was offered with**, or in the buffer an older
//!   one of its run ([`Action::Settle`]): newer typing in the same slot stays, and the page
//!   writes its own text again into what the settling freed.
//! - **A run's stored draft is judged by that run's buffer**, once: it goes behind the run's
//!   newer entry of the field, or, behind the shell's mirror, when the run has none (it ended
//!   clean); otherwise the offer's own entry takes it over.
//! - **The page's own fields are never offered**: a field this run's save machine holds saves
//!   itself, so the page leaves it out of what it sends (§6.6, `buffered_lines`).

use unicode_normalization::UnicodeNormalization;

use crate::buffer::BufferKey;
use crate::ids::ServerInstance;
use crate::session::{BufferSlot, Keep, StoredDraftId};

/// Whether two texts are one text in NFC, the form the store keeps.
pub fn same_text(a: &str, b: &str) -> bool {
    a == b || a.nfc().eq(b.nfc())
}

/// A text and the page's clock at its last keystroke (a stored entry's own time).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Text {
    pub text: String,
    pub at: i64,
}

/// The version of the record a text was typed over, as the place that holds the text names it.
/// `V` is the version as the executor holds it: a shown token on the wire, a number in tests.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Base<V> {
    /// Its token verifies here: *Keep* saves over it, so a record changed since becomes a
    /// conflict, never a revert.
    Verified(V),
    /// Its token names a version this installation cannot check (a new installation secret, a
    /// folder copied from another computer): *Keep* writes the text with no parents (R20).
    Unverified(V),
    /// No *Keep*: a field this build has no save for yet (a section, a new comment), or a token
    /// that does not parse. "Copy text", and the text stays until *Discard*.
    None,
}

impl<V> Base<V> {
    pub fn version(&self) -> Option<&V> {
        match self {
            Self::Verified(v) | Self::Unverified(v) => Some(v),
            Self::None => None,
        }
    }

    fn keep(&self) -> Keep {
        match self {
            Self::Verified(_) => Keep::Save,
            Self::Unverified(_) => Keep::AsVersion,
            Self::None => Keep::CopyText,
        }
    }
}

/// A place that holds an unsaved text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Home {
    /// A buffer item, or the mirror's line, of one run.
    Buffer(BufferSlot),
    /// An offered entry of `drafts.json`.
    Stored(StoredDraftId),
}

/// One unsaved text of a field, as one place holds it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Candidate<V> {
    pub text: String,
    pub at: i64,
    pub base: Base<V>,
    /// A buffer entry's save in flight, by its text: the save may have landed before the run
    /// ended.
    pub sent: Option<String>,
    /// The server run whose page typed it: a buffer slot's run, or the run a stored draft
    /// records.
    pub run: Option<ServerInstance>,
    pub home: Home,
}

/// What the store holds of a field now.
pub trait Record<V> {
    /// The text of each of the record's heads; empty for a field whose text the store does not
    /// compare (a section's buffer).
    fn heads(&self) -> Vec<String>;
    /// The record's version now, when it has exactly one head.
    fn sole(&self) -> Option<V>;
    /// The text the record held at `version`, when this installation can read it.
    fn text_of(&self, version: &V) -> Option<String>;
    /// Whether a version of the record after `version` held `text`: the text was stored, even
    /// if another change has replaced it since.
    fn held_since(&self, version: &V, text: &str) -> bool;
}

/// What a candidate holds, judged against the record.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Verdict<V> {
    /// A head of the record holds the text, or a version after the one it was typed over did.
    Saved,
    /// The text is the version it was typed over, and no save of it landed: typed and undone.
    Undone,
    /// Unsaved: *Keep* saves it over `over`. `landed` says that the save the entry waited for
    /// landed and `over` is the record's version now, so the text continues from the person's
    /// own save instead of standing beside it as a conflict (A4b).
    Offer {
        keep: Keep,
        over: Option<V>,
        landed: bool,
    },
}

/// The verdict on one candidate (§6.6's table, the open's rows).
pub fn classify<V: Clone>(candidate: &Candidate<V>, record: &dyn Record<V>) -> Verdict<V> {
    let heads = record.heads();
    let held = |text: &str| heads.iter().any(|head| same_text(head, text));
    if held(&candidate.text) {
        return Verdict::Saved;
    }
    // Stored after the version it was typed over, and replaced since: not unsaved typing.
    if let Base::Verified(base) = &candidate.base
        && record.held_since(base, &candidate.text)
    {
        return Verdict::Saved;
    }
    let landed = match (&candidate.sent, &candidate.base) {
        // Whoever stored the text the entry sent, its later typing continues from it. Asking
        // for this installation's own change instead would miss the save in a copy of the
        // folder, which has a device of its own, and drop a text typed back as undone.
        (Some(sent), Base::Verified(base)) => record.held_since(base, sent),
        _ => false,
    };
    if landed
        && heads.len() == 1
        && heads.iter().all(|head| {
            candidate
                .sent
                .as_deref()
                .is_some_and(|s| same_text(head, s))
        })
        && let Some(now) = record.sole()
    {
        return Verdict::Offer {
            keep: Keep::Save,
            over: Some(now),
            landed: true,
        };
    }
    // An offered entry of drafts.json was judged when it was taken in, with the save it was
    // waiting for, which it no longer names: a text it holds that is its version's own is a
    // text typed back after that save, never undone.
    if !landed
        && matches!(candidate.home, Home::Buffer(_))
        && let Base::Verified(version) = &candidate.base
        && record
            .text_of(version)
            .is_some_and(|then| same_text(&then, &candidate.text))
    {
        return Verdict::Undone;
    }
    Verdict::Offer {
        keep: candidate.base.keep(),
        over: candidate.base.version().cloned(),
        landed: false,
    }
}

/// One distinct unsaved text of a field and every place that holds it: one question.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Offer<V> {
    pub text: String,
    pub at: i64,
    pub keep: Keep,
    /// What *Keep* saves over; `None` for "Copy text".
    pub over: Option<V>,
    pub homes: Vec<Home>,
}

impl<V> Offer<V> {
    pub fn stored(&self) -> impl Iterator<Item = StoredDraftId> + '_ {
        self.homes.iter().filter_map(|home| match home {
            Home::Stored(id) => Some(*id),
            Home::Buffer(_) => None,
        })
    }

    pub fn slots(&self) -> impl Iterator<Item = &BufferSlot> + '_ {
        self.homes.iter().filter_map(|home| match home {
            Home::Buffer(slot) => Some(slot),
            Home::Stored(_) => None,
        })
    }
}

/// The page's own field in this server run: A4b's save machine (§6.5), as it shows in the
/// places. `base` is the version the field's next save names.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Live<V> {
    /// The page holds no unsaved text of the field.
    Clean,
    Typed {
        base: V,
        current: Text,
    },
    /// A save of `sent` is on its way; the buffer entry names it.
    Sending {
        base: V,
        sent: String,
        current: Text,
    },
    /// The save of `sent` landed as `over`; the page has not read the reply.
    Landed {
        base: V,
        over: V,
        sent: String,
        current: Text,
    },
}

/// A field's whole state: the page's own text, the questions an open asked, and the buffer
/// slots the page still has to drop once the server holds their text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FieldDraft<V> {
    pub live: Live<V>,
    pub offers: Vec<Offer<V>>,
    pub settle: Vec<BufferSlot>,
}

impl<V> Default for FieldDraft<V> {
    fn default() -> Self {
        Self {
            live: Live::Clean,
            offers: Vec::new(),
            settle: Vec::new(),
        }
    }
}

/// What happens to a field.
pub enum Event<'a, V> {
    /// A keystroke of this run's page; `shown` is the version the editor shows, the base of a
    /// field that had no unsaved text.
    Typed { text: String, at: i64, shown: V },
    /// The page's draft tick, at most once a second while the field is dirty (§6.6).
    DraftTick,
    /// The page sends the field's save.
    SaveSent,
    /// The store wrote the save in flight as `over`.
    SaveLanded { over: V },
    /// The page read the reply of a save that landed.
    SaveAcked,
    /// The save was refused or failed; the text stays.
    SaveRefused,
    /// The page reloaded in the same server run.
    Reload,
    /// The server run ended (a crash, a quit) and the app started again.
    Relaunch,
    /// An open asks what to offer: every candidate of the field in the buffer, the mirror and
    /// `drafts.json`, but the page's own (the page leaves those out). `mirror` says that the
    /// candidates include the shell's mirror, which holds every keystroke and every removal of
    /// each run: a run with no line of the field then ended with the field clean.
    Offer {
        candidates: Vec<Candidate<V>>,
        record: &'a dyn Record<V>,
        mirror: bool,
    },
    /// *Keep* of the offer at this index.
    Keep(usize),
    /// *Discard* of the offer at this index.
    Discard(usize),
    /// The copy of a read-only project (§6.4): the folder is copied with its `drafts.json`, and
    /// these slots, every one of the folder's in the buffer and the mirror, move to `to`. The
    /// page holds its own field on in the copy, which has the same project id, so the field's
    /// tokens stand there too.
    Copy {
        slots: Vec<BufferSlot>,
        to: BufferKey,
    },
    /// The page drops the slots the server answered as held elsewhere.
    Settle,
}

/// What a place does. The order of a list is the order to carry it out in.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action<V> {
    /// The field's buffer item (when it is this run's or empty) and a mirror line: this run's
    /// entry with `text`, `base` and the save in flight.
    Write {
        text: String,
        at: i64,
        base: V,
        sent: Option<String>,
    },
    /// This run's item and mirror line of the field go, while they hold `text`: the field is
    /// clean.
    Remove { text: String },
    /// `slot`'s buffer item and mirror line go, each only while it holds the slot's run and text.
    Settle(BufferSlot),
    /// The copy: `slot` is written under `to`, in the buffer and the mirror, then settled.
    Rekey { slot: BufferSlot, to: BufferKey },
    /// `drafts.json`'s current entry of the field.
    Draft { text: String, base: V },
    /// The field's save over `base`.
    Send { text: String, base: V },
    /// An offered entry of `drafts.json`, written before the server answers; only then are
    /// `then`'s slots settled and the `older` copies of the same typing removed.
    Absorb {
        text: String,
        at: i64,
        over: V,
        keep: Keep,
        then: Vec<BufferSlot>,
        older: Vec<StoredDraftId>,
    },
    /// An offered entry goes, written before the server answers.
    Unstore(StoredDraftId),
    /// *Keep*: the text saved over `over` as `keep` says.
    Keep { text: String, keep: Keep, over: V },
    /// The project folder is copied with its `drafts.json` (R7).
    CopyFolder,
}

/// The stored drafts that hold nothing unsaved, each with the index of the buffer entry that
/// supersedes it, if one does. One page edits a field per run and its buffer entry takes every
/// keystroke, so a stored draft of a run whose own entry of the field, one that has a *Keep*,
/// is at least as new holds nothing that entry does not. With the shell's mirror, a run with no
/// entry of the field ended with the field clean: its save holds the draft's typing or newer.
fn older_copies<V>(candidates: &[Candidate<V>], mirror: bool) -> Vec<(usize, Option<usize>)> {
    candidates
        .iter()
        .enumerate()
        .filter(|(_, copy)| matches!(copy.home, Home::Stored(_)))
        .filter_map(|(at, copy)| {
            let run = copy.run?;
            let entries: Vec<(usize, &Candidate<V>)> = candidates
                .iter()
                .enumerate()
                .filter(|(_, entry)| {
                    matches!(entry.home, Home::Buffer(_)) && entry.run == Some(run)
                })
                .collect();
            if entries.is_empty() {
                return mirror.then_some((at, None));
            }
            entries
                .into_iter()
                .filter(|(_, entry)| entry.at >= copy.at && entry.base.version().is_some())
                .max_by_key(|(_, entry)| entry.at)
                .map(|(newer, _)| (at, Some(newer)))
        })
        .collect()
}

/// One question being planned.
struct Planned<V> {
    offer: Offer<V>,
    /// The person's own landed save decides what it keeps over.
    landed: bool,
    /// The candidates it stands for.
    members: Vec<usize>,
    /// Stored drafts of a run it takes over: an entry of a run is judged by that run's buffer
    /// once, at the first open after it, and then the offer's own entry, of no run, holds the
    /// text.
    provisional: Vec<StoredDraftId>,
}

/// The open's plan for one field's candidates: the questions, what the places do, and the
/// buffer slots the page drops.
fn plan<V: Clone>(
    mut candidates: Vec<Candidate<V>>,
    record: &dyn Record<V>,
    mirror: bool,
) -> (Vec<Offer<V>>, Vec<Action<V>>, Vec<BufferSlot>) {
    candidates.sort_by_key(|candidate| std::cmp::Reverse(candidate.at));
    let copies = older_copies(&candidates, mirror);
    let mut offers: Vec<Planned<V>> = Vec::new();
    let mut actions = Vec::new();
    let mut settle = Vec::new();
    // What each superseding entry's copies become once its text is held.
    let older_of = |index: usize| -> Vec<StoredDraftId> {
        copies
            .iter()
            .filter(|(_, newer)| *newer == Some(index))
            .filter_map(|(copy, _)| match candidates[*copy].home {
                Home::Stored(id) => Some(id),
                Home::Buffer(_) => None,
            })
            .collect()
    };
    // A run that ended clean: its stored drafts go now.
    for (copy, newer) in &copies {
        if newer.is_none()
            && let Home::Stored(id) = candidates[*copy].home
        {
            actions.push(Action::Unstore(id));
        }
    }
    for (index, candidate) in candidates.iter().enumerate() {
        if copies.iter().any(|(copy, _)| *copy == index) {
            continue;
        }
        match classify(candidate, record) {
            Verdict::Saved | Verdict::Undone => {
                match &candidate.home {
                    Home::Buffer(slot) => settle.push(slot.clone()),
                    Home::Stored(id) => actions.push(Action::Unstore(*id)),
                }
                actions.extend(older_of(index).into_iter().map(Action::Unstore));
            }
            Verdict::Offer { keep, over, landed } => {
                let (home, taken) = match (&candidate.home, candidate.run) {
                    (Home::Stored(id), Some(_)) => (None, Some(*id)),
                    (home, _) => (Some(home.clone()), None),
                };
                let stored = matches!(home, Some(Home::Stored(_)));
                match offers
                    .iter_mut()
                    .find(|p| same_text(&p.offer.text, &candidate.text))
                {
                    Some(p) => {
                        // One text, one question. The person's own landed save decides what
                        // Keep saves over; an entry already offered, the next.
                        let better = (landed && !p.landed)
                            || (!p.landed && stored && p.offer.stored().next().is_none());
                        if better {
                            p.offer.keep = keep;
                            p.offer.over = over;
                            p.landed = landed;
                        }
                        p.offer.homes.extend(home);
                        p.provisional.extend(taken);
                        p.members.push(index);
                    }
                    None => offers.push(Planned {
                        offer: Offer {
                            text: candidate.text.clone(),
                            at: candidate.at,
                            keep,
                            over,
                            homes: home.into_iter().collect(),
                        },
                        landed,
                        members: vec![index],
                        provisional: taken.into_iter().collect(),
                    }),
                }
            }
        }
    }
    let mut planned = Vec::new();
    for Planned {
        mut offer,
        members,
        provisional,
        ..
    } in offers
    {
        let slots: Vec<BufferSlot> = offer.slots().cloned().collect();
        let mut older: Vec<StoredDraftId> = members.iter().flat_map(|m| older_of(*m)).collect();
        let held = offer.stored().next().is_some();
        match (offer.over.clone(), held) {
            // The text is in an offered entry already: its slots and copies can go.
            (_, true) => {
                settle.extend(slots);
                older.extend(provisional);
                actions.extend(older.into_iter().map(Action::Unstore));
            }
            (Some(over), false) => {
                older.extend(provisional);
                actions.push(Action::Absorb {
                    text: offer.text.clone(),
                    at: offer.at,
                    over,
                    keep: offer.keep,
                    then: slots,
                    older,
                });
            }
            // "Copy text": no entry of its own can hold it, so what holds it now stays, until
            // Discard.
            (None, false) => offer
                .homes
                .extend(provisional.into_iter().map(Home::Stored)),
        }
        planned.push(offer);
    }
    (planned, actions, settle)
}

/// After an absorb, the offer's home is the offered entry; the executor names its number.
pub fn absorbed<V>(offer: &mut Offer<V>, id: StoredDraftId) {
    offer.homes.retain(|home| matches!(home, Home::Stored(_)));
    offer.homes.push(Home::Stored(id));
}

/// The table of APP-DESIGN §6.6. Every pair is listed; a pair that is not proved safe keeps
/// every text where it is.
pub fn next<V: Clone>(
    state: FieldDraft<V>,
    event: Event<'_, V>,
) -> (FieldDraft<V>, Vec<Action<V>>) {
    let FieldDraft {
        live,
        mut offers,
        mut settle,
    } = state;
    let write = |current: &Text, base: &V, sent: Option<&String>| Action::Write {
        text: current.text.clone(),
        at: current.at,
        base: base.clone(),
        sent: sent.cloned(),
    };
    let (live, actions) = match (live, event) {
        // The page's own typing (A4b): the entry is written on every keystroke, naming the
        // save in flight.
        (Live::Clean, Event::Typed { text, at, shown }) => {
            let current = Text { text, at };
            let action = write(&current, &shown, None);
            (
                Live::Typed {
                    base: shown,
                    current,
                },
                vec![action],
            )
        }
        (Live::Typed { base, .. }, Event::Typed { text, at, .. }) => {
            let current = Text { text, at };
            let action = write(&current, &base, None);
            (Live::Typed { base, current }, vec![action])
        }
        (Live::Sending { base, sent, .. }, Event::Typed { text, at, .. }) => {
            let current = Text { text, at };
            let action = write(&current, &base, Some(&sent));
            (
                Live::Sending {
                    base,
                    sent,
                    current,
                },
                vec![action],
            )
        }
        (
            Live::Landed {
                base, over, sent, ..
            },
            Event::Typed { text, at, .. },
        ) => {
            // The page does not know yet that the save landed: its entry still names it.
            let current = Text { text, at };
            let action = write(&current, &base, Some(&sent));
            (
                Live::Landed {
                    base,
                    over,
                    sent,
                    current,
                },
                vec![action],
            )
        }
        // The draft tick sends nothing while a save is on its way (A4b's `pump_draft`).
        (live @ (Live::Clean | Live::Sending { .. } | Live::Landed { .. }), Event::DraftTick) => {
            (live, Vec::new())
        }
        (Live::Typed { base, current }, Event::DraftTick) => {
            let action = Action::Draft {
                text: current.text.clone(),
                base: base.clone(),
            };
            (Live::Typed { base, current }, vec![action])
        }
        // One save in flight per field; its entry names it before it is sent.
        (Live::Typed { base, current }, Event::SaveSent) => {
            let sent = current.text.clone();
            let actions = vec![
                write(&current, &base, Some(&sent)),
                Action::Send {
                    text: sent.clone(),
                    base: base.clone(),
                },
            ];
            (
                Live::Sending {
                    base,
                    sent,
                    current,
                },
                actions,
            )
        }
        (live @ (Live::Clean | Live::Sending { .. } | Live::Landed { .. }), Event::SaveSent) => {
            (live, Vec::new())
        }
        (
            Live::Sending {
                base,
                sent,
                current,
            },
            Event::SaveLanded { over },
        ) => (
            Live::Landed {
                base,
                over,
                sent,
                current,
            },
            Vec::new(),
        ),
        (
            live @ (Live::Clean | Live::Typed { .. } | Live::Landed { .. }),
            Event::SaveLanded { .. },
        ) => (live, Vec::new()),
        // Clean only on the stored text (A-2); else the next save names the landed version,
        // and the draft follows at once (R26).
        (
            Live::Landed {
                over,
                sent,
                current,
                ..
            },
            Event::SaveAcked,
        ) => {
            if same_text(&current.text, &sent) {
                (Live::Clean, vec![Action::Remove { text: current.text }])
            } else {
                let actions = vec![
                    write(&current, &over, None),
                    Action::Draft {
                        text: current.text.clone(),
                        base: over.clone(),
                    },
                ];
                (
                    Live::Typed {
                        base: over,
                        current,
                    },
                    actions,
                )
            }
        }
        (live @ (Live::Clean | Live::Typed { .. } | Live::Sending { .. }), Event::SaveAcked) => {
            (live, Vec::new())
        }
        (Live::Sending { base, current, .. }, Event::SaveRefused) => {
            let action = write(&current, &base, None);
            (Live::Typed { base, current }, vec![action])
        }
        (live @ (Live::Clean | Live::Typed { .. } | Live::Landed { .. }), Event::SaveRefused) => {
            (live, Vec::new())
        }
        // A reload keeps the run: the page takes its own entries back and sends a lost reply's
        // request again, which the run's reply cache answers. It asks for the offer again.
        (live, Event::Reload) => {
            offers.clear();
            settle.clear();
            (live, Vec::new())
        }
        // The run ended: what the page held is in the buffer and the mirror, for the offer.
        (_, Event::Relaunch) => {
            offers.clear();
            settle.clear();
            (Live::Clean, Vec::new())
        }
        (
            live,
            Event::Offer {
                candidates,
                record,
                mirror,
            },
        ) => {
            let (planned, actions, slots) = plan(candidates, record, mirror);
            offers = planned;
            settle.extend(slots);
            (live, actions)
        }
        (live, Event::Keep(index)) => {
            // "Copy text" copies; the offer stays until Discard.
            let keepable = offers.get(index).is_some_and(|offer| offer.over.is_some());
            let mut actions = Vec::new();
            if keepable {
                let offer = offers.remove(index);
                if let Some(over) = offer.over.clone() {
                    actions.push(Action::Keep {
                        text: offer.text.clone(),
                        keep: offer.keep,
                        over,
                    });
                }
                actions.extend(offer.stored().map(Action::Unstore));
                settle.extend(offer.slots().cloned());
            }
            (live, actions)
        }
        (live, Event::Discard(index)) => {
            let actions = if index < offers.len() {
                let offer = offers.remove(index);
                settle.extend(offer.slots().cloned());
                offer.stored().map(Action::Unstore).collect()
            } else {
                Vec::new()
            };
            (live, actions)
        }
        // A read-only project offered nothing; its texts move to the copy, whose open offers
        // those of other runs. The page's own text moves with it and stays the page's.
        (live, Event::Copy { slots, to }) => {
            offers.clear();
            settle.clear();
            let mut actions = vec![Action::CopyFolder];
            actions.extend(slots.into_iter().map(|slot| Action::Rekey {
                slot,
                to: to.clone(),
            }));
            (live, actions)
        }
        // A slot the settling frees takes the page's own text at once: it is the only place
        // that held none of it while another run's entry filled it.
        (live, Event::Settle) => {
            let mut actions: Vec<Action<V>> = settle.drain(..).map(Action::Settle).collect();
            if !actions.is_empty() {
                match &live {
                    Live::Clean => {}
                    Live::Typed { base, current } => actions.push(write(current, base, None)),
                    Live::Sending {
                        base,
                        sent,
                        current,
                    }
                    | Live::Landed {
                        base,
                        sent,
                        current,
                        ..
                    } => actions.push(write(current, base, Some(sent))),
                }
            }
            (live, actions)
        }
    };
    (
        FieldDraft {
            live,
            offers,
            settle,
        },
        actions,
    )
}
