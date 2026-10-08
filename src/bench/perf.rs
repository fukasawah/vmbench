use crate::bench::common::*;
use crate::bench::cpu_mix::vmbench_k_mix_iter;
use crate::bench::Benchmark;
use crate::runner::{Ctx, EntryMeta, ImplInfo, MetricDef, MetricKind, ParamValue};
use crate::stats::Direction;
use crate::sys;
use crate::time;

// ---------------------------------------------------------------------------
// Hardware performance counters (optional). If perf_event_open is not
// permitted the benchmark is reported as unsupported.
// ---------------------------------------------------------------------------

const PERF_TYPE_HARDWARE: u32 = 0;
const PERF_COUNT_HW_CPU_CYCLES: u64 = 0;
const PERF_COUNT_HW_INSTRUCTIONS: u64 = 1;
const PERF_COUNT_HW_CACHE_REFERENCES: u64 = 2;
const PERF_COUNT_HW_CACHE_MISSES: u64 = 3;
const PERF_COUNT_HW_BRANCH_INSTRUCTIONS: u64 = 4;
const PERF_COUNT_HW_BRANCH_MISSES: u64 = 5;

fn open_event(config: u64) -> Option<i32> {
    // perf_event_attr is a 128-byte structure; only the leading fields matter
    // here. flags: exclude_kernel (bit 5) | exclude_hv (bit 6).
    let mut attr = [0u8; 128];
    unsafe {
        core::ptr::write_unaligned(attr.as_mut_ptr() as *mut u32, PERF_TYPE_HARDWARE);
        core::ptr::write_unaligned(attr.as_mut_ptr().add(4) as *mut u32, 128);
        core::ptr::write_unaligned(attr.as_mut_ptr().add(8) as *mut u64, config);
        core::ptr::write_unaligned(attr.as_mut_ptr().add(40) as *mut u64, (1 << 5) | (1 << 6));
    }
    sys::perf_event_open(attr.as_mut_ptr(), 0, -1, -1, 0).ok()
}

fn read_counter(fd: i32) -> Option<u64> {
    let mut buf = [0u8; 8];
    match sys::read(fd, &mut buf) {
        Ok(8) => Some(u64::from_ne_bytes(buf)),
        _ => None,
    }
}

pub const M_CYCLES: MetricDef = MetricDef {
    key: "cycles",
    unit: "cycles",
    direction: Direction::Neutral,
    kind: MetricKind::Scalar,
};
pub const M_INSTRUCTIONS: MetricDef = MetricDef {
    key: "instructions",
    unit: "instructions",
    direction: Direction::Neutral,
    kind: MetricKind::Scalar,
};
pub const M_IPC: MetricDef = MetricDef {
    key: "ipc",
    unit: "insn/cycle",
    direction: Direction::HigherBetter,
    kind: MetricKind::Scalar,
};
pub const M_BRANCHES: MetricDef = MetricDef {
    key: "branches",
    unit: "branches",
    direction: Direction::Neutral,
    kind: MetricKind::Scalar,
};
pub const M_BRANCH_MISS_PCT: MetricDef = MetricDef {
    key: "branch_miss_pct",
    unit: "%",
    direction: Direction::LowerBetter,
    kind: MetricKind::Scalar,
};
pub const M_CACHE_REFS: MetricDef = MetricDef {
    key: "cache_references",
    unit: "refs",
    direction: Direction::Neutral,
    kind: MetricKind::Scalar,
};
pub const M_CACHE_MISS_PCT: MetricDef = MetricDef {
    key: "cache_miss_pct",
    unit: "%",
    direction: Direction::LowerBetter,
    kind: MetricKind::Scalar,
};

/// True when the kernel permits opening a hardware event for this process.
pub fn probe() -> bool {
    match open_event(PERF_COUNT_HW_CPU_CYCLES) {
        Some(fd) => {
            let _ = sys::close(fd);
            true
        }
        None => false,
    }
}

pub struct PerfCounters;

