//! The gateway languages a project can check against: tC4's registry of 18
//! (language, owner) pairs, and how each owner names its repositories.
//!
//! A gateway language is a (language, owner) pair, never a language alone: `fr` is
//! published by both Xenizo and MVHS. tC4 built the list from a Door43 catalog
//! intersection of released tN, tW and tA under one owner (2026-08-03, `gateways.ts`),
//! because its platform had no catalog-wide search. The flags record what each owner
//! publishes beyond that, as tC4 verified it (2026-08-27 and 2026-10-12):
//!
//! - `has_twl`: whether the owner publishes `<lang>_twl`. An owner without one pins the
//!   bundled English `en_twl` with its own `<lang>_tw` (D97 ruling 7; `ml` and `ur-deva`).
//!   This is the one flag that changes what is pinned.
//! - `has_bible`, `has_simplified` and `has_tq` record what the owner published when tC4
//!   looked. They are never gates, as in tC4: a set and the source panes take whatever
//!   `<lang>_ult`/`_glt`, `<lang>_ust`/`_gst` and `<lang>_tq` is installed, because a flag
//!   goes stale the day an owner publishes.
//! - `literal`: the literal Bible's name when it is not `<lang>_ult` or `<lang>_glt`
//!   (BSOJ publishes `ar_avd`).
//!
//! One entry differs from tC4 on purpose: ur-deva reads left to right and is named in
//! Devanagari (tC4 #701).

use std::fmt;

use tc_door43::{AccountName, RepoName, RepoRef};

use crate::pin::ResourcePin;
use crate::sets::{GatewayLanguage, LanguageSet, ProjectKind, SetSlot};
use crate::suite;

/// The direction the helps of a gateway language read in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Direction {
    Ltr,
    Rtl,
}

/// One gateway language package: a language and the Door43 owner that publishes it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Gateway {
    /// BCP 47, as Door43 reports it.
    pub language: &'static str,
    pub owner: &'static str,
    /// The English name.
    pub name: &'static str,
    /// The language's own name, which every screen shows first (D94).
    pub autonym: &'static str,
    /// The direction of the language's script, which its helps read in. Where the installed
    /// helps' own manifest states a direction, a helps pane prefers it
    /// (`tc_helps::metadata::ResourceIdentity::direction`; unit B3, tC4 #701).
    pub direction: Direction,
    pub has_twl: bool,
    pub has_bible: bool,
    pub has_tq: bool,
    pub has_simplified: bool,
    /// The literal Bible's repository when it is not `<lang>_ult` or `<lang>_glt`.
    pub literal: Option<&'static str>,
    /// The releases of the owner's notes, words and academy that tC4 saw when it built the
    /// list, which Get source texts shows under each package (`seen`).
    pub seen: Seen,
}

/// The releases of a package's notes, words and academy, as tC4's registry records them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Seen {
    pub tn: &'static str,
    pub tw: &'static str,
    pub ta: &'static str,
}

