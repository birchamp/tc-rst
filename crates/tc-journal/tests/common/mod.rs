//! Generators and builders shared by the integration tests.

#![allow(dead_code, clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use proptest::prelude::*;
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Arc;
use tc_journal::json::{Json, JsonInt, JsonMap};
use tc_journal::record::{
    AlignmentGroup, AlignmentValue, BatchReleaseValue, BookPresence, CarriedValue, ChapterFront,
    ChapterVerse, CheckValue, CommentTarget, CommentValue, DecisionRecord, DeviceProfileValue,
    Door43Username, Judgement, LayoutItem, LegacyIdentity, MarkerTerm, MarkerWs, PinGroupValue,
    PresentHow, ResourceRef, Scope, ScopeRange, ScopeRanges, Selection, SlotValue, SourceWord,
    TargetWord, UserStatus, UsfmBytes, VerseValue, VersificationValue,
};
use tc_journal::{
    BatchId, BatchRef, BookId, Cause, Change, ChangeBody, ChangeId, ChapterNo, CheckId, CheckKey,
    CheckTarget, CommentId, DeviceId, DeviceKind, DeviceSeq, ExternalSource, FrameNo, GitOid,
    ImportSource, Md5, MetaField, MetaPath, PinGroup, ProjectId, Record, RefPart, RepoPath,
    ResourcesKey, ResourcesLevel, SettingKey, ShortId, SlotName, StoryNo, SystemReason, ToolId,
    VerseNo, WallTime,
};

pub fn fixtures() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/journal")
}

pub fn project() -> ProjectId {
    ProjectId::parse("6f1c0e9a-2b7d-4c3e-8a5f-0b1d2c3e4f50").unwrap()
}

pub fn other_project() -> ProjectId {
    ProjectId::parse("0b1d2c3e-4f50-4c3e-8a5f-6f1c0e9a2b7d").unwrap()
}

pub fn local(n: u8) -> DeviceId {
    DeviceId::local([n; 15])
}

pub fn chapter(n: u16) -> ChapterNo {
    ChapterNo::new(n).unwrap()
}

pub fn verse_no(n: u16) -> VerseNo {
    VerseNo::new(n).unwrap()
}

pub fn batch(seed: u8, n: u32) -> BatchRef {
    BatchRef::new(BatchId::from_bytes([seed; 16]), n).unwrap()
}

/// A verse change of `device` with the given words.
pub fn verse_body(device: DeviceId, seq: u64, v: u16, words: &str) -> ChangeBody {
    ChangeBody {
        v: 0,
        record: Record::Verse {
            book: BookId::Titus,
            chapter: chapter(1),
            verse: verse_no(v),
            value: VerseValue::Text {
                marker: MarkerWs::canonical(),
                words: words.into(),
                layout: "\n".into(),
                last: verse_no(v),
                from: None,
            },
        },
        parents: vec![],
        device,
        seq: DeviceSeq(seq),
        at: WallTime(1_760_520_000_000 + seq as i64),
        batch: batch(seq as u8, 1),
        cause: Cause::Edit,
    }
}

pub fn verse_change(device: DeviceId, seq: u64, v: u16, words: &str) -> Change {
    Change::new(verse_body(device, seq, v, words)).unwrap()
}

/// The bytes of a change line built by hand from canonical JSON text.
pub fn line_of(json: &str) -> Vec<u8> {
    tc_journal::line::encode_line(ChangeId::of(json.as_bytes()), json.as_bytes())
}

pub fn raw(json: &str) -> Arc<[u8]> {
    Arc::from(json.as_bytes())
}

// ---------------------------------------------------------------------------------------
// Generators
// ---------------------------------------------------------------------------------------

pub fn arb_bytes<const N: usize>() -> impl Strategy<Value = [u8; N]> {
    prop::collection::vec(any::<u8>(), N).prop_map(|v| {
        let mut out = [0u8; N];
        out.copy_from_slice(&v);
        out
    })
}

pub fn arb_change_id() -> impl Strategy<Value = ChangeId> {
    arb_bytes::<32>().prop_map(ChangeId::from_bytes)
}

pub fn arb_md5() -> impl Strategy<Value = Md5> {
    arb_bytes::<16>().prop_map(Md5::from_bytes)
}

pub fn arb_oid() -> impl Strategy<Value = GitOid> {
    arb_bytes::<20>().prop_map(GitOid::from_bytes)
}

