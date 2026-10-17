//! Newer releases of what a project pins (tC4 `upgrade.ts`, J12, D72 point 5): found on
//! demand while the internet is on, described per language set and per text, and never
//! installed here.
//!
//! - A help set is offered as one step per rung; its pins move only after every
//!   resource of the release is installed and verified ([`crate::install_all`]), so a
//!   failed download changes nothing. A rung's upgrade leaves the other rung, the texts
//!   and the panes as they were.
//! - Each scripture text (original language, source panes, simplified Bibles) is offered
//!   per repository. An original-language upgrade invalidates the alignments of the
//!   books it covers; they are kept, and compute as invalid against the new pin (STORAGE
//!   §5.8, §7.7).
//! - A release is newer when it names another commit than the pin (D58): the label is not
//!   compared. A repository the catalog says nothing about is neither offered nor current;
//!   it is listed as unanswered, never guessed.
//!
//! Door43 releases come from the catalog: one search per owner, at stage `prod`, so
//! pre-releases are never offered. Aquifer releases come from GitHub, per asset.

use std::collections::BTreeMap;

use tc_core::Testament;
use tc_door43::{
    AccountName, CatalogQuery, CatalogRef, Door43Client, Door43Error, GitOid, RepoRef, Stage,
    TagName,
};
use tc_helps::aquifer::AquiferHelpsKind;

use crate::aquifer::AquiferClient;
use crate::choice::AquiferHelps;
use crate::error::AquiferError;
use crate::pin::{AquiferPin, RepoPath, ResourcePin};
use crate::sets::{LanguageSet, PaneId, ProjectPins, Rung, SetSlot};

/// What the catalog reports as a repository's newest production release.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReleaseInfo {
    pub tag: TagName,
    pub sha: GitOid,
    pub published: Option<String>,
}

/// The newest release of each repository asked about, and those the catalog does not
/// list.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LatestReleases {
    found: BTreeMap<RepoRef, ReleaseInfo>,
    pub not_in_catalog: Vec<RepoRef>,
}

impl LatestReleases {
    pub fn get(&self, repo: &RepoRef) -> Option<&ReleaseInfo> {
        self.found.get(repo)
    }

    pub fn insert(&mut self, repo: RepoRef, info: ReleaseInfo) {
        self.found.insert(repo, info);
    }
}

/// Asks the catalog for the newest production release of each repository, one search
/// per owner.
pub async fn latest_releases(
    client: &Door43Client,
    repos: &[RepoRef],
) -> Result<LatestReleases, Door43Error> {
    let mut by_owner: BTreeMap<AccountName, Vec<RepoRef>> = BTreeMap::new();
    for repo in repos {
        let list = by_owner.entry(repo.owner.clone()).or_default();
        if !list.contains(repo) {
            list.push(repo.clone());
        }
    }
    let mut out = LatestReleases::default();
    for (owner, wanted) in by_owner {
        let query = CatalogQuery {
            owners: vec![owner],
            repos: wanted.iter().map(|r| r.name.clone()).collect(),
            stage: Some(Stage::Prod),
            ..CatalogQuery::default()
        };
        let listing = client.catalog_search(&query).await?;
        for entry in listing.items {
            let CatalogRef::Tag(tag) = entry.git_ref else {
                continue;
            };
            let Some(repo) = wanted.iter().find(|r| **r == entry.repo) else {
                continue;
            };
            let newer = out.found.get(repo).is_none_or(|held| {
                entry.released.as_deref().unwrap_or("") > held.published.as_deref().unwrap_or("")
            });
            if newer {
                out.found.insert(
                    repo.clone(),
                    ReleaseInfo {
                        tag,
                        sha: entry.commit,
                        published: entry.released,
                    },
                );
            }
        }
        for repo in wanted {
            if !out.found.contains_key(&repo) {
                out.not_in_catalog.push(repo);
            }
        }
    }
    Ok(out)
}

