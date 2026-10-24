use wrapn::{wrap, wu32};

use crate::prng::b32::SplitMix32;
use crate::rng::Rng;

/// A xoroshiro64** random number generator.
///
/// A fast, high-quality 32-bit generator with a 64-bit state.
///
/// # Example
/// ```
/// use urng::{Rng, Xoroshiro64Ss};
///
/// let mut rng = Xoroshiro64Ss::new(12345);
/// let _ = rng.nextu();
/// ```
#[repr(C, align(64))]
#[derive(Debug, Clone, Copy)]
pub struct Xoroshiro64Ss {
    s: [wu32; 2],
}

impl Xoroshiro64Ss {
    /// Creates a new `Xoroshiro64Ss` instance with the given seed.
    pub const fn new(seed: u32) -> Self {
        let mut seedgen = SplitMix32::new(seed);

        Self {
            s: wrap![seedgen.nextu_const(), seedgen.nextu_const()],
        }
    }
}

impl Rng for Xoroshiro64Ss {
    type Word = u32;

    #[inline(always)]
    fn nextu(&mut self) -> Self::Word {
        let s0 = self.s[0];
        let mut s1 = self.s[1];
        let result = (s0 * 0x9E3779BB).rotate_left(5) * 5;

        s1 ^= s0;
        self.s[0] = s0.rotate_left(26) ^ s1 ^ (s1 << 9);
        self.s[1] = s1.rotate_left(13);

        *result
    }
}

#[cfg(feature = "simd")]
pub use simd::*;

#[cfg(feature = "simd")]
pub mod simd {
    use std::arch::x86_64::*;

    use crate::prng::b32::SplitMix32;
    use crate::rngv::RngV;

    pub(crate) const XOROSHIRO64SSX8: usize = 8;

    /// 8-way SIMD implementation of xoroshiro64** 32-bit RNG.
    /// This implementation uses AVX2 instructions to generate 8 random numbers in parallel.
    ///
    /// # Example
    /// ```
    /// use urng::{RngV, Xoroshiro64Ssx8};
    ///
    /// let mut rng = unsafe { Xoroshiro64Ssx8::new(12345) };
    /// let _ = rng.nextuv();
    /// ```
    #[cfg(target_arch = "x86_64")]
    #[repr(C, align(64))]
    pub struct Xoroshiro64Ssx8 {
        s0: __m256i,
        s1: __m256i,
    }

    #[allow(dead_code)]
    impl Xoroshiro64Ssx8 {
        /// # Safety
        /// This function requires AVX2 support. Ensure that the CPU supports it and that the code is compiled with the appropriate target features.
        #[target_feature(enable = "avx2")]
        pub fn new(seed: u32) -> Self {
            let mut seedgen = SplitMix32::new(seed);

            let mut s0 = [0u32; XOROSHIRO64SSX8];
            let mut s1 = [0u32; XOROSHIRO64SSX8];

            for i in 0..XOROSHIRO64SSX8 {
                s0[i] = seedgen.nextu_const();
                s1[i] = seedgen.nextu_const();
            }

            unsafe {
                Self {
                    s0: _mm256_loadu_si256(s0.as_ptr() as *const __m256i),
                    s1: _mm256_loadu_si256(s1.as_ptr() as *const __m256i),
                }
            }
        }
    }

    impl RngV for Xoroshiro64Ssx8 {
        type Word = __m256i;

        #[inline]
        fn nextuv(&mut self) -> Self::Word {
            let s0 = self.s0;
            let mut s1 = self.s1;

            unsafe {
                let mult = _mm256_set1_epi32(0x9E3779BBu32 as i32);
                let mullo = _mm256_mullo_epi32(s0, mult);
                // The ** scrambler: the output is `rotl(mullo, 5) * 5`, the
                // same as the scalar `Xoroshiro64Ss` (issue A9: the vector
                // version previously returned the plain mullo, diverging
                // from the scalar stream). Both ops are AVX2-only.
                let rotl5 = _mm256_or_si256(
                    _mm256_slli_epi32(mullo, 5),
                    _mm256_srli_epi32(mullo, 27),
                );
                let result = _mm256_add_epi32(rotl5, _mm256_slli_epi32(rotl5, 2));

                s1 = _mm256_xor_si256(s1, s0);
                self.s0 = _mm256_xor_si256(
                    _mm256_or_si256(
                        _mm256_slli_epi32(s0, 26),
                        _mm256_srli_epi32(s0, 6),
                    ),
                    _mm256_xor_si256(s1, _mm256_slli_epi32(s1, 9)),
                );
                self.s1 = _mm256_or_si256(
                    _mm256_slli_epi32(s1, 13),
                    _mm256_srli_epi32(s1, 19),
                );

                result
            }
        }
    }

    pub(crate) const XOROSHIRO64SSX16: usize = 16;

    /// 16-way SIMD implementation of xoroshiro64** 32-bit RNG.
    /// This implementation uses AVX-512F instructions to generate 16 random numbers in parallel.
    ///
    /// # Example
    /// ```no_run
    /// use urng::{RngV, Xoroshiro64Ssx16};
    ///
    /// let mut rng = unsafe { Xoroshiro64Ssx16::new(12345) };
    /// let _ = rng.nextuv();
    /// ```
    #[cfg(target_arch = "x86_64")]
    #[repr(C, align(64))]
    pub struct Xoroshiro64Ssx16 {
        s0: __m512i,
        s1: __m512i,
    }

