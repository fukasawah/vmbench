use crate::bench::common::*;
use crate::bench::Benchmark;
use crate::runner::{Ctx, EntryMeta, ImplInfo, ParamValue};
use crate::sys;
use crate::time;
use core::sync::atomic::{AtomicU32, Ordering};

#[repr(C, align(64))]
pub struct PingShared {
    pub seq: AtomicU32,
    pub _pad: [u8; 60],
    pub result_ns: u64,
    pub result_rounds: u64,
}

#[no_mangle]
pub extern "C" fn vmbench_w_ping_worker(arg: usize) -> i32 {
    let wctx = unsafe { &*(arg as *const WorkerCtx) };
    let mut mask = [0u8; 128];
    mask[(wctx.cpu / 8) as usize] |= 1 << (wctx.cpu % 8);
    let _ = sys::sched_setaffinity(0, &mask);
    let s = wctx.shared as *mut PingShared;
    let rounds = wctx.iters as u32;
    if wctx.index == 0 {
        let t0 = time::mono_ns();
        for r in 0..rounds {
            let want = r.wrapping_mul(2);
            while unsafe { (*s).seq.load(Ordering::Acquire) } != want {
                core::hint::spin_loop();
            }
            unsafe { (*s).seq.store(want.wrapping_add(1), Ordering::Release) };
            let want2 = want.wrapping_add(2);
            while unsafe { (*s).seq.load(Ordering::Acquire) } != want2 {
                core::hint::spin_loop();
            }
        }
        let t1 = time::mono_ns();
        unsafe {
            (*s).result_ns = t1.wrapping_sub(t0);
            (*s).result_rounds = rounds as u64;
        }
    } else {
        for r in 0..rounds {
            let want = r.wrapping_mul(2).wrapping_add(1);
            while unsafe { (*s).seq.load(Ordering::Acquire) } != want {
                core::hint::spin_loop();
            }
            unsafe { (*s).seq.store(want.wrapping_add(1), Ordering::Release) };
        }
    }
    0
}

fn ping_once(a: u32, b: u32, rounds: u64) -> Result<(u64, u64), &'static str> {
    let mut pool = WorkerPool::new(2, core::mem::size_of::<PingShared>())
        .ok_or("worker pool allocation failed")?;
    pool.set_ctx(0, a, rounds);
    pool.set_ctx(1, b, rounds);
    pool.spawn(vmbench_w_ping_worker)?;
    pool.wait_all()?;
    let s = pool.shared_at::<PingShared>(0);
    Ok(unsafe { ((*s).result_ns, (*s).result_rounds) })
}

fn calibrate_rounds(a: u32, b: u32, target_ns: u64) -> u64 {
    let rounds = 2000u64;
    let (ns, r) = match ping_once(a, b, rounds) {
        Ok(v) => v,
        Err(_) => return 100_000,
    };
    if ns == 0 || r == 0 {
        return 100_000;
    }
    let scaled = r as u128 * target_ns as u128 / ns as u128;
    if scaled > 100_000_000u128 {
        100_000_000
    } else {
        (scaled as u64).max(1000)
    }
}

/// Protocol check: a short ping-pong must complete the requested rounds.
pub fn verify_pair(a: u32, b: u32) -> Result<(), &'static str> {
    let rounds = 200u64;
    let (ns, r) = ping_once(a, b, rounds)?;
    if r != rounds || ns == 0 {
        return Err("core2core ping protocol failed");
    }
    Ok(())
}

fn run_pair(ctx: &mut Ctx, entry: usize, a: u32, b: u32, relation: &'static str) -> Result<(), &'static str> {
    if a as usize >= ctx.env.online_cpus.len() || b as usize >= ctx.env.online_cpus.len() {
        return Err("cpu index out of range");
    }
    ctx.add_param(entry, "cpu_a", ParamValue::Int(a as i64));
    ctx.add_param(entry, "cpu_b", ParamValue::Int(b as i64));
    ctx.add_param(entry, "relation", ParamValue::Str(relation));
    let rounds = calibrate_rounds(a, b, ctx.cfg.target_run_ns);
    for _ in 0..ctx.cfg.runs.max(1) {
        let (ns, r) = ping_once(a, b, rounds)?;
        ctx.begin_run(entry, ns, r);
        let rtf = r.max(1) as f64;
        ctx.scalar(entry, "ns_per_roundtrip", ns as f64 / rtf);
        ctx.scalar(entry, "ns_per_transfer", ns as f64 / (rtf * 2.0));
        let secs = ns as f64 / 1e9;
        if secs > 0.0 {
            ctx.scalar(entry, "roundtrips_per_sec", rtf / secs);
        }
        ctx.end_run(entry);
    }
    Ok(())
}

pub fn sibling_pair(ctx: &Ctx) -> Option<(u32, u32)> {
    for t in ctx.env.topology {
        if let Some(list) = t.thread_siblings {
            let mut found: Option<(u32, u32)> = None;
            for part in list.split(',') {
                let part = part.trim().as_bytes();
                let (lo, hi) = match part.iter().position(|&c| c == b'-') {
                    Some(d) => (
                        crate::util::parse_u64(&part[..d])? as u32,
                        crate::util::parse_u64(&part[d + 1..])? as u32,
                    ),
                    None => {
                        let v = crate::util::parse_u64(part)? as u32;
                        (v, v)
                    }
                };
                if lo != hi {
                    found = Some((lo, hi));
                    break;
                }
            }
            if let Some(pair) = found {
                if pair.0 != t.cpu || pair.1 != t.cpu {
                    return Some(pair);
                }
            }
        }
    }
    None
}

macro_rules! c2c_fixed {
    ($struct_name:ident, $id:literal, $a:literal, $b:literal, $rel:literal) => {
        pub struct $struct_name;
        impl Benchmark for $struct_name {
            fn meta(&self) -> &'static EntryMeta {
                static M: EntryMeta = EntryMeta {
                    id: $id,
                    version: 1,
                    description: "Core-to-core cache-line ping-pong latency",
                    imp: ImplInfo {
                        source: "src/bench/cpu_core2core.rs",
                        kernel: "vmbench_w_ping_worker",
                        algorithm: "two pinned worker processes exchange ownership of one cache line via acquire/release sequence counters",
                        isa: BASELINE_ISA,
                    },
                    metrics: &[M_RT_NS, M_TRANSFER_NS, M_RT_RATE],
                };
                &M
            }
            fn supported(&self, ctx: &Ctx) -> Result<(), &'static str> {
                if $b as usize >= ctx.env.online_cpus.len() {
                    return Err("not enough online cpus");
                }
                Ok(())
            }
            fn run(&self, ctx: &mut Ctx, entry: usize) -> Result<(), &'static str> {
                run_pair(ctx, entry, $a, $b, $rel)
            }
        }
    };
}