const fn gl(
    language: &'static str,
    owner: &'static str,
    name: &'static str,
    autonym: &'static str,
    direction: Direction,
    (tn, tw, ta): (&'static str, &'static str, &'static str),
) -> Gateway {
    Gateway {
        language,
        owner,
        name,
        autonym,
        direction,
        has_twl: true,
        has_bible: true,
        has_tq: true,
        has_simplified: true,
        literal: None,
        seen: Seen { tn, tw, ta },
    }
}

const BCS: &str = "translationCore-Create-BCS";

/// tC4's registry (`gateways.ts`), in its order.
pub const GATEWAYS: [Gateway; 18] = [
    gl(
        "en",
        "unfoldingWord",
        "English",
        "English",
        Direction::Ltr,
        ("v89", "v89", "v89"),
    ),
    gl(
        "es-419",
        "es-419_gl",
        "Spanish (Latin American)",
        "Español (Latinoamérica)",
        Direction::Ltr,
        ("v66", "v37", "v4"),
    ),
    Gateway {
        has_bible: false,
        has_simplified: false,
        ..gl(
            "fr",
            "Xenizo",
            "French",
            "Français",
            Direction::Ltr,
            ("v2.0", "v2.0", "v2.0"),
        )
    },
    gl(
        "fr",
        "MVHS",
        "French",
        "Français",
        Direction::Ltr,
        ("v0.0.2", "v0.0.1", "v0.0.2"),
    ),
    Gateway {
        has_simplified: false,
        ..gl(
            "ru",
            "ru_gl",
            "Russian",
            "Русский",
            Direction::Ltr,
            ("v60.1", "v14.2", "v13.3"),
        )
    },
    gl(
        "fa",
        "fa_gl",
        "Persian",
        "فارسی",
        Direction::Rtl,
        ("v9", "v9", "v7.8"),
    ),
    Gateway {
        literal: Some("ar_avd"),
        ..gl(
            "ar",
            "BSOJ",
            "Arabic",
            "العربية",
            Direction::Rtl,
            ("v6.1", "v5", "v3"),
        )
    },
    gl(
        "id",
        "bahtraku",
        "Indonesian",
        "Bahasa Indonesia",
        Direction::Ltr,
        ("v25", "v26", "v30"),
    ),
    gl(
        "hi",
        BCS,
        "Hindi",
        "हिन्दी",
        Direction::Ltr,
        ("v9", "v8", "v7"),
    ),
    gl(
        "bn",
        BCS,
        "Bengali",
        "বাংলা",
        Direction::Ltr,
        ("v6", "v4", "v5.0"),
    ),
    gl(
        "gu",
        BCS,
        "Gujarati",
        "ગુજરાતી",
        Direction::Ltr,
        ("v6", "v4", "v4"),
    ),
    gl(
        "kn",
        BCS,
        "Kannada",
        "ಕನ್ನಡ",
        Direction::Ltr,
        ("v6", "v4", "v4"),
    ),
    gl(
        "mr",
        BCS,
        "Marathi",
        "मराठी",
        Direction::Ltr,
        ("v6", "v4", "v4"),
    ),
    gl(
        "ne",
        BCS,
        "Nepali",
        "नेपाली",
        Direction::Ltr,
        ("v7", "v5", "v5"),
    ),
    gl("or", BCS, "Odia", "ଓଡ଼ିଆ", Direction::Ltr, ("v5", "v4", "v4")),
    gl(
        "te",
        BCS,
        "Telugu",
        "తెలుగు",
        Direction::Ltr,
        ("v6", "v4", "v4"),
    ),
    Gateway {
        has_twl: false,
        has_simplified: false,
        ..gl(
            "ml",
            "Door43-Catalog",
            "Malayalam",
            "മലയാളം",
            Direction::Ltr,
            ("v14.2", "v8.2", "v10.3"),
        )
    },
    // tC4 lists ur-deva as right to left and names it اردو, in Arabic script. Devanagari reads
    // left to right, and the language's name in it is उर्दू (tC4 #701; docs/PLAN.md).
    Gateway {
        has_twl: false,
        has_simplified: false,
        ..gl(
            "ur-deva",
            "Door43-Catalog",
            "Urdu (Devanagari)",
            "उर्दू",
            Direction::Ltr,
            ("v14.1", "v8.3", "v10.1"),
        )
    },
];

/// The registry key of a gateway: `<language>::<owner>` (tC4 `gatewayKey`).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct GatewayKey(String);

impl fmt::Display for GatewayKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// The helps suffixes a repository name carries after `<language>_` (tC4 `orgForRepoName`).
const SUFFIXES: [&str; 11] = [
    "tn", "tw", "twl", "ta", "tq", "gst", "ust", "obs", "obs-tn", "obs-twl", "obs-tq",
];

impl Gateway {
    pub fn key(&self) -> GatewayKey {
        GatewayKey(format!("{}::{}", self.language, self.owner))
    }

    /// The gateway as a language set names it.
    pub fn gateway_language(&self) -> GatewayLanguage {
        GatewayLanguage {
            language: self.language.to_owned(),
            owner: account(self.owner),
        }
    }

    /// The repository this owner publishes under `name`.
    fn repo(&self, name: &str) -> Option<RepoRef> {
        Some(RepoRef::new(account(self.owner), name.parse().ok()?))
    }

    fn named(&self, suffix: &str) -> Option<RepoRef> {
        self.repo(&format!("{}_{suffix}", self.language))
    }

    /// The repository a language set of this gateway pins in `slot`, by the owner's
    /// naming. `None` when the owner publishes nothing for the slot. The words links of
    /// an owner without `_twl` are the bundled English `en_twl` (D97 ruling 7). The
    /// image pack has no name rule: it is chosen by flavor.
    pub fn repo_for(&self, slot: SetSlot) -> Option<RepoRef> {
        match slot {
            SetSlot::TranslationNotes => self.named("tn"),
            SetSlot::TranslationWordsLinks if self.has_twl => self.named("twl"),
            SetSlot::TranslationWordsLinks => Some(suite::english_twl().repo.repo().clone()),
            SetSlot::TranslationWords => self.named("tw"),
            SetSlot::TranslationAcademy => self.named("ta"),
            SetSlot::TranslationQuestions => self.named("tq"),
            SetSlot::SimplifiedText => self.simplified_candidates().into_iter().next(),
            SetSlot::Obs => self.named("obs"),
            SetSlot::ObsNotes => self.named("obs-tn"),
            SetSlot::ObsWordsLinks => self.named("obs-twl"),
            SetSlot::ObsQuestions => self.named("obs-tq"),
            SetSlot::ObsImages => None,
        }
    }

    /// The names the literal Bible may have, most specific first: the registry's own
    /// name, then `<lang>_ult`, then `<lang>_glt` (tC4 `gatewayBiblesFromInstalled`).
    pub fn literal_candidates(&self) -> Vec<RepoRef> {
        let mut out: Vec<RepoRef> = self
            .literal
            .and_then(|n| self.repo(n))
            .into_iter()
            .collect();
        out.extend(self.named("ult"));
        out.extend(self.named("glt"));
        out
    }

    /// The names the simplified Bible may have: `<lang>_ust`, then `<lang>_gst`.
    pub fn simplified_candidates(&self) -> Vec<RepoRef> {
        self.named("ust")
            .into_iter()
            .chain(self.named("gst"))
            .collect()
    }
}

fn account(name: &str) -> AccountName {
    match name.parse() {
        Ok(owner) => owner,
        // Every owner in GATEWAYS is a valid Door43 account name; the unit test
        // `every_registry_name_parses` proves it.
        Err(_) => unreachable!("registry owner {name} must parse"),
    }
}

/// The gateway with this language and owner. The owner compares without regard to case,
/// as Door43 resolves it.
pub fn find(language: &str, owner: &str) -> Option<&'static Gateway> {
    GATEWAYS
        .iter()
        .find(|g| g.language == language && g.owner.eq_ignore_ascii_case(owner))
}

