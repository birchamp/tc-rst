//! INV-IDS-ROUND-TRIP for the names Door43 reports and the app writes.

use proptest::prelude::*;
use tc_door43::{AccountName, NewRepoName, RepoName, RepoRef, ServerUrl, TagName, TokenName};

/// A value that parses prints a text that parses back to it (INV-IDS-ROUND-TRIP).
fn round_trips<T: std::str::FromStr + std::fmt::Display + PartialEq + std::fmt::Debug>(
    text: &str,
) -> Result<(), TestCaseError> {
    if let Ok(value) = text.parse::<T>() {
        let printed = value.to_string();
        prop_assert_eq!(
            printed.parse::<T>().ok(),
            Some(value),
            "{:?} printed {:?}",
            text,
            printed
        );
    }
    Ok(())
}

/// An identifier the app writes: a text that parses prints back exactly as written.
fn prints_back<T: std::str::FromStr + std::fmt::Display>(text: &str) -> Result<(), TestCaseError> {
    if let Ok(value) = text.parse::<T>() {
        prop_assert_eq!(value.to_string(), text);
    }
    Ok(())
}

proptest! {
    #[test]
    fn ids_round_trip(
        name in "[A-Za-z0-9_.-]{0,12}( |/|%|\\+)?[A-Za-z0-9_.-]{0,6}",
        tag in "v?[0-9]{1,3}(\\.[0-9]{1,2})?(/[a-z]{1,4})?",
        server in "(https|http)://(git\\.door43\\.org|127\\.0\\.0\\.1:[1-9][0-9]{3}|localhost)(/[a-z]{1,3})?/?",
    ) {
        round_trips::<AccountName>(&name)?;
        round_trips::<RepoName>(&name)?;
        round_trips::<NewRepoName>(&name)?;
        round_trips::<RepoRef>(&name)?;
        round_trips::<TagName>(&tag)?;
        round_trips::<TokenName>(&name)?;
        round_trips::<ServerUrl>(&server)?;
        prints_back::<AccountName>(&name)?;
        prints_back::<RepoName>(&name)?;
        prints_back::<NewRepoName>(&name)?;
        prints_back::<RepoRef>(&name)?;
        prints_back::<TagName>(&tag)?;
        prints_back::<TokenName>(&name)?;
    }
}
