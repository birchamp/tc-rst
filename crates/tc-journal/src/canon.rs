//! The one serializer of the journal (`docs/STORAGE.md` §2.5 and the wire table of §2.8):
//! every type to and from canonical JSON.
//!
//! Field names are camelCase. A variant of an enum whose variants carry no data is its
//! camelCase name in a string. Every other variant is an object with one member named
//! after the variant: its fields as an object (`{}` when it has none), or, for a variant
//! that wraps one value, that value's encoding. `None` fields are left out; `extra` maps
//! are left out when empty; empty arrays are written.
//!
//! Reading is strict for a change whose `v` is at or below this app's minor: a missing,
//! unknown or mistyped field, or an unknown variant, is a V1 violation. For a newer `v`,
//! unknown fields are ignored (they stay in the stored bytes), an unknown record kind reads
//! as [`Record::Unknown`], an unknown cause as [`Cause::Newer`], and any other unknown
//! variant makes the change a quarantined [`RuleViolation::NewerVariant`].

use crate::change::{Cause, ChangeBody, ChangeError, ExternalSource, ImportSource, SystemReason};
use crate::format::FORMAT_MINOR;
use crate::ids::{
    BatchId, BatchRef, ChangeId, ChapterNo, CommentId, DeviceId, DeviceSeq, FrameNo, GitOid, Md5,
    PackName, ProjectId, RepoPath, ShortId, StoryNo, VerseNo, WallTime,
};
use crate::json::{self, Json, JsonInt, JsonMap, ObjIter, ObjView, View};
use crate::keys::{
    CheckId, CheckKey, CheckTarget, KNOWN_KINDS, MetaField, MetaPath, PinGroup, RecordKey, RefPart,
    ResourcesKey, ResourcesLevel, SettingKey, SlotName, ToolId, UnknownKey,
};
use crate::record::{
    AlignmentGroup, AlignmentValue, BatchReleaseValue, BookPresence, CarriedValue, ChapterFront,
    ChapterVerse, CheckValue, CommentTarget, CommentValue, DecisionRecord, DeviceProfileValue,
    Door43Username, Judgement, LayoutItem, LegacyIdentity, MarkerTerm, MarkerWs, PinGroupValue,
    PresentHow, Record, ResourceRef, Scope, ScopeRange, ScopeRanges, Selection, SlotValue,
    SourceWord, TargetWord, UnknownRecord, UserStatus, UsfmBytes, VerseValue, VersificationValue,
};
use crate::rules::RuleViolation;
use smallvec::SmallVec;
use tc_core::BookId;

// ---------------------------------------------------------------------------------------
// Errors and context
// ---------------------------------------------------------------------------------------

#[derive(Debug, Clone, Copy)]
pub(crate) struct Cx {
    /// The change comes from a newer minor: unknown fields are ignored.
    pub(crate) lenient: bool,
}

pub(crate) const STRICT: Cx = Cx { lenient: false };

#[derive(Debug, Clone)]
enum Problem {
    Shape(String),
    Newer(String),
    Identity(String),
}

/// A decoding failure with the JSON path it happened at (segments innermost first).
#[derive(Debug, Clone)]
pub(crate) struct DecodeError {
    problem: Problem,
    path: Vec<Box<str>>,
}

impl DecodeError {
    fn shape(text: impl Into<String>) -> Self {
        Self {
            problem: Problem::Shape(text.into()),
            path: Vec::new(),
        }
    }

    fn identity(text: impl std::fmt::Display) -> Self {
        Self {
            problem: Problem::Identity(text.to_string()),
            path: Vec::new(),
        }
    }

    fn within(mut self, segment: &str) -> Self {
        self.path.push(segment.into());
        self
    }

    fn is_newer(&self) -> bool {
        matches!(self.problem, Problem::Newer(_))
    }

    fn at(&self) -> Box<str> {
        let mut out = String::new();
        for segment in self.path.iter().rev() {
            out.push('/');
            out.push_str(segment);
        }
        out.into()
    }

    pub(crate) fn into_violation(self) -> RuleViolation {
        let at = self.at();
        match self.problem {
            Problem::Shape(problem) => RuleViolation::Shape {
                at,
                problem: problem.into(),
            },
            Problem::Newer(name) => RuleViolation::NewerVariant {
                at,
                name: name.into(),
            },
            Problem::Identity(problem) => RuleViolation::Identity {
                at,
                problem: problem.into(),
            },
        }
    }
}

type R<T> = Result<T, DecodeError>;

/// An unknown variant: made by a newer minor, or malformed.
fn unknown(cx: Cx, what: &str, name: &str) -> DecodeError {
    if cx.lenient {
        DecodeError {
            problem: Problem::Newer(name.to_owned()),
            path: Vec::new(),
        }
    } else {
        DecodeError::shape(format!("unknown {what} {name:?}"))
    }
}

fn expected(what: &str, found: View<'_>) -> DecodeError {
    DecodeError::shape(format!("expected {what}, found {}", found.kind_name()))
}

// ---------------------------------------------------------------------------------------
// The Wire trait and object helpers
// ---------------------------------------------------------------------------------------

pub(crate) trait Wire: Sized {
    fn to_json(&self) -> Json;
    fn from_json(j: View<'_>, cx: Cx) -> R<Self>;
}

/// Builds an object.
struct Out(JsonMap);

impl Out {
    fn new() -> Self {
        Self(JsonMap::new())
    }

    fn f<T: Wire>(mut self, key: &str, value: &T) -> Self {
        self.0.insert(key.to_owned(), value.to_json());
        self
    }

    fn j(mut self, key: &str, value: Json) -> Self {
        self.0.insert(key.to_owned(), value);
        self
    }

    fn o<T: Wire>(mut self, key: &str, value: Option<&T>) -> Self {
        if let Some(value) = value {
            self.0.insert(key.to_owned(), value.to_json());
        }
        self
    }

    fn x(mut self, key: &str, value: &JsonMap) -> Self {
        if !value.is_empty() {
            self.0.insert(key.to_owned(), Json::Obj(value.clone()));
        }
        self
    }

    fn done(self) -> Json {
        Json::Obj(self.0)
    }
}

fn variant(name: &str, inner: Json) -> Json {
    let mut map = JsonMap::new();
    map.insert(name.to_owned(), inner);
    Json::Obj(map)
}

fn unit(name: &str) -> Json {
    variant(name, Json::Obj(JsonMap::new()))
}

/// Reads an object's fields, then checks that no field is left over.
struct In<'a> {
    map: ObjView<'a>,
    /// The members after the last one read: the decoders read fields in key order, so
    /// each lookup usually continues from here.
    rest: ObjIter<'a>,
    /// The key of the member just before `rest`: every member before `rest` has a key no
    /// greater than it.
    last: Option<&'static str>,
    taken: SmallVec<[&'static str; 12]>,
    cx: Cx,
}

impl<'a> In<'a> {
    fn new(j: View<'a>, cx: Cx) -> R<Self> {
        match j.as_obj() {
            Some(map) => Ok(Self {
                map,
                rest: map.iter(),
                last: None,
                taken: SmallVec::new(),
                cx,
            }),
            None => Err(expected("an object", j)),
        }
    }

    fn get(&mut self, key: &'static str) -> Option<View<'a>> {
        let value = match json::find_member(self.rest.clone(), key) {
            Some((value, rest)) => {
                self.rest = rest;
                self.last = Some(key);
                Some(value)
            }
            // Keys are sorted: a key past `last` that is not after `rest` is absent.
            None if self
                .last
                .is_none_or(|last| last.as_bytes() < key.as_bytes()) =>
            {
                None
            }
            None => self.map.get(key),
        };
        if value.is_some() {
            self.taken.push(key);
        }
        value
    }

    fn req<T: Wire>(&mut self, key: &'static str) -> R<T> {
        let cx = self.cx;
        self.req_with(key, |j| T::from_json(j, cx))
    }

    fn req_with<T>(&mut self, key: &'static str, read: impl FnOnce(View<'a>) -> R<T>) -> R<T> {
        match self.get(key) {
            Some(j) => read(j).map_err(|e| e.within(key)),
            None => Err(DecodeError::shape(format!("missing {key}"))),
        }
    }

    fn opt<T: Wire>(&mut self, key: &'static str) -> R<Option<T>> {
        let cx = self.cx;
        match self.get(key) {
            Some(j) => T::from_json(j, cx).map(Some).map_err(|e| e.within(key)),
            None => Ok(None),
        }
    }

