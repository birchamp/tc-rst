//! A small deterministic random number generator (xoshiro256**, seeded by SplitMix64), so
//! the corpus is the same on every machine and in every build for one seed.

#[derive(Debug, Clone)]
pub struct Rng {
    s: [u64; 4],
}

fn splitmix(state: &mut u64) -> u64 {
    *state = state.wrapping_add(0x9e37_79b9_7f4a_7c15);
    let mut z = *state;
    z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    z ^ (z >> 31)
}

impl Rng {
    pub fn new(seed: u64) -> Self {
        let mut state = seed;
        Self {
            s: [
                splitmix(&mut state),
                splitmix(&mut state),
                splitmix(&mut state),
                splitmix(&mut state),
            ],
        }
    }

    /// An independent stream for one named part, so adding a draw in one part does not
    /// change every other part.
    pub fn fork(&self, label: &str) -> Self {
        let mut seed = self.s[0] ^ self.s[3].rotate_left(17);
        for byte in label.bytes() {
            seed = (seed ^ u64::from(byte)).wrapping_mul(0x0100_0000_01b3);
        }
        Self::new(seed)
    }

    pub fn next_u64(&mut self) -> u64 {
        let result = self.s[1].wrapping_mul(5).rotate_left(7).wrapping_mul(9);
        let t = self.s[1] << 17;
        self.s[2] ^= self.s[0];
        self.s[3] ^= self.s[1];
        self.s[1] ^= self.s[2];
        self.s[0] ^= self.s[3];
        self.s[2] ^= t;
        self.s[3] = self.s[3].rotate_left(45);
        result
    }

    /// A number in `0..n`; 0 when `n` is 0.
    pub fn below(&mut self, n: u64) -> u64 {
        if n == 0 {
            return 0;
        }
        // Multiply-shift: unbiased enough for a corpus and branch-free.
        ((u128::from(self.next_u64()) * u128::from(n)) >> 64) as u64
    }

    pub fn index(&mut self, len: usize) -> usize {
        self.below(len as u64) as usize
    }

    /// True with probability `p`.
    pub fn chance(&mut self, p: f64) -> bool {
        (self.next_u64() >> 11) as f64 / (1u64 << 53) as f64 <= p && p > 0.0
    }

    pub fn bytes<const N: usize>(&mut self) -> [u8; N] {
        let mut out = [0u8; N];
        for chunk in out.chunks_mut(8) {
            let word = self.next_u64().to_le_bytes();
            chunk.copy_from_slice(&word[..chunk.len()]);
        }
        out
    }

    pub fn pick<'a, T>(&mut self, items: &'a [T]) -> Option<&'a T> {
        if items.is_empty() {
            None
        } else {
            items.get(self.index(items.len()))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn one_seed_gives_one_stream() {
        let mut a = Rng::new(7);
        let mut b = Rng::new(7);
        let mut c = Rng::new(8);
        let xs: Vec<u64> = (0..8).map(|_| a.next_u64()).collect();
        let ys: Vec<u64> = (0..8).map(|_| b.next_u64()).collect();
        let zs: Vec<u64> = (0..8).map(|_| c.next_u64()).collect();
        assert_eq!(xs, ys);
        assert_ne!(xs, zs);
        let mut r = Rng::new(1);
        assert!((0..1000).all(|_| r.below(10) < 10));
        assert_eq!(r.below(0), 0);
        let forked = Rng::new(1).fork("a").next_u64();
        assert_ne!(forked, Rng::new(1).fork("b").next_u64());
    }
}
