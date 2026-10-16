//! The OBS project template, the New Open Bible Stories dialog's rules and the new-project
//! rules (J20, #287; D74).
//!
//! A new OBS project is the platform's `text_stories` content template — pankosmia
//! `resource-core` 54802be, which tC4 vendors byte for byte as
//! `conformance/fixtures/text_stories` — with every story in its seed form (R-10.2.4) and
//! its `metadata.json` placeholders filled. The template ships inside this crate, in
//! `template/text_stories`, and its stories must equal tC4's pinned copy byte for byte
//! ([`validate_template`]). The scope is always all fifty stories (R-10.2.2), and an OBS
//! project has no versification.

use std::collections::{BTreeMap, BTreeSet};

use serde_json::Value;

use crate::error::{NewProjectError, TemplateError, WriteError};
use crate::ids::{STORY_COUNT, StoryNo};
use crate::js::trim;
use crate::story::seed_story;

macro_rules! template_file {
    ($path:literal) => {
        (
            $path,
            include_str!(concat!("../template/text_stories/", $path)),
        )
    };
}
macro_rules! story {
    ($n:literal) => {
        (
            concat!("ingredients/content/", $n, ".md"),
            include_str!(concat!(
                "../template/text_stories/ingredients/content/",
                $n,
                ".md"
            )),
        )
    };
}

/// Every file of the template, by its path below `text_stories/`.
pub const TEMPLATE_FILES: [(&str, &str); 55] = [
    template_file!("metadata.json"),
    template_file!("ingredients/LICENSE.md"),
    template_file!("ingredients/content/front/title.md"),
    template_file!("ingredients/content/front/intro.md"),
    template_file!("ingredients/content/back/intro.md"),
    story!("01"),
    story!("02"),
    story!("03"),
    story!("04"),
    story!("05"),
    story!("06"),
    story!("07"),
    story!("08"),
    story!("09"),
    story!("10"),
    story!("11"),
    story!("12"),
    story!("13"),
    story!("14"),
    story!("15"),
    story!("16"),
    story!("17"),
    story!("18"),
    story!("19"),
    story!("20"),
    story!("21"),
    story!("22"),
    story!("23"),
    story!("24"),
    story!("25"),
    story!("26"),
    story!("27"),
    story!("28"),
    story!("29"),
    story!("30"),
    story!("31"),
    story!("32"),
    story!("33"),
    story!("34"),
    story!("35"),
    story!("36"),
    story!("37"),
    story!("38"),
    story!("39"),
    story!("40"),
    story!("41"),
    story!("42"),
    story!("43"),
    story!("44"),
    story!("45"),
    story!("46"),
    story!("47"),
    story!("48"),
    story!("49"),
    story!("50"),
];

/// The template's `metadata.json`, with its `%%…%%` placeholders and without
/// `localizedNames`, as resource-core has it.
pub fn template_metadata() -> &'static str {
    TEMPLATE_FILES[0].1
}

/// The template's file for one story, with the gateway title the platform copies.
pub fn template_story(story: StoryNo) -> &'static str {
    TEMPLATE_FILES[4 + usize::from(story.get())].1
}

/// The files a new project copies from the template and the app never writes, by their
/// path below `ingredients/` (R-10.2.1; `isObsBaseIngredient`).
pub fn template_base_ingredients() -> impl Iterator<Item = (&'static str, &'static str)> {
    TEMPLATE_FILES[1..5]
        .iter()
        .map(|(path, text)| (&path["ingredients/".len()..], *text))
}

const MARKER: &str = "\"localizedNames\"";

