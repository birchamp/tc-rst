//! Shown tokens (`docs/STORAGE.md` §2.7). The core hands the UI an opaque token with every
//! record it displays, and a command that changes a record carries the token of what was
//! displayed. The token's heads become the new change's parents (I4), and its MAC stops a
//! screen from constructing "the current heads".
//!
//! Byte form: base64url (no padding) of `payload ‖ mac`, where `payload` is the canonical
//! JSON `{"heads":[…],"key":{…},"kind":"edit"|"review"}` (the key as in a record object,
//! without its value) and `mac` is HMAC-SHA256 of the payload, truncated to 16 bytes, under
//! the [`TokenKey`].
//!
//! The kind is under the MAC: an **edit** token lists the heads by the token rule (a
//! conflicted record's displayed candidate only), and a save takes it; a **review** token
//! lists every head of the record, and only a resolution takes it. So a resolution screen's
//! token can never be replayed as an edit that silently resolves, nor an editor's as a
//! resolution of versions it never showed (APP-DESIGN R19).

use crate::canon::{decode_key, encode_key};
use crate::ids::{ChangeId, ProjectId};
use crate::json::{self, Json, JsonMap};
use crate::keys::RecordKey;
use crate::text::{base64url, unbase64url};
use hmac::{Hmac, KeyInit, Mac};
use sha2::Sha256;
use smallvec::SmallVec;

type HmacSha256 = Hmac<Sha256>;

const MAC_LEN: usize = 16;

/// The key tokens of one project are signed with on this installation: HMAC-SHA256 under
/// the installation secret (§3.7) of `"tc4-shown-token\0" ‖ the project id's 16 bytes`.
/// It never leaves the server.
#[derive(Clone, PartialEq, Eq)]
pub struct TokenKey([u8; 32]);

impl std::fmt::Debug for TokenKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("TokenKey(…)")
    }
}

impl TokenKey {
    pub fn derive(installation_secret: &[u8; 32], project: ProjectId) -> Self {
        let mut mac = mac_with(installation_secret);
        mac.update(b"tc4-shown-token\0");
        mac.update(project.as_bytes());
        Self(mac.finalize().into_bytes().into())
    }
}

/// HMAC-SHA256 under a 32-byte key. HMAC pads a key shorter than the 64-byte block with
/// zeros, so passing the padded block is the same function, without a fallible call.
fn mac_with(key: &[u8; 32]) -> HmacSha256 {
    let mut block = [0u8; 64];
    block[..32].copy_from_slice(key);
    HmacSha256::new(&block.into())
}

/// What a token was minted for (see the module documentation).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TokenKind {
    /// The heads a screen displayed by the token rule of §2.7: what an edit continues.
    Edit,
    /// Every head of the record, as a resolution screen displays them.
    Review,
}

impl TokenKind {
    fn name(self) -> &'static str {
        match self {
            Self::Edit => "edit",
            Self::Review => "review",
        }
    }
}

/// What a record looked like when it was displayed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShownToken {
    kind: TokenKind,
    key: RecordKey,
    heads: SmallVec<[ChangeId; 2]>,
    mac: [u8; MAC_LEN],
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum TokenError {
    #[error("the token is not base64url")]
    NotBase64,
    #[error("the token is too short")]
    TooShort,
    #[error("the token was not made by this installation for this project")]
    BadMac,
    #[error("the token's content is malformed: {0}")]
    Malformed(String),
    #[error("the record key cannot be put in a token: {0}")]
    BadKey(String),
}

impl ShownToken {
    /// An edit token for `key` displayed as standing for `heads` (the token rule of §2.7
    /// decides which heads those are).
    pub fn mint(
        signing: &TokenKey,
        key: RecordKey,
        heads: impl IntoIterator<Item = ChangeId>,
    ) -> Result<Self, TokenError> {
        Self::mint_kind(signing, TokenKind::Edit, key, heads)
    }

    /// A review token: `heads` are every head of the record.
    pub fn mint_review(
        signing: &TokenKey,
        key: RecordKey,
        heads: impl IntoIterator<Item = ChangeId>,
    ) -> Result<Self, TokenError> {
        Self::mint_kind(signing, TokenKind::Review, key, heads)
    }

    pub fn mint_kind(
        signing: &TokenKey,
        kind: TokenKind,
        key: RecordKey,
        heads: impl IntoIterator<Item = ChangeId>,
    ) -> Result<Self, TokenError> {
        let mut heads: SmallVec<[ChangeId; 2]> = heads.into_iter().collect();
        heads.sort();
        heads.dedup();
        let payload = payload(kind, &key, &heads)?;
        let mac = sign(signing, &payload);
        Ok(Self {
            kind,
            key,
            heads,
            mac,
        })
    }

