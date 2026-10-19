//! A project's verse numbering against its sources' (unit B3, tC4 #118 and #121).
//!
//! Every unfoldingWord source text and help is numbered in `eng`. A project created in
//! another scheme (New Bible offers six) numbers some verses otherwise: `rsc` Jonah 2:1 is
//! `eng` Jonah 1:17. Understand then shows one unit per project verse with the source verse
//! the project's numbering names, never the source verse of the same number (tC4's
//! `mappedSourceReferences`, `tC4:src/state.jsx:3124-3170`), and a help stands at the place
//! its reference maps to (`deriveForProject`). A project whose scheme cannot be established
//! shows no passage at all.

use tc_api::editor::{FrameRow, RowSource};
use tc_api::ids::RefPart;
use tc_core::BookId;
use tc_journal::record::VersificationValue;
use tc_versification::{self as vrs, MapOutcome, RowReference, Scheme, StandardScheme};

/// The scheme every source text and help is numbered in (tC4's `RESOURCE_FRAME`).
const SOURCES: StandardScheme = StandardScheme::Eng;

/// How a project numbers its verses.
#[derive(Debug, Clone, Copy)]
pub(crate) enum Numbering {
    /// As the sources do.
    Same,
    /// In another scheme.
    Other(&'static Scheme),
    /// Not established: nothing is shown under numbers the project may not have.
    Unknown,
}

impl Numbering {
    /// tC4's `resolveProjectFrame`: the recorded name when it is a scheme the app has, else the
    /// scheme whose document means what the project's `vrs.json` means, else unknown.
    pub(crate) fn of(register: Option<&VersificationValue>) -> Self {
        let Some(value) = register else {
            return Self::Unknown;
        };
        let known: Vec<(&str, &str)> = StandardScheme::ALL
            .iter()
            .map(|s| (s.name(), s.json()))
            .collect();
        let register = vrs::VrsRegister {
            name: value.name.as_deref().unwrap_or(vrs::UNRECORDED_SCHEME),
            json: &value.bytes,
        };
        match vrs::resolve_scheme(Some(register), &known)
            .name()
            .and_then(StandardScheme::from_name)
        {
            Some(SOURCES) => Self::Same,
            Some(scheme) => Self::Other(scheme.scheme()),
            None => Self::Unknown,
        }
    }
}

fn api_part(part: &vrs::RefPart) -> Option<RefPart> {
    RefPart::parse(&part.to_string()).ok()
}

/// Where the sources have each verse of a project chapter. `verses` are the chapter's verses
/// in order, each a verse or the first and last of a span.
pub(crate) fn rows(
    scheme: &Scheme,
    book: BookId,
    chapter: u16,
    verses: &[(u16, u16)],
) -> Vec<FrameRow> {
    verses
        .iter()
        .filter_map(|&(first, last)| {
            let verse = if last > first {
                vrs::RefPart::Span { first, last }
            } else {
                vrs::RefPart::Number(first)
            };
            let at = RowReference {
                book: book.into(),
                chapter: vrs::RefPart::Number(chapter),
                verse: verse.clone(),
            };
            let source = match vrs::map_reference(scheme, SOURCES.scheme(), &at) {
                MapOutcome::Mapped(to) if to.book == at.book => RowSource::Verse {
                    chapter: to.chapter,
                    verse: api_part(&to.verses.into())?,
                },
                MapOutcome::Mapped(to) => RowSource::OtherBook {
                    to: format!(
                        "{} {}:{}",
                        to.book,
                        to.chapter,
                        vrs::RefPart::from(to.verses)
                    ),
                },
                MapOutcome::SameFrame(_) => RowSource::Verse {
                    chapter,
                    verse: api_part(&verse)?,
                },
                MapOutcome::Unplaceable { .. } => RowSource::Unmapped,
            };
            Some(FrameRow {
                verse: api_part(&verse)?,
                source,
            })
        })
        .collect()
}

/// The chapter and verse in the project of a help at `chapter`:`verse` of the sources, when
/// it maps into this book; a help that cannot be placed is left out, as tC4 leaves it out.
/// A chapter's introduction (`1:intro`) stays at its chapter: tC4 leaves it out, so a project
/// in another numbering lost every chapter introduction, which the translator needs.
pub(crate) fn place(
    scheme: &Scheme,
    book: BookId,
    chapter: &RefPart,
    verse: &RefPart,
) -> Option<(u16, RefPart)> {
    let at = RowReference {
        book: book.into(),
        chapter: vrs::RefPart::parse(chapter.as_str()),
        verse: vrs::RefPart::parse(verse.as_str()),
    };
    if let (vrs::RefPart::Number(chapter), vrs::RefPart::Other(_)) = (&at.chapter, &at.verse) {
        return Some((*chapter, verse.clone()));
    }
    match vrs::map_reference(SOURCES.scheme(), scheme, &at) {
        MapOutcome::Mapped(to) if to.book == at.book => {
            Some((to.chapter, api_part(&to.verses.into())?))
        }
        _ => None,
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    fn part(text: &str) -> RefPart {
        RefPart::parse(text).unwrap()
    }

    fn row(verse: &str, source: RowSource) -> FrameRow {
        FrameRow {
            verse: part(verse),
            source,
        }
    }

    #[test]
    fn a_project_verse_shows_the_source_verse_its_numbering_names() {
        let rsc = StandardScheme::Rsc.scheme();
        assert_eq!(
            rows(rsc, BookId::Jonah, 2, &[(1, 1), (2, 2)]),
            vec![
                row(
                    "1",
                    RowSource::Verse {
                        chapter: 1,
                        verse: part("17")
                    }
                ),
                row(
                    "2",
                    RowSource::Verse {
                        chapter: 2,
                        verse: part("1")
                    }
                ),
            ]
        );
        // A help at `eng` 1:17 stands at the project's 2:1.
        assert_eq!(
            place(rsc, BookId::Jonah, &part("1"), &part("17")),
            Some((2, part("1")))
        );
        assert_eq!(
            place(rsc, BookId::Jonah, &part("2"), &part("intro")),
            Some((2, part("intro")))
        );
    }

    /// tC4 #121: a verse the sources have in another book is stated with that place: `vul`
    /// Daniel 13 is the sources' Greek Daniel.
    #[test]
    fn a_verse_in_another_book_names_that_book() {
        let vul = StandardScheme::Vul.scheme();
        let [only] = rows(vul, BookId::Daniel, 13, &[(1, 1)]).try_into().unwrap();
        assert!(
            matches!(&only.source, RowSource::OtherBook { to } if to == "DAG 13:1"),
            "{only:?}"
        );
    }

    #[test]
    fn the_scheme_is_the_recorded_one_or_the_one_its_document_means() {
        let value = |name: Option<&str>, scheme: StandardScheme| VersificationValue {
            name: name.map(Into::into),
            bytes: scheme.json().to_owned(),
        };
        let eng = value(None, StandardScheme::Eng);
        assert!(matches!(Numbering::of(Some(&eng)), Numbering::Same));
        let rsc = value(Some("rsc"), StandardScheme::Rsc);
        assert!(matches!(Numbering::of(Some(&rsc)), Numbering::Other(s) if s.name() == "rsc"));
        let fingerprint = value(None, StandardScheme::Rso);
        assert!(
            matches!(Numbering::of(Some(&fingerprint)), Numbering::Other(s) if s.name() == "rso")
        );
        let custom = VersificationValue {
            name: None,
            bytes: "{}".into(),
        };
        assert!(matches!(Numbering::of(Some(&custom)), Numbering::Unknown));
        assert!(matches!(Numbering::of(None), Numbering::Unknown));
    }
}