pub fn arb_kind() -> impl Strategy<Value = DeviceKind> {
    prop::sample::select(vec![
        DeviceKind::Local,
        DeviceKind::Tc4Actor,
        DeviceKind::Tc3User,
        DeviceKind::Door43Edit,
        DeviceKind::ExchangeEdit,
        DeviceKind::Importer,
    ])
}

pub fn arb_device() -> impl Strategy<Value = DeviceId> {
    (arb_kind(), arb_bytes::<15>()).prop_map(|(kind, rest)| DeviceId::with_kind(kind, rest))
}

pub fn arb_book() -> impl Strategy<Value = BookId> {
    prop::sample::select(BookId::ALL.to_vec())
}

pub fn arb_chapter() -> impl Strategy<Value = ChapterNo> {
    prop_oneof![1u16..200, Just(u16::MAX)].prop_map(chapter)
}

pub fn arb_verse() -> impl Strategy<Value = VerseNo> {
    prop_oneof![1u16..200, Just(u16::MAX)].prop_map(verse_no)
}

/// Any text, with every character class the escaper must handle.
pub fn arb_text() -> impl Strategy<Value = String> {
    prop_oneof![
        3 => any::<String>(),
        1 => prop::collection::vec(
            prop::sample::select(vec![
                "\"", "\\", "\u{0}", "\u{8}", "\u{c}", "\n", "\r", "\t", "\u{1f}", "\u{7f}",
                "é", "\u{2028}", "/", "𝔄", "אֱלֹהִים", "a", " ",
            ]),
            0..12,
        )
        .prop_map(|parts| parts.concat()),
    ]
}

/// Text in NFC without the given characters.
pub fn arb_nfc(forbidden: &'static [char]) -> impl Strategy<Value = String> {
    "[a-zA-Z0-9 _.éñ-]{1,12}"
        .prop_map(move |s: String| {
            s.chars()
                .filter(|c| !forbidden.contains(c))
                .collect::<String>()
        })
        .prop_filter("non-empty", |s| !s.is_empty())
}

pub fn arb_json_leaf() -> impl Strategy<Value = Json> {
    prop_oneof![
        Just(Json::Null),
        any::<bool>().prop_map(Json::Bool),
        any::<i64>().prop_map(Json::from),
        any::<u64>().prop_map(Json::from),
        Just(Json::Int(JsonInt::new(JsonInt::MAX).unwrap())),
        arb_text().prop_map(Json::Str),
    ]
}

pub fn arb_json() -> impl Strategy<Value = Json> {
    arb_json_leaf().prop_recursive(4, 24, 4, |inner| {
        prop_oneof![
            prop::collection::vec(inner.clone(), 0..4).prop_map(Json::Arr),
            prop::collection::btree_map(arb_text(), inner, 0..4)
                .prop_map(|map| Json::Obj(map.into_iter().collect())),
        ]
    })
}

pub fn arb_map() -> impl Strategy<Value = JsonMap> {
    prop::collection::btree_map(arb_text(), arb_json(), 0..3)
        .prop_map(|map| map.into_iter().collect())
}

const WORD_PIECES: &[&str] = &[
    "Ellos",
    "deben",
    "enseñar",
    "\\w a|x-occurrence=\"1\"\\w*",
    "\\f + \\ft nota\\f*",
    " ",
    "\n\\q2 ",
    "\r\n",
    "\\s1 Heading",
    "\\vp 3\\vp*",
    "\t",
    "___",
    "\u{a0}",
];
const LAYOUT_LINES: &[&str] = &["\\p", "\\q1", "", "\\b", "\\ts\\*", "  \\m"];

pub fn arb_words() -> impl Strategy<Value = String> {
    prop_oneof![
        prop::collection::vec(prop::sample::select(WORD_PIECES), 1..6)
            .prop_map(|p| p
                .concat()
                .trim_end_matches(['\n', '\r', ' ', '\t'])
                .to_owned())
            .prop_filter("words end with text", |w| !w.is_empty()
                && !w.ends_with("\\q2")),
        arb_text().prop_map(|s| s.replace('\\', "/")),
        Just(String::new()),
    ]
}

pub fn arb_layout() -> impl Strategy<Value = Vec<&'static str>> {
    prop::collection::vec(prop::sample::select(LAYOUT_LINES), 0..3)
}

