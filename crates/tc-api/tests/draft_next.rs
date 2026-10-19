//! `model::draft::next` over every pair of state and event (APP-DESIGN §6.6, C1b): no pair
//! drops a text silently. A text the state held before an event is, after it, still in the
//! state, or in an action that puts it in a place (the buffer, the mirror, `drafts.json`, the
//! record), or dropped only where the table says why: the person discarded it, the record holds
//! it, or it lived only in the page's memory and every keystroke had put it in the buffer.

#![cfg(not(target_arch = "wasm32"))]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use tc_api::buffer::{BufferKey, FieldKey, FieldKind, FieldPlace};
use tc_api::ids::{ProjectId, ProjectRef, ServerInstance, WallMs};
use tc_api::model::draft::{
    Action, Base, Candidate, Event, FieldDraft, Home, Live, Offer, Record, Text, next, same_text,
};
use tc_api::session::{BufferSlot, Keep, StoredDraftId};

type V = u32;

/// Version 0 holds "seed", 1 "saved"; 1 descends from 0 and is the only head.
struct Rec;

impl Record<V> for Rec {
    fn heads(&self) -> Vec<String> {
        vec!["saved".to_owned()]
    }
    fn sole(&self) -> Option<V> {
        Some(1)
    }
    fn text_of(&self, version: &V) -> Option<String> {
        Some(if *version == 0 { "seed" } else { "saved" }.to_owned())
    }
    fn held_since(&self, version: &V, text: &str) -> bool {
        *version == 0 && text == "saved"
    }
}

fn key() -> BufferKey {
    BufferKey {
        project: ProjectRef::parse("en_ult").unwrap(),
        id: ProjectId::parse("6f1c0e9a-2b7d-4c3e-8a5f-0b1d2c3e4f50").unwrap(),
        field: FieldKey {
            kind: FieldKind::Verse,
            place: FieldPlace::parse("TIT 1:1").unwrap(),
        },
    }
}

fn slot(text: &str) -> BufferSlot {
    BufferSlot {
        key: key(),
        instance: ServerInstance::from_bytes([7; 16]),
        text: text.to_owned(),
        at: WallMs(5),
    }
}

fn text(t: &str) -> Text {
    Text {
        text: t.to_owned(),
        at: 10,
    }
}

fn lives() -> Vec<Live<V>> {
    vec![
        Live::Clean,
        Live::Typed {
            base: 0,
            current: text("typed"),
        },
        Live::Sending {
            base: 0,
            sent: "sent".to_owned(),
            current: text("typed on"),
        },
        Live::Landed {
            base: 0,
            over: 1,
            sent: "sent".to_owned(),
            current: text("typed on"),
        },
        Live::Landed {
            base: 0,
            over: 1,
            sent: "sent".to_owned(),
            current: text("sent"),
        },
    ]
}

fn offers() -> Vec<Vec<Offer<V>>> {
    vec![
        Vec::new(),
        vec![
            Offer {
                text: "offered".to_owned(),
                at: 3,
                keep: Keep::Save,
                over: Some(0),
                homes: vec![Home::Stored(StoredDraftId(4))],
            },
            Offer {
                text: "copy only".to_owned(),
                at: 2,
                keep: Keep::CopyText,
                over: None,
                homes: vec![Home::Buffer(slot("copy only"))],
            },
        ],
    ]
}

/// One candidate of each verdict: saved, undone, offered with a landed save, offered, and a
/// stored copy of a buffer entry's run.
fn candidates() -> Vec<Candidate<V>> {
    let buffer = |t: &str, base: Base<V>, sent: Option<&str>, at: i64| Candidate {
        text: t.to_owned(),
        at,
        base,
        sent: sent.map(str::to_owned),
        run: Some(ServerInstance::from_bytes([7; 16])),
        home: Home::Buffer(slot(t)),
    };
    vec![
        buffer("saved", Base::Verified(0), None, 1),
        buffer("seed", Base::Verified(0), None, 2),
        buffer("seed", Base::Verified(0), Some("saved"), 3),
        buffer("new", Base::Unverified(0), None, 4),
        Candidate {
            text: "older copy".to_owned(),
            at: 0,
            base: Base::Verified(0),
            sent: None,
            run: Some(ServerInstance::from_bytes([7; 16])),
            home: Home::Stored(StoredDraftId(9)),
        },
    ]
}

