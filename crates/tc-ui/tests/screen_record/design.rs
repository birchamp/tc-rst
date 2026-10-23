//! What docs/APP-DESIGN.md says the gate must know: the work units of §10 and the scope each
//! builds; for each of the 62 baseline screens of §7.6, which unit captures which layer, on
//! which backend, and which unit captures it again; and for each group of gallery cases in
//! §7.4, which units capture its DOM and its pixels.

use std::collections::BTreeSet;
use std::fmt;

use serde::{Deserialize, Serialize};

/// A work unit of §10: a letter A to G, a number, and `a` or `b` for a split unit.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct UnitId(String);

impl UnitId {
    pub fn parse(text: &str) -> Result<Self, String> {
        let mut chars = text.chars();
        let letter_ok = chars.next().is_some_and(|c| ('A'..='G').contains(&c));
        let rest: String = chars.collect();
        let digits = rest.trim_end_matches(['a', 'b']);
        let suffix_ok = rest.len() - digits.len() <= 1;
        if letter_ok
            && suffix_ok
            && !digits.is_empty()
            && digits.bytes().all(|b| b.is_ascii_digit())
        {
            Ok(Self(text.to_owned()))
        } else {
            Err(format!("{text:?} is not a unit id (A1, A6b, G0, ...)"))
        }
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl TryFrom<String> for UnitId {
    type Error = String;
    fn try_from(text: String) -> Result<Self, String> {
        Self::parse(&text)
    }
}

impl From<UnitId> for String {
    fn from(unit: UnitId) -> String {
        unit.0
    }
}

impl fmt::Display for UnitId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// The backend a screen is captured on (§7.5).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Backend {
    #[serde(rename = "memory")]
    Memory,
    /// `MemoryBackend` with a scripted job outcome, captured again when its recapture unit lands.
    #[serde(rename = "memory+scripted")]
    MemoryScripted,
    #[serde(rename = "store")]
    Store,
}

/// A layer of §7.3 that a capture can claim. Behaviour is compared per recorded sequence.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Layer {
    Pixels,
    Dom,
    Meaning,
}

impl Layer {
    pub const ALL: [Layer; 3] = [Layer::Pixels, Layer::Dom, Layer::Meaning];
}

/// A layer of §7.3 that a row can differ in: the three a capture claims, and behaviour. A
/// residual names exactly the layers its row differs in.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DiffLayer {
    Pixels,
    Dom,
    Meaning,
    Behaviour,
}

/// One row of §7.6.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ScreenPlan {
    pub name: String,
    pub scenario: String,
    pub locale: String,
    pub pixels: UnitId,
    pub dom: UnitId,
    pub backend: Backend,
    pub recapture: Option<UnitId>,
    /// The unit that must capture a scripted screen again (its recapture unit).
    pub pending_until: Option<UnitId>,
}

impl ScreenPlan {
    /// The unit that first captures `layer`. What the DOM shows comes with the pixels.
    pub fn unit_of(&self, layer: Layer) -> &UnitId {
        match layer {
            Layer::Pixels | Layer::Meaning => &self.pixels,
            Layer::Dom => &self.dom,
        }
    }

    /// A screen whose DOM is compared before its pixels: the Home behind its dialog is not the
    /// port's yet.
    pub fn dom_first(&self) -> bool {
        self.dom != self.pixels
    }
}

/// One row of §7.4's group table: the units that capture a gallery group's Rust side.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GalleryGroup {
    pub name: String,
    pub dom: UnitId,
    pub pixels: UnitId,
}

/// A unit of §10 and its scope cell, with the italic notes `*(…)*` left out: what the unit
/// builds, not what a note says about it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UnitScope {
    pub unit: UnitId,
    pub scope: String,
}

impl UnitScope {
    /// True when the scope names `id` as a whole word (`M18`, never `M180`).
    pub fn names(&self, id: &str) -> bool {
        self.scope
            .split(|c: char| !c.is_ascii_alphanumeric())
            .any(|word| word == id)
    }
}

#[derive(Clone, Debug)]
pub struct Design {
    pub units: BTreeSet<UnitId>,
    pub scopes: Vec<UnitScope>,
    pub screens: Vec<ScreenPlan>,
    pub gallery: Vec<GalleryGroup>,
}

impl Design {
    pub fn screen(&self, name: &str) -> Option<&ScreenPlan> {
        self.screens.iter().find(|s| s.name == name)
    }

    pub fn group(&self, name: &str) -> Option<&GalleryGroup> {
        self.gallery.iter().find(|g| g.name == name)
    }
}

/// The lines between the heading `heading` (exactly) and the next heading of its level or
/// higher.
fn section<'a>(doc: &'a str, heading: &str) -> Result<Vec<&'a str>, String> {
    let level = heading.chars().take_while(|&c| c == '#').count();
    let mut lines = doc.lines();
    lines
        .by_ref()
        .find(|line| line.starts_with(heading))
        .ok_or_else(|| format!("docs/APP-DESIGN.md has no heading {heading:?}"))?;
    Ok(lines
        .take_while(|line| {
            let hashes = line.chars().take_while(|&c| c == '#').count();
            !(hashes > 0 && hashes <= level && line[hashes..].starts_with(' '))
        })
        .collect())
}

/// `text` without its italic notes, each from `*(` to the next `)*`.
fn without_notes(text: &str) -> String {
    let mut out = String::new();
    let mut rest = text;
    while let Some(start) = rest.find("*(") {
        out.push_str(&rest[..start]);
        match rest[start..].find(")*") {
            Some(end) => rest = &rest[start + end + 2..],
            None => {
                rest = "";
            }
        }
    }
    out.push_str(rest);
    out
}

