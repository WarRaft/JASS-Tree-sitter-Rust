//! Blizzard `StringHash` implementation (Bob Jenkins' lookup2 hash).
//!
//! This is a faithful Rust port of the C `SStrHash2` function used by the
//! Warcraft III engine for the `StringHash` JASS native.  It is used at
//! build time to fold `StringHash(expr)` calls into integer constants when
//! the argument can be fully evaluated at compile time.

// ─── Jenkins hash ────────────────────────────────────────────────────────────

/// Bob Jenkins' lookup2 `hash` — the core mixing function.
fn jenkins_hash(k: &[u8], initval: u32) -> u32 {
    let length = k.len() as u32;
    let mut a: u32 = 0x9e3779b9;
    let mut b: u32 = 0x9e3779b9;
    let mut c: u32 = initval;

    let mut i = 0usize;
    let mut len = k.len();

    // Handle most of the key (12-byte chunks).
    while len >= 12 {
        a = a.wrapping_add(
            k[i] as u32
                | (k[i + 1] as u32) << 8
                | (k[i + 2] as u32) << 16
                | (k[i + 3] as u32) << 24,
        );
        b = b.wrapping_add(
            k[i + 4] as u32
                | (k[i + 5] as u32) << 8
                | (k[i + 6] as u32) << 16
                | (k[i + 7] as u32) << 24,
        );
        c = c.wrapping_add(
            k[i + 8] as u32
                | (k[i + 9] as u32) << 8
                | (k[i + 10] as u32) << 16
                | (k[i + 11] as u32) << 24,
        );
        mix(&mut a, &mut b, &mut c);
        i += 12;
        len -= 12;
    }

    // Handle the last 11 bytes.
    c = c.wrapping_add(length);
    #[allow(clippy::identity_op)]
    match len {
        11 => {
            c = c.wrapping_add((k[i + 10] as u32) << 24);
            c = c.wrapping_add((k[i + 9] as u32) << 16);
            c = c.wrapping_add((k[i + 8] as u32) << 8);
            b = b.wrapping_add((k[i + 7] as u32) << 24);
            b = b.wrapping_add((k[i + 6] as u32) << 16);
            b = b.wrapping_add((k[i + 5] as u32) << 8);
            b = b.wrapping_add(k[i + 4] as u32);
            a = a.wrapping_add((k[i + 3] as u32) << 24);
            a = a.wrapping_add((k[i + 2] as u32) << 16);
            a = a.wrapping_add((k[i + 1] as u32) << 8);
            a = a.wrapping_add(k[i + 0] as u32);
        }
        10 => {
            c = c.wrapping_add((k[i + 9] as u32) << 16);
            c = c.wrapping_add((k[i + 8] as u32) << 8);
            b = b.wrapping_add((k[i + 7] as u32) << 24);
            b = b.wrapping_add((k[i + 6] as u32) << 16);
            b = b.wrapping_add((k[i + 5] as u32) << 8);
            b = b.wrapping_add(k[i + 4] as u32);
            a = a.wrapping_add((k[i + 3] as u32) << 24);
            a = a.wrapping_add((k[i + 2] as u32) << 16);
            a = a.wrapping_add((k[i + 1] as u32) << 8);
            a = a.wrapping_add(k[i + 0] as u32);
        }
        9 => {
            c = c.wrapping_add((k[i + 8] as u32) << 8);
            b = b.wrapping_add((k[i + 7] as u32) << 24);
            b = b.wrapping_add((k[i + 6] as u32) << 16);
            b = b.wrapping_add((k[i + 5] as u32) << 8);
            b = b.wrapping_add(k[i + 4] as u32);
            a = a.wrapping_add((k[i + 3] as u32) << 24);
            a = a.wrapping_add((k[i + 2] as u32) << 16);
            a = a.wrapping_add((k[i + 1] as u32) << 8);
            a = a.wrapping_add(k[i + 0] as u32);
        }
        8 => {
            b = b.wrapping_add((k[i + 7] as u32) << 24);
            b = b.wrapping_add((k[i + 6] as u32) << 16);
            b = b.wrapping_add((k[i + 5] as u32) << 8);
            b = b.wrapping_add(k[i + 4] as u32);
            a = a.wrapping_add((k[i + 3] as u32) << 24);
            a = a.wrapping_add((k[i + 2] as u32) << 16);
            a = a.wrapping_add((k[i + 1] as u32) << 8);
            a = a.wrapping_add(k[i + 0] as u32);
        }
        7 => {
            b = b.wrapping_add((k[i + 6] as u32) << 16);
            b = b.wrapping_add((k[i + 5] as u32) << 8);
            b = b.wrapping_add(k[i + 4] as u32);
            a = a.wrapping_add((k[i + 3] as u32) << 24);
            a = a.wrapping_add((k[i + 2] as u32) << 16);
            a = a.wrapping_add((k[i + 1] as u32) << 8);
            a = a.wrapping_add(k[i + 0] as u32);
        }
        6 => {
            b = b.wrapping_add((k[i + 5] as u32) << 8);
            b = b.wrapping_add(k[i + 4] as u32);
            a = a.wrapping_add((k[i + 3] as u32) << 24);
            a = a.wrapping_add((k[i + 2] as u32) << 16);
            a = a.wrapping_add((k[i + 1] as u32) << 8);
            a = a.wrapping_add(k[i + 0] as u32);
        }
        5 => {
            b = b.wrapping_add(k[i + 4] as u32);
            a = a.wrapping_add((k[i + 3] as u32) << 24);
            a = a.wrapping_add((k[i + 2] as u32) << 16);
            a = a.wrapping_add((k[i + 1] as u32) << 8);
            a = a.wrapping_add(k[i + 0] as u32);
        }
        4 => {
            a = a.wrapping_add((k[i + 3] as u32) << 24);
            a = a.wrapping_add((k[i + 2] as u32) << 16);
            a = a.wrapping_add((k[i + 1] as u32) << 8);
            a = a.wrapping_add(k[i + 0] as u32);
        }
        3 => {
            a = a.wrapping_add((k[i + 2] as u32) << 16);
            a = a.wrapping_add((k[i + 1] as u32) << 8);
            a = a.wrapping_add(k[i + 0] as u32);
        }
        2 => {
            a = a.wrapping_add((k[i + 1] as u32) << 8);
            a = a.wrapping_add(k[i + 0] as u32);
        }
        1 => {
            a = a.wrapping_add(k[i + 0] as u32);
        }
        _ => {} // case 0: nothing left to add
    }
    mix(&mut a, &mut b, &mut c);
    c
}

