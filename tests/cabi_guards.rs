//! Phase 7 regression tests (C ABI hardening + the SIMD full-span `rand_i32s`
//! defect).
//!
//! Covered contracts:
//! 1. Bulk entry points are no-ops for `count == 0`, a null rng pointer, or a
//!    null output pointer (pre-hardening the pointer deref was UB).
//! 2. A `count == 0` call must not consume generator state (shape-b wrappers
//!    used to call `rng.nextu()` for the reseed base even on zero counts;
//!    twin-stream check here).
//! 3. Full-span `*_rand_i32s(.., i32::MIN, i32::MAX)` must populate both signs
//!    with a spread of values. The SIMD-tier implementations previously
//!    broadcast the range into 32-bit lanes, so `range == 2^32` degenerated
//!    to all-`i32::MIN` (and the all-scalar kernels overflowed).
//! 4. Near-full spans (range in `(2^31, 2^32)`) must stay inside `[min, max]`
//!    — the all-scalar per-lane kernels previously evaluated
//!    `(hi_word as i32) + min` in checked `i32` arithmetic.
//! 5. Small ranges are inclusive on both ends (`[min, max]`).

#![cfg(feature = "cabi")]

use std::ptr::{null, null_mut};

use urng::cabi::*;

// ---------------------------------------------------------------- helpers

macro_rules! fill_i32s {
    ($ptr:expr, $rand:ident, $min:expr, $max:expr, $n:expr) => {{
        let mut buf = vec![0i32; $n];
        unsafe { $rand($ptr, buf.as_mut_ptr(), $n, $min, $max) };
        buf
    }};
}

/// Full-span sanity for one `*_rand_i32s` site: not all-`min`, both signs
/// widely populated, many distinct values.
macro_rules! fullspan_site {
    ($name:ident, $new:ident, $free:ident, $rand:ident) => {
        #[test]
        fn $name() {
            const N: usize = 65_536;
            let ptr = unsafe { $new(1) };
            let vals = fill_i32s!(ptr, $rand, i32::MIN, i32::MAX, N);
            unsafe { $free(ptr) };
            check_fullspan(&vals, stringify!($name));
        }
    };
}

fn check_fullspan(vals: &[i32], what: &str) {
    let negatives = vals.iter().filter(|&&v| v < 0).count();
    let positives = vals.len() - negatives;
    // The degenerate pre-fix behavior emitted exactly `i32::MIN` everywhere.
    assert!(negatives > 0, "{what}: no negative values at all");
    assert!(positives > 0, "{what}: no positive values at all");
    let f = negatives as f64 / vals.len() as f64;
    assert!(
        (0.2..=0.8).contains(&f),
        "{what}: sign split wildly off uniform ({f:.3})"
    );
    let mut sorted = vals.to_vec();
    sorted.sort_unstable();
    sorted.dedup();
    assert!(
        sorted.len() > vals.len() / 2,
        "{what}: too few distinct values ({}/{})",
        sorted.len(),
        vals.len()
    );
}

fn check_range(vals: &[i32], min: i32, max: i32, what: &str) {
    for (i, &v) in vals.iter().enumerate() {
        assert!(
            v >= min && v <= max,
            "{what}[{i}] = {v} outside [{min}, {max}]"
        );
    }
}

// -------------------------------------- 1. guards: null pointer / null out

#[test]
fn null_ptr_scalar_is_noop() {
    let mut out = [7u32; 16];
    unsafe {
        sfc32_next_u32s(null_mut(), out.as_mut_ptr(), 16);
        sfc32_next_f32s(null_mut(), out.as_mut_ptr() as *mut f32, 16);
        mt19937_next_f32s(null_mut(), out.as_mut_ptr() as *mut f32, 16);
        splitmix32_next_u32s(null_mut(), out.as_mut_ptr(), 16);
        xorshift64_next_u64s(null_mut(), out.as_mut_ptr() as *mut u64, 16);
    }
}

#[test]
fn null_out_scalar_is_noop() {
    let ptr = unsafe { sfc32_new(1) };
    unsafe {
        sfc32_next_u32s(ptr, null_mut(), 16);
        sfc32_next_f32s(ptr, null_mut(), 16);
        sfc32_rand_i32s(ptr, null_mut(), 16, -5, 7);
        sfc32_rand_i32s(ptr, null_mut(), 0, -5, 7);
    }
    unsafe { sfc32_free(ptr) };
}

