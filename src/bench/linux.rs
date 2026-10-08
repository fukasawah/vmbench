use crate::bench::common::*;
use crate::bench::Benchmark;
use crate::runner::{Ctx, EntryMeta, ImplInfo, MetricDef, MetricKind, ParamValue};
use crate::stats::Direction;
use crate::sys;
use crate::time;
use core::sync::atomic::{AtomicU32, AtomicU64, Ordering};

// ---------------------------------------------------------------------------
// Linux / scheduler diagnostics. These are not CPU scores; they characterise
// the syscall boundary, scheduling and hypervisor behaviour.
// ---------------------------------------------------------------------------

#[inline(never)]
#[no_mangle]
pub unsafe extern "C" fn vmbench_k_syscall_getpid(iters: u64, seed: u64) -> u64 {
    let mut x = seed;
    let mut i = 0u64;
    while i < iters {
        x = x.wrapping_add(sys::getpid() as u64);
        i += 1;
    }
    core::hint::black_box(x)
}

pub const M_NS_SYSCALL: MetricDef = MetricDef {
    key: "ns_per_syscall",
    unit: "ns",
    direction: Direction::LowerBetter,
    kind: MetricKind::Scalar,
};
pub const M_CYC_SYSCALL: MetricDef = MetricDef {
    key: "cycles_per_syscall",
    unit: "cycles",
    direction: Direction::LowerBetter,
    kind: MetricKind::Scalar,
};

pub struct SyscallGetpid;

impl Benchmark for SyscallGetpid {
    fn meta(&self) -> &'static EntryMeta {
        static M: EntryMeta = EntryMeta {
            id: "linux.syscall.getpid.v1",
            version: 1,
            description: "getpid() syscall round-trip latency",
            imp: ImplInfo {
                source: "src/bench/linux.rs",
                kernel: "vmbench_k_syscall_getpid",
                algorithm: "tight loop calling getpid(2) through the raw syscall instruction",
                isa: BASELINE_ISA,
            },
            metrics: &[M_NS_SYSCALL, M_CYC_SYSCALL],
        };
        &M
    }

    fn run(&self, ctx: &mut Ctx, entry: usize) -> Result<(), &'static str> {
        let cpu = ctx.default_cpu();
        ctx.pin_self(cpu);
        ctx.add_param(entry, "cpu", ParamValue::Int(cpu as i64));
        let seed = 0x0123_4567_89ab_cdefu64;
        ctx.measure(
            entry,
            ctx.cfg.target_run_ns,
            ctx.cfg.runs,
            |iters| {
                let _ = unsafe { vmbench_k_syscall_getpid(iters, seed) };
                iters
            },
            |ctx, e, s| {
                let n = s.units.max(1) as f64;
                ctx.scalar(e, "ns_per_syscall", s.duration_ns as f64 / n);
                if time::cycles_hz() > 0 {
                    ctx.scalar(e, "cycles_per_syscall", s.cycles as f64 / n);
                }
            },
        );
        Ok(())
    }
}

/// Expected-value check: the kernel sums getpid() over `n` iterations.
pub fn verify_syscall() -> Result<(), &'static str> {
    let n = 100u64;
    let pid = sys::getpid() as u64;
    let r = unsafe { vmbench_k_syscall_getpid(n, 0) };
    if r != n.wrapping_mul(pid) {
        return Err("syscall getpid sum mismatch");
    }
    Ok(())
}

/// Side-effect check: `n` intervals written, each plausibly a 1 ms sleep.
pub fn verify_jitter() -> Result<(), &'static str> {
    let mut vals = [0u64; 8];
    let n = unsafe { vmbench_k_jitter_loop(8, vals.as_mut_ptr()) };
    if n != 8 {
        return Err("jitter loop count mismatch");
    }
    for v in vals.iter() {
        if *v < 500_000 || *v > 5_000_000_000 {
            return Err("jitter interval out of range");
        }
    }
    Ok(())
}

