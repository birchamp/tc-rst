//! Scripture Burrito 1.0 metadata for tC4 projects.
//!
//! - [`Metadata`] is a typed model of `metadata.json`. Every object in it keeps the fields
//!   the model does not know in an `extra` map, and open enumerations keep values they do
//!   not know, so a read-modify-write never drops data.
//! - [`Metadata::to_canonical_json`] writes the same bytes for the same model: keys sorted
//!   at every level, two-space indent, a trailing newline.
//! - [`Ingredient::from_bytes`] and [`FileFacts::of`] compute md5, size and mimeType from
//!   the exact bytes of a file.
//! - [`validate_metadata_bytes`], [`validate_project`] and [`validate_dir`] run the
//!   embedded SB 1.0.0 JSON schema fully offline, plus the checks that schema misses, and
//!   return a typed list of [`Problem`]s. An empty list means the burrito is valid.
//!
//! # Strict and lenient values
//!
//! Values the app acts on are parsed strictly: ingredient paths (they become file reads),
//! md5 digests, media types and scopes. A document with a bad one does not parse into the
//! model, and the validator says why. Values the app only shows or carries (names, tags,
//! ids, timestamps) are kept as written, and the validator judges them.
//!
//! The JSON reader refuses what a read-modify-write could not give back: a key written
//! twice, a number an f64 cannot hold exactly, a lone surrogate escape. The writer refuses
//! an `extra` map that repeats a modelled key.

mod build;
mod error;
mod files;
mod ingredient;
mod json;
mod model;
mod open_enum;
mod path;
mod schema;
mod scope;
mod time;
mod unzip;
mod validate;

pub use build::{
    DERIVED_FIELDS, IngredientEntry, MetadataSource, build_metadata, check_field,
    check_relationship, check_scope, is_derived_field, render_metadata_unchecked,
};
pub use error::{Error, ValueError};
pub use files::{DirProject, FileState, MemProject, ProjectFiles, UnlistedDir};
pub use ingredient::{FileFacts, Md5, MimeType};
pub use json::to_canonical_json;
pub use model::{
    Agency, AgencyRole, Audience, BurritoType, Category, Checksum, Copyright, Extra, Flavor,
    FlavorType, FlavorTypeName, Format, Generator, IdAuthority, Identification, Ingredient,
    LOCAL_AUTHORITY, Language, License, LocalizedName, LocalizedText, Meta, Metadata, NewProject,
    Normalization, OtherFlavor, PrimaryId, PrimaryIdError, Progress, ProjectKind, ProjectType,
    RelationType, Relationship, Revision, Role, ScriptDirection, ShortStatement, SpecVersion,
    TargetArea, TextStories, TextTranslation, TranslationType,
};
pub use open_enum::UnknownValue;
pub use path::{IngredientPath, PathRule, check_project_path, collision_form};
pub use schema::{SchemaLoadError, is_language_tag, schema_ready};
pub use scope::{Scope, ScopeBook, ScopeRange};
pub use time::{Timestamp, TimestampError};
pub use unzip::{ZIP_LIMITS, ZipEntry, ZipLimits, ZipReadError, read_zip};
pub use validate::{
    CopyrightBasis, Problem, TimestampField, validate_dir, validate_metadata,
    validate_metadata_bytes, validate_project,
};
