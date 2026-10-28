//! Properties of the print model over generated books and story sets: every verse and frame
//! with text prints exactly once, in order; every other verse of a printed chapter prints
//! once as not yet drafted; the gap lines stand for exactly the parts left between.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    reason = "test code: a failed unwrap is a failed test"
)]

use proptest::prelude::*;
use tc_api::export::PrintVerse;
use tc_api::ids::RefPart;
use tc_api::model::print::{
    ChapterOpening, ChapterPart, FlowBlock, ObsLayout, Opening, PageSetup, Pictures, Piece,
    PrintPart, VerseLine, VerseNumbers, print_parts,
};
use tc_export::book::{BookChapter, BookVerse, print_book, print_items, printed_items};
use tc_export::stories::{FrameBlock, Side, StoryBlock, StoryPictures, StoryTitle, print_stories};
use tc_export::{Direction, PrintText, Run};
use tc_helps::obs::{ObsFrame, ObsImage, ObsStory};
use tc_obs::print::printed;

fn setups() -> impl Strategy<Value = PageSetup> {
    (any::<bool>(), any::<bool>(), any::<bool>(), any::<bool>()).prop_map(
        |(drop_cap, numbers, pictures, wrapped)| PageSetup {
            chapter_opening: if drop_cap {
                ChapterOpening::DropCap
            } else {
                ChapterOpening::Heading
            },
            verse_numbers: if numbers {
                VerseNumbers::Shown
            } else {
                VerseNumbers::Hidden
            },
            pictures: if pictures {
                Pictures::Shown
            } else {
                Pictures::Hidden
            },
            obs_layout: if wrapped {
                ObsLayout::Wrapped
            } else {
                ObsLayout::Above
            },
            ..PageSetup::default()
        },
    )
}

fn directions() -> impl Strategy<Value = Direction> {
    prop_oneof![Just(Direction::Ltr), Just(Direction::Rtl)]
}

/// A verse: not drafted, drafted with text, or drafted with no text to show.
fn verses() -> impl Strategy<Value = (bool, String)> {
    prop_oneof![
        3 => Just((false, String::new())),
        5 => "[a-zé<&\" ]{1,12}".prop_map(|t| (true, t)),
        1 => Just((true, String::new())),
    ]
}

/// Chapters in ascending order, some numbers skipped, each with its verses.
fn books() -> impl Strategy<Value = Vec<BookChapter>> {
    prop::collection::vec((1u16..3, prop::collection::vec(verses(), 0..6)), 0..12).prop_map(
        |chapters| {
            let mut number = 0;
            chapters
                .into_iter()
                .map(|(step, verses)| {
                    number += step;
                    BookChapter {
                        number,
                        verses: verses
                            .into_iter()
                            .enumerate()
                            .map(|(i, (drafted, text))| BookVerse {
                                label: (i + 1).to_string(),
                                drafted,
                                text,
                            })
                            .collect(),
                    }
                })
                .collect()
        },
    )
}

/// The text a verse prints, stated from its fields rather than by `printed_text`, so the
/// property below checks the rule instead of repeating it: a drafted verse with no text to
/// show prints as not yet drafted.
fn shown(verse: &BookVerse) -> Option<&str> {
    (verse.drafted && !verse.text.is_empty()).then_some(verse.text.as_str())
}

fn chapter_prints(chapter: &BookChapter) -> bool {
    chapter.verses.iter().any(|verse| shown(verse).is_some())
}

