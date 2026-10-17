//! Whether a check tool can open on this computer, and the guided fix when its pinned
//! resource is missing (tC4 `preflightToolBook`, `preflightObsTool`, the `openFix`
//! action, `repinOffer` and `verifySideload`; D30.4, D30.5, D72 point 6).
//!
//! A missing resource never blocks other work: the tool is `Unavailable`, a state the
//! screen explains, and the guided fix offers three ways to get the exact pinned commit:
//! download it, move the pin to a version of the same repository that is installed, or
//! install it from a file that declares that commit.

use tc_helps::CommitSha;

use crate::key::{Collection, ToolId};
use crate::resolve::{
    Answer, Coverage, Pin, Pins, Resolution, ResourceRef, Rung, Slot, SlotPin, UnusablePin,
    answer_with, coverage_of,
};

/// A resource installed on this computer: one repository at one commit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Installed {
    pub resource: ResourceRef,
    pub version: Option<String>,
    /// The books the installed copy holds.
    pub coverage: Coverage,
}

/// What the computer holds and whether the internet switch is on.
#[derive(Debug, Clone, Copy)]
pub struct Machine<'a> {
    pub installed: &'a [Installed],
    pub online: bool,
}

impl Machine<'_> {
    /// True when exactly this commit of the repository is installed.
    pub fn has(&self, resource: &ResourceRef) -> bool {
        self.installed.iter().any(|i| i.resource == *resource)
    }

    fn local_coverage(&self, resource: &ResourceRef) -> Option<Coverage> {
        self.installed
            .iter()
            .find(|i| i.resource == *resource)
            .map(|i| i.coverage.clone())
    }
}

/// What opening a (tool, collection) needs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PreflightState {
    /// The resolved resource is installed.
    Ready,
    /// A pinned resource is missing and the internet is on: download it.
    Fetch,
    /// A pinned resource is missing and the internet is off. Not an error.
    Unavailable,
    /// The project pins nothing for this tool.
    Unpinned,
    /// The pins are known and installed, and neither covers the book.
    NotCovered,
    /// A pre-release build pinned the translation words repository in the word links
    /// slot; the app reads no links from it, and the project must be created again.
    OldWordLinks,
    /// The rung that answers holds a value the app cannot use as a pin, such as a tag
    /// without a commit (D59): no download or file can satisfy it, and the next rung is
    /// not used in its place. The project's pins must be repaired.
    UnusablePin,
}

/// The preflight's answer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Preflight {
    pub tool: ToolId,
    pub collection: Collection,
    pub state: PreflightState,
    pub resolution: Option<Resolution>,
    /// The pinned resource this computer lacks, which a fetch or the guided fix gets.
    pub needs: Option<Pin>,
    /// Set when the tool opened on the fallback only because the primary's pin is not
    /// installed, or not usable, and records no coverage: the screen warns that a
    /// substitute is in use.
    pub unavailable_primary: Option<SlotPin>,
    /// The unusable pin behind [`PreflightState::UnusablePin`], and its rung.
    pub unusable: Option<(Rung, UnusablePin)>,
}

