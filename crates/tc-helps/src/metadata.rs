//! Identifying a downloaded Door43 resource from its `metadata.json` (the DCS Scripture
//! Burrito export) or its `manifest.yaml` (the RC repository), and pinning helps sources.

use std::collections::BTreeSet;
use std::fmt;

use serde_json::Value;
use tc_core::BookId;
use yaml_rust2::Yaml;

use crate::Error;
use crate::articles::{load_yaml, scalar, without_bom, word_article_id};
use crate::error::{JsonFile, PinMismatch, YamlFile};
use crate::ids::{GitOid, Sha256Digest, WordArticleId};

/// The kind of resource, from the repository name (`en_tn`, `en_obs-tn`) or the RC
/// `identifier`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResourceKind {
    TranslationNotes,
    WordsLinks,
    TranslationWords,
    TranslationAcademy,
    TranslationQuestions,
    ObsStories,
    ObsNotes,
    ObsWordsLinks,
    ObsQuestions,
    /// A Bible text: ULT, UST, GLT, GST, UHB, UGNT and the like.
    Bible,
    Lexicon,
    Other(String),
}

impl ResourceKind {
    fn from_identifier(identifier: &str, flavor: Option<&str>) -> Self {
        match identifier {
            "tn" => Self::TranslationNotes,
            "twl" => Self::WordsLinks,
            "tw" => Self::TranslationWords,
            "ta" => Self::TranslationAcademy,
            "tq" => Self::TranslationQuestions,
            "obs" => Self::ObsStories,
            "obs-tn" => Self::ObsNotes,
            "obs-twl" => Self::ObsWordsLinks,
            "obs-tq" => Self::ObsQuestions,
            _ if flavor == Some("peripheral/x-lexicon") => Self::Lexicon,
            _ if flavor == Some("scripture/textTranslation") => Self::Bible,
            "ult" | "ust" | "glt" | "gst" | "uhb" | "ugnt" | "avd" => Self::Bible,
            other => Self::Other(other.to_owned()),
        }
    }
}

/// The revision a resource states for itself.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Revision {
    Commit(GitOid),
    /// Some burritos state a label instead of a commit, such as `1`.
    Label(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MetadataFormat {
    ScriptureBurrito,
    ResourceContainer,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScriptDirection {
    Ltr,
    Rtl,
}

impl ScriptDirection {
    fn parse(text: &str) -> Option<Self> {
        match text {
            "ltr" => Some(Self::Ltr),
            "rtl" => Some(Self::Rtl),
            _ => None,
        }
    }
}

/// Who and what a downloaded resource says it is. Fields a format does not carry are
/// `None`: a burrito has no version or subject, and a manifest has no owner or revision
/// (those come from the repository and the archive).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResourceIdentity {
    pub format: MetadataFormat,
    /// The organization at export time. Treat it as provenance, not as an address: a
    /// renamed organization stays stale here (`Idiomas-Puentes/es-419_tn`).
    pub owner: Option<String>,
    pub repo: Option<String>,
    pub kind: ResourceKind,
    pub language: String,
    pub direction: Option<ScriptDirection>,
    pub title: Option<String>,
    pub subject: Option<String>,
    /// `flavorType/flavor`, such as `parascriptural/x-bcvnotes`.
    pub flavor: Option<String>,
    pub version: Option<String>,
    pub revision: Option<Revision>,
    /// The books the resource covers, from `currentScope` or `projects`.
    pub books: Vec<BookId>,
}

/// A parsed burrito `metadata.json`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BurritoMetadata {
    pub identity: ResourceIdentity,
    /// Ingredient paths as the metadata lists them, such as `ingredients/TIT.tsv`.
    pub ingredients: Vec<String>,
    pub copyright: Vec<String>,
}

impl BurritoMetadata {
    /// The translation words articles the resource carries (`…/payload/<category>/<slug>.md`
    /// ingredients). A words link to an article outside this set is not a check.
    pub fn word_articles(&self) -> BTreeSet<WordArticleId> {
        self.ingredients
            .iter()
            .filter(|path| path.contains("payload/") && path.ends_with(".md"))
            .filter_map(|path| word_article_id(path).ok())
            .collect()
    }
}

fn json_error(problem: impl Into<String>) -> Error {
    Error::Json {
        file: JsonFile::BurritoMetadata,
        problem: problem.into(),
    }
}