/// One repository of a set with a newer release than its pin. `to` keeps the pin's
/// repository casing and flavor and records no books; the caller records coverage from
/// the installed copy.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RepoUpgrade {
    pub slots: Vec<SetSlot>,
    pub from: ResourcePin,
    pub to: ResourcePin,
    pub published: Option<String>,
}

/// One rung's offer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SetOffer {
    pub rung: Rung,
    pub upgrades: Vec<RepoUpgrade>,
    /// Repositories already at the newest release.
    pub current: Vec<RepoPath>,
    /// Repositories the catalog said nothing about.
    pub unanswered: Vec<RepoPath>,
}

impl SetOffer {
    pub fn is_empty(&self) -> bool {
        self.upgrades.is_empty()
    }
}

fn upgraded_pin(pin: &ResourcePin, info: &ReleaseInfo) -> ResourcePin {
    let mut to = ResourcePin::new(pin.repo.clone(), info.sha).with_version(info.tag.clone());
    to.flavor = pin.flavor.clone();
    to
}

/// The offer for one rung's set (tC4 `offerForSet`). An image pack may be maintained by
/// commit only, so its absence from the catalog is not reported (tC4 #288).
pub fn offer_for_set(rung: Rung, set: &LanguageSet, latest: &LatestReleases) -> SetOffer {
    let mut offer = SetOffer {
        rung,
        upgrades: Vec::new(),
        current: Vec::new(),
        unanswered: Vec::new(),
    };
    for (pin, slots) in set.help_repos() {
        let Some(info) = latest.get(pin.repo.repo()) else {
            if !slots.iter().all(|s| *s == SetSlot::ObsImages) {
                offer.unanswered.push(pin.repo.clone());
            }
            continue;
        };
        if info.sha == pin.sha {
            offer.current.push(pin.repo.clone());
            continue;
        }
        offer.upgrades.push(RepoUpgrade {
            to: upgraded_pin(&pin, info),
            from: pin,
            slots,
            published: info.published.clone(),
        });
    }
    offer
}

/// The set with every offered repository re-pinned; other slots keep their pins.
pub fn upgraded_set(set: &LanguageSet, offer: &SetOffer) -> LanguageSet {
    let mut next = set.clone();
    for upgrade in &offer.upgrades {
        for slot in &upgrade.slots {
            next.pins.insert(*slot, upgrade.to.clone());
        }
    }
    next
}

/// The pins with one rung upgraded. The other rung, the texts and the panes are not
/// touched by a help-set upgrade (D72).
pub fn apply_upgrade(pins: &ProjectPins, offer: &SetOffer) -> ProjectPins {
    let mut next = pins.clone();
    if let Some(set) = pins.set(offer.rung) {
        *next.set_mut(offer.rung) = Some(upgraded_set(set, offer));
    }
    next
}

/// Whether an offer no longer describes this set: some slot it would move does not hold
/// the pin the offer was computed from (another project, or pins that moved since).
pub fn offer_is_stale(offer: &SetOffer, set: Option<&LanguageSet>) -> bool {
    offer.upgrades.iter().any(|u| {
        u.slots.iter().any(|slot| {
            set.and_then(|s| s.pin(*slot))
                .is_none_or(|current| !current.same_identity(&u.from))
        })
    })
}

/// A place in the pins that holds a scripture text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TextSlot {
    OriginalLanguage(Testament),
    Pane(PaneId),
    Simplified(Rung),
}

/// `Original` is the text alignments point at; `Gateway` is a source Bible.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextKind {
    Original,
    Gateway,
}

/// One text repository and every slot that pins it, with the pin each slot holds.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextRepo {
    pub kind: TextKind,
    pub slots: Vec<(TextSlot, ResourcePin)>,
}

fn text_pin<'a>(pins: &'a ProjectPins, slot: &TextSlot) -> Option<&'a ResourcePin> {
    match slot {
        TextSlot::OriginalLanguage(t) => pins.original_language.get(*t),
        TextSlot::Pane(id) => pins
            .extra_scripture
            .iter()
            .find(|p| p.id == *id)
            .map(|p| &p.pin),
        TextSlot::Simplified(rung) => pins.set(*rung)?.pin(SetSlot::SimplifiedText),
    }
}

