use wrapn::{wrap, wu64};

use crate::prng::b64::SplitMix64;
use crate::rng::Rng;

// --- Sfc64 ---

/// SFC (Small Fast Chaotic) 64-bit RNG implementation.
///
/// # Example
/// ```
/// use urng::{Rng, Sfc64};
///
/// let mut rng = Sfc64::new(1);
/// let _ = rng.nextu();
/// ```
#[repr(C, align(64))]
#[derive(Debug, Clone, Copy)]
pub struct Sfc64 {
    a: wu64,
    b: wu64,
    c: wu64,
    counter: wu64,
}

impl Sfc64 {
    /// Creates a new `Sfc64` instance with the given seed.
    pub const fn new(seed: u64) -> Self {
        let mut seedgen = SplitMix64::new(seed);
        Self {
            a: wrap!(seedgen.nextu_const()),
            b: wrap!(seedgen.nextu_const()),
            c: wrap!(seedgen.nextu_const()),
            counter: wrap!(1),
        }
    }
}

impl Rng for Sfc64 {
    type Word = u64;

    #[inline(always)]
    fn nextu(&mut self) -> Self::Word {
        let res = self.a + self.b + self.counter;
        self.a = self.b ^ (self.b >> 11);
        self.b = self.c + (self.c << 3);
        self.c = res.rotate_left(24);

        self.counter += 1;
        *res
    }
}

#[cfg(feature = "simd")]
pub use simd::*;

#[cfg(feature = "simd")]
pub mod simd {
    use std::arch::x86_64::*;

    use crate::_internal::{i2f_bits, u2f_01};
    use crate::prng::b64::SplitMix64;

    // --- Sfc64x8 (AVX-512F) ---

    /// An 8-way SIMD SFC64 generator using AVX-512 512-bit intrinsics.
    ///
    /// Requires the `avx512f` target feature at compile time (see
    /// `# Safety`), not just "any AVX support at runtime".
    ///
    /// # Example
    /// ```no_run
    /// use urng::Sfc64x8;
    ///
    /// let mut rng = Sfc64x8::new(0);
    /// let _ = unsafe { rng.nextu() };
    /// ```
    #[cfg(all(feature = "simd", target_arch = "x86_64"))]
    #[repr(C, align(64))]
    pub struct Sfc64x8 {
        a: __m512i,
        b: __m512i,
        c: __m512i,
        counter: __m512i,
    }

    #[cfg(all(feature = "simd", target_arch = "x86_64"))]
    impl Sfc64x8 {
        /// Creates a new `Sfc64x8` from 8 independent seeds.
        ///
        /// # Safety
        /// Requires AVX512 support (guaranteed by `target-cpu=native` on modern x86_64).
        #[inline(always)]
        pub fn new(seed: u64) -> Self {
            let mut a = [0u64; 8];
            let mut b = [0u64; 8];
            let mut c = [0u64; 8];
            let mut sg = SplitMix64::new(seed);
            for i in 0..8 {
                a[i] = sg.nextu_const();
                b[i] = sg.nextu_const();
                c[i] = sg.nextu_const();
            }
            unsafe {
                Self {
                    a: _mm512_loadu_si512(a.as_ptr() as *const __m512i),
                    b: _mm512_loadu_si512(b.as_ptr() as *const __m512i),
                    c: _mm512_loadu_si512(c.as_ptr() as *const __m512i),
                    counter: _mm512_set1_epi64(1),
                }
            }
        }

        /// Generates 8 random `u64` values simultaneously.
        ///
        /// # Safety
        /// Requires AVX512 support (the CPU must support `avx512f`).
        #[inline(always)]
        pub unsafe fn nextu(&mut self) -> [u64; 8] {
            unsafe {
                let one = _mm512_set1_epi64(1);

                // res = a + b + counter
                let res = _mm512_add_epi64(_mm512_add_epi64(self.a, self.b), self.counter);

                // a = b ^ (b >> 11)
                self.a = _mm512_xor_si512(self.b, _mm512_srli_epi64(self.b, 11));

                // b = c + (c << 3)
                self.b = _mm512_add_epi64(self.c, _mm512_slli_epi64(self.c, 3));

                // c = rotate_left(res, 24) = (res << 24) | (res >> 40)
                self.c = _mm512_or_si512(_mm512_slli_epi64(res, 24), _mm512_srli_epi64(res, 40));

                // counter += 1
                self.counter = _mm512_add_epi64(self.counter, one);

                // Store 8 results
                let mut out = [0u64; 8];
                _mm512_storeu_si512(out.as_mut_ptr() as *mut __m512i, res);
                out
            }
        }

