//! The conflict review (unit B11): the port of tC4's `test/conflictReview.test.tsx` as the
//! decisions its view draws (which conflict shows, the selection, the tabs, the footer's
//! words), the review's half of `test/actorLabel.test.ts` (a version is labelled as the sync
//! result labels its teammate, #680), and the double-submit rule: a second press of one choice
//! sends the first press's request, whose first reply the server gives again (A-27).

use tc_api::check::Tool;
use tc_api::conflicts::{
    ConflictEntry, ConflictKind, KeepChoice, MadeBy, NoticeView, ReviewPlace, ReviewTab, TabCount,
    Version, VersionValue,
};
use tc_api::ids::{ChangeRef, ConflictRef, ProjectRef, RecordRef, RequestId, WallMs};
use tc_api::jobs::SyncResultView;
use tc_api::shown::{Computer, PersonLabel, Who};
use tc_api::write::SaveRefusal;
use tc_api::{ApiError, NotSaved};
use tc_core::BookId;
use tc_ui::app::client::Transport;
use tc_ui::app::net::NetEnd;
use tc_ui::app::review::{
    BackTo, Choices, KeepOutcome, KeepPhase, KeepReply, Owner, Press, ReviewState, back_label,
    choices, consequence, failure_text, group_others, keep_passage_label, kept_name, made_by,
    notice_text, persons, picks_of, place_book, place_label, tabs_shown,
};
use tc_ui::app::sync::{SyncEnd, SyncToast, person_label};

fn conflict(c: char) -> ConflictRef {
    ConflictRef::parse(&c.to_string().repeat(64)).unwrap()
}

fn change(c: char) -> ChangeRef {
    ChangeRef::parse(&c.to_string().repeat(64)).unwrap()
}

/// *Keep this version* of change `c` in conflict `k`.
fn press_keep(k: char, c: char) -> Press {
    Press::Keep {
        conflict: conflict(k),
        choice: KeepChoice::Candidate { change: change(c) },
    }
}

fn person(who: Who) -> MadeBy {
    MadeBy::Person(PersonLabel {
        who,
        computer: None,
    })
}

fn version(c: char, by: MadeBy) -> Version {
    Version {
        change: change(c),
        place: ReviewPlace::Verse {
            book: BookId::Titus,
            chapter: 1,
            verse: 1,
        },
        by,
        at: WallMs(1_790_812_860_000),
        clock_ahead: false,
        shown: false,
        value: VersionValue::Text("Pablo".into()),
    }
}

/// A verse conflict at Titus 1:`verse`, in `group`, with `versions`.
fn entry(id: char, verse: u16, group: u32, versions: Vec<Version>) -> ConflictEntry {
    ConflictEntry {
        conflict: conflict(id),
        kind: ConflictKind::VerseText,
        record: RecordRef::parse(&format!(
            r#"{{"book":"TIT","chapter":1,"kind":"verse","verse":{verse}}}"#
        ))
        .unwrap(),
        place: ReviewPlace::Verse {
            book: BookId::Titus,
            chapter: 1,
            verse,
        },
        group,
        versions,
        review: Vec::new(),
        incomplete_history: false,
        unreadable: 0,
    }
}

fn three() -> Vec<ConflictEntry> {
    let two = || {
        vec![
            version('b', person(Who::Teammate(2))),
            version('a', person(Who::Mine)),
        ]
    };
    vec![
        entry('1', 1, 0, two()),
        entry('2', 2, 1, two()),
        entry('4', 4, 1, two()),
    ]
}

/// #680: a version's owner has the one label the sync result gives the same person, and a
/// change no person typed here names where it came from.
#[test]
fn a_version_is_labelled_as_the_sync_result_labels_its_person() {
    for who in [
        Who::Mine,
        Who::Username("door43-c".into()),
        Who::DisplayName("María".into()),
        Who::Teammate(2),
    ] {
        let label = PersonLabel {
            who: who.clone(),
            computer: None,
        };
        assert_eq!(made_by(&person(who)), person_label(&label));
    }
    assert_eq!(made_by(&person(Who::Mine)), "Mine");
    assert_eq!(made_by(&person(Who::Teammate(2))), "Teammate 2");
    let sources = [MadeBy::Door43, MadeBy::ExchangeFile, MadeBy::Import].map(|by| made_by(&by));
    assert!(sources.iter().all(|s| !s.is_empty()));
    assert_eq!(
        sources.len(),
        sources
            .iter()
            .collect::<std::collections::BTreeSet<_>>()
            .len(),
        "each source has its own words"
    );
}