    #[allow(dead_code)]
    impl Xoroshiro64Ssx16 {
        /// # Safety
        /// This function requires AVX-512F support. Ensure that the CPU supports it and that the code is compiled with the appropriate target features.
        #[target_feature(enable = "avx512f")]
        pub fn new(seed: u32) -> Self {
            let mut seedgen = SplitMix32::new(seed);

            let mut s0 = [0u32; XOROSHIRO64SSX16];
            let mut s1 = [0u32; XOROSHIRO64SSX16];

            for i in 0..XOROSHIRO64SSX16 {
                s0[i] = seedgen.nextu_const();
                s1[i] = seedgen.nextu_const();
            }

            unsafe {
                Self {
                    s0: _mm512_loadu_si512(s0.as_ptr() as _),
                    s1: _mm512_loadu_si512(s1.as_ptr() as _),
                }
            }
        }
    }

    impl RngV for Xoroshiro64Ssx16 {
        type Word = __m512i;

        #[inline]
        fn nextuv(&mut self) -> Self::Word {
            let s0 = self.s0;
            let mut s1 = self.s1;

            unsafe {
                let mult = _mm512_set1_epi32(0x9E3779BBu32 as i32);
                let mullo = _mm512_mullo_epi32(s0, mult);
                // The ** scrambler: output = `rotl(mullo, 5) * 5`, matching
                // the scalar `Xoroshiro64Ss` (issue A9: this vector variant
                // previously returned the plain mullo). `_mm512_rol_epi32`
                // is AVX512F-tier here, matching this type's target feature.
                let result = _mm512_mullo_epi32(_mm512_rol_epi32(mullo, 5), _mm512_set1_epi32(5));

                s1 = _mm512_xor_si512(s1, s0);
                self.s0 = _mm512_xor_si512(
                    _mm512_xor_si512(_mm512_rol_epi32(s0, 26), s1),
                    _mm512_slli_epi32(s1, 9),
                );
                self.s1 = _mm512_rol_epi32(s1, 13);

                result
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    crate::safe_test! { Xoroshiro64Ss }
    #[cfg(all(feature = "simd", target_feature = "avx512f"))]
    crate::unsafe_test! { Xoroshiro64Ssx16 }

    /// Regression test for issue A9: the vector variants returned the plain
    /// `s0 * 0x9E3779BB` product while the scalar `Xoroshiro64Ss` applies
    /// the ** scrambler `(x).rotl(5) * 5`, so SIMD and scalar streams
    /// diverged from the very first draw. (Also covers A2: the AVX2 tier of
    /// `Xoroshiro64Ssx8` previously used the AVX-512VL
    /// `_mm256_rol_epi32`.)
    #[cfg(all(feature = "simd", target_arch = "x86_64"))]
    #[test]
    fn xoroshiro64ssx8_lanes_match_scalar() {
        use crate::RngV;

        if !std::arch::is_x86_feature_detected!("avx2") {
            eprintln!("skipping Xoroshiro64Ssx8 lane test: avx2 not detected");
            return;
        }
        // Seeding contract of `Xoroshiro64Ssx8::new`: lane i gets
        // s0 = draw 2i, s1 = draw 2i+1 (contiguous pairs).
        let mut scalars: [Xoroshiro64Ss; 8] = {
            let mut seedgen = SplitMix32::new(22);
            std::array::from_fn(|_| Xoroshiro64Ss {
                s: wrap![seedgen.nextu(), seedgen.nextu()],
            })
        };

        let mut vector = unsafe { Xoroshiro64Ssx8::new(22) };
        for _ in 0..128 {
            let got: [u32; 8] = unsafe { std::mem::transmute(vector.nextuv()) };
            let want: [u32; 8] = std::array::from_fn(|i| scalars[i].nextu());
            assert_eq!(got, want);
        }
    }

    /// Same contract for the AVX-512 tier (issue A9 in the 16-lane variant;
    /// its `_mm512_rol_epi32` is within the type's `avx512f` requirement).
    #[cfg(all(feature = "simd", target_arch = "x86_64"))]
    #[test]
    fn xoroshiro64ssx16_lanes_match_scalar() {
        use crate::RngV;

        if !std::arch::is_x86_feature_detected!("avx512f") {
            eprintln!("skipping Xoroshiro64Ssx16 lane test: avx512f not detected");
            return;
        }
        let mut scalars: [Xoroshiro64Ss; 16] = {
            let mut seedgen = SplitMix32::new(23);
            std::array::from_fn(|_| Xoroshiro64Ss {
                s: wrap![seedgen.nextu(), seedgen.nextu()],
            })
        };

        let mut vector = unsafe { Xoroshiro64Ssx16::new(23) };
        for _ in 0..128 {
            let got: [u32; 16] = unsafe { std::mem::transmute(vector.nextuv()) };
            let want: [u32; 16] = std::array::from_fn(|i| scalars[i].nextu());
            assert_eq!(got, want);
        }
    }
}