/// The template metadata with `"localizedNames": {}` before `"ingredients"`, indented as
/// that line is; unchanged when it carries the key. The platform's own metadata reader
/// requires the key, which resource-core's OBS template lacks (`withLocalizedNames`,
/// `scripts/fix-obs-template.mjs:30-36`; PLATFORM-NOTES #36).
pub fn with_localized_names(text: &str) -> Result<String, TemplateError> {
    if text.contains(MARKER) {
        return Ok(text.to_owned());
    }
    // `/^[ \t]*"ingredients":/m`: the first line that holds only spaces or tabs before
    // the key.
    let mut at = 0;
    for line in text.split_inclusive('\n') {
        let body = line.trim_start_matches([' ', '\t']);
        if body.starts_with("\"ingredients\":") {
            let indent = &line[..line.len() - body.len()];
            return Ok(format!(
                "{}{indent}\"localizedNames\": {{}},\n{}",
                &text[..at],
                &text[at..]
            ));
        }
        at += line.len();
    }
    Err(TemplateError::NoIngredientsKey)
}

const STORY_DIR: &str = "ingredients/content/";

/// The names directly inside `dir` of a set of file paths: files, and the first segment
/// of deeper paths.
fn entries<'a>(files: impl Iterator<Item = &'a str>, dir: &str) -> Vec<String> {
    let names: BTreeSet<&str> = files
        .filter_map(|path| path.strip_prefix(dir))
        .map(|rest| rest.split('/').next().unwrap_or(rest))
        .collect();
    names.into_iter().map(str::to_owned).collect()
}

/// Checks a prepared template, by path below `text_stories/`, against the pinned one,
/// byte for byte: the metadata carries `localizedNames`, the story directory holds
/// exactly the fifty stories, `front` and `back`, no story or matter file holds a carriage
/// return, and each equals the reference's (`validateObsTemplate`,
/// `scripts/fix-obs-template.mjs:43-88`, the carriage returns at `:69` and `:77`). Returns
/// the number of stories. Nothing is rewritten: a converted, truncated, added or removed
/// file is refused.
pub fn validate_template(
    files: &BTreeMap<String, Vec<u8>>,
    reference: &BTreeMap<String, Vec<u8>>,
) -> Result<usize, TemplateError> {
    let metadata = files
        .get("metadata.json")
        .ok_or_else(|| TemplateError::MetadataMissing("metadata.json".to_owned()))?;
    if !String::from_utf8_lossy(metadata).contains(MARKER) {
        return Err(TemplateError::LocalizedNamesMissing(
            "metadata.json".to_owned(),
        ));
    }
    let names: Vec<String> = (1..=STORY_COUNT).map(|n| format!("{n:02}.md")).collect();
    let mut expected: Vec<String> = names.clone();
    expected.extend(["back".to_owned(), "front".to_owned()]);
    expected.sort();
    let found = entries(files.keys().map(String::as_str), STORY_DIR);
    if found != expected {
        return Err(TemplateError::ContentSet {
            dir: STORY_DIR.to_owned(),
            expected: expected.join(", "),
            found: found.join(", "),
        });
    }
    let same = |name: &str, what: &'static str| -> Result<(), TemplateError> {
        let path = format!("{STORY_DIR}{name}");
        let actual = files.get(&path).ok_or_else(|| TemplateError::ContentSet {
            dir: STORY_DIR.to_owned(),
            expected: name.to_owned(),
            found: String::new(),
        })?;
        let wanted = reference
            .get(&path)
            .ok_or_else(|| TemplateError::ReferenceMissing(path.clone()))?;
        if actual.contains(&b'\r') {
            return Err(TemplateError::CarriageReturn { what, path });
        }
        if actual != wanted {
            return Err(TemplateError::BytesDiffer { what, path });
        }
        Ok(())
    };
    for name in &names {
        same(name, "story")?;
    }
    for name in ["front/title.md", "front/intro.md", "back/intro.md"] {
        same(name, "content")?;
    }
    for (dir, wanted) in [
        ("front", &["intro.md", "title.md"][..]),
        ("back", &["intro.md"][..]),
    ] {
        let path = format!("{STORY_DIR}{dir}/");
        let found = entries(files.keys().map(String::as_str), &path);
        if found != wanted {
            return Err(TemplateError::ContentSet {
                dir: path,
                expected: wanted.join(", "),
                found: found.join(", "),
            });
        }
    }
    Ok(names.len())
}

