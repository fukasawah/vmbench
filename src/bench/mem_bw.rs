use crate::bench::common::*;
use crate::bench::Benchmark;
use crate::runner::{Ctx, EntryMeta, ImplInfo, ParamValue};
use crate::sys;
use crate::time;
use core::sync::atomic::{AtomicU32, Ordering};

#[repr(C)]
pub struct BwArgs {
    pub dst: u64,
    pub src1: u64,
    pub src2: u64,
    pub len: u64,
}

/// Last checksum computed by a read kernel. Used by the expected-value
/// verification step to confirm the kernel really read the data it claims.
static mut BW_SINK: u64 = 0;

pub fn bw_sink() -> u64 {
    unsafe { core::ptr::read_volatile(core::ptr::addr_of!(BW_SINK)) }
}

#[inline(never)]
#[no_mangle]
pub unsafe extern "C" fn vmbench_k_bw_read(len: u64, ptr: usize) -> u64 {
    let p = ptr as *const u64;
    let n = (len / 8) as usize;
    let mut sum = 0u64;
    let mut i = 0usize;
    while i < n {
        sum = sum.wrapping_add(core::ptr::read_volatile(p.add(i)));
        i += 1;
    }
    core::ptr::write_volatile(core::ptr::addr_of_mut!(BW_SINK), sum);
    len
}

#[inline(never)]
#[no_mangle]
pub unsafe extern "C" fn vmbench_k_bw_write(len: u64, ptr: usize) -> u64 {
    let p = ptr as *mut u64;
    let n = (len / 8) as usize;
    let mut i = 0usize;
    while i < n {
        core::ptr::write_volatile(p.add(i), i as u64);
        i += 1;
    }
    len
}

#[inline(never)]
#[no_mangle]
pub unsafe extern "C" fn vmbench_k_bw_copy(args: usize) -> u64 {
    let a = &*(args as *const BwArgs);
    let d = a.dst as *mut u64;
    let s = a.src1 as *const u64;
    let n = (a.len / 8) as usize;
    let mut i = 0usize;
    while i < n {
        core::ptr::write_volatile(d.add(i), core::ptr::read_volatile(s.add(i)));
        i += 1;
    }
    a.len
}

#[inline(never)]
#[no_mangle]
pub unsafe extern "C" fn vmbench_k_bw_triad(args: usize) -> u64 {
    let a = &*(args as *const BwArgs);
    let d = a.dst as *mut u64;
    let s1 = a.src1 as *const u64;
    let s2 = a.src2 as *const u64;
    let n = (a.len / 8) as usize;
    let mut i = 0usize;
    while i < n {
        let v = core::ptr::read_volatile(s1.add(i))
            .wrapping_add(core::ptr::read_volatile(s2.add(i)).wrapping_mul(3));
        core::ptr::write_volatile(d.add(i), v);
        i += 1;
    }
    a.len
}

#[inline(never)]
#[no_mangle]
pub unsafe extern "C" fn vmbench_k_bw_scale(args: usize) -> u64 {
    let a = &*(args as *const BwArgs);
    let d = a.dst as *mut u64;
    let s = a.src1 as *const u64;
    let n = (a.len / 8) as usize;
    let mut i = 0usize;
    while i < n {
        core::ptr::write_volatile(
            d.add(i),
            core::ptr::read_volatile(s.add(i)).wrapping_mul(3),
        );
        i += 1;
    }
    a.len
}

#[inline(never)]
#[no_mangle]
pub unsafe extern "C" fn vmbench_k_bw_add(args: usize) -> u64 {
    let a = &*(args as *const BwArgs);
    let d = a.dst as *mut u64;
    let s1 = a.src1 as *const u64;
    let s2 = a.src2 as *const u64;
    let n = (a.len / 8) as usize;
    let mut i = 0usize;
    while i < n {
        core::ptr::write_volatile(
            d.add(i),
            core::ptr::read_volatile(s1.add(i)).wrapping_add(core::ptr::read_volatile(s2.add(i))),
        );
        i += 1;
    }
    a.len
}

#[repr(C)]
pub struct RandArgs {
    pub base: u64,
    pub list: u64,
    pub n: u64,
}