    fn extra(&mut self, key: &'static str) -> R<JsonMap> {
        match self.get(key) {
            None => Ok(JsonMap::new()),
            Some(j) => match j.as_obj() {
                Some(map) if map.is_empty() => {
                    Err(DecodeError::shape("an empty extra map is left out").within(key))
                }
                Some(map) => Ok(map.to_map()),
                None => Err(expected("an object", j).within(key)),
            },
        }
    }

    fn finish(self) -> R<()> {
        if self.cx.lenient || self.taken.len() == self.map.len() {
            return Ok(());
        }
        let unknown = self
            .map
            .iter()
            .map(|(k, _)| k)
            .find(|k| !self.taken.contains(k))
            .unwrap_or("");
        Err(DecodeError::shape(format!("unknown field {unknown:?}")))
    }
}

/// The one member of a variant object.
fn one_member(j: View<'_>) -> R<(&str, View<'_>)> {
    match j.as_obj() {
        Some(map) if map.len() == 1 => map.iter().next().ok_or_else(|| DecodeError::shape("")),
        Some(_) => Err(DecodeError::shape(
            "expected an object with exactly one member",
        )),
        None => Err(expected("an object", j)),
    }
}

/// The one member of an enum's object, when it names a known variant. An unknown variant
/// is reported at the enum's own path.
fn known_member<'a>(j: View<'a>, cx: Cx, what: &str, names: &[&str]) -> R<(&'a str, View<'a>)> {
    let (name, inner) = one_member(j)?;
    if names.contains(&name) {
        Ok((name, inner))
    } else {
        Err(unknown(cx, what, name))
    }
}

/// A variant without fields: `{}`, or any object from a newer minor.
fn no_fields(j: View<'_>, cx: Cx) -> R<()> {
    In::new(j, cx)?.finish()
}

fn string(j: View<'_>) -> R<&str> {
    j.as_str().ok_or_else(|| expected("a string", j))
}

// ---------------------------------------------------------------------------------------
// Scalars
// ---------------------------------------------------------------------------------------

impl Wire for bool {
    fn to_json(&self) -> Json {
        Json::Bool(*self)
    }

    fn from_json(j: View<'_>, _: Cx) -> R<Self> {
        j.as_bool().ok_or_else(|| expected("a boolean", j))
    }
}

fn int_in(j: View<'_>, min: i128, max: i128) -> R<i128> {
    match j.as_int() {
        Some(n) if (min..=max).contains(&n) => Ok(n),
        Some(n) => Err(DecodeError::shape(format!("{n} is outside {min} to {max}"))),
        None => Err(expected("an integer", j)),
    }
}

macro_rules! wire_int {
    ($($t:ty),*) => {$(
        impl Wire for $t {
            fn to_json(&self) -> Json {
                Json::Int(JsonInt::from(*self))
            }

            fn from_json(j: View<'_>, _: Cx) -> R<Self> {
                let n = int_in(j, i128::from(<$t>::MIN), i128::from(<$t>::MAX))?;
                <$t>::try_from(n).map_err(|_| DecodeError::shape("integer out of range"))
            }
        }
    )*};
}
wire_int!(u8, u16, u32, u64, i64);

impl Wire for String {
    fn to_json(&self) -> Json {
        Json::Str(self.clone())
    }

    fn from_json(j: View<'_>, _: Cx) -> R<Self> {
        string(j).map(str::to_owned)
    }
}

impl Wire for Box<str> {
    fn to_json(&self) -> Json {
        Json::Str(self.to_string())
    }

    fn from_json(j: View<'_>, _: Cx) -> R<Self> {
        string(j).map(Into::into)
    }
}

impl Wire for Json {
    fn to_json(&self) -> Json {
        self.clone()
    }

    fn from_json(j: View<'_>, _: Cx) -> R<Self> {
        Ok(j.to_json())
    }
}

impl Wire for JsonMap {
    fn to_json(&self) -> Json {
        Json::Obj(self.clone())
    }

    fn from_json(j: View<'_>, _: Cx) -> R<Self> {
        j.as_obj()
            .map(ObjView::to_map)
            .ok_or_else(|| expected("an object", j))
    }
}

impl<T: Wire> Wire for Vec<T> {
    fn to_json(&self) -> Json {
        Json::Arr(self.iter().map(Wire::to_json).collect())
    }

    fn from_json(j: View<'_>, cx: Cx) -> R<Self> {
        match j.as_arr() {
            Some(items) => {
                let mut out = Vec::with_capacity(items.len());
                for (i, item) in items.iter().enumerate() {
                    out.push(T::from_json(item, cx).map_err(|e| e.within(&i.to_string()))?);
                }
                Ok(out)
            }
            None => Err(expected("an array", j)),
        }
    }
}

macro_rules! wire_hex {
    ($($t:ty),*) => {$(
        impl Wire for $t {
            fn to_json(&self) -> Json {
                Json::Str(self.to_hex())
            }

            fn from_json(j: View<'_>, _: Cx) -> R<Self> {
                let text = string(j)?;
                <$t>::parse(text).ok_or_else(|| {
                    DecodeError::shape(format!("{text:?} is not a {}", stringify!($t)))
                })
            }
        }
    )*};
}
wire_hex!(ChangeId, ShortId, BatchId, CommentId, Md5, GitOid, PackName);

impl Wire for DeviceId {
    fn to_json(&self) -> Json {
        Json::Str(self.to_string())
    }

    fn from_json(j: View<'_>, _: Cx) -> R<Self> {
        let text = string(j)?;
        DeviceId::parse(text)
            .ok_or_else(|| DecodeError::shape(format!("{text:?} is not a device id")))
    }
}

impl Wire for ProjectId {
    fn to_json(&self) -> Json {
        Json::Str(self.to_string())
    }

    fn from_json(j: View<'_>, _: Cx) -> R<Self> {
        let text = string(j)?;
        ProjectId::parse(text)
            .ok_or_else(|| DecodeError::shape(format!("{text:?} is not a project id")))
    }
}

impl Wire for BookId {
    fn to_json(&self) -> Json {
        Json::Str(self.code().to_owned())
    }

    fn from_json(j: View<'_>, cx: Cx) -> R<Self> {
        let text = string(j)?;
        match text.parse::<BookId>() {
            Ok(book) if book.code() == text => Ok(book),
            _ => Err(unknown(cx, "book", text)),
        }
    }
}

impl Wire for ChapterNo {
    fn to_json(&self) -> Json {
        Json::from(self.get())
    }

    fn from_json(j: View<'_>, _: Cx) -> R<Self> {
        let n = int_in(j, 1, i128::from(u16::MAX))?;
        u16::try_from(n)
            .ok()
            .and_then(ChapterNo::new)
            .ok_or_else(|| DecodeError::shape("not a chapter number"))
    }
}

impl Wire for VerseNo {
    fn to_json(&self) -> Json {
        Json::from(self.get())
    }

    fn from_json(j: View<'_>, _: Cx) -> R<Self> {
        let n = int_in(j, 1, i128::from(u16::MAX))?;
        u16::try_from(n)
            .ok()
            .and_then(VerseNo::new)
            .ok_or_else(|| DecodeError::shape("not a verse number"))
    }
}

impl Wire for StoryNo {
    fn to_json(&self) -> Json {
        Json::from(self.get())
    }

    fn from_json(j: View<'_>, _: Cx) -> R<Self> {
        let n = int_in(j, 1, 50)?;
        u8::try_from(n)
            .ok()
            .and_then(StoryNo::new)
            .ok_or_else(|| DecodeError::shape("not a story number"))
    }
}

impl Wire for FrameNo {
    fn to_json(&self) -> Json {
        Json::from(self.get())
    }

    fn from_json(j: View<'_>, cx: Cx) -> R<Self> {
        u16::from_json(j, cx).map(FrameNo::new)
    }
}

impl Wire for RepoPath {
    fn to_json(&self) -> Json {
        Json::Str(self.as_str().to_owned())
    }

    fn from_json(j: View<'_>, _: Cx) -> R<Self> {
        RepoPath::parse(string(j)?).map_err(DecodeError::identity)
    }
}

macro_rules! wire_identity {
    ($($t:ty),*) => {$(
        impl Wire for $t {
            fn to_json(&self) -> Json {
                Json::Str(self.as_str().to_owned())
            }

            fn from_json(j: View<'_>, _: Cx) -> R<Self> {
                <$t>::parse(string(j)?).map_err(DecodeError::identity)
            }
        }
    )*};
}
wire_identity!(CheckId, RefPart, SettingKey, SlotName, MetaPath);

