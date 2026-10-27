//! Which standard schemes a project's verse counts fit (owner ruling H13).

use crate::book::BookCode;
use crate::standard::StandardScheme;

/// A book as the content numbers it: each chapter it holds, with that chapter's last verse.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BookShape {
    pub book: BookCode,
    pub chapters: Vec<(u16, u16)>,
}

/// How far `books` are from `scheme`: for every chapter the content holds, the verses between
/// its last verse and the scheme's. A chapter the content does not hold counts nothing, so a
/// book drafted in part is not held against any scheme.
pub fn distance(books: &[BookShape], scheme: StandardScheme) -> u64 {
    let versification = scheme.scheme().versification();
    books
        .iter()
        .flat_map(|shape| {
            let counts = versification.verse_counts(shape.book).unwrap_or_default();
            shape.chapters.iter().map(move |&(chapter, last)| {
                let theirs = usize::from(chapter)
                    .checked_sub(1)
                    .and_then(|i| counts.get(i))
                    .copied()
                    .unwrap_or(0);
                u64::from(last.abs_diff(theirs))
            })
        })
        .sum()
}

/// The schemes nearest to `books`, in [`StandardScheme::ALL`]'s order. More than one is a tie:
/// the content does not tell them apart.
pub fn best_fits(books: &[BookShape]) -> Vec<StandardScheme> {
    let scored: Vec<(StandardScheme, u64)> = StandardScheme::ALL
        .into_iter()
        .map(|scheme| (scheme, distance(books, scheme)))
        .collect();
    let least = scored.iter().map(|&(_, d)| d).min().unwrap_or(0);
    scored
        .into_iter()
        .filter(|&(_, d)| d == least)
        .map(|(scheme, _)| scheme)
        .collect()
}

/// The default a tC3 import offers (H13 and its addendum). The translator can change it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SchemeGuess {
    /// One scheme holds the content exactly.
    Fits(StandardScheme),
    /// `eng` holds the content exactly, and so do others: the content did not decide.
    Undecided,
    /// Several schemes hold the content exactly, `eng` not among them: `scheme` is the first of
    /// [`TIE_ORDER`] among `tied`.
    Tied {
        scheme: StandardScheme,
        tied: Vec<StandardScheme>,
    },
    /// No scheme holds the content exactly: `scheme` is the nearest.
    Nearest(StandardScheme),
}

/// The order a tie without `eng` is settled in.
pub const TIE_ORDER: [StandardScheme; 5] = [
    StandardScheme::Org,
    StandardScheme::Lxx,
    StandardScheme::Vul,
    StandardScheme::Rso,
    StandardScheme::Rsc,
];

impl SchemeGuess {
    /// The scheme the review page shows as the default.
    pub fn scheme(&self) -> StandardScheme {
        match self {
            Self::Fits(scheme) | Self::Nearest(scheme) | Self::Tied { scheme, .. } => *scheme,
            Self::Undecided => StandardScheme::Eng,
        }
    }
}

