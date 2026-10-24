use std::arch::x86_64::*;

use crate::_internal::simd_f01;

mod sealed {
    use std::arch::x86_64::{__m128i, __m256i, __m512i};

    pub trait Sealed {}
    impl Sealed for __m128i {}
    impl Sealed for __m256i {}
    impl Sealed for __m512i {}
}

pub trait WordV: sealed::Sealed + Copy + Sized {
    type Int;
    type Float;

    #[must_use]
    fn to_f01(self) -> Self::Float;

    #[must_use]
    fn to_randi(self, scale: Self::Int, min: Self::Int) -> Self::Int;

    #[must_use]
    fn to_randf(self, scale: Self::Float, min: Self::Float) -> Self::Float;
}

impl WordV for __m128i {
    type Int = Self;
    type Float = __m128;

    fn to_f01(self) -> Self::Float {
        unsafe { simd_f01::u32x4(self) }
    }

    fn to_randi(self, scale: Self::Int, min: Self::Int) -> Self::Int {
        // Per-lane uniform mapping, mirroring the (correct) `Sfc32x4::randiv`:
        // even 32-bit lanes take the high word of `u_even * scale_even`, odd
        // lanes the high word of `u_odd * scale_odd`, then `+ min`, all with
        // plain SSE2 integer ops. The previous version paired the odd `u`
        // words with the *even* `scale` words and selected the *low* word of
        // that product through an fp-shuffle merge (issue A8), corrupting
        // every odd lane for any non-broadcast `scale`.
        unsafe {
            let hi = _mm_set1_epi64x((0xffff_ffffu64 << 32) as i64);
            let res_even = _mm_srli_epi64(_mm_mul_epu32(self, scale), 32);
            let prod_odd = _mm_and_si128(
                _mm_mul_epu32(_mm_srli_epi64(self, 32), _mm_srli_epi64(scale, 32)),
                hi,
            );
            _mm_add_epi32(_mm_or_si128(res_even, prod_odd), min)
        }
    }

    fn to_randf(self, scale: Self::Float, min: Self::Float) -> Self::Float {
        unsafe {
            _mm_add_ps(_mm_mul_ps(simd_f01::u32x4(self), scale), min)
        }
    }
}

impl WordV for __m256i {
    type Int = Self;
    type Float = __m256;

    fn to_f01(self) -> Self::Float {
        unsafe { simd_f01::u32x8(self) }
    }

    fn to_randi(self, scale: Self::Int, min: Self::Int) -> Self::Int {
        // Same per-lane mapping as the `__m128i` implementation above
        // (eight lanes: even lanes take the high word of
        // `u_even * scale_even`, odd lanes that of `u_odd * scale_odd`).
        // Fixes two issues: the odd `u` words were paired with the *even*
        // `scale` words (issue A8), and the merge used
        // `_mm256_mask_blend_epi32`, an AVX-512VL intrinsic that is illegal
        // under plain AVX2 (issue A2) — the OR merge below is AVX2-only.
        unsafe {
            let hi = _mm256_set1_epi64x((0xffff_ffffu64 << 32) as i64);
            let res_even = _mm256_srli_epi64(_mm256_mul_epu32(self, scale), 32);
            let prod_odd = _mm256_and_si256(
                _mm256_mul_epu32(_mm256_srli_epi64(self, 32), _mm256_srli_epi64(scale, 32)),
                hi,
            );
            _mm256_add_epi32(_mm256_or_si256(res_even, prod_odd), min)
        }
    }

    fn to_randf(self, scale: Self::Float, min: Self::Float) -> Self::Float {
        unsafe {
            _mm256_add_ps(_mm256_mul_ps(simd_f01::u32x8(self), scale), min)
        }
    }
}

impl WordV for __m512i {
    type Int = Self;
    type Float = __m512;

    fn to_f01(self) -> Self::Float {
        unsafe { simd_f01::u32x16(self) }
    }

    fn to_randi(self, scale: Self::Int, min: Self::Int) -> Self::Int {
        // Same per-lane mapping as the `__m256i` implementation above (even
        // lanes: high word of `u_even * scale_even`; odd lanes: high word of
        // `u_odd * scale_odd`). Fix for issue A8: the odd `u` words were
        // previously paired with the *even* `scale` words. The mask blend is
        // AVX512F-tier, matching this operand type's feature requirement,
        // and its odd-lane picks land on the high words of `prod_odd`.
        const MERGE_MASK: u16 = 0b1010_1010_1010_1010;
        unsafe {
            let res_even = _mm512_srli_epi64(_mm512_mul_epu32(self, scale), 32);
            let prod_odd = _mm512_mul_epu32(
                _mm512_srli_epi64(self, 32),
                _mm512_srli_epi64(scale, 32),
            );
            let merged = _mm512_mask_blend_epi32(MERGE_MASK, res_even, prod_odd);
            _mm512_add_epi32(merged, min)
        }
    }

    fn to_randf(self, scale: Self::Float, min: Self::Float) -> Self::Float {
        unsafe {
            _mm512_add_ps(_mm512_mul_ps(simd_f01::u32x16(self), scale), min)
        }
    }
}

pub trait RngV {
    type Word: WordV;

    fn nextuv(&mut self) -> Self::Word;

    fn nextfv(&mut self) -> <Self::Word as WordV>::Float {
        self.nextuv().to_f01()
    }