/// The gateway a language set names, when the registry has it.
pub fn of_set(set: &LanguageSet) -> Option<&'static Gateway> {
    let gateway = set.gateway.named()?;
    find(&gateway.language, gateway.owner.as_str())
}

/// English, the bundled suite's language and the fallback rung's (D30.2).
pub fn english() -> &'static Gateway {
    &GATEWAYS[0]
}

/// The one name screens show for a gateway language: its autonym. A language the
/// registry does not name shows its tag (tC4 `gatewayDisplayName`).
pub fn display_name(language: &str) -> String {
    GATEWAYS
        .iter()
        .find(|g| g.language == language)
        .map_or_else(|| language.to_owned(), |g| g.autonym.to_owned())
}

/// The owner that publishes a helps repository of this name, when exactly one registry
/// owner publishes its language (tC4 `orgForRepoName`). `fr_tn` is published by two
/// owners, so it names none. A Door43 export's metadata records the owner at export time,
/// which an organization rename makes stale (`Idiomas-Puentes`); the registry owner is
/// the address to use where it is unambiguous.
pub fn owner_for_repo_name(name: &RepoName) -> Option<&'static str> {
    let name = name.as_str();
    let language = SUFFIXES.iter().find_map(|suffix| {
        name.strip_suffix(suffix)
            .and_then(|rest| rest.strip_suffix('_'))
            .filter(|lang| !lang.is_empty())
    })?;
    let mut owners: Vec<&'static str> = GATEWAYS
        .iter()
        .filter(|g| g.language == language)
        .map(|g| g.owner)
        .collect();
    owners.dedup();
    match owners.as_slice() {
        [one] => Some(one),
        _ => None,
    }
}

