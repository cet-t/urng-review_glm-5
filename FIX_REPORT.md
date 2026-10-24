# urng 1.0.0 — Defect Fix Report (IN PROGRESS)

- **Baseline commit:** `eda342e "f c"` (main, clean tree, REVIEW.md committed)
- **Start date:** 2026-10-25
- **Status:** IN PROGRESS — see [Progress log](#progress-log) and [Resume state](#resume-state)
- **Source review:** REVIEW.md (issues A1–A7); A8 found during verification probing.

**Priority rule:** correctness and memory safety > performance. Performance-affecting
changes are documented with rationale; correctness is never traded for speed.
**No dead-issue rule:** anything left unfixed is reported in [Unfixed issues](#unfixed-issues),
never as fixed. No test is deleted or disabled to hide a defect.

---

## Issue list and fix status

| ID | Severity | Issue | Status |
|----|----------|-------|--------|
| A1 | Critical | `Xoshiro256Ssx2` writes past struct storage (32 B state in a 64-byte area? — OOB/zombie state), shuffle logic broken; release builds survive only via UB elision | ☐ planned (Phase 1) |
| A2 | High | "AVX2 tier" code uses AVX512VL intrinsics: `_mm256_rol_epi32` (jsf.rs:108–109, sfc.rs:210, xoroshiro.rs:119/122), `_mm256_mask_blend_epi32` (rngv.rs:77, sfc.rs:227) → SIGILL on AVX2-only CPUs | ☐ planned (Phase 2) |
| A3 | High | `Shuffle::shuffle/shuffled` biased Fisher–Yates: draw range never shrinks (baseline: len=3 P(identity)=0.2211 vs 1/6) | ☐ planned (Phase 3) |
| A4 | Medium | `lib.rs` `include_str!("../Readme.md")` case mismatch vs actual `README.md` → Linux build break | ☐ planned (Phase 4) |
| A5 | High | Scalar `Word::to_randi/to_index` overflow (`randi(i32::MIN,i32::MAX)` debug panic/garbage; `to_index(len>2^32)` u64 wrap); plus missing `wrapping_add(min)` in b64 SIMD inline randi (Sfc64x8 sfc.rs, Cet256x2 cet.rs) | ☐ planned (Phase 5) |
| A6 | Medium | `rand` bridge `impl_rand_trait!` uses BE `u32::from_be_bytes`-style carry for u64 words (`Seed=[u8;4]` truncates); b64 rand impls missing/incorrect | ☐ planned (Phase 5) |
| A7 | Medium | misc: `choice(empty)` panic type, sampler validation (Alias all-zero weights returns nonsense silently), SeedGen retry loop, doc mismatches (Sfc64x8 doc says 4 lanes, README gaps) | ☐ planned (Phase 6) |
| A8 | High | `WordV::to_randi` for `__m128i` (rngv.rs:36–52): odd lanes take **low** word of product and pair odd u×even scale → garbage for non-broadcast scale. `__m256i/__m512i`: odd lanes pair u_i×scale_{i-1} (low word OK, pairing wrong). Reference implementation already in-repo: `Sfc32x4::randiv` (sfc.rs:124–135). Verified: with broadcast scale (current C ABI usage) `__m256i/x16` high-word paths are correct; `__m128i` even with broadcast scale takes low word for odd lanes — **needs re-probe to confirm magnitude of impact** | ☐ planned (Phase 2) |
| — | Not a defect | PCG `inc|1` stream-seed (kept; LCG bijective for any odd inc; output-compat preserved → documented, not changed) | will not fix (rationale in report) |
| — | Not a defect | MT19937 custom SplitMix-style seeding (design choice) | will not fix |
| — | Note | cabi bulk fill (par_fill_reseed) ≠ scalar sequential stream (by design) | document in README |

C ABI probe results (Phase 0):
- `sfc32x4_rand_i32s` / `sfc32x8_rand_i32s` inclusive bounds / boundary values valid: OK
  (probe initially flagged 4/4000 and 8/8000 "violations" = true-max boundary samples only).
- Xoshiro256Ssx2 baseline release build survives UB (optimizer elides the store); debug = access violation `0xc0000005`.
- `choice(empty)` panics with "index out of bounds" (library panic, not a clear contract message).
- `randi(5,1)` debug panic / release garbage; `to_index(len>u32::MAX)` debug overflow panic; the panics are debug-only overflow checks.

## Fix plan (one commit per phase)

- [x] Phase 0: baseline probes + empirical verification (shuffle bias, ssx2 UB, cabi ranges)
- [ ] Phase B: bench harness (`benches/urng-core.rs`, criterion) + baseline numbers
- [ ] Phase 1: A1 Xoshiro256Ssx2 rewrite (state `[u64;8]`, safe `nextu() -> [u64;2]`,
      runtime AVX512F dispatch, splitmix64 sequential seeding, lane-exact vs scalar + cross-check)
- [ ] Phase 2: A2 intrinsics replacement (rol → slli/srli OR; mask_blend_epi32 → blend_epi32;
      AVX2-tier only) + A8 to_randi fixes (m128i low-word bug + odd-lane scale pairing everywhere)
      + per-lane scale pairing, SIMD-vs-scalar lane tests for Jsf32x8/Sfc32x8/Xoroshiro64Ssx8/…
- [ ] Phase 3: A3 shuffle fix `j = i + nextu().to_index(len-i)` in shuffle+shuffled + statistical tests
- [ ] Phase 4: A4 README include path case fix (+ doc build check)
- [ ] Phase 5: A5 scalar overflow fixes (wrapping_add(min), assert min<=max, to_index u128 for len≥2^32,
      SIMD b64 inline randi wrap fixes) + A6 rand-bridge from_le_bytes + `Seed=[u8;8]` b64 impls
- [ ] Phase 6: A7 misc (choice contract, sampler validation, SeedGen, docs/README updates)
- [ ] Phase 7: C ABI guards (null pointer early-returns, boundary conditions, stream-dup checks)
- [ ] Phase 8: full test matrix (`cargo test --locked` default / all-features), feature builds,
      post-fix benches, finalize report, version bump 1.0.0 → 1.0.1

## Progress log

- 2026-10-25 Phase 0: baseline commit `eda342e`; probes confirm A3 bias (len=3: P(identity)=0.2211
  pre-fix, expected 1/6=0.1667), A1 UB (release survives via elision, debug SIGSEGV), cabi
 inclusive-range semantics OK for broadcast scale. A8 confirmed by code reading + Sfc32x4 is the
  correct reference (fixed inline impl in-repo).
- 2026-10-25 Phase B: benches/urng-core.rs added; baseline run (numbers below in "Baseline benchmarks").

## Baseline benchmarks (to be filled after run)

(placeholder — criterion output will be pasted here after baseline and post-fix runs)

## Resume state (ephemeral checkpoint — update after every step)

- Last completed: Phase A (reading/planning). Next: write bench file, run `cargo bench`, commit.
- Scratch probe crate: `C:\Users\cets0\AppData\Local\Temp\opencode\urng-fix-worker` (packages probe_wordv128/probe_ssx2/probe_shuffle; path-dep on this repo as `urng-fixed`).
- CPU here: AVX-512VL+ capable (so AVX512 intrinsics do NOT crash here; risk is AVX2-only machines).
- Fix design decisions summary:
  - A1 rewrite: struct fields `state: [u64;8]` (0-3 = xoshiro256** A, 4-7 = B, seeded sequentially
    with one SplitMix64); `nextu() -> [u64;2]` safe; fast path per-lane permute + mask-blend +
    shift-add mullo; runtime `is_x86_feature_detected!("avx512f")` dispatch; scalar fallback.
  - A2 replacements are semantically identical selects/rotates, just AVX2-tier intrinsics.
  - A5: `randi` becomes `wrapping_add(min)` + `assert!(min <= max)` (documented panic contract),
    `to_index` goes through u128 for `len >= 2^32` on the u32 path.
  - A6: rand bridge `from_le_bytes` (LE) for 32-bit words; b64 gets `impl_rand_trait64!` with
    `Seed = [u8; 8]` (breaking change, documented).

## Unfixed issues

(final list; empty for now)

## Compatibility impact

(final list; empty for now)

## Performance changes

(final list; see baseline benchmark section)

## Remaining risks

(final list)
