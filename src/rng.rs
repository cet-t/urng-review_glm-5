//! A 32/64-bit random number generator trait.

use crate::_internal::{i2f_bits, randi_wide, u2f_01};

mod sealed {
    /// Prevents `Word` from being implemented outside this crate, so the
    /// `u32 -> f32/i32` / `u64 -> f64/i64` width relationships stay closed.
    pub trait Sealed {}
    impl Sealed for u32 {}
    impl Sealed for u64 {}
}

/// The unsigned output word of a generator (`u32` or `u64`).
///
/// A `Word` fixes the whole numeric family of an RNG — its float type, its
/// signed-range integer type, and the bit width — and centralizes the
/// uniform-conversion formulas in one place so every [`Rng`] implementor
/// inherits them for free.
pub trait Word: sealed::Sealed + Copy {
    /// The float type produced from this word (`f32` for `u32`, `f64` for `u64`).
    type Float: Copy;
    /// The signed integer type used for ranged draws (`i32` / `i64`).
    type Int: Copy;

    /// Maps the word uniformly onto `[0, 1)`.
    #[must_use]
    fn to_f01(self) -> Self::Float;

    /// Maps the word uniformly onto the inclusive integer range `[min, max]`.
    #[must_use]
    fn to_randi(self, min: Self::Int, max: Self::Int) -> Self::Int;

    /// Maps the word uniformly onto the half-open float range `[min, max)`.
    #[must_use]
    fn to_randf(self, min: Self::Float, max: Self::Float) -> Self::Float;

    /// Maps the word uniformly onto a slice index in `[0, len)` (Lemire's
    /// multiply-shift). Equivalent to `to_randi(0, len as Int - 1) as usize`.
    #[must_use]
    fn to_index(self, len: usize) -> usize;
}

macro_rules! impl_word {
    ($bits:tt) => {
        pastey::paste! {
            impl Word for [<u $bits>] {
                type Float = [<f $bits>];
                type Int = [<i $bits>];

                #[inline(always)]
                fn to_f01(self) -> Self::Float {
                    u2f_01!([<f $bits>], $bits, self)
                }

                #[inline(always)]
                fn to_randi(self, min: Self::Int, max: Self::Int) -> Self::Int {
                    // Issue A5 contract, enforced in both profiles:
                    // - `min > max` is a caller bug and panics (previously:
                    //   debug panic, silent garbage in release — the range
                    //   computation turned negative);
                    // - the value mapping is modular (`wrapping_add`), so
                    //   the full-span case (`randi(i32::MIN, i32::MAX)`)
                    //   is exact — the plain `+ min` overflowed on half of
                    //   the draw space in debug builds.
                    assert!(min <= max, "to_randi: invalid span (min {min} > max {max})");
                    let range = (max as randi_wide!(i $bits) - min as randi_wide!(i $bits) + 1)
                        as randi_wide!(u $bits);
                    (((self as randi_wide!(u $bits) * range) >> $bits) as [<i $bits>])
                        .wrapping_add(min)
                }

                #[inline(always)]
                fn to_randf(self, min: Self::Float, max: Self::Float) -> Self::Float {
                    self.to_f01() * (max - min) + min
                }

                #[inline(always)]
                fn to_index(self, len: usize) -> usize {
                    // Issue A5: for `len >= 2^$bits` (only possible on hosts
                    // with more address width), the scaled multiply below
                    // wrapped silently in release and panicked in debug.
                    // Only that case takes the 128-bit path; the common
                    // `len < 2^$bits` path keeps the single wide multiply
                    // (documented in FIX_REPORT.md — the branch is
                    // loop-invariant in typical callers).
                    if len > [<u $bits>]::MAX as usize {
                        ((self as u128 * len as u128) >> $bits) as usize
                    } else {
                        ((self as randi_wide!(u $bits) * len as randi_wide!(u $bits)) >> $bits)
                            as usize
                    }
                }
            }
        }
    };
}

impl_word!(32);
impl_word!(64);

/// A random number generator characterized by a single output [`Word`].
///
/// Implementors provide only [`nextu`](Rng::nextu); the ranged and float
/// draws are supplied as defaults derived from the associated [`Word`], so the
/// numeric formulas live in exactly one place.
pub trait Rng {
    /// The unsigned output word this generator produces.
    type Word: Word;

    /// Generates the next raw word in `[0, 2^BITS)`.
    #[must_use]
    fn nextu(&mut self) -> Self::Word;

    /// Generates the next float in `[0, 1)`.
    #[must_use]
    #[inline(always)]
    fn nextf(&mut self) -> <Self::Word as Word>::Float {
        self.nextu().to_f01()
    }

