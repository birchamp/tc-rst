//! The typed model of `metadata.json`.
//!
//! Every struct keeps the keys it does not model in `extra`, and every optional field
//! refuses an explicit `null` instead of reading it as absent, so serializing a model read
//! from a document gives back an equal document.

use crate::error::Error;
use crate::ingredient::{FileFacts, Md5, MimeType};
use crate::json::{StrictJsonError, parse_strict, strip_bom, to_canonical_json};
use crate::open_enum::open_enum;
use crate::path::IngredientPath;
use crate::scope::Scope;
use crate::time::Timestamp;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde_json::{Map, Value};
use std::collections::BTreeMap;
use std::collections::btree_map::Entry;

/// Keys a struct does not model, with their values as read.
///
/// A key the owning struct models (`size` in an [`Ingredient`]'s map) must not be added
/// here: [`Metadata::to_value`] and [`Metadata::to_canonical_json`] refuse such a model
/// with [`Error::ShadowedField`] rather than let one value replace the other.
pub type Extra = BTreeMap<String, Value>;

/// Reads a present optional field. Absent fields become `None` through `#[serde(default)]`;
/// a present `null` is refused, because writing the model back would drop it.
fn present<'de, D, T>(deserializer: D) -> Result<Option<T>, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de>,
{
    T::deserialize(deserializer).map(Some)
}

/// The root of `metadata.json`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Metadata {
    pub format: Format,
    pub meta: Meta,
    pub id_authorities: BTreeMap<String, IdAuthority>,
    pub identification: Identification,
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    pub languages: Option<Vec<Language>>,
    #[serde(rename = "type")]
    pub burrito_type: BurritoType,
    pub confidential: bool,
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    pub localized_names: Option<BTreeMap<String, LocalizedName>>,
    pub ingredients: BTreeMap<IngredientPath, Ingredient>,
    pub copyright: Copyright,
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    pub relationships: Option<Vec<Relationship>>,
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    pub agencies: Option<Vec<Agency>>,
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    pub target_areas: Option<Vec<TargetArea>>,
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    pub progress: Option<Progress>,
    #[serde(flatten)]
    pub extra: Extra,
}

/// The two kinds of project tC4 works on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ProjectKind {
    /// `scripture` / `textTranslation`
    BibleText,
    /// `gloss` / `textStories`
    OpenBibleStories,
}

/// What a new project's metadata starts from. See [`Metadata::new`].
#[derive(Debug, Clone, PartialEq)]
pub struct NewProject {
    /// The project's identifier under the `local` authority, such as `ejemplo_tj`.
    pub project_id: String,
    pub name: LocalizedText,
    pub language: Language,
    /// The language the metadata's own labels default to, such as `en`.
    pub default_locale: String,
    pub flavor_type: FlavorType,
    pub copyright: Copyright,
    pub generator: Generator,
    pub created: Timestamp,
}

/// The authority label tC4 uses for a project's own identifier.
pub const LOCAL_AUTHORITY: &str = "local";

impl Metadata {
    /// Metadata for a new source burrito with no ingredients yet. The schema needs at least
    /// one ingredient, so add files with [`Metadata::record_ingredient`] before writing.
    pub fn new(project: NewProject) -> Self {
        let local = IdAuthority {
            id: Some("http://_local_".to_owned()),
            name: Some(LocalizedText::single("en", "Local Project")),
            extra: Extra::new(),
        };
        let revision = Revision {
            revision: "1".to_owned(),
            timestamp: project.created.clone(),
            extra: Extra::new(),
        };
        Self {
            format: Format::ScriptureBurrito,
            meta: Meta {
                version: SpecVersion::V1_0_0,
                category: Category::Source,
                generator: Some(project.generator),
                default_locale: project.default_locale,
                date_created: project.created,
                normalization: None,
                comments: None,
                extra: Extra::new(),
            },
            id_authorities: BTreeMap::from([(LOCAL_AUTHORITY.to_owned(), local)]),
            identification: Identification {
                name: project.name,
                description: None,
                abbreviation: None,
                primary: BTreeMap::from([(
                    LOCAL_AUTHORITY.to_owned(),
                    BTreeMap::from([(project.project_id, revision)]),
                )]),
                extra: Extra::new(),
            },
            languages: Some(vec![project.language]),
            burrito_type: BurritoType {
                flavor_type: project.flavor_type,
                extra: Extra::new(),
            },
            confidential: false,
            localized_names: None,
            ingredients: BTreeMap::new(),
            copyright: project.copyright,
            relationships: None,
            agencies: None,
            target_areas: None,
            progress: None,
            extra: Extra::new(),
        }
    }