#[inline(never)]
#[no_mangle]
pub unsafe extern "C" fn vmbench_k_bw_rand_read(iters: u64, args: usize) -> u64 {
    let a = &*(args as *const RandArgs);
    let base = a.base as *const u8;
    let list = a.list as *const u32;
    let n = a.n as usize;
    let mut acc = 0u64;
    let mut it = 0u64;
    while it < iters {
        let mut i = 0usize;
        while i < n {
            let line = core::ptr::read_volatile(list.add(i)) as usize;
            acc = acc.wrapping_add(core::ptr::read_volatile(base.add(line * 64) as *const u64));
            i += 1;
        }
        it += 1;
    }
    core::ptr::write_volatile(core::ptr::addr_of_mut!(BW_SINK), acc);
    a.n * 64 * iters
}

#[inline(never)]
#[no_mangle]
pub unsafe extern "C" fn vmbench_k_bw_rand_write(iters: u64, args: usize) -> u64 {
    let a = &*(args as *const RandArgs);
    let base = a.base as *mut u8;
    let list = a.list as *const u32;
    let n = a.n as usize;
    let mut it = 0u64;
    while it < iters {
        let mut i = 0usize;
        while i < n {
            let line = core::ptr::read_volatile(list.add(i)) as usize;
            core::ptr::write_volatile(base.add(line * 64) as *mut u64, i as u64);
            i += 1;
        }
        it += 1;
    }
    a.n * 64 * iters
}

#[repr(C)]
pub struct BwShared {
    pub region_len: u64,
    pub n_workers: u64,
    pub base: u64,
    pub units: [u64; MAX_WORKERS],
    pub dur_ns: [u64; MAX_WORKERS],
    pub done: AtomicU32,
    pub _pad: [u8; 60],
}

