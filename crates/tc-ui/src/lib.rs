//! The Leptos UI of the tC4 rebuild.
//!
//! [`ds`] is tC4's design system (`src/ds` in translationCore 4): one Leptos component per
//! React component, with the same name, the same props and the same DOM (elements, `data-*`
//! hooks, inline styles, ARIA attributes, test ids), so tC4's own CSS in `assets/` applies
//! unchanged. The `gallery` feature adds the page that draws every case of
//! `parity/ds-gallery/gallery-spec.json` for the pixel comparison with tC4.
//!
//! The components only run in a browser, so they compile for `wasm32` only. The pure parts
//! (inline-style building, keyword enums, layout arithmetic, string rules) compile everywhere
//! and are tested on the host.

#[macro_use]
pub mod css;
pub mod attrs;
pub mod geometry;
pub mod text;
pub mod tokens;

#[cfg(target_arch = "wasm32")]
pub mod ds;

#[cfg(all(target_arch = "wasm32", feature = "gallery"))]
pub mod gallery;