    pub fn kind(&self) -> TokenKind {
        self.kind
    }

    pub fn key(&self) -> &RecordKey {
        &self.key
    }

    /// Sorted, no repeats. Empty when the record had no version yet.
    pub fn heads(&self) -> &[ChangeId] {
        &self.heads
    }

    /// The opaque text the UI holds.
    pub fn to_text(&self) -> String {
        // The key encoded when the token was minted, so it encodes now.
        let mut bytes = payload(self.kind, &self.key, &self.heads).unwrap_or_default();
        bytes.extend_from_slice(&self.mac);
        base64url(&bytes)
    }

    /// Reads a token the UI sent back, refusing one this key did not sign.
    pub fn parse(signing: &TokenKey, text: &str) -> Result<Self, TokenError> {
        Self::decode(text, |payload, mac| {
            let mut check = mac_with(&signing.0);
            check.update(payload);
            check
                .verify_truncated_left(mac)
                .map_err(|_| TokenError::BadMac)
        })
    }

    /// What a token says, without checking its MAC: the record and heads of a draft whose
    /// token this installation can no longer verify (a new installation secret, R20), so its
    /// text is offered and kept as a change with no parents, never trusted as a base.
    pub fn parse_unverified(text: &str) -> Result<Self, TokenError> {
        Self::decode(text, |_, _| Ok(()))
    }

    /// Splits the token's text into payload and MAC, runs `check` on them, then reads the
    /// payload.
    fn decode(
        text: &str,
        check: impl FnOnce(&[u8], &[u8]) -> Result<(), TokenError>,
    ) -> Result<Self, TokenError> {
        let bytes = unbase64url(text).ok_or(TokenError::NotBase64)?;
        if bytes.len() <= MAC_LEN {
            return Err(TokenError::TooShort);
        }
        let (payload, mac) = bytes.split_at(bytes.len() - MAC_LEN);
        check(payload, mac)?;
        Self::read_payload(payload, mac)
    }

    fn read_payload(payload: &[u8], mac: &[u8]) -> Result<Self, TokenError> {
        let malformed = |text: String| TokenError::Malformed(text);
        let value = json::parse(payload).map_err(|e| malformed(e.to_string()))?;
        if value.to_canonical() != payload {
            return Err(malformed("the payload is not canonical JSON".into()));
        }
        let map = value
            .as_obj()
            .filter(|map| map.len() == 3)
            .ok_or_else(|| malformed("expected heads, key and kind".into()))?;
        let kind = match map.get("kind").and_then(Json::as_str) {
            Some("edit") => TokenKind::Edit,
            Some("review") => TokenKind::Review,
            _ => return Err(malformed("kind is not edit or review".into())),
        };
        let heads = match map.get("heads") {
            Some(Json::Arr(items)) => items
                .iter()
                .map(|item| item.as_str().and_then(ChangeId::parse))
                .collect::<Option<SmallVec<[ChangeId; 2]>>>()
                .ok_or_else(|| malformed("a head is not a change id".into()))?,
            _ => return Err(malformed("heads is not an array".into())),
        };
        let key = map
            .get("key")
            .ok_or_else(|| malformed("no key".into()))
            .and_then(|k| decode_key(k).map_err(|e| malformed(e.to_string())))?;
        if heads.windows(2).any(|w| w[0] >= w[1]) {
            return Err(malformed("heads are not sorted without repeats".into()));
        }
        let mut out = [0u8; MAC_LEN];
        out.copy_from_slice(mac);
        Ok(Self {
            kind,
            key,
            heads,
            mac: out,
        })
    }
}

fn payload(kind: TokenKind, key: &RecordKey, heads: &[ChangeId]) -> Result<Vec<u8>, TokenError> {
    let mut map = JsonMap::new();
    map.insert(
        "heads".into(),
        Json::Arr(heads.iter().map(|h| Json::from(h.to_hex())).collect()),
    );
    map.insert(
        "key".into(),
        encode_key(key).map_err(|e| TokenError::BadKey(e.to_string()))?,
    );
    map.insert("kind".into(), Json::from(kind.name()));
    Ok(Json::Obj(map).to_canonical())
}

fn sign(signing: &TokenKey, payload: &[u8]) -> [u8; MAC_LEN] {
    let mut mac = mac_with(&signing.0);
    mac.update(payload);
    let full = mac.finalize().into_bytes();
    let mut out = [0u8; MAC_LEN];
    out.copy_from_slice(&full[..MAC_LEN]);
    out
}