extern "C" fn bw_worker(arg: usize) -> i32 {
    let wctx = unsafe { &*(arg as *const WorkerCtx) };
    let mut mask = [0u8; 128];
    mask[(wctx.cpu / 8) as usize] |= 1 << (wctx.cpu % 8);
    let _ = sys::sched_setaffinity(0, &mask);
    let shared = wctx.shared as *mut BwShared;
    let region_len = unsafe { (*shared).region_len };
    let n_workers = unsafe { (*shared).n_workers };
    let base = unsafe { (*shared).base };
    let op = wctx.arg as usize;
    let target = wctx.arg2.max(1_000_000);
    // Every worker streams the whole region, phase-shifted by one step, so the
    // combined working set is the full region instead of per-worker slices
    // that could fit in the last-level cache.
    let start = if n_workers > 1 {
        (wctx.index as u64 * region_len / n_workers) & !63
    } else {
        0
    };
    let seg1 = region_len - start;
    let seg2 = start;
    let goal = (target / 8).clamp(2_000_000, 20_000_000);
    let chunk = probe_chunk(goal, 1, |n| {
        let mut units = 0u64;
        for _ in 0..n {
            units = units.saturating_add(unsafe { run_op(op, base, start, seg1, region_len) });
            if seg2 > 0 {
                units = units.saturating_add(unsafe { run_op(op, base, 0, seg2, region_len) });
            }
        }
        units
    });
    let t0 = time::mono_ns();
    let deadline = time::Deadline::new(target);
    let mut units = 0u64;
    loop {
        for _ in 0..chunk {
            units = units.saturating_add(unsafe { run_op(op, base, start, seg1, region_len) });
            if seg2 > 0 {
                units = units.saturating_add(unsafe { run_op(op, base, 0, seg2, region_len) });
            }
        }
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

unsafe fn run_op(op: usize, base: u64, off: u64, slice_len: u64, region_len: u64) -> u64 {
    match op {
        0 => vmbench_k_bw_read(slice_len, (base + off) as usize),
        1 => vmbench_k_bw_write(slice_len, (base + off) as usize),
        2 => {
            let args = BwArgs {
                dst: base + off,
                src1: base + region_len + off,
                src2: 0,
                len: slice_len,
            };
            vmbench_k_bw_copy(&args as *const BwArgs as usize)
        }
        3 => {
            let args = BwArgs {
                dst: base + off,
                src1: base + region_len + off,
                src2: base + 2 * region_len + off,
                len: slice_len,
            };
            vmbench_k_bw_triad(&args as *const BwArgs as usize)
        }
        4 => {
            let args = BwArgs {
                dst: base + off,
                src1: base + region_len + off,
                src2: 0,
                len: slice_len,
            };
            vmbench_k_bw_scale(&args as *const BwArgs as usize)
        }
        _ => {
            let args = BwArgs {
                dst: base + off,
                src1: base + region_len + off,
                src2: base + 2 * region_len + off,
                len: slice_len,
            };
            vmbench_k_bw_add(&args as *const BwArgs as usize)
        }
    }
}

fn op_regions(op: usize) -> usize {
    match op {
        0 | 1 => 1,
        2 | 4 => 2,
        _ => 3,
    }
}

/// Known-answer / side-effect verification for one bandwidth op on a small
/// buffer. The buffer is filled with a deterministic pattern and the result is
/// checked with an independent loop.
pub fn verify_single(op: usize) -> Result<(), &'static str> {
    let len = 64 * 1024usize;
    let regions = op_regions(op);
    let buf = BigBuf::new(len * regions).ok_or("mmap failed")?;
    let base = buf.as_ptr();
    let words = len / 8;
    unsafe {
        for i in 0..words {
            let v = 0x1000_0000_0000_0000u64.wrapping_add(i as u64);
            core::ptr::write_volatile(base.add(i * 8) as *mut u64, v);
            if regions >= 2 {
                core::ptr::write_volatile(
                    base.add(len + i * 8) as *mut u64,
                    0x2000_0000_0000_0000u64.wrapping_add(i as u64),
                );
            }
            if regions >= 3 {
                core::ptr::write_volatile(
                    base.add(2 * len + i * 8) as *mut u64,
                    0x3000_0000_0000_0000u64.wrapping_add(i as u64),
                );
            }
        }
        let mut expected_sum = 0u64;
        for i in 0..words {
            expected_sum =
                expected_sum.wrapping_add(0x1000_0000_0000_0000u64.wrapping_add(i as u64));
        }
        match op {
            0 => {
                let r = vmbench_k_bw_read(len as u64, base as usize);
                if r != len as u64 {
                    return Err("bw read returned wrong length");
                }
                if bw_sink() != expected_sum {
                    return Err("bw read checksum mismatch");
                }
            }
            1 => {
                let _ = vmbench_k_bw_write(len as u64, base as usize);
                for i in 0..words {
                    let v = core::ptr::read_volatile(base.add(i * 8) as *const u64);
                    if v != i as u64 {
                        return Err("bw write content mismatch");
                    }
                }
            }
            2 => {
                let args = BwArgs {
                    dst: base as u64,
                    src1: (base as u64) + len as u64,
                    src2: 0,
                    len: len as u64,
                };
                let _ = vmbench_k_bw_copy(&args as *const BwArgs as usize);
                for i in 0..words {
                    let d = core::ptr::read_volatile(base.add(i * 8) as *const u64);
                    let s = core::ptr::read_volatile(base.add(len + i * 8) as *const u64);
                    if d != s {
                        return Err("bw copy content mismatch");
                    }
                }
            }
            3 | 4 | 5 => {
                let args = BwArgs {
                    dst: base as u64,
                    src1: (base as u64) + len as u64,
                    src2: (base as u64) + 2 * len as u64,
                    len: len as u64,
                };
                match op {
                    3 => {
                        let _ = vmbench_k_bw_triad(&args as *const BwArgs as usize);
                    }
                    4 => {
                        let _ = vmbench_k_bw_scale(&args as *const BwArgs as usize);
                    }
                    _ => {
                        let _ = vmbench_k_bw_add(&args as *const BwArgs as usize);
                    }
                }
                for i in 0..words {
                    let s1 = core::ptr::read_volatile(base.add(len + i * 8) as *const u64);
                    let want = match op {
                        3 => {
                            let s2 =
                                core::ptr::read_volatile(base.add(2 * len + i * 8) as *const u64);
                            s1.wrapping_add(s2.wrapping_mul(3))
                        }
                        4 => s1.wrapping_mul(3),
                        _ => {
                            let s2 =
                                core::ptr::read_volatile(base.add(2 * len + i * 8) as *const u64);
                            s1.wrapping_add(s2)
                        }
                    };
                    let d = core::ptr::read_volatile(base.add(i * 8) as *const u64);
                    if d != want {
                        return Err("bw compute content mismatch");
                    }
                }
            }
            _ => return Err("unknown bandwidth op"),
        }
    }
    Ok(())
}

/// Protocol check for the multi-thread bandwidth path.
pub fn verify_protocol(ctx: &Ctx, op: usize) -> Result<(), &'static str> {
    let region_len = 1 << 20;
    let regions = op_regions(op);
    let buf = BigBuf::new(region_len * regions).ok_or("mmap failed")?;
    let base = buf.as_ptr() as u64;
    let mut pool = WorkerPool::new(1, core::mem::size_of::<BwShared>())
        .ok_or("worker pool allocation failed")?;
    {
        let shared = pool.shared_at::<BwShared>(0);
        unsafe {
            (*shared).region_len = region_len as u64;
            (*shared).n_workers = 1;
            (*shared).base = base;
        }
    }
    pool.set_ctx(0, ctx.cpu_at(0), 1);
    pool.set_arg(0, op as u64);
    pool.set_arg2(0, 5_000_000);
    pool.spawn(bw_worker)?;
    pool.wait_all()?;
    let shared = pool.shared_at::<BwShared>(0);
    if unsafe { (*shared).units[0] } == 0 {
        return Err("bandwidth worker produced no bytes");
    }
    Ok(())
}

