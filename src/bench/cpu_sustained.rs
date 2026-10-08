use crate::bench::common::*;
use crate::bench::cpu_mix::vmbench_k_mix_iter;
use crate::bench::Benchmark;
use crate::runner::{Ctx, EntryMeta, ImplInfo, MetricDef, MetricKind, ParamValue, Point};
use crate::stats::Direction;
use crate::time;

pub const M_THROUGHPUT_CURVE: MetricDef = MetricDef {
    key: "throughput",
    unit: "iter/s",
    direction: Direction::HigherBetter,
    kind: MetricKind::Curve,
};

pub const M_MIN: MetricDef = MetricDef {
    key: "throughput_min",
    unit: "iter/s",
    direction: Direction::HigherBetter,
    kind: MetricKind::Scalar,
};
pub const M_MAX: MetricDef = MetricDef {
    key: "throughput_max",
    unit: "iter/s",
    direction: Direction::HigherBetter,
    kind: MetricKind::Scalar,
};
pub const M_MED: MetricDef = MetricDef {
    key: "throughput_median",
    unit: "iter/s",
    direction: Direction::HigherBetter,
    kind: MetricKind::Scalar,
};

pub struct Sustained;

impl Benchmark for Sustained {
    fn meta(&self) -> &'static EntryMeta {
        static M: EntryMeta = EntryMeta {
            id: "cpu.sustained.v1",
            version: 1,
            description: "Sustained throughput over time (time-dependent, not directly comparable)",
            imp: ImplInfo {
                source: "src/bench/cpu_sustained.rs",
                kernel: "vmbench_k_mix_iter",
                algorithm: "fixed mix kernel run continuously; throughput recorded per 1 s bucket",
                isa: BASELINE_ISA,
            },
            metrics: &[M_THROUGHPUT_CURVE, M_MIN, M_MED, M_MAX],
        };
        &M
    }

    fn quick_skip(&self) -> bool {
        true
    }

    fn run(&self, ctx: &mut Ctx, entry: usize) -> Result<(), &'static str> {
        let cpu = ctx.default_cpu();
        ctx.pin_self(cpu);
        let total_ns = ctx.cfg.sustained_ns.max(1_000_000_000);
        ctx.add_param(entry, "duration_s", ParamValue::Int((total_ns / 1_000_000_000) as i64));
        ctx.add_param(entry, "bucket_s", ParamValue::Int(1));
        ctx.add_param(entry, "cpu", ParamValue::Int(cpu as i64));
        ctx.annotate("sustained: time-dependent workload; may be affected by provider burst credits and throttling");

        let seed = 0x0123_4567_89ab_cdefu64;
        // Calibrate a ~50 ms chunk.
        let mut chunk_iters = 100_000u64;
        let mut chunk_ns = 1u64;
        for _ in 0..24 {
            let t0 = time::mono_ns();
            let _ = unsafe { vmbench_k_mix_iter(chunk_iters, seed) };
            chunk_ns = time::mono_ns().wrapping_sub(t0).max(1);
            if chunk_ns >= 50_000_000 {
                break;
            }
            chunk_iters = chunk_iters.saturating_mul(4);
        }
        let scaled = chunk_iters as u128 * 50_000_000u128 / chunk_ns as u128;
        let chunk_iters = if scaled > 1_000_000_000 {
            1_000_000_000
        } else {
            (scaled as u64).max(1)
        };

        let bucket_ns = 1_000_000_000u64;
        let mut points: [Point; 128] = [Point { x: 0.0, y: 0.0 }; 128];
        let mut npoints = 0usize;
        let start = time::mono_ns();
        let mut total_iters = 0u64;
        let mut min_rate = f64::MAX;
        let mut max_rate = 0f64;
        let mut rates = [0f64; 128];
        let mut nrates = 0usize;

        while time::mono_ns().wrapping_sub(start) < total_ns && npoints < 128 {
            let b0 = time::mono_ns();
            let mut units = 0u64;
            while time::mono_ns().wrapping_sub(b0) < bucket_ns {
                let _ = unsafe { vmbench_k_mix_iter(chunk_iters, seed) };
                units = units.saturating_add(chunk_iters);
                if time::mono_ns().wrapping_sub(start) >= total_ns {
                    break;
                }
            }
            let b1 = time::mono_ns();
            let d = b1.wrapping_sub(b0).max(1);
            let rate = units as f64 / (d as f64 / 1e9);
            points[npoints] = Point {
                x: (b1.wrapping_sub(start)) as f64 / 1e9,
                y: rate,
            };
            npoints += 1;
            rates[nrates.min(127)] = rate;
            nrates += 1;
            total_iters = total_iters.saturating_add(units);
            if rate < min_rate {
                min_rate = rate;
            }
            if rate > max_rate {
                max_rate = rate;
            }
        }
        let elapsed = time::mono_ns().wrapping_sub(start).max(1);
        ctx.begin_run(entry, elapsed, total_iters);
        ctx.curve(entry, "throughput", &points[..npoints]);
        let mut sorted = rates;
        let m = nrates.min(128);
        sorted[..m].sort_unstable_by(|a, b| a.partial_cmp(b).unwrap_or(core::cmp::Ordering::Equal));
        let median = if m == 0 {
            0.0
        } else if m % 2 == 1 {
            sorted[m / 2]
        } else {
            (sorted[m / 2 - 1] + sorted[m / 2]) / 2.0
        };
        ctx.scalar(entry, "throughput_min", if m == 0 { 0.0 } else { min_rate });
        ctx.scalar(entry, "throughput_median", median);
        ctx.scalar(entry, "throughput_max", max_rate);
        ctx.end_run(entry);
        Ok(())
    }
}