/// Expected-value check: one page touched per stride.
pub fn verify_first_touch() -> Result<(), &'static str> {
    let buf = BigBuf::new(64 * 1024).ok_or("mmap failed")?;
    let pages = unsafe { vmbench_k_first_touch(buf.as_ptr() as usize, 64 * 1024, 4096) };
    if pages != 16 {
        return Err("first_touch page count mismatch");
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Futex ping-pong (context switch cost)
// ---------------------------------------------------------------------------

#[repr(C, align(64))]
pub struct FutexShared {
    pub seq: AtomicU32,
    pub _pad: [u8; 60],
    pub result_ns: u64,
    pub result_rounds: u64,
}

#[no_mangle]
pub extern "C" fn vmbench_w_futex_ping(arg: usize) -> i32 {
    let wctx = unsafe { &*(arg as *const WorkerCtx) };
    let mut mask = [0u8; 128];
    mask[(wctx.cpu / 8) as usize] |= 1 << (wctx.cpu % 8);
    let _ = sys::sched_setaffinity(0, &mask);
    let s = wctx.shared as *mut FutexShared;
    let rounds = wctx.iters as u32;
    let initiator = wctx.index == 0;
    let t0 = if initiator { time::mono_ns() } else { 0 };
    for r in 0..rounds {
        let want = if initiator {
            r.wrapping_mul(2)
        } else {
            r.wrapping_mul(2).wrapping_add(1)
        };
        loop {
            let v = unsafe { (*s).seq.load(Ordering::Acquire) };
            if v == want {
                break;
            }
            let _ = sys::futex_wait(unsafe { &(*s).seq }, v);
        }
        unsafe { (*s).seq.store(want.wrapping_add(1), Ordering::Release) };
        let _ = sys::futex_wake(unsafe { &(*s).seq }, 1);
    }
    if initiator {
        let t1 = time::mono_ns();
        unsafe {
            (*s).result_ns = t1.wrapping_sub(t0);
            (*s).result_rounds = rounds as u64;
        }
    }
    0
}

fn futex_ping_once(a: u32, b: u32, rounds: u64) -> Result<(u64, u64), &'static str> {
    let mut pool = WorkerPool::new(2, core::mem::size_of::<FutexShared>())
        .ok_or("worker pool allocation failed")?;
    pool.set_ctx(0, a, rounds);
    pool.set_ctx(1, b, rounds);
    pool.spawn(vmbench_w_futex_ping)?;
    pool.wait_all()?;
    let s = pool.shared_at::<FutexShared>(0);
    Ok(unsafe { ((*s).result_ns, (*s).result_rounds) })
}

pub struct ContextSwitchFutex;

impl Benchmark for ContextSwitchFutex {
    fn meta(&self) -> &'static EntryMeta {
        static M: EntryMeta = EntryMeta {
            id: "linux.ctxswitch.futex.v1",
            version: 1,
            description: "Context switch cost via futex ping-pong between two pinned processes",
            imp: ImplInfo {
                source: "src/bench/linux.rs",
                kernel: "vmbench_w_futex_ping",
                algorithm: "two processes alternate ownership of a cache line using FUTEX_WAIT/FUTEX_WAKE; ns per switch = roundtrip/2",
                isa: BASELINE_ISA,
            },
            metrics: &[M_CS_SWITCH, M_CS_RT, M_CS_RATE],
        };
        &M
    }

    fn supported(&self, ctx: &Ctx) -> Result<(), &'static str> {
        if ctx.env.online_cpus.len() >= 2 {
            Ok(())
        } else {
            Err("not enough online cpus")
        }
    }

    fn run(&self, ctx: &mut Ctx, entry: usize) -> Result<(), &'static str> {
        let a = ctx.cpu_at(0);
        let b = ctx.cpu_at(1);
        ctx.add_param(entry, "cpu_a", ParamValue::Int(a as i64));
        ctx.add_param(entry, "cpu_b", ParamValue::Int(b as i64));
        // calibrate rounds with one probe run
        let (ns, r) = futex_ping_once(a, b, 500)?;
        let target = ctx.cfg.target_run_ns;
        let rounds = if ns == 0 {
            2000
        } else {
            ((r as u128 * target as u128 / ns as u128) as u64).clamp(100, 2_000_000)
        };
        for _ in 0..ctx.cfg.runs.max(1) {
            let (ns, r) = futex_ping_once(a, b, rounds)?;
            ctx.begin_run(entry, ns, r);
            let rt = r.max(1) as f64;
            ctx.scalar(entry, "ns_per_roundtrip", ns as f64 / rt);
            ctx.scalar(entry, "ns_per_switch", ns as f64 / (rt * 2.0));
            let secs = ns as f64 / 1e9;
            if secs > 0.0 {
                ctx.scalar(entry, "roundtrips_per_sec", rt / secs);
            }
            ctx.end_run(entry);
        }
        Ok(())
    }
}