fn text_at<'a>(v: &'a Value, path: &[&str]) -> Option<&'a str> {
    path.iter().try_fold(v, |v, key| v.get(key))?.as_str()
}

/// Reads a Scripture Burrito `metadata.json` as DCS exports it.
pub fn parse_burrito_metadata(json: &str) -> Result<BurritoMetadata, Error> {
    let v: Value =
        serde_json::from_str(without_bom(json)).map_err(|e| json_error(e.to_string()))?;
    let format = v.get("format").or_else(|| v.pointer("/meta/format"));
    if format.and_then(Value::as_str) != Some("scripture burrito") {
        return Err(json_error(
            "not a Scripture Burrito (no format \"scripture burrito\")",
        ));
    }
    let primary = v
        .pointer("/identification/primary")
        .and_then(Value::as_object)
        .ok_or_else(|| json_error("no identification.primary"))?;
    let (key, entry) = primary
        .values()
        .filter_map(Value::as_object)
        .flat_map(|m| m.iter())
        .next()
        .ok_or_else(|| json_error("identification.primary names no resource"))?;
    let (owner, repo) = match key.split_once('/') {
        Some((o, r)) => (Some(o.to_owned()), r.to_owned()),
        None => (None, key.clone()),
    };
    let revision =
        entry
            .get("revision")
            .and_then(Value::as_str)
            .map(|r| match GitOid::parse_any_case(r) {
                Some(sha) => Revision::Commit(sha),
                None => Revision::Label(r.to_owned()),
            });
    let language = v.pointer("/languages/0");
    let tag = language
        .and_then(|l| l.get("tag"))
        .and_then(Value::as_str)
        .ok_or_else(|| json_error("no languages[0].tag"))?;
    let flavor = match (
        text_at(&v, &["type", "flavorType", "name"]),
        text_at(&v, &["type", "flavorType", "flavor", "name"]),
    ) {
        (Some(t), Some(f)) => Some(format!("{t}/{f}")),
        _ => None,
    };
    let suffix = repo.split_once('_').map_or(repo.as_str(), |(_, s)| s);
    let books = v
        .pointer("/type/flavorType/currentScope")
        .and_then(Value::as_object)
        .map(|scope| scope.keys().filter_map(|k| k.parse().ok()).collect())
        .unwrap_or_default();
    let title = v
        .pointer("/identification/name")
        .and_then(Value::as_object)
        .and_then(|names| names.values().next())
        .and_then(Value::as_str)
        .map(str::to_owned);
    let ingredients = v
        .get("ingredients")
        .and_then(Value::as_object)
        .map(|m| m.keys().cloned().collect())
        .unwrap_or_default();
    let copyright = v
        .pointer("/copyright/shortStatements")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(|s| s.get("statement").and_then(Value::as_str))
                .map(str::to_owned)
                .collect()
        })
        .unwrap_or_default();
    Ok(BurritoMetadata {
        identity: ResourceIdentity {
            format: MetadataFormat::ScriptureBurrito,
            owner,
            kind: ResourceKind::from_identifier(suffix, flavor.as_deref()),
            repo: Some(repo),
            language: tag.to_owned(),
            direction: language
                .and_then(|l| l.get("scriptDirection"))
                .and_then(Value::as_str)
                .and_then(ScriptDirection::parse),
            title,
            subject: None,
            flavor,
            version: None,
            revision,
            books,
        },
        ingredients,
        copyright,
    })
}

/// A parsed RC `manifest.yaml`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RcManifest {
    pub identity: ResourceIdentity,
    /// `relation` entries such as `hbo/uhb?v=2.1.30`.
    pub relations: Vec<String>,
    /// `projects[].path`, such as `./tn_TIT.tsv`, by project identifier.
    pub project_paths: Vec<(String, String)>,
    pub rights: Option<String>,
}

fn yaml_error(problem: impl Into<String>) -> Error {
    Error::Yaml {
        file: YamlFile::Manifest,
        problem: problem.into(),
    }
}

