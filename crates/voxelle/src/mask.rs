// SPDX-License-Identifier: MIT OR Apache-2.0
// Copyright (c) lapoule.dev

//! Bit masks as slices of 64-bit words: bit `i` is bit `i % 64` of word
//! `i / 64`.

#[inline]
pub(crate) fn get(words: &[u64], i: usize) -> bool {
    (words[i >> 6] >> (i & 63)) & 1 != 0
}

#[inline]
pub(crate) fn put(words: &mut [u64], i: usize, on: bool) {
    let bit = 1u64 << (i & 63);
    if on {
        words[i >> 6] |= bit;
    } else {
        words[i >> 6] &= !bit;
    }
}

/// Whether every bit is `on`.
pub(crate) fn all(words: &[u64], on: bool) -> bool {
    let w = if on { !0 } else { 0 };
    words.iter().all(|&x| x == w)
}

/// The number of bits on.
pub(crate) fn count(words: &[u64]) -> u64 {
    words.iter().map(|w| u64::from(w.count_ones())).sum()
}

/// The first bit on at or after `from`.
#[inline]
pub(crate) fn next_on(words: &[u64], from: usize) -> Option<usize> {
    next_on_with(words.len(), from, |i| words[i])
}

/// The first bit on in `a | b` at or after `from` (same lengths).
#[inline]
pub(crate) fn next_on_either(a: &[u64], b: &[u64], from: usize) -> Option<usize> {
    next_on_with(a.len(), from, |i| a[i] | b[i])
}

#[inline]
fn next_on_with(len: usize, from: usize, word: impl Fn(usize) -> u64) -> Option<usize> {
    let mut wi = from >> 6;
    if wi >= len {
        return None;
    }
    let mut w = word(wi) & (!0u64 << (from & 63));
    loop {
        if w != 0 {
            return Some(wi * 64 + w.trailing_zeros() as usize);
        }
        wi += 1;
        if wi == len {
            return None;
        }
        w = word(wi);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn next_on_walks_every_set_bit_in_order() {
        let mut m = [0u64; 3];
        for i in [0, 5, 63, 64, 130, 191] {
            put(&mut m, i, true);
        }
        let mut seen = vec![];
        let mut from = 0;
        while let Some(i) = next_on(&m, from) {
            seen.push(i);
            from = i + 1;
        }
        assert_eq!(seen, [0, 5, 63, 64, 130, 191]);
        assert_eq!(count(&m), 6);
        assert_eq!(next_on(&m, 192), None);
        put(&mut m, 5, false);
        assert!(!get(&m, 5));
        assert_eq!(next_on(&m, 1), Some(63));
    }

    #[test]
    fn next_on_either_merges_two_masks() {
        let (a, b) = ([1u64 << 3, 0], [0u64, 1 << 1]);
        assert_eq!(next_on_either(&a, &b, 0), Some(3));
        assert_eq!(next_on_either(&a, &b, 4), Some(65));
        assert!(all(&[0, 0], false) && all(&[!0], true) && !all(&[1], true));
    }
}