pub const M_CS_SWITCH: MetricDef = MetricDef {
    key: "ns_per_switch",
    unit: "ns",
    direction: Direction::LowerBetter,
    kind: MetricKind::Scalar,
};
pub const M_CS_RT: MetricDef = MetricDef {
    key: "ns_per_roundtrip",
    unit: "ns",
    direction: Direction::LowerBetter,
    kind: MetricKind::Scalar,
};
pub const M_CS_RATE: MetricDef = MetricDef {
    key: "roundtrips_per_sec",
    unit: "roundtrips/s",
    direction: Direction::HigherBetter,
    kind: MetricKind::Scalar,
};

// ---------------------------------------------------------------------------
// Wakeup latency
// ---------------------------------------------------------------------------

#[repr(C, align(64))]
pub struct WakeShared {
    pub ack: AtomicU32,
    pub _pad: [u8; 60],
    pub wake_tsc: AtomicU64,
    pub samples: [u64; 512],
    pub n: u64,
}

#[no_mangle]
pub extern "C" fn vmbench_w_wake(arg: usize) -> i32 {
    let wctx = unsafe { &*(arg as *const WorkerCtx) };
    let mut mask = [0u8; 128];
    mask[(wctx.cpu / 8) as usize] |= 1 << (wctx.cpu % 8);
    let _ = sys::sched_setaffinity(0, &mask);
    let s = wctx.shared as *mut WakeShared;
    let n = (wctx.iters as usize).min(512);
    for i in 0..n {
        let want = i as u32 + 1;
        loop {
            let v = unsafe { (*s).ack.load(Ordering::Acquire) };
            if v >= want {
                break;
            }
            let _ = sys::futex_wait(unsafe { &(*s).ack }, v);
        }
        let t = time::cycles();
        let w = unsafe { (*s).wake_tsc.load(Ordering::Acquire) };
        unsafe {
            (*s).samples[i] = t.wrapping_sub(w);
        }
    }
    unsafe { (*s).n = n as u64 };
    0
}

pub struct WakeupFutex;

impl Benchmark for WakeupFutex {
    fn meta(&self) -> &'static EntryMeta {
        static M: EntryMeta = EntryMeta {
            id: "linux.wakeup.futex.v1",
            version: 1,
            description: "Futex wakeup latency (wake timestamp to woken process)",
            imp: ImplInfo {
                source: "src/bench/linux.rs",
                kernel: "vmbench_w_wake",
                algorithm: "one process sleeps in FUTEX_WAIT; the other timestamps immediately before FUTEX_WAKE; delta measured on the woken core",
                isa: BASELINE_ISA,
            },
            metrics: &[M_WAKE_MIN, M_WAKE_MED, M_WAKE_P99, M_WAKE_MAX],
        };
        &M
    }

    fn supported(&self, ctx: &Ctx) -> Result<(), &'static str> {
        if ctx.env.online_cpus.len() < 2 {
            return Err("not enough online cpus");
        }
        if time::cycles_hz() == 0 {
            return Err("cycle counter unavailable");
        }
        Ok(())
    }

    fn run(&self, ctx: &mut Ctx, entry: usize) -> Result<(), &'static str> {
        let n = 256u64;
        // One sleeper; this thread is the waker. A second worker would compete
        // for the same futex wakeups and could never finish.
        let mut pool = WorkerPool::new(1, core::mem::size_of::<WakeShared>())
            .ok_or("worker pool allocation failed")?;
        pool.set_ctx(0, ctx.cpu_at(0), n);
        pool.spawn(vmbench_w_wake)?;
        let waker_cpu = ctx.cpu_at(1);
        ctx.pin_self(waker_cpu);
        // give the worker time to park on the first futex_wait
        time::sleep_ns(200_000);
        let s = pool.shared_at::<WakeShared>(0);
        let t0 = time::mono_ns();
        for i in 0..n {
            time::sleep_ns(100_000);
            unsafe {
                (*s).wake_tsc.store(time::cycles(), Ordering::Release);
                (*s).ack.store(i as u32 + 1, Ordering::Release);
            }
            let _ = sys::futex_wake(unsafe { &(*s).ack }, 1);
        }
        pool.wait_all()?;
        let total_ns = time::mono_ns().wrapping_sub(t0);
        let mut vals = [0u64; 512];
        let cnt = unsafe { (*s).n as usize }.min(512);
        for (i, v) in vals.iter_mut().enumerate().take(cnt) {
            *v = unsafe { (*s).samples[i] };
        }
        let vals = &mut vals[..cnt];
        vals.sort_unstable();
        let hz = time::cycles_hz().max(1) as f64;
        let to_ns = |c: u64| c as f64 * 1e9 / hz;
        let pick = |p: f64| -> f64 {
            if cnt == 0 {
                0.0
            } else {
                let idx = (((cnt as f64 - 1.0) * p) + 0.5) as usize;
                to_ns(vals[idx.min(cnt - 1)])
            }
        };
        ctx.begin_run(entry, total_ns, cnt as u64);
        ctx.scalar(entry, "wakeup_ns_min", to_ns(vals[0]));
        ctx.scalar(entry, "wakeup_ns_median", pick(0.5));
        ctx.scalar(entry, "wakeup_ns_p99", pick(0.99));
        ctx.scalar(entry, "wakeup_ns_max", to_ns(vals[cnt - 1]));
        ctx.end_run(entry);
        Ok(())
    }
}

