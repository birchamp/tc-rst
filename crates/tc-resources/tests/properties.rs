//! Round trips of the stored forms this crate reads, and the ladder's rule.

// Test helpers sit outside `#[test]` functions, where clippy.toml already allows these.
#![allow(clippy::unwrap_used, clippy::panic)]

use proptest::prelude::*;
use tc_core::BookId;
use tc_resources::resolve::resolve_slot;
use tc_resources::{
    BookCoverage, GatewayLanguage, LanguageSet, PinGroup, ProjectPins, ResourcePin, Rung,
    ScripturePane, SetSlot, Sha256, Testaments,
};
use tc_resources::{Collection, Ladder, ToolId};

fn book() -> impl Strategy<Value = BookId> {
    (0usize..66).prop_map(|i| BookId::ALL[i])
}

fn coverage() -> impl Strategy<Value = BookCoverage> {
    prop_oneof![
        Just(BookCoverage::WholeCollection),
        proptest::collection::vec(book(), 1..8).prop_map(|b| BookCoverage::books(b).unwrap()),
    ]
}

fn flavor() -> impl Strategy<Value = String> {
    "(scripture|parascriptural|peripheral)/[a-zA-Z-]{3,12}"
}

/// Fields a newer app may add to a pin.
fn unknown_fields() -> impl Strategy<Value = serde_json::Map<String, serde_json::Value>> {
    proptest::collection::btree_map("x-[a-z]{1,5}", any::<i32>(), 0..2).prop_map(|m| {
        m.into_iter()
            .map(|(k, v)| (k, serde_json::Value::from(v)))
            .collect()
    })
}

/// Any pin, as tC4's JSON may hold it.
fn pin() -> impl Strategy<Value = ResourcePin> {
    pin_with(proptest::option::of(flavor()))
}

/// A pin a project keeps: tC4 requires its flavor.
fn project_pin() -> impl Strategy<Value = ResourcePin> {
    pin_with(flavor().prop_map(Some))
}

fn pin_with(flavor: impl Strategy<Value = Option<String>>) -> impl Strategy<Value = ResourcePin> {
    (
        "[A-Za-z][A-Za-z0-9_.-]{0,12}[A-Za-z0-9]",
        "[a-z]{2,3}_[a-z]{2,3}",
        "[0-9a-f]{40}",
        proptest::option::of("v[0-9]{1,2}(\\.[0-9]{1,2})?"),
        flavor,
        proptest::option::of(coverage()),
        unknown_fields(),
    )
        .prop_map(|(owner, name, sha, version, flavor, books, extra)| {
            let mut pin = ResourcePin::new(
                format!("git.door43.org/{owner}/{name}").parse().unwrap(),
                sha.parse().unwrap(),
            );
            pin.version = version.map(|v| v.parse().unwrap());
            pin.flavor = flavor;
            pin.books = books;
            pin.extra = extra;
            pin
        })
}

/// A slot value not in tC4's §5.3 form, which a project keeps as written: a pin with a tag
/// and no commit, or one whose coverage names a book in lower case.
fn unreadable_slot() -> impl Strategy<Value = serde_json::Value> {
    (
        "[a-z]{2,3}_[a-z]{2,3}",
        "v[0-9]{1,2}",
        prop::bool::ANY,
        "[0-9a-f]{40}",
    )
        .prop_map(|(name, version, tag_only, sha)| {
            let mut slot = serde_json::json!({
                "repoPath": format!("git.door43.org/someone/{name}"),
                "version": version,
                "flavor": "parascriptural/x-bcvnotes",
            });
            if tag_only {
                slot["books"] = serde_json::json!(["TIT"]);
            } else {
                slot["sha"] = serde_json::Value::from(sha);
                slot["books"] = serde_json::json!(["tit"]);
            }
            slot
        })
}

fn language_set() -> impl Strategy<Value = LanguageSet> {
    (
        "[a-z]{2}(-[0-9]{3})?",
        "[A-Za-z][A-Za-z0-9_-]{0,10}",
        proptest::collection::btree_map(
            (0usize..SetSlot::ALL.len()).prop_map(|i| SetSlot::ALL[i]),
            project_pin(),
            0..6,
        ),
        proptest::collection::btree_map(
            (0usize..SetSlot::ALL.len()).prop_map(|i| SetSlot::ALL[i]),
            unreadable_slot(),
            0..3,
        ),
        unknown_fields(),
        unknown_fields(),
    )
        .prop_map(
            |(language, owner, pins, mut unreadable, extra, gateway_extra)| {
                unreadable.retain(|slot, _| !pins.contains_key(slot));
                LanguageSet {
                    gateway: GatewayLanguage {
                        language,
                        owner: owner.parse().unwrap(),
                    },
                    pins,
                    unreadable,
                    extra,
                    gateway_extra,
                }
            },
        )
}