/// A pin among `installed` whose repository is `repo`, preferring the bundled suite's
/// identity when several commits are installed (tC4 `preferredNamedPin`), else the first.
/// A pin with no flavor is never taken: tC4 pins only what its metadata identifies.
fn preferred<'a>(installed: &'a [ResourcePin], repo: &RepoRef) -> Option<&'a ResourcePin> {
    let mut matches = installed
        .iter()
        .filter(|p| p.flavor.is_some() && p.is_repo(repo));
    let first = matches.next()?;
    let bundled = suite::bundled_pins();
    let is_bundled = |pin: &ResourcePin| bundled.iter().any(|b| b.same_identity(pin));
    if is_bundled(first) {
        return Some(first);
    }
    Some(matches.find(|p| is_bundled(p)).unwrap_or(first))
}

/// The language set of one gateway, built from what is installed (tC4
/// `languageSetFromInstalled`). `None` when a slot a set of `kind` needs is not
/// installed: a partial suite is never pinned (§5.3). Every slot is matched by the full
/// `<language>_<suffix>` name, so a multi-language owner never yields a mixed set. The
/// optional slots (questions, the simplified Bible, Open Bible Stories members, one image
/// pack) are pinned when installed.
pub fn language_set_from_installed(
    installed: &[ResourcePin],
    gateway: &Gateway,
    kind: ProjectKind,
) -> Option<LanguageSet> {
    let pick = |slot: SetSlot| {
        gateway
            .repo_for(slot)
            .and_then(|repo| preferred(installed, &repo))
    };
    let mut set = LanguageSet::new(gateway.gateway_language());
    for slot in LanguageSet::required(kind) {
        let pin = pick(*slot)?;
        set.pins.insert(*slot, pin.clone());
    }
    let optional = [
        SetSlot::TranslationNotes,
        SetSlot::TranslationQuestions,
        SetSlot::Obs,
        SetSlot::ObsNotes,
        SetSlot::ObsWordsLinks,
        SetSlot::ObsQuestions,
    ];
    for slot in optional {
        if let Some(pin) = pick(slot) {
            set.pins.entry(slot).or_insert_with(|| pin.clone());
        }
    }
    if let Some(pin) = gateway
        .simplified_candidates()
        .iter()
        .find_map(|repo| preferred(installed, repo))
    {
        set.pins.insert(SetSlot::SimplifiedText, pin.clone());
    }
    let owner = account(gateway.owner);
    let mut packs = installed.iter().filter(|p| {
        p.repo.owner() == &owner
            && p.flavor
                .as_deref()
                .is_some_and(|f| f.ends_with("/x-obsimages"))
    });
    if let (Some(pack), None) = (packs.next(), packs.next()) {
        set.pins.insert(SetSlot::ObsImages, pack.clone());
    }
    Some(set)
}

/// The Bibles a gateway package supplies for the source panes, each when installed
/// (tC4 `gatewayBiblesFromInstalled`).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct GatewayBibles {
    pub literal: Option<ResourcePin>,
    pub simplified: Option<ResourcePin>,
}

