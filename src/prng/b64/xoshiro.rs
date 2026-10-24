use wrapn::{wrap, wu64};

use crate::prng::b64::SplitMix64;
use crate::rng::Rng;

/// Xoshiro256++ 64-bit RNG implementation.
///
/// # Example
/// ```
/// use urng::{Rng, Xoshiro256Pp};
///
/// let mut rng = Xoshiro256Pp::new(1);
/// let _ = rng.nextu();
/// ```
#[repr(C, align(64))]
#[derive(Debug, Clone, Copy)]
pub struct Xoshiro256Pp {
    s: [wu64; 4],
}

impl Xoshiro256Pp {
    /// Creates a new `Xoshiro256Pp` instance with the given seed.
    pub const fn new(seed: u64) -> Self {
        let mut seedgen = SplitMix64::new(seed);
        Self {
            s: wrap![
                seedgen.nextu_const(),
                seedgen.nextu_const(),
                seedgen.nextu_const(),
                seedgen.nextu_const(),
            ],
        }
    }
}

impl Rng for Xoshiro256Pp {
    type Word = u64;

    #[inline]
    fn nextu(&mut self) -> Self::Word {
        let s = &mut self.s;
        let res = s[0] + (s[0] + s[3]).rotate_left(23);
        let t = s[1] << 17;

        s[2] ^= s[0];
        s[3] ^= s[1];
        s[1] ^= s[2];
        s[0] ^= s[3];
        s[2] ^= t;
        s[3] = s[3].rotate_left(45);

        *res
    }
}

/// Xoshiro256** 64-bit RNG implementation.
///
/// # Example
/// ```
/// use urng::{Rng, Xoshiro256Ss};
///
/// let mut rng = Xoshiro256Ss::new(1);
/// let _ = rng.nextu();
/// ```
#[repr(C, align(64))]
#[derive(Debug, Clone, Copy)]
pub struct Xoshiro256Ss {
    s: [wu64; 4],
}

impl Xoshiro256Ss {
    /// Creates a new `Xoshiro256Ss` instance with the given seed.
    pub const fn new(seed: u64) -> Self {
        let mut seedgen = SplitMix64::new(seed);
        Self {
            s: wrap![
                seedgen.nextu_const(),
                seedgen.nextu_const(),
                seedgen.nextu_const(),
                seedgen.nextu_const(),
            ],
        }
    }
}

impl Rng for Xoshiro256Ss {
    type Word = u64;

    #[inline]
    fn nextu(&mut self) -> Self::Word {
        let s = &mut self.s;
        let res = (s[1] * 5).rotate_left(7) * 9;
        let t = s[1] << 17;

        s[2] ^= s[0];
        s[3] ^= s[1];
        s[1] ^= s[2];
        s[0] ^= s[3];
        s[2] ^= t;
        s[3] = s[3].rotate_left(45);

        *res
    }
}

#[cfg(feature = "simd")]
pub use simd::*;

#[cfg(feature = "simd")]
pub mod simd {
    #[cfg(target_arch = "x86_64")]
    use std::arch::x86_64::__m512i;

    use crate::prng::b64::SplitMix64;

    /// AVX-512-accelerated view over two independent `xoshiro256**`
    /// generators sharing one 64-byte state block.
    ///
    /// State layout (8 `u64` lanes): `[a0, a1, a2, a3, b0, b1, b2, b3]`,
    /// where `a0..a3` is generator A and `b0..b3` is generator B. The
    /// generators are seeded sequentially with eight consecutive
    /// [`SplitMix64`] draws of `seed` (draws 0–3 → A, 4–7 → B), so generator
    /// A therefore runs the same stream as `Xoshiro256Ss::new(seed)`.
    ///
    /// [`nextu`](Xoshiro256Ssx2::nextu) advances both generators and returns
    /// `[output_A, output_B]`. On an `avx512f`-capable CPU the two halves run
    /// as lanes of one 512-bit register (the vectorized core uses only
    /// `avx512f` instructions); otherwise an identical scalar core is used.
    /// The dispatch happens at runtime, so the type is safe on any x86-64 CPU.
    #[cfg(target_arch = "x86_64")]
    #[repr(C, align(64))]
    pub struct Xoshiro256Ssx2 {
        pub(crate) s: [u64; 8],
    }

