use crate::bench::common::*;
use crate::bench::Benchmark;
use crate::runner::{Ctx, EntryMeta, ImplInfo, MetricDef, MetricKind, ParamValue, Point};
use crate::stats::Direction;
use crate::time;
use crate::util::Arena;

pub const M_NS_ACCESS: MetricDef = MetricDef {
    key: "ns_per_access",
    unit: "ns",
    direction: Direction::LowerBetter,
    kind: MetricKind::Curve,
};
pub const M_NS_ACCESS_INDEPENDENT: MetricDef = MetricDef {
    key: "ns_per_access_independent",
    unit: "ns",
    direction: Direction::LowerBetter,
    kind: MetricKind::Curve,
};
pub const M_EFFECTIVE_CAPACITY: MetricDef = MetricDef {
    key: "effective_capacity_bytes",
    unit: "bytes",
    direction: Direction::HigherBetter,
    kind: MetricKind::Scalar,
};
pub const M_NS_ACCESS_S: MetricDef = MetricDef {
    key: "ns_per_access",
    unit: "ns",
    direction: Direction::LowerBetter,
    kind: MetricKind::Scalar,
};
pub const M_CYC_ACCESS_S: MetricDef = MetricDef {
    key: "cycles_per_access",
    unit: "cycles",
    direction: Direction::LowerBetter,
    kind: MetricKind::Scalar,
};

pub const LINE: usize = 64;

#[inline(never)]
#[no_mangle]
pub unsafe extern "C" fn vmbench_k_chase(iters: u64, start: usize) -> u64 {
    let mut p = start as *const u64;
    let mut i = 0u64;
    while i < iters {
        p = core::ptr::read_volatile(p) as *const u64;
        i += 1;
    }
    p as u64
}

unsafe fn chase_k<const K: usize>(iters: u64, ptrs: *const u64) -> u64 {
    let mut p = [0u64; K];
    let mut j = 0usize;
    while j < K {
        p[j] = core::ptr::read_volatile(ptrs.add(j));
        j += 1;
    }
    let mut i = 0u64;
    while i < iters {
        let mut j = 0usize;
        while j < K {
            p[j] = core::ptr::read_volatile(p[j] as *const u64);
            j += 1;
        }
        i += 1;
    }
    let mut r = 0u64;
    let mut j = 0usize;
    while j < K {
        r ^= p[j];
        j += 1;
    }
    core::hint::black_box(r)
}

macro_rules! mlp_kernel {
    ($fname:ident, $k:literal) => {
        #[inline(never)]
        #[no_mangle]
        pub unsafe extern "C" fn $fname(iters: u64, ptrs: u64) -> u64 {
            chase_k::<$k>(iters, ptrs as usize as *const u64)
        }
    };
}

mlp_kernel!(vmbench_k_mlp1, 1);
mlp_kernel!(vmbench_k_mlp2, 2);
mlp_kernel!(vmbench_k_mlp4, 4);
mlp_kernel!(vmbench_k_mlp8, 8);
mlp_kernel!(vmbench_k_mlp16, 16);

#[inline(never)]
#[no_mangle]
pub unsafe extern "C" fn vmbench_k_tlb(iters: u64, args: usize) -> u64 {
    let a = &*(args as *const TlbArgs);
    let list = a.list as *const u32;
    let n = a.n_pages as usize;
    let stride = a.page_stride as usize;
    let off = a.offset as usize;
    let base = a.base as *const u8;
    let mut acc = 0u64;
    let mut i = 0u64;
    let mut idx = 0usize;
    while i < iters {
        let page = core::ptr::read_volatile(list.add(idx)) as usize;
        acc = acc.wrapping_add(core::ptr::read_volatile(base.add(page * stride + off) as *const u64));
        idx += 1;
        if idx >= n {
            idx = 0;
        }
        i += 1;
    }
    core::hint::black_box(acc)
}

