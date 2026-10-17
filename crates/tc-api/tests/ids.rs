//! Identifiers and tokens are checked where they enter (APP-DESIGN §3.2, §3.4, AD3).
#![cfg(not(target_arch = "wasm32"))]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use serde::de::DeserializeOwned;
use serde_json::json;
use tc_api::account::WebUrl;
use tc_api::ids::{
    ChangeRef, ConflictRef, LOCALE_IDS, LocaleId, ProjectId, ProjectRef, RecordRef, RequestId,
    ServerInstance, UploadId,
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
    // Real folder names keep their spaces, dots, colons and scripts, so no project becomes
    // unreachable from the UI.
    for good in [
        "kr_ulb",
        "Kanuri Bible",
        "en_ult.v2",
        ".hidden",
        "a:b",
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
    for bad in ["", "TIT 1:1", "[1]", "{\"a\":\"\n\"}"] {
        assert!(!reads::<RecordRef>(bad), "{bad:?}");
    }
    let long = format!("{{\"a\":\"{}\"}}", "x".repeat(RecordRef::MAX_BYTES));
    assert!(!reads::<RecordRef>(&long));
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
