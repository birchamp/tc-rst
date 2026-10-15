//! Which original-language verse stands behind a project verse.
//!
//! An alignment record is keyed by the project's own chapter and verse. The original-
//! language texts (UGNT, UHB) are in the `eng` frame, so for a project in another scheme
//! the same numbers name a different verse of the source. [`source_reference`] maps the
//! project reference into the `eng` frame for the lookup only; the record keeps its
//! project key. When no single source verse stands behind the project verse it refuses,
//! rather than align the draft against the wrong Greek or Hebrew (tC4 `openAlign`,
//! `test/alignmentFrame.test.ts`).

use tc_core::Reference;
use tc_versification::{
    MapOutcome, RowReference, Scheme, SchemeReference, StandardScheme, Unplaceable,
};

/// The frame every unfoldingWord original-language resource is numbered in.
pub const RESOURCE_FRAME: StandardScheme = StandardScheme::Eng;

/// Why a project verse has no original-language verse to align against.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, thiserror::Error)]
pub enum NoSourceVerse {
    /// The project's scheme is not known, so its numbers cannot be mapped.
    #[error("the project's versification is not known")]
    UnknownFrame,
    /// The mapping refused, for this reason (verse 0 for a psalm superscription, past the
    /// end of the chapter, a chapter the source frame lacks, ...).
    #[error("no single source verse: {0:?}")]
    Unplaceable(Unplaceable),
}

/// The `eng`-frame reference of the original-language verse to align `reference`
/// against, for a project in `project` (`None` when the project's scheme is unknown).
pub fn source_reference(
    project: Option<&Scheme>,
    reference: Reference,
) -> Result<SchemeReference, NoSourceVerse> {
    let project = project.ok_or(NoSourceVerse::UnknownFrame)?;
    match tc_versification::map_reference(
        project,
        RESOURCE_FRAME.scheme(),
        &RowReference::from(reference),
    ) {
        MapOutcome::SameFrame(_) => Ok(SchemeReference::from(reference)),
        MapOutcome::Mapped(mapped) => Ok(mapped),
        MapOutcome::Unplaceable { reason, .. } => Err(NoSourceVerse::Unplaceable(reason)),
    }
}
