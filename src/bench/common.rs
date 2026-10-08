use crate::runner::{Ctx, MetricDef, MetricKind, Sample};
use crate::stats::Direction;
use crate::sys;
use crate::time;
use crate::util::Arena;

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct WorkerCtx {
    pub cpu: u32,
    pub index: u32,
    pub shared: u64,
    /// Chunk size (iterations per timed chunk).
    pub iters: u64,
    pub arg: u64,
    /// Target run duration in nanoseconds.
    pub arg2: u64,
}

pub const MAX_WORKERS: usize = 64;
pub const WORKER_STACK: usize = 256 * 1024;
/// Maximum io_uring queue depth supported by the shared measurement struct.
pub const MAX_QD: usize = 64;
/// Latency sample slots (1 MiB of u32) shared by storage measurements.
pub const MAX_SAMPLES: usize = 262_144;

/// Number of opening operations whose latency seeds the sampling stride.
pub const SAMPLES_WARMUP: u64 = 256;

#[inline]
pub fn cycles_to_ns_u32(c: u64) -> u32 {
    let hz = time::cycles_hz();
    if hz == 0 {
        return 0;
    }
    let ns = (c as u128 * 1_000_000_000u128 / hz as u128) as u64;
    ns.min(u32::MAX as u64) as u32
}

/// Uniform-in-time latency sampling with a fixed per-worker slot budget.
///
/// The first `SAMPLES_WARMUP` operations are recorded, then the operation rate
/// is used to derive a stride that spreads the remaining budget over the whole
/// measurement window. The run is never shortened by the sample budget; only
/// recording stops when the budget is full.
pub struct Sampler {
    quota: u32,
    count: u32,
    ops: u64,
    stride: u64,
    warm_sum: u64,
    target_ns: u64,
}

impl Sampler {
    pub fn new(quota: u32, target_ns: u64) -> Sampler {
        Sampler {
            quota,
            count: 0,
            ops: 0,
            stride: 1,
            warm_sum: 0,
            target_ns,
        }
    }

    /// Feeds one completed operation and returns the slot to record into.
    #[inline]
    pub fn on_op(&mut self, ns: u32) -> Option<usize> {
        if self.quota == 0 {
            return None;
        }
        self.ops += 1;
        if self.count >= self.quota {
            return None;
        }
        if self.ops <= SAMPLES_WARMUP {
            self.warm_sum += ns as u64;
            if self.ops == SAMPLES_WARMUP {
                let avg = (self.warm_sum / SAMPLES_WARMUP).max(1);
                let expected = self.target_ns / avg;
                self.stride = expected.div_ceil(self.quota as u64).max(1);
            }
            let slot = self.count as usize;
            self.count += 1;
            return Some(slot);
        }
        if self.count < self.quota && (self.ops - SAMPLES_WARMUP) % self.stride == 0 {
            let slot = self.count as usize;
            self.count += 1;
            return Some(slot);
        }
        None
    }

    pub fn count(&self) -> u32 {
        self.count
    }
}

/// Latency statistics for one I/O measurement window.
pub struct IoStats {
    pub dur_ns: u64,
    pub ops: u64,
    pub block: u64,
    pub mean_ns: f64,
    pub p50_ns: f64,
    pub p95_ns: f64,
    pub p99_ns: f64,
    pub p999_ns: f64,
    pub max_ns: f64,
    pub has_latency: bool,
}

/// Computes latency statistics from unsorted samples, sorted in place.
pub fn io_stats(samples: &mut [u32], dur_ns: u64, ops: u64, block: u64) -> IoStats {
    let cnt = samples.len();
    let mut st = IoStats {
        dur_ns,
        ops,
        block,
        mean_ns: 0.0,
        p50_ns: 0.0,
        p95_ns: 0.0,
        p99_ns: 0.0,
        p999_ns: 0.0,
        max_ns: 0.0,
        has_latency: cnt > 0,
    };
    if cnt == 0 {
        return st;
    }
    let sum: u64 = samples.iter().map(|&v| v as u64).sum();
    st.mean_ns = sum as f64 / cnt as f64;
    samples.sort_unstable();
    let pick = |p: f64| -> f64 {
        let i = (((cnt as f64 - 1.0) * p) + 0.5) as usize;
        samples[i.min(cnt - 1)] as f64
    };
    st.p50_ns = pick(0.50);
    st.p95_ns = pick(0.95);
    st.p99_ns = pick(0.99);
    st.p999_ns = pick(0.999);
    st.max_ns = samples[cnt - 1] as f64;
    st
}

pub type Kernel64 = unsafe extern "C" fn(u64, u64) -> u64;

/// Calls a benchmark kernel through a volatile-loaded function pointer so the
/// compiler cannot inline or constant-fold it. The `#[used]` table that owns
/// the pointer also keeps the kernel machine code in the binary.
#[inline(never)]
pub fn call_k64(table: &'static [Kernel64], idx: usize, iters: u64, seed: u64) -> u64 {
    let f = unsafe { core::ptr::read_volatile(&table[idx]) };
    unsafe { f(iters, seed) }
}

