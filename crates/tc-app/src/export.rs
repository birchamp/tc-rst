//! The `export` service's reads (unit B8): what the Community Checking preview prints of the
//! open book or the open story project, by tc-export's and tc-obs' print rules (A10). A book's
//! print reads the verse records Translate's rows are made from (`translate::texts`), never a
//! second reading of the USFM, so the preview states each verse as the editor holds it; each
//! verse's text is tc-usfm's `shown_text_v1`, the port of tC4's verse text (#782: the words
//! inside nested alignments too). Nothing here writes the project.

use tc_api::ApiError;
use tc_api::export::{
    Book, BookPrint, DraftedFrame, PrintFrame, PrintStory, PrintStoryItem, Run, Stories,
    StoriesPrint, StoryRun,
};
use tc_api::ids::StoryNo;
use tc_export::book::{BookChapter, BookVerse, has_undrafted, print_items, printed_items};
use tc_helps::obs::ObsStory;
use tc_journal::RecordKey;
use tc_obs::print::{PrintedFrame, PrintedStory};

use crate::app::App;
use crate::dispatch::Handle;
use crate::story::{journal_story, read_story, story_numbers};

/// A verse's label as the book writes it: `4`, or `4-5` for a span.
fn label(verse: u16, last: u16) -> String {
    if last > verse {
        format!("{verse}-{last}")
    } else {
        verse.to_string()
    }
}

impl Handle<Book> for App {
    async fn handle(&self, req: Book) -> Result<BookPrint, ApiError> {
        let session = self.read(&req.at)?;
        let read = session.read();
        let numbers = read.chapters_of(req.book);
        if numbers.is_empty() {
            return Err(ApiError::BadRequest {
                field: "book".to_owned(),
                reason: format!("{} is not in the project", req.book.code()),
            });
        }
        let chapters: Vec<BookChapter> = numbers
            .into_iter()
            .map(|number| BookChapter {
                number,
                verses: crate::translate::texts(&read, req.book, number)
                    .into_iter()
                    .map(|text| {
                        let drafted = crate::drafted::words_drafted(&text.words);
                        BookVerse {
                            label: label(text.verse, text.last),
                            drafted,
                            text: if drafted {
                                tc_usfm::shown_text_v1(&tc_usfm::codec::join_verse(
                                    &text.words,
                                    &text.layout,
                                ))
                            } else {
                                String::new()
                            },
                        }
                    })
                    .collect(),
            })
            .collect();
        let items = print_items(&printed_items(&chapters)).map_err(|e| {
            self.internal(
                "a verse's number could not be written",
                crate::reports::facts(
                    serde_json::json!({"verse": e.label, "reason": e.source.to_string()}),
                ),
            )
        })?;
        Ok(BookPrint {
            items,
            undrafted: has_undrafted(&chapters),
        })
    }
}

/// A story number tc-obs printed, as the wire's: every story the project holds is 1 to 50.
fn story_no(app: &App, number: u8) -> Result<StoryNo, ApiError> {
    StoryNo::new(number).map_err(|e| {
        app.internal(
            "a story's number could not be written",
            crate::reports::facts(serde_json::json!({"story": number, "reason": e.to_string()})),
        )
    })
}

/// A frame number, 1 and up, as the wire's.
fn frame_no(n: usize) -> u16 {
    u16::try_from(n).unwrap_or(u16::MAX)
}

impl Handle<Stories> for App {
    async fn handle(&self, req: Stories) -> Result<StoriesPrint, ApiError> {
        let session = self.read(&req.at)?;
        let read = session.read();
        let mut stories: Vec<ObsStory> = Vec::new();
        for number in story_numbers() {
            let Ok(story) = StoryNo::new(number) else {
                continue;
            };
            if read
                .key_state(&RecordKey::StoryBase(journal_story(story)?))
                .is_none()
            {
                continue;
            }
            stories.push(read_story(&read, story)?);
        }
        if stories.is_empty() {
            return Err(ApiError::BadRequest {
                field: "at".to_owned(),
                reason: "the project holds no stories".to_owned(),
            });
        }
        let loaded = self.loaded_obs(&session).await?;
        let mut pictures = |story: &ObsStory| {
            loaded
                .frame_pictures(story)
                .into_iter()
                .enumerate()
                .filter_map(|(at, uri)| Some((at + 1, uri?)))
                .collect()
        };
        let printed = tc_obs::print::printed(&stories, Some(&mut pictures));
        let mut items = Vec::new();
        for item in &printed.items {
            items.push(match item {
                PrintedStory::Gap(from, to) => PrintStoryItem::Gap(StoryRun {
                    from: story_no(self, *from)?,
                    to: story_no(self, *to)?,
                }),
                PrintedStory::Story { story, frames } => {
                    let sources = printed.pictures.get(&story.number);
                    PrintStoryItem::Story(PrintStory {
                        story: story_no(self, story.number)?,
                        title: story.title.clone(),
                        frames: frames
                            .iter()
                            .map(|frame| match frame {
                                PrintedFrame::Gap(from, to) => PrintFrame::Gap(Run {
                                    from: frame_no(*from),
                                    to: frame_no(*to),
                                }),
                                PrintedFrame::Drafted { n, text } => {
                                    PrintFrame::Drafted(DraftedFrame {
                                        frame: frame_no(*n),
                                        text: text.clone(),
                                        picture: sources.and_then(|s| s.get(n)).cloned(),
                                    })
                                }
                            })
                            .collect(),
                        reference: story.reference.clone().filter(|r| !r.is_empty()),
                    })
                }
            });
        }
        Ok(StoriesPrint {
            items,
            undrafted: printed.undrafted,
        })
    }
}
