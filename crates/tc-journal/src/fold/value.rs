//! Where the fold finds a change's value (§14: values are byte ranges read on demand).
//!
//! The fold keeps a summary of each change (its key, links, batch and the shape of its
//! value) and reads the value itself only to merge, to list a conflict's candidates or to
//! project a record. A loader that reads packs from disk hands the fold where each change's
//! bytes lie ([`ValueAt`]) and a [`ValueSource`] that reads them again, so a whole-Bible
//! project does not keep half a gigabyte of change bytes in memory.

use crate::line::Span;
use std::fmt;
use std::sync::Arc;

/// Where a loader found a change's canonical JSON: a file it can read again, named by the
/// loader's own number, and the span of the JSON in it. The fold keeps it and hands it back
/// to its [`ValueSource`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ValueAt {
    pub offset: u64,
    pub file: u32,
    pub len: u32,
}

impl ValueAt {
    pub fn new(file: u32, span: Span) -> Self {
        Self {
            offset: span.offset,
            file,
            len: span.len,
        }
    }
}

/// Reads the values of changes whose bytes the fold does not keep. `tc-store` reads the pack
/// or pending file again, or the pack's blob in the object database when the file is gone or
/// damaged; a source that cannot read a value reports it as a notice of its own.
pub trait ValueSource: Send + Sync + fmt::Debug {
    /// The bytes at `at`, or `None` when they cannot be read. The fold checks them against
    /// the fingerprint it took of the change's verified bytes, so a file changed since it
    /// was read never passes for the change.
    fn read(&self, at: ValueAt) -> Option<Vec<u8>>;
}

/// A fingerprint of a change's canonical bytes, taken when they were verified against the
/// change's id and checked whenever they are read again (§2.6); the fold cache keys each file
/// by the same function of its bytes (`crate::index`). SHA-256 would also prove the id, but
/// the bytes were proved when they were read; computing it again for every value a
/// projection reads was a third of the projection's work without SHA instructions (§14).
///
/// Four lanes take the 8-byte words in turn, and each step is `lane = mix(lane ^ word)`,
/// where `mix` is a bijection whose every input bit flips about half its output bits. The
/// lanes and the length are then folded through the same step. Because every step is one to
/// one, inputs of one length that differ only within one aligned 8-byte word always get
/// different fingerprints; inputs that differ in several words get equal ones only when
/// those words' differences cancel exactly, about 2⁻⁶⁴ for damage that is not made on
/// purpose. It is not a defence against forgery: anyone can compute a collision. (FxHash,
/// used before, missed every two-bit burst that flipped bit 7 of the last byte of a word and
/// bit 4 of the next byte, because multiplying never moves a difference in the top bit.)
pub(crate) fn fingerprint(bytes: &[u8]) -> u64 {
    let mut fingerprinter = Fingerprinter::new();
    fingerprinter.update(bytes);
    fingerprinter.finish()
}

const LANES: usize = 4;
const BLOCK: usize = 8 * LANES;

/// splitmix64's finalizer (Steele, Lea and Flood, 2014): xor-shifts and multiplications by
/// odd constants, each invertible, so the whole is a bijection of `u64`.
fn mix(mut x: u64) -> u64 {
    x ^= x >> 30;
    x = x.wrapping_mul(0xbf58_476d_1ce4_e5b9);
    x ^= x >> 27;
    x = x.wrapping_mul(0x94d0_49bb_1331_11eb);
    x ^ (x >> 31)
}

fn word(bytes: &[u8]) -> u64 {
    let mut word = [0u8; 8];
    word[..bytes.len()].copy_from_slice(bytes);
    u64::from_le_bytes(word)
}

/// [`fingerprint`] of bytes that arrive in pieces, such as a file read a chunk at a time:
/// the same value as the fingerprint of all of them at once.
#[derive(Clone)]
pub struct Fingerprinter {
    lanes: [u64; LANES],
    /// The bytes of the last block, until it is whole.
    carry: [u8; BLOCK],
    carried: usize,
    len: u64,
}

impl Default for Fingerprinter {
    fn default() -> Self {
        Self::new()
    }
}

impl Fingerprinter {
    pub fn new() -> Self {
        // Distinct starting values, so words that trade lanes do not cancel.
        Self {
            lanes: [1, 2, 3, 4].map(|n: u64| n.wrapping_mul(0x9e37_79b9_7f4a_7c15)),
            carry: [0; BLOCK],
            carried: 0,
            len: 0,
        }
    }