#[repr(C)]
pub struct TlbArgs {
    pub base: u64,
    pub list: u64,
    pub n_pages: u64,
    pub page_stride: u64,
    pub offset: u64,
}

/// Builds a random single cycle over `entries` nodes of `stride` bytes.
/// Uses a Fisher-Yates permutation of node indices.
pub fn build_cycle(arena: &mut Arena, buf: *mut u8, entries: usize, rng: &mut crate::rand::Rng) -> usize {
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
            core::ptr::write_volatile(
                buf.add(cur * LINE) as *mut u64,
                buf.add(next * LINE) as u64,
            );
        }
    }
    unsafe { buf.add(order[0] as usize * LINE) as usize }
}

pub fn build_page_list(arena: &mut Arena, pages: usize, rng: &mut crate::rand::Rng) -> &'static [u32] {
    let order: &'static mut [u32] = unsafe { arena.alloc_slice(pages) }.unwrap_or_else(|| {
        crate::rt::fatal("arena exhausted building page list");
    });
    for (i, o) in order.iter_mut().enumerate() {
        *o = i as u32;
    }
    rng.shuffle_u32(order);
    order
}

fn mem_cap(ctx: &Ctx) -> usize {
    let mem = ctx.env.mem.total_kib.unwrap_or(1 << 20) as usize * 1024;
    mem / 4
}

fn max_ws(ctx: &Ctx) -> usize {
    let cap = mem_cap(ctx);
    if ctx.cfg.quick {
        (64 * 1024 * 1024).min(cap)
    } else {
        (1024 * 1024 * 1024).min(cap)
    }
}

fn calibrate_chase(iters0: u64, start: usize, target_ns: u64) -> u64 {
    let mut iters = iters0.max(1000);
    let mut ns = 1u64;
    for _ in 0..20 {
        let t0 = time::mono_ns();
        let _ = unsafe { vmbench_k_chase(iters, start) };
        ns = time::mono_ns().wrapping_sub(t0).max(1);
        if ns >= target_ns / 4 {
            break;
        }
        iters = iters.saturating_mul(4);
    }
    let scaled = iters as u128 * target_ns as u128 / ns as u128;
    if scaled > 100_000_000u128 {
        100_000_000
    } else {
        (scaled as u64).max(1000)
    }
}

pub struct CacheLatencyCurve;