proptest! {
    #[test]
    fn every_drafted_verse_prints_once(chapters in books(), setup in setups(), dir in directions()) {
        let items = print_items(&printed_items(&chapters)).unwrap();
        let view = print_book("Book", &items, &setup, dir);
        let printing: Vec<usize> = (0..chapters.len()).filter(|&i| chapter_prints(&chapters[i])).collect();
        let (Some(&first), Some(&last)) = (printing.first(), printing.last()) else {
            prop_assert!(view.flow.is_empty());
            return Ok(());
        };
        // Walk the flow alongside the chapters from the first printed one to the last.
        let mut flow = view.flow.iter();
        let mut i = first;
        while i <= last {
            let chapter = &chapters[i];
            match flow.next() {
                Some(FlowBlock::Chapter(block)) => {
                    prop_assert!(chapter_prints(chapter));
                    prop_assert_eq!(block.chapter, chapter.number);
                    prop_assert!(!block.continues);
                    let opening = match setup.chapter_opening {
                        ChapterOpening::DropCap => Opening::DropCap,
                        ChapterOpening::Heading => Opening::Heading,
                    };
                    prop_assert_eq!(block.opening, Some(opening));
                    // One line per verse, in order: every verse prints, and only once.
                    prop_assert_eq!(block.verses.len(), chapter.verses.len());
                    for (line, verse) in block.verses.iter().zip(&chapter.verses) {
                        let expected = match shown(verse) {
                            Some(text) => VerseLine::Text {
                                number: (setup.verse_numbers == VerseNumbers::Shown).then_some(verse.label.as_str()),
                                text,
                            },
                            None => VerseLine::NotDrafted { number: &verse.label },
                        };
                        prop_assert_eq!(*line, expected);
                    }
                    i += 1;
                }
                Some(FlowBlock::Gap(run)) => {
                    // A gap stands for the longest run of chapters with nothing to print.
                    let mut j = i;
                    while !chapter_prints(&chapters[j]) {
                        j += 1;
                    }
                    prop_assert!(j > i);
                    prop_assert_eq!((run.from, run.to), (chapter.number, chapters[j - 1].number));
                    i = j;
                }
                other => prop_assert!(false, "unexpected {:?}", other),
            }
        }
        prop_assert!(flow.next().is_none());
    }

    #[test]
    fn a_verse_split_by_a_page_break_prints_whole_and_numbered_once(
        text in "[a-z ]{2,30}",
        cut in any::<prop::sample::Index>(),
        setup in setups(),
    ) {
        let verse = PrintVerse { label: RefPart::parse("7").unwrap(), text: Some(text.clone()) };
        let at = 1 + cut.index(text.len() - 1);
        let (start, rest) = text.split_at(at);
        let page = |head, piece| vec![PrintPart::Chapter(ChapterPart {
            chapter: 3,
            head,
            verses: vec![(&verse, piece)],
        })];
        let first = print_parts(&page(true, Piece::Start(start)), &setup, true);
        let second = print_parts(&page(false, Piece::Continued(rest)), &setup, false);
        fn line<'a>(blocks: &[FlowBlock<'a>]) -> (bool, bool, VerseLine<'a>) {
            match &blocks[0] {
                FlowBlock::Chapter(block) => (block.opening.is_some(), block.continues, block.verses[0]),
                FlowBlock::Gap(_) => panic!("a gap"),
            }
        }
        let numbered = (setup.verse_numbers == VerseNumbers::Shown).then_some("7");
        prop_assert_eq!(line(&first), (true, true, VerseLine::Text { number: numbered, text: start }));
        prop_assert_eq!(line(&second), (false, false, VerseLine::Text { number: None, text: rest }));
    }
}

fn stories() -> impl Strategy<Value = Vec<ObsStory>> {
    let frame = prop_oneof![Just(String::new()), "[a-zé<& ]{1,10}"];
    let story = (
        any::<bool>(),
        prop::collection::vec(frame, 0..7),
        prop_oneof![
            Just(None),
            Just(Some(String::new())),
            Just(Some("R".to_owned()))
        ],
        any::<bool>(),
    );
    prop::collection::vec(story, 0..10).prop_map(|stories| {
        stories
            .into_iter()
            .enumerate()
            .filter(|(_, (.., kept))| *kept)
            .map(|(i, (titled, frames, reference, _))| ObsStory {
                number: u8::try_from(i + 1).unwrap(),
                title: if titled { "T".into() } else { String::new() },
                frames: frames
                    .into_iter()
                    .map(|text| ObsFrame {
                        image: ObsImage {
                            alt: String::new(),
                            url: String::new(),
                        },
                        text,
                    })
                    .collect(),
                reference,
            })
            .collect()
    })
}

/// The pictures a caller hands the print, built here rather than taken from
/// `tc_obs::print::printed` (which drops empty sources), so the print's own rule is what
/// the property checks: every fourth frame has no picture and the frame before it an empty
/// source, and neither prints a picture (tC4 prints one only when `src` is truthy).
fn pictures_of(stories: &[ObsStory]) -> StoryPictures {
    stories
        .iter()
        .map(|story| {
            let sources = (1..=story.frames.len())
                .filter(|n| n % 4 != 0)
                .map(|n| {
                    let src = if n % 4 == 3 {
                        String::new()
                    } else {
                        format!("{}-{n}.jpg", story.number)
                    };
                    (n, src)
                })
                .collect();
            (story.number, sources)
        })
        .collect()
}