pub fn arb_marker() -> impl Strategy<Value = MarkerWs> {
    (
        prop::sample::select(vec![" ", "\t", "  ", " \t"]),
        prop::sample::select(vec![
            MarkerTerm::Space,
            MarkerTerm::Tab,
            MarkerTerm::Lf,
            MarkerTerm::Cr,
        ]),
    )
        .prop_map(|(lead, term)| MarkerWs::new(lead, term).unwrap())
}

pub fn arb_chapter_verse() -> impl Strategy<Value = ChapterVerse> {
    (arb_chapter(), arb_verse()).prop_map(|(chapter, verse)| ChapterVerse { chapter, verse })
}

/// A verse value that passes V3 for verse `verse`.
pub fn arb_verse_value(verse: VerseNo) -> impl Strategy<Value = VerseValue> {
    let v = verse.get();
    let text = (
        arb_marker(),
        arb_words(),
        arb_layout(),
        prop::sample::select(vec!["\n", "\r\n"]),
        v..=v.saturating_add(3),
        prop::option::of(arb_chapter_verse()),
    )
        .prop_map(move |(marker, words, lines, line_break, last, from)| {
            let mut layout = String::new();
            for line in lines {
                layout.push_str(line_break);
                layout.push_str(line);
            }
            VerseValue::Text {
                marker,
                words,
                layout,
                last: verse_no(last),
                from,
            }
        });
    let absent =
        prop::option::of(arb_chapter_verse()).prop_map(|moved_to| VerseValue::Absent { moved_to });
    if v >= 2 {
        let covered = (1..v).prop_map(|by| VerseValue::Covered { by: verse_no(by) });
        prop_oneof![3 => text, 1 => covered, 1 => absent].boxed()
    } else {
        prop_oneof![3 => text, 1 => absent].boxed()
    }
}

pub fn arb_story() -> impl Strategy<Value = StoryNo> {
    (1u8..=50).prop_map(|n| StoryNo::new(n).unwrap())
}

pub fn arb_ref_part() -> impl Strategy<Value = RefPart> {
    prop_oneof![
        "[1-9][0-9]{0,2}",
        "[1-9][0-9]?-[1-9][0-9]?",
        Just("front".to_owned()),
    ]
    .prop_map(|s| RefPart::parse(&s).unwrap())
}

pub fn arb_check_key() -> impl Strategy<Value = CheckKey> {
    let target = prop_oneof![
        (arb_book(), arb_ref_part(), arb_ref_part()).prop_map(|(book, chapter, verse)| {
            CheckTarget::Verse {
                book,
                chapter,
                verse,
            }
        }),
        (arb_story(), any::<u16>()).prop_map(|(story, frame)| CheckTarget::Frame {
            story,
            frame: FrameNo::new(frame),
        }),
    ];
    (
        prop::sample::select(vec![
            ToolId::TranslationWords,
            ToolId::TranslationNotes,
            ToolId::TranslationQuestions,
        ]),
        arb_nfc(&['|', ':']),
        target,
        any::<u32>(),
    )
        .prop_map(|(tool, id, target, occurrence)| CheckKey {
            tool,
            check_id: CheckId::parse(&id).unwrap(),
            target,
            occurrence,
        })
}

pub fn arb_resource() -> impl Strategy<Value = ResourceRef> {
    ("[a-z0-9./_-]{1,30}", arb_oid()).prop_map(|(repo_path, sha)| ResourceRef {
        repo_path: repo_path.into(),
        sha,
    })
}

pub fn arb_target_word() -> impl Strategy<Value = TargetWord> {
    (arb_text(), any::<u32>(), any::<u32>(), arb_map()).prop_map(
        |(word, occurrence, occurrences, extra)| TargetWord {
            word,
            occurrence,
            occurrences,
            extra,
        },
    )
}