/// tC4 "colors follow the owner": "Mine" is light blue on every conflict and every teammate's
/// version cream, whatever its place in the list.
#[test]
fn colours_follow_the_owner_not_the_position() {
    let mine_first = [person(Who::Mine), person(Who::Teammate(1))];
    let mine_last = [person(Who::Username("juan".into())), person(Who::Mine)];
    for order in [mine_first, mine_last] {
        for by in &order {
            let owner = Owner::of(by);
            let expected = if matches!(by, MadeBy::Person(l) if l.who == Who::Mine) {
                (Owner::Mine, "var(--uw-inspire-200)", "mine")
            } else {
                (Owner::Teammate, "var(--uw-cream)", "teammate")
            };
            assert_eq!((owner, owner.fill(), owner.attr()), expected);
        }
    }
    assert_eq!(Owner::of(&MadeBy::Door43), Owner::Teammate);
}

/// STORAGE §8.5: when versions from two computers carry one person's label, each names its
/// computer, and only this computer's is drawn and named as Mine: one person's other
/// computer comes under its username, a team account's computers by their numbers.
#[test]
fn versions_from_two_computers_name_their_computer() {
    let by = |who: Who, computer: Computer| {
        MadeBy::Person(PersonLabel {
            who,
            computer: Some(computer),
        })
    };
    let here = by(Who::Mine, Computer::This);
    let there = by(Who::Username("ana".into()), Computer::Other(1));
    let team = by(Who::Username("equipo".into()), Computer::Other(2));
    assert_eq!(made_by(&here), "Mine · this computer");
    assert_eq!(made_by(&there), "ana · computer 1");
    assert_eq!(made_by(&team), "equipo · computer 2");
    assert_eq!(
        [&here, &there, &team].map(Owner::of),
        [Owner::Mine, Owner::Teammate, Owner::Teammate]
    );
    assert_eq!(kept_name(&version('a', here)), "Your version");
    assert!(kept_name(&version('b', there)).contains("ana · computer 1"));
}

/// tC4 "navigation n of m": the review shows the conflict asked for, else the first; Previous
/// and Next show another with nothing selected.
#[test]
fn the_review_shows_the_conflict_asked_for_and_steps_clear_the_selection() {
    let entries = three();
    assert_eq!(
        ReviewState::at(ReviewTab::Text, None).shown(&entries),
        Some(0)
    );
    let mut state = ReviewState::at(ReviewTab::Text, Some(conflict('2')));
    assert_eq!(state.shown(&entries), Some(1));
    assert_eq!(state.shown(&[]), None, "no conflict: nothing shows");
    state.selected = Some(change('a'));
    state.history = Some(entries[1].record.clone());
    state.show(conflict('2'), 1);
    assert_eq!(
        state.selected,
        Some(change('a')),
        "the same conflict keeps its selection"
    );
    state.show(conflict('4'), 2);
    assert_eq!(state.shown(&entries), Some(2));
    assert_eq!((state.selected, state.history), (None, None));
}

/// tC4 "after it the conflict leaves the list": once kept, the conflict now at its place
/// shows, so m goes down by one; after the last one, the one before it.
#[test]
fn after_a_keep_the_conflict_now_at_its_place_shows() {
    let entries = three();
    let mut state = ReviewState::at(ReviewTab::Text, Some(conflict('2')));
    state.position = 1;
    let after: Vec<ConflictEntry> = entries
        .iter()
        .filter(|e| e.conflict != conflict('2'))
        .cloned()
        .collect();
    assert_eq!(state.shown(&after), Some(1));
    assert_eq!(after[1].conflict, conflict('4'));
    let mut last = ReviewState::at(ReviewTab::Text, Some(conflict('4')));
    last.position = 2;
    assert_eq!(last.shown(&entries[..2]), Some(1));
}

/// A mint of request ids that counts what it minted.
fn minter() -> (impl FnMut() -> RequestId, std::rc::Rc<std::cell::Cell<u8>>) {
    let minted = std::rc::Rc::new(std::cell::Cell::new(0u8));
    let count = minted.clone();
    let mint = move || {
        count.set(count.get() + 1);
        RequestId::from_bytes([count.get(); 16])
    };
    (mint, minted)
}