/// Chooses a chunk size (iterations) whose execution should take about
/// `goal_ns`, measured under the *current* conditions. Used by worker
/// processes because contention can change the per-iteration cost by orders of
/// magnitude compared to a single-threaded calibration.
pub fn probe_chunk<F: FnMut(u64) -> u64>(goal_ns: u64, probe: u64, mut body: F) -> u64 {
    let probe = probe.max(1);
    let t0 = time::mono_ns();
    let units = body(probe);
    let ns = time::mono_ns().wrapping_sub(t0).max(1);
    core::hint::black_box(units);
    let chunk = probe as u128 * goal_ns.max(1) as u128 / ns as u128;
    if chunk > 100_000_000 {
        100_000_000
    } else {
        (chunk as u64).max(1)
    }
}

#[cfg(target_arch = "x86_64")]
pub const BASELINE_ISA: &str = "x86_64-baseline";
#[cfg(target_arch = "aarch64")]
pub const BASELINE_ISA: &str = "aarch64-baseline";

pub const M_OPS: MetricDef = MetricDef {
    key: "ops_per_sec",
    unit: "ops/s",
    direction: Direction::HigherBetter,
    kind: MetricKind::Scalar,
};
pub const M_NS_OP: MetricDef = MetricDef {
    key: "ns_per_op",
    unit: "ns",
    direction: Direction::LowerBetter,
    kind: MetricKind::Scalar,
};
pub const M_CYC_OP: MetricDef = MetricDef {
    key: "cycles_per_op",
    unit: "cycles",
    direction: Direction::LowerBetter,
    kind: MetricKind::Scalar,
};
pub const M_ITER: MetricDef = MetricDef {
    key: "iterations_per_sec",
    unit: "iter/s",
    direction: Direction::HigherBetter,
    kind: MetricKind::Scalar,
};
pub const M_NS_ITER: MetricDef = MetricDef {
    key: "ns_per_iter",
    unit: "ns",
    direction: Direction::LowerBetter,
    kind: MetricKind::Scalar,
};
pub const M_CYC_ITER: MetricDef = MetricDef {
    key: "cycles_per_iter",
    unit: "cycles",
    direction: Direction::LowerBetter,
    kind: MetricKind::Scalar,
};
/// Overhead-corrected cycles per op (throughput kernels only). The correction
/// subtracts the measured loop-control cost divided by the unroll factor; it
/// is an approximation, so the raw `cycles_per_op` remains the primary value.
pub const M_CYC_OP_NET: MetricDef = MetricDef {
    key: "cycles_per_op_net",
    unit: "cycles",
    direction: Direction::LowerBetter,
    kind: MetricKind::Scalar,
};

pub fn emit_rate_net(ctx: &mut Ctx, entry: usize, s: &Sample, ops_per_iter: u64) {
    emit_rate(ctx, entry, s);
    if time::cycles_hz() == 0 {
        return;
    }
    if let Some(ov) = ctx.loop_overhead_cycles() {
        if ov > 0.0 && ops_per_iter > 0 {
            let raw = s.cycles as f64 / s.units.max(1) as f64;
            let net = raw - ov / ops_per_iter as f64;
            ctx.scalar(entry, "cycles_per_op_net", net.max(0.0));
        }
    }
}

pub fn emit_rate(ctx: &mut Ctx, entry: usize, s: &Sample) {
    let units = s.units.max(1);
    let secs = (s.duration_ns as f64) / 1e9;
    if secs > 0.0 {
        ctx.scalar(entry, "ops_per_sec", units as f64 / secs);
    }
    ctx.scalar(entry, "ns_per_op", s.duration_ns as f64 / units as f64);
    if time::cycles_hz() > 0 {
        ctx.scalar(entry, "cycles_per_op", s.cycles as f64 / units as f64);
    }
}

pub fn emit_iter(ctx: &mut Ctx, entry: usize, s: &Sample) {
    let secs = (s.duration_ns as f64) / 1e9;
    let iters = s.units.max(1);
    if secs > 0.0 {
        ctx.scalar(entry, "iterations_per_sec", iters as f64 / secs);
    }
    ctx.scalar(entry, "ns_per_iter", s.duration_ns as f64 / iters as f64);
    if time::cycles_hz() > 0 {
        ctx.scalar(entry, "cycles_per_iter", s.cycles as f64 / iters as f64);
    }
}

pub fn emit_bytes_rate(ctx: &mut Ctx, entry: usize, s: &Sample, bytes: u64) {
    let secs = (s.duration_ns as f64) / 1e9;
    if secs > 0.0 {
        ctx.scalar(entry, "bytes_per_sec", bytes as f64 / secs);
    }
}

pub struct WorkerPool {
    pub shared: *mut u8,
    pub ctxs: *mut WorkerCtx,
    pub stacks: *mut u8,
    pub n: usize,
    pub pids: [i32; MAX_WORKERS],
    region: *mut u8,
    region_len: usize,
}