pub fn arb_alignment() -> impl Strategy<Value = AlignmentValue> {
    let source = (
        arb_text(),
        prop::option::of(arb_text()),
        prop::option::of(arb_text()),
        prop::option::of(arb_text()),
        any::<u32>(),
        any::<u32>(),
        arb_map(),
    )
        .prop_map(
            |(word, strong, lemma, morph, occurrence, occurrences, extra)| SourceWord {
                word,
                strong,
                lemma,
                morph,
                occurrence,
                occurrences,
                extra,
            },
        );
    let group = (
        prop::collection::vec(source, 0..3),
        prop::collection::vec(arb_target_word(), 0..3),
        arb_map(),
    )
        .prop_map(|(top_words, bottom_words, extra)| AlignmentGroup {
            top_words,
            bottom_words,
            extra,
        });
    (
        arb_verse(),
        prop::collection::vec(group, 0..3),
        prop::collection::vec(arb_target_word(), 0..3),
        arb_md5(),
        arb_text(),
        prop::option::of(arb_resource()),
        any::<(bool, bool, bool)>(),
        arb_map(),
    )
        .prop_map(
            |(span_end, alignments, word_bank, md5, version, source, flags, extra)| {
                AlignmentValue {
                    span_end,
                    alignments,
                    word_bank,
                    target_verse_md5: md5,
                    source_version: version.into(),
                    source,
                    done: flags.0,
                    marked_invalid: flags.1,
                    dismissed: flags.2,
                    extra,
                }
            },
        )
}

pub fn arb_decision() -> impl Strategy<Value = DecisionRecord> {
    let selection = (arb_text(), any::<u32>(), any::<u32>(), arb_map()).prop_map(
        |(text, occurrence, occurrences, extra)| Selection {
            text,
            occurrence,
            occurrences,
            extra,
        },
    );
    let judgement = (
        prop::option::of(prop::collection::vec(selection, 0..3)),
        any::<bool>(),
        prop::option::of(prop::sample::select(vec![
            UserStatus::Valid,
            UserStatus::Invalid,
            UserStatus::Todo,
        ])),
        arb_md5(),
    )
        .prop_map(
            |(selections, nothing_to_select, status, verse_text_md5)| Judgement {
                selections,
                nothing_to_select,
                status,
                verse_text_md5,
            },
        );
    (
        arb_map(),
        prop::option::of(arb_text()),
        judgement,
        prop::option::of(arb_text()),
        any::<(bool, bool, bool)>(),
        arb_resource(),
        arb_map(),
    )
        .prop_map(
            |(context, category, judgement, comment, flags, resource, extra)| DecisionRecord {
                context,
                category: category.map(Into::into),
                judgement,
                comment,
                bookmarked: flags.0,
                resource,
                imported_invalid: flags.1,
                dismissed: flags.2,
                extra,
            },
        )
}

pub fn arb_slot() -> impl Strategy<Value = SlotName> {
    "[a-zA-Z][a-zA-Z0-9_]{0,12}"
        .prop_filter("not a prototype name", |s| {
            s != "constructor" && s != "prototype"
        })
        .prop_map(|s| SlotName::parse(&s).unwrap())
}

pub fn arb_meta_field() -> impl Strategy<Value = MetaField> {
    prop_oneof![
        Just(MetaField::Name),
        Just(MetaField::Abbreviation),
        Just(MetaField::Description),
        Just(MetaField::Languages),
        Just(MetaField::Copyright),
        "[a-zA-Z]{1,8}(\\.[a-zA-Z]{1,8}){0,2}"
            .prop_filter_map("a free path", |s| MetaPath::parse(&s).ok())
            .prop_map(MetaField::Extra),
    ]
}

pub fn arb_carried_path() -> impl Strategy<Value = RepoPath> {
    prop_oneof![
        "ingredients/audio/[a-z0-9_-]{1,10}\\.(wav|mp3)",
        "ingredients/[A-Z][a-z]{2,8}\\.md",
        "[A-Z]{3,8}\\.md",
    ]
    .prop_filter_map("a carried path", |s| RepoPath::parse(&s).ok())
}

pub fn arb_scope() -> impl Strategy<Value = Scope> {
    prop_oneof![
        Just(Scope::Whole),
        prop::collection::vec(
            prop::sample::select(vec!["1", "1-2", "3:1", "3:1-9", "3:1-4:2"]),
            1..4
        )
        .prop_map(|texts| Scope::Ranges(
            ScopeRanges::new(
                texts
                    .iter()
                    .map(|t| t.parse::<ScopeRange>().unwrap())
                    .collect()
            )
            .unwrap()
        )),
    ]
}

pub fn arb_sorted<T: std::fmt::Debug + Ord + Clone>(
    item: impl Strategy<Value = T>,
    max: usize,
) -> impl Strategy<Value = Vec<T>> {
    prop::collection::btree_set(item, 0..max).prop_map(|set| set.into_iter().collect())
}

