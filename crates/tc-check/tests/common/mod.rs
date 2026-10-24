// Each test file uses a different subset of these helpers, and test helpers may panic.
#![allow(dead_code, clippy::panic, clippy::unwrap_used, clippy::expect_used)]

use std::path::PathBuf;

use serde_json::{Value, json};
use tc_check::carry::StoredDecision;
use tc_check::derive::{BookSpec, CheckList, DropReason};
use tc_check::key::reference_json;
use tc_check::status::{DecisionBasis, Edit};
use tc_check::{
    CheckId, CheckItem, CheckKey, CheckTarget, Coverage, DecisionRecord, Judgement, LanguageSets,
    Pin, RefPart, ResourceRef, Selection, SetSlot, SlotPin, TargetText, ToolId, UserStatus,
};
use tc_core::{BookId, GitOid, VerseSpan};
use tc_helps::tsv::{TsvScope, parse_notes, parse_word_links};
use tc_helps::{Note, WordLink};
use tc_journal::decision::verse_text_md5_v1;
use tc_journal::json::{Json, JsonMap};
use tc_journal::keys::SlotName;
use tc_journal::record::{PinGroupValue, SlotValue};
use tc_usfm::Book;
use tc_versification::{StandardScheme, Unplaceable};

pub fn fixture_path(rel: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures")
        .join(rel)
}

pub fn fixture(rel: &str) -> String {
    let path = fixture_path(rel);
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

/// A Door43 resource fixture from tC4: `resource("en_tn@v86/TIT.tsv")`.
pub fn resource(rel: &str) -> String {
    fixture(&format!("tc4/test-fixtures/resources/{rel}"))
}

pub fn notes(rel: &str, book: BookId) -> Vec<Note> {
    parse_notes(&resource(rel), TsvScope::Book(book))
        .unwrap()
        .notes
}

pub fn obs_notes(rel: &str) -> Vec<Note> {
    parse_notes(&resource(rel), TsvScope::Obs).unwrap().notes
}

pub fn word_links(rel: &str, book: BookId) -> Vec<WordLink> {
    parse_word_links(&resource(rel), TsvScope::Book(book)).unwrap()
}

pub fn obs_word_links(rel: &str) -> Vec<WordLink> {
    parse_word_links(&resource(rel), TsvScope::Obs).unwrap()
}

/// A whole-book list in `eng`, the frame of the unfoldingWord helps.
pub fn eng(book: BookId) -> BookSpec<'static> {
    BookSpec {
        book,
        resource_frame: StandardScheme::Eng.scheme(),
        project_frame: Some(StandardScheme::Eng.scheme()),
        scope: &[],
    }
}

/// tC4's name for why a mapping failed (`UnplaceableReason`).
pub fn reason(reason: &DropReason) -> &'static str {
    match reason {
        DropReason::UnknownFrame => "unknown-frame",
        DropReason::Unplaceable(u) => match u {
            Unplaceable::VerseZero => "verse-zero",
            Unplaceable::NoChapter => "no-chapter",
            Unplaceable::PastChapterEnd => "past-chapter-end",
            Unplaceable::Ambiguous => "ambiguous",
            Unplaceable::SpanSplit => "span-split",
            Unplaceable::MalformedReference => "malformed-reference",
        },
        DropReason::OtherBook(book) => panic!("tC4 has no reason for a landing in {book:?}"),
    }
}

/// `[checkId, chapter, verse, occurrence]` as tC4's `contextId` holds them: a part is a
/// JSON number when it is digits.
pub fn identity(item: &CheckItem) -> Vec<Value> {
    let reference = reference_json(&item.key.target);
    vec![
        json!(item.key.check_id.as_str()),
        reference["chapter"].clone(),
        reference["verse"].clone(),
        json!(item.key.occurrence),
    ]
}

pub fn checks_of(list: &CheckList) -> Vec<String> {
    list.items
        .iter()
        .map(|i| i.key.check_id.to_string())
        .collect()
}

/// tC4's test helper: a deterministic fake commit per (repo, version) string.
pub fn sha40(s: &str) -> GitOid {
    let mut h: u32 = 5381;
    for c in s.encode_utf16() {
        h = h.wrapping_mul(33) ^ u32::from(c);
    }
    GitOid::parse(&format!("{h:08x}").repeat(5)).unwrap()
}

pub fn resource_ref(repo: &str, sha: GitOid) -> ResourceRef {
    ResourceRef {
        repo_path: format!("git.door43.org/{repo}").into(),
        sha,
    }
}