/// A-27: after a lost reply, a second press of Keep for the same version of the same
/// conflict sends the first press's request, so the server answers it with its first reply
/// and writes nothing more; another choice is a new request.
#[test]
fn a_second_press_of_one_choice_sends_the_first_request() {
    let (mut mint, minted) = minter();
    let mut keep = KeepPhase::Idle;
    let first = keep.press(&press_keep('1', 'a'), &mut mint).unwrap();
    keep.reply(
        in_flight(&keep),
        KeepReply::Lost("the app's server did not answer".into()),
    );
    assert_eq!(
        keep.outcome(),
        Some(KeepOutcome::NotKept(
            "the app's server did not answer".into()
        )),
        "a lost reply says why"
    );
    assert!(!keep.busy(), "after a lost reply the button is on again");
    let again = keep.press(&press_keep('1', 'a'), &mut mint).unwrap();
    assert_eq!(first, again);
    keep.reply(in_flight(&keep), KeepReply::Lost(String::new()));
    let other = keep.press(&press_keep('1', 'b'), &mut mint).unwrap();
    assert_ne!(other, first);
    keep.reply(in_flight(&keep), KeepReply::Lost(String::new()));
    let elsewhere = keep.press(&press_keep('2', 'b'), &mut mint).unwrap();
    assert_ne!(elsewhere, other);
    assert_eq!(minted.get(), 3);
}

/// Once the server has answered a keep (a refusal such as "another version was kept"), the
/// next press is a new request: the same one would get the refusal again. Until it answers,
/// a press after a lost reply sends the same request.
#[test]
fn a_press_after_an_answer_is_a_new_request() {
    let (mut mint, _) = minter();
    let mut keep = KeepPhase::Idle;
    let first = keep.press(&press_keep('1', 'a'), &mut mint).unwrap();
    keep.reply(in_flight(&keep), KeepReply::Lost(String::new()));
    assert_eq!(
        keep.press(&press_keep('1', 'a'), &mut mint),
        Some(first),
        "a lost reply: the same request"
    );
    keep.reply(
        in_flight(&keep),
        KeepReply::Refused("Another version was kept.".into()),
    );
    assert_eq!(
        keep.outcome(),
        Some(KeepOutcome::NotKept("Another version was kept.".into())),
        "a refusal is shown as not kept"
    );
    let next = keep.press(&press_keep('1', 'a'), &mut mint).unwrap();
    assert_ne!(next, first, "answered: a new request");
    keep.reply(in_flight(&keep), KeepReply::Kept { disk_warning: None });
    assert_eq!(keep.outcome(), None, "kept with nothing to say");
    keep.press(&press_keep('1', 'b'), &mut mint).unwrap();
    keep.reply(
        in_flight(&keep),
        KeepReply::Kept {
            disk_warning: Some("the disk complained".into()),
        },
    );
    assert_eq!(
        keep.outcome(),
        Some(KeepOutcome::DiskWarning("the disk complained".into()))
    );
}

/// INV-ONE-RESOLVE-PER-PRESS: while a keep is in flight a press sends nothing, and the
/// review says nothing about an outcome yet; Previous or Next after an answer clears what it
/// said, but keeps a lost reply's request for the same press.
#[test]
fn a_press_while_a_keep_is_in_flight_sends_nothing() {
    let (mut mint, minted) = minter();
    let mut keep = KeepPhase::Idle;
    let first = keep.press(&press_keep('1', 'a'), &mut mint).unwrap();
    assert!(keep.busy());
    keep.reply(
        RequestId::from_bytes([9; 16]),
        KeepReply::Refused("not mine".into()),
    );
    assert!(
        keep.busy() && keep.outcome().is_none(),
        "a reply to another press answers nothing here"
    );
    assert_eq!(keep.press(&press_keep('1', 'a'), &mut mint), None);
    assert_eq!(keep.press(&press_keep('2', 'b'), &mut mint), None);
    assert_eq!((keep.outcome(), minted.get()), (None, 1));
    keep.reply(in_flight(&keep), KeepReply::Refused("no".into()));
    assert!(keep.outcome().is_some());
    keep.dismiss();
    assert_eq!(
        keep,
        KeepPhase::Idle,
        "another conflict: the answer is gone"
    );
    let second = keep.press(&press_keep('1', 'a'), &mut mint).unwrap();
    assert_ne!(second, first);
    keep.reply(in_flight(&keep), KeepReply::Lost("gone".into()));
    keep.dismiss();
    assert_eq!(
        keep.press(&press_keep('1', 'a'), &mut mint),
        Some(second),
        "a lost reply survives a step: the same request goes again"
    );
    let mut state = ReviewState::at(ReviewTab::Text, None);
    state.keep = keep;
    state.choose_tab(ReviewTab::Alignments);
    assert_eq!(state.keep, KeepPhase::Idle, "another tab: a new start");
}

