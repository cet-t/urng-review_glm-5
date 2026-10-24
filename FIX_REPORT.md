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
| A1 | Critical | `Xoshiro256Ssx2` writes past struct storage (32 B state in a 64-byte area? — OOB/zombie state), shuffle logic broken; release builds survive only via UB elision | ☑ fixed (Phase 1, `51103b9`) |
| A2 | High | "AVX2 tier" code uses AVX512VL intrinsics: `_mm256_rol_epi32` (jsf.rs:108–109, sfc.rs:210, xoroshiro.rs:119/122), `_mm256_mask_blend_epi32` (rngv.rs:77, sfc.rs:227) → SIGILL on AVX2-only CPUs | ☑ fixed (Phase 2, `116eb79`) |
| A3 | High | `Shuffle::shuffle/shuffled` biased Fisher–Yates: draw range never shrinks (baseline: len=3 P(identity)=0.2211 vs 1/6) | ☑ fixed (Phase 3, `ed464ba`) |
| A4 | Medium | `lib.rs` `include_str!("../Readme.md")` case mismatch vs actual `README.md` → Linux build break | ☑ fixed (Phase 4, `fc6fba2`) |
| A5 | High | Scalar `Word::to_randi/to_index` overflow (`randi(i32::MIN,i32::MAX)` debug panic/garbage; `to_index(len>2^32)` u64 wrap); plus missing `wrapping_add(min)` in b64 SIMD inline randi (Sfc64x8 sfc.rs, Cet256x2 cet.rs) | ☑ fixed (Phase 5, `1be3cf0`) |
| A6 | Medium | `rand` bridge `impl_rand_trait!` uses BE `u32::from_be_bytes`-style carry for u64 words (`Seed=[u8;4]` truncates); b64 rand impls missing/incorrect | ☑ fixed (Phase 5, `1be3cf0`) |
| A7 | Medium | misc: `choice(empty)` panic type, sampler validation (Alias all-zero weights returns nonsense silently), SeedGen retry loop, doc mismatches (Sfc64x8 doc says 4 lanes, README gaps) | ☑ fixed (Phase 6, `ef60118`) |
| A8 | High | `WordV::to_randi` for `__m128i` (rngv.rs:36–52): odd lanes take **low** word of product and pair odd u×even scale → garbage for non-broadcast scale. `__m256i/__m512i`: odd lanes pair u_i×scale_{i-1} (low word OK, pairing wrong). Reference implementation already in-repo: `Sfc32x4::randiv` (sfc.rs:124–135). Verified: with broadcast scale (current C ABI usage) `__m256i/x16` high-word paths are correct; `__m128i` even with broadcast scale takes low word for odd lanes — **needs re-probe to confirm magnitude of impact** | ☑ fixed (Phase 2, `116eb79`) — note: broadcast-scale precondition is now moot, the mapping is genuinely per-lane correct and tested per-lane |
| A9 | High (new) | simd-feature vector SIMD variants of `Xoroshiro64Ss` in `xoroshiro.rs`: SIMD variants return `s0*0x9E3779BB` directly without the scalar path's `rotl(5) * 5` final step, causing SIMD and scalar streams to diverge from the first output | ☑ fixed (Phase 2, `116eb79`; found during Phase 2 tests) |
| A10 | Critical (new) | cabi `*_rand_i32s/_rand_i64s` range mapping at width/overflow boundaries: (a) 10 SIMD b32 sites broadcast `range` into 32-bit lanes, so `range == 2^32` becomes 0 and the full span (`i32::MIN..=i32::MAX`) degenerates to all-`i32::MIN` (sfc32x4/x8/x16, jsf32x16, xoroshiro64ssx8/x16, xoshiro128ppx16/ssx16, philox32x4x4, squares32x8); (b) scalar chunk kernels map via `((x*range)>>N) as iN + min` in checked arithmetic → value+min overflows for ranges > 2^31/2^63 (debug: panic → **process abort** across `extern "C"`; release: values shifted by 2^N): pcg32x8 (×2), squares32 scalar tail (×5), squares32x8 scalar remainder (×2), biski64x8 `apply_range`, philox64, threefish256, splitmix64 | ☑ fixed (Phase 7, this commit) |
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
- [x] Phase B: bench harness (`benches/urng-core.rs`, criterion) + baseline numbers
- [x] Phase 1: A1 Xoshiro256Ssx2 rewrite (state `[u64;8]`, safe `nextu() -> [u64;2]`,
      runtime AVX512F dispatch, splitmix64 sequential seeding, lane-exact vs scalar + cross-check)