/// The key members of a `resourcesExtra` record: `level` and `name`.
fn resources_key(o: &mut In<'_>) -> R<ResourcesKey> {
    let level: ResourcesLevel = o.req("level")?;
    let name: String = o.req("name")?;
    ResourcesKey::new(level, &name).map_err(|e| DecodeError::identity(e).within("name"))
}

/// Enums whose variants carry no data: the camelCase name in a string.
macro_rules! wire_names {
    ($t:ty, $what:literal, $($variant:ident = $name:literal),* $(,)?) => {
        impl Wire for $t {
            fn to_json(&self) -> Json {
                Json::Str(match self { $(Self::$variant => $name,)* }.to_owned())
            }

            fn from_json(j: View<'_>, cx: Cx) -> R<Self> {
                match string(j)? {
                    $($name => Ok(Self::$variant),)*
                    other => Err(unknown(cx, $what, other)),
                }
            }
        }
    };
}
wire_names!(
    ToolId,
    "tool",
    TranslationWords = "translationWords",
    TranslationNotes = "translationNotes",
    TranslationQuestions = "translationQuestions",
);
wire_names!(
    PinGroup,
    "pin group",
    Primary = "primary",
    Fallback = "fallback",
    OriginalLanguage = "originalLanguage",
    Lexicon = "lexicon",
    ExtraScripture = "extraScripture",
);
wire_names!(
    ResourcesLevel,
    "resources.json level",
    Top = "top",
    LanguageSets = "languageSets",
    Resources = "resources",
);
wire_names!(
    UserStatus,
    "status",
    Valid = "valid",
    Invalid = "invalid",
    Todo = "todo"
);
wire_names!(
    MarkerTerm,
    "marker terminator",
    Space = "space",
    Tab = "tab",
    Lf = "lf",
    Cr = "cr"
);
wire_names!(
    PresentHow,
    "presence",
    Added = "added",
    RestoredText = "restoredText",
    StartedEmpty = "startedEmpty",
);
wire_names!(
    SystemReason,
    "system reason",
    PinCoverageBackfill = "pinCoverageBackfill",
    CreationPins = "creationPins",
);

// ---------------------------------------------------------------------------------------
// Batches and small structs
// ---------------------------------------------------------------------------------------

impl Wire for BatchRef {
    fn to_json(&self) -> Json {
        Out::new().f("id", &self.id).f("n", &self.n()).done()
    }

    fn from_json(j: View<'_>, cx: Cx) -> R<Self> {
        let mut o = In::new(j, cx)?;
        let id = o.req("id")?;
        let n = o.req("n")?;
        o.finish()?;
        BatchRef::new(id, n)
            .ok_or_else(|| DecodeError::shape("a batch holds at least one change").within("n"))
    }
}

impl Wire for ChapterVerse {
    fn to_json(&self) -> Json {
        Out::new()
            .f("chapter", &self.chapter)
            .f("verse", &self.verse)
            .done()
    }

    fn from_json(j: View<'_>, cx: Cx) -> R<Self> {
        let mut o = In::new(j, cx)?;
        let value = ChapterVerse {
            chapter: o.req("chapter")?,
            verse: o.req("verse")?,
        };
        o.finish()?;
        Ok(value)
    }
}

impl Wire for MarkerWs {
    fn to_json(&self) -> Json {
        Out::new()
            .j("lead", Json::from(self.lead()))
            .f("term", &self.term())
            .done()
    }

    fn from_json(j: View<'_>, cx: Cx) -> R<Self> {
        let mut o = In::new(j, cx)?;
        let lead: String = o.req("lead")?;
        let term = o.req("term")?;
        o.finish()?;
        MarkerWs::new(&lead, term)
            .ok_or_else(|| DecodeError::shape("lead is one or more spaces and tabs").within("lead"))
    }
}

impl Wire for ResourceRef {
    fn to_json(&self) -> Json {
        Out::new()
            .f("repoPath", &self.repo_path)
            .f("sha", &self.sha)
            .done()
    }

    fn from_json(j: View<'_>, cx: Cx) -> R<Self> {
        let mut o = In::new(j, cx)?;
        let repo_path: Box<str> = o.req("repoPath")?;
        let sha = o.req("sha")?;
        o.finish()?;
        if repo_path.is_empty() {
            return Err(DecodeError::shape("a resource needs a repository path").within("repoPath"));
        }
        Ok(ResourceRef { repo_path, sha })
    }
}

/// A list that must be sorted with no repeats (`seen`, a release's `changes`).
fn sorted_unique<T: Wire + Ord>(j: View<'_>, cx: Cx) -> R<Vec<T>> {
    let items = Vec::<T>::from_json(j, cx)?;
    if items.windows(2).any(|pair| pair[0] >= pair[1]) {
        return Err(DecodeError::shape("not sorted, or an item is repeated"));
    }
    Ok(items)
}

// ---------------------------------------------------------------------------------------
// Keys
// ---------------------------------------------------------------------------------------

impl Wire for CheckTarget {
    fn to_json(&self) -> Json {
        match self {
            Self::Verse {
                book,
                chapter,
                verse,
            } => variant(
                "verse",
                Out::new()
                    .f("book", book)
                    .f("chapter", chapter)
                    .f("verse", verse)
                    .done(),
            ),
            Self::Frame { story, frame } => variant(
                "frame",
                Out::new().f("frame", frame).f("story", story).done(),
            ),
        }
    }

    fn from_json(j: View<'_>, cx: Cx) -> R<Self> {
        let (name, inner) = known_member(j, cx, "check target", &["verse", "frame"])?;
        let read = || -> R<Self> {
            let mut o = In::new(inner, cx)?;
            let value = match name {
                "verse" => Self::Verse {
                    book: o.req("book")?,
                    chapter: o.req("chapter")?,
                    verse: o.req("verse")?,
                },
                "frame" => Self::Frame {
                    story: o.req("story")?,
                    frame: o.req("frame")?,
                },
                other => return Err(unknown(cx, "check target", other)),
            };
            o.finish()?;
            Ok(value)
        };
        read().map_err(|e| e.within(name))
    }
}

impl Wire for CheckKey {
    fn to_json(&self) -> Json {
        Out::new()
            .f("checkId", &self.check_id)
            .f("occurrence", &self.occurrence)
            .f("target", &self.target)
            .f("tool", &self.tool)
            .done()
    }

    fn from_json(j: View<'_>, cx: Cx) -> R<Self> {
        let mut o = In::new(j, cx)?;
        let key = CheckKey {
            check_id: o.req("checkId")?,
            occurrence: o.req("occurrence")?,
            target: o.req("target")?,
            tool: o.req("tool")?,
        };
        o.finish()?;
        Ok(key)
    }
}

impl Wire for MetaField {
    fn to_json(&self) -> Json {
        match self {
            Self::Name => unit("name"),
            Self::Abbreviation => unit("abbreviation"),
            Self::Description => unit("description"),
            Self::Languages => unit("languages"),
            Self::Copyright => unit("copyright"),
            Self::Extra(path) => variant("extra", path.to_json()),
        }
    }

    fn from_json(j: View<'_>, cx: Cx) -> R<Self> {
        let (name, inner) = known_member(
            j,
            cx,
            "project detail",
            &[
                "name",
                "abbreviation",
                "description",
                "languages",
                "copyright",
                "extra",
            ],
        )?;
        let read = || -> R<Self> {
            Ok(match name {
                "name" => no_fields(inner, cx).map(|()| Self::Name)?,
                "abbreviation" => no_fields(inner, cx).map(|()| Self::Abbreviation)?,
                "description" => no_fields(inner, cx).map(|()| Self::Description)?,
                "languages" => no_fields(inner, cx).map(|()| Self::Languages)?,
                "copyright" => no_fields(inner, cx).map(|()| Self::Copyright)?,
                "extra" => Self::Extra(MetaPath::from_json(inner, cx)?),
                other => return Err(unknown(cx, "project detail", other)),
            })
        };
        read().map_err(|e| e.within(name))
    }
}