fn counts(text: u32, alignments: u32, notices: u32) -> Vec<TabCount> {
    ReviewTab::ALL
        .into_iter()
        .map(|tab| TabCount {
            tab,
            count: match tab {
                ReviewTab::Text => text,
                ReviewTab::Alignments => alignments,
                ReviewTab::Notices => notices,
                _ => 0,
            },
        })
        .collect()
}

/// tC4's review has one tab: with only verse conflicts no tab strip shows. Once another kind
/// holds something, the strip shows Text, the tabs that hold something, and the one shown.
#[test]
fn the_tab_strip_shows_only_when_another_kind_holds_something() {
    assert!(tabs_shown(&counts(3, 0, 0), ReviewTab::Text).is_empty());
    let tabs = |shown| -> Vec<ReviewTab> {
        tabs_shown(&counts(1, 2, 1), shown)
            .into_iter()
            .map(|t| t.tab)
            .collect()
    };
    assert_eq!(
        tabs(ReviewTab::Text),
        vec![ReviewTab::Text, ReviewTab::Alignments, ReviewTab::Notices]
    );
    assert_eq!(
        tabs(ReviewTab::Detached),
        vec![
            ReviewTab::Text,
            ReviewTab::Alignments,
            ReviewTab::Detached,
            ReviewTab::Notices
        ]
    );
    let mut state = ReviewState::at(ReviewTab::Text, Some(conflict('1')));
    state.selected = Some(change('a'));
    state.choose_tab(ReviewTab::Alignments);
    assert_eq!(state, ReviewState::at(ReviewTab::Alignments, None));
}

/// tC4's `KeepBar`: what Keep this version does, with the kept version's name between its
/// two halves.
#[test]
fn the_footer_says_what_the_kept_version_replaces() {
    let (before, after) = consequence(2);
    assert_eq!(
        format!("{before}<v>{after}"),
        "<v> replaces the other version. It stays in the project history."
    );
    let (before, after) = consequence(4);
    assert_eq!(
        format!("{before}<v>{after}"),
        "<v> replaces the other 3 versions. They stay in the project history."
    );
    assert_eq!(kept_name(&version('a', person(Who::Mine))), "Your version");
    assert_eq!(
        kept_name(&version('b', person(Who::Teammate(2)))),
        "Teammate 2’s version"
    );
}

#[test]
fn places_and_back_are_named_in_words() {
    assert_eq!(place_label(&three()[0].place), "Titus 1:1");
    let check = ReviewPlace::Check {
        tool: Some(Tool::TranslationWords),
        place: "TIT 1:4".parse().unwrap(),
    };
    assert!(place_label(&check).starts_with("Titus 1:4 · "));
    assert_eq!(
        back_label(BackTo::Translate(Some(BookId::Titus), 2)),
        "Back to Titus 2"
    );
    assert_eq!(back_label(BackTo::Translate(None, 1)), "Back to Translate");
    assert_eq!(back_label(BackTo::Check), "Back to Check");
}

/// The review of a project opened from Home opens Translate at the book of its first
/// conflict (tC4 `openProjectConflicts`), a check's included; a story or the whole project
/// names no book, so Translate stays where the open put it.
#[test]
fn translate_opens_at_the_book_of_the_first_conflict() {
    assert_eq!(place_book(&three()[0].place), Some(BookId::Titus));
    let check = ReviewPlace::Check {
        tool: Some(Tool::TranslationNotes),
        place: "JON 2:1".parse().unwrap(),
    };
    assert_eq!(place_book(&check), Some(BookId::Jonah));
    let frame = ReviewPlace::Story {
        story: tc_api::ids::StoryNo::new(3).unwrap(),
        frame: Some(2),
    };
    assert_eq!(place_book(&frame), None);
    assert_eq!(place_book(&ReviewPlace::Project), None);
}