#[cfg(feature = "simd")]
#[test]
fn null_ptr_or_out_simd_is_noop() {
    let mut out = [3u32; 32];
    let mut b64 = [5u64; 32];
    unsafe {
        sfc32x4_next_u32s(null_mut(), out.as_mut_ptr(), 32);
        sfc32x4_next_f32s(null_mut(), out.as_mut_ptr() as *mut f32, 32);
        sfc32x4_rand_i32s(null_mut(), out.as_mut_ptr() as *mut i32, 32, -5, 7);
        // null out with a valid pointer
        let ptr = sfc32x4_new(2);
        sfc32x4_rand_i32s(ptr, null_mut(), 32, -5, 7);
        sfc32x4_free(ptr);
        let ptr = sfc32x8_new(2);
        sfc32x8_rand_i32s(ptr, null_mut(), 32, -5, 7);
        sfc32x8_free(ptr);
        squares32x8_next_u32s(null_mut(), out.as_mut_ptr(), 32);
        philox32x4x4_rand_i32s(null_mut(), out.as_mut_ptr() as *mut i32, 32, -5, 7);
        xoroshiro64ssx8_rand_i32s(null_mut(), out.as_mut_ptr() as *mut i32, 32, -5, 7);
        pcg32x8_rand_i32s(null_mut(), out.as_mut_ptr() as *mut i32, 32, -5, 7);
        pcg32x8_rand_f32s(null_mut(), out.as_mut_ptr() as *mut f32, 32, -0.5, 0.5);
        sfc64x8_next_u64s(null_mut(), b64.as_mut_ptr(), 32);
        cet256x2_next_u64s(null_mut(), b64.as_mut_ptr(), 32);
    }
}

// --------------------------------------------- 2. zero count consumes nothing

macro_rules! zero_count_scalar {
    ($name:ident, $new:ident, $free:ident, $next:ident, $word:ty) => {
        #[test]
        fn $name() {
            let a = unsafe { $new(42) };
            let b = unsafe { $new(42) };
            let mut x = [0 as $word; 8];
            let mut y = [0 as $word; 8];
            unsafe { $next(a, x.as_mut_ptr(), 8) };
            unsafe { $next(b, y.as_mut_ptr(), 8) };
            assert_eq!(x, y, "initial streams differ");
            // zero-count call on the drained stream of A only
            unsafe { $next(a, x.as_mut_ptr() as *mut $word, 0) };
            unsafe { $next(a, x.as_mut_ptr(), 8) };
            unsafe { $next(b, y.as_mut_ptr(), 8) };
            assert_eq!(x, y, "count=0 call changed the state of A");
            unsafe { $free(a) };
            unsafe { $free(b) };
        }
    };
}

zero_count_scalar!(zc_sfc32, sfc32_new, sfc32_free, sfc32_next_u32s, u32);
zero_count_scalar!(zc_splitmix32, splitmix32_new, splitmix32_free, splitmix32_next_u32s, u32);
zero_count_scalar!(zc_xorshift64, xorshift64_new, xorshift64_free, xorshift64_next_u64s, u64);

#[test]
fn zero_count_rand_scalar() {
    // rand_i32s with count 0 must also be a state no-op (it draws a reseed
    // base first, so it used to burn one word).
    let a = unsafe { sfc32_new(9) };
    let b = unsafe { sfc32_new(9) };
    let mut x = [0u32; 8];
    let mut y = [0u32; 8];
    unsafe { sfc32_next_u32s(a, x.as_mut_ptr(), 8) };
    unsafe { sfc32_next_u32s(b, y.as_mut_ptr(), 8) };
    assert_eq!(x, y);
    unsafe { sfc32_rand_i32s(a, x.as_mut_ptr() as *mut i32, 0, -3, 3) };
    unsafe { sfc32_next_u32s(a, x.as_mut_ptr(), 8) };
    unsafe { sfc32_next_u32s(b, y.as_mut_ptr(), 8) };
    assert_eq!(x, y, "count=0 rand call burned state");
    unsafe { sfc32_free(a) };
    unsafe { sfc32_free(b) };
}