    /// Reads `metadata.json` bytes. A leading byte order mark is skipped, as RFC 8259
    /// allows; the canonical writer never writes one. A document with a key written twice,
    /// or with a number the writer would change, is refused, because writing it back would
    /// lose a value.
    pub fn from_slice(bytes: &[u8]) -> Result<Self, Error> {
        let (_, bytes) = strip_bom(bytes);
        Self::from_value(&parse_strict(bytes)?)
    }

    pub fn from_value(value: &Value) -> Result<Self, Error> {
        Self::deserialize(value).map_err(Error::Model)
    }

    /// The model as JSON. A key in an `extra` map that its struct also models is refused
    /// with [`Error::ShadowedField`] instead of overriding the typed field.
    pub fn to_value(&self) -> Result<Value, Error> {
        // Serializing to text keeps both copies of a shadowed key, where serializing to a
        // `Value` would keep the last one without a trace; the strict reader then finds it.
        let text = serde_json::to_vec(self).map_err(Error::Model)?;
        parse_strict(&text).map_err(|error| match error {
            StrictJsonError::DuplicateKey { pointer } => Error::ShadowedField { pointer },
            other => other.into(),
        })
    }

    /// The bytes to store as `metadata.json`. The same model always gives the same bytes.
    pub fn to_canonical_json(&self) -> Result<Vec<u8>, Error> {
        Ok(to_canonical_json(&self.to_value()?))
    }

    pub fn kind(&self) -> Option<ProjectKind> {
        let flavor_type = &self.burrito_type.flavor_type;
        match (&flavor_type.name, &flavor_type.flavor) {
            (FlavorTypeName::Scripture, Flavor::TextTranslation(_)) => Some(ProjectKind::BibleText),
            (FlavorTypeName::Gloss, Flavor::TextStories(_)) => Some(ProjectKind::OpenBibleStories),
            _ => None,
        }
    }

    pub fn current_scope(&self) -> Option<&Scope> {
        self.burrito_type.flavor_type.current_scope.as_ref()
    }

    /// Records the exact bytes about to be written at `path`. A new ingredient gets md5,
    /// size and mimeType from the bytes; an existing one keeps its mimeType, scope, role and
    /// unknown fields and gets a fresh md5 and size. Call it with the final bytes, after any
    /// normalization, so the listed checksum matches the file on disk.
    pub fn record_ingredient(&mut self, path: IngredientPath, bytes: &[u8]) -> &mut Ingredient {
        match self.ingredients.entry(path) {
            Entry::Occupied(entry) => {
                let ingredient = entry.into_mut();
                ingredient.refresh(FileFacts::of(bytes));
                ingredient
            }
            Entry::Vacant(entry) => {
                let ingredient = Ingredient::from_bytes(entry.key(), bytes);
                entry.insert(ingredient)
            }
        }
    }

    /// The project's own identifier. tC4 writes exactly one; SB 1.0 allows one authority
    /// with several identifiers, which leaves the project's own one ambiguous.
    pub fn primary(&self) -> Result<PrimaryId<'_>, PrimaryIdError> {
        let mut found = self
            .identification
            .primary
            .iter()
            .flat_map(|(authority, ids)| {
                ids.iter().map(move |(id, revision)| PrimaryId {
                    authority,
                    id,
                    revision,
                })
            });
        match (found.next(), found.next()) {
            (Some(primary), None) => Ok(primary),
            (None, _) => Err(PrimaryIdError::Missing),
            (Some(_), Some(_)) => Err(PrimaryIdError::Ambiguous),
        }
    }
}

/// Why [`Metadata::primary`] found no single identifier.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, thiserror::Error)]
pub enum PrimaryIdError {
    #[error("identification.primary lists no identifier")]
    Missing,
    #[error("identification.primary lists more than one identifier")]
    Ambiguous,
}