- [x] Phase 2: A2 intrinsics replacement (rol → slli/srli OR; mask_blend_epi32 → blend_epi32;
      AVX2-tier only) + A8 to_randi fixes (m128i low-word bug + odd-lane scale pairing everywhere)
      + per-lane scale pairing, SIMD-vs-scalar lane tests for Jsf32x8/Sfc32x8/Xoroshiro64Ssx8/…
- [x] Phase 3: A3 shuffle fix `j = i + nextu().to_index(len-i)` in shuffle+shuffled + statistical tests
- [x] Phase 4: A4 README include path case fix (+ doc build check)
- [x] Phase 5: A5 scalar overflow fixes (wrapping_add(min), assert min<=max, to_index u128 for len≥2^32,
      SIMD b64 inline randi wrap fixes) + A6 rand-bridge from_le_bytes + `Seed=[u8;8]` b64 impls
- [x] Phase 6: A7 misc (choice contract, sampler validation, SeedGen, docs/README updates)
- [x] Phase 7: C ABI hardening — 176 bulk entry points guarded (`count == 0 || ptr.is_null() ||
      out.is_null()` → no-op, no state consumption) + A10 range-mapping fixes (10 full-span
      branches + wrapping-add mapping at 8 scalar/remainder sites) + `tests/cabi_guards.rs` (39 tests)
- [ ] Phase 8: full test matrix (`cargo test --locked` default / all-features), feature builds,
      post-fix benches, finalize report, version bump 1.0.0 → 1.0.1

## Progress log

- 2026-10-25 Phase 0: baseline commit `eda342e`; probes confirm A3 bias (len=3: P(identity)=0.2211
  pre-fix, expected 1/6=0.1667), A1 UB (release survives via elision, debug SIGSEGV), cabi
  inclusive-range semantics OK for broadcast scale. A8 confirmed by code reading + Sfc32x4 is the
  correct reference (fixed inline impl in-repo).
