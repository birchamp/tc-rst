//! What each USFM marker name means to this crate.
//!
//! The sets follow USFM 3.0. Paragraph-level markers form a closed set in USFM, so any
//! marker not listed here is treated as a character marker, which is also the right
//! default for custom `\z…` markers.

/// The role of a marker name.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MarkerClass {
    /// `\c`.
    Chapter,
    /// `\v`.
    Verse,
    /// Book identification and running headers: `\id`, `\usfm`, `\ide`, `\h`, `\toc1`…
    Identification,
    /// Book titles and introduction material: `\mt1`, `\imt`, `\is`, `\ip`, `\ie`…
    Introduction,
    /// Chapter presentation that belongs to the chapter, not to a verse: `\cl`, `\cp`, `\cd`.
    ChapterLevel,
    /// A paragraph-level marker whose text is a title, not verse text: `\s1`, `\ms`, `\r`,
    /// `\d`, `\sp`, `\qa`, `\rem`…
    Heading,
    /// A paragraph-level marker whose text is verse text, or that carries no text: `\p`,
    /// `\q1`, `\m`, `\b`, `\li1`, `\pi`, `\tr`, `\ts`…
    Paragraph,
    /// A note container: `\f`, `\fe`, `\ef`, `\x`, `\ex`.
    Note,
    /// A marker that structures the inside of a note: `\fr`, `\ft`, `\fq`, `\xo`, `\xt`…
    NoteContent,
    /// Every other marker: character markers such as `\w`, `\add`, `\nd`, and unknown ones.
    Character,
}

impl MarkerClass {
    /// Paragraph-level markers, verses and chapters end every open character span.
    pub fn is_paragraph_level(self) -> bool {
        matches!(
            self,
            Self::Chapter
                | Self::Verse
                | Self::Identification
                | Self::Introduction
                | Self::ChapterLevel
                | Self::Heading
                | Self::Paragraph
        )
    }
}

/// The marker name with a trailing level number removed: `q1` → `q`, `toc2` → `toc`.
/// Hyphenated milestone names keep their suffix: `zaln-s` stays `zaln-s`.
pub fn base_name(name: &str) -> &str {
    if name.contains('-') {
        name
    } else {
        name.trim_end_matches(|c: char| c.is_ascii_digit())
    }
}

/// Classify a marker name, such as `q1`, `zaln-s` or `w`. The `+` of a nested character
/// marker and the `*` of a closing marker are not part of the name.
pub fn classify(name: &str) -> MarkerClass {
    match base_name(name) {
        "c" => MarkerClass::Chapter,
        "v" => MarkerClass::Verse,
        "id" | "usfm" | "ide" | "sts" | "h" | "toc" | "toca" => MarkerClass::Identification,
        "mt" | "mte" | "imt" | "imte" | "is" | "ip" | "ipi" | "im" | "imi" | "ipq" | "imq"
        | "ipr" | "iq" | "ib" | "ili" | "iot" | "io" | "ior" | "iex" | "ie" | "periph" => {
            MarkerClass::Introduction
        }
        "cl" | "cp" | "cd" => MarkerClass::ChapterLevel,
        "ms" | "mr" | "s" | "sr" | "r" | "d" | "sp" | "sd" | "qa" | "rem" | "lit" => {
            MarkerClass::Heading
        }
        "p" | "m" | "po" | "pr" | "cls" | "pmo" | "pm" | "pmc" | "pmr" | "pi" | "mi" | "nb"
        | "pc" | "ph" | "b" | "q" | "qr" | "qc" | "qm" | "qd" | "lh" | "li" | "lf" | "lim"
        | "tr" | "th" | "thr" | "tc" | "tcr" | "pb" | "esb" | "esbe" | "ts" => {
            MarkerClass::Paragraph
        }
        "f" | "fe" | "ef" | "x" | "ex" => MarkerClass::Note,
        "fr" | "ft" | "fq" | "fqa" | "fk" | "fl" | "fw" | "fp" | "fv" | "fdc" | "fm" | "xo"
        | "xk" | "xq" | "xt" | "xta" | "xop" | "xot" | "xnt" | "xdc" => MarkerClass::NoteContent,
        _ => MarkerClass::Character,
    }
}

/// Character spans whose content is never verse text: notes, figures, alternate and
/// published numbers, categories and inline quotation references.
pub(crate) fn is_non_text_span(name: &str) -> bool {
    classify(name) == MarkerClass::Note || matches!(name, "fig" | "ca" | "va" | "vp" | "cat" | "rq")
}

/// Spans that stay open to the next paragraph-level marker when their closing marker is
/// missing, so that the text of an unclosed footnote never leaks into verse text.
pub(crate) fn closes_implicitly(name: &str) -> bool {
    classify(name) == MarkerClass::Note || name == "fig"
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn levels_are_stripped_except_on_milestones() {
        assert_eq!(base_name("q1"), "q");
        assert_eq!(base_name("toc3"), "toc");
        assert_eq!(base_name("zaln-s"), "zaln-s");
        assert_eq!(classify("q2"), MarkerClass::Paragraph);
        assert_eq!(classify("s5"), MarkerClass::Heading);
        assert_eq!(classify("mt1"), MarkerClass::Introduction);
        assert_eq!(classify("zaln-s"), MarkerClass::Character);
        assert_eq!(classify("qs"), MarkerClass::Character);
        assert!(MarkerClass::Paragraph.is_paragraph_level());
        assert!(!MarkerClass::Note.is_paragraph_level());
    }
}