fn testaments() -> impl Strategy<Value = Testaments> {
    (
        proptest::option::of(project_pin()),
        proptest::option::of(project_pin()),
        proptest::option::of(unreadable_slot()),
        unknown_fields(),
    )
        .prop_map(|(nt, ot, raw, extra)| {
            let mut unreadable = std::collections::BTreeMap::new();
            // A testament the group pins in tC4's form cannot also be kept as written.
            if let Some(raw) = raw.filter(|_| ot.is_none()) {
                unreadable.insert(tc_core::Testament::Old, raw);
            }
            Testaments {
                nt,
                ot,
                unreadable,
                extra,
            }
        })
}

fn project_pins() -> impl Strategy<Value = ProjectPins> {
    (
        proptest::option::of(language_set()),
        proptest::option::of(language_set()),
        testaments(),
        testaments(),
        proptest::collection::vec(
            prop_oneof![
                ("[a-z]{2,4}", project_pin()).prop_map(|(id, pin)| Ok(ScripturePane {
                    id: id.parse().unwrap(),
                    pin,
                })),
                ("[a-z]{2,4}", unreadable_slot()).prop_map(|(id, mut raw)| {
                    raw["id"] = serde_json::Value::from(id);
                    Err(raw)
                }),
            ],
            0..4,
        ),
    )
        .prop_map(|(primary, fallback, original_language, lexicon, entries)| {
            let mut extra_scripture = Vec::new();
            let mut unreadable_panes = Vec::new();
            for (at, entry) in entries.into_iter().enumerate() {
                match entry {
                    Ok(pane) => extra_scripture.push(pane),
                    Err(raw) => unreadable_panes.push((at, raw)),
                }
            }
            ProjectPins {
                primary,
                fallback,
                original_language,
                lexicon,
                extra_scripture,
                unreadable_panes,
                unknown: Default::default(),
            }
        })
}

proptest! {
    #[test]
    fn sha256_digests_round_trip_through_hex(bytes in proptest::collection::vec(any::<u8>(), 0..64)) {
        let digest = Sha256::of(&bytes);
        prop_assert_eq!(digest.to_hex().parse::<Sha256>().unwrap(), digest);
        prop_assert_eq!(format!("sha256:{digest}").parse::<Sha256>().unwrap(), digest);
    }

    #[test]
    fn coverage_round_trips_through_its_codes(c in coverage()) {
        prop_assert_eq!(BookCoverage::from_codes(&c.codes()).unwrap(), c);
    }

    #[test]
    fn a_pin_round_trips_through_tc4s_json(p in pin()) {
        let json = serde_json::to_string(&p).unwrap();
        prop_assert_eq!(serde_json::from_str::<ResourcePin>(&json).unwrap(), p);
    }

    #[test]
    fn pins_round_trip_through_resources_json_and_each_group(pins in project_pins()) {
        let json = pins.to_resources_json().unwrap();
        prop_assert_eq!(&ProjectPins::from_resources_json(&json).unwrap(), &pins);
        let mut rebuilt = ProjectPins::default();
        for group in PinGroup::ALL {
            rebuilt.set_group_json(group, &pins.group_json(group).unwrap()).unwrap();
        }
        prop_assert_eq!(rebuilt, pins);
    }

    /// D30: the primary answers when its pin covers the book, else the fallback, else
    /// nothing; no other set is ever consulted.
    #[test]
    fn the_ladder_has_two_rungs_and_the_primary_wins(
        primary in proptest::option::of(coverage()),
        fallback in proptest::option::of(coverage()),
        b in book(),
    ) {
        let make = |books: Option<BookCoverage>, sha: char| {
            let mut pin = ResourcePin::new(
                "git.door43.org/unfoldingWord/en_tn".parse().unwrap(),
                sha.to_string().repeat(40).parse().unwrap(),
            );
            pin.books = books;
            LanguageSet::new(GatewayLanguage { language: "en".to_owned(), owner: "unfoldingWord".parse().unwrap() })
                .with(SetSlot::TranslationNotes, pin)
        };
        let pins = ProjectPins {
            primary: Some(make(primary.clone(), 'a')),
            fallback: Some(make(fallback.clone(), 'b')),
            ..ProjectPins::default()
        };
        // On a computer with nothing installed, a pin that records nothing covers nothing.
        let nothing = |_: &ResourcePin| None;
        let got = resolve_slot(&pins, SetSlot::TranslationNotes, Collection::Book(b), &nothing)
            .map(|r| r.rung());
        let want = if primary.as_ref().is_some_and(|c| c.covers(b)) {
            Some(Rung::Primary)
        } else if fallback.as_ref().is_some_and(|c| c.covers(b)) {
            Some(Rung::Fallback)
        } else {
            None
        };
        prop_assert_eq!(got, want);
        // In the fold, which reads no installed copy, it covers every book (STORAGE §7.7).
        let fold = pins
            .resolution(ToolId::TranslationNotes, Collection::Book(b))
            .map(|a| a.rung());
        let want = if primary.as_ref().is_none_or(|c| c.covers(b)) {
            Some(Rung::Primary)
        } else if fallback.as_ref().is_none_or(|c| c.covers(b)) {
            Some(Rung::Fallback)
        } else {
            None
        };
        prop_assert_eq!(fold, want);
    }
}