/// A record whose value passes V3, V6 and V8, and the parents it may have.
pub fn arb_record() -> impl Strategy<Value = Record> {
    let header_text = prop::collection::vec(
        prop::sample::select(vec![
            "\\id TIT EN\n",
            "\\h Tito\r\n",
            "\\mt1 Tito\n",
            "\\v 1 x\n",
            "texto",
        ]),
        0..4,
    )
    .prop_map(|parts| parts.concat());
    prop_oneof![
        (
            arb_book(),
            arb_scope(),
            prop::sample::select(vec![
                PresentHow::Added,
                PresentHow::RestoredText,
                PresentHow::StartedEmpty,
            ])
        )
            .prop_map(|(book, scope, how)| Record::BookPresence {
                book,
                value: BookPresence::Present { scope, how },
            }),
        (
            arb_book(),
            arb_sorted(arb_bytes::<16>().prop_map(ShortId::from_bytes), 4)
        )
            .prop_map(|(book, seen)| Record::BookPresence {
                book,
                value: BookPresence::Removed { seen },
            }),
        (arb_book(), header_text).prop_map(|(book, text)| Record::BookHeader {
            book,
            value: UsfmBytes(text),
        }),
        (
            arb_book(),
            arb_chapter(),
            prop::sample::select(vec!["\n", "\r\n", " \n\\s1 Title\n\\p\n", ""]),
            any::<bool>()
        )
            .prop_map(|(book, chapter, tail, absent)| Record::ChapterFront {
                book,
                chapter,
                value: if absent {
                    ChapterFront::Absent
                } else {
                    ChapterFront::Present(UsfmBytes(format!("\\c {chapter}{tail}")))
                },
            }),
        (arb_book(), arb_chapter(), arb_verse()).prop_flat_map(|(book, chapter, verse)| {
            arb_verse_value(verse).prop_map(move |value| Record::Verse {
                book,
                chapter,
                verse,
                value,
            })
        }),
        (
            arb_book(),
            prop::option::of(prop::collection::vec(
                prop_oneof![
                    arb_chapter().prop_map(LayoutItem::Chapter),
                    (arb_chapter(), arb_verse()).prop_map(|(c, v)| LayoutItem::Verse(c, v)),
                ],
                0..4
            ))
        )
            .prop_map(|(book, value)| Record::BookLayout { book, value }),
        (arb_book(), arb_chapter(), arb_verse(), arb_alignment()).prop_map(
            |(book, chapter, verse, value)| Record::Alignment {
                book,
                chapter,
                verse,
                value,
            }
        ),
        (arb_check_key(), arb_decision()).prop_map(|(key, decision)| Record::Check {
            key,
            value: CheckValue::Decision(decision),
        }),
        (arb_check_key(), arb_check_key()).prop_map(|(key, to)| Record::Check {
            key,
            value: CheckValue::MovedTo { to },
        }),
        (
            arb_bytes::<16>(),
            prop_oneof![
                (arb_book(), arb_chapter(), arb_verse()).prop_map(|(book, chapter, verse)| {
                    CommentTarget::Verse {
                        book,
                        chapter,
                        verse,
                    }
                }),
                arb_check_key().prop_map(CommentTarget::Check),
                (arb_story(), any::<u16>()).prop_map(|(story, frame)| CommentTarget::Frame {
                    story,
                    frame: FrameNo::new(frame)
                }),
            ],
            arb_text()
        )
            .prop_map(|(id, target, text)| Record::Comment {
                id: CommentId::from_bytes(id),
                value: CommentValue { target, text },
            }),
        (
            prop::sample::select(vec![
                PinGroup::Primary,
                PinGroup::Fallback,
                PinGroup::OriginalLanguage,
                PinGroup::Lexicon,
                PinGroup::ExtraScripture,
            ]),
            prop::collection::btree_map(arb_slot(), arb_json(), 0..3)
        )
            .prop_map(|(group, slots)| Record::Pins {
                group,
                value: PinGroupValue(
                    slots
                        .into_iter()
                        .map(|(k, v)| (k, SlotValue(v)))
                        .collect::<BTreeMap<_, _>>()
                ),
            }),
        (
            arb_nfc(&[]).prop_filter_map("a setting key", |s| SettingKey::parse(&s).ok()),
            prop::option::of(arb_json())
        )
            .prop_map(|(key, value)| Record::Setting { key, value }),
        (
            prop::sample::select(vec![
                ResourcesLevel::Top,
                ResourcesLevel::LanguageSets,
                ResourcesLevel::Resources,
            ]),
            arb_nfc(&[]),
            prop::option::of(arb_json())
        )
            .prop_filter_map("a resources.json key", |(level, name, value)| {
                Some(Record::ResourcesExtra {
                    key: ResourcesKey::new(level, &name).ok()?,
                    value,
                })
            }),
        (arb_meta_field(), prop::option::of(arb_json()))
            .prop_map(|(field, value)| Record::ProjectMeta { field, value }),
        (prop::option::of(arb_text()), arb_text()).prop_map(|(name, bytes)| {
            Record::Versification {
                value: VersificationValue {
                    name: name.map(Into::into),
                    bytes,
                },
            }
        }),
        (arb_story(), arb_text()).prop_map(|(story, value)| Record::StoryBase { story, value }),
        (arb_story(), any::<u16>(), arb_text()).prop_map(|(story, frame, value)| {
            Record::Frame {
                story,
                frame: FrameNo::new(frame),
                value,
            }
        }),
        (arb_story(), prop::option::of(arb_text()))
            .prop_map(|(story, value)| Record::StoryRef { story, value }),
        (
            arb_carried_path(),
            prop_oneof![
                (arb_oid(), any::<u64>(), arb_md5())
                    .prop_map(|(oid, size, md5)| CarriedValue::Present { oid, size, md5 }),
                Just(CarriedValue::Removed),
            ]
        )
            .prop_map(|(path, value)| Record::CarriedFile { path, value }),
        (arb_bytes::<16>(), arb_sorted(arb_change_id(), 4)).prop_map(|(batch, changes)| {
            Record::BatchRelease {
                batch: BatchId::from_bytes(batch),
                value: BatchReleaseValue { changes },
            }
        }),
    ]
}