fn has_picture(frame: usize) -> bool {
    matches!(frame % 4, 1 | 2)
}

/// Whether a story prints, stated from its fields rather than by tc-obs' `story_drafted`:
/// it has a title, a reference with text or a frame with text.
fn story_prints(story: &ObsStory) -> bool {
    !story.title.is_empty()
        || story.reference.as_deref().is_some_and(|r| !r.is_empty())
        || story.frames.iter().any(|frame| !frame.text.is_empty())
}

proptest! {
    #[test]
    fn every_drafted_frame_prints_once(stories in stories(), setup in setups(), dir in directions()) {
        let printed = printed(&stories, None);
        let pictures = pictures_of(&stories);
        let view = print_stories(&printed.items, &pictures, &setup, dir);
        let drafted: Vec<usize> = (0..stories.len()).filter(|&i| story_prints(&stories[i])).collect();
        let (Some(&first), Some(&last)) = (drafted.first(), drafted.last()) else {
            prop_assert!(view.blocks.is_empty());
            prop_assert_eq!(view.nothing_drafted(), Some(PrintText::NothingDrafted));
            return Ok(());
        };
        prop_assert_eq!(view.nothing_drafted(), None);
        let float = match (setup.obs_layout, dir) {
            (ObsLayout::Above, _) => None,
            (ObsLayout::Wrapped, Direction::Ltr) => Some(Side::Left),
            (ObsLayout::Wrapped, Direction::Rtl) => Some(Side::Right),
        };
        let mut blocks = view.blocks.iter();
        let mut i = first;
        while i <= last {
            let story = &stories[i];
            match blocks.next() {
                Some(StoryBlock::Story(page)) => {
                    prop_assert!(story_prints(story));
                    prop_assert_eq!(page.number, story.number);
                    let title = if story.title.is_empty() {
                        StoryTitle::Numbered(PrintText::StoryNumber { story: story.number })
                    } else {
                        StoryTitle::Own(&story.title)
                    };
                    prop_assert_eq!(page.title, title);
                    prop_assert_eq!(page.reference, story.reference.as_deref().filter(|r| !r.is_empty()));
                    // Each drafted frame once, in order; each run of the others one line.
                    let mut frames = page.frames.iter();
                    let mut n = 1;
                    while n <= story.frames.len() {
                        let text = &story.frames[n - 1].text;
                        match frames.next() {
                            Some(FrameBlock::Frame(frame)) => {
                                prop_assert!(!text.is_empty());
                                prop_assert_eq!(frame.frame, n);
                                prop_assert_eq!(frame.text, text.as_str());
                                let shown = setup.pictures == Pictures::Shown && has_picture(n);
                                prop_assert_eq!(frame.picture.is_some(), shown);
                                if let Some(picture) = frame.picture {
                                    prop_assert_eq!(picture.src, format!("{}-{n}.jpg", story.number));
                                    prop_assert_eq!(picture.alt, PrintText::FramePicture { frame: n });
                                    prop_assert_eq!(picture.float, float);
                                }
                                n += 1;
                            }
                            Some(FrameBlock::Gap(PrintText::FramesNotDrafted(run))) => {
                                let mut m = n;
                                while m <= story.frames.len() && story.frames[m - 1].text.is_empty() {
                                    m += 1;
                                }
                                prop_assert!(m > n);
                                prop_assert_eq!(*run, Run { from: n, to: m - 1 });
                                n = m;
                            }
                            other => prop_assert!(false, "unexpected {:?}", other),
                        }
                    }
                    prop_assert!(frames.next().is_none());
                    i += 1;
                }
                Some(StoryBlock::Gap(PrintText::StoriesNotDrafted(run))) => {
                    let mut j = i;
                    while !story_prints(&stories[j]) {
                        j += 1;
                    }
                    prop_assert!(j > i);
                    prop_assert_eq!(*run, Run { from: story.number, to: stories[j - 1].number });
                    i = j;
                }
                other => prop_assert!(false, "unexpected {:?}", other),
            }
        }
        prop_assert!(blocks.next().is_none());
    }
}

/// The drafted verses of the sample burrito's Titus, which the property aligns.
const ALIGNED_VERSES: [u16; 5] = [1, 2, 3, 4, 5];

fn titus() -> tc_usfm::codec::BookParts {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/tc4/sample-burrito/ingredients/TIT.usfm");
    tc_usfm::codec::decompose(&std::fs::read_to_string(path).unwrap()).unwrap()
}

