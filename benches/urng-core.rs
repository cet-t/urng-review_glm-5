//! Micro-benchmarks for the `urng` hot paths.
//!
//! Purpose: a stable before/after comparison for the 1.0.1 correctness fixes
//! (see FIX_REPORT.md at the repository root). The scalar tier requires no
//! features; the SIMD tier needs `--features simd` and only runs on x86_64
//! CPUs that actually support the relevant extensions (runtime check).
//!
//! Run:
//!   cargo bench --bench urng-core
//!   cargo bench --bench urng-core --features simd

use std::hint::black_box;
use std::time::Duration;

use criterion::{criterion_group, criterion_main, Criterion, Throughput};
use urng::{Rng, Shuffle, Word};

const N_WORDS: usize = 4096;

fn bench_scalar(c: &mut Criterion) {
    let mut fill = c.benchmark_group("scalar/fill");
    fill.throughput(Throughput::Bytes((N_WORDS * 4) as u64));

    fill.bench_function("sfc32_nextu", |b| {
        let mut rng = urng::Sfc32::new(0x1234_5678);
        b.iter(|| {
            let mut acc: u32 = 0;
            for _ in 0..N_WORDS {
                acc ^= rng.nextu();
            }
            black_box(acc)
        })
    });

    fill.bench_function("splitmix64_nextu", |b| {
        let mut rng = urng::SplitMix64::new(0x1234_5678);
        b.iter(|| {
            let mut acc: u64 = 0;
            for _ in 0..N_WORDS / 2 {
                acc ^= rng.nextu();
            }
            black_box(acc)
        })
    });

    fill.bench_function("sfc64_nextu", |b| {
        let mut rng = urng::Sfc64::new(0x1234_5678);
        b.iter(|| {
            let mut acc: u64 = 0;
            for _ in 0..N_WORDS / 2 {
                acc ^= rng.nextu();
            }
            black_box(acc)
        })
    });

    fill.bench_function("xoshiro256ss_nextu", |b| {
        let mut rng = urng::Xoshiro256Ss::new(0x1234_5678);
        b.iter(|| {
            let mut acc: u64 = 0;
            for _ in 0..N_WORDS / 2 {
                acc ^= rng.nextu();
            }
            black_box(acc)
        })
    });

    fill.finish();

    let mut ranged = c.benchmark_group("scalar/ranged");
    ranged.throughput(Throughput::Elements(N_WORDS as u64));

    ranged.bench_function("sfc32_randi", |b| {
        let mut rng = urng::Sfc32::new(42);
        b.iter(|| {
            let mut acc: i32 = 0;
            for _ in 0..N_WORDS {
                acc ^= rng.randi(0, 4095);
            }
            black_box(acc)
        })
    });

    ranged.bench_function("sfc32_to_index", |b| {
        let mut rng = urng::Sfc32::new(42);
        b.iter(|| {
            let mut acc: usize = 0;
            for _ in 0..N_WORDS {
                acc ^= rng.nextu().to_index(1000);
            }
            black_box(acc)
        })
    });

    ranged.throughput(Throughput::Elements(100));
    ranged.bench_function("sfc32_shuffle100", |b| {
        let mut rng = urng::Sfc32::new(99);
        let mut buf: Vec<usize> = (0..100).collect();
        b.iter(|| {
            rng.shuffle(black_box(buf.as_mut_slice())).unwrap();
            black_box(buf[0])
        })
    });

    ranged.finish();
}

