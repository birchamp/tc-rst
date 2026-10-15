//! The six standard schemes, bundled. Provenance and license: `schemes/README.md`.

use crate::scheme::Scheme;
use crate::versification::Versification;
use std::sync::LazyLock;

/// A scheme tC4 offers at project creation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum StandardScheme {
    /// English: the default, and the frame of every unfoldingWord resource.
    Eng,
    /// Septuagint.
    Lxx,
    /// Original: BHS for the Old Testament, UBS GNT for the New. The pivot every other
    /// scheme maps through.
    Org,
    /// Russian Protestant.
    Rsc,
    /// Russian Orthodox.
    Rso,
    /// Vulgate.
    Vul,
}

impl StandardScheme {
    pub const ALL: [StandardScheme; 6] = [
        Self::Eng,
        Self::Lxx,
        Self::Org,
        Self::Rsc,
        Self::Rso,
        Self::Vul,
    ];

    /// The scheme tC4 gives a project unless the user picks another.
    pub const DEFAULT: StandardScheme = Self::Eng;

    pub fn name(self) -> &'static str {
        match self {
            Self::Eng => "eng",
            Self::Lxx => "lxx",
            Self::Org => "org",
            Self::Rsc => "rsc",
            Self::Rso => "rso",
            Self::Vul => "vul",
        }
    }

    pub fn from_name(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|s| s.name() == name)
    }

    /// The bundled `vrs.json` bytes: what the platform writes into a new project as
    /// `ingredients/vrs.json`.
    pub fn json(self) -> &'static str {
        match self {
            Self::Eng => include_str!("../schemes/eng.json"),
            Self::Lxx => include_str!("../schemes/lxx.json"),
            Self::Org => include_str!("../schemes/org.json"),
            Self::Rsc => include_str!("../schemes/rsc.json"),
            Self::Rso => include_str!("../schemes/rso.json"),
            Self::Vul => include_str!("../schemes/vul.json"),
        }
    }

    /// The parsed and compiled scheme, built once per process.
    pub fn scheme(self) -> &'static Scheme {
        &LOADED[self as usize]
    }
}

static LOADED: [LazyLock<Scheme>; 6] = [
    LazyLock::new(|| load(StandardScheme::Eng)),
    LazyLock::new(|| load(StandardScheme::Lxx)),
    LazyLock::new(|| load(StandardScheme::Org)),
    LazyLock::new(|| load(StandardScheme::Rsc)),
    LazyLock::new(|| load(StandardScheme::Rso)),
    LazyLock::new(|| load(StandardScheme::Vul)),
];

#[allow(clippy::expect_used)]
fn load(which: StandardScheme) -> Scheme {
    // The bundled files are fixed at build time, and the test
    // `every_standard_scheme_loads` parses and compiles each one, so this cannot fail.
    let versification = Versification::from_json(which.json()).expect("bundled scheme parses");
    Scheme::new(which.name(), versification).expect("bundled scheme compiles")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_standard_scheme_loads() {
        for which in StandardScheme::ALL {
            let v = Versification::from_json(which.json()).unwrap();
            Scheme::new(which.name(), v).unwrap();
            assert_eq!(which.scheme().name(), which.name());
            assert_eq!(StandardScheme::from_name(which.name()), Some(which));
        }
        assert_eq!(StandardScheme::from_name("unrecorded"), None);
    }
}
