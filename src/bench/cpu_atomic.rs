use crate::bench::common::*;
use crate::bench::Benchmark;
use crate::runner::{Ctx, EntryMeta, ImplInfo, ParamValue};
use crate::sys;
use crate::time;
use core::sync::atomic::{AtomicU32, AtomicU64, Ordering};

#[inline(never)]
#[no_mangle]
pub unsafe extern "C" fn vmbench_k_atomic_add(iters: u64, ptr: usize) -> u64 {
    let a = &*(ptr as *const AtomicU64);
    let mut i = 0u64;
    while i < iters {
        a.fetch_add(1, Ordering::Relaxed);
        i += 1;
    }
    core::hint::black_box(a.load(Ordering::Relaxed))
}

#[inline(never)]
#[no_mangle]
pub unsafe extern "C" fn vmbench_k_atomic_xchg(iters: u64, ptr: usize) -> u64 {
    let a = &*(ptr as *const AtomicU64);
    let mut i = 0u64;
    while i < iters {
        a.swap(i, Ordering::Relaxed);
        i += 1;
    }
    core::hint::black_box(a.load(Ordering::Relaxed))
}

#[inline(never)]
#[no_mangle]
pub unsafe extern "C" fn vmbench_k_atomic_cas(iters: u64, ptr: usize) -> u64 {
    let a = &*(ptr as *const AtomicU64);
    let mut cur = a.load(Ordering::Relaxed);
    let mut i = 0u64;
    while i < iters {
        match a.compare_exchange_weak(cur, cur.wrapping_add(1), Ordering::Relaxed, Ordering::Relaxed)
        {
            Ok(_) => {
                cur = cur.wrapping_add(1);
                i += 1;
            }
            Err(v) => cur = v,
        }
    }
    core::hint::black_box(cur)
}

pub type AtomicKernel = unsafe extern "C" fn(u64, usize) -> u64;

#[used]
static ATOMIC_KERNELS: [AtomicKernel; 3] = [
    vmbench_k_atomic_add,
    vmbench_k_atomic_xchg,
    vmbench_k_atomic_cas,
];

static ATOMIC_NAMES: [&str; 3] = [
    "vmbench_k_atomic_add",
    "vmbench_k_atomic_xchg",
    "vmbench_k_atomic_cas",
];

#[repr(C, align(64))]
pub struct AtomicLine {
    pub v: AtomicU64,
    pub pad: [u8; 56],
}

#[repr(C)]
pub struct AtomicShared {
    pub line: AtomicLine,
    pub units: [u64; MAX_WORKERS],
    pub dur_ns: [u64; MAX_WORKERS],
    pub done: AtomicU32,
    pub pad2: [u8; 60],
}

extern "C" fn atomic_worker(arg: usize) -> i32 {
    let wctx = unsafe { &*(arg as *const WorkerCtx) };
    let mut mask = [0u8; 128];
    mask[(wctx.cpu / 8) as usize] |= 1 << (wctx.cpu % 8);
    let _ = sys::sched_setaffinity(0, &mask);
    let shared = wctx.shared as *mut AtomicShared;
    let ptr = unsafe { &(*shared).line.v as *const AtomicU64 as usize };
    let idx = wctx.index as usize;
    let kernel_idx = wctx.arg as usize;
    let target = wctx.arg2.max(1_000_000);
    let f = unsafe { core::ptr::read_volatile(&ATOMIC_KERNELS[kernel_idx]) };
    // Probe the contended cost from inside the worker, then choose a chunk
    // that lasts roughly target/8. A chunk calibrated single-threaded can be
    // tens of times too large under heavy cache-line contention.
    let goal = (target / 8).clamp(2_000_000, 20_000_000);
    let chunk = probe_chunk(goal, 1024, |n| {
        let _ = unsafe { f(n, ptr) };
        n
    });
    // Time-budgeted loop: contention changes the per-op cost, so count ops
    // within a bounded wall-clock window instead of using a fixed count.
    let t0 = time::mono_ns();
    let deadline = time::Deadline::new(target);
    let mut ops = 0u64;
    loop {
        let _ = unsafe { f(chunk, ptr) };
        ops = ops.saturating_add(chunk);
        if deadline.expired() {
            break;
        }
    }
    let dur = time::mono_ns().wrapping_sub(t0).max(1);
    unsafe {
        (*shared).units[idx] = ops;
        (*shared).dur_ns[idx] = dur;
        (*shared).done.fetch_add(1, Ordering::Relaxed);
    }
    0
}

/// Known-answer check for the three atomic kernels on a private line.
pub fn verify_kernels() -> Result<(), &'static str> {
    let n = 1000u64;
    let mut line = AtomicLine {
        v: AtomicU64::new(0),
        pad: [0; 56],
    };
    let ptr = &mut line.v as *mut AtomicU64 as usize;
    for (idx, expected) in [(0usize, n), (1, n - 1), (2, n)] {
        let f = unsafe { core::ptr::read_volatile(&ATOMIC_KERNELS[idx]) };
        line.v.store(0, Ordering::Relaxed);
        let r = unsafe { f(n, ptr) };
        if r != expected {
            return Err(match idx {
                0 => "atomic fetch_add final value mismatch",
                1 => "atomic swap final value mismatch",
                _ => "atomic compare_exchange final value mismatch",
            });
        }
    }
    Ok(())
}