/// The fields of a record object that name its record, without `value`.
fn key_fields(key: &RecordKey) -> Result<(String, Json), ChangeError> {
    let fields = match key {
        RecordKey::BookPresence(book)
        | RecordKey::BookHeader(book)
        | RecordKey::BookLayout(book) => Out::new().f("book", book).done(),
        RecordKey::ChapterFront(book, chapter) => {
            Out::new().f("book", book).f("chapter", chapter).done()
        }
        RecordKey::Verse(book, chapter, verse) | RecordKey::Alignment(book, chapter, verse) => {
            Out::new()
                .f("book", book)
                .f("chapter", chapter)
                .f("verse", verse)
                .done()
        }
        RecordKey::Check(key) => Out::new().f("key", key).done(),
        RecordKey::Comment(id) => Out::new().f("id", id).done(),
        RecordKey::Pins(group) => Out::new().f("group", group).done(),
        RecordKey::ResourcesExtra(key) => Out::new()
            .f("level", &key.level())
            .f("name", &key.name().to_owned())
            .done(),
        RecordKey::Setting(key) => Out::new().f("key", key).done(),
        RecordKey::ProjectMeta(field) => Out::new().f("field", field).done(),
        RecordKey::Versification => Out::new().done(),
        RecordKey::DeviceProfile(device) => Out::new().f("device", device).done(),
        RecordKey::StoryBase(story) | RecordKey::StoryRef(story) => {
            Out::new().f("story", story).done()
        }
        RecordKey::Frame(story, frame) => Out::new().f("frame", frame).f("story", story).done(),
        RecordKey::CarriedFile(path) => Out::new().f("path", path).done(),
        RecordKey::BatchRelease(batch) => Out::new().f("batch", batch).done(),
        RecordKey::Unknown(key) => unknown_object(&key.kind, &key.key_json)?,
    };
    Ok((key.kind_name().to_owned(), fields))
}

/// The key object of an unknown record, checked to be canonical and of an unknown kind.
fn unknown_object(kind: &str, key_json: &str) -> Result<Json, ChangeError> {
    if KNOWN_KINDS.contains(&kind) {
        return Err(ChangeError::BadUnknownRecord(format!(
            "{kind:?} is a kind this app knows"
        )));
    }
    match json::parse(key_json.as_bytes()) {
        Ok(Json::Obj(map)) if !map.contains_key("value") => Ok(Json::Obj(map)),
        Ok(_) => Err(ChangeError::BadUnknownRecord(
            "the key is not an object without a value member".into(),
        )),
        Err(e) => Err(ChangeError::BadUnknownRecord(e.to_string())),
    }
}

/// The canonical JSON of a record key: `{"<kind>":{<key fields>}}`, the record object
/// without its `value`.
pub fn encode_key(key: &RecordKey) -> Result<Json, ChangeError> {
    let (kind, fields) = key_fields(key)?;
    Ok(variant(&kind, fields))
}

/// Reads a key written by [`encode_key`].
pub fn decode_key(j: &Json) -> Result<RecordKey, RuleViolation> {
    read_key(View::Owned(j)).map_err(DecodeError::into_violation)
}

fn read_key(j: View<'_>) -> R<RecordKey> {
    let cx = STRICT;
    let (kind, inner) = one_member(j)?;
    let read = || -> R<RecordKey> {
        if !KNOWN_KINDS.contains(&kind) {
            let Some(map) = inner.as_obj() else {
                return Err(expected("an object", inner));
            };
            if map.contains_key("value") {
                return Err(DecodeError::shape("a key has no value member"));
            }
            return Ok(RecordKey::Unknown(UnknownKey {
                kind: kind.into(),
                key_json: inner.to_json().to_canonical_string().into(),
            }));
        }
        let mut o = In::new(inner, cx)?;
        let key = match kind {
            "bookPresence" => RecordKey::BookPresence(o.req("book")?),
            "bookHeader" => RecordKey::BookHeader(o.req("book")?),
            "bookLayout" => RecordKey::BookLayout(o.req("book")?),
            "chapterFront" => RecordKey::ChapterFront(o.req("book")?, o.req("chapter")?),
            "verse" => RecordKey::Verse(o.req("book")?, o.req("chapter")?, o.req("verse")?),
            "alignment" => RecordKey::Alignment(o.req("book")?, o.req("chapter")?, o.req("verse")?),
            "check" => RecordKey::Check(o.req("key")?),
            "comment" => RecordKey::Comment(o.req("id")?),
            "pins" => RecordKey::Pins(o.req("group")?),
            "resourcesExtra" => RecordKey::ResourcesExtra(resources_key(&mut o)?),
            "setting" => RecordKey::Setting(o.req("key")?),
            "projectMeta" => RecordKey::ProjectMeta(o.req("field")?),
            "versification" => RecordKey::Versification,
            "deviceProfile" => RecordKey::DeviceProfile(o.req("device")?),
            "storyBase" => RecordKey::StoryBase(o.req("story")?),
            "frame" => RecordKey::Frame(o.req("story")?, o.req("frame")?),
            "storyRef" => RecordKey::StoryRef(o.req("story")?),
            "carriedFile" => RecordKey::CarriedFile(o.req("path")?),
            _ => RecordKey::BatchRelease(o.req("batch")?),
        };
        o.finish()?;
        Ok(key)
    };
    read().map_err(|e| e.within(kind))
}

// ---------------------------------------------------------------------------------------
// Values
// ---------------------------------------------------------------------------------------

impl Wire for Scope {
    fn to_json(&self) -> Json {
        match self {
            Self::Whole => unit("whole"),
            Self::Ranges(ranges) => variant(
                "ranges",
                Json::Arr(
                    ranges
                        .as_slice()
                        .iter()
                        .map(|r| Json::Str(r.to_string()))
                        .collect(),
                ),
            ),
        }
    }

    fn from_json(j: View<'_>, cx: Cx) -> R<Self> {
        let (name, inner) = known_member(j, cx, "scope", &["whole", "ranges"])?;
        let read = || -> R<Self> {
            match name {
                "whole" => no_fields(inner, cx).map(|()| Self::Whole),
                "ranges" => {
                    let texts = Vec::<String>::from_json(inner, cx)?;
                    let ranges = texts
                        .iter()
                        .map(|text| {
                            text.parse::<ScopeRange>()
                                .ok()
                                .filter(|range| range.to_string() == *text)
                                .ok_or_else(|| {
                                    DecodeError::shape(format!("{text:?} is not a scope range"))
                                })
                        })
                        .collect::<R<Vec<_>>>()?;
                    ScopeRanges::new(ranges)
                        .map(Self::Ranges)
                        .ok_or_else(|| DecodeError::shape("a passage set has at least one range"))
                }
                other => Err(unknown(cx, "scope", other)),
            }
        };
        read().map_err(|e| e.within(name))
    }
}

impl Wire for BookPresence {
    fn to_json(&self) -> Json {
        match self {
            Self::Present { scope, how } => {
                variant("present", Out::new().f("how", how).f("scope", scope).done())
            }
            Self::Removed { seen } => variant("removed", Out::new().f("seen", seen).done()),
        }
    }

    fn from_json(j: View<'_>, cx: Cx) -> R<Self> {
        let (name, inner) = known_member(j, cx, "book presence", &["present", "removed"])?;
        let read = || -> R<Self> {
            let mut o = In::new(inner, cx)?;
            let value = match name {
                "present" => Self::Present {
                    how: o.req("how")?,
                    scope: o.req("scope")?,
                },
                "removed" => Self::Removed {
                    seen: o.req_with("seen", |j| sorted_unique(j, cx))?,
                },
                other => return Err(unknown(cx, "book presence", other)),
            };
            o.finish()?;
            Ok(value)
        };
        read().map_err(|e| e.within(name))
    }
}

impl Wire for UsfmBytes {
    fn to_json(&self) -> Json {
        Json::Str(self.0.clone())
    }

    fn from_json(j: View<'_>, cx: Cx) -> R<Self> {
        String::from_json(j, cx).map(UsfmBytes)
    }
}

impl Wire for ChapterFront {
    fn to_json(&self) -> Json {
        match self {
            Self::Present(bytes) => variant("present", bytes.to_json()),
            Self::Absent => unit("absent"),
        }
    }

    fn from_json(j: View<'_>, cx: Cx) -> R<Self> {
        let (name, inner) = known_member(j, cx, "chapter front", &["present", "absent"])?;
        let read = || -> R<Self> {
            match name {
                "present" => UsfmBytes::from_json(inner, cx).map(Self::Present),
                "absent" => no_fields(inner, cx).map(|()| Self::Absent),
                other => Err(unknown(cx, "chapter front", other)),
            }
        };
        read().map_err(|e| e.within(name))
    }
}