#[cfg(all(feature = "simd", target_arch = "x86_64"))]
fn bench_simd(c: &mut Criterion) {
    use urng::RngV;

    if !std::arch::is_x86_feature_detected!("avx2") {
        eprintln!("skipping SIMD benches: avx2 not detected");
        return;
    }

    let mut fill = c.benchmark_group("simd/fill");
    // Each of these types yields N lanes of u32 per call, one call covers
    // N words; iterate enough calls to touch N_WORDS words total.
    fill.throughput(Throughput::Bytes((N_WORDS * 4) as u64));

    fill.bench_function("sfc32x8_nextu", |b| {
        let mut rng = unsafe { urng::Sfc32x8::new(1) };
        b.iter(|| {
            let mut acc: u32 = 0;
            for _ in 0..N_WORDS / 8 {
                for x in rng.nextu() {
                    acc ^= x;
                }
            }
            black_box(acc)
        })
    });

    fill.bench_function("jsf32x8_nextu", |b| {
        let mut rng = unsafe { urng::Jsf32x8::new(1) };
        b.iter(|| {
            let mut acc: u32 = 0;
            for _ in 0..N_WORDS / 8 {
                let v: [u32; 8] = unsafe { std::mem::transmute(rng.nextuv()) };
                for x in v {
                    acc ^= x;
                }
            }
            black_box(acc)
        })
    });

    fill.bench_function("xoroshiro64ssx8_nextu", |b| {
        let mut rng = unsafe { urng::Xoroshiro64Ssx8::new(1) };
        b.iter(|| {
            let mut acc: u32 = 0;
            for _ in 0..N_WORDS / 8 {
                let v: [u32; 8] = unsafe { std::mem::transmute(rng.nextuv()) };
                for x in v {
                    acc ^= x;
                }
            }
            black_box(acc)
        })
    });

    fill.bench_function("pcg32x8_nextu", |b| {
        let mut rng = unsafe { urng::Pcg32x8::new(1) };
        b.iter(|| {
            let mut acc: u32 = 0;
            for _ in 0..N_WORDS / 8 {
                for x in unsafe { rng.nextu() } {
                    acc ^= x;
                }
            }
            black_box(acc)
        })
    });

    // 128-bit (SSE2 baseline) tier.
    fill.bench_function("sfc32x4_nextu", |b| {
        let mut rng = unsafe { urng::Sfc32x4::new(1) };
        b.iter(|| {
            let mut acc: u32 = 0;
            for _ in 0..N_WORDS / 4 {
                for x in rng.nextu() {
                    acc ^= x;
                }
            }
            black_box(acc)
        })
    });

    // 64-bit SIMD tier.
    fill.bench_function("sfc64x8_nextu", |b| {
        let mut rng = unsafe { urng::Sfc64x8::new(1) };
        b.iter(|| {
            let mut acc: u64 = 0;
            for _ in 0..N_WORDS / 8 {
                for x in unsafe { rng.nextu() } {
                    acc ^= x;
                }
            }
            black_box(acc)
        })
    });

    fill.bench_function("cet256x2_nextu", |b| {
        let mut rng = unsafe { urng::Cet256x2::new(1) };
        b.iter(|| {
            let mut acc: u64 = 0;
            for _ in 0..N_WORDS / 2 {
                for x in unsafe { rng.nextu() } {
                    acc ^= x;
                }
            }
            black_box(acc)
        })
    });

    fill.bench_function("biski64x8_nextu", |b| {
        let mut rng = unsafe { urng::Biski64x8::new(1) };
        b.iter(|| {
            let mut acc: u64 = 0;
            for _ in 0..N_WORDS / 8 {
                for x in unsafe { rng.nextu() } {
                    acc ^= x;
                }
            }
            black_box(acc)
        })
    });

    // A1 target: requires avx512f at runtime.
    if std::arch::is_x86_feature_detected!("avx512f") {
        fill.bench_function("xoshiro256ssx2_nextu", |b| {
            let mut rng = urng::Xoshiro256Ssx2::new(1);
            b.iter(|| {
                let mut acc: u64 = 0;
                for _ in 0..N_WORDS / 2 {
                    for x in rng.nextu() {
                        acc ^= x;
                    }
                }
                black_box(acc)
            })
        });
    } else {
        eprintln!("skipping xoshiro256ssx2 bench: avx512f not detected");
    }

    fill.finish();

    // RngV::randiv path (exercises WordV::to_randi after the A8 fix).
    let mut ranged = c.benchmark_group("simd/randiv");
    ranged.throughput(Throughput::Elements((N_WORDS / 8) as u64));
    ranged.bench_function("jsf32x8_randiv", |b| {
        use std::arch::x86_64::{_mm256_set1_epi32, _mm256_setzero_si256};
        let mut rng = unsafe { urng::Jsf32x8::new(1) };
        b.iter(|| {
            let v = unsafe { rng.randiv(_mm256_set1_epi32(1000), _mm256_setzero_si256()) };
            black_box(v)
        })
    });
    ranged.finish();
}

fn config() -> Criterion {
    Criterion::default()
        .warm_up_time(Duration::from_millis(500))
        .measurement_time(Duration::from_secs(2))
}

#[cfg(all(feature = "simd", target_arch = "x86_64"))]
criterion_group! {
    name = benches;
    config = config();
    targets = bench_scalar, bench_simd
}

#[cfg(not(all(feature = "simd", target_arch = "x86_64")))]
criterion_group! {
    name = benches;
    config = config();
    targets = bench_scalar
}

criterion_main!(benches);