    /// One step of the xoshiro256** core over a 4-word state (`st[0]..st[3]`
    /// = `s0..s3`); returns that step's output. Used by the scalar fallback
    /// and mirrored lane-for-lane by the AVX-512F core.
    #[cfg(target_arch = "x86_64")]
    fn ss_step(st: &mut [u64]) -> u64 {
        debug_assert_eq!(st.len(), 4);
        let (mut s0, mut s1, mut s2, mut s3) = (st[0], st[1], st[2], st[3]);

        let res = s1.wrapping_mul(5).rotate_left(7).wrapping_mul(9);
        let t = s1 << 17;

        s2 ^= s0;
        s3 ^= s1;
        s1 ^= s2;
        s0 ^= s3;
        s2 ^= t;
        s3 = s3.rotate_left(45);

        (st[0], st[1], st[2], st[3]) = (s0, s1, s2, s3);
        res
    }

    #[cfg(target_arch = "x86_64")]
    impl Xoshiro256Ssx2 {
        /// Creates a new instance seeded with eight sequential [`SplitMix64`]
        /// draws of `seed` (generator A gets draws 0–3, B gets 4–7).
        pub fn new(seed: u64) -> Self {
            let mut seedgen = SplitMix64::new(seed);
            let s = [0u64; 8].map(|_| seedgen.nextu_const());
            Self { s }
        }

        /// Generates the next output pair `[output_A, output_B]`, advancing
        /// both generators. Between calls it dispatches to the AVX-512F core
        /// when the CPU supports it and to the identical scalar core
        /// otherwise.
        ///
        /// # Example
        /// ```
        /// use urng::Xoshiro256Ssx2;
        ///
        /// let mut rng = Xoshiro256Ssx2::new(1);
        /// let [a, b] = rng.nextu();
        /// let _ = (a, b);
        /// ```
        #[inline]
        pub fn nextu(&mut self) -> [u64; 2] {
            if std::arch::is_x86_feature_detected!("avx512f") {
                // SAFETY: `nextu_avx512f` needs exactly the `avx512f` target
                // feature, which was just confirmed at runtime.
                unsafe { self.nextu_avx512f() }
            } else {
                self.nextu_scalar()
            }
        }

        /// The scalar core (also the fallback half of
        /// [`nextu`](Xoshiro256Ssx2::nextu)). Exposed to the crate so the
        /// vectorized path can be compared lane-exactly against it.
        #[inline]
        pub(crate) fn nextu_scalar(&mut self) -> [u64; 2] {
            let s = &mut self.s;
            let (a, b) = s.split_at_mut(4);
            [ss_step(a), ss_step(b)]
        }

        /// The AVX-512F vectorized core. The 8 lanes of one register hold
        /// generator A (lanes 0-3) and B (lanes 4-7) simultaneously; every
        /// step of `ss_step` is mirrored with per-lane permutes + k-masked
        /// 64-bit ops:
        /// - `t` and both outputs are computed *first*, from the pre-update
        ///   `s1`, exactly like the scalar core's `res`/`t`,
        /// - `s2 ^= s0`, `s3 ^= s1`, `s1 ^= s2`, `s0 ^= s3`, `s2 ^= t` use
        ///   only lanes updated before them,
        /// - `s3 = rotl45(s3)` is written back through a mask so the other
        ///   lanes keep their values,
        /// - the two `**` outputs (lanes 1 and 5) are gathered to lanes 0-1.
        #[target_feature(enable = "avx512f")]
        unsafe fn nextu_avx512f(&mut self) -> [u64; 2] {
            use std::arch::x86_64::{
                _mm512_add_epi64, _mm512_loadu_si512, _mm512_mask_add_epi64,
                _mm512_mask_mov_epi64, _mm512_mask_xor_epi64, _mm512_permutexvar_epi64,
                _mm512_rol_epi64, _mm512_slli_epi64, _mm512_storeu_si512,
            };

            /// Builds a lane-index vector for `_mm512_permutexvar_epi64`
            /// from a source-lane array (`i`-th lane of the index vector
            /// selects the `idx[i]`-th lane of the source register).
            #[inline(always)]
            fn idx(l: [i64; 8]) -> __m512i {
                unsafe { _mm512_loadu_si512(l.as_ptr() as *const _) }
            }

            // k-mask bit i selects lane i.
            const K_L2L6: u8 = 0b0100_0100; // A's s2 (lane 2) and B's s2 (lane 6)
            const K_L3L7: u8 = 0b1000_1000; // A's s3, B's s3
            const K_L1L5: u8 = 0b0010_0010; // A's s1, B's s1
            const K_L0L4: u8 = 0b0001_0001; // A's s0, B's s0

            let mut s = unsafe { _mm512_loadu_si512(self.s.as_ptr() as *const _) };

            // s1 of A (lane 1) and B (lane 5), pre-update. It drives both the
            // output formula and `t`, exactly like the scalar core.
            let v1 = _mm512_permutexvar_epi64(idx([1, 1, 1, 1, 5, 5, 5, 5]), s);

            // res = (s1 * 5).rotate_left(7) * 9 on lanes 1 and 5, taken from
            // the pre-update s1; the multiply is realized shift-add so only
            // avx512f is required.
            let m5 = _mm512_mask_add_epi64(v1, K_L1L5, v1, _mm512_slli_epi64(v1, 2));
            let r7 = _mm512_rol_epi64(m5, 7);
            let r9 = _mm512_add_epi64(r7, _mm512_slli_epi64(r7, 3));

            // t = s1 << 17 (also pre-update).
            let t = _mm512_slli_epi64(v1, 17);

            // s2 ^= s0 (masked writes keep all other lanes intact).
            s = _mm512_mask_xor_epi64(
                s,
                K_L2L6,
                s,
                _mm512_permutexvar_epi64(idx([0, 0, 0, 0, 4, 4, 4, 4]), s),
            );
            // s3 ^= s1
            s = _mm512_mask_xor_epi64(
                s,
                K_L3L7,
                s,
                _mm512_permutexvar_epi64(idx([1, 1, 1, 1, 5, 5, 5, 5]), s),
            );
            // s1 ^= s2 (updated s2)
            s = _mm512_mask_xor_epi64(
                s,
                K_L1L5,
                s,
                _mm512_permutexvar_epi64(idx([2, 2, 2, 2, 6, 6, 6, 6]), s),
            );
            // s0 ^= s3 (updated s3)
            s = _mm512_mask_xor_epi64(
                s,
                K_L0L4,
                s,
                _mm512_permutexvar_epi64(idx([3, 3, 3, 3, 7, 7, 7, 7]), s),
            );
            // s2 ^= t
            s = _mm512_mask_xor_epi64(s, K_L2L6, s, t);
            // s3 = rotl(s3, 45)
            s = _mm512_mask_mov_epi64(s, K_L3L7, _mm512_rol_epi64(s, 45));

            // Gather A's output (lane 1) and B's output (lane 5) to lanes 0-1.
            let gathered = _mm512_permutexvar_epi64(idx([1, 5, 1, 0, 0, 0, 0, 0]), r9);
            let mut out = [0u64; 8];
            unsafe { _mm512_storeu_si512(out.as_mut_ptr() as *mut _, gathered) };
            unsafe { _mm512_storeu_si512(self.s.as_mut_ptr() as *mut _, s) };

            [out[0], out[1]]
        }
    }