/// Decides what opening the tool needs (tC4 `preflightToolBook`, `preflightObsTool`).
/// `pins` is `None` when the project has no pins at all.
pub fn preflight(
    pins: Option<&Pins>,
    tool: ToolId,
    collection: Collection,
    machine: &Machine<'_>,
) -> Preflight {
    let answer = |state, resolution, needs, unavailable_primary| Preflight {
        tool,
        collection,
        state,
        resolution,
        needs,
        unavailable_primary,
        unusable: None,
    };
    let unusable = |rung, pin: &UnusablePin| Preflight {
        unusable: Some((rung, pin.clone())),
        ..answer(PreflightState::UnusablePin, None, None, None)
    };
    let Some(pins) = pins.filter(|p| p.primary.is_some() || p.fallback.is_some()) else {
        return answer(PreflightState::Unpinned, None, None, None);
    };
    let slot = Slot::of_tool(tool, collection);
    if slot == Slot::TranslationWordsLinks
        && Rung::LADDER.into_iter().any(|rung| {
            pins.pin(rung, slot)
                .is_some_and(|p| !p.resource.repo_path.as_str().ends_with("_twl"))
        })
    {
        return answer(PreflightState::OldWordLinks, None, None, None);
    }
    let local = |r: &ResourceRef| machine.local_coverage(r);
    let availability = |pin: &Pin| {
        if machine.has(&pin.resource) {
            PreflightState::Ready
        } else if machine.online {
            PreflightState::Fetch
        } else {
            PreflightState::Unavailable
        }
    };
    // A slot whose coverage nothing here can tell: a pin that records no books and is
    // not installed, or an unusable one that records none.
    let unknown = |held: &&SlotPin| match held {
        SlotPin::Usable(p) => !coverage_of(p, &local).is_recorded() && !machine.has(&p.resource),
        SlotPin::Unusable(u) => !u.coverage.is_recorded(),
    };

    let resolution = match answer_with(pins, tool, collection, local) {
        Some(Answer::Pin(resolution)) => resolution,
        Some(Answer::Unusable { rung, pin }) => return unusable(rung, &pin),
        None => {
            let ladder: Vec<(Rung, &SlotPin)> = Rung::LADDER
                .into_iter()
                .filter_map(|rung| Some((rung, pins.slot(rung, slot)?)))
                .collect();
            if ladder.is_empty() {
                return answer(PreflightState::Unpinned, None, None, None);
            }
            // A slot that might cover the book: fetch it rather than say it does not.
            return match ladder.into_iter().find(|(_, held)| unknown(held)) {
                Some((_, SlotPin::Usable(pin))) if machine.online => {
                    answer(PreflightState::Fetch, None, Some(pin.clone()), None)
                }
                Some((_, SlotPin::Usable(_))) => {
                    answer(PreflightState::Unavailable, None, None, None)
                }
                Some((rung, SlotPin::Unusable(pin))) => unusable(rung, pin),
                None => answer(PreflightState::NotCovered, None, None, None),
            };
        }
    };

    if collection == Collection::Stories {
        let state = availability(&resolution.pin);
        // tC4 names the story pin only for a download; the guided fix falls back to the
        // resolved pin.
        let needs = (state == PreflightState::Fetch).then(|| resolution.pin.clone());
        return answer(state, Some(resolution), needs, None);
    }

    // The tW tool also reads the resolving set's articles.
    if let (ToolId::TranslationWords, Some(SlotPin::Unusable(pin))) =
        (tool, pins.slot(resolution.rung, Slot::TranslationWords))
    {
        return unusable(resolution.rung, pin);
    }
    let missing = missing_pin(pins, tool, &resolution, machine);
    let state = missing.as_ref().map_or(PreflightState::Ready, availability);
    let unavailable_primary = pins
        .slot(Rung::Primary, slot)
        .filter(|held| resolution.used_fallback() && unknown(held))
        .cloned();
    answer(state, Some(resolution), missing, unavailable_primary)
}

/// The pin a resolved tool still lacks: the resolved pin, or for tW the resolving set's
/// translation words pin, which holds the articles (tC4 `missingPinOf`, #646).
fn missing_pin(
    pins: &Pins,
    tool: ToolId,
    resolution: &Resolution,
    machine: &Machine<'_>,
) -> Option<Pin> {
    if !machine.has(&resolution.pin.resource) {
        return Some(resolution.pin.clone());
    }
    if tool != ToolId::TranslationWords {
        return None;
    }
    pins.pin(resolution.rung, Slot::TranslationWords)
        .filter(|p| !machine.has(&p.resource))
        .cloned()
}

/// What the guided fix screen offers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GuidedFix {
    pub tool: ToolId,
    /// The exact pinned resource to get.
    pub pin: Pin,
    /// The rung that pins it; a re-pin moves this rung's slots.
    pub rung: Rung,
    /// Other commits of the same repository installed here: the re-pin choices.
    pub candidates: Vec<Installed>,
}

