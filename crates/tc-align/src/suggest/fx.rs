//! A fast hasher for the strings of the alignment memory: n-gram keys of a few dozen bytes
//! looked up thousands of times for each suggestion. The memory is never iterated, so the
//! hasher decides speed and nothing else; the keys are the project's own words, so a hash
//! that is not keyed against flooding costs nothing.

use std::hash::{BuildHasherDefault, Hasher};

/// FxHash (the rustc hasher): one rotate, xor and multiply for each 8 bytes.
#[derive(Debug, Default, Clone, Copy)]
pub(crate) struct Fx(u64);

const K: u64 = 0x517c_c1b7_2722_0a95;

impl Fx {
    #[inline]
    fn add(&mut self, word: u64) {
        self.0 = (self.0.rotate_left(5) ^ word).wrapping_mul(K);
    }
}

impl Hasher for Fx {
    #[inline]
    fn write(&mut self, bytes: &[u8]) {
        let (words, rest) = bytes.as_chunks::<8>();
        for word in words {
            self.add(u64::from_le_bytes(*word));
        }
        if !rest.is_empty() {
            let mut word = [0u8; 8];
            word[..rest.len()].copy_from_slice(rest);
            self.add(u64::from_le_bytes(word) ^ ((rest.len() as u64) << 56));
        }
    }

    #[inline]
    fn write_u8(&mut self, byte: u8) {
        self.add(u64::from(byte));
    }

    #[inline]
    fn write_u32(&mut self, n: u32) {
        self.add(u64::from(n));
    }

    #[inline]
    fn write_u64(&mut self, n: u64) {
        self.add(n);
    }

    #[inline]
    fn write_usize(&mut self, n: usize) {
        self.add(n as u64);
    }

    /// The last multiplication leaves the high bits best mixed, and the table reads the low
    /// ones: fold them down.
    #[inline]
    fn finish(&self) -> u64 {
        self.0 ^ (self.0 >> 29)
    }
}

pub(crate) type FxBuild = BuildHasherDefault<Fx>;

/// A map from strings or small keys that is looked up often.
pub(crate) type FxMap<K, V> = std::collections::HashMap<K, V, FxBuild>;