    #[cfg(all(test, feature = "simd", target_arch = "x86_64"))]
    mod tests {
        use super::Xoshiro256Ssx2;
        use crate::prng::b64::SplitMix64;

        /// Deterministic reference states for both generators, rebuilt from
        /// the seeding contract (A = draws 0-3, B = draws 4-7 of SplitMix64).
        fn reference_states(seed: u64) -> ([u64; 4], [u64; 4]) {
            let mut seedgen = SplitMix64::new(seed);
            let a = [
                seedgen.nextu_const(),
                seedgen.nextu_const(),
                seedgen.nextu_const(),
                seedgen.nextu_const(),
            ];
            let b = [
                seedgen.nextu_const(),
                seedgen.nextu_const(),
                seedgen.nextu_const(),
                seedgen.nextu_const(),
            ];
            (a, b)
        }

        /// The official xoshiro256** step (Blackman & Vigna), transcribed
        /// independently of the crate's `Xoshiro256Ss`/`ss_step` here so a
        /// shared transcription slip cannot hide.
        fn official_step(st: &mut [u64; 4]) -> u64 {
            let res = st[1].wrapping_mul(5).rotate_left(7).wrapping_mul(9);
            let t = st[1] << 17;

            st[2] ^= st[0];
            st[3] ^= st[1];
            st[1] ^= st[2];
            st[0] ^= st[3];
            st[2] ^= t;
            st[3] = st[3].rotate_left(45);

            res
        }

        /// Advances both generators with the official scalar reference and
        /// compares outputs **and** the full internal state, lane-exact.
        fn run_pair(seed: u64, steps: usize) {
            let (mut a, mut b) = reference_states(seed);
            let mut rng = Xoshiro256Ssx2::new(seed);
            for _ in 0..steps {
                let want_a = official_step(&mut a);
                let want_b = official_step(&mut b);

                let got = rng.nextu();
                assert_eq!(got, [want_a, want_b], "seed {seed:#x}: outputs");

                // Lane-exact internal state: catches cross-contaminating
                // lane shuffles and seeding slippage alike.
                assert_eq!(rng.s[..4], a, "seed {seed:#x}: generator A state");
                assert_eq!(
                    rng.s[4..8],
                    b,
                    "seed {seed:#x}: generator B state"
                );
            }
        }