    fn randiv(
        &mut self,
        scale: <Self::Word as WordV>::Int,
        min: <Self::Word as WordV>::Int,
    ) -> <Self::Word as WordV>::Int {
        self.nextuv().to_randi(scale, min)
    }

    fn randfv(
        &mut self,
        scale: <Self::Word as WordV>::Float,
        min: <Self::Word as WordV>::Float,
    ) -> <Self::Word as WordV>::Float {
        self.nextuv().to_randf(scale, min)
    }
}

#[cfg(all(test, target_arch = "x86_64"))]
mod tests {
    use super::*;

    /// The per-lane contract of `WordV::to_randi` as plain scalar math:
    /// the high word of `u * scale` plus `min` (wrapping), matching
    /// `Word::to_randi(min, min + scale - 1)` in `rng.rs`.
    fn ref_randi(u: u32, scale: u32, min: u32) -> u32 {
        ((u as u64 * scale as u64) >> 32).wrapping_add(min as u64) as u32
    }

    #[test]
    fn to_randi_m128i_matches_scalar_reference_per_lane() {
        // Distinct per-lane scales pair each odd `u` lane with a different
        // `scale` word than its neighbor; a large scale forces the mapping
        // to depend on the real high word of the 64-bit product (the old
        // shuffle-based merge took the *low* word here).
        let us = [0x1234_5678u32, 0x9abc_def0, 0xdead_beef, 0x0246_8ace];
        let scales = [1u32, 0x1FFF_FFF1, 0x3FFF_FFE3, 0x7FFF_FFC5];
        let mins = [-3i32, 0, 123_456_789, 0x7FFF_FFF0];

        let v_u = unsafe { std::mem::transmute::<[u32; 4], __m128i>(us) };
        let v_scale = unsafe { std::mem::transmute::<[u32; 4], __m128i>(scales) };
        let v_min = unsafe { std::mem::transmute::<[i32; 4], __m128i>(mins) };
        let got: [u32; 4] = unsafe { std::mem::transmute(v_u.to_randi(v_scale, v_min)) };
        for i in 0..4 {
            let want = ref_randi(us[i], scales[i], mins[i] as u32);
            assert_eq!(got[i], want, "__m128i lane {i}");
        }
    }

    #[test]
    fn to_randi_m128i_broadcast_scale_unchanged() {
        // Behavior-preservation check: a broadcast scale must map every
        // lane identically (the document semantic used by the C ABI).
        let v_u = unsafe { _mm_set1_epi32(0x5555_5555) };
        let v_scale = unsafe { _mm_set1_epi32(1000) };
        let v_min = unsafe { _mm_set1_epi32(5) };
        let got: [u32; 4] = unsafe { std::mem::transmute(v_u.to_randi(v_scale, v_min)) };
        let want = ref_randi(0x5555_5555, 1000, 5);
        assert_eq!(got, [want; 4]);
    }

    #[test]
    fn to_randi_m256i_matches_scalar_reference_per_lane() {
        if !std::arch::is_x86_feature_detected!("avx2") {
            eprintln!("skipping __m256i to_randi test: avx2 not detected");
            return;
        }
        let us: [u32; 8] =
            std::array::from_fn(|i| (i as u32).wrapping_mul(0x85EB_CA6B).wrapping_add(9));
        // Adjacent scale lanes differ by ~2^28, so mixing up even/odd
        // partners shifts the product's high word with near-certainty.
        let scales: [u32; 8] =
            std::array::from_fn(|i| (((i + 1) * 0x0FFF_FFF3) & 0x7FFF_FFFF) as u32);
        let mins: [i32; 8] = std::array::from_fn(|i| i as i32 - 4);

        let v_u = unsafe { std::mem::transmute::<[u32; 8], __m256i>(us) };
        let v_scale = unsafe { std::mem::transmute::<[u32; 8], __m256i>(scales) };
        let v_min = unsafe { std::mem::transmute::<[i32; 8], __m256i>(mins) };
        let got: [u32; 8] = unsafe { std::mem::transmute(v_u.to_randi(v_scale, v_min)) };
        for i in 0..8 {
            let want = ref_randi(us[i], scales[i], mins[i] as u32);
            assert_eq!(got[i], want, "__m256i lane {i}");
        }
    }

    #[test]
    fn to_randi_m512i_matches_scalar_reference_per_lane() {
        if !std::arch::is_x86_feature_detected!("avx512f") {
            eprintln!("skipping __m512i to_randi test: avx512f not detected");
            return;
        }
        let us: [u32; 16] =
            std::array::from_fn(|i| (i as u32).wrapping_mul(0x9E37_79B9).wrapping_add(1));
        let scales: [u32; 16] =
            std::array::from_fn(|i| (((i + 1) * 0x007F_FFF9) & 0x7FFF_FFFF) as u32);
        let mins: [i32; 16] = std::array::from_fn(|i| i as i32 - 8);

        let v_u = unsafe { std::mem::transmute::<[u32; 16], __m512i>(us) };
        let v_scale = unsafe { std::mem::transmute::<[u32; 16], __m512i>(scales) };
        let v_min = unsafe { std::mem::transmute::<[i32; 16], __m512i>(mins) };
        let got: [u32; 16] = unsafe { std::mem::transmute(v_u.to_randi(v_scale, v_min)) };
        for i in 0..16 {
            let want = ref_randi(us[i], scales[i], mins[i] as u32);
            assert_eq!(got[i], want, "__m512i lane {i}");
        }
    }
}
