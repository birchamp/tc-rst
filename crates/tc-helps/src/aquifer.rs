//! Aquifer helps: the release JSON of the BibleAquifer repositories
//! (`<lang>/json/*.content.json` and `<lang>/metadata.json`), read into the same model as
//! Door43 helps.
//!
//! Aquifer mirrors unfoldingWord's notes, words, academy and questions as HTML articles.
//! The conversion keeps all text and links (see [`crate::markdown`] for what the HTML
//! conversion loses) but the source itself lacks data a tC check needs:
//!
//! - **Notes** keep the original-language quote but not its occurrence, so every
//!   converted note has [`Occurrence::Unrecorded`] and cannot drive a check until the
//!   occurrence is recovered (for example by joining to Door43 en_tn by row id, where
//!   Aquifer kept it, see [`HelpItemId::upstream_row_id`]). The Translation Academy link
//!   survives only as a numeric Aquifer id.
//! - **Words** are articles with whole-verse passage lists. There are no original-language
//!   words and no occurrences, so the word links built from the passages are view-only:
//!   [`WordLink::check_readiness`] reports [`crate::CheckBlocker::NoOriginalWords`] and
//!   [`crate::CheckBlocker::OccurrenceUnrecorded`].
//! - **Academy** articles have no slug and no subtitle.
//! - **Questions** are reading material in Door43 too.
//!
//! The release files have errors of their own. 1,789 notes are filed under
//! `index_reference` `00000000`, most of them in the file of another book; they are placed
//! by their `associations.passage` ([`IndexKey::Unindexed`]). An article that still does
//! not parse or cannot be placed is refused alone, named in [`Loaded::refused`], so it
//! never costs the rest of its book.

use std::collections::BTreeMap;

use serde::Deserialize;
use serde_json::Value;

use crate::Error;
use crate::articles::without_bom;
use crate::error::{JsonFile, PinMismatch};
use crate::html::{self, Element, Node};
use crate::ids::{
    AcademyArticleId, AquiferContentId, HelpItemId, Occurrence, Quote, WordArticleId,
};
use crate::markdown::{html_to_markdown, nodes_to_markdown};
use crate::metadata::AquiferPin;
use crate::model::{AcademyArticle, Note, Question, WordArticle, WordLink};
use crate::refs::{Bcv, BibleLocus, BibleRef, HelpRef, aquifer_span, parse_aquifer_ref};

const MANUAL: &str = "UWTranslationManual";

#[derive(Deserialize)]
#[serde(untagged)]
enum RawId {
    Text(String),
    Number(u64),
}

impl RawId {
    fn text(&self) -> String {
        match self {
            RawId::Text(s) => s.clone(),
            RawId::Number(n) => n.to_string(),
        }
    }
}

#[derive(Deserialize)]
struct RawArticle {
    content_id: RawId,
    title: String,
    #[serde(default)]
    media_type: Option<String>,
    index_reference: String,
    language: String,
    #[serde(default)]
    review_level: Option<String>,
    content: String,
    #[serde(default)]
    associations: Option<RawAssociations>,
}

#[derive(Deserialize, Default)]
struct RawAssociations {
    #[serde(default)]
    passage: Vec<RawPassage>,
    #[serde(default)]
    resource: Vec<RawResource>,
}

#[derive(Deserialize)]
struct RawPassage {
    start_ref: String,
    end_ref: String,
}

#[derive(Deserialize)]
struct RawResource {
    content_id: RawId,
    resource_code: String,
    #[serde(default)]
    label: String,
    #[serde(default)]
    language: String,
}

/// How an article is indexed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IndexKey {
    /// Canonical resources: a passage, or a book or chapter introduction.
    Passage(BibleRef),
    /// `00000000`: filed under no passage. Real notes files hold such articles for other
    /// books (the French Genesis file has a 2 Timothy 2:16 note), so the converters place
    /// them by their first `associations.passage`.
    Unindexed,
    /// Alphabetical and monograph resources: a sort key.
    Sort(String),
}

const UNINDEXED: &str = "00000000";

/// What a reader or converter took, and what it refused. A refusal is an
/// [`Error::AquiferArticle`] naming the article and the field, so one bad article is
/// reported without costing the rest of its book.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Loaded<T> {
    pub items: Vec<T>,
    pub refused: Vec<Error>,
}

