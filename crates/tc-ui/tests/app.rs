//! The behaviour of the app runtime's pure parts through the public API of tc-ui. One test
//! binary, so the crate and its dependencies link once.
#![cfg(not(target_arch = "wasm32"))]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod app {
    mod buffer;
    mod cache;
    mod client;
    mod effects;
    mod sse;
}