impl Wire for VerseValue {
    fn to_json(&self) -> Json {
        match self {
            Self::Text {
                marker,
                words,
                layout,
                last,
                from,
            } => variant(
                "text",
                Out::new()
                    .o("from", from.as_ref())
                    .f("last", last)
                    .f("layout", layout)
                    .f("marker", marker)
                    .f("words", words)
                    .done(),
            ),
            Self::Covered { by } => variant("covered", Out::new().f("by", by).done()),
            Self::Absent { moved_to } => {
                variant("absent", Out::new().o("movedTo", moved_to.as_ref()).done())
            }
        }
    }

    fn from_json(j: View<'_>, cx: Cx) -> R<Self> {
        let (name, inner) = known_member(j, cx, "verse value", &["text", "covered", "absent"])?;
        let read = || -> R<Self> {
            let mut o = In::new(inner, cx)?;
            let value = match name {
                "text" => Self::Text {
                    from: o.opt("from")?,
                    last: o.req("last")?,
                    layout: o.req("layout")?,
                    marker: o.req("marker")?,
                    words: o.req("words")?,
                },
                "covered" => Self::Covered { by: o.req("by")? },
                "absent" => Self::Absent {
                    moved_to: o.opt("movedTo")?,
                },
                other => return Err(unknown(cx, "verse value", other)),
            };
            o.finish()?;
            Ok(value)
        };
        read().map_err(|e| e.within(name))
    }
}

impl Wire for LayoutItem {
    fn to_json(&self) -> Json {
        match self {
            Self::Chapter(chapter) => variant("chapter", Out::new().f("chapter", chapter).done()),
            Self::Verse(chapter, verse) => variant(
                "verse",
                Out::new().f("chapter", chapter).f("verse", verse).done(),
            ),
        }
    }

    fn from_json(j: View<'_>, cx: Cx) -> R<Self> {
        let (name, inner) = known_member(j, cx, "layout item", &["chapter", "verse"])?;
        let read = || -> R<Self> {
            let mut o = In::new(inner, cx)?;
            let value = match name {
                "chapter" => Self::Chapter(o.req("chapter")?),
                "verse" => Self::Verse(o.req("chapter")?, o.req("verse")?),
                other => return Err(unknown(cx, "layout item", other)),
            };
            o.finish()?;
            Ok(value)
        };
        read().map_err(|e| e.within(name))
    }
}

impl Wire for SourceWord {
    fn to_json(&self) -> Json {
        Out::new()
            .x("extra", &self.extra)
            .o("lemma", self.lemma.as_ref())
            .o("morph", self.morph.as_ref())
            .f("occurrence", &self.occurrence)
            .f("occurrences", &self.occurrences)
            .o("strong", self.strong.as_ref())
            .f("word", &self.word)
            .done()
    }

    fn from_json(j: View<'_>, cx: Cx) -> R<Self> {
        let mut o = In::new(j, cx)?;
        let value = Self {
            extra: o.extra("extra")?,
            lemma: o.opt("lemma")?,
            morph: o.opt("morph")?,
            occurrence: o.req("occurrence")?,
            occurrences: o.req("occurrences")?,
            strong: o.opt("strong")?,
            word: o.req("word")?,
        };
        o.finish()?;
        Ok(value)
    }
}

impl Wire for TargetWord {
    fn to_json(&self) -> Json {
        Out::new()
            .x("extra", &self.extra)
            .f("occurrence", &self.occurrence)
            .f("occurrences", &self.occurrences)
            .f("word", &self.word)
            .done()
    }

    fn from_json(j: View<'_>, cx: Cx) -> R<Self> {
        let mut o = In::new(j, cx)?;
        let value = Self {
            extra: o.extra("extra")?,
            occurrence: o.req("occurrence")?,
            occurrences: o.req("occurrences")?,
            word: o.req("word")?,
        };
        o.finish()?;
        Ok(value)
    }
}

impl Wire for AlignmentGroup {
    fn to_json(&self) -> Json {
        Out::new()
            .f("bottomWords", &self.bottom_words)
            .x("extra", &self.extra)
            .f("topWords", &self.top_words)
            .done()
    }

    fn from_json(j: View<'_>, cx: Cx) -> R<Self> {
        let mut o = In::new(j, cx)?;
        let value = Self {
            bottom_words: o.req("bottomWords")?,
            extra: o.extra("extra")?,
            top_words: o.req("topWords")?,
        };
        o.finish()?;
        Ok(value)
    }
}

impl Wire for AlignmentValue {
    fn to_json(&self) -> Json {
        Out::new()
            .f("alignments", &self.alignments)
            .f("dismissed", &self.dismissed)
            .f("done", &self.done)
            .x("extra", &self.extra)
            .f("markedInvalid", &self.marked_invalid)
            .o("source", self.source.as_ref())
            .f("sourceVersion", &self.source_version)
            .f("spanEnd", &self.span_end)
            .f("targetVerseMd5", &self.target_verse_md5)
            .f("wordBank", &self.word_bank)
            .done()
    }

    fn from_json(j: View<'_>, cx: Cx) -> R<Self> {
        let mut o = In::new(j, cx)?;
        let value = Self {
            alignments: o.req("alignments")?,
            dismissed: o.req("dismissed")?,
            done: o.req("done")?,
            extra: o.extra("extra")?,
            marked_invalid: o.req("markedInvalid")?,
            source: o.opt("source")?,
            source_version: o.req("sourceVersion")?,
            span_end: o.req("spanEnd")?,
            target_verse_md5: o.req("targetVerseMd5")?,
            word_bank: o.req("wordBank")?,
        };
        o.finish()?;
        Ok(value)
    }
}

impl Wire for Selection {
    fn to_json(&self) -> Json {
        Out::new()
            .x("extra", &self.extra)
            .f("occurrence", &self.occurrence)
            .f("occurrences", &self.occurrences)
            .f("text", &self.text)
            .done()
    }

    fn from_json(j: View<'_>, cx: Cx) -> R<Self> {
        let mut o = In::new(j, cx)?;
        let value = Self {
            extra: o.extra("extra")?,
            occurrence: o.req("occurrence")?,
            occurrences: o.req("occurrences")?,
            text: o.req("text")?,
        };
        o.finish()?;
        Ok(value)
    }
}

impl Wire for Judgement {
    fn to_json(&self) -> Json {
        Out::new()
            .f("nothingToSelect", &self.nothing_to_select)
            .o("selections", self.selections.as_ref())
            .o("status", self.status.as_ref())
            .f("verseTextMd5", &self.verse_text_md5)
            .done()
    }

    fn from_json(j: View<'_>, cx: Cx) -> R<Self> {
        let mut o = In::new(j, cx)?;
        let value = Self {
            nothing_to_select: o.req("nothingToSelect")?,
            selections: o.opt("selections")?,
            status: o.opt("status")?,
            verse_text_md5: o.req("verseTextMd5")?,
        };
        o.finish()?;
        Ok(value)
    }
}

impl Wire for DecisionRecord {
    fn to_json(&self) -> Json {
        Out::new()
            .f("bookmarked", &self.bookmarked)
            .o("category", self.category.as_ref())
            .o("comment", self.comment.as_ref())
            .f("context", &self.context)
            .f("dismissed", &self.dismissed)
            .x("extra", &self.extra)
            .f("importedInvalid", &self.imported_invalid)
            .f("judgement", &self.judgement)
            .f("resource", &self.resource)
            .done()
    }

    fn from_json(j: View<'_>, cx: Cx) -> R<Self> {
        let mut o = In::new(j, cx)?;
        let value = Self {
            bookmarked: o.req("bookmarked")?,
            category: o.opt("category")?,
            comment: o.opt("comment")?,
            context: o.req("context")?,
            dismissed: o.req("dismissed")?,
            extra: o.extra("extra")?,
            imported_invalid: o.req("importedInvalid")?,
            judgement: o.req("judgement")?,
            resource: o.req("resource")?,
        };
        o.finish()?;
        Ok(value)
    }
}

impl Wire for CheckValue {
    fn to_json(&self) -> Json {
        match self {
            Self::Decision(decision) => variant("decision", decision.to_json()),
            Self::MovedTo { to } => variant("movedTo", Out::new().f("to", to).done()),
        }
    }

    fn from_json(j: View<'_>, cx: Cx) -> R<Self> {
        let (name, inner) = known_member(j, cx, "check value", &["decision", "movedTo"])?;
        let read = || -> R<Self> {
            match name {
                "decision" => DecisionRecord::from_json(inner, cx).map(Self::Decision),
                "movedTo" => {
                    let mut o = In::new(inner, cx)?;
                    let to = o.req("to")?;
                    o.finish()?;
                    Ok(Self::MovedTo { to })
                }
                other => Err(unknown(cx, "check value", other)),
            }
        };
        read().map_err(|e| e.within(name))
    }
}