#[cfg(feature = "simd")]
#[test]
fn zero_count_simd() {
    // shape (a) family (pre-existing count guard, extended)
    let a = unsafe { sfc32x4_new(7) };
    let b = unsafe { sfc32x4_new(7) };
    let mut x = [0u32; 8];
    let mut y = [0u32; 8];
    unsafe { sfc32x4_next_u32s(a, x.as_mut_ptr(), 8) };
    unsafe { sfc32x4_next_u32s(b, y.as_mut_ptr(), 8) };
    assert_eq!(x, y);
    unsafe { sfc32x4_rand_i32s(a, x.as_mut_ptr() as *mut i32, 0, -1, 1) };
    unsafe { sfc32x4_next_u32s(a, x.as_mut_ptr(), 8) };
    unsafe { sfc32x4_next_u32s(b, y.as_mut_ptr(), 8) };
    assert_eq!(x, y, "count=0 rand call changed simd state");
    unsafe { sfc32x4_free(a) };
    unsafe { sfc32x4_free(b) };

    // shape (b) family (fresh guard): sfc64x8
    let a = unsafe { sfc64x8_new(7) };
    let mut w = [0u64; 8];
    let mut z = [0u64; 8];
    unsafe { sfc64x8_next_u64s(a, w.as_mut_ptr(), 8) };
    let b = unsafe { sfc64x8_new(7) };
    unsafe { sfc64x8_next_u64s(b, z.as_mut_ptr(), 8) };
    assert_eq!(w, z);
    unsafe { sfc64x8_next_u64s(a, w.as_mut_ptr(), 0) };
    unsafe { sfc64x8_next_u64s(a, w.as_mut_ptr(), 8) };
    unsafe { sfc64x8_next_u64s(b, z.as_mut_ptr(), 8) };
    assert_eq!(w, z, "count=0 call changed b64 simd state");
    unsafe { sfc64x8_free(a) };
    unsafe { sfc64x8_free(b) };
}

// ------------------------------------------------- 3. scalar-tier full span

// Scalar cabi fills route through either the Rng-level `randi` (issue A5,
// already fixed) or wrapn-based elementwise kernels whose arithmetic wraps.
fullspan_site!(fs_scalar_sfc32, sfc32_new, sfc32_free, sfc32_rand_i32s);
fullspan_site!(fs_scalar_pcg32, pcg32_new, pcg32_free, pcg32_rand_i32s);
fullspan_site!(fs_scalar_mt19937, mt19937_new, mt19937_free, mt19937_rand_i32s);
fullspan_site!(fs_scalar_philox32, philox32_new, philox32_free, philox32_rand_i32s);
fullspan_site!(fs_scalar_threefry32x4, threefry32x4_new, threefry32x4_free, threefry32x4_rand_i32s);

// The scalar Squares32 cabi fill has its own all-scalar per-lane kernel:
// pre-fix `(hi as i32) + min` overflowed for the upper half of the span
// (debug panic; release: values shifted by 2^32).
fullspan_site!(fs_scalar_squares32, squares32_new, squares32_free, squares32_rand_i32s);

// b64 kernels with their own all-scalar mapping (`((u128) * range) >> 64
// as i64 + min`, checked): splitmix64, philox64, threefish256, and the
// SIMD-tier biski64x8 chunk kernel. Same overflow family as the b32 sites.
macro_rules! fullspan_b64_site {
    ($name:ident, $new:ident, $free:ident, $rand:ident) => {
        #[test]
        fn $name() {
            const N: usize = 8192;
            let ptr = unsafe { $new(2) };
            let mut buf = vec![0u64; N];
            unsafe { $rand(ptr, buf.as_mut_ptr() as *mut i64, N, i64::MIN, i64::MAX) };
            unsafe { $free(ptr) };
            let negatives = buf.iter().filter(|&&v| (v as i64) < 0).count();
            let f = negatives as f64 / N as f64;
            assert!(
                negatives > N / 4 && negatives < N * 3 / 4,
                "{}: sign split off ({f:.3})",
                stringify!($name)
            );
            let mut sorted = buf.clone();
            sorted.sort_unstable();
            sorted.dedup();
            assert!(
                sorted.len() > N / 2,
                "{}: degenerate output",
                stringify!($name)
            );
        }
    };
}

fullspan_b64_site!(fs_b64_sfc64, sfc64_new, sfc64_free, sfc64_rand_i64s);
fullspan_b64_site!(fs_b64_mt1993764, mt1993764_new, mt1993764_free, mt1993764_rand_i64s);
fullspan_b64_site!(fs_b64_splitmix64, splitmix64_new, splitmix64_free, splitmix64_rand_i64s);
fullspan_b64_site!(fs_b64_philox64, philox64_new, philox64_free, philox64_rand_i64s);
fullspan_b64_site!(fs_b64_threefish256, threefish256_new, threefish256_free, threefish256_rand_i64s);
fullspan_b64_site!(fs_b64_xorshift64, xorshift64_new, xorshift64_free, xorshift64_rand_i64s);