impl<T> Loaded<T> {
    fn new() -> Self {
        Self {
            items: Vec::new(),
            refused: Vec::new(),
        }
    }

    fn take(&mut self, result: Result<T, Error>) {
        match result {
            Ok(item) => self.items.push(item),
            Err(e) => self.refused.push(e),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MediaType {
    Text,
    Image,
    Video,
    Audio,
    Other(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReviewLevel {
    None,
    Community,
    Professional,
    /// The Aquifer API also reports `Ai`.
    Ai,
    Other(String),
}

/// A link an article declares in `associations.resource`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AquiferLink {
    pub resource_code: String,
    pub content_id: AquiferContentId,
    pub label: String,
    pub language: String,
}

/// One article of an Aquifer content file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AquiferArticle {
    pub content_id: AquiferContentId,
    pub title: String,
    pub index: IndexKey,
    pub language: String,
    pub media_type: MediaType,
    pub review_level: Option<ReviewLevel>,
    pub html: String,
    pub passages: Vec<BibleRef>,
    pub links: Vec<AquiferLink>,
}

impl AquiferArticle {
    /// The `(#n)` at the end of a note title: the note's place among the notes on its
    /// verse. Arabic titles write it `(n#)`.
    pub fn ordinal(&self) -> Option<u32> {
        let inner = self.title.trim_end().strip_suffix(')')?;
        let (_, n) = inner.rsplit_once('(')?;
        n.trim_matches('#').parse().ok()
    }
}

fn article_error(content_id: &str, field: &'static str, problem: impl Into<String>) -> Error {
    Error::AquiferArticle {
        content_id: content_id.to_owned(),
        field,
        problem: problem.into(),
    }
}

/// Reads an Aquifer content file (`json/NN.content.json`): a JSON array of articles. A file
/// that is not such an array is refused; an article that does not parse is refused alone.
pub fn parse_aquifer_articles(json: &str) -> Result<Loaded<AquiferArticle>, Error> {
    let values: Vec<Value> = serde_json::from_str(without_bom(json)).map_err(|e| Error::Json {
        file: JsonFile::AquiferArticles,
        problem: e.to_string(),
    })?;
    let mut loaded = Loaded::new();
    for value in values {
        let id = match value.get("content_id") {
            Some(Value::String(s)) => s.clone(),
            Some(Value::Number(n)) => n.to_string(),
            _ => String::new(),
        };
        let result = RawArticle::deserialize(value)
            .map_err(|e| article_error(&id, "object", e.to_string()))
            .and_then(article);
        loaded.take(result);
    }
    Ok(loaded)
}

fn article(raw: RawArticle) -> Result<AquiferArticle, Error> {
    let id_text = raw.content_id.text();
    let content_id = AquiferContentId::parse(&id_text)
        .map_err(|_| article_error(&id_text, "content_id", "not a content id"))?;
    let looks_like_key = raw
        .index_reference
        .bytes()
        .all(|b| b.is_ascii_digit() || b == b'-')
        && raw.index_reference.len() >= 8;
    let index = if raw.index_reference == UNINDEXED {
        IndexKey::Unindexed
    } else if looks_like_key {
        IndexKey::Passage(
            parse_aquifer_ref(&raw.index_reference)
                .map_err(|why| article_error(&id_text, "index_reference", why))?,
        )
    } else {
        IndexKey::Sort(raw.index_reference)
    };
    let assoc = raw.associations.unwrap_or_default();
    let passages = assoc
        .passage
        .iter()
        .map(|p| {
            let start = Bcv::parse(&p.start_ref);
            let end = Bcv::parse(&p.end_ref);
            let span = match (start, end) {
                (Some(s), Some(e)) if s.book == e.book => aquifer_span(s, e),
                _ => None,
            };
            span.ok_or_else(|| {
                article_error(
                    &id_text,
                    "associations.passage",
                    format!("{}-{} is not a passage in one book", p.start_ref, p.end_ref),
                )
            })
        })
        .collect::<Result<_, _>>()?;
    let links = assoc
        .resource
        .iter()
        .map(|r| {
            let id = r.content_id.text();
            Ok(AquiferLink {
                resource_code: r.resource_code.clone(),
                content_id: AquiferContentId::parse(&id).map_err(|_| {
                    article_error(
                        &id_text,
                        "associations.resource",
                        format!("{id:?} is not a content id"),
                    )
                })?,
                label: r.label.clone(),
                language: r.language.clone(),
            })
        })
        .collect::<Result<_, Error>>()?;
    let media_type = match raw.media_type.as_deref().unwrap_or("Text") {
        "Text" => MediaType::Text,
        "Image" => MediaType::Image,
        "Video" => MediaType::Video,
        "Audio" => MediaType::Audio,
        other => MediaType::Other(other.to_owned()),
    };
    let review_level = raw.review_level.map(|r| match r.as_str() {
        "None" => ReviewLevel::None,
        "Community" => ReviewLevel::Community,
        "Professional" => ReviewLevel::Professional,
        "Ai" => ReviewLevel::Ai,
        _ => ReviewLevel::Other(r),
    });
    Ok(AquiferArticle {
        content_id,
        title: raw.title,
        index,
        language: raw.language,
        media_type,
        review_level,
        html: raw.content,
        passages,
        links,
    })
}

/// How a resource's articles are ordered (`resource_metadata.order`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AquiferOrder {
    Canonical,
    Alphabetical,
    Monograph,
}

/// The four unfoldingWord helps Aquifer mirrors.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AquiferHelpsKind {
    Notes,
    Questions,
    Words,
    Academy,
}

impl AquiferHelpsKind {
    pub fn from_resource_code(code: &str) -> Option<Self> {
        match code {
            "UWTranslationNotes" => Some(Self::Notes),
            "UWTranslationQuestions" => Some(Self::Questions),
            "UWTranslationWords" => Some(Self::Words),
            MANUAL => Some(Self::Academy),
            _ => None,
        }
    }
}

/// The license block of a resource, as the About page must show it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AquiferLicense {
    pub title: String,
    /// `© 2022 unfoldingWord`.
    pub copyright: String,
    pub holder_url: Option<String>,
    /// `(name, url)` pairs, such as `("CC BY-SA 4.0 license", "https://…")`.
    pub licenses: Vec<(String, String)>,
    pub show_adaptation_notice_for_english: bool,
    pub show_adaptation_notice_for_non_english: bool,
}

