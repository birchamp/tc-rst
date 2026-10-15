//! A set of helps, each part from one pinned resource, and what the UI asks of it for a
//! passage.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::ops::Deref;

use tc_core::{BookId, Reference};

use crate::Error;
use crate::ids::{AcademyArticleId, WordArticleId};
use crate::links::LinkTarget;
use crate::metadata::HelpsSource;
use crate::model::{
    AcademyArticle, CheckBlocker, CheckReadiness, Note, Question, WordArticle, WordLink,
};
use crate::refs::{BibleLocus, BibleRef, HelpRef, ObsRef};

/// The parts of a helps set. The passage questions need several repositories at once (for
/// en: en_tn, en_twl, en_tw, en_ta, en_tq, and the en_obs-* ones for stories), so each
/// part records the pinned resource it was read from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum HelpsPart {
    Notes,
    ObsNotes,
    WordLinks,
    ObsWordLinks,
    Questions,
    ObsQuestions,
    WordArticles,
    AcademyArticles,
}

impl HelpsPart {
    fn of(reference: &HelpRef, bible: Self, obs: Self) -> Self {
        match reference {
            HelpRef::Bible(_) => bible,
            HelpRef::Obs(_) => obs,
        }
    }
}

impl fmt::Display for HelpsPart {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            HelpsPart::Notes => "translation notes",
            HelpsPart::ObsNotes => "Open Bible Stories notes",
            HelpsPart::WordLinks => "translation words links",
            HelpsPart::ObsWordLinks => "Open Bible Stories words links",
            HelpsPart::Questions => "translation questions",
            HelpsPart::ObsQuestions => "Open Bible Stories questions",
            HelpsPart::WordArticles => "translation words articles",
            HelpsPart::AcademyArticles => "Translation Academy articles",
        })
    }
}

/// An item of a helps set and the pinned resource it was read from, which a check
/// decision records. It reads as the item.
#[derive(Debug, PartialEq, Eq)]
pub struct Found<'a, T> {
    pub item: &'a T,
    pub source: &'a HelpsSource,
}

impl<T> Clone for Found<'_, T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T> Copy for Found<'_, T> {}

impl<T> Deref for Found<'_, T> {
    type Target = T;

    fn deref(&self) -> &T {
        self.item
    }
}

/// Notes, word links and questions for any number of books and for Open Bible Stories,
/// and the articles they link to, each part from one pinned resource. Door43 and Aquifer
/// helps both load into it, so the helps screens ask the same questions of either.
#[derive(Debug, Clone, Default)]
pub struct Helps {
    pins: BTreeMap<HelpsPart, HelpsSource>,
    notes: Vec<Note>,
    word_links: Vec<WordLink>,
    questions: Vec<Question>,
    words: BTreeMap<WordArticleId, WordArticle>,
    academy: BTreeMap<AcademyArticleId, AcademyArticle>,
}