/// The guided fix for a tool whose preflight found its resource missing (tC4
/// `openFix`). The pin to get is the one the preflight named, else the resolved pin,
/// else the first pin of the ladder that records no books and is not installed (never a
/// pin recorded as not covering the book). `None` when there is nothing to fix, or when
/// the pin is unusable: getting another rung's resource would not repair it.
pub fn guided_fix(pins: &Pins, preflight: &Preflight, machine: &Machine<'_>) -> Option<GuidedFix> {
    if preflight.state == PreflightState::UnusablePin {
        return None;
    }
    let slot = Slot::of_tool(preflight.tool, preflight.collection);
    let local = |r: &ResourceRef| machine.local_coverage(r);
    let pin = preflight
        .needs
        .clone()
        .or_else(|| preflight.resolution.as_ref().map(|r| r.pin.clone()))
        .or_else(|| {
            Rung::LADDER
                .into_iter()
                .filter_map(|rung| pins.pin(rung, slot))
                .find(|p| !coverage_of(p, &local).is_recorded() && !machine.has(&p.resource))
                .cloned()
        })?;
    // For tW the missing pin can be the resolving set's translation words pin, which
    // the tool's slot does not hold: it is on the rung that resolved.
    let rung = pins
        .rung_pinning(slot, &pin.resource)
        .or(preflight.resolution.as_ref().map(|r| r.rung))
        .unwrap_or(Rung::Primary);
    let candidates = machine
        .installed
        .iter()
        .filter(|i| {
            i.resource.repo_path == pin.resource.repo_path && i.resource.sha != pin.resource.sha
        })
        .cloned()
        .collect();
    Some(GuidedFix {
        tool: preflight.tool,
        pin,
        rung,
        candidates,
    })
}

/// Moving a rung's pins from a missing commit to an installed one (tC4 `repinOffer`).
/// It is confirmed like a release upgrade, with the carry-over counts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Repin {
    pub rung: Rung,
    /// Every slot of the rung that pins the missing commit.
    pub slots: Vec<Slot>,
    pub from: Pin,
    /// The new pin: the installed commit, its tag if it has one, and no recorded
    /// coverage (the caller records the installed copy's books).
    pub to: Pin,
}

/// The re-pin from `from` to the installed `to`; `None` when the rung pins `from`
/// nowhere.
pub fn repin(pins: &Pins, rung: Rung, from: &Pin, to: &Installed) -> Option<Repin> {
    let slots: Vec<Slot> = Slot::HELPS
        .into_iter()
        .filter(|s| {
            pins.pin(rung, *s)
                .is_some_and(|p| p.resource == from.resource)
        })
        .collect();
    (!slots.is_empty()).then(|| Repin {
        rung,
        slots,
        from: from.clone(),
        to: Pin {
            resource: ResourceRef {
                repo_path: from.resource.repo_path.clone(),
                sha: to.resource.sha.clone(),
            },
            version: to.version.clone().filter(|v| !v.is_empty()),
            coverage: Coverage::Unrecorded,
        },
    })
}

/// Why a file chosen in the guided fix is not installed.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum SideloadRefusal {
    #[error("the file declares no revision")]
    NoRevision,
    #[error(
        "the file is commit {found}, but the project pins {pinned} ({label}); it is not installed"
    )]
    OtherCommit {
        found: CommitSha,
        pinned: CommitSha,
        label: String,
    },
}

/// A file is installed only when it declares exactly the pinned commit (tC4
/// `verifySideload`, D58).
pub fn verify_sideload(pin: &Pin, revision: Option<&CommitSha>) -> Result<(), SideloadRefusal> {
    match revision {
        None => Err(SideloadRefusal::NoRevision),
        Some(found) if *found == pin.resource.sha => Ok(()),
        Some(found) => Err(SideloadRefusal::OtherCommit {
            found: found.clone(),
            pinned: pin.resource.sha.clone(),
            label: pin.label(),
        }),
    }
}
