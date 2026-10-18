//! The English suite that ships with the app (tC4 `installedSuite.js`): exact pins, each
//! the commit the Door43 tags API names for the release, equal to the revision the
//! release's burrito export declares (tC4 verified them on 2026-10-01 and 2026-10-12).
//!
//! A new project starts with `primary` and `fallback` both this suite (§5.3 migration
//! rule); choosing a gateway language rewrites `primary` only. The words links come from
//! `en_twl` and the articles from `en_tw`, each at its own release (D96, D97): the `en_tw`
//! export also carries links, but Door43 builds them from `en_twl` master, so they are
//! never read. The lexicons have no tags and are pinned by commit only (D71).
//!
//! An installer pre-seeds these resources into a bundled store (see
//! [`crate::ResourceStore::seed_from`]), so a first run works offline.

use std::sync::LazyLock;

use crate::pin::ResourcePin;
use crate::sets::{
    GatewayLanguage, LanguageSet, PaneId, ProjectPins, ScripturePane, SetSlot, Testaments,
    UnknownKeys,
};

struct Suite {
    helps: LanguageSet,
    original_language: Testaments,
    lexicon: Testaments,
    extra_scripture: Vec<ScripturePane>,
    obs_images: ResourcePin,
    all: Vec<ResourcePin>,
}

/// `(repository, version, sha, flavor)`; an empty version is a pin by commit only.
type Row = (&'static str, &'static str, &'static str, &'static str);

const NOTES: &str = "parascriptural/x-bcvnotes";
const ARTICLES: &str = "parascriptural/x-bcvarticles";
const SCRIPTURE: &str = "scripture/textTranslation";

const HELPS: [(SetSlot, Row); 10] = [
    (
        SetSlot::TranslationNotes,
        (
            "unfoldingWord/en_tn",
            "v91",
            "e586762e330f482a60c52aedd1c7b3a2f155df8a",
            NOTES,
        ),
    ),
    (
        SetSlot::TranslationWordsLinks,
        (
            "unfoldingWord/en_twl",
            "v91",
            "7cbb3419f79780ce84b92ad7d42bd476cc5d8a64",
            ARTICLES,
        ),
    ),
    (
        SetSlot::TranslationWords,
        (
            "unfoldingWord/en_tw",
            "v91",
            "ff5b3852c27c3a0d01b109e482eb26047dcd20e2",
            ARTICLES,
        ),
    ),
    (
        SetSlot::TranslationAcademy,
        (
            "unfoldingWord/en_ta",
            "v91",
            "ce9a1bb9431317ca888e8c1f9620caa7f5fe45fd",
            "peripheral/x-peripheralArticles",
        ),
    ),
    (
        SetSlot::TranslationQuestions,
        (
            "unfoldingWord/en_tq",
            "v91",
            "8be02772584ff5a5fea893a392b4f019e3efcc77",
            "parascriptural/x-bcvquestions",
        ),
    ),
    (
        SetSlot::SimplifiedText,
        (
            "unfoldingWord/en_ust",
            "v91",
            "85f274a74245cb418f85266e1a5b524bc3e91e9c",
            SCRIPTURE,
        ),
    ),
    (
        SetSlot::Obs,
        (
            "unfoldingWord/en_obs",
            "v9",
            "d39a1dc7a7557ac54e4a8fecc3462147fe7eec3b",
            "gloss/textStories",
        ),
    ),
    (
        SetSlot::ObsNotes,
        (
            "unfoldingWord/en_obs-tn",
            "v13",
            "e86138ea13f619f09f7a6dcaa60592716d407fe4",
            "peripheral/x-obsnotes",
        ),
    ),
    (
        SetSlot::ObsWordsLinks,
        (
            "unfoldingWord/en_obs-twl",
            "v3",
            "44ebc9fafe8101665f985007d566f5036a2be85b",
            ARTICLES,
        ),
    ),
    (
        SetSlot::ObsQuestions,
        (
            "unfoldingWord/en_obs-tq",
            "v10",
            "01b92fe8793d62cff3a2221f5174c768cbad3dc1",
            "peripheral/x-obsquestions",
        ),
    ),
];

const UGNT: Row = (
    "unfoldingWord/el-x-koine_ugnt",
    "v0.34",
    "fc95b2b8aad08bb65ab54628ab685413a1139e97",
    SCRIPTURE,
);
const UHB: Row = (
    "unfoldingWord/hbo_uhb",
    "v3.0.0",
    "74022f0fed012a3ef169886f595dd98e7b200543",
    SCRIPTURE,
);
const UGL: Row = (
    "uW/en_ugl",
    "",
    "d9d29e2d589258ce27f92b59f753a3af03ab7a72",
    "peripheral/x-lexicon",
);
const UHL: Row = (
    "uW/en_uhl",
    "",
    "72df5ac25acf9d51e826b20e3ad883a5a657ef4e",
    "peripheral/x-lexicon",
);
const ULT: Row = (
    "unfoldingWord/en_ult",
    "v91",
    "35d215957f3203fd2e2fac5702ce14902d417f9d",
    SCRIPTURE,
);
const UST: Row = (
    "unfoldingWord/en_ust",
    "v91",
    "85f274a74245cb418f85266e1a5b524bc3e91e9c",
    SCRIPTURE,
);
const OBS_IMAGES: Row = (
    "uW/obs_images_360",
    "",
    "7146d5b504f6b63b9e11f7dc0b18c594d0ae179d",
    "peripheral/x-obsimages",
);