        /// Generates 8 random `f64` values in [0, 1) and writes them to `out`.
        ///
        /// # Safety
        ///
        /// The caller must ensure the CPU supports the `avx512f` target feature.
        #[inline(always)]
        pub unsafe fn nextf(&mut self) -> [f64; 8] {
            unsafe {
                let u = self.nextu();
                let mut out = [0f64; 8];
                for i in 0..8 {
                    out[i] = u2f_01!(f64, 64, u[i]);
                }
                out
            }
        }

        /// Generates 8 random `i64` values in [min, max].
        ///
        /// # Safety
        ///
        /// The caller must ensure the CPU supports the `avx512f` target feature.
        #[inline(always)]
        pub unsafe fn randi(&mut self, min: i64, max: i64) -> [i64; 8] {
            // Issue A5: same contract as the scalar `Word::to_randi`  E
            // panic on `min > max`, and modular `+ min` so the full-span
            // case does not overflow (the plain `+` panicked on about half
            // of all draws in debug builds).
            assert!(min <= max, "randi: invalid span (min {min} > max {max})");
            unsafe {
                let u = self.nextu();
                let range = (max as i128 - min as i128 + 1) as u128;
                let mut out = [0i64; 8];
                for i in 0..8 {
                    out[i] = (((u[i] as u128 * range) >> 64) as u64).wrapping_add(min as u64) as i64;
                }
                out
            }
        }

        /// Generates 8 random `f64` values in [min, max) and writes them to `out`.
        ///
        /// # Safety
        ///
        /// The caller must ensure the CPU supports the `avx512f` target feature.
        #[inline(always)]
        pub unsafe fn randf(&mut self, min: f64, max: f64) -> [f64; 8] {
            unsafe {
                let u = self.nextu();
                let range = max - min;
                let mut out = [0f64; 8];
                for i in 0..8 {
                    out[i] = u2f_01!(f64, 64, u[i]) * range + min;
                }
                out
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    crate::safe_test! { Sfc64 }

    #[cfg(all(feature = "simd", target_feature = "avx512f"))]
    crate::unsafe_test! { Sfc64x8 }

    /// Issue A5 (regression), lane-exact: the inline SIMD `randi` used a
    /// plain `+ min` and overflowed on about half of the full-span draw
    /// space in debug builds. Lane seeding of `Sfc64x8::new` matches the
    /// scalar `SplitMix64` draws 3i..3i+2 with counter 1, so the scalar
    /// `Sfc64` reproduces every lane exactly and the mapping must agree
    /// bit-for-bit with `wrapping_add`.
    #[cfg(all(feature = "simd", target_arch = "x86_64"))]
    #[test]
    fn sfc64x8_randi_full_span_lane_exact() {
        if !std::arch::is_x86_feature_detected!("avx512f") {
            eprintln!("skipping Sfc64x8 randi test: avx512f not detected");
            return;
        }
        let mut seedgen = SplitMix64::new(31);
        let mut lanes: [Sfc64; 8] = std::array::from_fn(|_| Sfc64 {
            a: wrap!(seedgen.nextu_const()),
            b: wrap!(seedgen.nextu_const()),
            c: wrap!(seedgen.nextu_const()),
            counter: wrap!(1),
        });

        let mut rng = unsafe { Sfc64x8::new(31) };
        for _ in 0..64 {
            let got = unsafe { rng.randi(i64::MIN, i64::MAX) };
            for i in 0..8 {
                let u = lanes[i].nextu();
                // Full-span range is 2^64, so the high word is `u` itself
                // and the mapping is `u` wrapped with `min`.
                let want = u.wrapping_add(i64::MIN as u64) as i64;
                assert_eq!(got[i], want, "full-span lane {i}");
            }
        }
    }

    /// Issue A5: `min > max` must panic (was silent garbage in release).
    #[cfg(all(feature = "simd", target_arch = "x86_64"))]
    #[test]
    #[should_panic]
    fn sfc64x8_randi_min_gt_max_panics() {
        if !std::arch::is_x86_feature_detected!("avx512f") {
            panic!("unsupported host (requires avx512f; nothing to assert)");
        }
        let mut rng = unsafe { Sfc64x8::new(32) };
        let _ = unsafe { rng.randi(5, 1) };
    }
}
