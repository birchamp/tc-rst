//! tC4's views (`src/views` in translationCore 4), one module per tC4 file, read top to bottom
//! (APP-DESIGN 6.1, 6.12). They draw what `use_app()` holds and call its actions. wasm32 with
//! the `app` feature only.
//!
//! The modules index with `get` and never with `[]` (APP-DESIGN 6.5, A-15).
#![deny(clippy::indexing_slicing)]

pub mod account_menu;
pub mod help_panel;
pub mod modals;
