//! Byte encodings used on the wire: lowercase hex, RFC 4648 base32 for device ids and
//! base64url for shown tokens. Each decoder accepts exactly the text its encoder writes.

use sha2::{Digest, Sha256};

pub(crate) fn sha256(bytes: &[u8]) -> [u8; 32] {
    Sha256::digest(bytes).into()
}

const HEX: &[u8; 16] = b"0123456789abcdef";

pub(crate) fn hex(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        out.push(char::from(HEX[usize::from(b >> 4)]));
        out.push(char::from(HEX[usize::from(b & 0x0f)]));
    }
    out
}

fn hex_digit(c: u8) -> Option<u8> {
    match c {
        b'0'..=b'9' => Some(c - b'0'),
        b'a'..=b'f' => Some(c - b'a' + 10),
        _ => None,
    }
}

/// Lowercase hex of exactly `N` bytes.
pub(crate) fn unhex<const N: usize>(text: &str) -> Option<[u8; N]> {
    let bytes = text.as_bytes();
    if bytes.len() != N * 2 {
        return None;
    }
    let mut out = [0u8; N];
    for (slot, pair) in out.iter_mut().zip(bytes.chunks_exact(2)) {
        *slot = (hex_digit(pair[0])? << 4) | hex_digit(pair[1])?;
    }
    Some(out)
}

const BASE32: &[u8; 32] = b"abcdefghijklmnopqrstuvwxyz234567";

/// RFC 4648 base32, lowercase, no padding.
pub(crate) fn base32(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len().div_ceil(5) * 8);
    let mut buffer: u32 = 0;
    let mut bits = 0u32;
    for b in bytes {
        buffer = (buffer << 8) | u32::from(*b);
        bits += 8;
        while bits >= 5 {
            bits -= 5;
            out.push(char::from(BASE32[((buffer >> bits) & 31) as usize]));
        }
    }
    if bits > 0 {
        out.push(char::from(BASE32[((buffer << (5 - bits)) & 31) as usize]));
    }
    out
}

/// The inverse of [`base32`] for exactly `N` bytes. Refuses unused bits that are not zero,
/// so each value has one text.
pub(crate) fn unbase32<const N: usize>(text: &str) -> Option<[u8; N]> {
    if text.len() != (N * 8).div_ceil(5) {
        return None;
    }
    let mut out = [0u8; N];
    let mut buffer: u32 = 0;
    let mut bits = 0u32;
    let mut at = 0;
    for c in text.bytes() {
        let value = BASE32.iter().position(|x| *x == c)? as u32;
        buffer = ((buffer << 5) | value) & 0xffff;
        bits += 5;
        if bits >= 8 {
            bits -= 8;
            *out.get_mut(at)? = (buffer >> bits) as u8;
            at += 1;
        }
    }
    (at == N && buffer & ((1 << bits) - 1) == 0).then_some(out)
}

const BASE64URL: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";

/// RFC 4648 base64url, no padding.
pub(crate) fn base64url(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let b = [
            chunk[0],
            *chunk.get(1).unwrap_or(&0),
            *chunk.get(2).unwrap_or(&0),
        ];
        let n = (u32::from(b[0]) << 16) | (u32::from(b[1]) << 8) | u32::from(b[2]);
        let chars = chunk.len() + 1;
        for i in 0..chars {
            out.push(char::from(BASE64URL[((n >> (18 - 6 * i)) & 63) as usize]));
        }
    }
    out
}

/// The inverse of [`base64url`]. Refuses padding, other alphabets and unused bits that
/// are not zero.
pub(crate) fn unbase64url(text: &str) -> Option<Vec<u8>> {
    let bytes = text.as_bytes();
    if bytes.len() % 4 == 1 {
        return None;
    }
    let mut out = Vec::with_capacity(bytes.len() * 3 / 4);
    for chunk in bytes.chunks(4) {
        let mut n: u32 = 0;
        for (i, c) in chunk.iter().enumerate() {
            let value = BASE64URL.iter().position(|x| x == c)? as u32;
            n |= value << (18 - 6 * i);
        }
        let produced = chunk.len() - 1;
        let all = [(n >> 16) as u8, (n >> 8) as u8, n as u8];
        if all[produced..].iter().any(|b| *b != 0) && produced < 3 {
            return None;
        }
        out.extend_from_slice(&all[..produced]);
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_round_trips_and_refuses_uppercase() {
        let bytes = [0u8, 1, 0xab, 0xff];
        assert_eq!(hex(&bytes), "0001abff");
        assert_eq!(unhex::<4>("0001abff"), Some(bytes));
        assert_eq!(unhex::<4>("0001ABFF"), None);
        assert_eq!(unhex::<4>("0001abf"), None);
    }

    #[test]
    fn base32_has_one_text_per_value() {
        let bytes = [1u8, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 255];
        let text = base32(&bytes);
        assert_eq!(text.len(), 26);
        assert_eq!(unbase32::<16>(&text), Some(bytes));
        // The last character carries two unused bits; setting them makes another text.
        let mut other = text.into_bytes();
        let last = other.len() - 1;
        let index = BASE32.iter().position(|c| *c == other[last]).unwrap_or(0);
        other[last] = BASE32[index | 1];
        let other = String::from_utf8(other).unwrap_or_default();
        assert_eq!(unbase32::<16>(&other), None);
        assert_eq!(base32(b"foobar"), "mzxw6ytboi");
    }

    #[test]
    fn base64url_matches_rfc4648_vectors() {
        for (raw, text) in [
            (&b""[..], ""),
            (b"f", "Zg"),
            (b"fo", "Zm8"),
            (b"foo", "Zm9v"),
            (b"foob", "Zm9vYg"),
            (b"fooba", "Zm9vYmE"),
            (b"foobar", "Zm9vYmFy"),
            (&[0xfb, 0xff], "-_8"),
        ] {
            assert_eq!(base64url(raw), text);
            assert_eq!(unbase64url(text).as_deref(), Some(raw));
        }
        assert_eq!(unbase64url("Zh"), None, "unused bits set");
        assert_eq!(unbase64url("Zm9v="), None);
        assert_eq!(unbase64url("Z"), None);
    }
}