impl Benchmark for PerfCounters {
    fn meta(&self) -> &'static EntryMeta {
        static M: EntryMeta = EntryMeta {
            id: "perf.counters.v1",
            version: 1,
            description: "Hardware performance counters for the fixed mix workload",
            imp: ImplInfo {
                source: "src/bench/perf.rs",
                kernel: "vmbench_k_mix_iter",
                algorithm: "perf_event_open hardware events around a fixed-duration mix run",
                isa: BASELINE_ISA,
            },
            metrics: &[
                M_CYCLES,
                M_INSTRUCTIONS,
                M_IPC,
                M_BRANCHES,
                M_BRANCH_MISS_PCT,
                M_CACHE_REFS,
                M_CACHE_MISS_PCT,
            ],
        };
        &M
    }

    fn supported(&self, ctx: &Ctx) -> Result<(), &'static str> {
        if ctx.env.online_cpus.is_empty() {
            return Err("no online cpus");
        }
        // Probe one event so unsupported kernels/VMs are reported cleanly.
        match open_event(PERF_COUNT_HW_CPU_CYCLES) {
            Some(fd) => {
                let _ = sys::close(fd);
                Ok(())
            }
            None => Err("perf_event_open denied (perf_event_paranoid or PMU unavailable)"),
        }
    }

    fn run(&self, ctx: &mut Ctx, entry: usize) -> Result<(), &'static str> {
        let cpu = ctx.default_cpu();
        ctx.pin_self(cpu);
        ctx.add_param(entry, "cpu", ParamValue::Int(cpu as i64));
        ctx.add_param(entry, "workload", ParamValue::Str("cpu.mix"));
        let seed = 0x0123_4567_89ab_cdefu64;
        let goal = (ctx.cfg.target_run_ns / 8).clamp(2_000_000, 20_000_000);
        let chunk = crate::runner::calibrate_chunk(goal, |n| {
            let _ = unsafe { vmbench_k_mix_iter(n, seed) };
            n
        });
        for _ in 0..ctx.cfg.runs.max(1) {
            let fds = [
                open_event(PERF_COUNT_HW_CPU_CYCLES),
                open_event(PERF_COUNT_HW_INSTRUCTIONS),
                open_event(PERF_COUNT_HW_BRANCH_INSTRUCTIONS),
                open_event(PERF_COUNT_HW_BRANCH_MISSES),
                open_event(PERF_COUNT_HW_CACHE_REFERENCES),
                open_event(PERF_COUNT_HW_CACHE_MISSES),
            ];
            let t0 = time::mono_ns();
            let deadline = time::Deadline::new(ctx.cfg.target_run_ns);
            let mut units = 0u64;
            loop {
                let _ = unsafe { vmbench_k_mix_iter(chunk, seed) };
                units = units.saturating_add(chunk);
                if deadline.expired() {
                    break;
                }
            }
            let dur = time::mono_ns().wrapping_sub(t0).max(1);
            let mut vals = [None; 6];
            for (i, fd) in fds.iter().enumerate() {
                if let Some(fd) = fd {
                    vals[i] = read_counter(*fd);
                    let _ = sys::close(*fd);
                }
            }
            ctx.begin_run(entry, dur, units);
            let get = |i: usize| vals[i].map(|v| v as f64);
            if let Some(v) = get(0) {
                ctx.scalar(entry, "cycles", v);
            }
            if let Some(v) = get(1) {
                ctx.scalar(entry, "instructions", v);
            }
            if let (Some(c), Some(i)) = (get(0), get(1)) {
                if c > 0.0 {
                    ctx.scalar(entry, "ipc", i / c);
                }
            }
            if let Some(v) = get(2) {
                ctx.scalar(entry, "branches", v);
            }
            if let (Some(b), Some(m)) = (get(2), get(3)) {
                if b > 0.0 {
                    ctx.scalar(entry, "branch_miss_pct", m / b * 100.0);
                }
            }
            if let Some(v) = get(4) {
                ctx.scalar(entry, "cache_references", v);
            }
            if let (Some(r), Some(m)) = (get(4), get(5)) {
                if r > 0.0 {
                    ctx.scalar(entry, "cache_miss_pct", m / r * 100.0);
                }
            }
            ctx.end_run(entry);
        }
        Ok(())
    }
}
