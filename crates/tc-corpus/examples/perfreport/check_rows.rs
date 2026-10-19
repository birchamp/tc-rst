//! The tc-check rows (PERFORMANCE.md "Checking"), on the open whole-Bible project: the
//! Psalms tN and TWL lists are derived from the real v91 tables, the decisions are the
//! fold's (the corpus decided the real rows of 59 books and generated the rest), and the
//! sessions, the rail, the counts and the carry-over run on them as the Check screen runs
//! them.
//!
//! Two children: `check-open` (everything about Psalms; the first open of a process is the
//! first-time row) and `check-bible` (what an upgrade counts and plans over all 132
//! (tool, book) units).

use crate::common::{header, ms, on_pool, open, store};
use crate::table::Measured;
use std::cell::Cell;
use std::collections::BTreeSet;
use std::path::Path;
use std::time::Instant;
use tc_check::carry::{self, StoredDecision};
use tc_check::change;
use tc_check::derive::{BookSpec, CheckList, notes_for_book, word_links_for_book};
use tc_check::item::{Category, CheckItem, NoteCategory};
use tc_check::rail::{self, Filter, SortMode};
use tc_check::session::{self, Session};
use tc_check::{
    Collection, Ladder, LanguageSets, Pin, ResourceRef, Rung, SetSlot, SlotPin, TargetText, ToolId,
    Unrecorded,
};
use tc_core::{BookId, GitOid};
use tc_corpus::{CheckRow, CheckTool, Corpus, Spec};
use tc_helps::metadata::parse_burrito_metadata;
use tc_helps::tsv::{TsvScope, parse_notes, parse_word_links};
use tc_helps::{BibleRef, HelpItemId, HelpRef, Quote, RowId, WordArticleId, WordCategory};
use tc_journal::fold::{FoldState, ProjectedDecision};
use tc_journal::{PinGroup, Record, RecordKey};
use tc_versification::StandardScheme;

const PSALMS: BookId = BookId::Psalms;
const TN: ToolId = ToolId::TranslationNotes;
const TW: ToolId = ToolId::TranslationWords;