impl Wire for CommentTarget {
    fn to_json(&self) -> Json {
        match self {
            Self::Verse {
                book,
                chapter,
                verse,
            } => variant(
                "verse",
                Out::new()
                    .f("book", book)
                    .f("chapter", chapter)
                    .f("verse", verse)
                    .done(),
            ),
            Self::Check(key) => variant("check", key.to_json()),
            Self::Frame { story, frame } => variant(
                "frame",
                Out::new().f("frame", frame).f("story", story).done(),
            ),
        }
    }

    fn from_json(j: View<'_>, cx: Cx) -> R<Self> {
        let (name, inner) = known_member(j, cx, "comment target", &["verse", "check", "frame"])?;
        let read = || -> R<Self> {
            if name == "check" {
                return CheckKey::from_json(inner, cx).map(Self::Check);
            }
            let mut o = In::new(inner, cx)?;
            let value = match name {
                "verse" => Self::Verse {
                    book: o.req("book")?,
                    chapter: o.req("chapter")?,
                    verse: o.req("verse")?,
                },
                "frame" => Self::Frame {
                    frame: o.req("frame")?,
                    story: o.req("story")?,
                },
                other => return Err(unknown(cx, "comment target", other)),
            };
            o.finish()?;
            Ok(value)
        };
        read().map_err(|e| e.within(name))
    }
}

impl Wire for CommentValue {
    fn to_json(&self) -> Json {
        Out::new()
            .f("target", &self.target)
            .f("text", &self.text)
            .done()
    }

    fn from_json(j: View<'_>, cx: Cx) -> R<Self> {
        let mut o = In::new(j, cx)?;
        let value = Self {
            target: o.req("target")?,
            text: o.req("text")?,
        };
        o.finish()?;
        Ok(value)
    }
}

impl Wire for PinGroupValue {
    fn to_json(&self) -> Json {
        Json::Obj(
            self.0
                .iter()
                .map(|(slot, value)| (slot.as_str().to_owned(), value.0.clone()))
                .collect(),
        )
    }

    fn from_json(j: View<'_>, _: Cx) -> R<Self> {
        let map = j.as_obj().ok_or_else(|| expected("an object", j))?;
        map.iter()
            .map(|(slot, value)| {
                let name =
                    SlotName::parse(slot).map_err(|e| DecodeError::identity(e).within(slot))?;
                Ok((name, SlotValue(value.to_json())))
            })
            .collect::<R<_>>()
            .map(PinGroupValue)
    }
}

impl Wire for VersificationValue {
    fn to_json(&self) -> Json {
        Out::new()
            .f("bytes", &self.bytes)
            .o("name", self.name.as_ref())
            .done()
    }

    fn from_json(j: View<'_>, cx: Cx) -> R<Self> {
        let mut o = In::new(j, cx)?;
        let value = Self {
            bytes: o.req("bytes")?,
            name: o.opt("name")?,
        };
        o.finish()?;
        Ok(value)
    }
}

impl Wire for Door43Username {
    fn to_json(&self) -> Json {
        Json::Str(self.as_str().to_owned())
    }

    fn from_json(j: View<'_>, _: Cx) -> R<Self> {
        Door43Username::parse(string(j)?).map_err(|e| DecodeError::shape(e.to_string()))
    }
}

impl Wire for LegacyIdentity {
    fn to_json(&self) -> Json {
        match self {
            Self::Tc4Actor { actor } => variant("tc4Actor", Out::new().f("actor", actor).done()),
            Self::Tc3User { user } => variant("tc3User", Out::new().f("user", user).done()),
        }
    }

    fn from_json(j: View<'_>, cx: Cx) -> R<Self> {
        let (name, inner) = known_member(j, cx, "legacy identity", &["tc4Actor", "tc3User"])?;
        let read = || -> R<Self> {
            let mut o = In::new(inner, cx)?;
            let value = match name {
                "tc4Actor" => Self::Tc4Actor {
                    actor: o.req("actor")?,
                },
                "tc3User" => Self::Tc3User {
                    user: o.req("user")?,
                },
                other => return Err(unknown(cx, "legacy identity", other)),
            };
            o.finish()?;
            Ok(value)
        };
        read().map_err(|e| e.within(name))
    }
}

impl Wire for DeviceProfileValue {
    fn to_json(&self) -> Json {
        Out::new()
            .o("displayName", self.display_name.as_ref())
            .o("door43Username", self.door43_username.as_ref())
            .o("legacy", self.legacy.as_ref())
            .done()
    }

    fn from_json(j: View<'_>, cx: Cx) -> R<Self> {
        let mut o = In::new(j, cx)?;
        let value = Self {
            display_name: o.opt("displayName")?,
            door43_username: o.opt("door43Username")?,
            legacy: o.opt("legacy")?,
        };
        o.finish()?;
        Ok(value)
    }
}

impl Wire for CarriedValue {
    fn to_json(&self) -> Json {
        match self {
            Self::Present { oid, size, md5 } => variant(
                "present",
                Out::new()
                    .f("md5", md5)
                    .f("oid", oid)
                    .f("size", size)
                    .done(),
            ),
            Self::Removed => unit("removed"),
        }
    }

    fn from_json(j: View<'_>, cx: Cx) -> R<Self> {
        let (name, inner) = known_member(j, cx, "carried file value", &["present", "removed"])?;
        let read = || -> R<Self> {
            match name {
                "present" => {
                    let mut o = In::new(inner, cx)?;
                    let value = Self::Present {
                        md5: o.req("md5")?,
                        oid: o.req("oid")?,
                        size: o.req("size")?,
                    };
                    o.finish()?;
                    Ok(value)
                }
                "removed" => no_fields(inner, cx).map(|()| Self::Removed),
                other => Err(unknown(cx, "carried file value", other)),
            }
        };
        read().map_err(|e| e.within(name))
    }
}

impl Wire for BatchReleaseValue {
    fn to_json(&self) -> Json {
        Out::new().f("changes", &self.changes).done()
    }

    fn from_json(j: View<'_>, cx: Cx) -> R<Self> {
        let mut o = In::new(j, cx)?;
        let changes = o.req_with("changes", |j| sorted_unique(j, cx))?;
        o.finish()?;
        Ok(Self { changes })
    }
}

// ---------------------------------------------------------------------------------------
// Records
// ---------------------------------------------------------------------------------------

fn encode_record(record: &Record) -> Result<Json, ChangeError> {
    if let Record::Unknown(unknown) = record {
        let Json::Obj(mut map) = unknown_object(&unknown.kind, &unknown.key_json)? else {
            return Err(ChangeError::BadUnknownRecord(
                "the key is not an object".into(),
            ));
        };
        if let Some(value) = &unknown.value_json {
            let value = json::parse(value.as_bytes())
                .map_err(|e| ChangeError::BadUnknownRecord(e.to_string()))?;
            map.insert("value".to_owned(), value);
        }
        return Ok(variant(&unknown.kind, Json::Obj(map)));
    }
    let (kind, fields) = key_fields(&record.key())?;
    let value = match record {
        Record::BookPresence { value, .. } => Some(value.to_json()),
        Record::BookHeader { value, .. } => Some(value.to_json()),
        Record::ChapterFront { value, .. } => Some(value.to_json()),
        Record::Verse { value, .. } => Some(value.to_json()),
        Record::BookLayout { value, .. } => value.as_ref().map(Wire::to_json),
        Record::Alignment { value, .. } => Some(value.to_json()),
        Record::Check { value, .. } => Some(value.to_json()),
        Record::Comment { value, .. } => Some(value.to_json()),
        Record::Pins { value, .. } => Some(value.to_json()),
        Record::Setting { value, .. }
        | Record::ProjectMeta { value, .. }
        | Record::ResourcesExtra { value, .. } => value.clone(),
        Record::Versification { value } => Some(value.to_json()),
        Record::DeviceProfile { value, .. } => Some(value.to_json()),
        Record::StoryBase { value, .. } | Record::Frame { value, .. } => Some(value.to_json()),
        Record::StoryRef { value, .. } => value.as_ref().map(Wire::to_json),
        Record::CarriedFile { value, .. } => Some(value.to_json()),
        Record::BatchRelease { value, .. } => Some(value.to_json()),
        Record::Unknown(_) => None,
    };
    let Json::Obj(mut map) = fields else {
        return Err(ChangeError::BadUnknownRecord(
            "a key is not an object".into(),
        ));
    };
    if let Some(value) = value {
        map.insert("value".to_owned(), value);
    }
    Ok(variant(&kind, Json::Obj(map)))
}