    /// Generates an integer uniformly in the inclusive range `[min, max]`.
    #[must_use]
    #[inline(always)]
    fn randi(
        &mut self,
        min: <Self::Word as Word>::Int,
        max: <Self::Word as Word>::Int,
    ) -> <Self::Word as Word>::Int {
        self.nextu().to_randi(min, max)
    }

    /// Generates a float uniformly in the half-open range `[min, max)`.
    #[must_use]
    #[inline(always)]
    fn randf(
        &mut self,
        min: <Self::Word as Word>::Float,
        max: <Self::Word as Word>::Float,
    ) -> <Self::Word as Word>::Float {
        self.nextu().to_randf(min, max)
    }
}

/// Constructs a generator from a raw seed of its native word width.
pub trait Seed {
    /// The seed type (`u32` or `u64`, matching the generator's word width).
    type Seed;

    /// Builds `Self` from a seed.
    #[must_use]
    fn from_seed(seed: Self::Seed) -> Self;
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Issue A5: `randi(i32::MIN, i32::MAX)` used to panic with
    /// "attempt to add with overflow" on about half of the draw space in
    /// debug builds (the plain `+ min`). The full range must be covered
    /// exactly and without panicking.
    #[test]
    fn randi_full_span_exact_and_panic_free() {
        let mut rng = crate::Sfc32::new(5);
        let n = 20_000;
        // Four 25% buckets over [i32::MIN, i32::MAX]; every bucket must be
        // well populated (a uniform draw cannot skip an eighth of the
        // span), and the endpoint coverage requires exact modular mapping.
        let mut buckets = [0u32; 4];
        for _ in 0..n {
            let v = rng.randi(i32::MIN, i32::MAX);
            buckets[(v as u32 >> 30) as usize] += 1;
        }
        for (b, &c) in buckets.iter().enumerate() {
            let p = c as f64 / n as f64;
            assert!(
                (0.20..=0.30).contains(&p),
                "bucket {b}: p = {p}, expected ≈ 0.25"
            );
        }
    }

    /// Issue A5: `min > max` must panic consistently in all profiles
    /// (release previously returned silent garbage).
    #[test]
    #[should_panic(expected = "invalid span")]
    fn randi_min_gt_max_panics() {
        let mut rng = crate::Sfc32::new(1);
        let _ = rng.randi(5, 1);
    }

    /// The 64-bit word path has the same contract.
    #[test]
    fn randi_full_span64_exact_and_panic_free() {
        let mut rng = crate::Sfc64::new(6);
        let mut lows = 0u32;
        let mut highs = 0u32;
        for _ in 0..4000 {
            let v = rng.randi(i64::MIN, i64::MAX);
            if v as u64 >> 63 == 0 {
                lows += 1;
            } else {
                highs += 1;
            }
        }
        let p = lows as f64 / (lows + highs) as f64;
        assert!((0.3..=0.7).contains(&p), "sign split p = {p}, expected ≈ 0.5");
    }

    /// Issue A5: `to_index` with `len > 2^32` (64-bit host) used to wrap
    /// silently in release builds and panic in debug ones. The small-`len`
    /// fast path must be unchanged.
    #[test]
    fn to_index_len_over_2pow32_is_exact() {
        let w = 0x9E37_79B9u32;
        // len > u32::MAX: compare against the 128-bit reference.
        for shift in [32u32, 33, 40] {
            let len = (1usize << shift) + 5;
            assert!(len > u32::MAX as usize);
            let want = ((w as u128 * len as u128) >> 32) as usize;
            assert_eq!(w.to_index(len), want, "len 2^{shift}+5");
            assert!(w.to_index(len) < len);
        }

        // Small-len path unchanged: plain u32 mathematics.
        assert_eq!(w.to_index(1000), ((w as u64 * 1000u64) >> 32) as usize);
        assert_eq!(0u32.to_index(7), 0);
        assert_eq!(u32::MAX.to_index(7), ((u32::MAX as u64 * 7) >> 32) as usize);
    }

    /// Full-span to_index on the 64-bit word path must stay exact (it
    /// already used 128-bit math; make it a regression test anyway).
    #[test]
    fn to_index64_full_host_range_exact() {
        let w = 0x9E37_79B9_7F4A_7C15u64;
        let len = usize::MAX / 3;
        let want = ((w as u128 * len as u128) >> 64) as usize;
        assert_eq!(w.to_index(len), want);
        assert!(w.to_index(len) < len);
    }
}