fn content(parts: &tc_usfm::codec::BookParts, chapter: u16, verse: u16) -> String {
    match &parts.chapters[&chapter].verses[&verse] {
        tc_usfm::codec::VersePart::Text(t) => tc_usfm::codec::join_verse(&t.words, &t.layout),
        other => panic!("{chapter}:{verse} is {other:?}"),
    }
}

/// A record of `verse` that places its words in runs of `cuts`: each run of 1 to 3 words in a
/// row is one card on its own source word, or is left in the bank.
fn record_of(
    verse: &str,
    number: u16,
    cuts: &[(usize, bool)],
) -> tc_journal::record::AlignmentValue {
    use tc_journal::json::JsonMap;
    use tc_journal::record::{AlignmentGroup, AlignmentValue, SourceWord};
    let target = tc_align::TargetVerse::new(verse);
    let mut words = target.words().iter().cloned();
    let mut alignments = Vec::new();
    let mut word_bank = Vec::new();
    for (n, &(len, placed)) in cuts.iter().enumerate() {
        let run: Vec<_> = words.by_ref().take(len).collect();
        if run.is_empty() {
            break;
        }
        if placed {
            alignments.push(AlignmentGroup {
                top_words: vec![SourceWord {
                    word: format!("λόγος{n}"),
                    strong: Some(format!("G{n:05}")),
                    lemma: Some("λόγος".into()),
                    morph: None,
                    occurrence: 1,
                    occurrences: 1,
                    extra: JsonMap::new(),
                }],
                bottom_words: run,
                extra: JsonMap::new(),
            });
        } else {
            word_bank.extend(run);
        }
    }
    word_bank.extend(words);
    AlignmentValue {
        span_end: tc_journal::ids::VerseNo::new(number).unwrap(),
        alignments,
        word_bank,
        target_verse_md5: target.md5(),
        source_version: "".into(),
        source: None,
        done: false,
        marked_invalid: false,
        dismissed: false,
        extra: JsonMap::new(),
    }
}

proptest! {
    #[test]
    fn an_aligned_export_reads_back_to_its_records(
        cuts in prop::collection::vec(prop::collection::vec((1usize..4, any::<bool>()), 0..30), 5),
    ) {
        use tc_export::{BookInput, ExportInput, Produced, VerseAlignment, produce};
        use tc_journal::ids::{ChapterNo, VerseNo};
        let parts = titus();
        let records: Vec<(u16, tc_journal::record::AlignmentValue)> = ALIGNED_VERSES
            .iter()
            .zip(&cuts)
            .map(|(&v, cuts)| (v, record_of(&content(&parts, 1, v), v, cuts)))
            .collect();
        let input = BookInput {
            book: tc_core::BookId::Titus,
            parts: parts.clone(),
            alignments: records
                .iter()
                .map(|(v, value)| {
                    let v = VerseNo::new(*v).unwrap();
                    ((ChapterNo::new(1).unwrap(), v), VerseAlignment { anchor: v, value: value.clone(), pin: None })
                })
                .collect(),
            source_sha: None,
        };
        let file = |input| match produce(&input).unwrap() {
            (Produced::File(bytes), facts) => (String::from_utf8(bytes).unwrap(), facts),
            (other, _) => panic!("{other:?}"),
        };
        let (aligned, facts) = file(ExportInput::AlignedUsfm(&input));
        prop_assert_eq!(facts, tc_export::ExportFacts::default());

        let read = tc_usfm::codec::decompose(&aligned).unwrap();
        for (v, value) in &records {
            let stored = content(&parts, 1, *v);
            let exported = content(&read, 1, *v);
            if value.usfm_groups().is_empty() {
                prop_assert_eq!(&exported, &stored);
                continue;
            }
            let (plain, back) = tc_journal::alignment::split_aligned(&exported, value.span_end)
                .unwrap_or_else(|| panic!("1:{v} does not split: {exported}"));
            prop_assert_eq!(&plain, &stored);
            prop_assert_eq!(back.usfm_groups(), value.usfm_groups());
        }

        let (plain, _) = file(ExportInput::PlainUsfm(&input));
        prop_assert_eq!(&plain, &tc_usfm::strip_alignment(&aligned).unwrap());
        prop_assert_eq!(plain, tc_usfm::codec::recompose(&parts).usfm);
    }
}