/// A resource's `metadata.json`, without the per-article table except for the links
/// between languages.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AquiferMetadata {
    /// The GitHub repository name, such as `UWTranslationNotes`.
    pub resource_code: String,
    pub resource_type: String,
    pub title: String,
    /// ISO 639-3, such as `eng`.
    pub language: String,
    pub order: AquiferOrder,
    pub date_created: Option<String>,
    pub license: AquiferLicense,
    /// HTML.
    pub adaptation_notice: Option<String>,
    /// HTML one-liners from `scripture_burrito.copyright.shortStatements`.
    pub copyright_statements: Vec<String>,
    /// For each article, its id in each other language that has it.
    pub localizations: BTreeMap<AquiferContentId, BTreeMap<String, AquiferContentId>>,
}

impl AquiferMetadata {
    pub fn kind(&self) -> Option<AquiferHelpsKind> {
        AquiferHelpsKind::from_resource_code(&self.resource_code)
    }

    /// Whether the About page must show the adaptation notice for this language.
    pub fn shows_adaptation_notice(&self) -> bool {
        if self.language == "eng" {
            self.license.show_adaptation_notice_for_english
        } else {
            self.license.show_adaptation_notice_for_non_english
        }
    }

    /// The attribution CC BY-SA requires, as markdown lines: title, copyright, license,
    /// the adaptation notice when it applies, and the pinned source.
    pub fn attribution(&self, pin: Option<&AquiferPin>) -> String {
        let mut lines = vec![
            format!("**{}**", self.license.title),
            self.license.copyright.clone(),
        ];
        for (name, url) in &self.license.licenses {
            lines.push(format!("[{name}]({url})"));
        }
        if self.shows_adaptation_notice()
            && let Some(notice) = &self.adaptation_notice
        {
            lines.push(html_to_markdown(notice));
        }
        if let Some(pin) = pin {
            lines.push(format!(
                "Source: https://github.com/BibleAquifer/{} {} ({}), {}",
                pin.resource_code, pin.tag, pin.commit, pin.language
            ));
        }
        lines.join("\n\n")
    }
}