/// Near-full span at 64-bit width (range == 2^64 - 1) exercises the same
/// checked-add overflow in the scalar splitmix64 kernel.
#[test]
fn near_full_span_b64_stays_in_range() {
    const MIN: i64 = i64::MIN + 1;
    const MAX: i64 = i64::MAX;
    let ptr = unsafe { splitmix64_new(3) };
    let mut buf = vec![0u64; 4095];
    unsafe { splitmix64_rand_i64s(ptr, buf.as_mut_ptr() as *mut i64, 4095, MIN, MAX) };
    unsafe { splitmix64_free(ptr) };
    for (i, &v) in buf.iter().enumerate() {
        let s = v as i64;
        assert!(s >= MIN && s <= MAX, "splitmix64 near-full[{i}] = {s}");
    }
}

// ------------------------------------------------------ 4. SIMD-tier sites

#[cfg(feature = "simd")]
mod simd_tier {
    use urng::cabi::*;

    use super::{check_fullspan, check_range};

    // One regression test per SIMD site whose range broadcast degenerated
    // when `max - min + 1 == 2^32`.
    fullspan_site!(fs_sfc32x4, sfc32x4_new, sfc32x4_free, sfc32x4_rand_i32s);
    fullspan_site!(fs_sfc32x8, sfc32x8_new, sfc32x8_free, sfc32x8_rand_i32s);
    fullspan_site!(fs_sfc32x16, sfc32x16_new, sfc32x16_free, sfc32x16_rand_i32s);
    fullspan_site!(fs_jsf32x16, jsf32x16_new, jsf32x16_free, jsf32x16_rand_i32s);
    fullspan_site!(fs_xoroshiro64ssx8, xoroshiro64ssx8_new, xoroshiro64ssx8_free, xoroshiro64ssx8_rand_i32s);
    fullspan_site!(fs_xoroshiro64ssx16, xoroshiro64ssx16_new, xoroshiro64ssx16_free, xoroshiro64ssx16_rand_i32s);
    fullspan_site!(fs_xoshiro128ppx16, xoshiro128ppx16_new, xoshiro128ppx16_free, xoshiro128ppx16_rand_i32s);
    fullspan_site!(fs_xoshiro128ssx16, xoshiro128ssx16_new, xoshiro128ssx16_free, xoshiro128ssx16_rand_i32s);
    fullspan_site!(fs_pcg32x8, pcg32x8_new, pcg32x8_free, pcg32x8_rand_i32s);
    fullspan_site!(fs_philox32x4x4, philox32x4x4_new, philox32x4x4_free, philox32x4x4_rand_i32s);
    fullspan_site!(fs_squares32x8, squares32x8_new, squares32x8_free, squares32x8_rand_i32s);

    /// Near-full span at 64-bit width through the SIMD biski64x8 chunk
    /// kernel (same checked-add overflow family).
    #[test]
    fn near_full_span_b64_simd() {
        const MIN: i64 = i64::MIN + 1;
        const MAX: i64 = i64::MAX;
        let ptr = unsafe { biski64x8_new(3) };
        let mut buf = vec![0u64; 4095]; // odd count reaches the remainder loop
        unsafe { biski64x8_rand_i64s(ptr, buf.as_mut_ptr() as *mut i64, 4095, MIN, MAX) };
        unsafe { biski64x8_free(ptr) };
        for (i, &v) in buf.iter().enumerate() {
            let s = v as i64;
            assert!(s >= MIN && s <= MAX, "biski64x8 near-full[{i}] = {s}");
        }
    }

    /// Near-full span (range == 2^32 - 1) through the all-scalar pcg32x8
    /// chunk kernels: pre-fix `(hi as i32) + min` overflowed (debug abort);
    /// release produced values shifted by 2^32.
    #[test]
    fn near_full_pcg32x8() {
        const MIN: i32 = i32::MIN + 1;
        const MAX: i32 = i32::MAX;
        let ptr = unsafe { pcg32x8_new(3) };
        let vals = fill_i32s!(ptr, pcg32x8_rand_i32s, MIN, MAX, 4095);
        unsafe { pcg32x8_free(ptr) };
        check_range(&vals, MIN, MAX, "pcg32x8 near-full");
    }

