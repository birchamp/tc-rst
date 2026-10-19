//! The storage side of tc-app (APP-DESIGN §4.2). The seam itself (`ProjectStore`,
//! `ProjectHandle`, the token wrappers) and the `MemoryBackend` come with unit B1b; unit B1a
//! holds only the seed that backend folds: STORAGE §13.2's state seed of a tC4 project, with
//! its check ([`seed`], feature `fixture-backend`, never in release builds, AD35).

#[cfg(feature = "fixture-backend")]
pub mod seed;