/// Reads an RC `manifest.yaml`.
pub fn parse_rc_manifest(text: &str) -> Result<RcManifest, Error> {
    let doc = load_yaml(text, YamlFile::Manifest)?;
    let dc = &doc["dublin_core"];
    if matches!(dc, Yaml::BadValue) {
        return Err(yaml_error("no dublin_core"));
    }
    let field = |name: &str| scalar(&dc[name]);
    let identifier = field("identifier").ok_or_else(|| yaml_error("no dublin_core.identifier"))?;
    let language = scalar(&dc["language"]["identifier"])
        .ok_or_else(|| yaml_error("no dublin_core.language.identifier"))?;
    let projects = doc["projects"].as_vec().cloned().unwrap_or_default();
    let mut project_paths = Vec::new();
    let mut books = Vec::new();
    for p in &projects {
        let Some(id) = scalar(&p["identifier"]) else {
            continue;
        };
        if let Ok(book) = id.parse::<BookId>() {
            books.push(book);
        }
        if let Some(path) = scalar(&p["path"]) {
            project_paths.push((id, path));
        }
    }
    let relations = dc["relation"]
        .as_vec()
        .map(|items| items.iter().filter_map(scalar).collect())
        .unwrap_or_default();
    Ok(RcManifest {
        identity: ResourceIdentity {
            format: MetadataFormat::ResourceContainer,
            owner: None,
            repo: None,
            kind: ResourceKind::from_identifier(&identifier, None),
            direction: scalar(&dc["language"]["direction"])
                .as_deref()
                .and_then(ScriptDirection::parse),
            language,
            title: field("title"),
            subject: field("subject"),
            flavor: None,
            version: field("version"),
            revision: None,
            books,
        },
        relations,
        project_paths,
        rights: field("rights"),
    })
}

/// A Door43 resource pinned to one commit. The commit is the identity; the tag is a label.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DcsPin {
    pub owner: String,
    pub repo: String,
    pub tag: Option<String>,
    pub commit: GitOid,
}

impl DcsPin {
    /// Checks a downloaded resource against the pin: the repository name (in any letter
    /// case; the owner is not compared, because exports keep stale owner names) and the
    /// revision.
    pub fn verify(&self, identity: &ResourceIdentity) -> Result<(), Error> {
        if let Some(repo) = &identity.repo
            && !repo.eq_ignore_ascii_case(&self.repo)
        {
            return Err(Error::PinMismatch(PinMismatch::Repository {
                pinned: self.repo.clone(),
                found: repo.clone(),
            }));
        }
        match &identity.revision {
            Some(Revision::Commit(sha)) if *sha == self.commit => Ok(()),
            Some(Revision::Commit(sha)) => Err(Error::PinMismatch(PinMismatch::Revision {
                pinned: self.commit.to_string(),
                found: sha.to_string(),
            })),
            Some(Revision::Label(label)) => Err(Error::PinMismatch(PinMismatch::Revision {
                pinned: self.commit.to_string(),
                found: label.clone(),
            })),
            None => Err(Error::PinMismatch(PinMismatch::NoRevision)),
        }
    }
}

/// An Aquifer resource pinned to a release: the GitHub repository (which is the
/// `resource_code`), the language folder, the tag, its commit, and the digest of the
/// release asset when the resource came as one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AquiferPin {
    pub resource_code: String,
    /// ISO 639-3, such as `eng` or `por`.
    pub language: String,
    pub tag: String,
    pub commit: GitOid,
    pub asset_sha256: Option<Sha256Digest>,
}

/// Where a set of helps came from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HelpsSource {
    Dcs(DcsPin),
    Aquifer(AquiferPin),
}

/// `unfoldingWord/en_tn v86 (c354b8a…)`, `Aquifer UWTranslationNotes/por v2026-10-12
/// (dbfb46e…)`: enough to tell two pins apart in a message.
impl fmt::Display for HelpsSource {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let short = |sha: &GitOid| sha.to_hex().chars().take(7).collect::<String>();
        match self {
            HelpsSource::Dcs(pin) => {
                write!(f, "{}/{}", pin.owner, pin.repo)?;
                if let Some(tag) = &pin.tag {
                    write!(f, " {tag}")?;
                }
                write!(f, " ({}…)", short(&pin.commit))
            }
            HelpsSource::Aquifer(pin) => write!(
                f,
                "Aquifer {}/{} {} ({}…)",
                pin.resource_code,
                pin.language,
                pin.tag,
                short(&pin.commit)
            ),
        }
    }
}