fn decode_record(j: View<'_>, cx: Cx) -> R<Record> {
    let (kind, inner) = one_member(j)?;
    if !cx.lenient && !KNOWN_KINDS.contains(&kind) {
        return Err(DecodeError::shape(format!("unknown record kind {kind:?}")));
    }
    let read = || -> R<Record> {
        if !KNOWN_KINDS.contains(&kind) {
            let Some(map) = inner.as_obj() else {
                return Err(expected("an object", inner));
            };
            let mut key = map.to_map();
            let value = key.remove("value");
            return Ok(Record::Unknown(UnknownRecord {
                kind: kind.into(),
                key_json: Json::Obj(key).to_canonical_string().into(),
                value_json: value.map(|v| v.to_canonical_string().into()),
            }));
        }
        let mut o = In::new(inner, cx)?;
        let record = match kind {
            "bookPresence" => Record::BookPresence {
                book: o.req("book")?,
                value: o.req("value")?,
            },
            "bookHeader" => Record::BookHeader {
                book: o.req("book")?,
                value: o.req("value")?,
            },
            "chapterFront" => Record::ChapterFront {
                book: o.req("book")?,
                chapter: o.req("chapter")?,
                value: o.req("value")?,
            },
            "verse" => Record::Verse {
                book: o.req("book")?,
                chapter: o.req("chapter")?,
                value: o.req("value")?,
                verse: o.req("verse")?,
            },
            "bookLayout" => Record::BookLayout {
                book: o.req("book")?,
                value: o.opt("value")?,
            },
            "alignment" => Record::Alignment {
                book: o.req("book")?,
                chapter: o.req("chapter")?,
                value: o.req("value")?,
                verse: o.req("verse")?,
            },
            "check" => Record::Check {
                key: o.req("key")?,
                value: o.req("value")?,
            },
            "comment" => Record::Comment {
                id: o.req("id")?,
                value: o.req("value")?,
            },
            "pins" => Record::Pins {
                group: o.req("group")?,
                value: o.req("value")?,
            },
            "resourcesExtra" => Record::ResourcesExtra {
                key: resources_key(&mut o)?,
                value: o.opt("value")?,
            },
            "setting" => Record::Setting {
                key: o.req("key")?,
                value: o.opt("value")?,
            },
            "projectMeta" => Record::ProjectMeta {
                field: o.req("field")?,
                value: o.opt("value")?,
            },
            "versification" => Record::Versification {
                value: o.req("value")?,
            },
            "deviceProfile" => Record::DeviceProfile {
                device: o.req("device")?,
                value: o.req("value")?,
            },
            "storyBase" => Record::StoryBase {
                story: o.req("story")?,
                value: o.req("value")?,
            },
            "frame" => Record::Frame {
                frame: o.req("frame")?,
                story: o.req("story")?,
                value: o.req("value")?,
            },
            "storyRef" => Record::StoryRef {
                story: o.req("story")?,
                value: o.opt("value")?,
            },
            "carriedFile" => Record::CarriedFile {
                path: o.req("path")?,
                value: o.req("value")?,
            },
            _ => Record::BatchRelease {
                batch: o.req("batch")?,
                value: o.req("value")?,
            },
        };
        o.finish()?;
        Ok(record)
    };
    read().map_err(|e| e.within(kind))
}

/// Reads a record object leniently (unknown fields ignored): for values the fold builds by
/// merging units of stored values, which every reader accepted already.
pub(crate) fn decode_record_lenient(j: &Json) -> Option<Record> {
    decode_record(View::Owned(j), Cx { lenient: true }).ok()
}

/// The wire form of a comment target (§2.8).
pub(crate) fn comment_target_json(target: &CommentTarget) -> Option<Json> {
    Some(target.to_json())
}

/// A comment target in its wire form (§2.8), as a draft of the comment box keeps it.
pub fn encode_comment_target(target: &CommentTarget) -> Json {
    target.to_json()
}

/// Reads a comment target written by [`encode_comment_target`].
pub fn decode_comment_target(j: &Json) -> Option<CommentTarget> {
    CommentTarget::from_json(View::Owned(j), STRICT).ok()
}

/// The wire form of a check key (§2.8).
pub(crate) fn check_key_json(key: &CheckKey) -> Option<Json> {
    Some(key.to_json())
}

/// Reads a check key from its wire form.
pub(crate) fn check_key_from_json(j: &Json) -> Option<CheckKey> {
    CheckKey::from_json(View::Owned(j), STRICT).ok()
}

/// A draft's unsaved links (§5.14): `{"alignments", "done", "wordBank"}` in the wire form of
/// §2.8, canonical.
pub(crate) fn alignment_edit_json(edit: &crate::command::AlignmentEdit) -> String {
    Out::new()
        .f("alignments", &edit.alignments)
        .f("done", &edit.done)
        .f("wordBank", &edit.word_bank)
        .done()
        .to_canonical_string()
}

/// Reads what [`alignment_edit_json`] wrote.
pub(crate) fn alignment_edit_from_json(text: &str) -> Option<crate::command::AlignmentEdit> {
    let j = json::parse(text.as_bytes()).ok()?;
    let mut o = In::new(View::Owned(&j), STRICT).ok()?;
    let edit = crate::command::AlignmentEdit {
        alignments: o.req("alignments").ok()?,
        done: o.req("done").ok()?,
        word_bank: o.req("wordBank").ok()?,
    };
    o.finish().ok()?;
    Some(edit)
}

/// The canonical JSON of a record (key and value), as a change stores it.
pub fn encode_record_json(record: &Record) -> Result<Json, ChangeError> {
    encode_record(record)
}

// ---------------------------------------------------------------------------------------
// Causes
// ---------------------------------------------------------------------------------------

impl Wire for ExternalSource {
    fn to_json(&self) -> Json {
        match self {
            Self::LocalFile { path, seen_md5 } => variant(
                "localFile",
                Out::new().f("path", path).f("seenMd5", seen_md5).done(),
            ),
            Self::Door43 {
                commit,
                path,
                author,
            } => variant(
                "door43",
                Out::new()
                    .f("author", author)
                    .f("commit", commit)
                    .f("path", path)
                    .done(),
            ),
            Self::ExchangeFile { path, exporter } => variant(
                "exchangeFile",
                Out::new()
                    .o("exporter", exporter.as_ref())
                    .f("path", path)
                    .done(),
            ),
        }
    }

    fn from_json(j: View<'_>, cx: Cx) -> R<Self> {
        let (name, inner) = known_member(
            j,
            cx,
            "outside source",
            &["localFile", "door43", "exchangeFile"],
        )?;
        let read = || -> R<Self> {
            let mut o = In::new(inner, cx)?;
            let value = match name {
                "localFile" => Self::LocalFile {
                    path: o.req_with("path", |j| {
                        RepoPath::parse_source(string(j)?).map_err(DecodeError::identity)
                    })?,
                    seen_md5: o.req("seenMd5")?,
                },
                "door43" => Self::Door43 {
                    author: o.req("author")?,
                    commit: o.req("commit")?,
                    path: o.req("path")?,
                },
                "exchangeFile" => Self::ExchangeFile {
                    exporter: o.opt("exporter")?,
                    path: o.req("path")?,
                },
                other => return Err(unknown(cx, "outside source", other)),
            };
            o.finish()?;
            Ok(value)
        };
        read().map_err(|e| e.within(name))
    }
}

impl Wire for ImportSource {
    fn to_json(&self) -> Json {
        match self {
            Self::Tc4 { actor, ts, op } => variant(
                "tc4",
                Out::new().f("actor", actor).f("op", op).f("ts", ts).done(),
            ),
            Self::Tc4Reconcile => unit("tc4Reconcile"),
            Self::Tc3 { user } => variant("tc3", Out::new().o("user", user.as_ref()).done()),
            Self::Usfm { file_name } => variant("usfm", Out::new().f("fileName", file_name).done()),
            Self::Burrito { file_name } => {
                variant("burrito", Out::new().f("fileName", file_name).done())
            }
        }
    }