- 2026-10-25 Phase B: benches/urng-core.rs added; baseline run (numbers below in "Baseline benchmarks").
- 2026-10-25 Phase 1 DONE (`51103b9`): A1 Xoshiro256Ssx2 rewritten (see commit msg). All 62 lib
  tests (feature simd) + 60 doc tests pass; default-feature suite unchanged (48 tests).
  New regression tests: nextu_matches_official_scalar_reference (outputs AND full internal state,
  lane-exact), vector_core_matches_scalar_core, generator_a_matches_crate_scalar_xoshiro256ss,
  nextu_is_deterministic_per_seed, cross_check_against_rand_xoshiro (dev-dep rand_xoshiro 0.8.1 +
  rand_core 0.10; odd seeds only because urng's SplitMix64 does seed|1 — deviation documented below),
  scalar_core_matches_official_reference. During the work the vector core was first wrong by one
  step (output from post-update s1) — caught by the new tests, then fixed. cabi ssx2 chunk fill
  aligned to ** output.
- 2026-10-25 Phase 2 DONE (`116eb79`): A2 (5 AVX2-tier sites → shift-or rotates / OR merges),
  A8 (WordV m128i/m256i/m512i + Sfc32x8/x16 randiv: odd-lane pairing + word selection), and a
  newly found defect A9 (Xoroshiro64Ssx8/x16 missing the ** scrambler rotl(5)*5 → SIMD stream
  ≠ scalar stream) all fixed. 11 new tests (73 lib total with simd); docs 60 ok; default 48 ok.
  Next: Phase 3 (A3 shuffle).
- 2026-10-25 Phase 3 DONE (`ed464ba`): A3 fixed in shuffle + shuffled (`j = i + nextu().to_index(len-i)`).
  4 new statistical tests (len2 stay 0.5; len3 census 1/6 ±15% both paths; permutation validity;
  edge cases). Referee proof: old formula reinstated → census tests fail; restored → pass.
  52 default-lib tests pass. Next: Phase 4 (A4 README include).
- 2026-10-25 Phase 4 DONE (`fc6fba2`): A4 — `lib.rs` now `include_str!("../README.md")` (correct
  case). `cargo doc` + 44 doctests pass.
- 2026-10-25 Phase 5 DONE (this commit): A5 — scalar `Word::to_randi`: `assert!(min <= max)` +  `wrapping_add(min)` (full-span `randi(i32::MIN, i32::MAX)` exact, panic contract consistent in
  both profiles); `Word::to_index` takes the 128-bit path only for `len > 2^32` (branch is
  loop-invariant at call sites; small-len fast path unchanged). b64 SIMD inline `randi`
  (Sfc64x8, Biski64x8, Cet64x8, Cet256x2) got the same assert + wrapping mapping. A6 — rand
  bridge standardized to little-endian: `impl_rand_trait!` uses `u32::from_le_bytes`; new
  `impl_rand_trait64!` gives b64 generators `Seed = [u8; 8]` (BREAKING: old bridge truncated the
  space to 2^32 and used native order); new `impl_try_rng_trait64!` gives the 16 b64 generators
  the previously missing `TryRng`/`Rng` impls. Tests: full-span panic-free (scalar u32/u64),
  min>max should_panic (scalar u32), to_index ≥2^32 exact + small-path unchanged; b64 SIMD
  randi full-span lane-exact (Sfc64x8 vs scalar) / sign-split (Biski64x8, Cet64x8, Cet256x2)
  and min>max panics; LE seed bridges for b32 + b64 (both seed words verified). Counts:
  default-lib 57 ok; simd-lib 89 ok; simd+rand-lib 93 ok; doc+rand 44 ok; `cargo build
  --features rand` clean. Next: Phase 6 (A7 misc).
- 2026-10-25 Phase 6 DONE (this commit): A7 misc —
  (1) `choice`/`choice_mut` empty-slice behavior is now a **documented panic**
  (`# Panics` sections + trait-level note explaining the deliberate
  inconsistency vs `Shuffle::shuffle`'s `Err`/`Result`); behavior unchanged
  (public API preserved).
  (2) All 4 samplers (`Alias32/64`, `Bst32/64`) validate weights at build and
  at `weights()`: empty slice, negative/NaN weights, and zero-total weights
  panic with a `sampler weights:` message in both profiles — previously these
  silently produced a uniform/unweighted distribution or pinned `sample()`
  to index 0 (probed on the baseline). Zero weights on individual outcomes
  stay legal (test: never selected). Shared helper `sampler::validate_weights`.
  (3) Alias table memory: `alias: Vec<usize>` → `Vec<u32>` private change
  (halved; guarded by a `u32::MAX` count assertion) — documented memory
  optimization, no API change.
  (4) SeedGen RDSEED/RDRAND retry budget 4 → 10 (Intel ISA guidance;
  4 could abort mid-backoff), doc: "reproducibly" claim removed and replaced
  with an explicit non-reproducibility statement.
  (5) Doc fixes: `Sfc64x8`/`Biski64x8` struct + `nextu` docs (said "4-way/
  AVX2/4 values", are 8-way/AVX-512/`[u64;8]`); README: `mt19937_new` C
  signature (no `warm` parameter), wide-feature table additions
  (`Threefry32x2x*`, `Threefry32x4x*`, `Squares32x*`), `Sfc32x4` moved out of
  the 32-bit standard table into an explicit SSE2 tier, SIMD section intro
  now states per-tier compile-time ISA requirements (compile-time gating,
  runtime detection only in the C ABI), new "Determinism / compatibility
  notes" section (PCG `inc` without `|1`, MT19937/SFMT custom seeding —
  will-not-fix with rationale) and the cabi bulk-fill ≠ scalar sequential
  stream note. Tests: choice should_panic + mut/single/reachability,
  16 sampler tests (panic contracts ×4 families ×{all-zero, negative,
  update-path, empty} + zero-entry legality). Suite counts: lib+sampler+
  seedgen 80 ok, doc 52 ok. Next: Phase 7 (C ABI guards).
- 2026-10-25 Phase 7 DONE (this commit): C ABI hardening + new defect A10.
  **Guards:** all 176 bulk cabi entry points now start with
  `if count == 0 || ptr.is_null() || out.is_null() { return; }`
  (78 extended, 98 newly inserted; script `guard_cabi.py` used
  brace-matching body parsing + per-file brace balance assertion).
  Effects, pre-guard referee evidence (stash/run/pop):
  (1) null pointer deref → selected sites segmented/aborted
  (`sfc32_next_f32s` at `src/cabi/prng/b32/sfc.rs:22`, abort `0xc0000409`);
  (2) zero-count calls were NOT no-ops — several wrappers consumed one
  generator word to compute the reseed base before checking count
  (twin-stream test `zc_sfc32`, `zc_splitmix32` FAILED pre-guard, PASS
  post-guard; `zc_xorshift64` was already fine — `fill_with` never calls the
  closure at count 0). Now the full matrix of null/zero-count tests is green
  and zero-count calls leave the generator state bit-identical.
  **A10 (range mapping family), tests-first pre-patch run
  (`cargo test --features cabi,simd --test cabi_guards -- --test-threads 1`):**
  - 10 FAIL (full span → all-`i32::MIN`): `fs_sfc32x4`, `fs_sfc32x8`,
    `fs_sfc32x16`, `fs_jsf32x16`, `fs_xoroshiro64ssx8`, `fs_xoroshiro64ssx16`,
    `fs_xoshiro128ppx16`, `fs_xoshiro128ssx16`, `fs_philox32x4x4`,
    `fs_squares32x8`.
  - Process ABORT `0xc0000409` (overflow panic inside `extern "C"` cannot
    unwind): `fs_scalar_squares32` (own scalar kernel
    `squares.rs:484-487+496`), `fs_pcg32x8` / `near_full_pcg32x8`
    (`pcg.rs:263/273`), `near_full_squares32x8` (`squares.rs:239/249`),
    `near_full_span_b64_stays_in_range` (`splitmix.rs:100:38`),
    `fs_b64_splitmix64`, `fs_b64_philox64` (`b64/philox.rs:122:13`),
    `fs_b64_threefish256` (`b64/threefish.rs:76:44`), `b64_full_span`
    (biski64x8 `apply_range`, `biski.rs:472`).
  - Already exact pre-patch (documented, no change): scalar A5-routed sites
    (sfc32, pcg32, mt19937, philox32 — passing), threefry32x4 and philox32
    wrapn-kernels (ARITHMETICALLY EXACT — verified that wrapn elementwise
    `Wrap` Add/Mul/Sub is **wrapping**, see `Wrap::new(u32::MAX) + 1 == 0`
    in wrapn-0.10.0), and the SIMD-vector near-full paths (modular
    `_add_epi32` lanes; passed).
  **Fixes:** (a) 10 SIMD b32 sites: full-span branch
  `min == i32::MIN && max == i32::MAX` routes through the type's own
  `*_next_u32s(ptr, out.cast(), count)` and applies the exact bijective
  top-bit-XOR map `*v = (*v as u32).wrapping_add(min as u32) as i32`
  (uniform u32 → uniform i32 over the whole span; the lane mapping cannot
  represent range == 2^32). (b) wrapping mapping at pcg32x8 (chunk + rem),
  squares32 scalar tail (4+1 sites), squares32x8 scalar remainder (2),
  biski64x8 `apply_range`, philox64, threefish256, splitmix64:
  `((x * range) >> N) as uN` then `.wrapping_add(min as uN) as iN` —
  quotient + min ≤ max always, so the modular closed form is exact for
  near-full AND full spans with no branch (pcg32x8 u64 range math is
  pure-scalar per-lane: no broadcast truncation exists → no branch needed).
  Tests: `tests/cabi_guards.rs`, 39 tests (cabi tier + `#[cfg(simd)]`
  sub-tier): full-span sanity (25 sites: both signs, ±sign-split window,
  >½ distinct values), near-full in-range (incl. odd counts hitting the
  scalar remainder loops), null/zero-count no-op twins, post-fullspan
  bounded call, inclusive small range, degenerate one-value range.
  Pre-patch detection matrix recorded above. Suite counts post-fix:
  cabi_guards 39 ok; default-lib 59 ok; simd-lib 91 ok; sampler/seedgen-lib
  80 ok; doc(cabi,simd) 61 ok; `cargo build --features full` clean. README
  gained the FFI safety contract note (no-op guard conditions; min>max
  contract violation aborts the process; exact i32/i64 range mapping).
  Next: Phase 8 (full matrix, benches, finalize, version bump).


## Baseline benchmarks (pre-fix; criterion mean, release profile, `--noplot` same conditions)

Format: `test → mean ns`. Group = criterion group name.

| Bench | Mean |
|-------|------:|
| scalar/fill/sfc32_nextu (4096 words) | 3173.3 |
| scalar/fill/splitmix64_nextu (2048 words) | 1567.1 |
| scalar/fill/sfc64_nextu (2048 words) | 1186.2 |
| scalar/fill/xoshiro256ss_nextu (2048 words) | 1060.7 |
| scalar/ranged/sfc32_randi (4096 draws) | 3206.0 |
| scalar/ranged/sfc32_to_index (4096 draws) | 3264.0 |
| scalar/ranged/sfc32_shuffle100 | 112.5 |
| simd/fill/sfc32x4_nextu (1024 w) | 1553.5 |
| simd/fill/sfc32x8_nextu (4096 w) | 3062.8 |
| simd/fill/jsf32x8_nextu (4096 w) | 18058.1 |
| simd/fill/xoroshiro64ssx8_nextu (4096 w) | 14332.5 |
| simd/fill/pcg32x8_nextu (4096 w) | 2201.8 |
| simd/fill/sfc64x8_nextu (4096 w) | 28224.0 |
| simd/fill/cet256x2_nextu (2048 w) | 22589.6 |
| simd/fill/biski64x8_nextu (4096 w) | 15353.6 |
| simd/fill/xoshiro256ssx2_nextu (2048 w, broken pre-fix) | 140198.3 |
| simd/randiv/jsf32x8_randiv (8 lanes/call) | 53.5 |

Post-fix numbers go into "Performance changes" after the Phase 8 run. Raw snapshots are in
`bench-baseline/` (on disk, untracked).

## Resume state (ephemeral checkpoint — update after every step)

- Last completed: Phase 7 (C ABI guards + A10), commit `(this commit)`. Next: Phase 8 —
  full `cargo test --locked` (default + all-features), feature-matrix builds
  (rand, seedgen, sampler, cabi±simd), post-fix criterion benches vs
  `bench-baseline/` → "Performance changes" section, finalize "Unfixed issues"
  / "Compatibility impact" / "Remaining risks", version bump 1.0.0 → 1.0.1
  (build.rs auto-updates the README version string).
- Phase 7 test file: `tests/cabi_guards.rs` (cabi + simd tiers). Scratch
  guard scripts (idempotent, kept as evidence):
  `C:\Users\cets0\AppData\Local\Temp\opencode\guard_cabi.py` +
  `fix_guard_indent.py`; pre-patch/refs logs `prepatch_test.log`,
  `prepatch_main.log`, `prepatch_b64*.log`, `preguard_zc.log`,
  `preguard_null.log` (same Temp dir).
- Baseline benches: done, snapshotted to `bench-baseline/` (untracked) + table in this report.
- Scratch probe crate: `C:\Users\cets0\AppData\Local\Temp\opencode\urng-fix-worker` (bins
  probe_wordv128, probe_ssx2, probe_shuffle, probe_ssx2_core; path-dep on this repo as `urng-fixed`).
- CPU here: AVX-512VL+ capable (AVX512VL intrinsics do NOT crash here; risk is AVX2-only machines).
- Phase 8 prep: cabi stream-dup check between consecutive chunk fills was NOT explicitly
  deep-probed in Phase 7 beyond the twin-stream zero-count tests — if time permits, do a
  quick consecutive-bulk-call continuation sanity in Phase 8. Note the baseline log
  `prepatch_test.log` in Temp is UTF-8; `prepatch_main.log` may be UTF-16 when written via
  PowerShell redirection of older runs.
- Findings to include in the final report: SplitMix64 begins at `seed | 1` (documented
  deviation → cross-checks must use odd seeds); cabi ssx2 emitted xoshiro256++ (now **); criterion
  on this machine for `simd/randiv` ≈ 53.5 ns/8 lanes.

## Unfixed issues

(final list; nothing to report so far — A7 documentation items pending; PCG `inc|1` and MT19937
seeding recorded above as will-not-fix with rationale in the final report)

## Compatibility impact

(final list; so far: A6 makes `SeedableRng::Seed = [u8; 8]` for all 16 b64 generators
(BREAKING) and standardizes bridge seeds to little-endian (affects byte-level consumers on
big-endian hosts only, a determinism fix))

## Performance changes

(final list; see baseline benchmark section)

## Remaining risks

(final list)