fn llc_total_bytes(ctx: &Ctx) -> usize {
    let mut total = 0u64;
    let mut seen: [&str; 64] = [""; 64];
    let mut nseen = 0usize;
    for c in ctx.env.caches.iter().filter(|c| c.level == 3) {
        let mut dup = false;
        for s in seen.iter().take(nseen) {
            if *s == c.shared_cpu_list {
                dup = true;
                break;
            }
        }
        if dup {
            continue;
        }
        if nseen < seen.len() {
            seen[nseen] = c.shared_cpu_list;
            nseen += 1;
        }
        total = total.saturating_add(c.size_bytes);
    }
    if total == 0 {
        return 32 * 1024 * 1024;
    }
    total as usize
}

/// Total working set for the DRAM bandwidth tests: at least 4x the total
/// last-level cache, capped at one third of RAM.
fn dram_total_target(ctx: &Ctx) -> usize {
    let mem = ctx.env.mem.total_kib.unwrap_or(1 << 20) as usize * 1024;
    let cap = mem / 3;
    let base = if ctx.cfg.quick {
        64 * 1024 * 1024
    } else {
        256 * 1024 * 1024
    };
    let llc4 = llc_total_bytes(ctx).saturating_mul(4);
    base.max(llc4).min(cap).max(16 * 1024 * 1024)
}