/// One set of changes that made two conflicts says so on each (STORAGE §8.1).
#[test]
fn conflicts_made_together_name_each_other() {
    let entries = three();
    assert_eq!(group_others(&entries, &entries[0]), 0);
    assert_eq!(group_others(&entries, &entries[1]), 1);
    assert_eq!(group_others(&entries, &entries[2]), 1);
}

/// The sync toast's Review opens on the verses, or on the alignments when only a verse with
/// two alignments waits (#791), never on an empty tab.
#[test]
fn the_toasts_review_opens_where_something_waits() {
    let toast = |verse_conflicts, alignments| SyncToast {
        project: ProjectRef::parse("equipo").unwrap(),
        end: SyncEnd::Synced(SyncResultView {
            received: 2,
            from: Vec::new(),
            kept_newest: 0,
            verse_conflicts,
            alignments,
            restored: 0,
            missing_pins: 0,
            replaced: Vec::new(),
        }),
    };
    assert_eq!(toast(1, 0).review_tab(), ReviewTab::Text);
    assert_eq!(toast(1, 1).review_tab(), ReviewTab::Text);
    assert_eq!(toast(0, 1).review_tab(), ReviewTab::Alignments);
}

/// Every notice has its own sentence with its values in it, and never a placeholder left.
#[test]
fn every_notice_has_its_own_sentence() {
    let file = || "ingredients/TIT.usfm".to_owned();
    let all = vec![
        NoticeView::DamagedChanges { recovered: 212 },
        NoticeView::NewerApp,
        NoticeView::ClockAhead { by: None, days: 3 },
        NoticeView::OutsideEditRecorded {
            file: file(),
            changes: 4,
        },
        NoticeView::OutsideEditHeld { file: file() },
        NoticeView::OutsideEditUnreadable { file: file() },
        NoticeView::DetailsEditedOutside,
        NoticeView::WriteBlocked { file: file() },
        NoticeView::FileRepaired { file: file() },
        NoticeView::FileMissing { file: file() },
        NoticeView::FileUnreadable { file: file() },
        NoticeView::StorageDamage { file: file() },
        NoticeView::FileMoved {
            file: "ingredients/tit.USFM".into(),
            to: file(),
        },
        NoticeView::FolderCopied,
        NoticeView::SecondWriter,
        NoticeView::SavedWithDiskError,
        NoticeView::ImportUnfinished,
        NoticeView::IncompleteChanges { have: 3, of: 5 },
        NoticeView::KeptApart { changes: 2 },
    ];
    let texts: Vec<String> = all.iter().map(notice_text).collect();
    for (notice, text) in all.iter().zip(&texts) {
        assert!(
            !text.is_empty() && !text.contains('{'),
            "{notice:?}: {text}"
        );
    }
    assert!(texts[0].contains("212"));
    assert!(texts[2].contains("another computer") && texts[2].contains('3'));
    assert!(texts[3].contains("ingredients/TIT.usfm"));
    let unique: std::collections::BTreeSet<&String> = texts.iter().collect();
    assert_eq!(unique.len(), texts.len());
}

/// INV-EVERY-KIND-HAS-A-CHOICE: every kind of conflict has its choices, from one match with
/// no wildcard: the Text tab's kinds a choice this version makes, the others `Later`, with
/// every version kept.
#[test]
fn every_kind_has_its_choices() {
    use ConflictKind as K;
    let expected = [
        (K::VerseText, Choices::Candidate),
        (K::VerseStructure, Choices::JoinedOrSeparate),
        (K::Passage, Choices::PassageSides),
        (K::StructureIssue, Choices::Candidate),
        (K::BookHeader, Choices::Candidate),
        (K::ChapterFront, Choices::Candidate),
        (K::BookPresence, Choices::Later),
        (K::BookRemovedWhileEdited, Choices::Later),
        (K::LastBookRemoved, Choices::Later),
        (K::BookLayout, Choices::Later),
        (K::Alignment, Choices::Later),
        (K::Check, Choices::Later),
        (K::CheckMoved, Choices::Later),
        (K::Comment, Choices::Later),
        (K::Pins, Choices::Later),
        (K::Setting, Choices::Later),
        (K::ProjectMeta, Choices::Later),
        (K::Versification, Choices::Later),
        (K::StoryBase, Choices::Candidate),
        (K::Frame, Choices::Candidate),
        (K::StoryRef, Choices::Candidate),
        (K::CarriedFile, Choices::Later),
        (K::PathCase, Choices::Later),
        (K::DeviceProfile, Choices::Later),
        (K::Unknown, Choices::Later),
    ];
    assert_eq!(expected.len(), 25, "every variant of ConflictKind");
    for (kind, offered) in expected {
        assert_eq!(choices(kind), offered, "{kind:?}");
    }
    // A passage's sides are its persons, each once, with their changes as picks; a change
    // no person made here is no side.
    let mut entry = three().remove(0);
    entry.kind = ConflictKind::Passage;
    entry.versions.push(Version {
        value: VersionValue::Joined { by: 1 },
        ..version('c', person(Who::Teammate(2)))
    });
    entry.versions.push(version('d', MadeBy::Door43));
    let sides = persons(&entry);
    assert_eq!(
        sides.iter().map(|p| p.who.clone()).collect::<Vec<_>>(),
        vec![Who::Teammate(2), Who::Mine]
    );
    assert_eq!(picks_of(&entry, &sides[0]), vec![change('b'), change('c')]);
    assert_eq!(picks_of(&entry, &sides[1]), vec![change('a')]);
    assert_eq!(keep_passage_label(&sides[0]), "Keep Teammate 2’s passage");
    assert_eq!(keep_passage_label(&sides[1]), "Keep my passage");
}