/// One entry per text repository the pins name (tC4 `textReposOf`): the
/// original-language texts, the panes and each rung's simplified Bible. Slots that pin
/// one repository fold together (`en_ust` is often a pane and the simplified text).
pub fn text_repos_of(pins: &ProjectPins) -> Vec<TextRepo> {
    let mut slots: Vec<(TextSlot, TextKind)> = vec![
        (
            TextSlot::OriginalLanguage(Testament::New),
            TextKind::Original,
        ),
        (
            TextSlot::OriginalLanguage(Testament::Old),
            TextKind::Original,
        ),
    ];
    slots.extend(
        pins.extra_scripture
            .iter()
            .map(|p| (TextSlot::Pane(p.id.clone()), TextKind::Gateway)),
    );
    slots.extend(
        Rung::LADDER
            .into_iter()
            .map(|r| (TextSlot::Simplified(r), TextKind::Gateway)),
    );
    let mut out: Vec<TextRepo> = Vec::new();
    for (slot, kind) in slots {
        let Some(pin) = text_pin(pins, &slot) else {
            continue;
        };
        match out
            .iter_mut()
            .find(|r| r.slots.first().is_some_and(|(_, p)| p.repo == pin.repo))
        {
            Some(repo) => repo.slots.push((slot, pin.clone())),
            None => out.push(TextRepo {
                kind,
                slots: vec![(slot, pin.clone())],
            }),
        }
    }
    out
}

/// One text repository with a release newer than some slot's pin.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextOffer {
    pub kind: TextKind,
    /// Each slot and the pin it held when the offer was computed.
    pub slots: Vec<(TextSlot, ResourcePin)>,
    /// The first pin that differs from the release, for the offer row.
    pub from: ResourcePin,
    pub to: ResourcePin,
    pub published: Option<String>,
}

/// The text offers (tC4 `textOffers`). An original-language text is offered only for a
/// testament the project's books use (`testaments`).
pub fn text_offers(
    pins: &ProjectPins,
    testaments: &[Testament],
    latest: &LatestReleases,
) -> (Vec<TextOffer>, Vec<RepoPath>) {
    let mut offers = Vec::new();
    let mut unanswered = Vec::new();
    for repo in text_repos_of(pins) {
        let used = repo.slots.iter().any(|(slot, _)| match slot {
            TextSlot::OriginalLanguage(t) => testaments.contains(t),
            _ => true,
        });
        let Some((_, first)) = repo.slots.first() else {
            continue;
        };
        if !used {
            continue;
        }
        let Some(info) = latest.get(first.repo.repo()) else {
            unanswered.push(first.repo.clone());
            continue;
        };
        let Some((_, behind)) = repo.slots.iter().find(|(_, p)| p.sha != info.sha) else {
            continue;
        };
        offers.push(TextOffer {
            kind: repo.kind,
            from: behind.clone(),
            to: upgraded_pin(first, info),
            slots: repo.slots.clone(),
            published: info.published.clone(),
        });
    }
    (offers, unanswered)
}

/// The pins with the offer's slots moved to its release; every other pin is kept.
pub fn apply_text_upgrade(pins: &ProjectPins, offer: &TextOffer) -> ProjectPins {
    let mut next = pins.clone();
    for (slot, _) in &offer.slots {
        match slot {
            TextSlot::OriginalLanguage(t) => next.original_language.set(*t, Some(offer.to.clone())),
            TextSlot::Pane(id) => {
                if let Some(pane) = next.extra_scripture.iter_mut().find(|p| p.id == *id) {
                    pane.pin = offer.to.clone();
                }
            }
            TextSlot::Simplified(rung) => {
                if let Some(set) = next.set_mut(*rung).as_mut() {
                    set.pins.insert(SetSlot::SimplifiedText, offer.to.clone());
                }
            }
        }
    }
    next
}