fn single_thread(ctx: &mut Ctx, entry: usize, op: usize, opname: &'static str) -> Result<(), &'static str> {
    let regions = op_regions(op);
    let region_len = dram_total_target(ctx) / regions;
    let region_len = region_len & !4095;
    let buf = BigBuf::new(region_len * regions).ok_or("mmap failed")?;
    buf.touch();
    let base = buf.as_ptr() as u64;
    ctx.add_param(entry, "op", ParamValue::Str(opname));
    ctx.add_param(entry, "threads", ParamValue::Int(1));
    ctx.add_param(entry, "region_bytes", ParamValue::Int(region_len as i64));
    let cpu = ctx.default_cpu();
    ctx.pin_self(cpu);
    ctx.measure(
        entry,
        ctx.cfg.target_run_ns,
        ctx.cfg.runs,
        |iters| {
            let mut units = 0u64;
            for _ in 0..iters {
                units = units.saturating_add(unsafe {
                    run_op(op, base, 0, region_len as u64, region_len as u64)
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

fn multi_thread(ctx: &mut Ctx, entry: usize, op: usize, opname: &'static str) -> Result<(), &'static str> {
    let n = ctx.env.online_cpus.len().min(MAX_WORKERS);
    if n < 2 {
        return Err("not enough online cpus");
    }
    let regions = op_regions(op);
    let region_len = (dram_total_target(ctx) / regions) & !4095;
    let buf = BigBuf::new(region_len * regions).ok_or("mmap failed")?;
    buf.touch();
    let base = buf.as_ptr() as u64;
    ctx.add_param(entry, "op", ParamValue::Str(opname));
    ctx.add_param(entry, "threads", ParamValue::Int(n as i64));
    ctx.add_param(entry, "region_bytes", ParamValue::Int(region_len as i64));
    ctx.unpin_self();
    for _ in 0..ctx.cfg.runs.max(1) {
        let mut pool = WorkerPool::new(n, core::mem::size_of::<BwShared>())
            .ok_or("worker pool allocation failed")?;
        {
            let shared = pool.shared_at::<BwShared>(0);
            unsafe {
                (*shared).region_len = region_len as u64;
                (*shared).n_workers = n as u64;
                (*shared).base = base;
            }
        }
        for i in 0..n {
            pool.set_ctx(i, ctx.cpu_at(i), 0);
            pool.set_arg(i, op as u64);
            pool.set_arg2(i, ctx.cfg.target_run_ns);
        }
        pool.spawn(bw_worker)?;
        pool.wait_all()?;
        let shared = pool.shared_at::<BwShared>(0);
        let mut units = 0u64;
        let mut max_dur = 1u64;
        for i in 0..n {
            units = units.saturating_add(unsafe { (*shared).units[i] });
            let d = unsafe { (*shared).dur_ns[i] };
            if d > max_dur {
                max_dur = d;
            }
        }
        ctx.begin_run(entry, max_dur, units);
        let secs = max_dur as f64 / 1e9;
        if secs > 0.0 {
            ctx.scalar(entry, "bytes_per_sec", units as f64 / secs);
        }
        ctx.end_run(entry);
    }
    Ok(())
}

macro_rules! bw_bench {
    ($struct_name:ident, $id:literal, $op:literal, $opname:literal, $threads:literal) => {
        pub struct $struct_name;
        impl Benchmark for $struct_name {
            fn meta(&self) -> &'static EntryMeta {
                static M: EntryMeta = EntryMeta {
                    id: $id,
                    version: 1,
                    description: "Sequential memory bandwidth",
                    imp: ImplInfo {
                        source: "src/bench/mem_bw.rs",
                        kernel: match $op {
                            0 => "vmbench_k_bw_read",
                            1 => "vmbench_k_bw_write",
                            2 => "vmbench_k_bw_copy",
                            3 => "vmbench_k_bw_triad",
                            4 => "vmbench_k_bw_scale",
                            _ => "vmbench_k_bw_add",
                        },
                        algorithm: "scalar volatile 64-bit accesses over a working set of at least 4x total LLC; threads stream the same regions at staggered offsets",
                        isa: BASELINE_ISA,
                    },
                    metrics: &[M_BYTES],
                };
                &M
            }
            fn supported(&self, ctx: &Ctx) -> Result<(), &'static str> {
                if $threads == 0 && ctx.env.online_cpus.len() < 2 {
                    return Err("not enough online cpus");
                }
                Ok(())
            }
            fn run(&self, ctx: &mut Ctx, entry: usize) -> Result<(), &'static str> {
                if $threads == 1 {
                    single_thread(ctx, entry, $op, $opname)
                } else {
                    multi_thread(ctx, entry, $op, $opname)
                }
            }
        }
    };
}

pub const M_BYTES: crate::runner::MetricDef = crate::runner::MetricDef {
    key: "bytes_per_sec",
    unit: "bytes/s",
    direction: crate::stats::Direction::HigherBetter,
    kind: crate::runner::MetricKind::Scalar,
};

bw_bench!(BwRead1, "memory.bandwidth.seq.read.1.v1", 0, "read", 1);
bw_bench!(BwReadAll, "memory.bandwidth.seq.read.all.v1", 0, "read", 0);
bw_bench!(BwWrite1, "memory.bandwidth.seq.write.1.v1", 1, "write", 1);
bw_bench!(BwWriteAll, "memory.bandwidth.seq.write.all.v1", 1, "write", 0);
bw_bench!(BwCopy1, "memory.bandwidth.seq.copy.1.v1", 2, "copy", 1);
bw_bench!(BwCopyAll, "memory.bandwidth.seq.copy.all.v1", 2, "copy", 0);
bw_bench!(BwTriad1, "memory.bandwidth.seq.triad.1.v1", 3, "triad", 1);
bw_bench!(BwTriadAll, "memory.bandwidth.seq.triad.all.v1", 3, "triad", 0);
bw_bench!(BwScale1, "memory.bandwidth.seq.scale.1.v1", 4, "scale", 1);
bw_bench!(BwScaleAll, "memory.bandwidth.seq.scale.all.v1", 4, "scale", 0);
bw_bench!(BwAdd1, "memory.bandwidth.seq.add.1.v1", 5, "add", 1);
bw_bench!(BwAddAll, "memory.bandwidth.seq.add.all.v1", 5, "add", 0);

fn random_bw(ctx: &mut Ctx, entry: usize, write: bool) -> Result<(), &'static str> {
    let region_len = (dram_total_target(ctx) & !4095).max(16 * 1024 * 1024);
    let buf = BigBuf::new(region_len).ok_or("mmap failed")?;
    buf.touch();
    let nlines = region_len / 64;
    let list: &'static mut [u32] = unsafe { ctx.arena.alloc_slice(nlines) }
        .ok_or("arena exhausted building line list")?;
    for (i, v) in list.iter_mut().enumerate() {
        *v = i as u32;
    }
    let mut rng = crate::rand::Rng::new(0x1357_9bdf_2468_ace0);
    rng.shuffle_u32(list);
    let args = RandArgs {
        base: buf.as_ptr() as u64,
        list: list.as_ptr() as u64,
        n: nlines as u64,
    };
    let cpu = ctx.default_cpu();
    ctx.pin_self(cpu);
    ctx.add_param(entry, "op", ParamValue::Str(if write { "random_write" } else { "random_read" }));
    ctx.add_param(entry, "region_bytes", ParamValue::Int(region_len as i64));
    ctx.add_param(entry, "cpu", ParamValue::Int(cpu as i64));
    let f: unsafe extern "C" fn(u64, usize) -> u64 = if write {
        vmbench_k_bw_rand_write
    } else {
        vmbench_k_bw_rand_read
    };
    ctx.measure(
        entry,
        ctx.cfg.target_run_ns,
        ctx.cfg.runs,
        |iters| unsafe { f(iters, &args as *const RandArgs as usize) },
        |ctx, e, s| {
            let secs = s.duration_ns as f64 / 1e9;
            if secs > 0.0 {
                ctx.scalar(e, "bytes_per_sec", s.units as f64 / secs);
            }
        },
    );
    Ok(())
}

pub struct BwRandomRead;
impl Benchmark for BwRandomRead {
    fn meta(&self) -> &'static EntryMeta {
        static M: EntryMeta = EntryMeta {
            id: "memory.bandwidth.random.read.v1",
            version: 1,
            description: "Cache-line granular random access bandwidth",
            imp: ImplInfo {
                source: "src/bench/mem_bw.rs",
                kernel: "vmbench_k_bw_rand_read",
                algorithm: "one 8-byte load per cache line following a shuffled line order",
                isa: BASELINE_ISA,
            },
            metrics: &[M_BYTES],
        };
        &M
    }
    fn run(&self, ctx: &mut Ctx, entry: usize) -> Result<(), &'static str> {
        random_bw(ctx, entry, false)
    }
}

pub struct BwRandomWrite;
impl Benchmark for BwRandomWrite {
    fn meta(&self) -> &'static EntryMeta {
        static M: EntryMeta = EntryMeta {
            id: "memory.bandwidth.random.write.v1",
            version: 1,
            description: "Cache-line granular random write bandwidth",
            imp: ImplInfo {
                source: "src/bench/mem_bw.rs",
                kernel: "vmbench_k_bw_rand_write",
                algorithm: "one 8-byte store per cache line following a shuffled line order",
                isa: BASELINE_ISA,
            },
            metrics: &[M_BYTES],
        };
        &M
    }
    fn run(&self, ctx: &mut Ctx, entry: usize) -> Result<(), &'static str> {
        random_bw(ctx, entry, true)
    }
}