/// A pin as tC4's tests write one: `pin("unfoldingWord/en_tn", "v86", &["TIT"])`. An
/// empty book list records no coverage.
pub fn pin(repo: &str, version: &str, books: &[&str]) -> Pin {
    Pin {
        resource: resource_ref(repo, sha40(&format!("{repo}@{version}"))),
        version: Some(version.to_owned()),
        coverage: Coverage::from_codes(books.iter().copied()),
    }
}

// ---- the sample project ------------------------------------------------------------

pub fn sample(rel: &str) -> String {
    fixture(&format!("tc4/sample-burrito/ingredients/{rel}"))
}

/// The content of a verse of a book (its USFM after `\v N `), as a verse record holds it.
pub fn verse_content(usfm: &str, chapter: u16, verse: u16) -> String {
    Book::parse(usfm)
        .verse(chapter, VerseSpan::range(verse, verse).unwrap())
        .unwrap()
        .body()
        .to_owned()
}

/// The text of a verse of a book as the Check screens and the fold read it.
pub fn verse_text(usfm: &str, chapter: u16, verse: u16) -> TargetText {
    TargetText::new(verse_content(usfm, chapter, verse))
}

/// Selections as a tC4 decision writes them.
pub fn selections_of(list: &Value) -> Vec<Selection> {
    list.as_array()
        .unwrap()
        .iter()
        .map(|s| Selection {
            text: s["text"].as_str().unwrap().into(),
            occurrence: u32::try_from(s["occurrence"].as_u64().unwrap()).unwrap(),
            occurrences: u32::try_from(s["occurrences"].as_u64().unwrap()).unwrap(),
            extra: JsonMap::new(),
        })
        .collect()
}

/// A decision with nothing else recorded: no context, category, comment or bookmark.
pub fn decision(judgement: Judgement, resource: &ResourceRef) -> DecisionRecord {
    DecisionRecord {
        context: JsonMap::new(),
        category: None,
        judgement,
        comment: None,
        bookmarked: false,
        resource: resource.clone(),
        imported_invalid: false,
        dismissed: false,
        extra: JsonMap::new(),
    }
}

/// A tC4 decision as an import would read it, stamped with the md5 of `content`.
pub fn decision_of(json: &Value, resource: &ResourceRef, content: &str) -> DecisionRecord {
    let selections = json
        .get("selections")
        .filter(|s| s.is_array())
        .map(selections_of);
    let context = match Json::try_from(&json["contextId"]) {
        Ok(Json::Obj(map)) => map,
        _ => JsonMap::new(),
    };
    DecisionRecord {
        context,
        category: json["category"].as_str().map(Into::into),
        dismissed: false,
        extra: JsonMap::new(),
        judgement: Judgement {
            selections,
            nothing_to_select: json["nothingToSelect"] == true,
            status: match json["status"].as_str() {
                Some("valid") => Some(UserStatus::Valid),
                Some("invalid") => Some(UserStatus::Invalid),
                Some("todo") => Some(UserStatus::Todo),
                _ => None,
            },
            verse_text_md5: verse_text_md5_v1(content),
        },
        comment: json["comments"].as_str().map(str::to_owned),
        bookmarked: json["reminders"] == true,
        resource: resource.clone(),
        imported_invalid: json["invalidated"] == true,
    }
}

/// The resource a tC4 decisions file records.
pub fn file_resource(file: &Value) -> ResourceRef {
    match SlotPin::read(&Json::try_from(&file["resource"]).unwrap()) {
        Some(SlotPin::Usable(pin)) => pin.resource,
        other => panic!("not a pin: {other:?}"),
    }
}

/// The decisions of a tC4 Bible decisions file, keyed as the journal keys them, each
/// stamped with the md5 of its verse in `usfm`.
pub fn stored_decisions(file: &Value, book: BookId, usfm: &str) -> Vec<StoredDecision> {
    let tool = ToolId::parse(file["tool"].as_str().unwrap()).unwrap();
    file["decisions"]
        .as_array()
        .unwrap()
        .iter()
        .map(|json| {
            let context = &json["contextId"];
            let part = |v: &Value| match v {
                Value::Number(n) => {
                    RefPart::from_number(u16::try_from(n.as_u64().unwrap()).unwrap())
                }
                other => RefPart::parse(other.as_str().unwrap()).unwrap(),
            };
            let chapter = part(&context["reference"]["chapter"]);
            let verse = part(&context["reference"]["verse"]);
            let text = verse_content(
                usfm,
                chapter.as_str().parse().unwrap(),
                verse.as_str().parse().unwrap(),
            );
            StoredDecision {
                key: CheckKey {
                    tool,
                    check_id: CheckId::parse(context["checkId"].as_str().unwrap()).unwrap(),
                    target: CheckTarget::Verse {
                        book,
                        chapter,
                        verse,
                    },
                    occurrence: u32::try_from(context["occurrence"].as_u64().unwrap()).unwrap(),
                },
                quote: context["quoteString"].as_str().unwrap().to_owned(),
                group_id: context["groupId"].as_str().unwrap().to_owned(),
                decision: decision_of(json, &file_resource(file), &text),
            }
        })
        .collect()
}