/// A refused choice says why in words: "Look again" says another version was kept, a
/// refusal of the choice gives the store's reason, and a lost reply names what stopped it;
/// nothing reaches the screen as an error's debug text.
#[test]
fn a_look_again_refusal_says_another_version_was_kept() {
    let api = |e: NotSaved| ApiError::NotSaved(e);
    assert_eq!(
        failure_text(&api(NotSaved::LookAgain)),
        "Another version of this verse is kept."
    );
    assert_eq!(
        failure_text(&api(NotSaved::Refused(SaveRefusal::Choice {
            reason: "the chosen version is not one of the conflict's versions".into()
        }))),
        "the chosen version is not one of the conflict's versions"
    );
    let every = [
        api(NotSaved::Refused(SaveRefusal::IncoherentView)),
        api(NotSaved::Refused(SaveRefusal::LastBook)),
        api(NotSaved::Disk("the disk is full".into())),
        api(NotSaved::ReadOnly(
            tc_api::write::ReadOnlyReason::SecondWriter,
        )),
        api(NotSaved::PlanStale),
        api(NotSaved::StaleScope),
        api(NotSaved::BadToken),
    ];
    for error in &every {
        let text = failure_text(error);
        assert!(
            !text.is_empty() && !text.contains('{') && !text.contains("Refused("),
            "{error:?}: {text}"
        );
    }
    // No usable answer: the reply is lost, in words, never as a refusal.
    for lost in [
        NetEnd::<()>::Dropped,
        NetEnd::Transport(Transport::Status(503)),
    ] {
        let reply =
            tc_ui::app::review::keep_reply(lost, |()| KeepReply::Kept { disk_warning: None });
        assert!(
            matches!(&reply, KeepReply::Lost(text) if !text.is_empty() && !text.contains('{')),
            "{reply:?}"
        );
    }
    let mut keep = KeepPhase::Idle;
    let (mut mint, _) = minter();
    keep.press(&keep_press(), &mut mint).unwrap();
    keep.reply(
        in_flight(&keep),
        tc_ui::app::review::keep_reply::<()>(NetEnd::Failed(api(NotSaved::LookAgain)), |()| {
            KeepReply::Kept { disk_warning: None }
        }),
    );
    assert_eq!(
        keep.outcome(),
        Some(KeepOutcome::NotKept(
            "Another version of this verse is kept.".into()
        ))
    );
}

/// The request of the press in flight.
fn in_flight(keep: &KeepPhase) -> RequestId {
    match keep {
        KeepPhase::InFlight(sent) => sent.request,
        other => panic!("{other:?} has no press in flight"),
    }
}

fn keep_press() -> Press {
    press_keep('1', 'a')
}

/// The screens of the H5 packet the ruling kept all draw, and the conflict cards of the kinds
/// whose newest version now wins are gone from it.
#[test]
fn the_packet_draws_every_screen_the_ruling_kept() {
    use tc_ui::app::q9::{SCREENS, screen};

    for name in SCREENS {
        assert!(screen(name).is_some(), "{name}");
    }
    for gone in [
        "checks",
        "alignments",
        "settings-carried",
        "settings-path-case",
    ] {
        assert!(screen(gone).is_none(), "{gone}");
    }
}
