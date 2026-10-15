//! Shown tokens (`docs/STORAGE.md` §2.7): what a screen displayed, signed so a screen cannot
//! make one up.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use common::*;
use proptest::prelude::*;
use tc_journal::fold::{ChangeIndex, ConflictId};
use tc_journal::keys::UnknownKey;
use tc_journal::token::TokenError;
use tc_journal::{
    BookId, ChangeId, CheckId, CheckKey, CheckTarget, PackId, RecordKey, RefPart, ShownToken,
    TokenKey, ToolId,
};

fn signing() -> TokenKey {
    TokenKey::derive(&[7; 32], project())
}

fn some_keys() -> Vec<RecordKey> {
    vec![
        RecordKey::Verse(BookId::Titus, chapter(1), verse_no(4)),
        RecordKey::Versification,
        RecordKey::Check(CheckKey {
            tool: ToolId::TranslationNotes,
            check_id: CheckId::parse("abcd").unwrap(),
            target: CheckTarget::Verse {
                book: BookId::Jonah,
                chapter: RefPart::parse("1").unwrap(),
                verse: RefPart::parse("9-10").unwrap(),
            },
            occurrence: 2,
        }),
        RecordKey::Unknown(UnknownKey {
            kind: "noteThread".into(),
            key_json: r#"{"book":"TIT","thread":"a1"}"#.into(),
        }),
    ]
}

#[test]
fn a_token_round_trips_through_the_ui_as_text() {
    let a = ChangeId::of(b"a");
    let b = ChangeId::of(b"b");
    for key in some_keys() {
        let token = ShownToken::mint(&signing(), key.clone(), [b, a, b]).unwrap();
        let mut heads = vec![a, b];
        heads.sort();
        assert_eq!(token.heads(), heads.as_slice(), "sorted, no repeats");
        let text = token.to_text();
        assert!(
            text.bytes()
                .all(|c| c.is_ascii_alphanumeric() || c == b'-' || c == b'_')
        );
        assert_eq!(ShownToken::parse(&signing(), &text), Ok(token.clone()));
        assert_eq!(token.key(), &key);
    }
    let empty = ShownToken::mint(&signing(), RecordKey::Versification, []).unwrap();
    assert!(empty.heads().is_empty());
}

#[test]
fn a_token_from_another_installation_or_project_is_refused() {
    let token = ShownToken::mint(&signing(), some_keys().remove(0), [ChangeId::of(b"a")]).unwrap();
    let text = token.to_text();
    let other_secret = TokenKey::derive(&[8; 32], project());
    let other_project = TokenKey::derive(&[7; 32], other_project());
    assert_eq!(
        ShownToken::parse(&other_secret, &text),
        Err(TokenError::BadMac)
    );
    assert_eq!(
        ShownToken::parse(&other_project, &text),
        Err(TokenError::BadMac)
    );
    assert_eq!(
        ShownToken::parse(&signing(), "!!"),
        Err(TokenError::NotBase64)
    );
    assert_eq!(
        ShownToken::parse(&signing(), "AAAA"),
        Err(TokenError::TooShort)
    );
}

/// A token's bytes and the key it is signed with are part of the format: a screen keeps
/// tokens across a restart (a draft, §5.14), and an app upgrade must not turn them into
/// forgeries. The vectors were computed from §2.7 with Python's hmac module, not with this
/// code: the token key is HMAC-SHA256 under the installation secret of
/// `"tc4-shown-token\0" ‖ the project id's 16 bytes`.
#[test]
fn tokens_have_the_pinned_byte_form_and_key() {
    let verse = ShownToken::mint(
        &signing(),
        RecordKey::Verse(BookId::Titus, chapter(1), verse_no(4)),
        [ChangeId::of(b"a")],
    )
    .unwrap();
    let pinned = "eyJoZWFkcyI6WyJjYTk3ODExMmNhMWJiZGNhZmFjMjMxYjM5YTIzZGM0ZGE3ODZlZmY4MTQ3YzRlNzJiOTgwNzc4NWFmZWU0OGJiIl0sImtleSI6eyJ2ZXJzZSI6eyJib29rIjoiVElUIiwiY2hhcHRlciI6MSwidmVyc2UiOjR9fX09rvbaRBHqUkSG5mlkaSPA";
    assert_eq!(verse.to_text(), pinned);
    assert_eq!(ShownToken::parse(&signing(), pinned), Ok(verse));
    let empty = ShownToken::mint(&signing(), RecordKey::Versification, []).unwrap();
    assert_eq!(
        empty.to_text(),
        "eyJoZWFkcyI6W10sImtleSI6eyJ2ZXJzaWZpY2F0aW9uIjp7fX199MzG6ZEyr2_zopX8XmltSw"
    );
}

proptest! {
    /// Any change to a token's text is refused: a screen cannot claim heads it was not
    /// shown, which is what keeps a stale screen from reverting a teammate (I4).
    #[test]
    fn a_tampered_token_is_refused(at in any::<prop::sample::Index>(), to in 0u8..64, heads in prop::collection::vec(arb_change_id(), 0..3)) {
        let token = ShownToken::mint(&signing(), some_keys().remove(2), heads).unwrap();
        let text = token.to_text();
        let alphabet = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
        let mut bytes = text.clone().into_bytes();
        let at = at.index(bytes.len());
        bytes[at] = alphabet[usize::from(to)];
        let tampered = String::from_utf8(bytes).unwrap();
        prop_assume!(tampered != text);
        prop_assert!(ShownToken::parse(&signing(), &tampered).is_err());
    }
}

#[test]
fn the_change_index_holds_each_change_once_with_every_pack() {
    let mut index = ChangeIndex::default();
    let change = verse_change(local(1), 1, 1, "a");
    let first = index.insert(change.clone(), Some(PackId::of(b"p1")));
    let again = index.insert(change.clone(), Some(PackId::of(b"p2")));
    let pending = index.insert(change.clone(), None);
    assert_eq!((first, again, pending), (first, first, first));
    assert_eq!(index.len(), 1);
    let entry = index.get(first).unwrap();
    assert_eq!(
        entry.packs.as_slice(),
        &[PackId::of(b"p1"), PackId::of(b"p2")]
    );
    assert_eq!(index.find(&change.id()), Some(first));
    let snapshot = index.clone();
    index.insert(verse_change(local(1), 2, 1, "b"), None);
    assert_eq!((snapshot.len(), index.len()), (1, 2));
    assert_eq!(ConflictId::of(&[change.id()]).to_hex().len(), 64);
}
