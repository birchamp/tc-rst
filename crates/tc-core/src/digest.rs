//! The two digests every layer shares: a git object id, which identifies a pinned resource
//! version, and an MD5, which a decision and an alignment record of the text they were made
//! against and a burrito records of each ingredient. One type each, so a value read by the
//! journal, the screens, the resource store and the Door43 client is the same value.
//!
//! Both have one text form, lowercase hexadecimal: the form tC4, git and the Scripture
//! Burrito schema write, and the only form the journal's canonical bytes allow.
//! [`GitOid::parse_any_case`] also reads the uppercase form a person may paste.

use serde::{Deserialize, Serialize};
use std::fmt;
use std::str::FromStr;

/// Text that is not a digest's lowercase hexadecimal form.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{text:?} is not {what}: {digits} lowercase hexadecimal digits")]
pub struct DigestError {
    pub what: &'static str,
    pub text: String,
    pub digits: usize,
}

fn nibble(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        _ => None,
    }
}

fn unhex<const N: usize>(text: &str) -> Option<[u8; N]> {
    let bytes = text.as_bytes();
    if bytes.len() != 2 * N {
        return None;
    }
    let mut out = [0u8; N];
    let (pairs, _) = bytes.as_chunks::<2>();
    for (byte, [high, low]) in out.iter_mut().zip(pairs) {
        *byte = nibble(*high)? << 4 | nibble(*low)?;
    }
    Some(out)
}

fn write_hex(f: &mut fmt::Formatter<'_>, bytes: &[u8]) -> fmt::Result {
    bytes.iter().try_for_each(|b| write!(f, "{b:02x}"))
}

macro_rules! digest {
    ($(#[$doc:meta])* $name:ident, $len:literal, $what:literal) => {
        $(#[$doc])*
        #[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
        pub struct $name([u8; $len]);

        impl $name {
            pub const fn from_bytes(bytes: [u8; $len]) -> Self {
                Self(bytes)
            }

            pub const fn as_bytes(&self) -> &[u8; $len] {
                &self.0
            }

            /// Exactly the lowercase hexadecimal this type writes.
            pub fn parse(text: &str) -> Option<Self> {
                unhex::<$len>(text).map(Self)
            }

            pub fn to_hex(&self) -> String {
                self.to_string()
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                write_hex(f, &self.0)
            }
        }

        impl fmt::Debug for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(f, "{}({self})", stringify!($name))
            }
        }

        impl FromStr for $name {
            type Err = DigestError;

            fn from_str(text: &str) -> Result<Self, Self::Err> {
                Self::parse(text).ok_or_else(|| DigestError {
                    what: $what,
                    text: text.to_owned(),
                    digits: 2 * $len,
                })
            }
        }

        impl Serialize for $name {
            fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
                s.collect_str(self)
            }
        }

        impl<'de> Deserialize<'de> for $name {
            fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
                let text = String::deserialize(d)?;
                text.parse().map_err(serde::de::Error::custom)
            }
        }
    };
}

digest!(
    /// A git object id; also the identity of a pinned resource version (a commit sha, D58).
    /// Text form: 40 lowercase hexadecimal digits.
    GitOid,
    20,
    "a git object id"
);

digest!(
    /// An MD5 digest. Text form: 32 lowercase hexadecimal digits.
    Md5,
    16,
    "an md5"
);

impl GitOid {
    /// 40 hexadecimal digits in either case, as a person may paste a commit id.
    pub fn parse_any_case(text: &str) -> Option<Self> {
        Self::parse(&text.to_ascii_lowercase())
    }
}

impl Md5 {
    /// The digest of `bytes`.
    pub fn of(bytes: &[u8]) -> Self {
        use md5::Digest;
        Self(md5::Md5::digest(bytes).into())
    }

    pub fn bytes(&self) -> [u8; 16] {
        self.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn digests_read_and_write_lowercase_hex_only() {
        let md5 = Md5::of(b"");
        assert_eq!(md5.to_string(), "d41d8cd98f00b204e9800998ecf8427e");
        assert_eq!(Md5::parse(&md5.to_hex()), Some(md5));
        assert_eq!(Md5::parse("D41D8CD98F00B204E9800998ECF8427E"), None);
        assert!("+0".repeat(16).parse::<Md5>().is_err());
        let sha = "e586762e330f482a60c52aedd1c7b3a2f155df8a";
        let oid = GitOid::parse(sha).unwrap();
        assert_eq!(oid.to_string(), sha);
        assert_eq!(format!("{oid:?}"), format!("GitOid({sha})"));
        assert_eq!(GitOid::parse(&sha.to_uppercase()), None);
        assert_eq!(GitOid::parse_any_case(&sha.to_uppercase()), Some(oid));
        assert!("c354b8ae".parse::<GitOid>().is_err());
        let json = serde_json::to_string(&oid).unwrap();
        assert_eq!(json, format!("\"{sha}\""));
        assert_eq!(serde_json::from_str::<GitOid>(&json).unwrap(), oid);
        assert!(serde_json::from_str::<GitOid>("\"abc\"").is_err());
    }
}
