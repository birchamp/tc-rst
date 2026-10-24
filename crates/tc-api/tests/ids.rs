//! Identifiers and tokens are checked where they enter (APP-DESIGN §3.2, §3.4, AD3).
#![cfg(not(target_arch = "wasm32"))]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use proptest::prelude::*;
use serde::de::DeserializeOwned;
use serde_json::json;
use tc_api::account::WebUrl;
use tc_api::check::CheckPlace;
use tc_api::ids::{
    ChangeRef, ConflictRef, LOCALE_IDS, LocaleId, ProjectId, ProjectRef, RecordRef, RefPart,
    RequestId, ServerInstance, StoryNo, UploadId,
};
use tc_api::{ReviewToken, ShownToken};

const BODY: &str = "eyJoZWFkcyI6W10sImtleSI6e319AAAAAAAAAAAAAAAAAAAAAA";

fn reads<T: DeserializeOwned>(text: &str) -> bool {
    serde_json::from_value::<T>(json!(text)).is_ok()
}

#[test]
fn a_token_text_with_the_wrong_prefix_does_not_parse() {
    let shown = format!("st1.{BODY}");
    let review = format!("rt1.{BODY}");
    assert!(reads::<ShownToken>(&shown));
    assert!(reads::<ReviewToken>(&review));
    // A review token never becomes an edit token, nor the other way round (AD3).
    assert!(!reads::<ShownToken>(&review));
    assert!(!reads::<ReviewToken>(&shown));
    // Without a prefix, with another version, or as tc-journal's bare text.
    for text in [
        BODY.to_owned(),
        format!("st2.{BODY}"),
        format!("ST1.{BODY}"),
    ] {
        assert!(!reads::<ShownToken>(&text), "{text}");
        assert!(!reads::<ReviewToken>(&text), "{text}");
    }
}

#[test]
fn a_token_body_must_be_base64url_of_a_plausible_length() {
    for body in [
        "",
        "short",
        &"A".repeat(22),
        &format!("{BODY}+/"),
        &format!("{BODY}=="),
        &format!("{BODY} "),
        &"A".repeat(64 * 1024 + 1),
        // A base64 text without padding is never 1 more than a multiple of 4 long.
        &"A".repeat(25),
    ] {
        assert!(!reads::<ShownToken>(&format!("st1.{body}")), "{body:?}");
        assert!(ShownToken::from_body(body).is_err(), "{body:?}");
    }
    let token = ShownToken::from_body(BODY).unwrap();
    assert_eq!(token.body(), BODY);
    assert_eq!(
        serde_json::to_value(&token).unwrap(),
        json!(format!("st1.{BODY}"))
    );
    let review = ReviewToken::from_body(BODY).unwrap();
    assert_eq!(
        serde_json::to_value(&review).unwrap(),
        json!(format!("rt1.{BODY}"))
    );
}

/// Whether a type implements a trait, decided when the test compiles: the inherent constant
/// exists only when the bound holds, and the trait's constant answers otherwise.
macro_rules! implements {
    ($ty:ty: $($bound:tt)+) => {{
        struct Probe<T: ?Sized>(std::marker::PhantomData<T>);
        #[allow(dead_code)]
        trait No {
            const YES: bool = false;
        }
        impl<T: ?Sized> No for Probe<T> {}
        #[allow(dead_code)]
        impl<T: ?Sized + $($bound)+> Probe<T> {
            const YES: bool = true;
        }
        <Probe<$ty>>::YES
    }};
}