pub fn arb_cause() -> impl Strategy<Value = Cause> {
    let source = prop_oneof![
        (arb_carried_path(), arb_md5())
            .prop_map(|(path, seen_md5)| ExternalSource::LocalFile { path, seen_md5 }),
        (arb_oid(), arb_carried_path(), arb_text()).prop_map(|(commit, path, author)| {
            ExternalSource::Door43 {
                commit,
                path,
                author: author.into(),
            }
        }),
        (arb_carried_path(), prop::option::of(arb_device()))
            .prop_map(|(path, exporter)| ExternalSource::ExchangeFile { path, exporter }),
    ];
    let import = prop_oneof![
        (arb_text(), arb_text(), arb_text()).prop_map(|(a, t, o)| ImportSource::Tc4 {
            actor: a.into(),
            ts: t.into(),
            op: o.into(),
        }),
        Just(ImportSource::Tc4Reconcile),
        prop::option::of(arb_text()).prop_map(|user| ImportSource::Tc3 {
            user: user.map(Into::into)
        }),
        arb_text().prop_map(|f| ImportSource::Usfm {
            file_name: f.into()
        }),
        arb_text().prop_map(|f| ImportSource::Burrito {
            file_name: f.into()
        }),
    ];
    prop_oneof![
        Just(Cause::Edit),
        Just(Cause::Resolve),
        arb_change_id().prop_map(|from| Cause::Restore { from }),
        prop::sample::select(vec![PinGroup::Primary, PinGroup::ExtraScripture])
            .prop_map(|group| Cause::CarryOver { group }),
        (source, any::<bool>()).prop_map(|(source, unknown_base)| Cause::External {
            source,
            unknown_base,
        }),
        import.prop_map(Cause::Import),
        Just(Cause::Seed),
        Just(Cause::StartOver),
        prop::sample::select(vec![
            SystemReason::PinCoverageBackfill,
            SystemReason::CreationPins
        ])
        .prop_map(Cause::System),
    ]
}