impl Benchmark for CacheLatencyCurve {
    fn meta(&self) -> &'static EntryMeta {
        static M: EntryMeta = EntryMeta {
            id: "cache.latency.curve.v1",
            version: 1,
            description: "Pointer-chase latency curve across working set sizes",
            imp: ImplInfo {
                source: "src/bench/mem_latency.rs",
                kernel: "vmbench_k_chase",
                algorithm: "dependent load pointer chase over a Fisher-Yates random cycle, working set swept from 4 KiB to 1 GiB",
                isa: BASELINE_ISA,
            },
            metrics: &[M_NS_ACCESS, M_EFFECTIVE_CAPACITY],
        };
        &M
    }

    fn run(&self, ctx: &mut Ctx, entry: usize) -> Result<(), &'static str> {
        let cap = max_ws(ctx);
        let mut sizes = [0usize; 24];
        let mut nsizes = 0usize;
        let mut s = 4096usize;
        while s <= cap && nsizes < 24 {
            sizes[nsizes] = s;
            nsizes += 1;
            s *= 2;
        }
        if nsizes == 0 {
            return Err("no working set sizes fit in memory");
        }
        let cpu = ctx.default_cpu();
        ctx.pin_self(cpu);
        ctx.add_param(entry, "cpu", ParamValue::Int(cpu as i64));
        let target = ctx.cfg.target_run_ns / 8;
        let nruns = ctx.cfg.runs.clamp(1, 8) as usize;
        let mut pts = [[Point { x: 0.0, y: 0.0 }; 24]; 8];
        let mut run_ns = [0u64; 8];
        let mut run_iters = [0u64; 8];
        let mut rng = crate::rand::Rng::new(0x5eed_1234_abcd_ef01);
        for si in 0..nsizes {
            let size = sizes[si];
            let buf = BigBuf::new(size).ok_or("mmap failed")?;
            buf.touch();
            let entries = size / LINE;
            let start = build_cycle(ctx.arena, buf.as_ptr(), entries, &mut rng);
            let iters = calibrate_chase(2000, start, target);
            let _ = unsafe { vmbench_k_chase(iters, start) };
            for r in 0..nruns {
                let t0 = time::mono_ns();
                let _ = unsafe { vmbench_k_chase(iters, start) };
                let d = time::mono_ns().wrapping_sub(t0).max(1);
                pts[r][si] = Point {
                    x: size as f64,
                    y: d as f64 / iters as f64,
                };
                run_ns[r] = run_ns[r].saturating_add(d);
                run_iters[r] = run_iters[r].saturating_add(iters);
            }
        }
        for r in 0..nruns {
            ctx.begin_run(entry, run_ns[r], run_iters[r]);
            ctx.curve(entry, "ns_per_access", &pts[r][..nsizes]);
            ctx.end_run(entry);
        }
        // Effective capacity: last size where latency is below 2x the minimum.
        let mut min_y = f64::MAX;
        for i in 0..nsizes {
            if pts[0][i].y < min_y {
                min_y = pts[0][i].y;
            }
        }
        let mut eff = 0.0;
        for i in 0..nsizes {
            if pts[0][i].y < min_y * 2.0 {
                eff = pts[0][i].x;
            }
        }
        ctx.scalar(entry, "effective_capacity_bytes", eff);
        Ok(())
    }
}

pub struct DramLatency;

impl Benchmark for DramLatency {
    fn meta(&self) -> &'static EntryMeta {
        static M: EntryMeta = EntryMeta {
            id: "memory.latency.dram.v1",
            version: 1,
            description: "DRAM dependent-load latency (random pointer chase)",
            imp: ImplInfo {
                source: "src/bench/mem_latency.rs",
                kernel: "vmbench_k_chase",
                algorithm: "dependent load pointer chase over a random cycle several times larger than LLC",
                isa: BASELINE_ISA,
            },
            metrics: &[M_NS_ACCESS_S, M_CYC_ACCESS_S],
        };
        &M
    }

    fn run(&self, ctx: &mut Ctx, entry: usize) -> Result<(), &'static str> {
        let cap = mem_cap(ctx);
        let size = (512 * 1024 * 1024).min(cap).max(64 * 1024 * 1024);
        let buf = BigBuf::new(size).ok_or("mmap failed")?;
        buf.touch();
        let cpu = ctx.default_cpu();
        ctx.pin_self(cpu);
        ctx.add_param(entry, "working_set_bytes", ParamValue::Int(size as i64));
        ctx.add_param(entry, "cpu", ParamValue::Int(cpu as i64));
        let mut rng = crate::rand::Rng::new(0xd00d_f00d_1234_5678);
        let start = build_cycle(ctx.arena, buf.as_ptr(), size / LINE, &mut rng);
        ctx.measure(
            entry,
            ctx.cfg.target_run_ns,
            ctx.cfg.runs,
            |iters| {
                let _ = unsafe { vmbench_k_chase(iters, start) };
                iters
            },
            |ctx, e, s| {
                let acc = s.units.max(1) as f64;
                ctx.scalar(e, "ns_per_access", s.duration_ns as f64 / acc);
                if time::cycles_hz() > 0 {
                    ctx.scalar(e, "cycles_per_access", s.cycles as f64 / acc);
                }
            },
        );
        Ok(())
    }
}

#[used]
static MLP_KERNELS: [Kernel64; 5] = [
    vmbench_k_mlp1,
    vmbench_k_mlp2,
    vmbench_k_mlp4,
    vmbench_k_mlp8,
    vmbench_k_mlp16,
];