/// The default for `books`: [`best_fits`], with a tie settled as H13's addendum says.
pub fn guess(books: &[BookShape]) -> SchemeGuess {
    let best = best_fits(books);
    let settled = if best.contains(&StandardScheme::Eng) {
        StandardScheme::Eng
    } else {
        TIE_ORDER
            .into_iter()
            .find(|s| best.contains(s))
            .unwrap_or(StandardScheme::DEFAULT)
    };
    let exact = best
        .first()
        .is_some_and(|&scheme| distance(books, scheme) == 0);
    match (exact, best.len()) {
        (false, _) => SchemeGuess::Nearest(settled),
        (true, 1) => SchemeGuess::Fits(settled),
        (true, _) if settled == StandardScheme::Eng => SchemeGuess::Undecided,
        (true, _) => SchemeGuess::Tied {
            scheme: settled,
            tied: best,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use StandardScheme::*;
    use proptest::prelude::*;

    fn shape(code: &str, counts: &[u16]) -> BookShape {
        BookShape {
            book: BookCode::new(code).unwrap(),
            chapters: (1..).zip(counts.iter().copied()).collect(),
        }
    }

    #[test]
    fn malachi_in_three_chapters_fits_the_hebrew_numbering() {
        assert_eq!(best_fits(&[shape("MAL", &[14, 17, 24])]), vec![Lxx, Org]);
        assert_eq!(
            best_fits(&[shape("MAL", &[14, 17, 18, 6])]),
            vec![Eng, Rsc, Rso, Vul]
        );
    }

    #[test]
    fn joel_in_four_chapters_fits_the_hebrew_numbering() {
        assert_eq!(best_fits(&[shape("JOL", &[20, 27, 5, 21])]), vec![Lxx, Org]);
        assert_eq!(
            best_fits(&[shape("JOL", &[20, 32, 21])]),
            vec![Eng, Rsc, Rso, Vul]
        );
    }

    #[test]
    fn a_psalm_title_numbered_as_verse_one_leaves_english_out() {
        let psalm_3 = BookShape {
            book: BookCode::new("PSA").unwrap(),
            chapters: vec![(3, 9)],
        };
        let fits = best_fits(&[psalm_3]);
        assert!(fits.contains(&Org) && fits.contains(&Lxx), "{fits:?}");
        assert!(!fits.contains(&Eng), "{fits:?}");
    }

    #[test]
    fn a_short_new_testament_book_ties_every_scheme() {
        assert_eq!(
            best_fits(&[shape("3JN", &[15])]),
            StandardScheme::ALL.to_vec()
        );
        assert_eq!(best_fits(&[]), StandardScheme::ALL.to_vec());
    }

    #[test]
    fn a_book_no_scheme_holds_fits_the_nearest() {
        // Malachi in five chapters: nothing holds it, Hebrew numbering is nearest.
        let odd = shape("MAL", &[14, 17, 24, 2, 3]);
        assert!(
            StandardScheme::ALL
                .iter()
                .all(|&s| distance(std::slice::from_ref(&odd), s) > 0)
        );
        assert_eq!(distance(std::slice::from_ref(&odd), Org), 5);
        assert_eq!(best_fits(&[odd]), vec![Lxx, Org]);
    }

    #[test]
    fn every_book_held_counts_toward_the_fit() {
        let books = [shape("MAL", &[14, 17, 24]), shape("JOL", &[20, 32, 21])];
        assert_eq!(distance(&books, Org), 5 + 16);
        assert_eq!(distance(&books, Eng), 6);
        assert_eq!(best_fits(&books), vec![Eng, Rsc, Rso, Vul]);
    }

    #[test]
    fn the_default_settles_a_tie_as_the_addendum_says() {
        assert_eq!(
            guess(&[shape("MAL", &[14, 17, 24])]),
            SchemeGuess::Tied {
                scheme: Org,
                tied: vec![Lxx, Org]
            }
        );
        assert_eq!(
            guess(&[shape("MAL", &[14, 17, 18, 6])]),
            SchemeGuess::Undecided
        );
        assert_eq!(guess(&[shape("3JN", &[15])]), SchemeGuess::Undecided);
        assert_eq!(guess(&[]).scheme(), Eng);
        assert_eq!(
            guess(&[shape("MAL", &[14, 17, 24, 2, 3])]),
            SchemeGuess::Nearest(Org)
        );
        assert_eq!(
            guess(&[shape("MAL", &[14, 17, 18, 7])]),
            SchemeGuess::Nearest(Eng)
        );
    }

    #[test]
    fn a_scheme_alone_in_its_counts_is_the_default() {
        for scheme in StandardScheme::ALL {
            let v = scheme.scheme().versification();
            let only = v.books().find_map(|book| {
                let counts = v.verse_counts(book)?;
                let one = shape(book.as_str(), counts);
                (best_fits(std::slice::from_ref(&one)) == vec![scheme]).then_some(one)
            });
            let one = only.unwrap_or_else(|| panic!("no book fits {scheme:?} alone"));
            assert_eq!(guess(&[one]), SchemeGuess::Fits(scheme));
        }
    }

    proptest! {
        #[test]
        fn a_book_in_a_schemes_counts_fits_that_scheme(
            scheme in proptest::sample::select(StandardScheme::ALL.to_vec()),
            pick in any::<proptest::sample::Index>(),
            keep in proptest::collection::vec(any::<bool>(), 0..160),
        ) {
            let v = scheme.scheme().versification();
            let books: Vec<BookCode> = v.books().collect();
            let book = books[pick.index(books.len())];
            let counts = v.verse_counts(book).unwrap();
            // Any chapters of it, as a book drafted in part holds.
            let chapters = (1..)
                .zip(counts.iter().copied())
                .enumerate()
                .filter(|(i, _)| keep.get(*i).copied().unwrap_or(true))
                .map(|(_, c)| c)
                .collect();
            let fits = best_fits(&[BookShape { book, chapters }]);
            prop_assert!(fits.contains(&scheme), "{book} in {scheme:?} fits {fits:?}");
        }
    }
}
