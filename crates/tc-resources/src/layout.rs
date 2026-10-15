//! Where a resource keeps its parts. A Door43 commit comes as the DCS burrito export
//! (`metadata.json`, `ingredients/TIT.tsv`, `ingredients/payload/kt/god.md`) or as the
//! repository itself, which is either a burrito (the lexicons) or a Resource Container
//! (`manifest.yaml`, `tn_TIT.tsv`, `57-TIT.usfm`, `bible/kt/god.md`). The TSV and USFM
//! bytes are the same in both; only the paths differ.

use tc_core::BookId;
use tc_door43::RepoRef;
use tc_helps::{Manual, Slug};

use crate::pin::BookCoverage;
use crate::sets::{is_tw_repo, is_twl_repo};

/// The folder a burrito keeps its content in.
const INGREDIENTS: &str = "ingredients/";

/// The flavors whose resources are divided into Bible books.
const BOOK_FLAVORS: [&str; 3] = [
    "scripture/textTranslation",
    "parascriptural/x-bcvnotes",
    "parascriptural/x-bcvquestions",
];

/// The top-level files a book-divided resource keeps its books in: `TIT.tsv` and
/// `TIT.usfm` (export), `57-TIT.usfm`, `tn_TIT.tsv`, `twl_TIT.tsv`, `tq_TIT.tsv` (RC).
fn book_of(path: &str) -> Option<BookId> {
    let name = path.strip_prefix(INGREDIENTS).unwrap_or(path);
    if name.contains('/') {
        return None;
    }
    let stem = name
        .strip_suffix(".tsv")
        .or_else(|| name.strip_suffix(".usfm"))?;
    let code = match stem.split_once(['_', '-']) {
        Some((prefix, code))
            if matches!(prefix, "tn" | "twl" | "tq")
                || (!prefix.is_empty() && prefix.bytes().all(|b| b.is_ascii_digit())) =>
        {
            code
        }
        Some(_) => return None,
        None => stem,
    };
    // Book codes are upper case in every Door43 file name; `obs` and `frt` are not books.
    if code.bytes().any(|b| b.is_ascii_lowercase()) {
        return None;
    }
    code.parse().ok()
}

/// Whether `path` is a translation words article: `payload/<category>/<slug>.md` (export)
/// or `bible/<category>/<slug>.md` (RC).
pub(crate) fn is_word_article(path: &str) -> bool {
    let rest = path.strip_prefix(INGREDIENTS).unwrap_or(path);
    let rest = rest
        .strip_prefix("payload/")
        .or_else(|| rest.strip_prefix("bible/"));
    rest.is_some_and(|r| r.matches('/').count() == 1 && r.ends_with(".md"))
}

/// The books the files hold, as a pin records them; `["BIBLE"]` for a resource that is
/// not divided into books (articles, lexicons, Open Bible Stories). `None` for a
/// book-divided resource that holds no book.
///
/// Book files decide before articles do: a `_twl` export also carries the tW articles,
/// and its coverage is still its book TSVs (tC4 `twlPin.test.ts`, D96). The one
/// exception is a `_tw` repository: its export carries book TSVs built from `en_twl`
/// master, which are never read (D96 ruling 5), and its articles serve every book.
pub(crate) fn coverage_of<'a>(
    paths: impl Iterator<Item = &'a str>,
    flavor: Option<&str>,
    repo: &RepoRef,
) -> Option<BookCoverage> {
    if is_tw_repo(repo) {
        return Some(BookCoverage::WholeCollection);
    }
    let mut books: Vec<BookId> = paths.filter_map(book_of).collect();
    books.sort();
    if let Some(coverage) = BookCoverage::books(books) {
        return Some(coverage);
    }
    // Words links are divided into books, though they share the articles' flavor.
    let divided = is_twl_repo(repo) || flavor.is_some_and(|f| BOOK_FLAVORS.contains(&f));
    (!divided).then_some(BookCoverage::WholeCollection)
}

/// The file of `book` among `paths`, with extension `ext` (`tsv` or `usfm`).
pub(crate) fn book_file<'a>(
    mut paths: impl Iterator<Item = &'a str>,
    book: BookId,
    ext: &str,
) -> Option<&'a str> {
    paths.find(|p| p.ends_with(&format!(".{ext}")) && book_of(p) == Some(book))
}

/// The Open Bible Stories table: `OBS.tsv` (export) or `<prefix>_OBS.tsv` (RC).
pub(crate) fn obs_table<'a>(mut paths: impl Iterator<Item = &'a str>) -> Option<&'a str> {
    paths.find(|p| {
        let name = p.strip_prefix(INGREDIENTS).unwrap_or(p);
        !name.contains('/') && (name == "OBS.tsv" || name.ends_with("_OBS.tsv"))
    })
}