/// The project's single primary identifier, from `identification.primary`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PrimaryId<'a> {
    pub authority: &'a str,
    pub id: &'a str,
    pub revision: &'a Revision,
}

/// `format`: always `"scripture burrito"`. Anything else is not a burrito.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Format {
    #[serde(rename = "scripture burrito")]
    ScriptureBurrito,
}

open_enum! {
    /// `meta.version`.
    pub enum SpecVersion {
        V1_0_0 => "1.0.0",
    }
}

open_enum! {
    /// `meta.category`. tC4 writes only `source`.
    pub enum Category {
        Source => "source",
        Derived => "derived",
        Template => "template",
    }
}

open_enum! {
    /// `meta.normalization`. Declaring a form does not make the content take it; the code
    /// that writes ingredients must normalize.
    pub enum Normalization {
        Nfc => "NFC",
        Nfd => "NFD",
        Nfkc => "NFKC",
        Nfkd => "NFKD",
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Meta {
    pub version: SpecVersion,
    pub category: Category,
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    pub generator: Option<Generator>,
    pub default_locale: String,
    pub date_created: Timestamp,
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    pub normalization: Option<Normalization>,
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    pub comments: Option<Vec<String>>,
    #[serde(flatten)]
    pub extra: Extra,
}

/// `meta.generator`: the program and person that wrote the metadata.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Generator {
    pub software_name: String,
    pub software_version: String,
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    pub user_id: Option<String>,
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    pub user_name: Option<String>,
    #[serde(flatten)]
    pub extra: Extra,
}

/// Text in one or more languages, keyed by BCP 47 tag. The schema refuses empty text and
/// text with leading or trailing whitespace.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct LocalizedText(pub BTreeMap<String, String>);

impl LocalizedText {
    pub fn single(tag: impl Into<String>, text: impl Into<String>) -> Self {
        Self(BTreeMap::from([(tag.into(), text.into())]))
    }

    pub fn get(&self, tag: &str) -> Option<&str> {
        self.0.get(tag).map(String::as_str)
    }
}

/// One entry of `idAuthorities`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct IdAuthority {
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    pub id: Option<String>,
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    pub name: Option<LocalizedText>,
    #[serde(flatten)]
    pub extra: Extra,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Identification {
    pub name: LocalizedText,
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    pub description: Option<LocalizedText>,
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    pub abbreviation: Option<LocalizedText>,
    /// Authority label, then identifier, then revision. The schema allows one authority;
    /// [`Metadata::primary`] reads the usual single entry.
    pub primary: BTreeMap<String, BTreeMap<String, Revision>>,
    #[serde(flatten)]
    pub extra: Extra,
}

/// The revision and timestamp of an identifier in `identification.primary`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Revision {
    pub revision: String,
    pub timestamp: Timestamp,
    #[serde(flatten)]
    pub extra: Extra,
}

