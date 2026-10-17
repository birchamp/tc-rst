//! Shown tokens (APP-DESIGN AD3, §3.4; STORAGE §2.7).
//!
//! The server hands the UI a token with every value it can edit, and a write carries back
//! the token of what was displayed. Two kinds exist and never mix: a [`ShownToken`] stands
//! for the one version an editor showed, and a [`ReviewToken`] for every head of a record in
//! conflict, which only a resolution may carry. Their texts carry the prefixes `st1.` and
//! `rt1.`, so a token of the wrong kind is refused when it is parsed, and a scan of the DOM
//! for either prefix finds any token that leaked into it.
//!
//! The UI cannot make, read or change a token: without the `server` feature, which only the
//! server side enables, a token is made only by deserializing one the server sent, and it
//! prints as `ShownToken(..)`.

use std::fmt;

use serde::{Deserialize, Serialize};

/// Why a token text was refused.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum TokenTextError {
    #[error("a {kind} must start with {prefix:?}")]
    WrongPrefix {
        kind: &'static str,
        prefix: &'static str,
    },
    #[error("a {kind} body must be base64url without padding")]
    NotBase64Url { kind: &'static str },
    #[error("a {kind} body must be {min} to {max} characters")]
    BadLength {
        kind: &'static str,
        min: usize,
        max: usize,
    },
}

/// The shortest body: the 16-byte MAC and at least one byte of payload (tc-journal
/// `token.rs`), in base64url without padding.
const MIN_BODY: usize = 23;
/// A review token lists every head of a record, 64 hex digits each. This bounds a body to
/// several hundred heads, far more than a team makes, and bounds what a request can carry.
const MAX_BODY: usize = 64 * 1024;

fn check_body(kind: &'static str, body: &str) -> Result<(), TokenTextError> {
    if !(MIN_BODY..=MAX_BODY).contains(&body.len()) {
        return Err(TokenTextError::BadLength {
            kind,
            min: MIN_BODY,
            max: MAX_BODY,
        });
    }
    let alphabet = body
        .bytes()
        .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_');
    // A base64 text without padding never has a length of 1 modulo 4.
    if !alphabet || body.len() % 4 == 1 {
        return Err(TokenTextError::NotBase64Url { kind });
    }
    Ok(())
}

macro_rules! token {
    ($(#[$doc:meta])* $name:ident, $prefix:literal, $kind:literal) => {
        $(#[$doc])*
        #[derive(Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
        #[serde(try_from = "String", into = "String")]
        pub struct $name(String);

        impl $name {
            /// The prefix of this kind's wire text.
            pub const PREFIX: &'static str = $prefix;

            fn parse_text(text: &str) -> Result<Self, TokenTextError> {
                let body = text.strip_prefix($prefix).ok_or(TokenTextError::WrongPrefix {
                    kind: $kind,
                    prefix: $prefix,
                })?;
                check_body($kind, body)?;
                Ok(Self(text.to_owned()))
            }
        }

        #[cfg(feature = "server")]
        impl $name {
            /// The token whose body is tc-journal's token text (`ShownToken::to_text`).
            pub fn from_body(body: &str) -> Result<Self, TokenTextError> {
                check_body($kind, body)?;
                Ok(Self(format!("{}{body}", $prefix)))
            }

            /// tc-journal's token text, for `ShownToken::parse`, which checks the MAC.
            pub fn body(&self) -> &str {
                self.0.get($prefix.len()..).unwrap_or_default()
            }
        }

        impl TryFrom<String> for $name {
            type Error = TokenTextError;
            fn try_from(text: String) -> Result<Self, TokenTextError> {
                Self::parse_text(&text)
            }
        }

        impl From<$name> for String {
            fn from(token: $name) -> Self {
                token.0
            }
        }

        impl fmt::Debug for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(concat!(stringify!($name), "(..)"))
            }
        }

        #[cfg(feature = "proptest")]
        impl proptest::arbitrary::Arbitrary for $name {
            type Parameters = ();
            type Strategy = proptest::strategy::BoxedStrategy<Self>;
            fn arbitrary_with((): ()) -> Self::Strategy {
                use proptest::strategy::Strategy;
                // The pattern is a literal that compiles.
                proptest::string::string_regex("[A-Za-z0-9_-]{24,80}")
                    .unwrap_or_else(|_| unreachable!("a valid regex"))
                    .prop_filter_map("a base64url length", |body| {
                        Self::parse_text(&format!("{}{body}", $prefix)).ok()
                    })
                    .boxed()
            }
        }
    };
}

token!(
    /// The version of one record an editor displayed (STORAGE:745-767). A save carries it,
    /// and the stored change takes that version as its parent.
    ShownToken,
    "st1.",
    "shown token"
);
token!(
    /// Every head of a record in conflict (STORAGE:1610-1616). Only a resolution carries
    /// it; a save that carried it would supersede versions the person never saw.
    ReviewToken,
    "rt1.",
    "review token"
);