static MLP_NAMES: [&str; 5] = [
    "vmbench_k_mlp1",
    "vmbench_k_mlp2",
    "vmbench_k_mlp4",
    "vmbench_k_mlp8",
    "vmbench_k_mlp16",
];

pub struct Mlp;

impl Benchmark for Mlp {
    fn meta(&self) -> &'static EntryMeta {
        static M: EntryMeta = EntryMeta {
            id: "memory.mlp.v1",
            version: 1,
            description: "Memory-level parallelism: independent pointer-chase chains",
            imp: ImplInfo {
                source: "src/bench/mem_latency.rs",
                kernel: "vmbench_k_mlp16",
                algorithm: "K independent random cycles in DRAM, chased round-robin; reports ns/access per K",
                isa: BASELINE_ISA,
            },
            metrics: &[M_NS_ACCESS],
        };
        &M
    }

    fn run(&self, ctx: &mut Ctx, entry: usize) -> Result<(), &'static str> {
        let cap = mem_cap(ctx);
        let size = (256 * 1024 * 1024).min(cap).max(64 * 1024 * 1024);
        let buf = BigBuf::new(size).ok_or("mmap failed")?;
        buf.touch();
        let cpu = ctx.default_cpu();
        ctx.pin_self(cpu);
        ctx.add_param(entry, "working_set_bytes", ParamValue::Int(size as i64));
        let ks = [1u64, 2, 4, 8, 16];
        let mut starts = [0u64; 16];
        let mut rng = crate::rand::Rng::new(0x1111_2222_3333_4444);
        let target = ctx.cfg.target_run_ns / 4;
        let nruns = ctx.cfg.runs.clamp(1, 8) as usize;
        let mut pts = [[Point { x: 0.0, y: 0.0 }; 5]; 8];
        let mut run_ns = [0u64; 8];
        for (ki, k) in ks.iter().enumerate() {
            let idx = match k {
                1 => 0,
                2 => 1,
                4 => 2,
                8 => 3,
                _ => 4,
            };
            let region = size / *k as usize;
            for j in 0..*k as usize {
                let base = unsafe { buf.as_ptr().add(j * region) };
                starts[j] = build_cycle(ctx.arena, base, region / LINE, &mut rng) as u64;
            }
            let f = unsafe { core::ptr::read_volatile(&MLP_KERNELS[idx]) };
            let mut iters = 500u64;
            let mut ns = 1u64;
            for _ in 0..16 {
                let t0 = time::mono_ns();
                let _ = unsafe { f(iters, starts.as_ptr() as u64) };
                ns = time::mono_ns().wrapping_sub(t0).max(1);
                if ns >= target / 4 {
                    break;
                }
                iters = iters.saturating_mul(4);
            }
            let scaled = iters as u128 * target as u128 / ns as u128;
            let iters = if scaled > 20_000_000 {
                20_000_000
            } else {
                (scaled as u64).max(500)
            };
            for r in 0..nruns {
                let t0 = time::mono_ns();
                let _ = unsafe { f(iters, starts.as_ptr() as u64) };
                let d = time::mono_ns().wrapping_sub(t0).max(1);
                let accesses = iters.saturating_mul(*k);
                pts[r][ki] = Point {
                    x: *k as f64,
                    y: d as f64 / accesses as f64,
                };
                run_ns[r] = run_ns[r].saturating_add(d);
            }
        }
        for r in 0..nruns {
            ctx.begin_run(entry, run_ns[r], 0);
            ctx.curve(entry, "ns_per_access", &pts[r][..5]);
            ctx.end_run(entry);
        }
        Ok(())
    }
}

pub struct TlbCurve;