open_enum! {
    /// `languages[].scriptDirection`.
    pub enum ScriptDirection {
        Ltr => "ltr",
        Rtl => "rtl",
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Language {
    pub tag: String,
    pub name: LocalizedText,
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    pub script_direction: Option<ScriptDirection>,
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    pub rod: Option<String>,
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    pub numbering_system: Option<String>,
    #[serde(flatten)]
    pub extra: Extra,
}

/// The `type` object.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BurritoType {
    pub flavor_type: FlavorType,
    #[serde(flatten)]
    pub extra: Extra,
}

open_enum! {
    /// `type.flavorType.name`.
    pub enum FlavorTypeName {
        Scripture => "scripture",
        Gloss => "gloss",
        Parascriptural => "parascriptural",
        Peripheral => "peripheral",
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FlavorType {
    pub name: FlavorTypeName,
    pub flavor: Flavor,
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    pub current_scope: Option<Scope>,
    #[serde(flatten)]
    pub extra: Extra,
}

impl FlavorType {
    /// `scripture` / `textTranslation`.
    pub fn text_translation(flavor: TextTranslation, scope: Scope) -> Self {
        Self {
            name: FlavorTypeName::Scripture,
            flavor: Flavor::TextTranslation(flavor),
            current_scope: Some(scope),
            extra: Extra::new(),
        }
    }

    /// `gloss` / `textStories`, the flavor of Open Bible Stories.
    pub fn text_stories(scope: Scope) -> Self {
        Self {
            name: FlavorTypeName::Gloss,
            flavor: Flavor::TextStories(TextStories::default()),
            current_scope: Some(scope),
            extra: Extra::new(),
        }
    }
}

/// `type.flavorType.flavor`, chosen by its `name`.
#[derive(Debug, Clone, PartialEq)]
pub enum Flavor {
    TextTranslation(TextTranslation),
    TextStories(TextStories),
    /// Any other flavor, such as an `x-` flavor, kept as written.
    Other(OtherFlavor),
}

const TEXT_TRANSLATION: &str = "textTranslation";
const TEXT_STORIES: &str = "textStories";

impl Flavor {
    pub fn name(&self) -> &str {
        match self {
            Self::TextTranslation(_) => TEXT_TRANSLATION,
            Self::TextStories(_) => TEXT_STORIES,
            Self::Other(other) => other.name(),
        }
    }
}

/// A flavor's fields with its `name`. Fields stream into the same object, so a `name` in
/// an `extra` map reaches the writer as a second key instead of vanishing.
#[derive(Serialize)]
struct Named<'a, T> {
    name: &'a str,
    #[serde(flatten)]
    fields: &'a T,
}

impl Serialize for Flavor {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let name = self.name();
        match self {
            Self::TextTranslation(fields) => Named { name, fields }.serialize(serializer),
            Self::TextStories(fields) => Named { name, fields }.serialize(serializer),
            Self::Other(flavor) => Named {
                name,
                fields: &flavor.extra,
            }
            .serialize(serializer),
        }
    }
}

impl<'de> Deserialize<'de> for Flavor {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        use serde::de::Error as _;
        let mut map = Map::<String, Value>::deserialize(deserializer)?;
        let name = match map.remove("name") {
            Some(Value::String(name)) => name,
            Some(_) => return Err(D::Error::custom("flavor name is not a string")),
            None => return Err(D::Error::missing_field("name")),
        };
        let rest = Value::Object(map);
        Ok(match name.as_str() {
            TEXT_TRANSLATION => Self::TextTranslation(
                TextTranslation::deserialize(&rest).map_err(D::Error::custom)?,
            ),
            TEXT_STORIES => {
                Self::TextStories(TextStories::deserialize(&rest).map_err(D::Error::custom)?)
            }
            _ => Self::Other(OtherFlavor {
                name,
                extra: Extra::deserialize(&rest).map_err(D::Error::custom)?,
            }),
        })
    }
}

open_enum! {
    pub enum ProjectType {
        Standard => "standard",
        Daughter => "daughter",
        StudyBible => "studyBible",
        StudyBibleAdditions => "studyBibleAdditions",
        BackTranslation => "backTranslation",
        Auxiliary => "auxiliary",
        TransliterationManual => "transliterationManual",
        TransliterationWithEncoder => "transliterationWithEncoder",
    }
}

open_enum! {
    pub enum TranslationType {
        FirstTranslation => "firstTranslation",
        NewTranslation => "newTranslation",
        Revision => "revision",
        StudyOrHelpMaterial => "studyOrHelpMaterial",
    }
}

open_enum! {
    pub enum Audience {
        Basic => "basic",
        Common => "common",
        CommonLiterary => "common-literary",
        Literary => "literary",
        Liturgical => "liturgical",
        Children => "children",
    }
}

/// The `textTranslation` flavor: a Bible translation in USFM.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TextTranslation {
    pub project_type: ProjectType,
    pub translation_type: TranslationType,
    pub audience: Audience,
    pub usfm_version: String,
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    pub conventions: Option<BTreeMap<String, String>>,
    #[serde(flatten)]
    pub extra: Extra,
}

impl TextTranslation {
    /// A standard first translation for a common audience, as tC4 creates.
    pub fn new(usfm_version: impl Into<String>) -> Self {
        Self {
            project_type: ProjectType::Standard,
            translation_type: TranslationType::FirstTranslation,
            audience: Audience::Common,
            usfm_version: usfm_version.into(),
            conventions: None,
            extra: Extra::new(),
        }
    }
}

/// The `textStories` flavor. SB 1.0 allows no other field on it.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct TextStories {
    #[serde(flatten)]
    pub extra: Extra,
}