    fn from_json(j: View<'_>, cx: Cx) -> R<Self> {
        let (name, inner) = known_member(
            j,
            cx,
            "import source",
            &["tc4", "tc4Reconcile", "tc3", "usfm", "burrito"],
        )?;
        let read = || -> R<Self> {
            let mut o = In::new(inner, cx)?;
            let value = match name {
                "tc4" => Self::Tc4 {
                    actor: o.req("actor")?,
                    op: o.req("op")?,
                    ts: o.req("ts")?,
                },
                "tc4Reconcile" => Self::Tc4Reconcile,
                "tc3" => Self::Tc3 {
                    user: o.opt("user")?,
                },
                "usfm" => Self::Usfm {
                    file_name: o.req("fileName")?,
                },
                "burrito" => Self::Burrito {
                    file_name: o.req("fileName")?,
                },
                other => return Err(unknown(cx, "import source", other)),
            };
            o.finish()?;
            Ok(value)
        };
        read().map_err(|e| e.within(name))
    }
}

fn encode_cause(cause: &Cause) -> Result<Json, ChangeError> {
    Ok(match cause {
        Cause::Edit => unit("edit"),
        Cause::Resolve => unit("resolve"),
        Cause::Restore { from } => variant("restore", Out::new().f("from", from).done()),
        Cause::CarryOver { group } => variant("carryOver", Out::new().f("group", group).done()),
        Cause::External {
            source,
            unknown_base,
        } => variant(
            "external",
            Out::new()
                .f("source", source)
                .f("unknownBase", unknown_base)
                .done(),
        ),
        Cause::Import(source) => variant("import", source.to_json()),
        Cause::Seed => unit("seed"),
        Cause::StartOver => unit("startOver"),
        Cause::System(reason) => variant("system", reason.to_json()),
        Cause::Newer { .. } => return Err(ChangeError::NewerCause),
    })
}

const CAUSES: &[&str] = &[
    "edit",
    "resolve",
    "restore",
    "carryOver",
    "external",
    "import",
    "seed",
    "startOver",
    "system",
];

fn decode_cause(j: View<'_>, cx: Cx) -> R<Cause> {
    let (name, inner) = one_member(j)?;
    if !CAUSES.contains(&name) {
        return if cx.lenient {
            Ok(Cause::Newer { name: name.into() })
        } else {
            Err(unknown(cx, "cause", name))
        };
    }
    let read = || -> R<Cause> {
        Ok(match name {
            "edit" => no_fields(inner, cx).map(|()| Cause::Edit)?,
            "resolve" => no_fields(inner, cx).map(|()| Cause::Resolve)?,
            "restore" => {
                let mut o = In::new(inner, cx)?;
                let from = o.req("from")?;
                o.finish()?;
                Cause::Restore { from }
            }
            "carryOver" => {
                let mut o = In::new(inner, cx)?;
                let group = o.req("group")?;
                o.finish()?;
                Cause::CarryOver { group }
            }
            "external" => {
                let mut o = In::new(inner, cx)?;
                let source = o.req("source")?;
                let unknown_base = o.req("unknownBase")?;
                o.finish()?;
                Cause::External {
                    source,
                    unknown_base,
                }
            }
            "import" => ImportSource::from_json(inner, cx).map(Cause::Import)?,
            "seed" => no_fields(inner, cx).map(|()| Cause::Seed)?,
            "startOver" => no_fields(inner, cx).map(|()| Cause::StartOver)?,
            "system" => SystemReason::from_json(inner, cx).map(Cause::System)?,
            other => return Err(unknown(cx, "cause", other)),
        })
    };
    match read() {
        // A cause this app cannot hold, from a newer minor, still lets the change fold.
        Err(e) if e.is_newer() => Ok(Cause::Newer { name: name.into() }),
        other => other.map_err(|e| e.within(name)),
    }
}

// ---------------------------------------------------------------------------------------
// Changes
// ---------------------------------------------------------------------------------------

/// The canonical bytes of a change body.
pub fn encode_body(body: &ChangeBody) -> Result<Vec<u8>, ChangeError> {
    let json = Out::new()
        .j("at", Json::from(body.at.0))
        .f("batch", &body.batch)
        .j("cause", encode_cause(&body.cause)?)
        .f("device", &body.device)
        .f("parents", &body.parents)
        .j("record", encode_record(&body.record)?)
        .j("seq", Json::from(body.seq.0))
        .f("v", &body.v)
        .done();
    Ok(json.to_canonical())
}

/// Reads a change body from parsed canonical JSON (rules V1 and V2).
pub fn decode_body(j: &Json) -> Result<ChangeBody, RuleViolation> {
    read_body(View::Owned(j)).map_err(DecodeError::into_violation)
}

/// [`decode_body`] of a parsed text.
pub(crate) fn decode_body_view(j: View<'_>) -> Result<ChangeBody, RuleViolation> {
    read_body(j).map_err(DecodeError::into_violation)
}

fn read_body(j: View<'_>) -> R<ChangeBody> {
    let v = match j.as_obj().and_then(|map| map.get("v")) {
        Some(v) => u16::from_json(v, STRICT).map_err(|e| e.within("v"))?,
        None if j.as_obj().is_none() => return Err(expected("an object", j)),
        None => return Err(DecodeError::shape("missing v")),
    };
    let cx = Cx {
        lenient: v > FORMAT_MINOR,
    };
    let mut o = In::new(j, cx)?;
    let body = ChangeBody {
        at: WallTime(o.req("at")?),
        batch: o.req("batch")?,
        cause: o.req_with("cause", |j| decode_cause(j, cx))?,
        device: o.req("device")?,
        parents: o.req("parents")?,
        record: o.req_with("record", |j| decode_record(j, cx))?,
        seq: DeviceSeq(o.req("seq")?),
        v: o.req("v")?,
    };
    o.finish()?;
    Ok(body)
}

/// The record of a change whose canonical JSON passed the per-change rules when it was
/// read, read as [`read_body`] reads it.
pub(crate) fn decode_stored_record(j: View<'_>) -> Option<Record> {
    let map = j.as_obj()?;
    let v = u16::from_json(map.get("v")?, STRICT).ok()?;
    let cx = Cx {
        lenient: v > FORMAT_MINOR,
    };
    decode_record(map.get("record")?, cx).ok()
}

/// The batch, device and seq of a change that does not decode, when those fields still
/// read, so the fold can hold its batch.
pub(crate) fn best_effort_origin(
    j: View<'_>,
) -> (Option<BatchRef>, Option<DeviceId>, Option<DeviceSeq>) {
    let lenient = Cx { lenient: true };
    let Some(map) = j.as_obj() else {
        return (None, None, None);
    };
    let batch = map
        .get("batch")
        .and_then(|b| BatchRef::from_json(b, lenient).ok());
    let device = map
        .get("device")
        .and_then(|d| DeviceId::from_json(d, lenient).ok());
    let seq = map
        .get("seq")
        .and_then(|s| u64::from_json(s, lenient).ok())
        .map(DeviceSeq);
    (batch, device, seq)
}

// ---------------------------------------------------------------------------------------
// Headers (packs and pending files share the object helpers)
// ---------------------------------------------------------------------------------------

/// Typed access for the header codecs: they read leniently, because a newer minor may add
/// header fields that older readers ignore (§12.2).
pub(crate) struct HeaderReader<'a>(In<'a>);

impl<'a> HeaderReader<'a> {
    pub(crate) fn new(j: &'a Json) -> Result<Self, String> {
        In::new(View::Owned(j), Cx { lenient: true })
            .map(Self)
            .map_err(|e| e.into_violation().to_string())
    }

    pub(crate) fn req<T: Wire>(&mut self, key: &'static str) -> Result<T, String> {
        self.0.req(key).map_err(|e| e.into_violation().to_string())
    }

    /// A path as the file of an outside edit: any [`RepoPath`], or `metadata.json`
    /// ([`RepoPath::parse_source`]).
    pub(crate) fn req_source_path(&mut self, key: &'static str) -> Result<RepoPath, String> {
        self.0
            .req_with(key, |j| {
                RepoPath::parse_source(string(j)?).map_err(DecodeError::identity)
            })
            .map_err(|e| e.into_violation().to_string())
    }
}

/// Builds header objects.
pub(crate) fn header_object(fields: Vec<(&str, Json)>) -> Json {
    Json::Obj(fields.into_iter().map(|(k, v)| (k.to_owned(), v)).collect())
}

pub(crate) fn to_json<T: Wire>(value: &T) -> Json {
    value.to_json()
}