/// Whether some slot of the offer no longer holds the pin it was computed from.
pub fn text_offer_is_stale(offer: &TextOffer, pins: &ProjectPins) -> bool {
    offer
        .slots
        .iter()
        .any(|(slot, was)| text_pin(pins, slot).is_none_or(|current| !current.same_identity(was)))
}

/// The testaments whose alignments an offer invalidates: the pinned testament of an
/// original-language text, none for a gateway Bible (D72).
pub fn invalidated_testaments(offer: &TextOffer) -> Vec<Testament> {
    offer
        .slots
        .iter()
        .filter_map(|(slot, _)| match slot {
            TextSlot::OriginalLanguage(t) => Some(*t),
            _ => None,
        })
        .collect()
}

/// Everything a "Check for updates" finds for a project.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UpdateReport {
    /// One offer per rung that pins a set.
    pub sets: Vec<SetOffer>,
    pub texts: Vec<TextOffer>,
    /// Text repositories the catalog said nothing about.
    pub unanswered_texts: Vec<RepoPath>,
}

/// Finds newer releases of everything the project pins, without installing anything.
/// `testaments` are those of the project's books.
pub async fn check_for_updates(
    client: &Door43Client,
    pins: &ProjectPins,
    testaments: &[Testament],
) -> Result<UpdateReport, Door43Error> {
    let mut repos: Vec<RepoRef> = Vec::new();
    for rung in Rung::LADDER {
        if let Some(set) = pins.set(rung) {
            repos.extend(
                set.help_repos()
                    .into_iter()
                    .map(|(p, _)| p.repo.repo().clone()),
            );
        }
    }
    for repo in text_repos_of(pins) {
        repos.extend(repo.slots.first().map(|(_, p)| p.repo.repo().clone()));
    }
    let latest = latest_releases(client, &repos).await?;
    let sets = Rung::LADDER
        .into_iter()
        .filter_map(|rung| Some(offer_for_set(rung, pins.set(rung)?, &latest)))
        .collect();
    let (texts, unanswered_texts) = text_offers(pins, testaments, &latest);
    Ok(UpdateReport {
        sets,
        texts,
        unanswered_texts,
    })
}