/// A flavor this model has no struct for.
#[derive(Debug, Clone, PartialEq)]
pub struct OtherFlavor {
    name: String,
    pub extra: Extra,
}

impl OtherFlavor {
    /// Returns `None` for `textTranslation` and `textStories`, which have their own variants.
    pub fn new(name: impl Into<String>) -> Option<Self> {
        let name = name.into();
        (name != TEXT_TRANSLATION && name != TEXT_STORIES).then(|| Self {
            name,
            extra: Extra::new(),
        })
    }

    pub fn name(&self) -> &str {
        &self.name
    }
}

/// One entry of `localizedNames`, such as the names of a book.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LocalizedName {
    pub short: LocalizedText,
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    pub long: Option<LocalizedText>,
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    pub abbr: Option<LocalizedText>,
    #[serde(flatten)]
    pub extra: Extra,
}

/// One entry of `ingredients`: a file of the burrito.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Ingredient {
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    pub checksum: Option<Checksum>,
    pub mime_type: MimeType,
    pub size: u64,
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    pub scope: Option<Scope>,
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    pub role: Option<Role>,
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    pub lang: Option<String>,
    #[serde(flatten)]
    pub extra: Extra,
}

impl Ingredient {
    /// The entry for a file at `path` holding exactly `bytes`.
    pub fn from_bytes(path: &IngredientPath, bytes: &[u8]) -> Self {
        Self::from_facts(path, FileFacts::of(bytes))
    }

    pub fn from_facts(path: &IngredientPath, facts: FileFacts) -> Self {
        Self {
            checksum: Some(Checksum {
                md5: facts.md5,
                extra: Extra::new(),
            }),
            mime_type: MimeType::for_path(path),
            size: facts.size,
            scope: None,
            role: None,
            lang: None,
            extra: Extra::new(),
        }
    }