impl AquiferPin {
    /// Checks a resource's metadata against the pin: the repository and the language.
    /// The commit cannot be checked from the metadata; check it on download.
    pub fn verify(&self, metadata: &AquiferMetadata) -> Result<(), Error> {
        if metadata.resource_code != self.resource_code {
            return Err(Error::PinMismatch(PinMismatch::Repository {
                pinned: self.resource_code.clone(),
                found: metadata.resource_code.clone(),
            }));
        }
        if metadata.language != self.language {
            return Err(Error::PinMismatch(PinMismatch::Language {
                pinned: self.language.clone(),
                found: metadata.language.clone(),
            }));
        }
        Ok(())
    }
}

fn meta_error(problem: impl Into<String>) -> Error {
    Error::Json {
        file: JsonFile::AquiferMetadata,
        problem: problem.into(),
    }
}

/// Reads a resource's `metadata.json`.
pub fn parse_aquifer_metadata(json: &str) -> Result<AquiferMetadata, Error> {
    let v: Value =
        serde_json::from_str(without_bom(json)).map_err(|e| meta_error(e.to_string()))?;
    let rm = v
        .get("resource_metadata")
        .ok_or_else(|| meta_error("no resource_metadata"))?;
    let text = |obj: &Value, key: &str| obj.get(key).and_then(Value::as_str).map(str::to_owned);
    let need = |obj: &Value, key: &'static str| {
        text(obj, key).ok_or_else(|| meta_error(format!("no resource_metadata.{key}")))
    };
    let order = match need(rm, "order")?.as_str() {
        "canonical" => AquiferOrder::Canonical,
        "alphabetical" => AquiferOrder::Alphabetical,
        "monograph" => AquiferOrder::Monograph,
        other => return Err(meta_error(format!("unknown order {other:?}"))),
    };
    let li = rm.get("license_info").unwrap_or(&Value::Null);
    let copyright = li.get("copyright").unwrap_or(&Value::Null);
    let holder = copyright.get("holder").unwrap_or(&Value::Null);
    let licenses = li
        .get("licenses")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(Value::as_object)
                .flat_map(|by_lang| by_lang.values())
                .filter_map(|l| Some((text(l, "name")?, text(l, "url")?)))
                .collect()
        })
        .unwrap_or_default();
    let flag = |key: &str| li.get(key).and_then(Value::as_bool).unwrap_or(false);
    let license = AquiferLicense {
        title: text(li, "title").unwrap_or_else(|| text(rm, "title").unwrap_or_default()),
        copyright: format!(
            "© {} {}",
            text(copyright, "dates").unwrap_or_default(),
            text(holder, "name").unwrap_or_default()
        )
        .trim()
        .to_owned(),
        holder_url: text(holder, "url"),
        licenses,
        show_adaptation_notice_for_english: flag("showAdaptationNoticeForEnglish"),
        show_adaptation_notice_for_non_english: flag("showAdaptationNoticeForNonEnglish"),
    };
    let copyright_statements = v
        .pointer("/scripture_burrito/copyright/shortStatements")
        .and_then(Value::as_array)
        .map(|items| items.iter().filter_map(|s| text(s, "statement")).collect())
        .unwrap_or_default();
    let mut localizations = BTreeMap::new();
    if let Some(table) = v.get("article_metadata").and_then(Value::as_object) {
        for (key, entry) in table {
            let Ok(id) = AquiferContentId::parse(key) else {
                continue;
            };
            let mut by_lang = BTreeMap::new();
            if let Some(locs) = entry.get("localizations").and_then(Value::as_object) {
                for (lang, loc) in locs {
                    let other = match loc.get("content_id") {
                        Some(Value::String(s)) => s.clone(),
                        Some(Value::Number(n)) => n.to_string(),
                        _ => continue,
                    };
                    if let Ok(other) = AquiferContentId::parse(&other) {
                        by_lang.insert(lang.clone(), other);
                    }
                }
            }
            localizations.insert(id, by_lang);
        }
    }
    Ok(AquiferMetadata {
        resource_code: need(rm, "resource_code")?,
        resource_type: need(rm, "resource_type")?,
        title: need(rm, "title")?,
        language: need(rm, "language")?,
        order,
        date_created: text(rm, "date_created"),
        license,
        adaptation_notice: text(rm, "adaptation_notice"),
        copyright_statements,
        localizations,
    })
}