        /// The dispatched `nextu` (AVX-512F core on this CPU, scalar core
        /// elsewhere) must track the official scalar reference exactly.
        #[test]
        fn nextu_matches_official_scalar_reference() {
            for seed in [1u64, 2, 13, 0xdead_beef_cafe_f00d, u64::MAX] {
                run_pair(seed, 512);
            }
        }

        /// Fast path and slow path must be indistinguishable, lane-exact.
        #[test]
        fn vector_core_matches_scalar_core() {
            if !std::arch::is_x86_feature_detected!("avx512f") {
                eprintln!("skipping: avx512f not available, `nextu` already ran the scalar core");
                return;
            }
            for seed in [1u64, 2, 13, 0xdead_beef_cafe_f00d, u64::MAX] {
                let mut vector = Xoshiro256Ssx2::new(seed);
                let mut scalar = Xoshiro256Ssx2::new(seed);
                for step in 0..256 {
                    let got_v = vector.nextu();
                    let got_s = scalar.nextu_scalar();
                    assert_eq!(
                        got_v, got_s,
                        "seed {seed:#x} step {step}: fast vs scalar cores"
                    );
                }
            }
        }

        /// Generator A must reproduce the crate's scalar `Xoshiro256Ss`
        /// stream (the documented seeding contract).
        #[test]
        fn generator_a_matches_crate_scalar_xoshiro256ss() {
            use crate::rng::Rng;
            for seed in [1u64, 42, 0x9e37_79b9] {
                let mut rng = Xoshiro256Ssx2::new(seed);
                let mut scalar = crate::prng::b64::Xoshiro256Ss::new(seed);
                for _ in 0..128 {
                    let [out_a, _] = rng.nextu();
                    assert_eq!(out_a, scalar.nextu(), "seed {seed:#x}");
                }
            }
        }

        /// Two instances with the same seed produce identical streams.
        #[test]
        fn nextu_is_deterministic_per_seed() {
            for seed in [7u64, 0x1234_5678_9abc_def0] {
                let mut r1 = Xoshiro256Ssx2::new(seed);
                let mut r2 = Xoshiro256Ssx2::new(seed);
                for _ in 0..128 {
                    assert_eq!(r1.nextu(), r2.nextu());
                }
            }
        }

        /// External cross-check against the reference crate
        /// (`rand_xoshiro::Xoshiro256StarStar`).
        ///
        /// Generator A is compared against a `rand_xoshiro` seeded with the
        /// same u64 (`SeedableRng::seed_from_u64`, which is a plain
        /// SplitMix64), and generator B against one seeded with the
        /// reference B state. Only odd seeds match exactly, because urng's
        /// `SplitMix64` gates its state with `seed | 1` (documented,
        /// deliberately-not-changed deviation).
        #[test]
        fn cross_check_against_rand_xoshiro() {
            use rand_core::{Rng, SeedableRng};
            use rand_xoshiro::Xoshiro256StarStar;

            for seed in [1u64, 21, 9_000_000_007] {
                assert_eq!(seed % 2, 1, "test contract: odd seeds only");

                // A: same seed → same stream entirely.
                let mut external = Xoshiro256StarStar::seed_from_u64(seed);
                let mut rng = Xoshiro256Ssx2::new(seed);
                for _ in 0..256 {
                    let [out_a, _out_b] = rng.nextu();
                    assert_eq!(out_a, external.next_u64(), "seed {seed:#x} (A)");
                }

                // B: seed the external reference with the reference B state
                // (little-endian u64 words, as `rand_xoshiro::from_seed`
                // interprets its 32-byte seed).
                let (_, b) = reference_states(seed);
                let mut bytes = [0u8; 32];
                for (i, w) in b.iter().enumerate() {
                    bytes[i * 8..i * 8 + 8].copy_from_slice(&w.to_le_bytes());
                }
                let mut external_b = Xoshiro256StarStar::from_seed(bytes);
                let mut rng = Xoshiro256Ssx2::new(seed);
                for _ in 0..256 {
                    let [_out_a, out_b] = rng.nextu();
                    assert_eq!(out_b, external_b.next_u64(), "seed {seed:#x} (B)");
                }
            }
        }

        /// The scalar fallback core must track the official reference too
        /// (guards against the old cross-lane contamination pattern),
        /// independently of the dispatched `nextu`.
        #[test]
        fn scalar_core_matches_official_reference() {
            for seed in [1u64, 0x1234_5678, u64::MAX] {
                let (mut a, mut b) = reference_states(seed);
                let mut rng = Xoshiro256Ssx2::new(seed);
                for _ in 0..64 {
                    let want = [official_step(&mut a), official_step(&mut b)];
                    let got = rng.nextu_scalar();
                    assert_eq!(got, want, "seed {seed:#x}");
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    crate::safe_test! {
        Xoshiro256Pp,
        Xoshiro256Ss
    }
}