#[test]
fn a_token_has_no_conversion_to_or_from_text() {
    // Only the `server` feature's `from_body` and `body` make or read a token, so the UI,
    // which never enables it, can neither build one from text nor relabel one (AD3).
    assert!(
        implements!(String: TryFrom<String>),
        "the probe answers yes"
    );
    assert!(
        implements!(RequestId: Into<String>),
        "the probe answers yes"
    );
    macro_rules! no_conversion {
        ($($token:ty),*) => {$(
            assert!(!implements!($token: TryFrom<String>));
            assert!(!implements!($token: TryFrom<&'static str>));
            assert!(!implements!($token: std::str::FromStr));
            assert!(!implements!($token: From<String>));
            assert!(!implements!($token: Into<String>));
            assert!(!implements!($token: AsRef<str>));
            assert!(!implements!($token: std::borrow::Borrow<str>));
            assert!(!implements!($token: std::ops::Deref));
            assert!(!implements!($token: std::fmt::Display));
        )*};
    }
    no_conversion!(ShownToken, ReviewToken);
}

#[test]
fn a_token_never_prints_its_text() {
    let token = ShownToken::from_body(BODY).unwrap();
    assert_eq!(format!("{token:?}"), "ShownToken(..)");
    let review = ReviewToken::from_body(BODY).unwrap();
    assert_eq!(format!("{review:?}"), "ReviewToken(..)");
}

#[test]
fn a_project_folder_is_one_path_segment_and_any_name_a_disk_allows() {
    for bad in ["", ".", "..", "a/b", "a\\b", "..\\x", "../x", "a\0b"] {
        assert!(ProjectRef::parse(bad).is_err(), "{bad:?}");
        assert!(!reads::<ProjectRef>(bad), "{bad:?}");
    }
    // Real folder names keep their spaces, dots and scripts, so no project becomes
    // unreachable from the UI.
    for good in [
        "kr_ulb",
        "Kanuri Bible",
        "en_ult.v2",
        ".hidden",
        "....hidden",
        " leading space",
        "العربية",
        "हिन्दी बाइबिल",
        &"x".repeat(255),
    ] {
        assert!(ProjectRef::parse(good).is_ok(), "{good:?}");
        assert!(reads::<ProjectRef>(good), "{good:?}");
    }
    assert!(ProjectRef::parse(&"x".repeat(ProjectRef::MAX_BYTES + 1)).is_err());
}

#[test]
fn a_project_folder_cannot_leave_the_root_on_windows() {
    // On Windows, `root.join("c:..")` replaces the whole path with a path relative to
    // drive C, and `a:b` names a stream of the file `a`. No Windows folder name holds a colon.
    for bad in [
        "C:",
        "C:evil",
        "c:..",
        "D:Users",
        "a:b",
        "kr_ulb:stream",
        "x::$DATA",
    ] {
        assert!(ProjectRef::parse(bad).is_err(), "{bad:?}");
        assert!(!reads::<ProjectRef>(bad), "{bad:?}");
    }
    // Windows drops the trailing dots and spaces of a path's last segment, so these would
    // name the root itself or its parent.
    for bad in ["...", ". ", ".. ", " ", "  ", ". . ."] {
        assert!(ProjectRef::parse(bad).is_err(), "{bad:?}");
        assert!(!reads::<ProjectRef>(bad), "{bad:?}");
    }
}

#[test]
fn a_project_id_is_a_lowercase_hyphenated_uuid() {
    assert!(reads::<ProjectId>("6f1c0e9a-2b7d-4c3e-8a5f-0b1d2c3e4f50"));
    for bad in [
        "6F1C0E9A-2B7D-4C3E-8A5F-0B1D2C3E4F50",
        "6f1c0e9a2b7d4c3e8a5f0b1d2c3e4f50",
        "6f1c0e9a-2b7d-4c3e-8a5f-0b1d2c3e4f5",
        "6f1c0e9a-2b7d-4c3e-8a5f-0b1d2c3e4f50-",
        "",
    ] {
        assert!(!reads::<ProjectId>(bad), "{bad:?}");
    }
}

#[test]
fn byte_ids_are_32_lowercase_hex_digits() {
    let id = RequestId::from_bytes([0x0f; 16]);
    let text = serde_json::to_value(id).unwrap();
    assert_eq!(text, json!("0f".repeat(16)));
    assert_eq!(serde_json::from_value::<RequestId>(text).unwrap(), id);
    for bad in [
        "0F".repeat(16),
        "0f".repeat(15),
        "0f".repeat(17),
        "zz".repeat(16),
    ] {
        assert!(!reads::<RequestId>(&bad), "{bad}");
        assert!(!reads::<ServerInstance>(&bad), "{bad}");
    }
}

#[test]
fn hex_refs_have_their_lengths() {
    assert!(reads::<UploadId>(&"a".repeat(32)));
    assert!(!reads::<UploadId>(&"a".repeat(64)));
    assert!(reads::<ConflictRef>(&"a".repeat(64)));
    assert!(reads::<ChangeRef>(&"0".repeat(64)));
    assert!(!reads::<ConflictRef>(&"A".repeat(64)));
    assert!(!reads::<ChangeRef>(&"a".repeat(63)));
}

#[test]
fn a_record_is_a_bounded_json_object() {
    assert!(reads::<RecordRef>(r#"{"book":"TIT","kind":"verse"}"#));
    for bad in ["", "TIT 1:1", "[1]", "{\"a\":\"\n\"}", "{\"a\":\"\u{1f}\"}"] {
        assert!(!reads::<RecordRef>(bad), "{bad:?}");
    }
    let long = format!("{{\"a\":\"{}\"}}", "x".repeat(RecordRef::MAX_BYTES));
    assert!(!reads::<RecordRef>(&long));
}

#[test]
fn a_record_reads_every_text_canonical_json_writes() {
    // tc-journal writes U+007F to U+009F and the line separators raw, and a resources key or
    // a setting name may hold them, so an Ack or a Changed event naming such a record reads.
    for raw in [
        '\u{7f}', '\u{80}', '\u{85}', '\u{9f}', '\u{2028}', '\u{2029}',
    ] {
        let text = format!(r#"{{"kind":"setting","name":"a{raw}b"}}"#);
        assert!(RecordRef::parse(&text).is_ok(), "{text:?}");
        assert!(reads::<RecordRef>(&text), "{text:?}");
    }
    // Escaped forms are plain ASCII.
    assert!(reads::<RecordRef>(r#"{"name":"a\u001fb\nc"}"#));
}

#[test]
fn a_reference_part_is_any_text_a_resource_writes_without_the_separator() {
    // Not empty and no `:`, which takes in tc-check's `RefPart::parse`. The shipped tables write
    // numbers, `front`, `intro`, spans and lists; other forms keep their spaces and scripts.
    for text in [
        "1",
        "150",
        "front",
        "intro",
        "9-10",
        "1,3,8,12",
        "1a",
        " 3",
        "front ",
        "1\u{2013}3",
        "\u{663}",
        "0",
    ] {
        assert!(reads::<RefPart>(text), "{text:?}");
        assert_eq!(RefPart::parse(text).unwrap().as_str(), text);
    }
    // Long, because no length limit may hide a check from the rail.
    assert!(reads::<RefPart>(&"1,".repeat(5000)));
    for text in ["", "1:2", ":", "front:intro"] {
        assert!(!reads::<RefPart>(text), "{text:?}");
    }
    assert!(serde_json::from_value::<RefPart>(json!(3)).is_err());
}

#[test]
fn a_story_is_one_of_the_fifty() {
    for number in [1u8, 2, 49, 50] {
        assert_eq!(StoryNo::new(number).unwrap().get(), number);
        assert_eq!(
            serde_json::from_value::<StoryNo>(json!(number)).unwrap(),
            StoryNo::new(number).unwrap()
        );
    }
    for number in [0u8, 51, 255] {
        assert!(StoryNo::new(number).is_none(), "{number}");
        assert!(
            serde_json::from_value::<StoryNo>(json!(number)).is_err(),
            "{number}"
        );
    }
    for bad in [json!(256), json!(-1), json!("3"), json!(3.5)] {
        assert!(
            serde_json::from_value::<StoryNo>(bad.clone()).is_err(),
            "{bad}"
        );
    }
}

#[test]
fn a_locale_is_one_of_the_ten_interface_languages() {
    for id in LOCALE_IDS {
        let locale = LocaleId::parse(id).unwrap();
        assert_eq!(locale.as_str(), id);
        assert_eq!(serde_json::to_value(locale).unwrap(), json!(id));
    }
    assert_eq!(LocaleId::all().count(), 10);
    for bad in ["EN", "es", "hi-IN", "de", "", "__proto__"] {
        assert!(!reads::<LocaleId>(bad), "{bad:?}");
    }
}

#[test]
fn a_web_link_is_http_or_https_with_a_host() {
    for good in [
        "https://git.door43.org/jdoe",
        "http://example.org",
        "https://a.b/c?d=e#f",
    ] {
        assert!(WebUrl::parse(good).is_ok(), "{good}");
    }
    for bad in [
        "javascript:alert(1)",
        "file:///etc/passwd",
        "https://",
        "https:///path",
        "https://a b",
        "ftp://example.org",
        "door43.org",
    ] {
        assert!(WebUrl::parse(bad).is_err(), "{bad}");
        assert!(!reads::<WebUrl>(bad), "{bad}");
    }
}

#[test]
fn a_sign_in_never_prints_the_password() {
    let sign_in = tc_api::account::SignIn {
        request: RequestId::from_bytes([1; 16]),
        login: "jdoe".into(),
        password: "correct horse".into(),
        stay: false,
    };
    let printed = format!("{sign_in:?}");
    assert!(printed.contains("jdoe"));
    assert!(!printed.contains("correct horse"), "{printed}");
}

/// M6's rules, shared by the dialog and the server (tC4 `openSourceOf`, `openDefaultName`,
/// `openRepoPathOf`, `repoPathInUse`; `test/openFromDoor43Dialog.test.tsx`).
mod open_from_door43 {
    use tc_api::door43::{ProjectName, RepoSource, default_project_name};
    use tc_api::ids::ProjectRef;

    const HOST: &str = "git.door43.org";

    fn source(input: &str) -> Option<String> {
        RepoSource::from_input(input, HOST).map(|s| s.to_string())
    }

    #[test]
    fn a_repository_is_owner_slash_name_or_a_link_on_the_apps_server() {
        let want = Some("anna_k/titus-team-draft".to_owned());
        assert_eq!(source("anna_k/titus-team-draft"), want);
        assert_eq!(source("  anna_k/titus-team-draft/  "), want);
        assert_eq!(
            source("https://git.door43.org/anna_k/titus-team-draft"),
            want
        );
        assert_eq!(
            source("https://git.door43.org/anna_k/titus-team-draft.git"),
            want
        );
        assert_eq!(
            source("https://GIT.door43.org/anna_k/titus-team-draft/"),
            want
        );
        for refused in [
            "",
            "anna_k",
            "anna_k/",
            "/titus",
            "anna k/titus",
            "anna_k/titus/src",
            "https://example.org/anna_k/titus-team-draft",
            "https://user:pw@git.door43.org/anna_k/titus-team-draft",
            "https://git.door43.org/anna_k",
            "https://git.door43.org/anna_k/titus/src/branch/master",
            "file:///tmp/anna_k/titus",
            "ftp://git.door43.org/anna_k/titus",
        ] {
            assert_eq!(source(refused), None, "{refused:?}");
        }
    }

    #[test]
    fn a_repository_on_a_test_server_is_read_by_its_host_and_port() {
        let host = "127.0.0.1:4141";
        let found = RepoSource::from_input("http://127.0.0.1:4141/facilitator-zq/es-419_tit", host);
        assert_eq!(
            found.map(|s| s.to_string()).as_deref(),
            Some("facilitator-zq/es-419_tit")
        );
        assert_eq!(
            RepoSource::from_input("http://127.0.0.1:4142/facilitator-zq/es-419_tit", host),
            None
        );
    }

    #[test]
    fn the_default_name_is_the_repository_name_with_each_run_of_other_signs_one_hyphen() {
        assert_eq!(default_project_name("titus-team-draft"), "titus-team-draft");
        assert_eq!(default_project_name("en_ult.v2"), "en_ult-v2");
        assert_eq!(default_project_name("..a  b..c.."), "a-b-c");
        assert_eq!(default_project_name("¿qué?"), "qu");
        assert_eq!(default_project_name("..."), "");
    }

    #[test]
    fn a_project_name_holds_letters_digits_hyphens_and_underscores() {
        assert_eq!(ProjectName::parse(" titus_2 ").unwrap().as_str(), "titus_2");
        for refused in ["", "  ", "my project", "a/b", "ñandú", "a.b", "a:b"] {
            assert!(ProjectName::parse(refused).is_err(), "{refused:?}");
        }
    }

    #[test]
    fn a_name_in_use_is_found_without_letter_case() {
        let listing = ["Jonah"];
        assert!(ProjectName::parse("jonah").unwrap().in_use(listing));
        assert!(!ProjectName::parse("jonah2").unwrap().in_use(listing));
        assert_eq!(
            ProjectName::parse("Titus_2").unwrap().folder(),
            ProjectRef::parse("Titus_2").unwrap()
        );
    }
}

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
        place in "(OBS|TIT|tit|3JN) [+ ]?[1-9][0-9]{0,2}:[+ ]?[0-9]{1,3}",
    ) {
        round_trips::<CheckPlace>(&place)?;
        // A book code is read in either case and written upper, as BookId does.
        prints_back::<CheckPlace>(&place.replace("tit", "TIT"))?;
    }
}