pub fn gateway_bibles_from_installed(
    installed: &[ResourcePin],
    gateway: &Gateway,
) -> GatewayBibles {
    let first = |candidates: Vec<RepoRef>| {
        candidates
            .iter()
            .find_map(|repo| preferred(installed, repo))
            .cloned()
    };
    GatewayBibles {
        literal: first(gateway.literal_candidates()),
        simplified: first(gateway.simplified_candidates()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pin::ResourcePin;

    #[test]
    fn every_registry_name_parses_and_keys_are_unique() {
        let mut keys = std::collections::HashSet::new();
        for g in &GATEWAYS {
            assert!(g.owner.parse::<AccountName>().is_ok(), "{}", g.owner);
            assert!(keys.insert(g.key()), "duplicate {}", g.key());
            for slot in SetSlot::ALL {
                let _ = g.repo_for(slot);
            }
        }
        assert_eq!(GATEWAYS.len(), 18);
        assert_eq!(english().language, "en");
    }

    #[test]
    fn registry_quirks_match_tc4() {
        let xenizo = find("fr", "xenizo").unwrap();
        assert!(!xenizo.has_bible);
        assert_eq!(xenizo.literal_candidates()[0].to_string(), "Xenizo/fr_ult");
        assert_eq!(find("fr", "MVHS").unwrap().key().to_string(), "fr::MVHS");
        let bsoj = find("ar", "BSOJ").unwrap();
        assert_eq!(bsoj.direction, Direction::Rtl);
        assert_eq!(bsoj.literal_candidates()[0].to_string(), "BSOJ/ar_avd");
        let ml = find("ml", "Door43-Catalog").unwrap();
        assert_eq!(
            ml.repo_for(SetSlot::TranslationWordsLinks)
                .unwrap()
                .to_string(),
            "unfoldingWord/en_twl"
        );
        assert_eq!(
            ml.repo_for(SetSlot::TranslationWords).unwrap().to_string(),
            "Door43-Catalog/ml_tw"
        );
        assert_eq!(
            find("en", "unfoldingWord")
                .unwrap()
                .repo_for(SetSlot::SimplifiedText)
                .unwrap()
                .to_string(),
            "unfoldingWord/en_ust"
        );
        assert_eq!(display_name("es-419"), "Español (Latinoamérica)");
        assert_eq!(display_name("xx"), "xx");
    }

    /// A letter of a right-to-left script: Hebrew, Arabic, Syriac, Thaana, N'Ko and the
    /// supplements and presentation forms of these (Unicode bidi classes R and AL).
    fn is_rtl_letter(c: char) -> bool {
        matches!(
            c as u32,
            0x0590..=0x08FF | 0xFB1D..=0xFDFF | 0xFE70..=0xFEFF | 0x10800..=0x10FFF | 0x1E800..=0x1EFFF
        )
    }

    /// The BCP 47 script subtag of a tag (`deva` in `ur-deva`), when it names one.
    fn script_subtag(tag: &str) -> Option<&str> {
        tag.split('-')
            .skip(1)
            .find(|s| s.len() == 4 && s.chars().all(|c| c.is_ascii_alphabetic()))
    }

    /// Whether `c` belongs to `script`; `None` for a script this table does not know, so an
    /// entry with a new script subtag must add its script here.
    fn in_script(script: &str, c: char) -> Option<bool> {
        let cp = c as u32;
        Some(match script.to_ascii_lowercase().as_str() {
            "deva" => matches!(cp, 0x0900..=0x097F | 0xA8E0..=0xA8FF),
            "arab" => matches!(
                cp,
                0x0600..=0x06FF | 0x0750..=0x077F | 0x08A0..=0x08FF | 0xFB50..=0xFDFF | 0xFE70..=0xFEFF
            ),
            "latn" => c.is_ascii_alphabetic() || matches!(cp, 0x00C0..=0x024F),
            _ => return None,
        })
    }

    #[test]
    fn ur_deva_helps_read_left_to_right_and_are_named_in_devanagari() {
        let ur = find("ur-deva", "Door43-Catalog").unwrap();
        assert_eq!(ur.direction, Direction::Ltr);
        assert_eq!(ur.autonym, "\u{909}\u{930}\u{94d}\u{926}\u{942}");
        assert_eq!(ur.autonym, "उर्दू");
        assert_eq!(display_name("ur-deva"), "उर्दू");
    }

    #[test]
    fn every_gateway_reads_in_the_direction_of_its_script() {
        for g in &GATEWAYS {
            let letters: Vec<char> = g.autonym.chars().filter(|c| c.is_alphabetic()).collect();
            assert!(
                !letters.is_empty(),
                "{}: no letters in {}",
                g.key(),
                g.autonym
            );
            let rtl = letters.iter().filter(|c| is_rtl_letter(**c)).count();
            assert!(
                rtl == 0 || rtl == letters.len(),
                "{}: {} mixes directions",
                g.key(),
                g.autonym
            );
            let script = if rtl > 0 {
                Direction::Rtl
            } else {
                Direction::Ltr
            };
            assert_eq!(
                g.direction,
                script,
                "{}: {} is written in a {script:?} script",
                g.key(),
                g.autonym
            );
            if let Some(tag) = script_subtag(g.language) {
                for c in &letters {
                    assert_eq!(
                        in_script(tag, *c),
                        Some(true),
                        "{}: the tag names {tag}, the autonym {} has {c}",
                        g.key(),
                        g.autonym
                    );
                }
            }
        }
        let rtl: Vec<String> = GATEWAYS
            .iter()
            .filter(|g| g.direction == Direction::Rtl)
            .map(|g| g.key().to_string())
            .collect();
        assert_eq!(rtl, ["fa::fa_gl", "ar::BSOJ"]);
    }

    #[test]
    fn owner_for_repo_name_is_unique_or_none() {
        let name = |s: &str| s.parse::<RepoName>().unwrap();
        assert_eq!(owner_for_repo_name(&name("es-419_tn")), Some("es-419_gl"));
        assert_eq!(owner_for_repo_name(&name("hi_obs-twl")), Some(BCS));
        assert_eq!(owner_for_repo_name(&name("fr_tn")), None);
        assert_eq!(owner_for_repo_name(&name("es-419_glt")), None);
        assert_eq!(owner_for_repo_name(&name("xx_tn")), None);
    }

    fn pin(path: &str, sha: char, flavor: &str) -> ResourcePin {
        ResourcePin::new(
            path.parse().unwrap(),
            sha.to_string().repeat(40).parse().unwrap(),
        )
        .with_flavor(flavor)
    }

    #[test]
    fn a_set_from_installed_matches_full_names_and_never_mixes_languages() {
        let installed = vec![
            pin(
                "git.door43.org/translationCore-Create-BCS/bn_tn",
                'a',
                "parascriptural/x-bcvnotes",
            ),
            pin(
                "git.door43.org/translationCore-Create-BCS/hi_twl",
                'b',
                "parascriptural/x-bcvarticles",
            ),
            pin(
                "git.door43.org/translationCore-Create-BCS/hi_tw",
                'c',
                "parascriptural/x-bcvarticles",
            ),
            pin(
                "git.door43.org/translationCore-Create-BCS/hi_ta",
                'd',
                "peripheral/x-peripheralArticles",
            ),
        ];
        let hi = find("hi", BCS).unwrap();
        assert!(language_set_from_installed(&installed, hi, ProjectKind::Bible).is_none());
        let mut with_tn = installed.clone();
        with_tn.push(pin(
            "git.door43.org/translationCore-Create-BCS/hi_tn",
            'e',
            "parascriptural/x-bcvnotes",
        ));
        let set = language_set_from_installed(&with_tn, hi, ProjectKind::Bible).unwrap();
        assert_eq!(
            set.pin(SetSlot::TranslationNotes).unwrap().sha.to_hex(),
            "e".repeat(40)
        );
        assert!(set.is_complete(ProjectKind::Bible));
    }

    #[test]
    fn an_owner_without_twl_pins_the_bundled_english_links() {
        let en_twl = suite::english_twl().clone();
        let installed = vec![
            pin(
                "git.door43.org/unfoldingWord/en_twl",
                '1',
                "parascriptural/x-bcvarticles",
            ),
            en_twl.clone(),
            pin(
                "git.door43.org/Door43-Catalog/ml_tn",
                'a',
                "parascriptural/x-bcvnotes",
            ),
            pin(
                "git.door43.org/Door43-Catalog/ml_tw",
                'b',
                "parascriptural/x-bcvarticles",
            ),
            pin(
                "git.door43.org/Door43-Catalog/ml_ta",
                'c',
                "peripheral/x-peripheralArticles",
            ),
        ];
        let ml = find("ml", "Door43-Catalog").unwrap();
        let set = language_set_from_installed(&installed, ml, ProjectKind::Bible).unwrap();
        assert_eq!(set.pin(SetSlot::TranslationWordsLinks), Some(&en_twl));
        assert_eq!(
            set.pin(SetSlot::TranslationWords).unwrap().repo.to_string(),
            "git.door43.org/Door43-Catalog/ml_tw"
        );
    }

    #[test]
    fn registry_flags_never_hide_what_is_installed() {
        // Xenizo published no Bible and ru_gl no simplified text when tC4 looked; both
        // are installed here, and both are used, as tC4 uses them.
        let xenizo = find("fr", "Xenizo").unwrap();
        let fr_glt = pin(
            "git.door43.org/Xenizo/fr_glt",
            'a',
            "scripture/textTranslation",
        );
        let bibles = gateway_bibles_from_installed(std::slice::from_ref(&fr_glt), xenizo);
        assert_eq!(bibles.literal, Some(fr_glt));

        let ru = find("ru", "ru_gl").unwrap();
        assert!(!ru.has_simplified);
        let installed: Vec<ResourcePin> = [
            ("ru_tn", "parascriptural/x-bcvnotes"),
            ("ru_twl", "parascriptural/x-bcvarticles"),
            ("ru_tw", "parascriptural/x-bcvarticles"),
            ("ru_ta", "peripheral/x-peripheralArticles"),
            ("ru_tq", "parascriptural/x-bcvquestions"),
            ("ru_gst", "scripture/textTranslation"),
        ]
        .into_iter()
        .map(|(name, flavor)| pin(&format!("git.door43.org/ru_gl/{name}"), 'b', flavor))
        .collect();
        let set = language_set_from_installed(&installed, ru, ProjectKind::Bible).unwrap();
        let name = |slot| set.pin(slot).map(|p| p.repo.name().to_string());
        assert_eq!(name(SetSlot::SimplifiedText).as_deref(), Some("ru_gst"));
        assert_eq!(
            name(SetSlot::TranslationQuestions).as_deref(),
            Some("ru_tq")
        );
        assert_eq!(
            gateway_bibles_from_installed(&installed, ru)
                .simplified
                .map(|p| p.repo.name().to_string())
                .as_deref(),
            Some("ru_gst")
        );
    }

    #[test]
    fn gateway_bibles_take_the_registry_literal_name_first() {
        let installed = vec![
            pin(
                "git.door43.org/BSOJ/ar_glt",
                'a',
                "scripture/textTranslation",
            ),
            pin(
                "git.door43.org/BSOJ/ar_avd",
                'b',
                "scripture/textTranslation",
            ),
            pin(
                "git.door43.org/BSOJ/ar_gst",
                'c',
                "scripture/textTranslation",
            ),
        ];
        let bibles = gateway_bibles_from_installed(&installed, find("ar", "BSOJ").unwrap());
        assert_eq!(bibles.literal.unwrap().repo.name().as_str(), "ar_avd");
        assert_eq!(bibles.simplified.unwrap().repo.name().as_str(), "ar_gst");
    }
}