/// A change body every reader accepts: rules V1 to V8 hold.
pub fn arb_body() -> impl Strategy<Value = ChangeBody> {
    let profile = (
        prop::option::of(arb_text()),
        prop::option::of("[a-z0-9]{1,4}([-._][a-z0-9]{1,4}){0,2}"),
        prop::option::of(prop_oneof![
            arb_text().prop_map(|actor| LegacyIdentity::Tc4Actor {
                actor: actor.into()
            }),
            arb_text().prop_map(|user| LegacyIdentity::Tc3User { user: user.into() }),
        ]),
    )
        .prop_map(|(display_name, username, legacy)| DeviceProfileValue {
            display_name: display_name.map(Into::into),
            door43_username: username.map(|u| Door43Username::parse(&u).unwrap()),
            legacy,
        });
    let record_and_device = prop_oneof![
        8 => (arb_record(), arb_device()),
        1 => (arb_device(), profile).prop_map(|(device, value)| {
            (Record::DeviceProfile { device, value }, device)
        }),
    ];
    (
        record_and_device,
        arb_sorted(arb_change_id(), 5),
        any::<u64>(),
        any::<i64>(),
        arb_bytes::<16>(),
        1u32..5,
        arb_cause(),
    )
        .prop_map(|((record, device), parents, seq, at, batch_id, n, cause)| {
            let grow_only = matches!(record, Record::Comment { .. } | Record::BatchRelease { .. });
            ChangeBody {
                v: 0,
                record,
                parents: if grow_only { vec![] } else { parents },
                device,
                seq: DeviceSeq(seq),
                at: WallTime(at),
                batch: BatchRef::new(BatchId::from_bytes(batch_id), n).unwrap(),
                cause,
            }
        })
}

/// A JSON object member list, for building change JSON by hand in canonical order.
pub fn obj(fields: &[(&str, Json)]) -> Json {
    Json::Obj(
        fields
            .iter()
            .map(|(k, v)| ((*k).to_owned(), v.clone()))
            .collect(),
    )
}

// ---------------------------------------------------------------------------------------
// Census
// ---------------------------------------------------------------------------------------

/// The states a property test claims to reach. Each run counts them and fails when one was
/// never reached (§16.1): a generator that never builds a state tests nothing about it.
pub struct Census {
    expected: &'static [&'static str],
    counts: std::cell::RefCell<BTreeMap<&'static str, u64>>,
}

impl Census {
    pub fn new(expected: &'static [&'static str]) -> Self {
        Self {
            expected,
            counts: Default::default(),
        }
    }

    /// Counts one case that reached `state`, which must be one of the expected states.
    pub fn saw(&self, state: &'static str) {
        assert!(
            self.expected.contains(&state),
            "{state:?} is not a state of this census"
        );
        *self.counts.borrow_mut().entry(state).or_default() += 1;
    }

    /// Fails when an expected state was never reached.
    pub fn check(&self) {
        let counts = self.counts.borrow();
        // Seen with `--nocapture`: how often each state was reached, for tuning.
        eprintln!("census {counts:?}");
        let missing: Vec<&str> = self
            .expected
            .iter()
            .copied()
            .filter(|state| !counts.contains_key(state))
            .collect();
        assert!(
            missing.is_empty(),
            "the generator never reached {missing:?}; it reached {counts:?}"
        );
    }
}

/// Runs a property over `cases` generated values, then checks the census of the states the
/// values reached.
pub fn run_with_census<S: Strategy>(
    cases: u32,
    strategy: S,
    expected: &'static [&'static str],
    test: impl Fn(S::Value, &Census) -> Result<(), TestCaseError>,
) {
    let census = Census::new(expected);
    let config = ProptestConfig {
        cases,
        failure_persistence: None,
        ..ProptestConfig::default()
    };
    let mut runner = proptest::test_runner::TestRunner::new(config);
    if let Err(error) = runner.run(&strategy, |value| test(value, &census)) {
        panic!("{error}");
    }
    census.check();
}

/// The id `read_pack` gives `bytes`: when they frame a pack (a header line with its line feed,
/// then lines that each verify and end with a line feed), its id (§4.3), computed here from
/// the hash fields of the lines; otherwise the SHA-256 of all of them. A pack named by its
/// first 32 hex digits passes P2.
pub fn pack_id_of(bytes: &[u8]) -> tc_journal::PackId {
    let framed = || {
        let header_end = bytes.iter().position(|b| *b == b'\n')? + 1;
        let body = bytes.get(header_end..)?;
        let ids = match body.strip_suffix(b"\n") {
            None if body.is_empty() => Vec::new(),
            None => return None,
            Some(lines) => lines
                .split(|b| *b == b'\n')
                .map(|line| tc_journal::line::verify_line(line).map(|(id, _)| id))
                .collect::<Option<Vec<ChangeId>>>()?,
        };
        tc_journal::line::verify_line(&bytes[..header_end - 1])?;
        Some(tc_journal::PackId::of_pack(&bytes[..header_end], ids))
    };
    framed().unwrap_or_else(|| tc_journal::PackId::of(bytes))
}