    /// Same family through the squares32x8 SIMD chunk kernel (its final
    /// partial chunk runs through the all-scalar remainder loop).
    #[test]
    fn near_full_squares32x8() {
        const MIN: i32 = i32::MIN + 1;
        const MAX: i32 = i32::MAX;
        let ptr = unsafe { squares32x8_new(3) };
        let vals = fill_i32s!(ptr, squares32x8_rand_i32s, MIN, MAX, 4095);
        unsafe { squares32x8_free(ptr) };
        check_range(&vals, MIN, MAX, "squares32x8 near-full");
    }

    /// SIMD-vector sites: the modular lane math already handles
    /// range <= 2^32 - 1 once the span is representable; document it.
    #[test]
    fn near_full_sfc32x8_and_philox() {
        const MIN: i32 = i32::MIN + 1;
        const MAX: i32 = i32::MAX;

        let ptr = unsafe { sfc32x8_new(3) };
        let vals = fill_i32s!(ptr, sfc32x8_rand_i32s, MIN, MAX, 4096);
        unsafe { sfc32x8_free(ptr) };
        check_range(&vals, MIN, MAX, "sfc32x8 near-full");

        let ptr = unsafe { philox32x4x4_new(3) };
        let vals = fill_i32s!(ptr, philox32x4x4_rand_i32s, MIN, MAX, 4096);
        unsafe { philox32x4x4_free(ptr) };
        check_range(&vals, MIN, MAX, "philox32x4x4 near-full");
    }

    /// After a full-span call the generator must still be usable; a
    /// subsequent bounded call returns in-range values only.
    #[test]
    fn bounded_call_after_fullspan() {
        let ptr = unsafe { sfc32x8_new(11) };
        let _ = fill_i32s!(ptr, sfc32x8_rand_i32s, i32::MIN, i32::MAX, 2048);
        let vals = fill_i32s!(ptr, sfc32x8_rand_i32s, -9, 11, 2048);
        unsafe { sfc32x8_free(ptr) };
        check_range(&vals, -9, 11, "post-fullspan bounded");
    }

    /// Small-range sanity through a SIMD site: bounds are inclusive and a
    /// one-value range degenerates to a constant.
    #[test]
    fn small_range_inclusive_simd() {
        let ptr = unsafe { sfc32x8_new(5) };
        let vals = fill_i32s!(ptr, sfc32x8_rand_i32s, -5, 7, 20_000);
        unsafe { sfc32x8_free(ptr) };
        check_range(&vals, -5, 7, "sfc32x8 [-5,7]");
        assert!(vals.contains(&-5), "lower bound never sampled");
        assert!(vals.contains(&7), "upper bound never sampled");

        let ptr = unsafe { sfc32x4_new(6) };
        let vals = fill_i32s!(ptr, sfc32x4_rand_i32s, 5, 5, 256);
        unsafe { sfc32x4_free(ptr) };
        assert!(vals.iter().all(|&v| v == 5));
    }

    /// b64 rand_i64s: assert the Phase-5 inline-mapping contract at the C ABI
    /// level too (scalar Sfc64 i64 span + the SIMD Biski64x8 lane mapping).
    #[test]
    fn b64_full_span() {
        fn check(vals: &[u64]) {
            let negatives = vals.iter().filter(|&&v| (v as i64) < 0).count();
            assert!(
                negatives > vals.len() / 4 && negatives < vals.len() * 3 / 4,
                "b64 full span: sign split off ({negatives}/{})",
                vals.len()
            );
            let mut sorted = vals.to_vec();
            sorted.sort_unstable();
            sorted.dedup();
            assert!(
                sorted.len() > vals.len() / 2,
                "b64 full span: degenerate output"
            );
        }
        let ptr = unsafe { sfc64_new(4) };
        let mut buf = vec![0u64; 8192];
        unsafe { sfc64_rand_i64s(ptr, buf.as_mut_ptr() as *mut i64, 8192, i64::MIN, i64::MAX) };
        unsafe { sfc64_free(ptr) };
        check(&buf);

        let ptr = unsafe { biski64x8_new(4) };
        let mut buf = vec![0u64; 8192];
        unsafe { biski64x8_rand_i64s(ptr, buf.as_mut_ptr() as *mut i64, 8192, i64::MIN, i64::MAX) };
        unsafe { biski64x8_free(ptr) };
        check(&buf);
    }
}
