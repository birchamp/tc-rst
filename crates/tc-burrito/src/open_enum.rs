/// A value of an open enumeration that this version does not know, kept exactly as written.
///
/// Only parsing creates one, and parsing maps every known spelling to its own variant, so an
/// `Other` never holds a known value.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct UnknownValue(pub(crate) String);

impl UnknownValue {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Defines an enumeration whose JSON form is a string. Known strings get their own variant;
/// any other string becomes `Other` and is written back unchanged, so a newer burrito
/// survives a read-modify-write by this version.
macro_rules! open_enum {
    (
        $(#[$meta:meta])*
        pub enum $name:ident {
            $( $(#[$vmeta:meta])* $variant:ident => $text:literal, )+
        }
    ) => {
        $(#[$meta])*
        #[derive(Debug, Clone, PartialEq, Eq, Hash)]
        pub enum $name {
            $( $(#[$vmeta])* $variant, )+
            /// A value this version does not know, kept as written.
            Other($crate::open_enum::UnknownValue),
        }

        impl $name {
            pub fn as_str(&self) -> &str {
                match self {
                    $( Self::$variant => $text, )+
                    Self::Other(value) => value.as_str(),
                }
            }

            pub fn is_known(&self) -> bool {
                !matches!(self, Self::Other(_))
            }
        }

        impl From<&str> for $name {
            fn from(text: &str) -> Self {
                match text {
                    $( $text => Self::$variant, )+
                    other => Self::Other($crate::open_enum::UnknownValue(other.to_owned())),
                }
            }
        }

        impl std::fmt::Display for $name {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str(self.as_str())
            }
        }

        impl serde::Serialize for $name {
            fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
                s.serialize_str(self.as_str())
            }
        }

        impl<'de> serde::Deserialize<'de> for $name {
            fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
                let text = <String as serde::Deserialize>::deserialize(d)?;
                Ok(Self::from(text.as_str()))
            }
        }
    };
}

pub(crate) use open_enum;