impl Benchmark for TlbCurve {
    fn meta(&self) -> &'static EntryMeta {
        static M: EntryMeta = EntryMeta {
            id: "memory.tlb.v1",
            version: 1,
            description: "TLB capacity / page-walk cost (dependent page-granular pointer chase)",
            imp: ImplInfo {
                source: "src/bench/mem_latency.rs",
                kernel: "vmbench_k_chase",
                algorithm: "dependent 8-byte pointer chase over a cycle whose nodes are spaced one 4 KiB page apart; an independent-loads curve over the same pages is measured for reference",
                isa: BASELINE_ISA,
            },
            metrics: &[M_NS_ACCESS, M_NS_ACCESS_INDEPENDENT],
        };
        &M
    }

    fn run(&self, ctx: &mut Ctx, entry: usize) -> Result<(), &'static str> {
        let cap = max_ws(ctx);
        let sizes = [4 << 20, 8 << 20, 16 << 20, 64 << 20, 256 << 20, 1024 << 20];
        let nruns = ctx.cfg.runs.clamp(1, 8) as usize;
        let mut dep = [[Point { x: 0.0, y: 0.0 }; 6]; 8];
        let mut ind = [[Point { x: 0.0, y: 0.0 }; 6]; 8];
        let mut run_ns = [0u64; 8];
        let mut run_iters = [0u64; 8];
        let cpu = ctx.default_cpu();
        ctx.pin_self(cpu);
        ctx.add_param(entry, "cpu", ParamValue::Int(cpu as i64));
        let target = ctx.cfg.target_run_ns / 8;
        let mut rng = crate::rand::Rng::new(0xfeed_beef_0bad_f00d);
        let mut nvalid = 0usize;
        for (si, size) in sizes.iter().enumerate() {
            if *size > cap {
                break;
            }
            nvalid += 1;
            let buf = BigBuf::new(*size).ok_or("mmap failed")?;
            buf.touch();
            let pages = size / 4096;
            let start_off =
                build_pointer_cycle(ctx.arena, buf.as_ptr(), pages, 4096, &mut rng);
            let start = buf.as_ptr() as usize + start_off;
            let list = build_page_list(ctx.arena, pages, &mut rng);
            let args = TlbArgs {
                base: buf.as_ptr() as u64,
                list: list.as_ptr() as u64,
                n_pages: pages as u64,
                page_stride: 4096,
                offset: 2048,
            };
            // Dependent chase: the next load address comes from the previous
            // load, so page walks cannot overlap across accesses.
            let iters = calibrate_chase(1000, start, target);
            for r in 0..nruns {
                let t0 = time::mono_ns();
                let _ = unsafe { vmbench_k_chase(iters, start) };
                let d = time::mono_ns().wrapping_sub(t0).max(1);
                dep[r][si] = Point {
                    x: *size as f64,
                    y: d as f64 / iters as f64,
                };
                run_ns[r] = run_ns[r].saturating_add(d);
                run_iters[r] = run_iters[r].saturating_add(iters);
            }
            // Independent loads over the same pages, as a reference curve:
            // here the memory-level parallelism can hide page-walk latency.
            let mut iters_i = 1000u64;
            let mut ns = 1u64;
            for _ in 0..16 {
                let t0 = time::mono_ns();
                let _ = unsafe { vmbench_k_tlb(iters_i, &args as *const TlbArgs as usize) };
                ns = time::mono_ns().wrapping_sub(t0).max(1);
                if ns >= target / 4 {
                    break;
                }
                iters_i = iters_i.saturating_mul(4);
            }
            let scaled = iters_i as u128 * target as u128 / ns as u128;
            let iters_i = if scaled > 20_000_000 {
                20_000_000
            } else {
                (scaled as u64).max(1000)
            };
            for r in 0..nruns {
                let t0 = time::mono_ns();
                let _ = unsafe { vmbench_k_tlb(iters_i, &args as *const TlbArgs as usize) };
                let d = time::mono_ns().wrapping_sub(t0).max(1);
                ind[r][si] = Point {
                    x: *size as f64,
                    y: d as f64 / iters_i as f64,
                };
                run_ns[r] = run_ns[r].saturating_add(d);
                run_iters[r] = run_iters[r].saturating_add(iters_i);
            }
        }
        for r in 0..nruns {
            ctx.begin_run(entry, run_ns[r], run_iters[r]);
            ctx.curve(entry, "ns_per_access", &dep[r][..nvalid]);
            ctx.curve(entry, "ns_per_access_independent", &ind[r][..nvalid]);
            ctx.end_run(entry);
        }
        Ok(())
    }
}