/// The Jenkins lookup2 mix macro, translated to Rust.
#[inline]
fn mix(a: &mut u32, b: &mut u32, c: &mut u32) {
    *a = a.wrapping_sub(*b).wrapping_sub(*c) ^ (*c >> 13);
    *b = b.wrapping_sub(*c).wrapping_sub(*a) ^ (*a << 8);
    *c = c.wrapping_sub(*a).wrapping_sub(*b) ^ (*b >> 13);
    *a = a.wrapping_sub(*b).wrapping_sub(*c) ^ (*c >> 12);
    *b = b.wrapping_sub(*c).wrapping_sub(*a) ^ (*a << 16);
    *c = c.wrapping_sub(*a).wrapping_sub(*b) ^ (*b >> 5);
    *a = a.wrapping_sub(*b).wrapping_sub(*c) ^ (*c >> 3);
    *b = b.wrapping_sub(*c).wrapping_sub(*a) ^ (*a << 10);
    *c = c.wrapping_sub(*a).wrapping_sub(*b) ^ (*b >> 15);
}

/// Compute the Blizzard `StringHash` value for a string.
///
/// Mirrors the C `SStrHash2` function:
/// - lowercase `a`–`z` → uppercase (subtract `0x20`)
/// - `/` → `\`
/// - everything else unchanged
///
/// Returns the hash as an `i32` (matching JASS `integer` semantics).
#[allow(dead_code)]
pub fn blizzard_string_hash(key: &str) -> i32 {
    let mut buf: Vec<u8> = Vec::with_capacity(key.len());
    for &byte in key.as_bytes() {
        if byte >= b'a' && byte <= b'z' {
            buf.push(byte - 0x20);
        } else if byte == b'/' {
            buf.push(b'\\');
        } else {
            buf.push(byte);
        }
    }
    jenkins_hash(&buf, 0) as i32
}