/// The text a target shows, from its content.
pub fn t(content: &str) -> TargetText {
    TargetText::new(content.to_owned())
}

/// The decision after the person's edit, shown `text`, through the journal's one rule
/// (`DecisionEdit::apply`). A first decision starts from an empty context.
pub fn apply(
    decision: Option<&DecisionRecord>,
    edit: Edit,
    text: &str,
    resource: &ResourceRef,
) -> DecisionRecord {
    let basis = DecisionBasis {
        context: JsonMap::new(),
        category: None,
    };
    edit.command(Some(basis), resource)
        .apply(decision, Some(text))
        .unwrap()
}

/// The decisions as the fold projects them for a session, each assessed against its own
/// target's text: the fold's computation, made here without a journal.
pub fn projected<'t>(
    stored: &[StoredDecision],
    resolution: Option<&ResourceRef>,
    text: impl Fn(&CheckTarget) -> Option<&'t TargetText>,
) -> Vec<tc_journal::fold::ProjectedDecision> {
    stored
        .iter()
        .map(|s| tc_journal::fold::ProjectedDecision {
            key: s.key.clone(),
            record: s.decision.clone(),
            assessment: tc_check::status::assess(&s.decision, resolution, text(&s.key.target)),
            detached: !resolution.is_some_and(|r| r.same_resource(&s.decision.resource)),
        })
        .collect()
}

/// The text of a single-verse target of `usfm`, by the no-text rule: none for a verse whose
/// content is empty or the stub `___`.
pub fn target_text(usfm: &str, target: &CheckTarget) -> Option<TargetText> {
    let (c, v) = target.parts();
    let content = verse_content(usfm, c.parse().ok()?, v.parse().ok()?);
    let body = content.trim();
    (!body.is_empty() && body != "___").then(|| TargetText::new(content))
}

pub type LanguageSet = PinGroupValue;
pub type Pins = LanguageSets;
pub type Slot = SetSlot;

/// What tests read and write of one language set.
pub trait Set {
    fn insert(&mut self, slot: SetSlot, pin: Pin);
    /// Sets a slot to a value as tC4 JSON writes it, usable or not.
    fn insert_json(&mut self, slot: SetSlot, value: &serde_json::Value);
    fn get(&self, slot: SetSlot) -> Option<Pin>;
    fn pins(&self) -> Vec<Pin>;
    fn unusable(&self) -> Vec<SetSlot>;
    fn slot_count(&self) -> usize;
    /// Every usable pin as one that records no books.
    fn forget_coverage(&mut self);
}

impl Set for PinGroupValue {
    fn insert(&mut self, slot: SetSlot, pin: Pin) {
        self.set_pin(slot, &pin);
    }

    fn insert_json(&mut self, slot: SetSlot, value: &serde_json::Value) {
        self.0.insert(
            SlotName::parse(slot.name()).unwrap(),
            SlotValue(Json::try_from(value).unwrap()),
        );
    }

    fn get(&self, slot: SetSlot) -> Option<Pin> {
        self.held(slot)?.usable().cloned()
    }

    fn pins(&self) -> Vec<Pin> {
        SetSlot::ALL
            .into_iter()
            .filter_map(|s| self.get(s))
            .collect()
    }

    fn unusable(&self) -> Vec<SetSlot> {
        SetSlot::ALL
            .into_iter()
            .filter(|s| matches!(self.held(*s), Some(SlotPin::Unusable(_))))
            .collect()
    }

    fn slot_count(&self) -> usize {
        SetSlot::ALL
            .into_iter()
            .filter(|s| self.held(*s).is_some())
            .count()
    }

    fn forget_coverage(&mut self) {
        for slot in SetSlot::ALL {
            if let Some(mut pin) = self.get(slot) {
                pin.coverage = Coverage::Unrecorded;
                self.insert(slot, pin);
            }
        }
    }
}
