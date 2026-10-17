//! Random bytes from the operating system.

use std::sync::atomic::{AtomicU64, Ordering};

/// `N` random bytes. Secrets ([`secret`]) need the operating system's source; everything
/// else here (temporary names, batch ids, comment ids) only needs to be unlikely to repeat,
/// so if the source fails it falls back to the clock, the process and a counter, mixed.
pub fn bytes<const N: usize>() -> [u8; N] {
    let mut out = [0u8; N];
    if getrandom::fill(&mut out).is_err() {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let mut state = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_nanos() as u64)
            ^ (u64::from(std::process::id()) << 32)
            ^ COUNTER
                .fetch_add(1, Ordering::Relaxed)
                .wrapping_mul(0x9e37_79b9_7f4a_7c15);
        for chunk in out.chunks_mut(8) {
            // splitmix64
            state = state.wrapping_add(0x9e37_79b9_7f4a_7c15);
            let mut z = state;
            z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
            z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
            z ^= z >> 31;
            chunk.copy_from_slice(&z.to_le_bytes()[..chunk.len()]);
        }
    }
    out
}

/// `N` random bytes from the operating system, or an error: an installation secret is never
/// made from a guessable source.
pub fn secret<const N: usize>() -> Result<[u8; N], String> {
    let mut out = [0u8; N];
    getrandom::fill(&mut out).map_err(|e| format!("no random source: {e}"))?;
    Ok(out)
}

/// `N` random bytes as lowercase hex.
pub fn hex<const N: usize>() -> String {
    to_hex(&bytes::<N>())
}

pub(crate) fn to_hex(bytes: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        out.push(DIGITS[usize::from(b >> 4)] as char);
        out.push(DIGITS[usize::from(b & 15)] as char);
    }
    out
}