/// Protocol check with contention: for fetch_add and CAS the final line value
/// must equal the total number of successful operations.
pub fn verify_protocol(ctx: &Ctx, kernel_idx: usize, n: usize) -> Result<(), &'static str> {
    let mut pool = WorkerPool::new(n, core::mem::size_of::<AtomicShared>())
        .ok_or("worker pool allocation failed")?;
    for i in 0..n {
        pool.set_ctx(i, ctx.cpu_at(i), 0);
        pool.set_arg(i, kernel_idx as u64);
        pool.set_arg2(i, 5_000_000);
    }
    pool.spawn(atomic_worker)?;
    pool.wait_all()?;
    let shared = pool.shared_at::<AtomicShared>(0);
    let mut total = 0u64;
    for i in 0..n {
        total = total.saturating_add(unsafe { (*shared).units[i] });
    }
    if total == 0 {
        return Err("atomic workers produced no operations");
    }
    if kernel_idx == 0 || kernel_idx == 2 {
        let v = unsafe { (*shared).line.v.load(Ordering::Relaxed) };
        if v != total {
            return Err("atomic contended final value != total operations");
        }
    }
    Ok(())
}

fn run_atomic(ctx: &mut Ctx, entry: usize, kernel_idx: usize, n: usize) -> Result<(), &'static str> {
    if n == 0 || n > ctx.env.online_cpus.len() {
        return Err("not enough online cpus");
    }
    ctx.add_param(entry, "threads", ParamValue::Int(n as i64));
    for _ in 0..ctx.cfg.runs.max(1) {
        let mut pool = WorkerPool::new(n, core::mem::size_of::<AtomicShared>())
            .ok_or("worker pool allocation failed")?;
        for i in 0..n {
            pool.set_ctx(i, ctx.cpu_at(i), 0);
            pool.set_arg(i, kernel_idx as u64);
            pool.set_arg2(i, ctx.cfg.target_run_ns);
        }
        pool.spawn(atomic_worker)?;
        pool.wait_all()?;
        let shared = pool.shared_at::<AtomicShared>(0);
        let mut total = 0u64;
        let mut max_dur = 1u64;
        for i in 0..n {
            total = total.saturating_add(unsafe { (*shared).units[i] });
            let d = unsafe { (*shared).dur_ns[i] };
            if d > max_dur {
                max_dur = d;
            }
        }
        ctx.begin_run(entry, max_dur, total);
        let secs = max_dur as f64 / 1e9;
        if secs > 0.0 {
            ctx.scalar(entry, "ops_per_sec", total as f64 / secs);
        }
        ctx.scalar(entry, "ns_per_op", max_dur as f64 / total.max(1) as f64);
        ctx.end_run(entry);
    }
    Ok(())
}

macro_rules! atomic_bench {
    ($struct_name:ident, $id:literal, $kidx:literal, $n:expr, $op:literal) => {
        pub struct $struct_name;
        impl Benchmark for $struct_name {
            fn meta(&self) -> &'static EntryMeta {
                static M: EntryMeta = EntryMeta {
                    id: $id,
                    version: 1,
                    description: "Atomic operation throughput",
                    imp: ImplInfo {
                        source: "src/bench/cpu_atomic.rs",
                        kernel: ATOMIC_NAMES[$kidx],
                        algorithm: "fetch_add / swap / compare_exchange_weak loop on a 64-byte aligned shared cache line (Relaxed)",
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
                ctx.add_param(entry, "op", ParamValue::Str($op));
                let n = if $n == 0 { ctx.env.online_cpus.len() } else { $n };
                run_atomic(ctx, entry, $kidx, n)
            }
        }
    };
}

atomic_bench!(AtomicAdd1, "cpu.atomic.fetch_add.1.v1", 0, 1, "fetch_add");
atomic_bench!(AtomicXchg1, "cpu.atomic.swap.1.v1", 1, 1, "swap");
atomic_bench!(AtomicCas1, "cpu.atomic.cas.1.v1", 2, 1, "compare_exchange");
atomic_bench!(AtomicAdd2, "cpu.atomic.fetch_add.2.v1", 0, 2, "fetch_add");
atomic_bench!(AtomicAddAll, "cpu.atomic.fetch_add.all.v1", 0, 0, "fetch_add");
atomic_bench!(AtomicCas2, "cpu.atomic.cas.2.v1", 2, 2, "compare_exchange");
atomic_bench!(AtomicCasAll, "cpu.atomic.cas.all.v1", 2, 0, "compare_exchange");
