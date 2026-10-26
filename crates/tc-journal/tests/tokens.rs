//! Shown tokens (`docs/STORAGE.md` §2.7): what a screen displayed, signed so a screen cannot
//! make one up.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    reason = "test code: a failed unwrap is a failed test"
)]

mod common;

use common::*;
use proptest::prelude::*;
use tc_journal::fold::{ConflictId, FoldInput, FoldState, ProjectKind};
use tc_journal::keys::UnknownKey;
use tc_journal::token::TokenError;
use tc_journal::{
    BookId, ChangeId, CheckId, CheckKey, CheckTarget, PackId, RecordKey, RefPart, ShownToken,
    TokenKey, TokenKind, ToolId,
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
    let pinned = "eyJoZWFkcyI6WyJjYTk3ODExMmNhMWJiZGNhZmFjMjMxYjM5YTIzZGM0ZGE3ODZlZmY4MTQ3YzRlNzJiOTgwNzc4NWFmZWU0OGJiIl0sImtleSI6eyJ2ZXJzZSI6eyJib29rIjoiVElUIiwiY2hhcHRlciI6MSwidmVyc2UiOjR9fSwia2luZCI6ImVkaXQifU7bjZZdVLjQi8dQnjJR_lY";
    assert_eq!(verse.to_text(), pinned);
    assert_eq!(ShownToken::parse(&signing(), pinned), Ok(verse));
    let empty = ShownToken::mint(&signing(), RecordKey::Versification, []).unwrap();
    assert_eq!(
        empty.to_text(),
        "eyJoZWFkcyI6W10sImtleSI6eyJ2ZXJzaWZpY2F0aW9uIjp7fX0sImtpbmQiOiJlZGl0In2NOId6AXdwh3QwAG_iZLmr"
    );
    let review = ShownToken::mint_review(
        &signing(),
        RecordKey::Verse(BookId::Titus, chapter(1), verse_no(4)),
        [ChangeId::of(b"a")],
    )
    .unwrap();
    let pinned = "eyJoZWFkcyI6WyJjYTk3ODExMmNhMWJiZGNhZmFjMjMxYjM5YTIzZGM0ZGE3ODZlZmY4MTQ3YzRlNzJiOTgwNzc4NWFmZWU0OGJiIl0sImtleSI6eyJ2ZXJzZSI6eyJib29rIjoiVElUIiwiY2hhcHRlciI6MSwidmVyc2UiOjR9fSwia2luZCI6InJldmlldyJ91diRlNpaTLx-N3MKnqefJQ";
    assert_eq!(review.to_text(), pinned);
    assert_eq!(
        ShownToken::parse(&signing(), pinned).unwrap().kind(),
        TokenKind::Review
    );
}

/// The kind is under the MAC (APP-DESIGN R19): an edit token's text with its kind changed
/// to review, or the reverse, does not verify, so a screen cannot turn one into the other.
#[test]
fn a_token_s_kind_cannot_be_changed() {
    let key = RecordKey::Verse(BookId::Titus, chapter(1), verse_no(4));
    let heads = [ChangeId::of(b"a")];
    let edit = ShownToken::mint(&signing(), key.clone(), heads).unwrap();
    let review = ShownToken::mint_review(&signing(), key, heads).unwrap();
    assert_eq!(edit.kind(), TokenKind::Edit);
    assert_eq!(review.kind(), TokenKind::Review);
    assert_ne!(edit.to_text(), review.to_text());
    for (from, to) in [(&edit, "review"), (&review, "edit")] {
        let text = from.to_text();
        let bytes = tc_journal_text_bytes(&text);
        let mac = &bytes[bytes.len() - 16..];
        let payload = String::from_utf8(bytes[..bytes.len() - 16].to_vec()).unwrap();
        let swapped = payload.replace(
            &format!(
                "\"kind\":\"{}\"",
                if to == "review" { "edit" } else { "review" }
            ),
            &format!("\"kind\":\"{to}\""),
        );
        let mut forged = swapped.into_bytes();
        forged.extend_from_slice(mac);
        let forged = base64url_text(&forged);
        assert_eq!(
            ShownToken::parse(&signing(), &forged),
            Err(TokenError::BadMac)
        );
        // Read without the MAC, the forged text says what it says; nothing trusts that.
        assert_eq!(
            ShownToken::parse_unverified(&forged).unwrap().kind(),
            if to == "review" {
                TokenKind::Review
            } else {
                TokenKind::Edit
            }
        );
    }
}

fn tc_journal_text_bytes(text: &str) -> Vec<u8> {
    let mut out = Vec::new();
    let mut buffer = 0u32;
    let mut bits = 0;
    for c in text.bytes() {
        let v = match c {
            b'A'..=b'Z' => c - b'A',
            b'a'..=b'z' => c - b'a' + 26,
            b'0'..=b'9' => c - b'0' + 52,
            b'-' => 62,
            _ => 63,
        };
        buffer = buffer << 6 | u32::from(v);
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((buffer >> bits) as u8);
            buffer &= (1 << bits) - 1;
        }
    }
    out
}

fn base64url_text(bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
    let mut out = String::new();
    for chunk in bytes.chunks(3) {
        let n =
            chunk.iter().fold(0u32, |acc, b| acc << 8 | u32::from(*b)) << (8 * (3 - chunk.len()));
        for i in 0..=chunk.len() {
            out.push(ALPHABET[(n >> (18 - 6 * i) & 63) as usize] as char);
        }
    }
    out
}

/// Any change to a token's text is refused, in its content or in its MAC: a screen cannot
/// claim heads it was not shown, which is what keeps a stale screen from reverting a
/// teammate (I4).
#[test]
fn a_tampered_token_is_refused() {
    let strategy = (
        any::<prop::sample::Index>(),
        0u8..64,
        prop::collection::vec(arb_change_id(), 0..3),
    );
    run_with_census(
        256,
        strategy,
        &["content", "mac"],
        |(at, to, heads), census| {
            let token = ShownToken::mint(&signing(), some_keys().remove(2), heads).unwrap();
            let text = token.to_text();
            let alphabet = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
            let mut bytes = text.clone().into_bytes();
            let at = at.index(bytes.len());
            bytes[at] = alphabet[usize::from(to)];
            let tampered = String::from_utf8(bytes).unwrap();
            if tampered == text {
                return Ok(());
            }
            // The last 22 characters hold the 16 bytes of the MAC (and a few bits of content).
            census.saw(if at + 22 >= text.len() {
                "mac"
            } else {
                "content"
            });
            prop_assert!(ShownToken::parse(&signing(), &tampered).is_err());
            Ok(())
        },
    );
}

#[test]
fn the_fold_holds_each_change_once_with_every_pack() {
    let change = verse_change(local(1), 1, 1, "a");
    let line = |pack: Option<PackId>| FoldInput::new(change.clone().into(), pack);
    let mut state = FoldState::new(ProjectKind::Bible);
    state.add([
        line(Some(PackId::of(b"p1"))),
        line(Some(PackId::of(b"p2"))),
        line(None),
    ]);
    assert_eq!(state.len(), 1);
    let view = state.change(&change.id()).unwrap();
    assert_eq!(view.packs, vec![PackId::of(b"p1"), PackId::of(b"p2")]);
    let snapshot = state.clone();
    state.add([FoldInput::from(verse_change(local(1), 2, 1, "b"))]);
    assert_eq!((snapshot.len(), state.len()), (1, 2));
    assert_eq!(ConflictId::of(&[change.id()]).to_hex().len(), 64);
}