/// Where a note or question belongs: its index key, or for an unindexed article its first
/// associated passage.
fn passage_of(a: &AquiferArticle) -> Result<BibleRef, Error> {
    match &a.index {
        IndexKey::Passage(r) => Ok(r.clone()),
        IndexKey::Unindexed => a.passages.first().cloned().ok_or_else(|| {
            article_error(
                a.content_id.as_str(),
                "index_reference",
                format!("{UNINDEXED:?} and no associations.passage place it nowhere"),
            )
        }),
        IndexKey::Sort(key) => Err(article_error(
            a.content_id.as_str(),
            "index_reference",
            format!("{key:?} is not a passage key"),
        )),
    }
}

fn is_intro(r: &BibleRef) -> bool {
    matches!(r.locus, BibleLocus::BookIntro | BibleLocus::ChapterIntro(_))
}

fn is_blank(node: &Node) -> bool {
    matches!(node, Node::Text(t) if t.trim().is_empty())
}

/// The first `<span style="direction: …">` in the subtree: Aquifer's original-language
/// quote.
fn direction_span(nodes: &[Node]) -> Option<&Element> {
    nodes.iter().find_map(|n| match n {
        Node::Element(e)
            if e.name == "span" && e.attr("style").is_some_and(|s| s.contains("direction")) =>
        {
            Some(e)
        }
        Node::Element(e) => direction_span(&e.children),
        Node::Text(_) => None,
    })
}

fn node_text(node: &Node) -> String {
    let mut out = String::new();
    html::collect_text(std::slice::from_ref(node), &mut out);
    out
}

/// A paragraph whose whole text is a double-quoted phrase, such as
/// `<p><strong>"that agrees with godliness"</strong></p>`: Aquifer's gateway quote.
fn gateway_quote(node: &Node) -> Option<String> {
    let Node::Element(p) = node else { return None };
    if p.name != "p" {
        return None;
    }
    let text = node_text(node);
    let text = text.trim();
    let text = text.strip_suffix('.').unwrap_or(text);
    let inner = text.strip_prefix('"')?.strip_suffix('"')?;
    (!inner.contains('"')).then(|| inner.trim().to_owned())
}

/// The sort key of a placed item: book, chapter and verse (0 for introductions), then
/// Aquifer's per-verse ordinal.
fn verse_order(r: &BibleRef, a: &AquiferArticle) -> (Bcv, u32) {
    let (chapter, verse) = match &r.locus {
        BibleLocus::BookIntro => (0, 0),
        BibleLocus::ChapterIntro(c) | BibleLocus::ChapterFront(c) => (*c, 0),
        BibleLocus::AsWritten(text) => (text.chapter().unwrap_or(0), 0),
        BibleLocus::Verses(list) => {
            let start = list.start();
            (start.chapter, start.verse)
        }
    };
    let key = Bcv {
        book: r.book,
        chapter,
        verse,
    };
    (key, a.ordinal().unwrap_or(0))
}

/// Places each article by [`passage_of`], refusing those it cannot place, in verse order.
fn placed(articles: &[AquiferArticle]) -> (Vec<(&AquiferArticle, BibleRef)>, Vec<Error>) {
    let mut out = Vec::new();
    let mut refused = Vec::new();
    for a in articles {
        match passage_of(a) {
            Ok(r) => out.push((a, r)),
            Err(e) => refused.push(e),
        }
    }
    out.sort_by_key(|(a, r)| verse_order(r, a));
    (out, refused)
}