pub struct NumaLatency;

/// True when at least one NUMA node accepts `mbind(MPOL_BIND)`. Binding can be
/// denied even when topology is exposed (for example by container policy), in
/// which case the benchmark cannot produce a meaningful local/remote split.
fn numa_bind_available(ctx: &Ctx) -> bool {
    for node in ctx.env.numa_nodes.iter().take(8) {
        let Some(p) = crate::sys::mmap_anon(4096, false) else {
            continue;
        };
        let bit = (node.id as usize) / 64;
        let mut mask = [0u64; 8];
        if bit < 8 {
            mask[bit] |= 1u64 << (node.id % 64);
        }
        let ok = crate::sys::mbind(p, 4096, crate::sys::MPOL_BIND, &mask, 0).is_ok();
        let _ = crate::sys::munmap(p, 4096);
        if ok {
            return true;
        }
    }
    false
}

impl Benchmark for NumaLatency {
    fn meta(&self) -> &'static EntryMeta {
        static M: EntryMeta = EntryMeta {
            id: "memory.numa.v1",
            version: 1,
            description: "NUMA local/remote memory latency (requires exposed NUMA topology)",
            imp: ImplInfo {
                source: "src/bench/mem_latency.rs",
                kernel: "vmbench_k_chase",
                algorithm: "pointer chase over a buffer bound to each NUMA node with mbind(MPOL_BIND)",
                isa: BASELINE_ISA,
            },
            metrics: &[M_NS_ACCESS],
        };
        &M
    }

    fn supported(&self, ctx: &Ctx) -> Result<(), &'static str> {
        if ctx.env.numa_nodes.len() < 2 {
            return Err("NUMA topology unavailable");
        }
        if !numa_bind_available(ctx) {
            return Err("NUMA memory binding unavailable");
        }
        Ok(())
    }

    fn run(&self, ctx: &mut Ctx, entry: usize) -> Result<(), &'static str> {
        let cap = mem_cap(ctx);
        let size = (256 * 1024 * 1024).min(cap).max(64 * 1024 * 1024);
        let cpu = ctx.default_cpu();
        ctx.pin_self(cpu);
        ctx.add_param(entry, "working_set_bytes", ParamValue::Int(size as i64));
        let nruns = ctx.cfg.runs.clamp(1, 8) as usize;
        let mut pts = [[Point { x: 0.0, y: 0.0 }; 8]; 8];
        let mut run_ns = [0u64; 8];
        let mut rng = crate::rand::Rng::new(0x9999_8888_7777_6666);
        let mut nvalid = 0usize;
        for node in ctx.env.numa_nodes.iter().take(8) {
            let buf = match BigBuf::new(size) {
                Some(b) => b,
                None => continue,
            };
            let bit = (node.id as usize) / 64;
            let mut mask = [0u64; 8];
            if bit < 8 {
                mask[bit] |= 1u64 << (node.id % 64);
            }
            if crate::sys::mbind(buf.as_ptr(), buf.len, crate::sys::MPOL_BIND, &mask, 0).is_err() {
                continue;
            }
            buf.touch();
            let start = build_cycle(ctx.arena, buf.as_ptr(), size / LINE, &mut rng);
            let iters = calibrate_chase(2000, start, ctx.cfg.target_run_ns / 2);
            for r in 0..nruns {
                let t0 = time::mono_ns();
                let _ = unsafe { vmbench_k_chase(iters, start) };
                let d = time::mono_ns().wrapping_sub(t0).max(1);
                pts[r][nvalid] = Point {
                    x: node.id as f64,
                    y: d as f64 / iters as f64,
                };
                run_ns[r] = run_ns[r].saturating_add(d);
            }
            nvalid += 1;
        }
        if nvalid == 0 {
            return Err("mbind failed for all nodes");
        }
        for r in 0..nruns {
            ctx.begin_run(entry, run_ns[r], 0);
            ctx.curve(entry, "ns_per_access", &pts[r][..nvalid]);
            ctx.end_run(entry);
        }
        Ok(())
    }
}