pub const M_WAKE_MIN: MetricDef = MetricDef {
    key: "wakeup_ns_min",
    unit: "ns",
    direction: Direction::LowerBetter,
    kind: MetricKind::Scalar,
};
pub const M_WAKE_MED: MetricDef = MetricDef {
    key: "wakeup_ns_median",
    unit: "ns",
    direction: Direction::LowerBetter,
    kind: MetricKind::Scalar,
};
pub const M_WAKE_P99: MetricDef = MetricDef {
    key: "wakeup_ns_p99",
    unit: "ns",
    direction: Direction::LowerBetter,
    kind: MetricKind::Scalar,
};
pub const M_WAKE_MAX: MetricDef = MetricDef {
    key: "wakeup_ns_max",
    unit: "ns",
    direction: Direction::LowerBetter,
    kind: MetricKind::Scalar,
};

// ---------------------------------------------------------------------------
// Scheduler jitter (nanosleep interval accuracy)
// ---------------------------------------------------------------------------

#[inline(never)]
#[no_mangle]
pub unsafe extern "C" fn vmbench_k_jitter_loop(n: u64, out: *mut u64) -> u64 {
    let mut prev = time::mono_ns();
    let mut i = 0u64;
    while i < n {
        time::sleep_ns(1_000_000);
        let now = time::mono_ns();
        core::ptr::write_volatile(out.add(i as usize), now.wrapping_sub(prev));
        prev = now;
        i += 1;
    }
    n
}

pub struct SchedulerJitter;

impl Benchmark for SchedulerJitter {
    fn meta(&self) -> &'static EntryMeta {
        static M: EntryMeta = EntryMeta {
            id: "linux.scheduler.jitter.v1",
            version: 1,
            description: "Sleep/wake interval jitter on a pinned CPU",
            imp: ImplInfo {
                source: "src/bench/linux.rs",
                kernel: "vmbench_k_jitter_loop",
                algorithm: "500 x nanosleep(1 ms); distribution of actual intervals",
                isa: BASELINE_ISA,
            },
            metrics: &[M_JIT_MED, M_JIT_P99, M_JIT_MAX],
        };
        &M
    }

    fn run(&self, ctx: &mut Ctx, entry: usize) -> Result<(), &'static str> {
        let cpu = ctx.default_cpu();
        ctx.pin_self(cpu);
        ctx.add_param(entry, "cpu", ParamValue::Int(cpu as i64));
        ctx.add_param(entry, "sleep_ns", ParamValue::Int(1_000_000));
        let n = 500usize;
        let mut vals = [0u64; 500];
        let _ = unsafe { vmbench_k_jitter_loop(n as u64, vals.as_mut_ptr()) };
        let mut sorted = vals;
        sorted.sort_unstable();
        ctx.begin_run(entry, 0, n as u64);
        ctx.scalar(entry, "jitter_ns_median", sorted[n / 2] as f64);
        ctx.scalar(entry, "jitter_ns_p99", sorted[(n * 99) / 100] as f64);
        ctx.scalar(entry, "jitter_ns_max", sorted[n - 1] as f64);
        ctx.end_run(entry);
        Ok(())
    }
}

pub const M_JIT_MED: MetricDef = MetricDef {
    key: "jitter_ns_median",
    unit: "ns",
    direction: Direction::LowerBetter,
    kind: MetricKind::Scalar,
};
pub const M_JIT_P99: MetricDef = MetricDef {
    key: "jitter_ns_p99",
    unit: "ns",
    direction: Direction::LowerBetter,
    kind: MetricKind::Scalar,
};
pub const M_JIT_MAX: MetricDef = MetricDef {
    key: "jitter_ns_max",
    unit: "ns",
    direction: Direction::LowerBetter,
    kind: MetricKind::Scalar,
};

// ---------------------------------------------------------------------------
// Minor page fault cost
// ---------------------------------------------------------------------------