/// Converts the articles of `UWTranslationNotes` into notes, in verse order. An article
/// that names no passage is refused alone.
pub fn notes_from_aquifer(articles: &[AquiferArticle]) -> Loaded<Note> {
    let (placed, refused) = placed(articles);
    let items = placed
        .into_iter()
        .map(|(a, reference)| {
            let mut nodes = html::parse(&a.html);
            let id = HelpItemId::Aquifer(a.content_id.clone());
            if is_intro(&reference) {
                return Note {
                    reference: HelpRef::Bible(reference),
                    id,
                    tags: Vec::new(),
                    support_reference: None,
                    quote: Quote::default(),
                    occurrence: Occurrence::NotTied,
                    gl_quote: None,
                    text: nodes_to_markdown(&nodes),
                    read_from: None,
                };
            }
            nodes.retain(|n| !is_blank(n));
            let mut quote = Quote::default();
            let mut gl_quote = None;
            let quote_at = nodes
                .iter()
                .position(|n| direction_span(std::slice::from_ref(n)).is_some());
            // The gateway quote follows the original-language quote, or opens the note when
            // the original-language quote is missing (por TIT 1:2 #4).
            let mut gl_at = Some(0);
            if let Some(at) = quote_at {
                let text = direction_span(&nodes[at..=at])
                    .map(|s| s.text())
                    .unwrap_or_default();
                let text = text.trim().to_owned();
                // Only a paragraph that holds nothing but the quote is taken out of the body.
                gl_at = None;
                if node_text(&nodes[at]).trim() == text {
                    nodes.remove(at);
                    gl_at = Some(at);
                }
                quote = Quote::new(text);
            }
            if let Some(at) = gl_at
                && let Some(gl) = nodes.get(at).and_then(gateway_quote)
            {
                gl_quote = Some(gl);
                nodes.remove(at);
            }
            let support_reference = a
                .links
                .iter()
                .find(|l| l.resource_code == MANUAL)
                .map(|l| AcademyArticleId::Aquifer(l.content_id.clone()));
            Note {
                reference: HelpRef::Bible(reference),
                id,
                tags: Vec::new(),
                support_reference,
                quote,
                occurrence: Occurrence::Unrecorded,
                gl_quote,
                text: nodes_to_markdown(&nodes),
                read_from: None,
            }
        })
        .collect();
    Loaded { items, refused }
}

/// Converts the articles of `UWTranslationQuestions` into questions, in verse order. The
/// first paragraph is the question (`<p><strong>…?</strong></p>`); the rest is the answer.
/// An article that names no passage is refused alone.
pub fn questions_from_aquifer(articles: &[AquiferArticle]) -> Loaded<Question> {
    let (placed, refused) = placed(articles);
    let items = placed
        .into_iter()
        .map(|(a, reference)| {
            let mut nodes = html::parse(&a.html);
            nodes.retain(|n| !is_blank(n));
            let question = if nodes.is_empty() {
                String::new()
            } else {
                let first = nodes.remove(0);
                match &first {
                    Node::Element(p) => {
                        let kids: Vec<&Node> = p.children.iter().filter(|n| !is_blank(n)).collect();
                        match kids.as_slice() {
                            [Node::Element(s)] if s.name == "strong" || s.name == "b" => {
                                nodes_to_markdown(&s.children)
                            }
                            _ => nodes_to_markdown(std::slice::from_ref(&first)),
                        }
                    }
                    Node::Text(_) => nodes_to_markdown(std::slice::from_ref(&first)),
                }
            };
            Question {
                reference: HelpRef::Bible(reference),
                id: HelpItemId::Aquifer(a.content_id.clone()),
                tags: Vec::new(),
                quote: Quote::default(),
                occurrence: Occurrence::Unrecorded,
                question,
                response: nodes_to_markdown(&nodes),
                read_from: None,
            }
        })
        .collect();
    Loaded { items, refused }
}

/// Translation words converted from `UWTranslationWords`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AquiferWords {
    pub articles: Vec<WordArticle>,
    /// One link per passage an article lists. They name no original-language words, so
    /// they show which articles belong to a verse but cannot drive a check.
    pub links: Vec<WordLink>,
}

/// Converts the articles of `UWTranslationWords`.
pub fn words_from_aquifer(articles: &[AquiferArticle]) -> AquiferWords {
    let mut out = AquiferWords {
        articles: Vec::new(),
        links: Vec::new(),
    };
    for a in articles {
        let id = WordArticleId::Aquifer(a.content_id.clone());
        for passage in &a.passages {
            out.links.push(WordLink {
                reference: HelpRef::Bible(passage.clone()),
                id: HelpItemId::Aquifer(a.content_id.clone()),
                tags: Vec::new(),
                orig_words: Quote::default(),
                occurrence: Occurrence::Unrecorded,
                article: id.clone(),
                read_from: None,
            });
        }
        out.articles.push(WordArticle {
            id,
            title: a.title.clone(),
            body: html_to_markdown(&a.html),
        });
    }
    out
}

/// Converts the articles of `UWTranslationManual`.
pub fn academy_from_aquifer(articles: &[AquiferArticle]) -> Vec<AcademyArticle> {
    articles
        .iter()
        .map(|a| AcademyArticle {
            id: AcademyArticleId::Aquifer(a.content_id.clone()),
            title: a.title.clone(),
            subtitle: None,
            body: html_to_markdown(&a.html),
        })
        .collect()
}
