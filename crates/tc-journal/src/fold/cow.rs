//! Containers the fold keeps its state in. Cloning one copies a few hundred pointers at
//! most, so a snapshot of a whole-Bible fold is effectively O(1) (§7.8); a write after a
//! snapshot copies only the chunk or shard it touches.
//!
//! The hash maps use a fixed hasher with no random state. Nothing that reaches an output
//! depends on their iteration order (I1): every list the fold hands out is sorted.

use std::collections::HashMap;
use std::hash::{BuildHasherDefault, Hash, Hasher};
use std::sync::Arc;

/// FxHash: one multiply and rotate per word. The keys are content hashes or small
/// structured keys, so a cryptographic or seeded hasher buys nothing here.
#[derive(Default, Clone, Copy)]
pub(crate) struct FxHasher(u64);

const SEED: u64 = 0x51_7c_c1_b7_27_22_0a_95;

impl FxHasher {
    fn add(&mut self, word: u64) {
        self.0 = (self.0.rotate_left(5) ^ word).wrapping_mul(SEED);
    }
}

impl Hasher for FxHasher {
    fn finish(&self) -> u64 {
        self.0
    }

    fn write(&mut self, bytes: &[u8]) {
        let mut chunks = bytes.chunks_exact(8);
        for chunk in &mut chunks {
            let mut word = [0u8; 8];
            word.copy_from_slice(chunk);
            self.add(u64::from_le_bytes(word));
        }
        let rest = chunks.remainder();
        if !rest.is_empty() {
            let mut word = [0u8; 8];
            word[..rest.len()].copy_from_slice(rest);
            self.add(u64::from_le_bytes(word));
        }
    }

    fn write_u8(&mut self, n: u8) {
        self.add(u64::from(n));
    }

    fn write_u16(&mut self, n: u16) {
        self.add(u64::from(n));
    }

    fn write_u32(&mut self, n: u32) {
        self.add(u64::from(n));
    }

    fn write_u64(&mut self, n: u64) {
        self.add(n);
    }

    fn write_usize(&mut self, n: usize) {
        self.add(n as u64);
    }
}

pub(crate) type FxBuild = BuildHasherDefault<FxHasher>;
pub(crate) type FxMap<K, V> = HashMap<K, V, FxBuild>;

const CHUNK: usize = 4096;

/// An append-only vector in shared chunks.
#[derive(Debug)]
pub(crate) struct ChunkVec<T> {
    chunks: Vec<Arc<Vec<T>>>,
    len: usize,
}

impl<T> Clone for ChunkVec<T> {
    fn clone(&self) -> Self {
        Self {
            chunks: self.chunks.clone(),
            len: self.len,
        }
    }
}

impl<T> Default for ChunkVec<T> {
    fn default() -> Self {
        Self {
            chunks: Vec::new(),
            len: 0,
        }
    }
}

impl<T: Clone> ChunkVec<T> {
    pub(crate) fn len(&self) -> usize {
        self.len
    }

    /// Appends `value` and returns its index.
    pub(crate) fn push(&mut self, value: T) -> usize {
        let index = self.len;
        if index.is_multiple_of(CHUNK) {
            self.chunks.push(Arc::new(Vec::with_capacity(CHUNK)));
        }
        if let Some(last) = self.chunks.last_mut() {
            Arc::make_mut(last).push(value);
        }
        self.len += 1;
        index
    }

    pub(crate) fn get_mut(&mut self, index: usize) -> Option<&mut T> {
        Arc::make_mut(self.chunks.get_mut(index / CHUNK)?).get_mut(index % CHUNK)
    }

    pub(crate) fn iter(&self) -> impl Iterator<Item = &T> {
        self.chunks.iter().flat_map(|chunk| chunk.iter())
    }
}

impl<T> std::ops::Index<usize> for ChunkVec<T> {
    type Output = T;

    /// Panics on an index the vector never handed out, as `Vec` does. The fold indexes its
    /// tables only with indices `push` returned, and never removes an element.
    fn index(&self, index: usize) -> &T {
        &self.chunks[index / CHUNK][index % CHUNK]
    }
}

impl<T: Clone> std::ops::IndexMut<usize> for ChunkVec<T> {
    fn index_mut(&mut self, index: usize) -> &mut T {
        &mut Arc::make_mut(&mut self.chunks[index / CHUNK])[index % CHUNK]
    }
}

const SHARDS: usize = 64;

/// A hash map in shared shards.
#[derive(Debug)]
pub(crate) struct CowMap<K, V> {
    shards: Vec<Arc<FxMap<K, V>>>,
    len: usize,
}