impl WorkerPool {
    pub fn new(n: usize, shared_bytes: usize) -> Option<WorkerPool> {
        if n == 0 || n > MAX_WORKERS {
            return None;
        }
        let shared_al = (shared_bytes + 63) & !63;
        let ctx_len = n * core::mem::size_of::<WorkerCtx>();
        let stack_off = (shared_al + ctx_len + 63) & !63;
        let stack_len = n * WORKER_STACK;
        let total = stack_off + stack_len;
        let region = sys::mmap_anon(total, true)?;
        unsafe {
            core::ptr::write_bytes(region, 0, total);
        }
        Some(WorkerPool {
            shared: region,
            ctxs: unsafe { region.add(shared_al) as *mut WorkerCtx },
            stacks: unsafe { region.add(stack_off) },
            n,
            pids: [0; MAX_WORKERS],
            region,
            region_len: total,
        })
    }

    pub fn set_ctx(&mut self, i: usize, cpu: u32, iters: u64) {
        unsafe {
            let c = &mut *self.ctxs.add(i);
            c.cpu = cpu;
            c.index = i as u32;
            c.shared = self.shared as u64;
            c.iters = iters;
            c.arg = 0;
            c.arg2 = 0;
        }
    }

    pub fn set_arg(&mut self, i: usize, arg: u64) {
        unsafe {
            let c = &mut *self.ctxs.add(i);
            c.arg = arg;
        }
    }

    pub fn set_arg2(&mut self, i: usize, arg: u64) {
        unsafe {
            let c = &mut *self.ctxs.add(i);
            c.arg2 = arg;
        }
    }

    pub fn spawn(&mut self, entry: extern "C" fn(usize) -> i32) -> Result<(), &'static str> {
        for i in 0..self.n {
            let stack_top = unsafe { self.stacks.add((i + 1) * WORKER_STACK) };
            let arg = unsafe { self.ctxs.add(i) } as usize;
            match sys::clone_process(stack_top, arg, entry) {
                Ok(pid) => self.pids[i] = pid,
                Err(_) => return Err("clone failed"),
            }
        }
        Ok(())
    }

    pub fn wait_all(&mut self) -> Result<(), &'static str> {
        for i in 0..self.n {
            let pid = self.pids[i];
            if pid <= 0 {
                continue;
            }
            match sys::wait_child(pid) {
                Ok(status) => {
                    if status != 0 {
                        return Err("worker failed");
                    }
                }
                Err(_) => return Err("wait4 failed"),
            }
        }
        Ok(())
    }

    pub fn shared_at<T>(&self, offset: usize) -> *mut T {
        unsafe { self.shared.add(offset) as *mut T }
    }
}

impl Drop for WorkerPool {
    fn drop(&mut self) {
        let _ = sys::munmap(self.region, self.region_len);
    }
}

/// Large anonymous mapping used by memory benchmarks. The buffer is not part
/// of the arena so that it can be released between benchmarks.
pub struct BigBuf {
    pub ptr: *mut u8,
    pub len: usize,
}

impl BigBuf {
    pub fn new(len: usize) -> Option<BigBuf> {
        let len = (len + 4095) & !4095;
        let ptr = sys::mmap_anon(len, false)?;
        Some(BigBuf { ptr, len })
    }

    pub fn touch(&self) {
        unsafe { touch(self.ptr, self.len) };
    }

    pub fn as_ptr(&self) -> *mut u8 {
        self.ptr
    }
}

impl Drop for BigBuf {
    fn drop(&mut self) {
        let _ = sys::munmap(self.ptr, self.len);
    }
}

/// Allocates a page-backed buffer from the arena and faults every page in.
pub unsafe fn touch(ptr: *mut u8, len: usize) {
    let ps = crate::rt::page_size();
    let mut off = 0;
    while off < len {
        core::ptr::write_volatile(ptr.add(off), 0);
        off += ps;
    }
    if len > 0 {
        core::ptr::write_volatile(ptr.add(len - 1), 0);
    }
}

pub unsafe fn alloc_buffer(arena: &mut Arena, len: usize, align: usize) -> Option<&'static mut [u8]> {
    let b = arena.alloc_bytes(len, align)?;
    touch(b.as_mut_ptr(), len);
    Some(b)
}

/// Builds a random single-cycle linked list over `entries` nodes of `stride`
/// bytes each inside `buf`. Returns the byte offset of the entry node.
pub fn build_pointer_cycle(
    arena: &mut Arena,
    buf: *mut u8,
    entries: usize,
    stride: usize,
    rng: &mut crate::rand::Rng,
) -> usize {
    let order: &'static mut [u32] = unsafe { arena.alloc_slice(entries) }.unwrap_or_else(|| {
        crate::rt::fatal("arena exhausted building pointer cycle");
    });
    for (i, o) in order.iter_mut().enumerate() {
        *o = i as u32;
    }
    rng.shuffle_u32(order);
    unsafe {
        for i in 0..entries {
            let cur = order[i] as usize;
            let next = order[(i + 1) % entries] as usize;
            let node = buf.add(cur * stride) as *mut u64;
            core::ptr::write_volatile(node, buf.add(next * stride) as u64);
        }
    }
    unsafe { buf.add(order[0] as usize * stride) as usize - buf as usize }
}