fn cells(line: &str) -> Vec<&str> {
    let inner = line.trim().trim_start_matches('|').trim_end_matches('|');
    inner.split('|').map(str::trim).collect()
}

/// The unit `text` names on `line` of section `section` (`"§7.6"`), one that §10 lists.
fn named_unit(
    units: &BTreeSet<UnitId>,
    text: &str,
    line: &str,
    section: &str,
) -> Result<UnitId, String> {
    let unit = UnitId::parse(text.trim()).map_err(|e| format!("{section} line {line:?}: {e}"))?;
    if units.contains(&unit) {
        Ok(unit)
    } else {
        Err(format!(
            "{section} line {line:?} names {unit}, which is no unit of §10"
        ))
    }
}

/// Reads §10's units, §7.6's screens and §7.4's gallery groups. Any row of §7.6 or §7.4 that
/// does not parse is an error naming the line.
pub fn parse(doc: &str) -> Result<Design, String> {
    let mut units = BTreeSet::new();
    let mut scopes = Vec::new();
    for line in section(doc, "## 10. Work breakdown")? {
        if let Some(rest) = line.strip_prefix("| ")
            && let Some(first) = rest.split(' ').next()
            && let Ok(unit) = UnitId::parse(first)
        {
            let scope = cells(line)
                .get(1)
                .map(|cell| without_notes(cell))
                .ok_or_else(|| format!("§10 line {line:?} has no scope cell"))?;
            scopes.push(UnitScope {
                unit: unit.clone(),
                scope,
            });
            units.insert(unit);
        }
    }
    if units.is_empty() {
        return Err("§10 lists no unit".to_owned());
    }

    let mut screens = Vec::new();
    for line in section(doc, "### 7.6")? {
        if !line.starts_with("| `") {
            continue;
        }
        let row = cells(line);
        let [name, scenario, first, recapture, notes] = row.as_slice() else {
            return Err(format!("§7.6 line {line:?} does not have five cells"));
        };
        let name = name
            .strip_prefix('`')
            .and_then(|n| n.strip_suffix('`'))
            .ok_or_else(|| format!("§7.6 line {line:?}: the screen is not in backticks"))?;
        let (scenario, locale) = match scenario.split_once(", ") {
            Some((scenario, locale)) => (scenario, locale),
            None => (*scenario, "en"),
        };
        let (pixels, dom, backend) = if let Some((dom, pixels)) = first.split_once(" DOM; ") {
            let pixels = pixels.strip_suffix(" pixels").ok_or_else(|| {
                format!("§7.6 line {line:?}: {first:?} is not \"<unit> DOM; <unit> pixels\"")
            })?;
            (
                named_unit(&units, pixels, line, S76)?,
                named_unit(&units, dom, line, S76)?,
                Backend::Memory,
            )
        } else {
            let (unit, kind) = first
                .split_once(", ")
                .ok_or_else(|| format!("§7.6 line {line:?}: {first:?} is not \"<unit>, M\""))?;
            let backend = match kind {
                "M" | "M + fake" => Backend::Memory,
                "M+S" => Backend::MemoryScripted,
                other => return Err(format!("§7.6 line {line:?}: unknown backend {other:?}")),
            };
            let unit = named_unit(&units, unit, line, S76)?;
            (unit.clone(), unit, backend)
        };
        let recapture = match *recapture {
            "—" => None,
            unit => Some(named_unit(&units, unit, line, S76)?),
        };
        let pending_until = if backend == Backend::MemoryScripted {
            let until = recapture.clone().ok_or_else(|| {
                format!("§7.6 line {line:?}: a scripted screen needs the unit that recaptures it")
            })?;
            Some(until)
        } else {
            None
        };
        if let Some((_, rest)) = notes.split_once("`pending_until: ") {
            let named = rest.split('`').next().unwrap_or_default();
            let named = named_unit(&units, named, line, S76)?;
            if Some(&named) != pending_until.as_ref() {
                return Err(format!(
                    "§7.6 line {line:?}: pending_until {named} is not the unit that recaptures it"
                ));
            }
        }
        screens.push(ScreenPlan {
            name: name.to_owned(),
            scenario: scenario.to_owned(),
            locale: locale.to_owned(),
            pixels,
            dom,
            backend,
            recapture,
            pending_until,
        });
    }
    let mut gallery = Vec::new();
    for line in section(doc, "### 7.4")? {
        if !line.starts_with("| `") {
            continue;
        }
        let row = cells(line);
        let [name, _views, dom, pixels] = row.as_slice() else {
            return Err(format!("§7.4 line {line:?} does not have four cells"));
        };
        let name = name
            .strip_prefix('`')
            .and_then(|n| n.strip_suffix('`'))
            .ok_or_else(|| format!("§7.4 line {line:?}: the group is not in backticks"))?;
        if gallery.iter().any(|g: &GalleryGroup| g.name == name) {
            return Err(format!("§7.4 lists the group {name} twice"));
        }
        gallery.push(GalleryGroup {
            name: name.to_owned(),
            dom: named_unit(&units, dom, line, S74)?,
            pixels: named_unit(&units, pixels, line, S74)?,
        });
    }
    Ok(Design {
        units,
        scopes,
        screens,
        gallery,
    })
}

const S76: &str = "§7.6";
const S74: &str = "§7.4";
