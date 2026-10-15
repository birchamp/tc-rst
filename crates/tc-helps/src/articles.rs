//! Door43 articles: translation words (`bible/<category>/<slug>.md`) and Translation
//! Academy (`<manual>/<slug>/{title.md, sub-title.md, 01.md}` with each manual's
//! `toc.yaml` and `config.yaml`).

use std::collections::BTreeMap;

use yaml_rust2::{Yaml, YamlLoader};

use crate::Error;
use crate::error::YamlFile;
use crate::ids::{AcademyArticleId, Manual, Slug, WordArticleId, WordCategory};
use crate::model::{AcademyArticle, WordArticle};

/// Names a translation words article by its path. Accepts the RC form
/// `bible/kt/god.md`, the export form `payload/kt/god.md` (with or without the
/// `ingredients/` prefix), or just `kt/god.md`.
pub fn word_article_id(path: &str) -> Result<WordArticleId, Error> {
    let bad = || Error::InvalidValue {
        what: "translation words article path",
        value: path.to_owned(),
    };
    let mut parts = path.rsplit('/');
    let file = parts.next().ok_or_else(bad)?;
    let folder = parts.next().ok_or_else(bad)?;
    let slug = file.strip_suffix(".md").ok_or_else(bad)?;
    Ok(WordArticleId::Dcs {
        category: WordCategory::parse(folder).ok_or_else(bad)?,
        slug: Slug::parse(slug).map_err(|_| bad())?,
    })
}

/// Text as a file holds it, without a leading byte order mark, which editors on Windows
/// write and which is not text.
pub(crate) fn without_bom(text: &str) -> &str {
    text.strip_prefix('\u{feff}').unwrap_or(text)
}

/// Reads a translation words article. The title is the first `# ` heading; the body is
/// what follows it. An article with no heading keeps its whole text as the body and
/// takes its slug as the title, as tC does.
pub fn parse_word_article(id: WordArticleId, markdown: &str) -> WordArticle {
    let markdown = without_bom(markdown);
    let mut lines = markdown.split_inclusive('\n');
    let mut consumed = 0;
    for line in lines.by_ref() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            consumed += line.len();
            continue;
        }
        if let Some(title) = trimmed.strip_prefix("# ") {
            consumed += line.len();
            let body = markdown[consumed..].trim_start_matches(['\n', '\r']);
            return WordArticle {
                id,
                title: title.trim().to_owned(),
                body: body.to_owned(),
            };
        }
        break;
    }
    let title = match &id {
        WordArticleId::Dcs { slug, .. } => slug.as_str().to_owned(),
        WordArticleId::Aquifer(content) => content.as_str().to_owned(),
    };
    WordArticle {
        id,
        title,
        body: markdown.to_owned(),
    }
}

/// Assembles a Translation Academy article from its three files. `title.md` holds the
/// bare title with no `#` and often no final newline; `sub-title.md` may be missing.
pub fn academy_article(
    manual: Manual,
    slug: Slug,
    title_md: &str,
    sub_title_md: Option<&str>,
    body_md: &str,
) -> AcademyArticle {
    let subtitle = sub_title_md
        .map(|s| without_bom(s).trim())
        .filter(|s| !s.is_empty())
        .map(str::to_owned);
    AcademyArticle {
        id: AcademyArticleId::Dcs { manual, slug },
        title: without_bom(title_md).trim().to_owned(),
        subtitle,
        body: without_bom(body_md).to_owned(),
    }
}

/// One entry of a manual's table of contents.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TocEntry {
    pub title: String,
    /// The article slug, for entries that open an article.
    pub link: Option<Slug>,
    pub sections: Vec<TocEntry>,
}

/// A manual's `toc.yaml`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Toc {
    pub title: String,
    pub sections: Vec<TocEntry>,
}

impl Toc {
    /// Every article slug, depth first, in reading order.
    pub fn links(&self) -> Vec<&Slug> {
        fn walk<'a>(entries: &'a [TocEntry], out: &mut Vec<&'a Slug>) {
            for e in entries {
                if let Some(link) = &e.link {
                    out.push(link);
                }
                walk(&e.sections, out);
            }
        }
        let mut out = Vec::new();
        walk(&self.sections, &mut out);
        out
    }
}