/// What a person types in the New Open Bible Stories dialog that the project itself
/// records, as typed (`NewObs.jsx`; the `np` form of `src/state.jsx`). The direction,
/// checking language and font are the project's settings and pins, not the template's.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NewObsForm<'a> {
    pub name: &'a str,
    pub language_name: &'a str,
    pub language_code: &'a str,
}

/// Why the dialog's Create button refuses its form, before anything is made
/// (`validateNewBible`, `src/state.jsx:1631-1637`; `createObs`, `:6491-6506`). The
/// messages are tC4's English ones (`src/i18n/en.json`, `wizard.*`).
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum FormRefusal {
    /// `wizard.nameRequired`
    #[error("Give the project a name.")]
    NameRequired,
    /// `wizard.codeRequired`
    #[error("Give the project a language code.")]
    CodeRequired,
    /// Neither the name nor the code holds a Latin letter or a digit (`wizard.abbrRequired`).
    #[error("Add a short name in Latin letters or digits — it names the project folder.")]
    AbbrRequired,
    /// `wizard.nameInUse`
    #[error("A project folder named “{abbreviation}” already exists.")]
    NameInUse { abbreviation: String },
}

/// tC4's folder name of a project name: lower case, each run of anything but `a-z` and
/// `0-9` one `_`, no `_` at either end (`slug`, `src/state.jsx:1634`).
pub fn slug(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for c in value.to_lowercase().chars() {
        if c.is_ascii_lowercase() || c.is_ascii_digit() {
            out.push(c);
        } else if !out.is_empty() && !out.ends_with('_') {
            out.push('_');
        }
    }
    let kept = out.trim_end_matches('_').len();
    out.truncate(kept);
    out
}

/// The path of an app-made project on this computer (`_local_/_local_/<folder>`).
const LOCAL_PROJECTS: &str = "_local_/_local_/";

impl NewObsForm<'_> {
    /// The project the Create button asks the platform for, or why it refuses the form
    /// (`validateNewBible`, `src/state.jsx:1631-1637`; `createObs`, `:6491-6517`): a name
    /// and a code are required; the folder is the name's slug, else the code's; a folder
    /// already among `existing` (the projects on this computer, as `listLocalRepos`
    /// lists them) is in use. The name and the code are sent trimmed, and a private-use
    /// code (`x-…`) takes the language name, or the project name when that is empty.
    pub fn project<'e>(
        &self,
        existing: impl IntoIterator<Item = &'e str>,
    ) -> Result<NewObsProject, FormRefusal> {
        let name = trim(self.name);
        let code = trim(self.language_code);
        if name.is_empty() {
            return Err(FormRefusal::NameRequired);
        }
        if code.is_empty() {
            return Err(FormRefusal::CodeRequired);
        }
        let abbreviation = Some(slug(name))
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| slug(code));
        if abbreviation.is_empty() {
            return Err(FormRefusal::AbbrRequired);
        }
        if existing
            .into_iter()
            .any(|path| path.strip_prefix(LOCAL_PROJECTS) == Some(abbreviation.as_str()))
        {
            return Err(FormRefusal::NameInUse { abbreviation });
        }
        let language_name = code.starts_with("x-").then(|| {
            let typed = trim(self.language_name);
            if typed.is_empty() { name } else { typed }.to_owned()
        });
        Ok(NewObsProject {
            name: name.to_owned(),
            abbreviation,
            language_code: code.to_owned(),
            language_name,
        })
    }
}