    pub fn update(&mut self, mut bytes: &[u8]) {
        self.len += bytes.len() as u64;
        if self.carried > 0 {
            let take = (BLOCK - self.carried).min(bytes.len());
            self.carry[self.carried..self.carried + take].copy_from_slice(&bytes[..take]);
            self.carried += take;
            bytes = &bytes[take..];
            if self.carried < BLOCK {
                return;
            }
            let block = self.carry;
            self.block(&block);
            self.carried = 0;
        }
        let mut blocks = bytes.chunks_exact(BLOCK);
        for block in &mut blocks {
            self.block(block);
        }
        let rest = blocks.remainder();
        self.carry[..rest.len()].copy_from_slice(rest);
        self.carried = rest.len();
    }

    fn block(&mut self, block: &[u8]) {
        for (lane, bytes) in self.lanes.iter_mut().zip(block.chunks_exact(8)) {
            *lane = mix(*lane ^ word(bytes));
        }
    }

    pub fn finish(&self) -> u64 {
        let mut lanes = self.lanes;
        // The last words of an unfinished block, the last one padded with zeros; the
        // length, folded in below, tells the padding from bytes.
        for (lane, bytes) in lanes.iter_mut().zip(self.carry[..self.carried].chunks(8)) {
            *lane = mix(*lane ^ word(bytes));
        }
        let mut out = 0;
        for lane in lanes {
            out = mix(out ^ lane);
        }
        mix(out ^ self.len)
    }
}

impl fmt::Debug for Fingerprinter {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Fingerprinter({} bytes)", self.len)
    }
}

/// A change's bytes as a loader hands them to the fold ([`super::FoldInput`]) and the fold
/// cache keeps them; the change table keeps the same in 20 bytes ([`super::meta::ValueRef`]).
#[derive(Debug, Clone)]
pub(crate) enum Stored {
    /// In memory: a change added by a command, or a loader that keeps every value.
    Bytes(Box<Arc<[u8]>>),
    /// Read from the fold's source when needed, and checked against the [`fingerprint`] of
    /// the bytes the loader verified.
    At {
        file: u32,
        offset: u32,
        len: u32,
        fingerprint: u64,
    },
}

impl Stored {
    pub(crate) fn bytes(bytes: Arc<[u8]>) -> Self {
        Self::Bytes(Box::new(bytes))
    }

    /// Where `at` says, for the verified `bytes` found there, when its offset fits in 32
    /// bits (a pack holds at most 64 MiB; only a pending file of more than 4 GiB does not
    /// fit, and its values stay in memory).
    pub(crate) fn located(at: ValueAt, bytes: &[u8]) -> Option<Self> {
        Some(Self::At {
            file: at.file,
            offset: u32::try_from(at.offset).ok()?,
            len: at.len,
            fingerprint: fingerprint(bytes),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Bytes of the shape a pack holds, long enough for every lane and a partial block.
    fn sample() -> Vec<u8> {
        let line = br#"{"at":1760520000000,"batch":{"id":"6f0c","n":2},"cause":{"edit":{}}}"#;
        line.iter().copied().cycle().take(1000).collect()
    }

    #[test]
    fn a_difference_within_one_word_always_changes_the_fingerprint() {
        let bytes = sample();
        let whole = fingerprint(&bytes);
        for at in 0..bytes.len() {
            for bit in 0..8 {
                let mut damaged = bytes.clone();
                damaged[at] ^= 1 << bit;
                assert_ne!(fingerprint(&damaged), whole, "bit {bit} of byte {at}");
            }
        }
    }

    /// Every two-bit change within 16 adjacent bytes across a word boundary, at boundaries
    /// between every pair of lanes: FxHash, used before, gave equal fingerprints for one such
    /// pattern at every boundary.
    #[test]
    fn two_bit_bursts_across_a_word_boundary_change_the_fingerprint() {
        let bytes = sample();
        let whole = fingerprint(&bytes);
        for boundary in (8..=96).step_by(8).chain([bytes.len() - 8]) {
            for a in 0..64 {
                for b in 0..64 {
                    let mut damaged = bytes.clone();
                    damaged[boundary - 8 + a / 8] ^= 1 << (a % 8);
                    damaged[boundary + b / 8] ^= 1 << (b % 8);
                    assert_ne!(fingerprint(&damaged), whole, "{boundary}: {a} {b}");
                }
            }
        }
    }

    #[test]
    fn pieces_give_the_fingerprint_of_the_whole() {
        let bytes = sample();
        for cut in 0..=bytes.len() {
            for second in [cut, (cut + 7).min(bytes.len()), (cut + 33).min(bytes.len())] {
                let mut pieces = Fingerprinter::new();
                pieces.update(&bytes[..cut]);
                pieces.update(&bytes[cut..second]);
                pieces.update(&bytes[second..]);
                assert_eq!(pieces.finish(), fingerprint(&bytes), "{cut} {second}");
            }
        }
        // The length tells trailing zeros from the padding of the last word.
        assert_ne!(fingerprint(b"ab"), fingerprint(b"ab\0"));
        assert_ne!(fingerprint(&[0; 32]), fingerprint(&[0; 31]));
    }
}