pub struct PageSize;

impl Benchmark for PageSize {
    fn meta(&self) -> &'static EntryMeta {
        static M: EntryMeta = EntryMeta {
            id: "memory.page_size.v1",
            version: 1,
            description: "DRAM chase latency with 4 KiB pages vs transparent huge pages",
            imp: ImplInfo {
                source: "src/bench/mem_latency.rs",
                kernel: "vmbench_k_chase",
                algorithm: "same random pointer chase with MADV_NOHUGEPAGE vs MADV_HUGEPAGE",
                isa: BASELINE_ISA,
            },
            metrics: &[M_NS_ACCESS],
        };
        &M
    }

    fn supported(&self, ctx: &Ctx) -> Result<(), &'static str> {
        match ctx.env.thp_enabled {
            Some(s) => {
                if let Some(open) = s.find('[') {
                    let rest = &s[open + 1..];
                    if let Some(close) = rest.find(']') {
                        let mode = &rest[..close];
                        if mode == "always" || mode == "madvise" {
                            return Ok(());
                        }
                    }
                }
                Err("transparent huge pages disabled")
            }
            None => Err("THP status unavailable"),
        }
    }

    fn run(&self, ctx: &mut Ctx, entry: usize) -> Result<(), &'static str> {
        let cap = mem_cap(ctx);
        let size = (256 * 1024 * 1024).min(cap).max(64 * 1024 * 1024);
        let cpu = ctx.default_cpu();
        ctx.pin_self(cpu);
        ctx.add_param(entry, "working_set_bytes", ParamValue::Int(size as i64));
        let nruns = ctx.cfg.runs.clamp(1, 8) as usize;
        let mut pts = [[Point { x: 0.0, y: 0.0 }; 2]; 8];
        let mut run_ns = [0u64; 8];
        let mut rng = crate::rand::Rng::new(0xabcdef01_23456789);
        for (pi, (page, advice)) in [(4096u64, crate::sys::MADV_NOHUGEPAGE), (2 << 20, crate::sys::MADV_HUGEPAGE)]
            .iter()
            .enumerate()
        {
            let buf = BigBuf::new(size).ok_or("mmap failed")?;
            let _ = crate::sys::madvise(buf.as_ptr(), buf.len, *advice);
            buf.touch();
            let start = build_cycle(ctx.arena, buf.as_ptr(), size / LINE, &mut rng);
            let iters = calibrate_chase(2000, start, ctx.cfg.target_run_ns / 2);
            let _ = unsafe { vmbench_k_chase(iters, start) };
            for r in 0..nruns {
                let t0 = time::mono_ns();
                let _ = unsafe { vmbench_k_chase(iters, start) };
                let d = time::mono_ns().wrapping_sub(t0).max(1);
                pts[r][pi] = Point {
                    x: *page as f64,
                    y: d as f64 / iters as f64,
                };
                run_ns[r] = run_ns[r].saturating_add(d);
            }
        }
        for r in 0..nruns {
            ctx.begin_run(entry, run_ns[r], 0);
            ctx.curve(entry, "ns_per_access", &pts[r][..2]);
            ctx.end_run(entry);
        }
        Ok(())
    }
}