    /// Takes the facts of the file's new bytes. When the md5 changes, other digests in the
    /// checksum (`sha3-256`, `sha3-512`) describe the old bytes, so they are removed rather
    /// than left wrong.
    pub fn refresh(&mut self, facts: FileFacts) {
        self.size = facts.size;
        let unchanged = self
            .checksum
            .as_ref()
            .is_some_and(|checksum| checksum.md5 == facts.md5);
        if !unchanged {
            self.checksum = Some(Checksum {
                md5: facts.md5,
                extra: Extra::new(),
            });
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Checksum {
    pub md5: Md5,
    /// Other digests, such as `sha3-256`.
    #[serde(flatten)]
    pub extra: Extra,
}

/// An ingredient role, such as `versification` or a custom `x-alignment`. Roles are
/// decorative: a Pankosmia server rescan drops them, so code must find files by path.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Role(String);

impl Role {
    pub fn new(role: impl Into<String>) -> Self {
        Self(role.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Whether this is a custom `x-` role.
    pub fn is_custom(&self) -> bool {
        self.0.starts_with("x-")
    }
}

/// `copyright`. The schema needs exactly one of `publicDomain: true`, `shortStatements`
/// or `licenses`; the fields stay separate so a document read is written back as it was.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Copyright {
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    pub public_domain: Option<bool>,
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    pub short_statements: Option<Vec<ShortStatement>>,
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    pub licenses: Option<Vec<License>>,
    #[serde(flatten)]
    pub extra: Extra,
}

impl Copyright {
    pub fn statement(text: impl Into<String>) -> Self {
        Self {
            public_domain: None,
            short_statements: Some(vec![ShortStatement {
                statement: text.into(),
                lang: None,
                mimetype: None,
                extra: Extra::new(),
            }]),
            licenses: None,
            extra: Extra::new(),
        }
    }

    pub fn public_domain() -> Self {
        Self {
            public_domain: Some(true),
            short_statements: None,
            licenses: None,
            extra: Extra::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ShortStatement {
    pub statement: String,
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    pub lang: Option<String>,
    /// Spelled `mimetype` in the schema, unlike an ingredient's `mimeType`.
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    pub mimetype: Option<String>,
    #[serde(flatten)]
    pub extra: Extra,
}

/// One of `copyright.licenses`: a URL or the path of an ingredient.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct License {
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    pub url: Option<String>,
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    pub ingredient: Option<String>,
    #[serde(flatten)]
    pub extra: Extra,
}

open_enum! {
    /// `relationships[].relationType`.
    pub enum RelationType {
        Source => "source",
        Target => "target",
        Expression => "expression",
        Parascriptural => "parascriptural",
        Peripheral => "peripheral",
    }
}

/// A burrito this one depends on, such as a source text or a helps resource.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Relationship {
    pub relation_type: RelationType,
    pub flavor: String,
    /// `authority::identifier`, such as `dcs::unfoldingWord/en_ult`.
    pub id: String,
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    pub revision: Option<String>,
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    pub variant: Option<String>,
    #[serde(flatten)]
    pub extra: Extra,
}

open_enum! {
    /// `agencies[].roles[]`.
    pub enum AgencyRole {
        RightsAdmin => "rightsAdmin",
        RightsHolder => "rightsHolder",
        Content => "content",
        Publication => "publication",
        Management => "management",
        Finance => "finance",
        Qa => "qa",
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Agency {
    /// `authority::identifier`.
    pub id: String,
    pub name: LocalizedText,
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    pub abbr: Option<LocalizedText>,
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    pub url: Option<String>,
    pub roles: Vec<AgencyRole>,
    #[serde(flatten)]
    pub extra: Extra,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TargetArea {
    /// An ISO 3166 country code or a UN M49 region code.
    pub code: String,
    pub name: LocalizedText,
    #[serde(flatten)]
    pub extra: Extra,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Progress {
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    pub date_started: Option<String>,
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    pub date_completed: Option<String>,
    #[serde(flatten)]
    pub extra: Extra,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn new_bytes_drop_the_digests_of_the_old_bytes() {
        let path = IngredientPath::parse("ingredients/TIT.usfm").unwrap();
        let mut ingredient = Ingredient::from_bytes(&path, b"old");
        let sha = json!("0".repeat(64));
        let checksum = ingredient.checksum.as_mut().unwrap();
        checksum.extra.insert("sha3-256".to_owned(), sha.clone());
        ingredient.role = Some(Role::new("x-alignment"));

        ingredient.refresh(FileFacts::of(b"old"));
        assert_eq!(ingredient.checksum.as_ref().unwrap().extra["sha3-256"], sha);

        ingredient.refresh(FileFacts::of(b"newer"));
        let checksum = ingredient.checksum.as_ref().unwrap();
        assert_eq!(checksum.md5, Md5::of(b"newer"));
        assert!(checksum.extra.is_empty());
        assert_eq!(ingredient.size, 5);
        assert_eq!(ingredient.role, Some(Role::new("x-alignment")));
    }

    #[test]
    fn values_this_version_does_not_know_are_kept() {
        let flavor_type = json!({
            "name": "scripture",
            "flavor": {"name": "textTranslation", "projectType": "standard",
                       "translationType": "firstTranslation", "audience": "youth",
                       "usfmVersion": "3.1", "x-new": [1, 2]},
            "currentScope": {"TOB": ["1:1-2:3"]}
        });
        let model = FlavorType::deserialize(&flavor_type).unwrap();
        let Flavor::TextTranslation(flavor) = &model.flavor else {
            panic!("expected textTranslation, got {:?}", model.flavor);
        };
        assert_eq!(flavor.audience.as_str(), "youth");
        assert!(!flavor.audience.is_known());
        assert_eq!(serde_json::to_value(&model).unwrap(), flavor_type);

        let custom = json!({"name": "x-tc4Project", "settings": {"a": null}});
        let flavor = Flavor::deserialize(&custom).unwrap();
        assert_eq!(flavor.name(), "x-tc4Project");
        assert_eq!(serde_json::to_value(&flavor).unwrap(), custom);
        assert!(OtherFlavor::new("textStories").is_none());
    }

    #[test]
    fn an_explicit_null_is_refused_rather_than_dropped() {
        let entry = json!({"mimeType": "text/plain", "size": 1, "role": null});
        assert!(Ingredient::deserialize(&entry).is_err());
        let entry = json!({"mimeType": "text/plain", "size": 1, "x-note": null});
        let ingredient = Ingredient::deserialize(&entry).unwrap();
        assert_eq!(serde_json::to_value(&ingredient).unwrap(), entry);
    }
}