fn fixture(rel: &str) -> String {
    let path = tc_corpus::repo_root().join("fixtures").join(rel);
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

/// The articles of the en_tw v91 pin, which gate the tW checks (the installed set the
/// preflight reads once, not part of an open).
fn articles() -> BTreeSet<WordArticleId> {
    use std::io::Read;
    let path = tc_corpus::repo_root().join("bundle/en/en_tw_v91.sb.zip");
    let file = std::fs::File::open(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    let mut archive = zip::ZipArchive::new(file).expect("the en_tw export");
    let mut text = String::new();
    archive
        .by_name("en_tw/metadata.json")
        .expect("its metadata")
        .read_to_string(&mut text)
        .expect("utf-8");
    parse_burrito_metadata(&text)
        .expect("burrito metadata")
        .word_articles()
}

/// Where the time of an open went, in ms.
#[derive(Debug, Default, Clone, Copy)]
struct Spent {
    read: f64,
    parse: f64,
    derive: f64,
    decisions: f64,
    session: f64,
}

/// Reads one Psalms table and derives the tool's list, as the Check screen does on open.
fn derive(tool: ToolId, articles: &BTreeSet<WordArticleId>, spent: &mut Spent) -> CheckList {
    let scope = TsvScope::Book(PSALMS);
    let spec = BookSpec {
        book: PSALMS,
        resource_frame: StandardScheme::Eng.scheme(),
        project_frame: Some(StandardScheme::Eng.scheme()),
        scope: &[],
    };
    let t = Instant::now();
    let text = if tool == TN {
        fixture("scale/psa/en_tn/tn_PSA.tsv")
    } else {
        fixture("scale/psa/en_twl/twl_PSA.tsv")
    };
    spent.read = ms(t);
    let t = Instant::now();
    if tool == TN {
        let notes = parse_notes(&text, scope).expect("tN").notes;
        spent.parse = ms(t);
        let t = Instant::now();
        let list = notes_for_book(&notes, &spec);
        spent.derive = ms(t);
        list
    } else {
        let links = parse_word_links(&text, scope).expect("TWL");
        spent.parse = ms(t);
        let t = Instant::now();
        let list = word_links_for_book(&links, Some(articles), &spec);
        spent.derive = ms(t);
        list
    }
}

/// One tool's Check screen opened from nothing: the table read and derived, the decisions
/// projected and placed on the checks.
fn open_tool(
    fold: &FoldState,
    sets: &LanguageSets,
    tool: ToolId,
    articles: &BTreeSet<WordArticleId>,
) -> (CheckList, Session, Vec<ProjectedDecision>, usize, Spent) {
    let mut spent = Spent::default();
    let list = derive(tool, articles, &mut spent);
    let resolution = resolution(sets, tool, PSALMS);
    let t = Instant::now();
    let decisions = fold.decisions(tool, Collection::Book(PSALMS));
    spent.decisions = ms(t);
    let t = Instant::now();
    let asked = Cell::new(0usize);
    let session = session::open(list.items.clone(), &decisions, resolution.as_ref(), |_| {
        asked.set(asked.get() + 1);
        None::<&TargetText>
    });
    spent.session = ms(t);
    (list, session, decisions, asked.get(), spent)
}

/// The project's pins as the ladder reads them.
fn ladder(fold: &FoldState) -> LanguageSets {
    let primary = match fold.projected(&RecordKey::Pins(PinGroup::Primary)) {
        Some(Record::Pins { value, .. }) => Some(value),
        _ => None,
    };
    LanguageSets {
        primary,
        fallback: None,
    }
}

fn resolution(sets: &LanguageSets, tool: ToolId, book: BookId) -> Option<ResourceRef> {
    sets.answer(tool, Collection::Book(book), Unrecorded::CoversEveryBook)
        .and_then(|a| a.resource().cloned())
}

/// The Psalms verses that have checks on them, spread over the book.
fn verses_with_checks(session: &Session, n: usize) -> Vec<(u16, u16)> {
    let mut all = Vec::new();
    for chapter in 1..=150u16 {
        for verse in 1..=176u16 {
            if !session.checks_at(chapter, verse).is_empty() {
                all.push((chapter, verse));
            }
        }
    }
    let step = (all.len() / n.max(1)).max(1);
    all.into_iter().step_by(step).take(n).collect()
}

/// Everything about Psalms, in a process of its own: the first open is the first open of a
/// session.
pub fn check_open(dir: &Path) -> Vec<Measured> {
    on_pool(|| check_open_in(dir))
}

fn check_open_in(dir: &Path) -> Vec<Measured> {
    header("check-open");
    let store = store(dir);
    let handle = open(&store, dir);
    let snapshot = handle.snapshot();
    let fold = snapshot.fold();
    let articles = articles();
    let sets = ladder(fold);
    let mut rows = Vec::new();

    // Open the Check screen of each tool from nothing, in the order a person opens them: the
    // tN screen is the first thing this process does; the tW screen follows.
    let (notes, tn, tn_decisions, tn_asked, tn_spent) = open_tool(fold, &sets, TN, &articles);
    let (words, tw, tw_decisions, tw_asked, tw_spent) = open_tool(fold, &sets, TW, &articles);
    let total = |s: &Spent| s.read + s.parse + s.derive + s.decisions + s.session;
    rows.push(Measured::new(
        "check.open_first",
        vec![total(&tn_spent), total(&tw_spent)],
        format!(
            "tN {} checks, {} decisions ({} off the list), texts asked for {tn_asked}: {tn_spent:.1?}; \
             tW {} checks, {} decisions ({} off), texts asked for {tw_asked}: {tw_spent:.1?}",
            tn.items.len(),
            tn_decisions.len(),
            tn.off_list.len(),
            tw.items.len(),
            tw_decisions.len(),
            tw.off_list.len(),
        ),
    ));

    // The same again in this process.
    let mut later = Vec::new();
    for _ in 0..20 {
        for tool in [TN, TW] {
            let started = Instant::now();
            let opened = open_tool(fold, &sets, tool, &articles);
            std::hint::black_box(&opened);
            later.push(ms(started));
        }
    }
    rows.push(Measured::new(
        "check.open_later",
        later,
        "derived again, in the same process, one tool at a time",
    ));

    // With only the derived lists cached, the decisions are projected and placed again.
    let mut again = Vec::new();
    let mut split = (0.0, 0.0);
    for _ in 0..21 {
        for (tool, list) in [(TN, &notes), (TW, &words)] {
            let items = list.items.clone();
            let started = Instant::now();
            let decisions = fold.decisions(tool, Collection::Book(PSALMS));
            let got = ms(started);
            let resolved = resolution(&sets, tool, PSALMS);
            let session =
                session::open(
                    items,
                    &decisions,
                    resolved.as_ref(),
                    |_| None::<&TargetText>,
                );
            std::hint::black_box(&session);
            again.push(ms(started));
            split = (got, ms(started) - got);
        }
    }
    rows.push(Measured::new(
        "check.open_decisions",
        again,
        format!(
            "lists cached, one tool's decisions projected and placed again; last run: the fold's \
             decisions {:.2} ms, the session {:.2} ms",
            split.0, split.1
        ),
    ));

    // From the cache (`CheckListCache`, APP-DESIGN 3.4): the session is held, and what has
    // changed since it was filled patches its rows: here five decisions recorded and five
    // verses edited since, then the counts the screen shows.
    let verses = verses_with_checks(&tn, 200);
    let mut hit = Vec::new();
    let mut cached = tn.clone();
    let tn_resolution = resolution(&sets, TN, PSALMS);
    for round in 0..21usize {
        let started = Instant::now();
        for i in 0..5usize {
            let at = (round * 5 + i) * 31 % cached.items.len();
            let mut state = cached.states[at].clone();
            state.bookmarked = !state.bookmarked;
            cached.set_state(at, state);
        }
        for i in 0..5usize {
            let (chapter, verse) = verses[(round * 5 + i) % verses.len()];
            let edited = TargetText::new(format!("Edit {round}.{i} of Psalm {chapter}"));
            let rows = cached.reassess(
                chapter,
                verse,
                &tn_decisions,
                tn_resolution.as_ref(),
                |_| Some(&edited),
            );
            std::hint::black_box(rows);
        }
        let shown = (cached.progress(), rail::counts(&cached.states));
        hit.push(ms(started));
        std::hint::black_box(shown);
    }
    rows.push(Measured::new(
        "check.open_cached",
        hit,
        "a session held by the cache: ten rows patched, then the counts; handing the rows to \
         the screen is the app's share",
    ));

    // The rail: every filter, in every sort the tool offers, with the chips' counts.
    let mut times = Vec::new();
    let mut work = tc_check::Work::default();
    for (tool, session) in [(TN, &tn), (TW, &tw)] {
        for filter in Filter::ORDER {
            for &mode in SortMode::modes(tool) {
                for _ in 0..5 {
                    let started = Instant::now();
                    let mut counted = tc_check::Work::default();
                    let groups = rail::groups_counted(
                        &session.items,
                        &session.states,
                        filter,
                        mode,
                        &mut counted,
                    );
                    let chips = rail::counts(&session.states);
                    times.push(ms(started));
                    std::hint::black_box((&groups, chips));
                    work = counted;
                }
            }
        }
    }
    rows.push(Measured::new(
        "check.rail",
        times,
        format!("{} samples; the last: {work:?}", 5 * 2 * 5 * 2),
    ));

    // Counts after a decision: the check's new state, then the chips and the progress.
    let mut session = tw.clone();
    let mut times = Vec::new();
    let mut updates = Vec::new();
    for i in 0..200usize {
        let at = (i * 37) % session.items.len();
        let mut state = session.states[at].clone();
        state.nothing_to_select = !state.nothing_to_select;
        state.has_selections = false;
        let started = Instant::now();
        session.set_state(at, state);
        let progress = session.progress();
        let started_chips = Instant::now();
        let chips = rail::counts(&session.states);
        times.push(ms(started));
        updates.push(started_chips.duration_since(started).as_secs_f64() * 1000.0);
        std::hint::black_box((progress, chips));
    }
    let update = crate::table::median(&updates);
    rows.push(Measured::new(
        "check.decision_counts",
        times,
        format!("the progress alone: median {update:.4} ms; the rest is the five chips' counts"),
    ));

    // A verse edit: the checks of the verse only.
    let mut session = tw;
    let decisions = tw_decisions;
    let sets_resolution = resolution(&sets, TW, PSALMS);
    let verses = verses_with_checks(&session, 200);
    let mut times = Vec::new();
    let mut touched = 0usize;
    for (i, &(chapter, verse)) in verses.iter().enumerate() {
        let started = Instant::now();
        let edited = TargetText::new(format!("An edited verse number {i} of Psalm {chapter}"));
        let rows = session.reassess(chapter, verse, &decisions, sets_resolution.as_ref(), |_| {
            Some(&edited)
        });
        times.push(ms(started));
        touched = touched.max(rows.len());
    }
    rows.push(Measured::new(
        "check.reassess",
        times,
        format!("{} verses; the most checks on one: {touched}", verses.len()),
    ));
    rows
}

// ---- the whole Bible --------------------------------------------------------------------

/// The pins after a new release of tN and TWL.
fn next_ladder(current: &LanguageSets) -> LanguageSets {
    let mut next = current.clone();
    let primary = next
        .primary
        .as_mut()
        .expect("the corpus pins a primary set");
    for slot in [SetSlot::TranslationNotes, SetSlot::TranslationWordsLinks] {
        let Some(SlotPin::Usable(mut pin)) = current.held(Rung::Primary, slot) else {
            continue;
        };
        pin.resource = ResourceRef {
            repo_path: pin.resource.repo_path.clone(),
            sha: GitOid::parse(&"ab".repeat(20)).expect("a commit id"),
        };
        pin.version = Some("v92".to_owned());
        let pin: Pin = pin;
        primary.set_pin(slot, &pin);
    }
    next
}

type Units = Vec<(ToolId, BookId, Vec<StoredDecision>)>;

/// Every (tool, book) unit with the decisions the fold holds for it.
fn gather(fold: &FoldState) -> Units {
    let mut units = Vec::new();
    for ordinal in 1..=66u8 {
        let Some(book) = BookId::from_ordinal(ordinal) else {
            continue;
        };
        for tool in [TN, TW] {
            let decisions: Vec<StoredDecision> = fold
                .decisions(tool, Collection::Book(book))
                .into_iter()
                .map(|d| StoredDecision::of(d.key, d.record))
                .collect();
            if !decisions.is_empty() {
                units.push((tool, book, decisions));
            }
        }
    }
    units
}

/// The check items a corpus book's rows make: the key, group, category and quote are what a
/// carry-over reads.
fn items_of(rows: &[CheckRow], tool: CheckTool) -> Vec<CheckItem> {
    rows.iter()
        .filter(|r| r.tool == tool)
        .filter_map(|row| {
            let at = tc_helps::ChapterVerse::new(1, 1)?;
            Some(CheckItem {
                key: tc_corpus::check_key(row).ok()?,
                group_id: row.group.clone(),
                category: match tool {
                    CheckTool::TranslationNotes => Category::Note(NoteCategory::of(&row.group)),
                    CheckTool::TranslationWords => {
                        Category::Word(WordCategory::parse(&row.category)?)
                    }
                },
                quote: Quote::new(row.quote.clone()),
                note: matches!(tool, CheckTool::TranslationNotes).then(|| row.note.clone()),
                source: HelpRef::Bible(BibleRef::verse(row.book, at)),
                help_id: HelpItemId::Row(RowId::parse(&row.id).ok()?),
            })
        })
        .collect()
}

/// The list after a release: every 50th check gone, and every 50th from the 25th with a
/// quote that says something else.
fn released(items: &[CheckItem]) -> Vec<CheckItem> {
    items
        .iter()
        .enumerate()
        .filter(|(i, _)| i % 50 != 0)
        .map(|(i, item)| {
            let mut item = item.clone();
            if i % 50 == 25 {
                item.quote = Quote::new(format!("{} ἄλλος", item.quote.as_str()));
            }
            item
        })
        .collect()
}

/// Four lowercase letters or digits for `n`: a check id.
fn base36(mut n: usize) -> String {
    const DIGITS: &[u8; 36] = b"0123456789abcdefghijklmnopqrstuvwxyz";
    let mut out = [b'a'; 4];
    for slot in out.iter_mut().rev() {
        *slot = DIGITS[n % 36];
        n /= 36;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// What an upgrade counts and plans over the whole Bible's decisions.
pub fn check_bible(dir: &Path) -> Vec<Measured> {
    on_pool(|| check_bible_in(dir))
}

fn check_bible_in(dir: &Path) -> Vec<Measured> {
    header("check-bible");
    let store = store(dir);
    let handle = open(&store, dir);
    let snapshot = handle.snapshot();
    let fold = snapshot.fold();
    let mut rows = Vec::new();

    // What an upgrade counts: the resource each decision of every (tool, book) was made
    // against, read from the fold, and the numbers the ladders make of them. Both parts are
    // timed, five times.
    let current = ladder(fold);
    let next = next_ladder(&current);
    let mut times = Vec::new();
    let (mut gathering, mut counting) = (0.0, 0.0);
    let mut at_risk = 0;
    let (mut units_found, mut decisions_found) = (0, 0);
    for _ in 0..5 {
        let started = Instant::now();
        let mut resources: Vec<(ToolId, BookId, Vec<std::sync::Arc<ResourceRef>>)> = Vec::new();
        for ordinal in 1..=66u8 {
            let Some(book) = BookId::from_ordinal(ordinal) else {
                continue;
            };
            for tool in [TN, TW] {
                let found = fold.decision_resources(tool, Collection::Book(book));
                if !found.is_empty() {
                    resources.push((tool, book, found));
                }
            }
        }
        gathering = ms(started);
        let as_units: Vec<change::ResourceUnit<'_>> = resources
            .iter()
            .map(|(tool, book, resources)| change::ResourceUnit {
                tool: *tool,
                collection: Collection::Book(*book),
                resources,
            })
            .collect();
        let counted = Instant::now();
        let out = change::consequences_by_resource(&current, &next, &as_units, |_| None);
        counting = ms(counted);
        times.push(ms(started));
        at_risk = out.at_risk();
        units_found = resources.len();
        decisions_found = resources.iter().map(|(_, _, r)| r.len()).sum::<usize>();
    }
    rows.push(Measured::new(
        "check.consequences",
        times,
        format!(
            "{units_found} units, {decisions_found} decisions, {at_risk} at risk; the last run read \
             the decisions' resources in {gathering:.0} ms and counted in {counting:.1} ms"
        ),
    ));

    // The carry-over's decisions: whole, as the plan carries them.
    let started = Instant::now();
    let units = gather(fold);
    let gathered = ms(started);
    let decisions: usize = units.iter().map(|(_, _, d)| d.len()).sum();

    // The carry-over: the corpus's rows are the lists of the new release (2% of the rows
    // gone and 2% of the quotes changed), the fold's decisions what is carried.
    let domain = Corpus::domain(&Spec::whole_bible(1)).expect("the corpus domain");
    let resource = ResourceRef {
        repo_path: "git.door43.org/unfoldingWord/en_tn".into(),
        sha: GitOid::parse(&"cd".repeat(20)).expect("a commit id"),
    };
    let mut lists: Vec<(ToolId, BookId, Vec<CheckItem>)> = Vec::new();
    for (tool, book, _) in &units {
        let Some(input) = domain.books.iter().find(|b| b.book == *book) else {
            continue;
        };
        let kind = if *tool == TN {
            CheckTool::TranslationNotes
        } else {
            CheckTool::TranslationWords
        };
        lists.push((*tool, *book, released(&items_of(&input.checks, kind))));
    }
    // The second list of each unit is the worst one for a carry-over: a gateway-language
    // change, where no check keeps its id and every decision is found by its place and
    // quote.
    let language: Vec<Vec<CheckItem>> = lists
        .iter()
        .map(|(_, _, items)| {
            items
                .iter()
                .enumerate()
                .map(|(i, item)| {
                    let mut item = item.clone();
                    item.key.check_id = tc_check::CheckId::parse(&base36(i)).expect("an id");
                    item
                })
                .collect()
        })
        .collect();
    let mut per_unit = Vec::new();
    let mut per_pass = Vec::new();
    let mut worst = (0.0f64, String::new());
    let mut work = tc_check::Work::default();
    for pass in 0..5 {
        let pass_started = Instant::now();
        for (n, ((tool, book, items), (_, _, stored))) in lists.iter().zip(&units).enumerate() {
            for (what, items) in [("release", items), ("language", &language[n])] {
                let started = Instant::now();
                let mut counted = tc_check::Work::default();
                let plan = carry::plan_counted(items, &resource, stored, &mut counted);
                let took = ms(started);
                std::hint::black_box(&plan);
                if pass < 3 {
                    per_unit.push(took);
                }
                if took > worst.0 {
                    worst = (took, format!("{tool:?} {} {what}", book.code()));
                }
                work = counted;
            }
        }
        per_pass.push(ms(pass_started) / 2.0);
    }
    rows.push(Measured::new(
        "check.carry_book",
        per_unit,
        format!(
            "{} units, a release and a language change, three times; slowest {:.0} ms ({}); the \
             last unit's work {work:?}",
            lists.len(),
            worst.0,
            worst.1
        ),
    ));
    rows.push(Measured::new(
        "check.carry_bible",
        per_pass,
        format!(
            "{} units, {decisions} decisions, 5 passes; the decisions were read from the fold in {gathered:.0} ms",
            lists.len()
        ),
    ));
    rows
}