fn pin((repo, version, sha, flavor): Row) -> ResourcePin {
    let built = (|| {
        let mut pin = ResourcePin::new(
            format!("{}/{repo}", crate::pin::DCS_HOST).parse().ok()?,
            sha.parse().ok()?,
        )
        .with_flavor(flavor);
        if !version.is_empty() {
            pin = pin.with_version(version.parse().ok()?);
        }
        Some(pin)
    })();
    match built {
        Some(pin) => pin,
        // Every row above is a valid repository, tag and sha; the unit test
        // `the_suite_matches_tc4` builds them all.
        None => unreachable!("bundled pin {repo} must parse"),
    }
}

fn pane(id: &str, row: Row) -> ScripturePane {
    match id.parse::<PaneId>() {
        Ok(id) => ScripturePane { id, pin: pin(row) },
        // `ult` and `ust` are valid pane ids.
        Err(_) => unreachable!("pane id {id} must parse"),
    }
}

static SUITE: LazyLock<Suite> = LazyLock::new(|| {
    let mut helps = LanguageSet::new(GatewayLanguage {
        language: "en".to_owned(),
        owner: match "unfoldingWord".parse() {
            Ok(owner) => owner,
            Err(_) => unreachable!("unfoldingWord is an account name"),
        },
    });
    for (slot, row) in HELPS {
        helps.pins.insert(slot, pin(row));
    }
    let original_language = Testaments {
        nt: Some(pin(UGNT)),
        ot: Some(pin(UHB)),
        ..Testaments::default()
    };
    let lexicon = Testaments {
        nt: Some(pin(UGL)),
        ot: Some(pin(UHL)),
        ..Testaments::default()
    };
    let extra_scripture = vec![pane("ult", ULT), pane("ust", UST)];
    let obs_images = pin(OBS_IMAGES);
    let mut all: Vec<ResourcePin> = Vec::new();
    let candidates = helps
        .pins
        .values()
        .chain(original_language.nt.iter())
        .chain(original_language.ot.iter())
        .chain(lexicon.nt.iter())
        .chain(lexicon.ot.iter())
        .chain(extra_scripture.iter().map(|p| &p.pin))
        .chain(std::iter::once(&obs_images));
    for candidate in candidates {
        if !all.iter().any(|held| held.same_identity(candidate)) {
            all.push(candidate.clone());
        }
    }
    Suite {
        helps,
        original_language,
        lexicon,
        extra_scripture,
        obs_images,
        all,
    }
});

/// The bundled English help set (tC4 `EN_HELPS`).
pub fn english_helps() -> &'static LanguageSet {
    &SUITE.helps
}

/// The bundled `en_twl`, which an owner with no `_twl` pins (D97 ruling 7).
pub fn english_twl() -> &'static ResourcePin {
    match SUITE.helps.pin(SetSlot::TranslationWordsLinks) {
        Some(pin) => pin,
        // HELPS above pins the links slot.
        None => unreachable!("the bundled suite pins translationWordsLinks"),
    }
}

/// The default Open Bible Stories image pack (tC4 `EN_OBS_IMAGES`), pinned by commit.
pub fn obs_images() -> &'static ResourcePin {
    &SUITE.obs_images
}

/// The bundled English literal and simplified Bibles, which fill a source pane a gateway
/// package cannot (tC4 `sourcePanesForGateway`).
pub fn english_bibles() -> (&'static ResourcePin, &'static ResourcePin) {
    (&SUITE.extra_scripture[0].pin, &SUITE.extra_scripture[1].pin)
}

/// A new project's pins (tC4 `INSTALLED_SUITE`): both rungs the English suite, the
/// original-language texts, the lexicons and the `ult` and `ust` panes.
pub fn project_pins() -> ProjectPins {
    ProjectPins {
        primary: Some(SUITE.helps.clone()),
        fallback: Some(SUITE.helps.clone()),
        original_language: SUITE.original_language.clone(),
        lexicon: SUITE.lexicon.clone(),
        extra_scripture: SUITE.extra_scripture.clone(),
        unreadable_panes: Vec::new(),
        unknown: UnknownKeys::default(),
    }
}

/// Every distinct resource the app ships, once each: the sixteen repositories of
/// tC4's installer (the eleven-resource Bible suite, four Open Bible Stories resources
/// and the picture pack; `en_ust` is both a pane and the simplified text).
pub fn bundled_pins() -> &'static [ResourcePin] {
    &SUITE.all
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_suite_matches_tc4() {
        let pins = project_pins();
        assert_eq!(pins.primary, pins.fallback);
        let helps = english_helps();
        assert_eq!(helps.pins.len(), 10);
        assert_eq!(
            helps.pin(SetSlot::TranslationNotes).unwrap().sha.to_hex(),
            "e586762e330f482a60c52aedd1c7b3a2f155df8a"
        );
        assert_eq!(english_twl().repo.name().as_str(), "en_twl");
        assert!(pins.lexicon.nt.as_ref().unwrap().version.is_none());
        assert_eq!(
            pins.original_language
                .ot
                .as_ref()
                .unwrap()
                .version
                .as_ref()
                .unwrap()
                .as_str(),
            "v3.0.0"
        );
        assert_eq!(bundled_pins().len(), 16);
        assert_eq!(
            obs_images().repo.to_string(),
            "git.door43.org/uW/obs_images_360"
        );
        assert!(pins.old_twl_pin().is_none());
    }

    #[test]
    fn the_suite_round_trips_through_resources_json() {
        let pins = project_pins();
        let json = pins.to_resources_json().unwrap();
        assert_eq!(ProjectPins::from_resources_json(&json).unwrap(), pins);
        assert_eq!(
            json["languageSets"]["primary"]["translationNotes"]["flavor"],
            "parascriptural/x-bcvnotes"
        );
    }
}