#[derive(Debug, Clone, Copy)]
enum Ev {
    Typed,
    DraftTick,
    SaveSent,
    SaveLanded,
    SaveAcked,
    SaveRefused,
    Reload,
    Relaunch,
    Offer,
    Keep(usize),
    Discard(usize),
    Copy,
    Settle,
}

const EVENTS: [Ev; 16] = [
    Ev::Typed,
    Ev::DraftTick,
    Ev::SaveSent,
    Ev::SaveLanded,
    Ev::SaveAcked,
    Ev::SaveRefused,
    Ev::Reload,
    Ev::Relaunch,
    Ev::Offer,
    Ev::Keep(0),
    Ev::Keep(1),
    Ev::Keep(5),
    Ev::Discard(0),
    Ev::Discard(5),
    Ev::Copy,
    Ev::Settle,
];

fn event<'a>(ev: Ev, record: &'a Rec) -> Event<'a, V> {
    match ev {
        Ev::Typed => Event::Typed {
            text: "keystroke".to_owned(),
            at: 20,
            shown: 1,
        },
        Ev::DraftTick => Event::DraftTick,
        Ev::SaveSent => Event::SaveSent,
        Ev::SaveLanded => Event::SaveLanded { over: 1 },
        Ev::SaveAcked => Event::SaveAcked,
        Ev::SaveRefused => Event::SaveRefused,
        Ev::Reload => Event::Reload,
        Ev::Relaunch => Event::Relaunch,
        Ev::Offer => Event::Offer {
            candidates: candidates(),
            record,
            mirror: false,
        },
        Ev::Keep(i) => Event::Keep(i),
        Ev::Discard(i) => Event::Discard(i),
        Ev::Copy => Event::Copy {
            slots: vec![slot("moved")],
            to: key(),
        },
        Ev::Settle => Event::Settle,
    }
}

/// The page's text: what it holds now. A save's text is an earlier state of the same typing,
/// which the text now supersedes.
fn live_texts(live: &Live<V>) -> Vec<String> {
    match live {
        Live::Clean => Vec::new(),
        Live::Typed { current, .. }
        | Live::Sending { current, .. }
        | Live::Landed { current, .. } => vec![current.text.clone()],
    }
}

fn state_texts(state: &FieldDraft<V>) -> Vec<String> {
    let mut texts = live_texts(&state.live);
    texts.extend(state.offers.iter().map(|o| o.text.clone()));
    texts.extend(state.settle.iter().map(|s| s.text.clone()));
    texts
}

/// The texts an action puts somewhere, or answers for.
fn action_texts(action: &Action<V>) -> Vec<String> {
    match action {
        Action::Write { text, sent, .. } => {
            let mut texts = vec![text.clone()];
            texts.extend(sent.clone());
            texts
        }
        Action::Remove { text }
        | Action::Draft { text, .. }
        | Action::Send { text, .. }
        | Action::Keep { text, .. } => vec![text.clone()],
        Action::Absorb { text, then, .. } => {
            let mut texts = vec![text.clone()];
            texts.extend(then.iter().map(|s| s.text.clone()));
            texts
        }
        Action::Settle(slot) | Action::Rekey { slot, .. } => vec![slot.text.clone()],
        Action::Unstore(_) | Action::CopyFolder => Vec::new(),
    }
}

/// The texts an event may drop without a place to show for them, and why.
fn may_drop(ev: Ev, before: &FieldDraft<V>) -> Vec<String> {
    match ev {
        // The page's memory and its answers: every keystroke wrote the buffer, and an offer's
        // texts are in drafts.json or the buffer, where the next offer finds them.
        Ev::Reload | Ev::Relaunch => state_texts(before),
        // The person's keystroke replaces the text before it.
        Ev::Typed => match &before.live {
            Live::Clean => Vec::new(),
            Live::Typed { current, .. }
            | Live::Sending { current, .. }
            | Live::Landed { current, .. } => vec![current.text.clone()],
        },
        // The text a save sent and the record now holds.
        Ev::SaveAcked => live_texts(&before.live),
        // The record holds them, or they were typed back to where they began.
        Ev::Offer => before
            .offers
            .iter()
            .map(|o| o.text.clone())
            .chain(["saved", "seed"].map(str::to_owned))
            .collect(),
        // The person discarded it.
        Ev::Discard(i) => before
            .offers
            .get(i)
            .map(|o| o.text.clone())
            .into_iter()
            .collect(),
        // A copy moves the folder, its drafts.json with it, and every slot of the folder, the
        // ones still to settle among them.
        Ev::Copy => before
            .offers
            .iter()
            .map(|o| o.text.clone())
            .chain(before.settle.iter().map(|s| s.text.clone()))
            .collect(),
        _ => Vec::new(),
    }
}

