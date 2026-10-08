use crate::bench::common::*;
use crate::bench::mem_bw::{BwArgs, M_BYTES};
use crate::bench::Benchmark;
use crate::runner::{Ctx, EntryMeta, ImplInfo, ParamValue};

/// Cache-level bandwidth: read/write/copy on working sets sized to L1, L2 and
/// the last-level cache reported by sysfs (with sane fallbacks).
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Level {
    L1,
    L2,
    Llc,
}

impl Level {
    const fn key(self) -> &'static str {
        match self {
            Level::L1 => "l1",
            Level::L2 => "l2",
            Level::Llc => "llc",
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Op {
    Read,
    Write,
    Copy,
}

impl Op {
    const fn key(self) -> &'static str {
        match self {
            Op::Read => "read",
            Op::Write => "write",
            Op::Copy => "copy",
        }
    }

    const fn kernel(self) -> &'static str {
        match self {
            Op::Read => "vmbench_k_bw_read",
            Op::Write => "vmbench_k_bw_write",
            Op::Copy => "vmbench_k_bw_copy",
        }
    }
}

fn level_bytes(ctx: &Ctx, level: Level) -> usize {
    let pick = |want_level: u32| -> Option<u64> {
        ctx.env
            .caches
            .iter()
            .filter(|c| c.level == want_level)
            .map(|c| c.size_bytes)
            .max()
    };
    match level {
        Level::L1 => pick(1).unwrap_or(32 * 1024).clamp(8 * 1024, 256 * 1024) as usize,
        Level::L2 => pick(2).unwrap_or(1024 * 1024).clamp(64 * 1024, 16 * 1024 * 1024) as usize,
        Level::Llc => pick(3).unwrap_or(32 * 1024 * 1024).clamp(1024 * 1024, 128 * 1024 * 1024) as usize,
    }
}

fn run_cache_bw(ctx: &mut Ctx, entry: usize, level: Level, op: Op) -> Result<(), &'static str> {
    let len = (level_bytes(ctx, level) & !4095).max(4096);
    let regions = if op == Op::Copy { 2 } else { 1 };
    let buf = BigBuf::new(len * regions).ok_or("mmap failed")?;
    buf.touch();
    let base = buf.as_ptr() as u64;
    ctx.add_param(entry, "level", ParamValue::Str(level.key()));
    ctx.add_param(entry, "op", ParamValue::Str(op.key()));
    ctx.add_param(entry, "working_set_bytes", ParamValue::Int(len as i64));
    let cpu = ctx.default_cpu();
    ctx.pin_self(cpu);
    ctx.add_param(entry, "cpu", ParamValue::Int(cpu as i64));
    ctx.measure(
        entry,
        ctx.cfg.target_run_ns,
        ctx.cfg.runs,
        |iters| {
            let mut units = 0u64;
            for _ in 0..iters {
                units = units.saturating_add(unsafe {
                    match op {
                        Op::Read => crate::bench::mem_bw::vmbench_k_bw_read(len as u64, base as usize),
                        Op::Write => {
                            crate::bench::mem_bw::vmbench_k_bw_write(len as u64, base as usize)
                        }
                        Op::Copy => {
                            let args = BwArgs {
                                dst: base + len as u64,
                                src1: base,
                                src2: 0,
                                len: len as u64,
                            };
                            crate::bench::mem_bw::vmbench_k_bw_copy(&args as *const BwArgs as usize)
                        }
                    }
                });
            }
            units
        },
        |ctx, e, s| {
            let secs = s.duration_ns as f64 / 1e9;
            if secs > 0.0 {
                ctx.scalar(e, "bytes_per_sec", s.units as f64 / secs);
            }
        },
    );
    Ok(())
}

macro_rules! cache_bw_bench {
    ($struct_name:ident, $id:literal, $level:expr, $op:expr) => {
        pub struct $struct_name;
        impl Benchmark for $struct_name {
            fn meta(&self) -> &'static EntryMeta {
                static M: EntryMeta = EntryMeta {
                    id: $id,
                    version: 1,
                    description: "Cache-level bandwidth",
                    imp: ImplInfo {
                        source: "src/bench/cache_bw.rs",
                        kernel: $op.kernel(),
                        algorithm: "scalar volatile 64-bit accesses over a cache-sized working set",
                        isa: BASELINE_ISA,
                    },
                    metrics: &[M_BYTES],
                };
                &M
            }
            fn run(&self, ctx: &mut Ctx, entry: usize) -> Result<(), &'static str> {
                run_cache_bw(ctx, entry, $level, $op)
            }
        }
    };
}

cache_bw_bench!(CacheL1Read, "cache.bandwidth.l1.read.v1", Level::L1, Op::Read);
cache_bw_bench!(CacheL1Write, "cache.bandwidth.l1.write.v1", Level::L1, Op::Write);
cache_bw_bench!(CacheL1Copy, "cache.bandwidth.l1.copy.v1", Level::L1, Op::Copy);
cache_bw_bench!(CacheL2Read, "cache.bandwidth.l2.read.v1", Level::L2, Op::Read);
cache_bw_bench!(CacheL2Write, "cache.bandwidth.l2.write.v1", Level::L2, Op::Write);
cache_bw_bench!(CacheL2Copy, "cache.bandwidth.l2.copy.v1", Level::L2, Op::Copy);
cache_bw_bench!(CacheLlcRead, "cache.bandwidth.llc.read.v1", Level::Llc, Op::Read);
cache_bw_bench!(CacheLlcWrite, "cache.bandwidth.llc.write.v1", Level::Llc, Op::Write);
cache_bw_bench!(CacheLlcCopy, "cache.bandwidth.llc.copy.v1", Level::Llc, Op::Copy);
