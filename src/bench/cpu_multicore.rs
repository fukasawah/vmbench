use crate::bench::common::*;
use crate::bench::cpu_mix::vmbench_k_mix_iter;
use crate::bench::Benchmark;
use crate::runner::{Ctx, EntryMeta, ImplInfo, ParamValue};
use crate::sys;
use crate::time;
use core::sync::atomic::{AtomicU32, Ordering};

#[repr(C)]
pub struct McShared {
    pub units: [u64; MAX_WORKERS],
    pub dur_ns: [u64; MAX_WORKERS],
    pub done: AtomicU32,
    pub _pad: [u8; 60],
}

extern "C" fn mc_worker(arg: usize) -> i32 {
    let wctx = unsafe { &*(arg as *const WorkerCtx) };
    let mut mask = [0u8; 128];
    mask[(wctx.cpu / 8) as usize] |= 1 << (wctx.cpu % 8);
    let _ = sys::sched_setaffinity(0, &mask);
    let shared = wctx.shared as *mut McShared;
    let target = wctx.arg2.max(1_000_000);
    let seed = 0x9e37_79b9_7f4a_7c15u64
        .wrapping_mul(wctx.index as u64 + 1)
        .wrapping_add(wctx.iters);
    let goal = (target / 8).clamp(2_000_000, 20_000_000);
    let chunk = probe_chunk(goal, 4096, |n| {
        let _ = unsafe { vmbench_k_mix_iter(n, seed) };
        n
    });
    // Time-budgeted: each worker runs for the same wall-clock window.
    let t0 = time::mono_ns();
    let deadline = time::Deadline::new(target);
    let mut units = 0u64;
    loop {
        let _ = unsafe { vmbench_k_mix_iter(chunk, seed) };
        units = units.saturating_add(chunk);
        if deadline.expired() {
            break;
        }
    }
    let dur = time::mono_ns().wrapping_sub(t0).max(1);
    unsafe {
        (*shared).units[wctx.index as usize] = units;
        (*shared).dur_ns[wctx.index as usize] = dur;
        (*shared).done.fetch_add(1, Ordering::Relaxed);
    }
    0
}

/// Protocol check: one worker must produce a positive, process-exit-clean
/// result through the same clone/shared-memory path used by the measurement.
pub fn verify_protocol(ctx: &Ctx) -> Result<(), &'static str> {
    let mut pool = WorkerPool::new(1, core::mem::size_of::<McShared>())
        .ok_or("worker pool allocation failed")?;
    pool.set_ctx(0, ctx.cpu_at(0), 0);
    pool.set_arg2(0, 5_000_000);
    pool.spawn(mc_worker)?;
    pool.wait_all()?;
    let shared = pool.shared_at::<McShared>(0);
    if unsafe { (*shared).units[0] } == 0 {
        return Err("multicore worker produced no work");
    }
    Ok(())
}

fn run_multicore(ctx: &mut Ctx, entry: usize, n: usize) -> Result<(), &'static str> {
    if n == 0 || n > ctx.env.online_cpus.len() {
        return Err("not enough online cpus");
    }
    ctx.add_param(entry, "threads", ParamValue::Int(n as i64));
    ctx.add_param(entry, "kernel", ParamValue::Str("cpu.mix"));
    let target = ctx.cfg.target_run_ns;
    for _ in 0..ctx.cfg.runs.max(1) {
        let mut pool = WorkerPool::new(n, core::mem::size_of::<McShared>())
            .ok_or("worker pool allocation failed")?;
        for i in 0..n {
            pool.set_ctx(i, ctx.cpu_at(i), 0);
            pool.set_arg2(i, ctx.cfg.target_run_ns);
        }
        let t0 = time::mono_ns();
        pool.spawn(mc_worker)?;
        pool.wait_all()?;
        let wall = time::mono_ns().wrapping_sub(t0);
        let shared = pool.shared_at::<McShared>(0);
        let mut total_units = 0u64;
        let mut max_dur = 1u64;
        for i in 0..n {
            let u = unsafe { (*shared).units[i] };
            let d = unsafe { (*shared).dur_ns[i] };
            total_units = total_units.saturating_add(u);
            if d > max_dur {
                max_dur = d;
            }
        }
        let dur = max_dur.min(wall.max(1));
        ctx.begin_run(entry, dur, total_units);
        let secs = dur as f64 / 1e9;
        if secs > 0.0 {
            ctx.scalar(entry, "ops_per_sec", total_units as f64 / secs);
        }
        ctx.scalar(entry, "ns_per_op", dur as f64 / total_units.max(1) as f64);
        ctx.end_run(entry);
        if target == 0 {
            break;
        }
    }
    Ok(())
}

macro_rules! mc_bench {
    ($struct_name:ident, $id:literal, $n:expr) => {
        pub struct $struct_name;
        impl Benchmark for $struct_name {
            fn meta(&self) -> &'static EntryMeta {
                static M: EntryMeta = EntryMeta {
                    id: $id,
                    version: 1,
                    description: "Multi-core scaling of the fixed mix kernel",
                    imp: ImplInfo {
                        source: "src/bench/cpu_multicore.rs",
                        kernel: "vmbench_k_mix_iter",
                        algorithm: "same fixed mix kernel per worker process, pinned to distinct CPUs; aggregate ops/s",
                        isa: BASELINE_ISA,
                    },
                    metrics: &[M_OPS, M_NS_OP],
                };
                &M
            }
            fn supported(&self, ctx: &Ctx) -> Result<(), &'static str> {
                if $n > ctx.env.online_cpus.len() {
                    return Err("not enough online cpus");
                }
                Ok(())
            }
            fn run(&self, ctx: &mut Ctx, entry: usize) -> Result<(), &'static str> {
                run_multicore(ctx, entry, $n)
            }
        }
    };
}

mc_bench!(MultiCore1, "cpu.multicore.1.v1", 1);
mc_bench!(MultiCore2, "cpu.multicore.2.v1", 2);
mc_bench!(MultiCore4, "cpu.multicore.4.v1", 4);
mc_bench!(MultiCore8, "cpu.multicore.8.v1", 8);

pub struct MultiCoreAll;

impl Benchmark for MultiCoreAll {
    fn meta(&self) -> &'static EntryMeta {
        static M: EntryMeta = EntryMeta {
            id: "cpu.multicore.all.v1",
            version: 1,
            description: "Multi-core scaling of the fixed mix kernel (all online CPUs)",
            imp: ImplInfo {
                source: "src/bench/cpu_multicore.rs",
                kernel: "vmbench_k_mix_iter",
                algorithm: "same fixed mix kernel per worker process, pinned to distinct CPUs; aggregate ops/s",
                isa: BASELINE_ISA,
            },
            metrics: &[M_OPS, M_NS_OP],
        };
        &M
    }

    fn supported(&self, ctx: &Ctx) -> Result<(), &'static str> {
        if ctx.env.online_cpus.is_empty() {
            return Err("not enough online cpus");
        }
        Ok(())
    }

    fn run(&self, ctx: &mut Ctx, entry: usize) -> Result<(), &'static str> {
        let n = ctx.env.online_cpus.len();
        run_multicore(ctx, entry, n)
    }
}