#[test]
fn no_pair_of_state_and_event_drops_a_text_silently() {
    let record = Rec;
    let mut pairs = 0;
    for live in lives() {
        for offers in offers() {
            for settle in [Vec::new(), vec![slot("to settle")]] {
                for ev in EVENTS {
                    let before = FieldDraft {
                        live: live.clone(),
                        offers: offers.clone(),
                        settle: settle.clone(),
                    };
                    let (after, actions) = next(before.clone(), event(ev, &record));
                    let kept = state_texts(&after);
                    let put: Vec<String> = actions.iter().flat_map(action_texts).collect();
                    let dropped = may_drop(ev, &before);
                    for t in state_texts(&before) {
                        let accounted = [&kept, &put, &dropped]
                            .iter()
                            .any(|list| list.iter().any(|k| same_text(k, &t)));
                        assert!(
                            accounted,
                            "{ev:?} on {before:?} drops {t:?} silently: {after:?}, {actions:?}"
                        );
                    }
                    if let Ev::Offer = ev {
                        // Every candidate is offered, or answered for.
                        for c in candidates() {
                            let offered = after.offers.iter().any(|o| same_text(&o.text, &c.text));
                            let answered =
                                ["saved", "seed", "older copy"].contains(&c.text.as_str());
                            assert!(offered || answered, "{c:?} vanished: {after:?}");
                        }
                    }
                    pairs += 1;
                }
            }
        }
    }
    assert_eq!(pairs, 5 * 2 * 2 * EVENTS.len());
}

/// The rows the review's findings are about, each as one pair.
#[test]
fn the_rows_of_the_findings() {
    let record = Rec;
    // 2: typed back to where it began after its save landed: offered over the stored version.
    let (state, _) = next(
        FieldDraft::default(),
        Event::Offer {
            candidates: vec![Candidate {
                text: "seed".to_owned(),
                at: 1,
                base: Base::Verified(0),
                sent: Some("saved".to_owned()),
                run: None,
                home: Home::Buffer(slot("seed")),
            }],
            record: &record,
            mirror: false,
        },
    );
    assert_eq!(state.offers.len(), 1);
    assert_eq!(state.offers[0].over, Some(1));
    // 1: Keep of one text leaves the field's other question standing.
    let two = FieldDraft {
        offers: vec![
            Offer {
                text: "one".to_owned(),
                at: 2,
                keep: Keep::Save,
                over: Some(0),
                homes: vec![Home::Stored(StoredDraftId(1))],
            },
            Offer {
                text: "two".to_owned(),
                at: 1,
                keep: Keep::Save,
                over: Some(0),
                homes: vec![Home::Stored(StoredDraftId(2))],
            },
        ],
        ..FieldDraft::default()
    };
    let (after, actions) = next(two, Event::Keep(0));
    assert_eq!(after.offers.len(), 1);
    assert!(!actions.contains(&Action::Unstore(StoredDraftId(2))));
    // 5: a run with no buffer entry of the field, behind the shell's mirror, ended clean: its
    // stored draft goes; without the mirror nothing says so, and it is offered.
    let stale = || Candidate {
        text: "older".to_owned(),
        at: 1,
        base: Base::Verified(0),
        sent: None,
        run: Some(ServerInstance::from_bytes([3; 16])),
        home: Home::Stored(StoredDraftId(6)),
    };
    let (state, actions) = next(
        FieldDraft::default(),
        Event::Offer {
            candidates: vec![stale()],
            record: &record,
            mirror: true,
        },
    );
    assert!(state.offers.is_empty());
    assert_eq!(actions, [Action::Unstore(StoredDraftId(6))]);
    let (state, _) = next(
        FieldDraft::default(),
        Event::Offer {
            candidates: vec![stale()],
            record: &record,
            mirror: false,
        },
    );
    assert_eq!(state.offers.len(), 1);
    // 8: settling writes the page's own text again into what it freed.
    let owned = FieldDraft {
        live: Live::Typed {
            base: 0,
            current: text("mine"),
        },
        settle: vec![slot("theirs")],
        ..FieldDraft::default()
    };
    let (_, actions) = next(owned, Event::Settle);
    assert!(
        matches!(&actions[..], [Action::Settle(_), Action::Write { text, .. }] if text == "mine")
    );
}
