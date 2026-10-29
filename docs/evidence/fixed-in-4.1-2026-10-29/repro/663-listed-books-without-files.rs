//! tC4 #663 repro: a Door43 gateway helps release whose metadata lists books it holds no file
//! for. The fake Door43's `es-419_gl/es-419_tn` v66 is the real release's metadata (it lists
//! 3JN, JON, RUT, TIT) with only TIT.tsv in the commit.
//! Goes at crates/tc-resources/tests/repro_663.rs.
//! Run: tc-heavy cargo test -p tc-resources --test repro_663 -- --nocapture

#![allow(
    clippy::unwrap_used,
    clippy::panic,
    reason = "test code: a failed unwrap is a failed test"
)]

mod common;

use common::{release, rig};
use tc_core::BookId;
use tc_door43::CatalogQuery;
use tc_resources::resolve::{PreflightState, Resolved, preflight_tool_book, resolve_tool};
use tc_resources::{
    BookCoverage, Collection, GatewayLanguage, LanguageSet, Origin, ProjectPins, ResourcePin,
    Rung, SetSlot, ToolId, install_dcs,
};

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_release_that_lists_books_it_lacks_offers_them_but_checks_only_what_it_holds() {
    let rig = rig().await;

    // 1. The catalog (what Get source texts reads, tc-app `package_rows`) lists the four books
    //    the metadata names.
    let listing = rig
        .client
        .catalog_search(&CatalogQuery {
            owners: vec!["es-419_gl".parse().unwrap()],
            ..CatalogQuery::default()
        })
        .await
        .unwrap();
    let tn = listing
        .items
        .iter()
        .find(|e| e.repo.name.as_str() == "es-419_tn")
        .unwrap();
    println!("catalog books for es-419_tn: {:?}", tn.books);
    assert!(tn.books.iter().any(|b| b.eq_ignore_ascii_case("jon")));

    // 2. The install succeeds, records the listed-but-absent files, and covers only Titus.
    let pin = release(&rig, "es-419_gl/es-419_tn", "v66").await;
    let installed = install_dcs(&rig.store, &rig.client, &pin).await.unwrap();
    let Origin::Dcs {
        missing_ingredients,
        ..
    } = installed.origin()
    else {
        panic!("a Door43 install")
    };
    println!("missing ingredients: {missing_ingredients:?}");
    println!("installed coverage: {:?}", installed.books());
    assert_eq!(
        installed.books(),
        BookCoverage::books([BookId::Titus]).as_ref()
    );
    let pin: ResourcePin = installed.dcs_pin().unwrap();

    let spanish = LanguageSet::new(GatewayLanguage {
        language: "es-419".to_owned(),
        owner: "es-419_gl".parse().unwrap(),
    })
    .with(SetSlot::TranslationNotes, pin);

    // 3. With English as the fallback (the fake's en_tn v86 holds Jonah and Titus): Titus
    //    checks against Spanish, Jonah against English.
    let en_tn = release(&rig, "unfoldingWord/en_tn", "v86").await;
    let en_tn = install_dcs(&rig.store, &rig.client, &en_tn)
        .await
        .unwrap()
        .dcs_pin()
        .unwrap();
    let english = LanguageSet::new(GatewayLanguage {
        language: "en".to_owned(),
        owner: "unfoldingWord".parse().unwrap(),
    })
    .with(SetSlot::TranslationNotes, en_tn);
    let pins = ProjectPins {
        primary: Some(spanish.clone()),
        fallback: Some(english),
        ..ProjectPins::default()
    };
    for (book, want) in [(BookId::Titus, Rung::Primary), (BookId::Jonah, Rung::Fallback)] {
        let r = resolve_tool(&pins, ToolId::TranslationNotes, Collection::Book(book), &rig.store)
            .and_then(Resolved::usable)
            .unwrap();
        println!("{book:?}: notes resolve to {:?} {}", r.rung, r.pin.repo);
        assert_eq!(r.rung, want, "{book:?}");
    }

    // 4. With no fallback, Jonah is "not covered", never an empty check list.
    let alone = ProjectPins {
        primary: Some(spanish),
        ..ProjectPins::default()
    };
    let installed = |p: &ResourcePin| rig.store.has_dcs(p);
    let jon = preflight_tool_book(
        Some(&alone),
        ToolId::TranslationNotes,
        BookId::Jonah,
        &rig.store,
        &installed,
        true,
    );
    println!("Jonah with Spanish alone: {:?}", jon.state);
    assert_eq!(jon.state, PreflightState::NotCovered);
    let tit = preflight_tool_book(
        Some(&alone),
        ToolId::TranslationNotes,
        BookId::Titus,
        &rig.store,
        &installed,
        true,
    );
    println!("Titus with Spanish alone: {:?}", tit.state);
    assert_eq!(tit.state, PreflightState::Ready);
}
