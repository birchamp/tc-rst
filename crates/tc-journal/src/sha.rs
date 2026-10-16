//! The one SHA-256 every hash of the format goes through: change ids (§2.5), the names of
//! packs and pending files (§4.3), conflict ids and the frontier (§2.6). A first open without
//! the fold cache hashes every byte of every change once (§14), so this is the hottest code of
//! that open on a processor without SHA instructions.
//!
//! The implementation is chosen on one line, [`Chosen`]. On a processor without SHA
//! instructions, the kind the reference laptop is, `ring` 0.17.14 hashed the whole-Bible
//! corpus's change lines (1,076 bytes on average) at 292 MB/s against 168 MB/s for `sha2`
//! 0.11.1, and whole files at 320 MB/s against 179 MB/s, on one thread (medians of 7,
//! `examples/shabench.rs` in `tc-corpus`; `docs/evidence/storage-fold-2026-10-16.md`). Both take the SHA instructions where a processor has them; if
//! `sha2` is ever measured faster on such a processor, make `Chosen` be `Sha2`. The two give
//! the same digests (`the_chosen_sha256_is_sha2s`).

/// See the module documentation. `ring` does not build for wasm32-unknown-unknown (it needs
/// a random source there), so a wasm32 build, which nothing makes today, takes `sha2`.
#[cfg(not(target_arch = "wasm32"))]
type Chosen = Ring;
#[cfg(target_arch = "wasm32")]
type Chosen = Sha2;

/// SHA-256 of `bytes`.
pub(crate) fn sha256(bytes: &[u8]) -> [u8; 32] {
    Chosen::digest(bytes)
}

/// SHA-256 of bytes given in pieces.
pub(crate) struct Sha256Stream(<Chosen as Engine>::Stream);

impl Sha256Stream {
    pub(crate) fn new() -> Self {
        Self(Chosen::stream())
    }

    pub(crate) fn update(&mut self, bytes: &[u8]) {
        Chosen::update(&mut self.0, bytes);
    }

    pub(crate) fn finish(self) -> [u8; 32] {
        Chosen::finish(self.0)
    }
}

trait Engine {
    type Stream;
    fn digest(bytes: &[u8]) -> [u8; 32];
    fn stream() -> Self::Stream;
    fn update(stream: &mut Self::Stream, bytes: &[u8]);
    fn finish(stream: Self::Stream) -> [u8; 32];
}

#[cfg(not(target_arch = "wasm32"))]
struct Ring;

/// A `ring` digest's 32 bytes.
#[cfg(not(target_arch = "wasm32"))]
fn bytes_of(digest: ring::digest::Digest) -> [u8; 32] {
    let mut out = [0u8; 32];
    // SHA-256 digests are 32 bytes, so every byte of `out` is written.
    for (slot, byte) in out.iter_mut().zip(digest.as_ref()) {
        *slot = *byte;
    }
    out
}

#[cfg(not(target_arch = "wasm32"))]
impl Engine for Ring {
    type Stream = ring::digest::Context;

    fn digest(bytes: &[u8]) -> [u8; 32] {
        bytes_of(ring::digest::digest(&ring::digest::SHA256, bytes))
    }

    fn stream() -> Self::Stream {
        ring::digest::Context::new(&ring::digest::SHA256)
    }

    fn update(stream: &mut Self::Stream, bytes: &[u8]) {
        stream.update(bytes);
    }

    fn finish(stream: Self::Stream) -> [u8; 32] {
        bytes_of(stream.finish())
    }
}

// Kept so that the choice above stays one line; the test below holds the two to one result.
#[cfg_attr(all(not(test), not(target_arch = "wasm32")), allow(dead_code))]
struct Sha2;

impl Engine for Sha2 {
    type Stream = sha2::Sha256;

    fn digest(bytes: &[u8]) -> [u8; 32] {
        use sha2::Digest;
        sha2::Sha256::digest(bytes).into()
    }

    fn stream() -> Self::Stream {
        use sha2::Digest;
        sha2::Sha256::new()
    }

    fn update(stream: &mut Self::Stream, bytes: &[u8]) {
        use sha2::Digest;
        stream.update(bytes);
    }

    fn finish(stream: Self::Stream) -> [u8; 32] {
        use sha2::Digest;
        stream.finalize().into()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(512))]

        /// The chosen implementation gives `sha2`'s digest for any bytes, whole or in any
        /// pieces, so choosing either one never changes an id or a name.
        #[test]
        fn the_chosen_sha256_is_sha2s(
            bytes in prop::collection::vec(any::<u8>(), 0..2048),
            cuts in prop::collection::vec(any::<prop::sample::Index>(), 0..4),
        ) {
            let expected = Sha2::digest(&bytes);
            prop_assert_eq!(sha256(&bytes), expected);
            let mut cuts: Vec<usize> = cuts.iter().map(|c| c.index(bytes.len() + 1)).collect();
            cuts.sort_unstable();
            let mut stream = Sha256Stream::new();
            let mut from = 0;
            for cut in cuts.into_iter().chain([bytes.len()]) {
                stream.update(&bytes[from..cut]);
                from = cut;
            }
            prop_assert_eq!(stream.finish(), expected);
        }
    }

    /// Lengths around SHA-256's 64-byte block and its padding, where implementations differ
    /// if they differ at all.
    #[test]
    fn the_chosen_sha256_is_sha2s_at_every_block_edge() {
        let bytes: Vec<u8> = (0..300u32).map(|n| n.wrapping_mul(37) as u8).collect();
        for len in 0..bytes.len() {
            assert_eq!(
                sha256(&bytes[..len]),
                Sha2::digest(&bytes[..len]),
                "{len} bytes"
            );
        }
    }
}
