//! Alignments to and from aligned USFM (`\zaln-s`/`\zaln-e` milestones around `\w` words).
//!
//! Export: [`usfm_groups`] turns a value's cards into `tc-usfm` groups, and
//! [`tc_usfm::weave`] writes them into the verse in en_ult's layout, byte for byte what
//! tC4's export writes (`tc-usfm`'s `tests/tc4_weave.rs`). [`weave_verse`] does both.
//!
//! Import: [`tc_journal::alignment::split_aligned`] is the one reader of aligned USFM. It
//! gives tC4's cards, one per outermost source word, so an export writes the same links in
//! tC4's bytes, which differ from en_ult's in 5 of the 46 verses of Titus.

use tc_journal::record::AlignmentValue;

/// The cards that align something, as `tc-usfm` groups for [`tc_usfm::weave`]: the
/// journal's [`AlignmentValue::usfm_groups`]. Cards with no placed word, or with no source
/// word, write nothing and are left out.
pub fn usfm_groups(value: &AlignmentValue) -> Vec<tc_usfm::AlignmentGroup> {
    value.usfm_groups()
}

/// The verse body `plain` with `value`'s alignment woven in. Fails as
/// [`tc_usfm::weave`] fails, for instance when the value names a word the verse does
/// not have: a caller weaves only a value that is valid for the verse (STORAGE.md §7.7),
/// as tC4's export does.
pub fn weave_verse(plain: &str, value: &AlignmentValue) -> Result<String, tc_usfm::Error> {
    tc_usfm::weave(plain, &value.usfm_groups())
}