pub const M_RT_NS: crate::runner::MetricDef = crate::runner::MetricDef {
    key: "ns_per_roundtrip",
    unit: "ns",
    direction: crate::stats::Direction::LowerBetter,
    kind: crate::runner::MetricKind::Scalar,
};
pub const M_TRANSFER_NS: crate::runner::MetricDef = crate::runner::MetricDef {
    key: "ns_per_transfer",
    unit: "ns",
    direction: crate::stats::Direction::LowerBetter,
    kind: crate::runner::MetricKind::Scalar,
};
pub const M_RT_RATE: crate::runner::MetricDef = crate::runner::MetricDef {
    key: "roundtrips_per_sec",
    unit: "roundtrips/s",
    direction: crate::stats::Direction::HigherBetter,
    kind: crate::runner::MetricKind::Scalar,
};

c2c_fixed!(Core2Core01, "cpu.core2core.0_1.v1", 0, 1, "adjacent");
c2c_fixed!(Core2Core02, "cpu.core2core.0_2.v1", 0, 2, "same_package");

pub struct Core2CoreHalf;

impl Benchmark for Core2CoreHalf {
    fn meta(&self) -> &'static EntryMeta {
        static M: EntryMeta = EntryMeta {
            id: "cpu.core2core.0_half.v1",
            version: 1,
            description: "Core-to-core cache-line ping-pong latency (cpu0 vs cpu n/2)",
            imp: ImplInfo {
                source: "src/bench/cpu_core2core.rs",
                kernel: "vmbench_w_ping_worker",
                algorithm: "two pinned worker processes exchange ownership of one cache line via acquire/release sequence counters",
                isa: BASELINE_ISA,
            },
            metrics: &[M_RT_NS, M_TRANSFER_NS, M_RT_RATE],
        };
        &M
    }
    fn run(&self, ctx: &mut Ctx, entry: usize) -> Result<(), &'static str> {
        let n = ctx.env.online_cpus.len();
        if n < 3 {
            return Err("not enough online cpus");
        }
        run_pair(ctx, entry, 0, (n / 2) as u32, "half")
    }
}

pub struct Core2CoreLast;

impl Benchmark for Core2CoreLast {
    fn meta(&self) -> &'static EntryMeta {
        static M: EntryMeta = EntryMeta {
            id: "cpu.core2core.0_last.v1",
            version: 1,
            description: "Core-to-core cache-line ping-pong latency (cpu0 vs last cpu)",
            imp: ImplInfo {
                source: "src/bench/cpu_core2core.rs",
                kernel: "vmbench_w_ping_worker",
                algorithm: "two pinned worker processes exchange ownership of one cache line via acquire/release sequence counters",
                isa: BASELINE_ISA,
            },
            metrics: &[M_RT_NS, M_TRANSFER_NS, M_RT_RATE],
        };
        &M
    }
    fn run(&self, ctx: &mut Ctx, entry: usize) -> Result<(), &'static str> {
        let n = ctx.env.online_cpus.len();
        if n < 2 {
            return Err("not enough online cpus");
        }
        run_pair(ctx, entry, 0, (n - 1) as u32, "last")
    }
}

pub struct Core2CoreSiblings;

impl Benchmark for Core2CoreSiblings {
    fn meta(&self) -> &'static EntryMeta {
        static M: EntryMeta = EntryMeta {
            id: "cpu.core2core.smt_siblings.v1",
            version: 1,
            description: "Core-to-core cache-line ping-pong latency (SMT siblings)",
            imp: ImplInfo {
                source: "src/bench/cpu_core2core.rs",
                kernel: "vmbench_w_ping_worker",
                algorithm: "two pinned worker processes exchange ownership of one cache line via acquire/release sequence counters",
                isa: BASELINE_ISA,
            },
            metrics: &[M_RT_NS, M_TRANSFER_NS, M_RT_RATE],
        };
        &M
    }
    fn supported(&self, ctx: &Ctx) -> Result<(), &'static str> {
        match sibling_pair(ctx) {
            Some(_) => Ok(()),
            None => Err("SMT sibling topology unavailable"),
        }
    }
    fn run(&self, ctx: &mut Ctx, entry: usize) -> Result<(), &'static str> {
        let (a, b) = sibling_pair(ctx).ok_or("SMT sibling topology unavailable")?;
        run_pair(ctx, entry, a, b, "smt_sibling")
    }
}