/// A Translation Academy article's folder and its id parts.
pub(crate) struct AcademyFolder {
    pub manual: Manual,
    pub slug: Slug,
    /// The folder, with a trailing `/`.
    pub folder: String,
}

/// Every Translation Academy article: a `<manual>/<slug>/01.md`, under `ingredients/` in
/// the export.
pub(crate) fn academy_folders<'a>(paths: impl Iterator<Item = &'a str>) -> Vec<AcademyFolder> {
    paths
        .filter_map(|p| {
            let rel = p.strip_prefix(INGREDIENTS).unwrap_or(p);
            let mut parts = rel.split('/');
            let (Some(manual), Some(slug), Some("01.md"), None) =
                (parts.next(), parts.next(), parts.next(), parts.next())
            else {
                return None;
            };
            Some(AcademyFolder {
                manual: Manual::parse(manual)?,
                slug: Slug::parse(slug).ok()?,
                folder: p[..p.len() - "01.md".len()].to_owned(),
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn books_are_found_in_both_layouts() {
        for path in [
            "ingredients/TIT.tsv",
            "ingredients/TIT.usfm",
            "57-TIT.usfm",
            "tn_TIT.tsv",
            "twl_TIT.tsv",
            "tq_TIT.tsv",
        ] {
            assert_eq!(book_of(path), Some(BookId::Titus), "{path}");
        }
        for path in [
            "ingredients/FRT.usfm",
            "A0-FRT.usfm",
            "ingredients/OBS.tsv",
            "ingredients/payload/kt/TIT.tsv",
            "LICENSE.md",
            "x_TIT.tsv",
            "ingredients/tit.tsv",
        ] {
            assert_eq!(book_of(path), None, "{path}");
        }
    }

    #[test]
    fn coverage_follows_what_the_files_hold() {
        let repo = |name: &str| format!("unfoldingWord/{name}").parse::<RepoRef>().unwrap();
        let notes = [
            "metadata.json",
            "ingredients/TIT.tsv",
            "ingredients/JON.tsv",
        ];
        assert_eq!(
            coverage_of(
                notes.iter().copied(),
                Some("parascriptural/x-bcvnotes"),
                &repo("en_tn")
            ),
            BookCoverage::books([BookId::Jonah, BookId::Titus])
        );
        let empty = ["metadata.json"];
        assert_eq!(
            coverage_of(
                empty.iter().copied(),
                Some("parascriptural/x-bcvnotes"),
                &repo("en_tn")
            ),
            None
        );
        assert_eq!(
            coverage_of(
                empty.iter().copied(),
                Some("peripheral/x-lexicon"),
                &repo("en_ugl")
            ),
            Some(BookCoverage::WholeCollection)
        );
    }

    #[test]
    fn words_links_cover_their_books_and_the_articles_cover_every_book() {
        let repo = |name: &str| format!("unfoldingWord/{name}").parse::<RepoRef>().unwrap();
        let articles = Some("parascriptural/x-bcvarticles");
        // Both exports carry book TSVs and the articles.
        let export = ["ingredients/TIT.tsv", "ingredients/payload/kt/god.md"];
        assert_eq!(
            coverage_of(export.iter().copied(), articles, &repo("en_twl")),
            BookCoverage::books([BookId::Titus])
        );
        assert_eq!(
            coverage_of(export.iter().copied(), articles, &repo("EN_TWL")),
            BookCoverage::books([BookId::Titus])
        );
        assert_eq!(
            coverage_of(export.iter().copied(), articles, &repo("en_tw")),
            Some(BookCoverage::WholeCollection)
        );
        // Links with no book file hold no book; articles with none hold every book.
        let only_articles = ["ingredients/payload/kt/god.md"];
        assert_eq!(
            coverage_of(only_articles.iter().copied(), articles, &repo("en_twl")),
            None
        );
        assert_eq!(
            coverage_of(only_articles.iter().copied(), articles, &repo("en_tw")),
            Some(BookCoverage::WholeCollection)
        );
        // The Resource Container layout of a links repository.
        let rc = ["manifest.yaml", "twl_TIT.tsv", "bible/kt/god.md"];
        assert_eq!(
            coverage_of(rc.iter().copied(), None, &repo("hi_twl")),
            BookCoverage::books([BookId::Titus])
        );
    }

    #[test]
    fn academy_folders_are_found_under_either_root() {
        let paths = [
            "ingredients/translate/figs-metaphor/01.md",
            "translate/figs-abstractnouns/01.md",
            "ingredients/translate/toc.yaml",
            "ingredients/unknown/x/01.md",
        ];
        let found = academy_folders(paths.iter().copied());
        assert_eq!(found.len(), 2);
        assert_eq!(found[0].folder, "ingredients/translate/figs-metaphor/");
        assert_eq!(found[1].slug.as_str(), "figs-abstractnouns");
    }
}