/// What GitHub's newest release means for one Aquifer pin.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AquiferUpdate {
    /// The newest release's asset is the pinned one.
    Current,
    /// A newer release, or a re-uploaded asset: the pin to move to after it installs.
    Offer {
        to: AquiferPin,
        published: Option<String>,
    },
    /// The newest release cannot be pinned: it lacks the asset, or GitHub lists no
    /// digest for it.
    CannotPin { tag: TagName, reason: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AquiferOffer {
    pub kind: AquiferHelpsKind,
    pub from: AquiferPin,
    pub update: AquiferUpdate,
}

/// Finds newer Aquifer releases for each pinned asset, without installing anything. An
/// asset is newer when its commit or digest differs from the pin's.
pub async fn check_aquifer_updates(
    client: &AquiferClient,
    helps: &AquiferHelps,
) -> Result<Vec<AquiferOffer>, AquiferError> {
    let mut out = Vec::new();
    for (kind, pin) in helps.pins() {
        let release = client.latest_release(&pin.repo).await?;
        let update = match release.asset(&pin.asset) {
            None => AquiferUpdate::CannotPin {
                tag: release.tag.clone(),
                reason: format!("the release has no asset {}", pin.asset),
            },
            Some(asset) => match asset.sha256 {
                None => AquiferUpdate::CannotPin {
                    tag: release.tag.clone(),
                    reason: "GitHub lists no SHA-256 digest for the asset".to_owned(),
                },
                Some(digest) => {
                    let commit = client.tag_commit(&pin.repo, &release.tag).await?;
                    if commit == pin.commit && digest == pin.sha256 {
                        AquiferUpdate::Current
                    } else {
                        AquiferUpdate::Offer {
                            to: AquiferPin {
                                tag: release.tag.clone(),
                                commit,
                                sha256: digest,
                                ..pin.clone()
                            },
                            published: release.published_at.clone(),
                        }
                    }
                }
            },
        };
        out.push(AquiferOffer {
            kind,
            from: pin.clone(),
            update,
        });
    }
    Ok(out)
}

/// The Aquifer pins with an offer applied, when the slot still holds the pin the offer
/// was computed from; `None` when the offer is stale or offers nothing.
pub fn apply_aquifer_upgrade(helps: &AquiferHelps, offer: &AquiferOffer) -> Option<AquiferHelps> {
    let AquiferUpdate::Offer { to, .. } = &offer.update else {
        return None;
    };
    let mut next = helps.clone();
    let slot = match offer.kind {
        AquiferHelpsKind::Notes => &mut next.notes,
        AquiferHelpsKind::Words => &mut next.words,
        AquiferHelpsKind::Academy => &mut next.academy,
        AquiferHelpsKind::Questions => &mut next.questions,
    };
    if slot.as_ref() != Some(&offer.from) {
        return None;
    }
    *slot = Some(to.clone());
    Some(next)
}

#[cfg(test)]
mod tests {
    //! tC4 `test/upgrade.test.ts` and `test/textUpgrade.test.ts`, ported.

    use super::*;
    use crate::sets::{GatewayLanguage, ScripturePane};

    /// A distinct commit per letter, as tC4's tests name them (`sha('e')`).
    fn sha(c: char) -> GitOid {
        crate::pin::Sha256::of(c.to_string().as_bytes()).to_hex()[..40]
            .parse()
            .unwrap()
    }

    fn pin(repo: &str, version: &str, s: char, flavor: &str) -> ResourcePin {
        ResourcePin::new(
            format!("git.door43.org/unfoldingWord/{repo}")
                .parse()
                .unwrap(),
            sha(s),
        )
        .with_version(version.parse().unwrap())
        .with_flavor(flavor)
    }

    fn en() -> LanguageSet {
        LanguageSet::new(GatewayLanguage {
            language: "en".to_owned(),
            owner: "unfoldingWord".parse().unwrap(),
        })
        .with(
            SetSlot::TranslationNotes,
            pin("en_tn", "v89", 'a', "parascriptural/x-bcvnotes"),
        )
        .with(
            SetSlot::TranslationWordsLinks,
            pin("en_twl", "v89", 'f', "parascriptural/x-bcvarticles"),
        )
        .with(
            SetSlot::TranslationWords,
            pin("en_tw", "v89", 'b', "parascriptural/x-bcvarticles"),
        )
        .with(
            SetSlot::TranslationAcademy,
            pin("en_ta", "v89", 'c', "peripheral/x-peripheralArticles"),
        )
        .with(
            SetSlot::TranslationQuestions,
            pin("en_tq", "v89", 'd', "parascriptural/x-bcvquestions"),
        )
    }

    fn latest(entries: &[(&str, &str, char)]) -> LatestReleases {
        let mut out = LatestReleases::default();
        for (repo, tag, s) in entries {
            out.insert(
                repo.parse().unwrap(),
                ReleaseInfo {
                    tag: tag.parse().unwrap(),
                    sha: sha(*s),
                    published: Some("2026-08-17T19:15:10Z".to_owned()),
                },
            );
        }
        out
    }

    #[test]
    fn the_simplified_bible_is_never_a_help_offer() {
        let with_ust = en().with(
            SetSlot::SimplifiedText,
            pin("en_ust", "v89", 'u', "scripture/textTranslation"),
        );
        let offer = offer_for_set(
            Rung::Primary,
            &with_ust,
            &latest(&[("unfoldingWord/en_ust", "v90", 'v')]),
        );
        assert!(offer.upgrades.is_empty());
        assert!(!offer.current.iter().any(|r| r.name().as_str() == "en_ust"));
        assert_eq!(upgraded_set(&with_ust, &offer), with_ust);
    }

    #[test]
    fn a_differing_label_with_the_same_commit_is_not_an_upgrade() {
        let offer = offer_for_set(
            Rung::Primary,
            &en(),
            &latest(&[("unfoldingWord/en_tn", "v89-rc", 'a')]),
        );
        assert!(offer.upgrades.is_empty());
        assert_eq!(offer.current.len(), 1);
        // The other repositories went unanswered and are said to.
        assert_eq!(offer.unanswered.len(), 4);
    }

    #[test]
    fn repositories_match_without_regard_to_case() {
        let offer = offer_for_set(
            Rung::Primary,
            &en(),
            &latest(&[("unfoldingword/EN_TN", "v90", 'e')]),
        );
        assert_eq!(offer.upgrades.len(), 1);
    }

    #[test]
    fn an_upgrade_moves_only_the_slots_naming_the_repository() {
        let set = en();
        let offer = offer_for_set(
            Rung::Primary,
            &set,
            &latest(&[("unfoldingWord/en_tw", "v90", 'g')]),
        );
        let next = upgraded_set(&set, &offer);
        let tw = next.pin(SetSlot::TranslationWords).unwrap();
        assert_eq!(
            (tw.version.as_ref().unwrap().as_str(), &tw.sha),
            ("v90", &sha('g'))
        );
        assert_eq!(tw.flavor.as_deref(), Some("parascriptural/x-bcvarticles"));
        for slot in [
            SetSlot::TranslationWordsLinks,
            SetSlot::TranslationNotes,
            SetSlot::TranslationAcademy,
            SetSlot::TranslationQuestions,
        ] {
            assert_eq!(next.pin(slot), set.pin(slot), "{slot}");
        }
    }

    #[test]
    fn each_rung_is_its_own_step_and_touches_nothing_else() {
        let pins = ProjectPins {
            primary: Some(en()),
            fallback: Some(en()),
            original_language: crate::sets::Testaments {
                nt: Some(pin(
                    "el-x-koine_ugnt",
                    "v0.34",
                    '9',
                    "scripture/textTranslation",
                )),
                ..Default::default()
            },
            lexicon: Default::default(),
            extra_scripture: vec![ScripturePane {
                id: "ult".parse().unwrap(),
                pin: pin("en_ult", "v89", '8', "scripture/textTranslation"),
            }],
            unreadable_panes: Vec::new(),
            unknown: Default::default(),
        };
        let releases = latest(&[("unfoldingWord/en_tn", "v90", 'e')]);
        let primary = offer_for_set(Rung::Primary, pins.primary.as_ref().unwrap(), &releases);
        let fallback = offer_for_set(Rung::Fallback, pins.fallback.as_ref().unwrap(), &releases);
        let after_primary = apply_upgrade(&pins, &primary);
        assert_eq!(
            after_primary
                .primary
                .as_ref()
                .unwrap()
                .pin(SetSlot::TranslationNotes)
                .unwrap()
                .sha,
            sha('e')
        );
        assert_eq!(after_primary.fallback, pins.fallback);
        assert_eq!(after_primary.original_language, pins.original_language);
        assert_eq!(after_primary.extra_scripture, pins.extra_scripture);
        let after_both = apply_upgrade(&after_primary, &fallback);
        assert_eq!(
            after_both
                .fallback
                .as_ref()
                .unwrap()
                .pin(SetSlot::TranslationNotes)
                .unwrap()
                .sha,
            sha('e')
        );
        assert_eq!(after_both.primary, after_primary.primary);
    }

    #[test]
    fn an_offer_applies_only_to_the_set_it_was_computed_from() {
        let set = en();
        let offer = offer_for_set(
            Rung::Primary,
            &set,
            &latest(&[("unfoldingWord/en_tn", "v90", 'e')]),
        );
        assert!(!offer_is_stale(&offer, Some(&set)));
        let other = en().with(SetSlot::TranslationNotes, pin("en_tn", "v88", 'z', ""));
        assert!(offer_is_stale(&offer, Some(&other)));
        assert!(offer_is_stale(&offer, None));
        assert!(offer_is_stale(&offer, Some(&upgraded_set(&set, &offer))));
    }

    #[test]
    fn a_commit_only_image_pack_missing_from_the_catalog_is_not_reported() {
        let set = en().with(SetSlot::ObsImages, crate::suite::obs_images().clone());
        let offer = offer_for_set(
            Rung::Primary,
            &set,
            &latest(&[("unfoldingWord/en_tn", "v90", 'e')]),
        );
        assert!(
            !offer
                .unanswered
                .iter()
                .any(|r| r.name().as_str() == "obs_images_360")
        );
        assert_eq!(offer.upgrades.len(), 1);
    }

    fn text_pins() -> ProjectPins {
        let mut pins = ProjectPins {
            primary: Some(en().with(
                SetSlot::SimplifiedText,
                pin("en_ust", "v89", 'u', "scripture/textTranslation"),
            )),
            fallback: Some(en().with(
                SetSlot::SimplifiedText,
                pin("en_ust", "v88", 't', "scripture/textTranslation"),
            )),
            ..ProjectPins::default()
        };
        pins.original_language.nt = Some(pin(
            "el-x-koine_ugnt",
            "v0.34",
            '1',
            "scripture/textTranslation",
        ));
        pins.original_language.ot =
            Some(pin("hbo_uhb", "v3.0.0", '2', "scripture/textTranslation"));
        pins.extra_scripture = vec![
            ScripturePane {
                id: "ult".parse().unwrap(),
                pin: pin("en_ult", "v89", '3', "scripture/textTranslation"),
            },
            ScripturePane {
                id: "ust".parse().unwrap(),
                pin: pin("en_ust", "v89", 'u', "scripture/textTranslation"),
            },
        ];
        pins
    }

    #[test]
    fn text_repositories_fold_every_slot_that_pins_them() {
        let repos = text_repos_of(&text_pins());
        assert_eq!(repos.len(), 4);
        let ust = repos
            .iter()
            .find(|r| r.slots[0].1.repo.name().as_str() == "en_ust")
            .unwrap();
        assert_eq!(ust.slots.len(), 3);
        assert_eq!(ust.kind, TextKind::Gateway);
    }

    #[test]
    fn text_offers_follow_the_projects_testaments_and_any_slot_behind() {
        let pins = text_pins();
        let releases = latest(&[
            ("unfoldingWord/el-x-koine_ugnt", "v0.35", '4'),
            ("unfoldingWord/hbo_uhb", "v3.0.1", '5'),
            ("unfoldingWord/en_ust", "v89", 'u'),
            ("unfoldingWord/en_ult", "v89", '3'),
        ]);
        let (offers, unanswered) = text_offers(&pins, &[Testament::New], &releases);
        assert!(unanswered.is_empty());
        // UHB is not asked about for a New Testament project; en_ust is offered because
        // the fallback's simplified text is behind, though the pane is current.
        let names: Vec<&str> = offers.iter().map(|o| o.to.repo.name().as_str()).collect();
        assert_eq!(names, ["el-x-koine_ugnt", "en_ust"]);
        let ugnt = &offers[0];
        assert_eq!(invalidated_testaments(ugnt), [Testament::New]);
        assert!(invalidated_testaments(&offers[1]).is_empty());

        let next = apply_text_upgrade(&pins, &offers[1]);
        assert_eq!(next.extra_scripture[1].pin.sha, sha('u'));
        assert_eq!(
            next.fallback
                .as_ref()
                .unwrap()
                .pin(SetSlot::SimplifiedText)
                .unwrap()
                .sha,
            sha('u')
        );
        assert_eq!(next.original_language, pins.original_language);
        assert!(!text_offer_is_stale(&offers[1], &pins));
        assert!(text_offer_is_stale(&offers[1], &next));
    }
}
