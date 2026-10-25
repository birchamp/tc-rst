//! The help pages as this app serves them name its own projects folder and download page, not
//! tC4 4.0's (H16), while the vendored files stay tc-website's bytes.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::PathBuf;

use tc_server::{HELP_OVERLAY, help_overlay};

fn vendored(page: &str) -> Vec<u8> {
    let help = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../tc-ui/help");
    std::fs::read(help.join(page)).unwrap_or_else(|e| panic!("{page}: {e}"))
}

fn served(page: &str) -> String {
    String::from_utf8(help_overlay(page, vendored(page))).unwrap()
}

#[test]
fn help_pages_name_the_real_projects_folder() {
    for page in [
        "st-where-projects-live.html",
        "search-index.json",
        "search-index.js",
    ] {
        let text = served(page);
        assert!(
            text.contains("translationCore4/projects"),
            "{page} does not name ~/translationCore4/projects"
        );
        assert!(
            !text.contains("pankosmia") && !text.contains("tc4-projects"),
            "{page} still names tC4 4.0's folder"
        );
    }
    let install = served("gs-install.html");
    assert!(install.contains("https://translationcore.com/tc4"));
    assert!(!install.contains("translationCore4/releases"));
}

/// A re-vendored page whose words moved would leave tC4's folder in place unnoticed.
#[test]
fn every_help_overlay_rule_finds_its_words_in_the_vendored_page() {
    for (page, from, _) in HELP_OVERLAY {
        let text = String::from_utf8(vendored(page)).unwrap();
        assert!(text.contains(from), "{page} no longer holds {from:?}");
    }
}

#[test]
fn a_help_page_without_a_rule_is_served_as_it_is() {
    let bytes = vendored("index.html");
    assert_eq!(help_overlay("index.html", bytes.clone()), bytes);
}
