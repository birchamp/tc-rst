//! A set of helps from one source, and what the UI asks of it for a passage.

use std::collections::BTreeMap;

use tc_core::{BookId, Reference};

use crate::ids::{AcademyArticleId, WordArticleId};
use crate::links::LinkTarget;
use crate::metadata::HelpsSource;
use crate::model::{AcademyArticle, CheckReadiness, Note, Question, WordArticle, WordLink};
use crate::refs::{BibleLocus, BibleRef, HelpRef, ObsRef};

/// Helps from one source: notes, word links and questions for any number of books or
/// for Open Bible Stories, and the articles they link to. Door43 and Aquifer helps both
/// load into it, so the helps screens ask the same questions of either.
#[derive(Debug, Clone)]
pub struct Helps {
    source: HelpsSource,
    notes: Vec<Note>,
    word_links: Vec<WordLink>,
    questions: Vec<Question>,
    words: BTreeMap<WordArticleId, WordArticle>,
    academy: BTreeMap<AcademyArticleId, AcademyArticle>,
}

/// An article a link resolves to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Article<'a> {
    Word(&'a WordArticle),
    Academy(&'a AcademyArticle),
}

impl Helps {
    pub fn new(source: HelpsSource) -> Self {
        Self {
            source,
            notes: Vec::new(),
            word_links: Vec::new(),
            questions: Vec::new(),
            words: BTreeMap::new(),
            academy: BTreeMap::new(),
        }
    }

    pub fn source(&self) -> &HelpsSource {
        &self.source
    }

    pub fn add_notes(&mut self, notes: impl IntoIterator<Item = Note>) {
        self.notes.extend(notes);
    }

    pub fn add_word_links(&mut self, links: impl IntoIterator<Item = WordLink>) {
        self.word_links.extend(links);
    }

    pub fn add_questions(&mut self, questions: impl IntoIterator<Item = Question>) {
        self.questions.extend(questions);
    }

    /// Adds articles. An article with the id of one already held replaces it.
    pub fn add_word_articles(&mut self, articles: impl IntoIterator<Item = WordArticle>) {
        self.words
            .extend(articles.into_iter().map(|a| (a.id.clone(), a)));
    }

    /// Adds articles. An article with the id of one already held replaces it.
    pub fn add_academy_articles(&mut self, articles: impl IntoIterator<Item = AcademyArticle>) {
        self.academy
            .extend(articles.into_iter().map(|a| (a.id.clone(), a)));
    }

    pub fn notes(&self) -> &[Note] {
        &self.notes
    }

    pub fn word_links(&self) -> &[WordLink] {
        &self.word_links
    }

    pub fn questions(&self) -> &[Question] {
        &self.questions
    }

    /// Notes on any verse of `passage`, in source order. Introductions are not included;
    /// ask for them with [`Helps::introductions`].
    pub fn notes_for(&self, passage: Reference) -> impl Iterator<Item = &Note> {
        self.notes
            .iter()
            .filter(move |n| on_passage(&n.reference, &passage))
    }

    /// The book introduction (`chapter` `None`) or a chapter introduction.
    pub fn introductions(&self, book: BookId, chapter: Option<u16>) -> impl Iterator<Item = &Note> {
        self.notes.iter().filter(move |n| match &n.reference {
            HelpRef::Bible(BibleRef { book: b, locus }) if *b == book => match (locus, chapter) {
                (BibleLocus::BookIntro, None) => true,
                (BibleLocus::ChapterIntro(c), Some(want)) => *c == want,
                _ => false,
            },
            _ => false,
        })
    }

    /// Notes on one Open Bible Stories frame.
    pub fn obs_notes_for(&self, frame: ObsRef) -> impl Iterator<Item = &Note> {
        self.notes
            .iter()
            .filter(move |n| n.reference == HelpRef::Obs(frame))
    }

    /// Word links on any verse of `passage`, in source order.
    pub fn word_links_for(&self, passage: Reference) -> impl Iterator<Item = &WordLink> {
        self.word_links
            .iter()
            .filter(move |l| on_passage(&l.reference, &passage))
    }

    /// The distinct articles linked from `passage`, in the order of their first link,
    /// with the article when this set holds it.
    pub fn words_for(&self, passage: Reference) -> Vec<(&WordArticleId, Option<&WordArticle>)> {
        let mut seen = Vec::new();
        for link in self.word_links_for(passage) {
            if !seen.iter().any(|(id, _)| *id == &link.article) {
                seen.push((&link.article, self.words.get(&link.article)));
            }
        }
        seen
    }

    /// Questions on any verse of `passage`.
    pub fn questions_for(&self, passage: Reference) -> impl Iterator<Item = &Question> {
        self.questions
            .iter()
            .filter(move |q| on_passage(&q.reference, &passage))
    }

    pub fn word_article(&self, id: &WordArticleId) -> Option<&WordArticle> {
        self.words.get(id)
    }

    pub fn academy_article(&self, id: &AcademyArticleId) -> Option<&AcademyArticle> {
        self.academy.get(id)
    }

    /// The article a resolved link opens, when this set holds it.
    pub fn article(&self, target: &LinkTarget) -> Option<Article<'_>> {
        match target {
            LinkTarget::Word(id) => self.word_article(id).map(Article::Word),
            LinkTarget::Academy(id) => self.academy_article(id).map(Article::Academy),
            _ => None,
        }
    }

    /// Whether a word link can drive a check, given the articles this set holds.
    pub fn word_link_readiness(&self, link: &WordLink) -> CheckReadiness {
        link.check_readiness(self.words.contains_key(&link.article))
    }
}

fn on_passage(reference: &HelpRef, passage: &Reference) -> bool {
    match reference {
        HelpRef::Bible(r) => r.overlaps(passage),
        HelpRef::Obs(_) => false,
    }
}