/// An article a link resolves to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Article<'a> {
    Word(Found<'a, WordArticle>),
    Academy(Found<'a, AcademyArticle>),
}

impl Helps {
    pub fn new() -> Self {
        Self::default()
    }

    /// The pinned resource a part was read from; `None` until something is added to it.
    pub fn source(&self, part: HelpsPart) -> Option<&HelpsSource> {
        self.pins.get(&part)
    }

    /// Records `source` for `parts`, or refuses when a part already has another source:
    /// one part from two pins would answer from either without saying which.
    fn pin(&mut self, parts: BTreeSet<HelpsPart>, source: &HelpsSource) -> Result<(), Error> {
        for part in &parts {
            if let Some(held) = self.pins.get(part)
                && held != source
            {
                return Err(Error::PinConflict {
                    part: *part,
                    held: Box::new(held.clone()),
                    offered: Box::new(source.clone()),
                });
            }
        }
        for part in parts {
            self.pins.insert(part, source.clone());
        }
        Ok(())
    }

    /// Adds notes read from `source`: Bible notes to [`HelpsPart::Notes`], story notes to
    /// [`HelpsPart::ObsNotes`]. Refused, adding nothing, when a part they go to is pinned
    /// to another source. An empty list pins nothing.
    pub fn add_notes(
        &mut self,
        source: &HelpsSource,
        notes: impl IntoIterator<Item = Note>,
    ) -> Result<(), Error> {
        let notes: Vec<Note> = notes.into_iter().collect();
        let parts = notes
            .iter()
            .map(|n| HelpsPart::of(&n.reference, HelpsPart::Notes, HelpsPart::ObsNotes))
            .collect();
        self.pin(parts, source)?;
        self.notes.extend(notes);
        Ok(())
    }

    /// Adds word links read from `source`, as [`Helps::add_notes`] adds notes.
    pub fn add_word_links(
        &mut self,
        source: &HelpsSource,
        links: impl IntoIterator<Item = WordLink>,
    ) -> Result<(), Error> {
        let links: Vec<WordLink> = links.into_iter().collect();
        let parts = links
            .iter()
            .map(|l| HelpsPart::of(&l.reference, HelpsPart::WordLinks, HelpsPart::ObsWordLinks))
            .collect();
        self.pin(parts, source)?;
        self.word_links.extend(links);
        Ok(())
    }

    /// Adds questions read from `source`, as [`Helps::add_notes`] adds notes.
    pub fn add_questions(
        &mut self,
        source: &HelpsSource,
        questions: impl IntoIterator<Item = Question>,
    ) -> Result<(), Error> {
        let questions: Vec<Question> = questions.into_iter().collect();
        let parts = questions
            .iter()
            .map(|q| HelpsPart::of(&q.reference, HelpsPart::Questions, HelpsPart::ObsQuestions))
            .collect();
        self.pin(parts, source)?;
        self.questions.extend(questions);
        Ok(())
    }

    /// Adds translation words articles read from `source`. An article with the id of one
    /// already held replaces it.
    pub fn add_word_articles(
        &mut self,
        source: &HelpsSource,
        articles: impl IntoIterator<Item = WordArticle>,
    ) -> Result<(), Error> {
        self.pin([HelpsPart::WordArticles].into(), source)?;
        self.words
            .extend(articles.into_iter().map(|a| (a.id.clone(), a)));
        Ok(())
    }

    /// Adds Translation Academy articles read from `source`. An article with the id of one
    /// already held replaces it.
    pub fn add_academy_articles(
        &mut self,
        source: &HelpsSource,
        articles: impl IntoIterator<Item = AcademyArticle>,
    ) -> Result<(), Error> {
        self.pin([HelpsPart::AcademyArticles].into(), source)?;
        self.academy
            .extend(articles.into_iter().map(|a| (a.id.clone(), a)));
        Ok(())
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

    /// The item with the source of its part. Every held item's part is pinned when the
    /// item is added, so the lookup finds it.
    fn found<'a, T>(&'a self, item: &'a T, part: HelpsPart) -> Option<Found<'a, T>> {
        self.pins.get(&part).map(|source| Found { item, source })
    }

    fn note<'a>(&'a self, n: &'a Note) -> Option<Found<'a, Note>> {
        self.found(
            n,
            HelpsPart::of(&n.reference, HelpsPart::Notes, HelpsPart::ObsNotes),
        )
    }

    fn word_link<'a>(&'a self, l: &'a WordLink) -> Option<Found<'a, WordLink>> {
        self.found(
            l,
            HelpsPart::of(&l.reference, HelpsPart::WordLinks, HelpsPart::ObsWordLinks),
        )
    }

    fn question<'a>(&'a self, q: &'a Question) -> Option<Found<'a, Question>> {
        self.found(
            q,
            HelpsPart::of(&q.reference, HelpsPart::Questions, HelpsPart::ObsQuestions),
        )
    }

    /// Notes on any verse of `passage`, in source order. Introductions are not included;
    /// ask for them with [`Helps::introductions`].
    pub fn notes_for(&self, passage: Reference) -> impl Iterator<Item = Found<'_, Note>> {
        self.notes
            .iter()
            .filter(move |n| on_passage(&n.reference, &passage))
            .filter_map(|n| self.note(n))
    }

    /// The book introduction (`chapter` `None`) or a chapter introduction.
    pub fn introductions(
        &self,
        book: BookId,
        chapter: Option<u16>,
    ) -> impl Iterator<Item = Found<'_, Note>> {
        self.notes
            .iter()
            .filter(move |n| match &n.reference {
                HelpRef::Bible(BibleRef { book: b, locus }) if *b == book => {
                    match (locus, chapter) {
                        (BibleLocus::BookIntro, None) => true,
                        (BibleLocus::ChapterIntro(c), Some(want)) => *c == want,
                        _ => false,
                    }
                }
                _ => false,
            })
            .filter_map(|n| self.note(n))
    }

    /// Notes on one Open Bible Stories frame, in source order.
    pub fn obs_notes_for(&self, frame: ObsRef) -> impl Iterator<Item = Found<'_, Note>> {
        self.notes
            .iter()
            .filter(move |n| n.reference == HelpRef::Obs(frame))
            .filter_map(|n| self.note(n))
    }

    /// Word links on any verse of `passage`, in source order.
    pub fn word_links_for(&self, passage: Reference) -> impl Iterator<Item = Found<'_, WordLink>> {
        self.word_links
            .iter()
            .filter(move |l| on_passage(&l.reference, &passage))
            .filter_map(|l| self.word_link(l))
    }

    /// Word links on one Open Bible Stories frame, in source order.
    pub fn obs_word_links_for(&self, frame: ObsRef) -> impl Iterator<Item = Found<'_, WordLink>> {
        self.word_links
            .iter()
            .filter(move |l| l.reference == HelpRef::Obs(frame))
            .filter_map(|l| self.word_link(l))
    }

    /// The distinct articles linked from `passage`, in the order of their first link,
    /// with the article when this set holds it.
    pub fn words_for(
        &self,
        passage: Reference,
    ) -> Vec<(&WordArticleId, Option<Found<'_, WordArticle>>)> {
        self.distinct_articles(self.word_links_for(passage))
    }

    /// The distinct articles linked from one story frame, as [`Helps::words_for`].
    pub fn obs_words_for(
        &self,
        frame: ObsRef,
    ) -> Vec<(&WordArticleId, Option<Found<'_, WordArticle>>)> {
        self.distinct_articles(self.obs_word_links_for(frame))
    }

    fn distinct_articles<'a>(
        &'a self,
        links: impl Iterator<Item = Found<'a, WordLink>>,
    ) -> Vec<(&'a WordArticleId, Option<Found<'a, WordArticle>>)> {
        let mut seen: Vec<(&WordArticleId, Option<Found<'_, WordArticle>>)> = Vec::new();
        for link in links {
            let id = &link.item.article;
            if !seen.iter().any(|(held, _)| *held == id) {
                seen.push((id, self.word_article(id)));
            }
        }
        seen
    }

    /// Questions on any verse of `passage`, in source order.
    pub fn questions_for(&self, passage: Reference) -> impl Iterator<Item = Found<'_, Question>> {
        self.questions
            .iter()
            .filter(move |q| on_passage(&q.reference, &passage))
            .filter_map(|q| self.question(q))
    }

    /// Questions on one Open Bible Stories frame, in source order.
    pub fn obs_questions_for(&self, frame: ObsRef) -> impl Iterator<Item = Found<'_, Question>> {
        self.questions
            .iter()
            .filter(move |q| q.reference == HelpRef::Obs(frame))
            .filter_map(|q| self.question(q))
    }

    pub fn word_article(&self, id: &WordArticleId) -> Option<Found<'_, WordArticle>> {
        let article = self.words.get(id)?;
        self.found(article, HelpsPart::WordArticles)
    }

    pub fn academy_article(&self, id: &AcademyArticleId) -> Option<Found<'_, AcademyArticle>> {
        let article = self.academy.get(id)?;
        self.found(article, HelpsPart::AcademyArticles)
    }

    /// The article a resolved link opens, when this set holds it.
    pub fn article(&self, target: &LinkTarget) -> Option<Article<'_>> {
        match target {
            LinkTarget::Word(id) => self.word_article(id).map(Article::Word),
            LinkTarget::Academy(id) => self.academy_article(id).map(Article::Academy),
            _ => None,
        }
    }

    /// Whether a word link can drive a check, judged against the articles of the
    /// [`HelpsPart::WordArticles`] source, which [`Helps::source`] names. With no words
    /// articles added, the link is blocked by [`CheckBlocker::WordsNotLoaded`].
    pub fn word_link_readiness(&self, link: &WordLink) -> CheckReadiness {
        if self.source(HelpsPart::WordArticles).is_none() {
            return link.readiness(Some(CheckBlocker::WordsNotLoaded));
        }
        link.check_readiness(self.words.contains_key(&link.article))
    }
}

fn on_passage(reference: &HelpRef, passage: &Reference) -> bool {
    match reference {
        HelpRef::Bible(r) => r.overlaps(passage),
        HelpRef::Obs(_) => false,
    }
}