fn yaml_error(file: YamlFile, problem: impl Into<String>) -> Error {
    Error::Yaml {
        file,
        problem: problem.into(),
    }
}

pub(crate) fn load_yaml(text: &str, file: YamlFile) -> Result<Yaml, Error> {
    let mut docs = YamlLoader::load_from_str(without_bom(text))
        .map_err(|e| yaml_error(file, e.to_string()))?;
    if docs.is_empty() {
        return Err(yaml_error(file, "the file is empty"));
    }
    Ok(docs.swap_remove(0))
}

/// The text of a scalar. YAML reads `title: 2021` as a number; a title is still text.
pub(crate) fn scalar(y: &Yaml) -> Option<String> {
    match y {
        Yaml::String(s) | Yaml::Real(s) => Some(s.clone()),
        Yaml::Integer(i) => Some(i.to_string()),
        Yaml::Boolean(b) => Some(b.to_string()),
        _ => None,
    }
}

/// Parses a manual's `toc.yaml`.
pub fn parse_toc(text: &str) -> Result<Toc, Error> {
    let doc = load_yaml(text, YamlFile::AcademyToc)?;
    let title =
        scalar(&doc["title"]).ok_or_else(|| yaml_error(YamlFile::AcademyToc, "no title"))?;
    Ok(Toc {
        title,
        sections: toc_entries(&doc["sections"], "sections")?,
    })
}

fn toc_entries(y: &Yaml, at: &str) -> Result<Vec<TocEntry>, Error> {
    let items = match y {
        Yaml::BadValue | Yaml::Null => return Ok(Vec::new()),
        Yaml::Array(items) => items,
        _ => {
            return Err(yaml_error(
                YamlFile::AcademyToc,
                format!("{at} is not a list"),
            ));
        }
    };
    items
        .iter()
        .enumerate()
        .map(|(i, item)| {
            let here = format!("{at}[{i}]");
            let title = scalar(&item["title"])
                .ok_or_else(|| yaml_error(YamlFile::AcademyToc, format!("{here} has no title")))?;
            let link = match scalar(&item["link"]) {
                None => None,
                Some(link) => Some(Slug::parse(&link).map_err(|_| {
                    yaml_error(
                        YamlFile::AcademyToc,
                        format!("{here} link {link:?} is not a slug"),
                    )
                })?),
            };
            Ok(TocEntry {
                title,
                link,
                sections: toc_entries(&item["sections"], &format!("{here}.sections"))?,
            })
        })
        .collect()
}

/// What a manual's `config.yaml` says about one article.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ArticleConfig {
    /// Articles to read first.
    pub dependencies: Vec<Slug>,
    /// Articles to read next.
    pub recommended: Vec<Slug>,
}

/// Parses a manual's `config.yaml`: a map from article slug to its `dependencies` and
/// `recommended` lists. A missing or `null` list is empty.
pub fn parse_academy_config(text: &str) -> Result<BTreeMap<Slug, ArticleConfig>, Error> {
    let doc = load_yaml(text, YamlFile::AcademyConfig)?;
    let Yaml::Hash(map) = doc else {
        return Err(yaml_error(
            YamlFile::AcademyConfig,
            "the file is not a map of articles",
        ));
    };
    let mut out = BTreeMap::new();
    for (key, value) in &map {
        let name = scalar(key).unwrap_or_default();
        let slug = Slug::parse(&name)
            .map_err(|_| yaml_error(YamlFile::AcademyConfig, format!("{name:?} is not a slug")))?;
        let list = |field: &str| -> Result<Vec<Slug>, Error> {
            match &value[field] {
                Yaml::BadValue | Yaml::Null => Ok(Vec::new()),
                Yaml::Array(items) => items
                    .iter()
                    .map(|item| {
                        let text = scalar(item).unwrap_or_default();
                        Slug::parse(&text).map_err(|_| {
                            yaml_error(
                                YamlFile::AcademyConfig,
                                format!("{name}.{field}: {text:?} is not a slug"),
                            )
                        })
                    })
                    .collect(),
                _ => Err(yaml_error(
                    YamlFile::AcademyConfig,
                    format!("{name}.{field} is not a list"),
                )),
            }
        };
        out.insert(
            slug,
            ArticleConfig {
                dependencies: list("dependencies")?,
                recommended: list("recommended")?,
            },
        );
    }
    Ok(out)
}