/// The new project the platform is asked for (`NewObsResourceParams`,
/// `src/data/serverApi.ts:119-128`), as [`NewObsForm::project`] makes it from the dialog.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewObsProject {
    pub name: String,
    /// The project's folder name, so one path segment.
    pub abbreviation: String,
    pub language_code: String,
    /// Required for a private-use code (`x-…`).
    pub language_name: Option<String>,
}

impl NewObsProject {
    /// Refuses, before anything exists, a name the template's JSON cannot hold as written
    /// (it is spliced in unescaped), a folder name that is not one safe path segment, and a
    /// private-use language code without a name (`assertJsonSafeText`,
    /// `assertSafeSegment`, `newObsResource`; `src/data/serverApi.ts:220-238`, `:653-662`).
    pub fn check(&self) -> Result<(), NewProjectError> {
        if self
            .name
            .chars()
            .any(|c| c == '"' || c == '\\' || c < '\u{20}')
        {
            return Err(NewProjectError::UnsafeName(self.name.clone()));
        }
        let a = &self.abbreviation;
        if a.is_empty()
            || a.starts_with('.')
            || a.chars()
                .any(|c| crate::js::is_space(c) || "&?:#%\\*\"'<>|".contains(c))
        {
            return Err(NewProjectError::UnsafeAbbreviation(a.clone()));
        }
        if self.language_code.starts_with("x-")
            && self.language_name.as_deref().is_none_or(str::is_empty)
        {
            return Err(NewProjectError::PrivateLanguageNeedsName);
        }
        Ok(())
    }
}

/// A new OBS project's files.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CreatedObs {
    /// The fifty stories in their seed form, the base the journal splices onto
    /// (R-10.7.4); none of it is journaled.
    pub stories: Vec<(StoryNo, String)>,
    /// The front and back matter and the license, by path below `ingredients/`, copied
    /// byte for byte.
    pub base_ingredients: Vec<(&'static str, &'static str)>,
    /// The template's `metadata.json` with its placeholders filled.
    pub metadata: String,
}

/// The template's metadata stamped the way the platform's `new-obs-resource` stamps it —
/// string replacement of the placeholders in the template tC4 serves, which carries
/// `localizedNames` (tC4's model of it: `test/helpers/journalingRig.ts:41-50`;
/// PLATFORM-NOTES #36, #43). `language` is the language object the platform writes,
/// such as `{"tag": "es-419", "name": {"en": "Spanish (419)"}}`.
pub fn stamp_metadata(
    project: &NewObsProject,
    created: &str,
    language: &Value,
) -> Result<String, crate::Error> {
    project.check()?;
    let text = with_localized_names(template_metadata())?
        .replace("%%ABBR%%", &project.abbreviation)
        .replace("%%CONTENT_NAME%%", &project.name)
        .replace("%%CREATED_TIMESTAMP%%", created)
        .replacen("%%LANGUAGE%%", &language.to_string(), 1);
    Ok(text)
}

/// A new project's stories, matter and metadata (`createObsProject`,
/// `src/data/journal/journalingStore.ts:1213-1236`): the platform's copy of the template,
/// then each story reduced to its seed form.
pub fn create_obs_project(
    project: &NewObsProject,
    created: &str,
    language: &Value,
) -> Result<CreatedObs, crate::Error> {
    let metadata = stamp_metadata(project, created, language)?;
    let stories = StoryNo::all()
        .map(|story| Ok((story, seed_story(template_story(story))?)))
        .collect::<Result<Vec<_>, WriteError>>()?;
    Ok(CreatedObs {
        stories,
        base_ingredients: template_base_ingredients().collect(),
        metadata,
    })
}

/// The scope table of every OBS project: the template's `currentScope`, kept verbatim and
/// never reconstructed (R-10.2.3).
pub fn template_current_scope() -> Value {
    let filled = template_metadata().replacen("%%LANGUAGE%%", "null", 1);
    serde_json::from_str::<Value>(&filled)
        .map(|meta| meta["type"]["flavorType"]["currentScope"].clone())
        .unwrap_or(Value::Null)
}