impl<K, V> Clone for CowMap<K, V> {
    fn clone(&self) -> Self {
        Self {
            shards: self.shards.clone(),
            len: self.len,
        }
    }
}

impl<K, V> Default for CowMap<K, V> {
    fn default() -> Self {
        Self {
            shards: (0..SHARDS).map(|_| Arc::new(FxMap::default())).collect(),
            len: 0,
        }
    }
}

fn shard_of<K: Hash + ?Sized>(key: &K) -> usize {
    let mut hasher = FxHasher::default();
    key.hash(&mut hasher);
    // Bits 40 to 45, which the shard's own table reads for neither purpose: hashbrown takes
    // the bucket from the low bits and a 7-bit tag from the top bits. Taking the shard from
    // the top bits left every key of a shard with nearly the same tag, so each probe compared
    // keys at about half the slots of a group, and a whole-Bible fold spent seconds there.
    ((hasher.finish() >> 40) as usize) % SHARDS
}

impl<K: Hash + Eq + Clone, V: Clone> CowMap<K, V> {
    pub(crate) fn len(&self) -> usize {
        self.len
    }

    pub(crate) fn get(&self, key: &K) -> Option<&V> {
        self.shards.get(shard_of(key))?.get(key)
    }

    pub(crate) fn contains_key(&self, key: &K) -> bool {
        self.get(key).is_some()
    }

    pub(crate) fn get_mut(&mut self, key: &K) -> Option<&mut V> {
        let shard = self.shards.get_mut(shard_of(key))?;
        if !shard.contains_key(key) {
            return None;
        }
        Arc::make_mut(shard).get_mut(key)
    }

    pub(crate) fn insert(&mut self, key: K, value: V) -> Option<V> {
        let shard = shard_of(&key);
        let old = self
            .shards
            .get_mut(shard)
            .and_then(|s| Arc::make_mut(s).insert(key, value));
        if old.is_none() {
            self.len += 1;
        }
        old
    }

    pub(crate) fn remove(&mut self, key: &K) -> Option<V> {
        let shard = self.shards.get_mut(shard_of(key))?;
        if !shard.contains_key(key) {
            return None;
        }
        let old = Arc::make_mut(shard).remove(key);
        if old.is_some() {
            self.len -= 1;
        }
        old
    }

    /// Makes room for about `additional` more entries, spread over the shards.
    pub(crate) fn reserve(&mut self, additional: usize) {
        let per_shard = additional / SHARDS + additional / (SHARDS * 8) + 1;
        for shard in &mut self.shards {
            Arc::make_mut(shard).reserve(per_shard);
        }
    }

    /// The value at `key`, inserted from `make` when there is none.
    pub(crate) fn entry_or(&mut self, key: K, make: impl FnOnce() -> V) -> &mut V {
        let shard = shard_of(&key);
        let map = Arc::make_mut(&mut self.shards[shard]);
        let len = &mut self.len;
        map.entry(key).or_insert_with(|| {
            *len += 1;
            make()
        })
    }

    /// Every entry, in no particular order: callers sort what they hand out.
    pub(crate) fn iter(&self) -> impl Iterator<Item = (&K, &V)> {
        self.shards.iter().flat_map(|shard| shard.iter())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_clone_is_a_snapshot() {
        let mut v: ChunkVec<u32> = ChunkVec::default();
        for i in 0..10_000 {
            assert_eq!(v.push(i), i as usize);
        }
        let snapshot = v.clone();
        if let Some(x) = v.get_mut(5) {
            *x = 99;
        }
        v.push(7);
        assert_eq!(snapshot[5], 5);
        assert_eq!(snapshot.len(), 10_000);
        assert_eq!(v[5], 99);
        assert_eq!(v[10_000], 7);
        assert_eq!(v.iter().count(), 10_001);

        let mut m: CowMap<u32, u32> = CowMap::default();
        for i in 0..1000 {
            m.insert(i, i);
        }
        let snapshot = m.clone();
        m.insert(5, 50);
        m.remove(&6);
        *m.entry_or(2000, || 0) += 1;
        assert_eq!(snapshot.get(&5), Some(&5));
        assert_eq!(snapshot.get(&6), Some(&6));
        assert_eq!(snapshot.len(), 1000);
        assert_eq!(m.get(&5), Some(&50));
        assert_eq!(m.get(&6), None);
        assert_eq!(m.get(&2000), Some(&1));
        assert_eq!(m.len(), 1000);
        assert_eq!(m.iter().count(), 1000);
    }
}
