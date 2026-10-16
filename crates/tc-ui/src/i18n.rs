//! The interface languages: tC4's locale registry (`src/i18n/locales.js`) and its message
//! catalogs.
//!
//! The catalogs in `assets/i18n/<id>.json` are tC4's `src/i18n/<id>.json` at commit `d310bf3`,
//! copied byte for byte: flat objects from a dotted key to a message with `{name}`
//! placeholders, 1,029 keys each. tC4 has no other source of interface text, so a screen that
//! uses the same key shows the same words. To update them, copy the files again from a tC4
//! checkout and run `cargo test -p tc-ui`, which checks every catalog against English.

/// The direction an interface reads in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextDir {
    Ltr,
    Rtl,
}

impl TextDir {
    /// The value of the `dir` attribute.
    pub fn as_str(self) -> &'static str {
        match self {
            TextDir::Ltr => "ltr",
            TextDir::Rtl => "rtl",
        }
    }
}

/// One installed interface language.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Locale {
    /// The catalog's id, which is also its file name.
    pub id: &'static str,
    /// The language's own name, as the picker shows it.
    pub label: &'static str,
    /// The direction the interface reads in (#657). Scripture sets its own from the project.
    pub dir: TextDir,
}

/// The installed catalogs, in picker order (#522, #622; Arabic #649). The picker offers exactly
/// these.
pub const LOCALES: [Locale; 10] = [
    Locale {
        id: "en",
        label: "English",
        dir: TextDir::Ltr,
    },
    Locale {
        id: "es-419",
        label: "Español (Latinoamérica)",
        dir: TextDir::Ltr,
    },
    Locale {
        id: "fr",
        label: "Français",
        dir: TextDir::Ltr,
    },
    Locale {
        id: "hi",
        label: "हिन्दी",
        dir: TextDir::Ltr,
    },
    Locale {
        id: "pt-BR",
        label: "Português (Brasil)",
        dir: TextDir::Ltr,
    },
    Locale {
        id: "id",
        label: "Bahasa Indonesia",
        dir: TextDir::Ltr,
    },
    Locale {
        id: "uk",
        label: "Українська",
        dir: TextDir::Ltr,
    },
    Locale {
        id: "ru",
        label: "Русский",
        dir: TextDir::Ltr,
    },
    Locale {
        id: "vi",
        label: "Tiếng Việt",
        dir: TextDir::Ltr,
    },
    Locale {
        id: "ar",
        label: "العربية",
        dir: TextDir::Rtl,
    },
];

/// The language of a first start, and of a saved choice that is not installed.
pub const DEFAULT_LOCALE: &str = "en";

/// The installed locale with this id. Ids are compared exactly: `EN` and `hi-IN` are not
/// installed.
pub fn locale(id: &str) -> Option<&'static Locale> {
    LOCALES.iter().find(|l| l.id == id)
}

/// The `{name}` placeholders of a message, sorted, each as often as it occurs. A translation
/// keeps those of its English source.
pub fn placeholders(message: &str) -> Vec<&str> {
    let mut found = Vec::new();
    let mut rest = message;
    while let Some(open) = rest.find('{') {
        let after = &rest[open + 1..];
        let name_len = after
            .find(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
            .unwrap_or(after.len());
        if name_len > 0 && after[name_len..].starts_with('}') {
            found.push(&rest[open..open + name_len + 2]);
            rest = &after[name_len + 1..];
        } else {
            rest = after;
        }
    }
    found.sort_unstable();
    found
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn placeholders_are_named_braces_sorted_with_repeats() {
        assert_eq!(
            placeholders("{n} of {total} · {n}"),
            vec!["{n}", "{n}", "{total}"]
        );
        assert_eq!(placeholders("{ n } {} {a-b} {x"), Vec::<&str>::new());
        assert_eq!(placeholders("{{span}}"), vec!["{span}"]);
    }

    #[test]
    fn a_locale_is_found_by_its_exact_id() {
        assert_eq!(locale("ar").map(|l| l.dir), Some(TextDir::Rtl));
        for bad in ["es", "de", "", "EN", "hi-IN", "__proto__", "toString"] {
            assert!(locale(bad).is_none(), "{bad}");
        }
        assert!(locale(DEFAULT_LOCALE).is_some());
    }
}
