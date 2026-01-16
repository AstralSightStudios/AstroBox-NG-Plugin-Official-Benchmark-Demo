use std::time::Instant;

pub const BENCH_SEED: u32 = 12345;
pub const BENCH_N1: u64 = 300_000_000;
pub const BENCH_N2: u64 = 200_000_000;
pub const BENCH_WARMUP: usize = 3;
pub const BENCH_REPEATS: usize = 9;
pub const TOTAL_STEPS: usize = 2 * (BENCH_WARMUP + BENCH_REPEATS);

#[derive(Clone, Copy)]
pub enum BenchPhase {
    Warmup,
    Measure,
}

#[derive(Clone, Copy)]
pub enum BenchStepStatus {
    Started,
    Finished,
}

pub struct ProgressUpdate {
    pub bench_id: &'static str,
    pub phase: BenchPhase,
    pub index: usize,
    pub total: usize,
    pub completed_steps: usize,
    pub total_steps: usize,
    pub status: BenchStepStatus,
}

pub struct BenchStats {
    pub min: f64,
    pub p50: f64,
    pub p95: f64,
    pub max: f64,
}

pub struct BenchCaseResult {
    pub id: &'static str,
    pub digest: u64,
    pub stats: BenchStats,
}

pub struct BenchmarkResult {
    pub t1: BenchCaseResult,
    pub t2: BenchCaseResult,
    pub final_digest: u64,
    pub json: String,
}

fn median(sorted: &[f64]) -> f64 {
    let n = sorted.len();
    if n == 0 {
        return f64::NAN;
    }
    if n % 2 == 1 {
        sorted[n / 2]
    } else {
        (sorted[n / 2 - 1] + sorted[n / 2]) / 2.0
    }
}

fn p95(sorted: &[f64]) -> f64 {
    let n = sorted.len();
    if n == 0 {
        return f64::NAN;
    }
    let idx = ((n as f64 - 1.0) * 0.95).round() as usize;
    sorted[idx.min(n - 1)]
}

// -------- xorshift32 PRNG (pure 32-bit, cross-lang) --------
struct XorShift32 {
    x: u32,
}

impl XorShift32 {
    fn new(seed: u32) -> Self {
        let x = if seed == 0 { 0x6D2B79F5 } else { seed };
        Self { x }
    }

    #[inline]
    fn next_u32(&mut self) -> u32 {
        let mut x = self.x;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.x = x;
        x
    }

    #[inline]
    fn next_f64_01(&mut self) -> f64 {
        (self.next_u32() as f64) / 4294967296.0 // 2^32
    }
}

// -------- Benchmarks --------
#[inline(never)]
fn bench_int32_mix(seed: u32, n: u64) -> u32 {
    let mut rng = XorShift32::new(seed);
    let mut acc: u32 = 0x1234_5678;

    for i in 0..n {
        let x = rng.next_u32();
        let mut v = x ^ acc;
        v = v.rotate_left((i as u32) & 31);
        v = v.wrapping_mul(0x9E37_79B1);
        v ^= v >> 16;
        acc = acc.wrapping_add(v);
        if (v & 0x8000) != 0 {
            acc ^= 0xA5A5_A5A5;
        }
    }

    std::hint::black_box(acc)
}

#[inline(never)]
fn bench_fp64_dot(seed: u32, n: u64) -> u64 {
    let mut rng = XorShift32::new(seed ^ 0xDEAD_BEEF);
    let mut sum: f64 = 0.0;
    let c: f64 = 1e-9;

    for _ in 0..n {
        let a = rng.next_f64_01();
        let b = rng.next_f64_01();
        sum = sum + (a * b + c);
    }

    std::hint::black_box(sum.to_bits())
}