#[inline(never)]
#[no_mangle]
pub unsafe extern "C" fn vmbench_k_first_touch(base: usize, size: u64, stride: u64) -> u64 {
    let p = base as *mut u8;
    let mut off = 0u64;
    let mut pages = 0u64;
    while off < size {
        core::ptr::write_volatile(p.add(off as usize) as *mut u64, off);
        off += stride;
        pages += 1;
    }
    pages
}

pub struct PageFaultMinor;

impl Benchmark for PageFaultMinor {
    fn meta(&self) -> &'static EntryMeta {
        static M: EntryMeta = EntryMeta {
            id: "linux.pagefault.minor.v1",
            version: 1,
            description: "Minor page fault cost (anonymous first touch)",
            imp: ImplInfo {
                source: "src/bench/linux.rs",
                kernel: "vmbench_k_first_touch",
                algorithm: "3 passes over 128 MiB with MADV_DONTNEED between passes; one 8-byte store per 4 KiB page",
                isa: BASELINE_ISA,
            },
            metrics: &[M_PF_NS, M_PF_CYC],
        };
        &M
    }

    fn run(&self, ctx: &mut Ctx, entry: usize) -> Result<(), &'static str> {
        let size = 128 * 1024 * 1024usize;
        let buf = BigBuf::new(size).ok_or("mmap failed")?;
        let pages = size / 4096;
        let cpu = ctx.default_cpu();
        ctx.pin_self(cpu);
        ctx.add_param(entry, "working_set_bytes", ParamValue::Int(size as i64));
        ctx.add_param(entry, "page_size", ParamValue::Int(4096));
        ctx.add_param(entry, "cpu", ParamValue::Int(cpu as i64));
        let mut best_ns = u64::MAX;
        let mut best_cyc = u64::MAX;
        for pass in 0..3 {
            if pass > 0 {
                let _ = sys::madvise(buf.as_ptr(), buf.len, sys::MADV_DONTNEED);
            }
            let t0 = time::mono_ns();
            let c0 = time::cycles();
            let _ = unsafe { vmbench_k_first_touch(buf.as_ptr() as usize, size as u64, 4096) };
            let d = time::mono_ns().wrapping_sub(t0);
            let c = time::cycles().wrapping_sub(c0);
            if d < best_ns {
                best_ns = d;
            }
            if c < best_cyc {
                best_cyc = c;
            }
        }
        ctx.begin_run(entry, best_ns, pages as u64);
        ctx.scalar(entry, "ns_per_fault", best_ns as f64 / pages as f64);
        if time::cycles_hz() > 0 {
            ctx.scalar(entry, "cycles_per_fault", best_cyc as f64 / pages as f64);
        }
        ctx.end_run(entry);
        Ok(())
    }
}

pub const M_PF_NS: MetricDef = MetricDef {
    key: "ns_per_fault",
    unit: "ns",
    direction: Direction::LowerBetter,
    kind: MetricKind::Scalar,
};
pub const M_PF_CYC: MetricDef = MetricDef {
    key: "cycles_per_fault",
    unit: "cycles",
    direction: Direction::LowerBetter,
    kind: MetricKind::Scalar,
};

// ---------------------------------------------------------------------------
// Protocol checks (no fixed expected value: scheduling latency is the measured
// environment-dependent quantity)
// ---------------------------------------------------------------------------

pub fn verify_ctxswitch(a: u32, b: u32) -> Result<(), &'static str> {
    let rounds = 50u64;
    let (ns, r) = futex_ping_once(a, b, rounds)?;
    if r != rounds || ns == 0 {
        return Err("futex ping protocol failed");
    }
    Ok(())
}

pub fn verify_wakeup(ctx: &Ctx) -> Result<(), &'static str> {
    let n = 4u64;
    let mut pool = WorkerPool::new(1, core::mem::size_of::<WakeShared>())
        .ok_or("worker pool allocation failed")?;
    pool.set_ctx(0, ctx.cpu_at(0), n);
    pool.spawn(vmbench_w_wake)?;
    time::sleep_ns(200_000);
    let s = pool.shared_at::<WakeShared>(0);
    for i in 0..n {
        time::sleep_ns(100_000);
        unsafe {
            (*s).wake_tsc.store(time::cycles(), Ordering::Release);
            (*s).ack.store(i as u32 + 1, Ordering::Release);
        }
        let _ = sys::futex_wake(unsafe { &(*s).ack }, 1);
    }
    pool.wait_all()?;
    if unsafe { (*s).n } != n {
        return Err("wakeup protocol count mismatch");
    }
    Ok(())
}