fn run_bench<F, P>(
    name: &'static str,
    warmup: usize,
    repeats: usize,
    mut f: F,
    progress: &mut P,
    completed_steps: &mut usize,
    total_steps: usize,
) -> (u64, Vec<f64>)
where
    F: FnMut() -> u64,
    P: FnMut(ProgressUpdate),
{
    let mut last = 0u64;
    for i in 0..warmup {
        progress(ProgressUpdate {
            bench_id: name,
            phase: BenchPhase::Warmup,
            index: i + 1,
            total: warmup,
            completed_steps: *completed_steps,
            total_steps,
            status: BenchStepStatus::Started,
        });
        last = f();
        *completed_steps += 1;
        progress(ProgressUpdate {
            bench_id: name,
            phase: BenchPhase::Warmup,
            index: i + 1,
            total: warmup,
            completed_steps: *completed_steps,
            total_steps,
            status: BenchStepStatus::Finished,
        });
    }

    let mut times: Vec<f64> = Vec::with_capacity(repeats);
    for i in 0..repeats {
        progress(ProgressUpdate {
            bench_id: name,
            phase: BenchPhase::Measure,
            index: i + 1,
            total: repeats,
            completed_steps: *completed_steps,
            total_steps,
            status: BenchStepStatus::Started,
        });
        let t0 = Instant::now();
        last = f();
        times.push(t0.elapsed().as_secs_f64() * 1000.0);
        *completed_steps += 1;
        progress(ProgressUpdate {
            bench_id: name,
            phase: BenchPhase::Measure,
            index: i + 1,
            total: repeats,
            completed_steps: *completed_steps,
            total_steps,
            status: BenchStepStatus::Finished,
        });
    }
    tracing::info!("{} done. last_digest={:016x}", name, last);
    (last, times)
}

fn calc_stats(times: &mut [f64]) -> BenchStats {
    times.sort_by(|a, b| a.partial_cmp(b).unwrap());
    BenchStats {
        min: times.first().copied().unwrap_or(0.0),
        p50: median(times),
        p95: p95(times),
        max: times.last().copied().unwrap_or(0.0),
    }
}

pub fn run_benchmark<P>(mut progress: P) -> BenchmarkResult
where
    P: FnMut(ProgressUpdate),
{
    let mut completed_steps = 0usize;
    let (d1, mut t1) = run_bench(
        "T1_INT32_MIX",
        BENCH_WARMUP,
        BENCH_REPEATS,
        || bench_int32_mix(BENCH_SEED, BENCH_N1) as u64,
        &mut progress,
        &mut completed_steps,
        TOTAL_STEPS,
    );

    let (d2, mut t2) = run_bench(
        "T2_FP64_DOT",
        BENCH_WARMUP,
        BENCH_REPEATS,
        || bench_fp64_dot(BENCH_SEED, BENCH_N2),
        &mut progress,
        &mut completed_steps,
        TOTAL_STEPS,
    );

    let t1_stats = calc_stats(&mut t1);
    let t2_stats = calc_stats(&mut t2);
    let final_digest = d1 ^ d2;

    let json = format!(
        r#"{{
  "lang": "rust",
  "seed": {seed},
  "params": {{ "n1": {n1}, "n2": {n2}, "warmup": {warmup}, "repeats": {repeats} }},
  "results": [
    {{
      "id": "T1_INT32_MIX",
      "digest_u64": "{d1:016x}",
      "time_ms": {{ "min": {t1min:.3}, "p50": {t1p50:.3}, "p95": {t1p95:.3}, "max": {t1max:.3} }}
    }},
    {{
      "id": "T2_FP64_DOT",
      "digest_u64": "{d2:016x}",
      "time_ms": {{ "min": {t2min:.3}, "p50": {t2p50:.3}, "p95": {t2p95:.3}, "max": {t2max:.3} }}
    }}
  ],
  "final_digest_u64": "{final_digest:016x}"
}}"#,
        seed = BENCH_SEED,
        n1 = BENCH_N1,
        n2 = BENCH_N2,
        warmup = BENCH_WARMUP,
        repeats = BENCH_REPEATS,
        d1 = d1,
        d2 = d2,
        t1min = t1_stats.min,
        t1p50 = t1_stats.p50,
        t1p95 = t1_stats.p95,
        t1max = t1_stats.max,
        t2min = t2_stats.min,
        t2p50 = t2_stats.p50,
        t2p95 = t2_stats.p95,
        t2max = t2_stats.max,
        final_digest = final_digest
    );

    BenchmarkResult {
        t1: BenchCaseResult {
            id: "T1_INT32_MIX",
            digest: d1,
            stats: t1_stats,
        },
        t2: BenchCaseResult {
            id: "T2_FP64_DOT",
            digest: d2,
            stats: t2_stats,
        },
        final_digest,
        json,
    }
}
