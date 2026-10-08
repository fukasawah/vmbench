use core::mem::MaybeUninit;

use crate::elfself::SelfImage;
use crate::env::EnvInfo;
pub use crate::stats::Direction;
use crate::stats as statslib;
use crate::time::{self, mono_ns, Deadline};
use crate::util::Arena;

pub const MAX_METRICS: usize = 16;
pub const MAX_PARAMS: usize = 8;
pub const MAX_RUNS: usize = 8;
pub const MAX_ENTRIES: usize = 512;
pub const MAX_CURVE_POINTS: usize = 96;
pub const MAX_STORAGE_TARGETS: usize = 8;

/// One storage measurement location. `auto_dir` targets are created (and
/// removed) by vmbench itself; explicit targets must already exist.
#[derive(Clone, Copy)]
pub struct StorageTarget {
    pub name: &'static str,
    pub path: &'static str,
    pub auto_dir: bool,
}

impl Default for StorageTarget {
    fn default() -> Self {
        StorageTarget {
            name: "",
            path: "",
            auto_dir: false,
        }
    }
}

#[derive(Clone, Copy)]
pub struct MetricDef {
    pub key: &'static str,
    pub unit: &'static str,
    pub direction: Direction,
    pub kind: MetricKind,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum MetricKind {
    Scalar,
    Curve,
}

#[derive(Clone, Copy)]
pub struct ImplInfo {
    pub source: &'static str,
    pub kernel: &'static str,
    pub algorithm: &'static str,
    pub isa: &'static str,
}

#[derive(Clone, Copy)]
pub struct EntryMeta {
    pub id: &'static str,
    pub version: u32,
    pub description: &'static str,
    pub imp: ImplInfo,
    pub metrics: &'static [MetricDef],
}

#[derive(Clone, Copy)]
pub enum ParamValue {
    Int(i64),
    Num(f64),
    Str(&'static str),
    Bool(bool),
}

#[derive(Clone, Copy)]
pub struct Param {
    pub key: &'static str,
    pub value: ParamValue,
}

#[derive(Clone, Copy, Default)]
pub struct Point {
    pub x: f64,
    pub y: f64,
}

#[derive(Clone, Copy, Default)]
pub struct CurveRef {
    pub ptr: *const Point,
    pub len: usize,
}

impl CurveRef {
    pub fn points(&self) -> &[Point] {
        if self.ptr.is_null() {
            &[]
        } else {
            unsafe { core::slice::from_raw_parts(self.ptr, self.len) }
        }
    }
}

#[derive(Clone, Copy, Default)]
pub struct Run {
    pub duration_ns: u64,
    /// Number of work units (operations, bytes, ...) counted during the run.
    pub work_units: u64,
    /// Per-chunk rate distribution within this run (0 when unavailable).
    pub chunk_rate_min: f64,
    pub chunk_rate_median: f64,
    pub chunk_rate_max: f64,
    pub chunks: u32,
    pub present: u16,
    pub scalars: [f64; MAX_METRICS],
    pub curves: [CurveRef; MAX_METRICS],
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Status {
    Ok,
    Unsupported,
    VerifyFailed,
    Failed,
}

pub struct Entry {
    pub meta: &'static EntryMeta,
    pub status: Status,
    pub reason: &'static str,
    pub params: [Param; MAX_PARAMS],
    pub n_params: usize,
    pub runs: [Run; MAX_RUNS],
    pub n_runs: usize,
}

impl Entry {
    fn empty(meta: &'static EntryMeta) -> Entry {
        Entry {
            meta,
            status: Status::Ok,
            reason: "",
            params: [Param {
                key: "",
                value: ParamValue::Bool(false),
            }; MAX_PARAMS],
            n_params: 0,
            runs: [Run::default(); MAX_RUNS],
            n_runs: 0,
        }
    }
}

static mut ENTRIES: [MaybeUninit<Entry>; MAX_ENTRIES] =
    [const { MaybeUninit::uninit() }; MAX_ENTRIES];

fn entry_slot(idx: usize) -> *mut Entry {
    unsafe {
        let base = core::ptr::addr_of_mut!(ENTRIES) as *mut MaybeUninit<Entry>;
        (*base.add(idx)).as_mut_ptr()
    }
}

pub fn entry_ptr(idx: usize) -> *const Entry {
    entry_slot(idx)
}

#[derive(Clone)]
pub struct Config {
    pub quick: bool,
    pub target_run_ns: u64,
    pub max_bench_ns: u64,
    pub sustained_ns: u64,
    pub pin_cpu: Option<u32>,
    pub no_pin: bool,
    pub no_progress: bool,
    pub only: Option<&'static str>,
    pub skip: Option<&'static str>,
    /// Independent repetitions per benchmark (normal 5, quick 1).
    pub runs: u32,
    /// Storage measurement locations (see `--storage`).
    pub storage_targets: [StorageTarget; MAX_STORAGE_TARGETS],
    pub n_storage_targets: usize,
    /// Do not add the implicit `cwd` storage target.
    pub no_cwd_storage: bool,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            quick: false,
            target_run_ns: 300_000_000,
            max_bench_ns: 9_000_000_000,
            sustained_ns: 10_000_000_000,
            pin_cpu: None,
            no_pin: false,
            no_progress: false,
            only: None,
            skip: None,
            runs: 5,
            storage_targets: [StorageTarget::default(); MAX_STORAGE_TARGETS],
            n_storage_targets: 0,
            no_cwd_storage: false,
        }
    }
}

pub struct Ctx {
    pub cfg: Config,
    pub arena: &'static mut Arena,
    pub env: EnvInfo,
    pub n_entries: usize,
    pub code_hashes: &'static [(&'static str, Option<[u8; 32]>)],
    pub self_image: Option<&'static SelfImage>,
    pub annotations: &'static mut Vec32,
    pub progress: bool,
    /// Cached per-iteration loop overhead in cycles (measured once).
    pub loop_overhead_cycles: Option<f64>,
    /// Lazily created storage test file environments, one per target.
    pub storage: [Option<&'static mut crate::bench::storage::StorageEnv>; MAX_STORAGE_TARGETS],
    /// Per-target state published in the report (fstype / status).
    pub storage_state: [crate::bench::storage::TargetState; MAX_STORAGE_TARGETS],
}

pub struct Vec32 {
    pub items: [&'static str; 32],
    pub len: usize,
}

impl Vec32 {
    pub fn push(&mut self, s: &'static str) {
        if self.len < 32 {
            self.items[self.len] = s;
            self.len += 1;
        }
    }

    pub fn as_slice(&self) -> &[&'static str] {
        &self.items[..self.len]
    }
}

pub struct Sample {
    pub duration_ns: u64,
    pub cycles: u64,
    /// Work units executed during this run (operations, bytes, iterations).
    pub units: u64,
}

impl Ctx {
    unsafe fn entry(&self, idx: usize) -> &'static mut Entry {
        &mut *entry_slot(idx)
    }

    pub fn begin_entry(&mut self, meta: &'static EntryMeta) -> usize {
        let idx = self.n_entries;
        if idx >= MAX_ENTRIES {
            crate::rt::fatal("too many benchmark entries (MAX_ENTRIES)");
        }
        unsafe {
            core::ptr::write(entry_slot(idx), Entry::empty(meta));
        }
        self.n_entries = idx + 1;
        if self.progress {
            crate::app::progress_step(meta.id);
        }
        idx
    }

    pub fn mark_unsupported(&mut self, idx: usize, reason: &'static str) {
        let e = unsafe { self.entry(idx) };
        e.status = Status::Unsupported;
        e.reason = reason;
    }

    pub fn mark_failed(&mut self, idx: usize, reason: &'static str) {
        let e = unsafe { self.entry(idx) };
        e.status = Status::Failed;
        e.reason = reason;
    }

    pub fn mark_verify_failed(&mut self, idx: usize, reason: &'static str) {
        let e = unsafe { self.entry(idx) };
        e.status = Status::VerifyFailed;
        e.reason = reason;
    }

    pub fn add_param(&mut self, idx: usize, key: &'static str, value: ParamValue) {
        let e = unsafe { self.entry(idx) };
        if e.n_params < MAX_PARAMS {
            e.params[e.n_params] = Param { key, value };
            e.n_params += 1;
        }
    }

    pub fn begin_run(&mut self, idx: usize, duration_ns: u64, work_units: u64) {
        let e = unsafe { self.entry(idx) };
        if e.n_runs < MAX_RUNS {
            e.runs[e.n_runs] = Run {
                duration_ns,
                work_units,
                ..Run::default()
            };
            e.n_runs += 1;
        }
    }

    pub fn end_run(&mut self, _idx: usize) {}

    pub fn set_chunk_stats(&mut self, idx: usize, min: f64, median: f64, max: f64, chunks: u32) {
        let e = unsafe { self.entry(idx) };
        let r = e.n_runs.saturating_sub(1);
        if r < MAX_RUNS {
            e.runs[r].chunk_rate_min = min;
            e.runs[r].chunk_rate_median = median;
            e.runs[r].chunk_rate_max = max;
            e.runs[r].chunks = chunks;
        }
    }

    /// Measures the cost of the benchmark loop skeleton once and caches it.
    /// Only the throughput kernels use it (as a supplementary net metric);
    /// latency kernels hide loop control behind their dependency chain.
    pub fn loop_overhead_cycles(&mut self) -> Option<f64> {
        if let Some(v) = self.loop_overhead_cycles {
            return Some(v);
        }
        if time::cycles_hz() == 0 {
            self.loop_overhead_cycles = Some(0.0);
            return Some(0.0);
        }
        let f = crate::bench::cpu_overhead::overhead_kernel();
        let goal = if self.cfg.quick {
            2_000_000
        } else {
            5_000_000
        };
        let chunk = calibrate_chunk(goal, |n| {
            let _ = unsafe { f(n, 0) };
            n
        });
        let c0 = time::cycles();
        let _ = unsafe { f(chunk, 0) };
        let c1 = time::cycles();
        core::hint::black_box(chunk);
        let per = if chunk > 0 {
            c1.wrapping_sub(c0) as f64 / chunk as f64
        } else {
            0.0
        };
        self.loop_overhead_cycles = Some(per);
        Some(per)
    }

    fn metric_index(e: &Entry, key: &str) -> Option<usize> {
        e.meta.metrics.iter().position(|m| m.key == key)
    }

    pub fn scalar(&mut self, idx: usize, key: &'static str, value: f64) {
        let e = unsafe { self.entry(idx) };
        let r = e.n_runs.saturating_sub(1);
        if let Some(mi) = Self::metric_index(e, key) {
            e.runs[r].present |= 1 << mi;
            e.runs[r].scalars[mi] = value;
        }
    }

    pub fn curve(&mut self, idx: usize, key: &'static str, points: &[Point]) {
        let copied: &'static [Point] = match unsafe { self.arena.alloc_slice::<Point>(points.len()) }
        {
            Some(s) => {
                s.copy_from_slice(points);
                s
            }
            None => {
                crate::rt::fatal("arena exhausted while storing curve");
            }
        };
        let e = unsafe { self.entry(idx) };
        let r = e.n_runs.saturating_sub(1);
        if let Some(mi) = Self::metric_index(e, key) {
            e.runs[r].present |= 1 << mi;
            e.runs[r].curves[mi] = CurveRef {
                ptr: copied.as_ptr(),
                len: copied.len(),
            };
        }
    }

    pub fn annotate(&mut self, s: &'static str) {
        self.annotations.push(s);
    }

    pub fn code_hash(&self, kernel: &str) -> Option<[u8; 32]> {
        self.code_hashes
            .iter()
            .find(|(k, _)| *k == kernel)
            .and_then(|(_, h)| *h)
    }

    pub fn cpu_count(&self) -> usize {
        self.env.online_cpus.len().max(1)
    }

    pub fn cpu_at(&self, i: usize) -> u32 {
        let n = self.env.online_cpus.len();
        if n == 0 {
            0
        } else {
            self.env.online_cpus[i % n]
        }
    }

    pub fn pin_self(&self, cpu: u32) {
        let mut mask = [0u8; 128];
        mask[(cpu / 8) as usize] |= 1 << (cpu % 8);
        let _ = crate::sys::sched_setaffinity(0, &mask);
    }

    pub fn unpin_self(&self) {
        let mut mask = [0u8; 128];
        for &c in self.env.online_cpus {
            mask[(c / 8) as usize] |= 1 << (c % 8);
        }
        let _ = crate::sys::sched_setaffinity(0, &mask);
    }

    pub fn default_cpu(&self) -> u32 {
        self.cfg.pin_cpu.unwrap_or_else(|| self.cpu_at(0))
    }

    pub fn want(&self, id: &str) -> bool {
        if let Some(only) = self.cfg.only {
            if !pattern_match(only, id) {
                return false;
            }
        }
        if let Some(skip) = self.cfg.skip {
            if pattern_match(skip, id) {
                return false;
            }
        }
        true
    }

    /// Time-budgeted measurement.
    ///
    /// A chunk of work that takes roughly `target/8` is calibrated first, then
    /// chunks are repeated until the target duration has elapsed. The wall
    /// clock time of a run is therefore bounded by construction, no matter how
    /// slow the machine (or how contended a shared cache line) is. Clock reads
    /// happen once per chunk, so their overhead is negligible.
    pub fn measure<F, E>(
        &mut self,
        entry: usize,
        target_ns: u64,
        n_runs: u32,
        mut body: F,
        mut emit: E,
    ) where
        F: FnMut(u64) -> u64,
        E: FnMut(&mut Ctx, usize, &Sample),
    {
        let target = target_ns.max(1_000_000);
        let goal = (target / 8).clamp(2_000_000, 20_000_000);
        let mut chunk = calibrate_chunk(goal, &mut body);
        let _ = body(chunk); // warmup
        let mut total = 0u64;
        let hz = time::cycles_hz();
        for _ in 0..n_runs {
            let t0 = mono_ns();
            let c0 = time::cycles();
            let deadline = Deadline::new(target);
            let mut units = 0u64;
            let mut rates = [0f64; 64];
            let mut nrates = 0usize;
            loop {
                let cc0 = time::cycles();
                let u = body(chunk);
                let cc1 = time::cycles();
                units = units.saturating_add(u);
                if nrates < rates.len() && hz > 0 && cc1 > cc0 {
                    rates[nrates] = u as f64 * hz as f64 / cc1.wrapping_sub(cc0) as f64;
                    nrates += 1;
                }
                if deadline.expired() {
                    break;
                }
            }
            let t1 = mono_ns();
            let c1 = time::cycles();
            let s = Sample {
                duration_ns: t1.wrapping_sub(t0),
                cycles: c1.wrapping_sub(c0),
                units,
            };
            self.begin_run(entry, s.duration_ns, s.units);
            emit(self, entry, &s);
            if nrates >= 4 {
                rates[..nrates].sort_unstable_by(|a, b| {
                    a.partial_cmp(b).unwrap_or(core::cmp::Ordering::Equal)
                });
                let med = if nrates % 2 == 1 {
                    rates[nrates / 2]
                } else {
                    (rates[nrates / 2 - 1] + rates[nrates / 2]) / 2.0
                };
                self.set_chunk_stats(entry, rates[0], med, rates[nrates - 1], nrates as u32);
            }
            self.end_run(entry);
            total = total.saturating_add(s.duration_ns);
            if s.duration_ns > target.saturating_mul(4) {
                chunk = (chunk / 4).max(1);
            }
            if total > self.cfg.max_bench_ns {
                break;
            }
        }
    }
}

/// Finds an iteration count whose execution takes about `goal_ns`. Bounded to
/// 40 steps; each step costs at most the goal plus clock overhead.
pub fn calibrate_chunk<F: FnMut(u64) -> u64>(goal_ns: u64, mut body: F) -> u64 {
    let goal = goal_ns.max(100_000);
    let mut iters: u64 = 1;
    for _ in 0..40 {
        let t0 = mono_ns();
        let units = body(iters);
        let ns = mono_ns().wrapping_sub(t0).max(1);
        core::hint::black_box(units);
        if ns >= goal {
            break;
        }
        let scaled = iters as u128 * goal as u128 / ns as u128;
        let next = if scaled > 1_000_000_000_000u128 {
            1_000_000_000_000u64
        } else {
            (scaled as u64).max(iters.saturating_mul(2).max(1))
        };
        if next == iters {
            break;
        }
        iters = next;
    }
    iters
}

fn pattern_match(pattern: &str, id: &str) -> bool {
    for part in pattern.split(',') {
        let p = part.trim();
        if p.is_empty() {
            continue;
        }
        if p == "all" || id.starts_with(p) {
            return true;
        }
    }
    false
}

pub fn scalar_series(entry: &Entry, mi: usize) -> ([f64; MAX_RUNS], usize) {
    let mut vals = [0f64; MAX_RUNS];
    let mut n = 0;
    for r in &entry.runs[..entry.n_runs] {
        if r.present & (1 << mi) != 0 {
            vals[n] = r.scalars[mi];
            n += 1;
        }
    }
    (vals, n)
}

pub fn curve_runs(entry: &Entry, mi: usize) -> ([&[Point]; MAX_RUNS], usize) {
    let mut runs: [&[Point]; MAX_RUNS] = [&[]; MAX_RUNS];
    let mut n = 0;
    for r in &entry.runs[..entry.n_runs] {
        if r.present & (1 << mi) == 0 {
            continue;
        }
        let pts = r.curves[mi].points();
        if pts.is_empty() {
            continue;
        }
        runs[n] = pts;
        n += 1;
    }
    (runs, n)
}

pub fn run_all(ctx: &mut Ctx) {
    // Validate storage targets before any benchmark runs, so a bad path is
    // reported immediately instead of after the CPU suite (see README).
    let mut want_storage = false;
    for b in crate::bench::registry() {
        let meta = b.meta();
        if meta.id.starts_with("storage.")
            && ctx.want(meta.id)
            && !(ctx.cfg.quick && b.quick_skip())
        {
            want_storage = true;
            break;
        }
    }
    if want_storage && ctx.cfg.n_storage_targets > 0 {
        crate::bench::storage::preflight(ctx);
    }

    for b in crate::bench::registry() {
        let meta = b.meta();
        if !ctx.want(meta.id) {
            continue;
        }
        if ctx.cfg.quick && b.quick_skip() {
            continue;
        }
        let ninst = b.instances(ctx);
        if ninst == 0 {
            continue;
        }
        let first = ctx.begin_entry(meta);
        if let Err(reason) = b.supported(ctx) {
            ctx.mark_unsupported(first, reason);
            continue;
        }
        // Known-answer verification runs before measurement. Kernels are
        // #[inline(never)] and reached through the same call path in both
        // phases, so a passing check applies to the measured machine code.
        if let Err(reason) = crate::bench::verify::verify_benchmark(meta.id, ctx) {
            ctx.mark_verify_failed(first, reason);
            continue;
        }
        for inst in 0..ninst {
            let idx = if inst == 0 {
                first
            } else {
                ctx.begin_entry(meta)
            };
            match b.run_at(ctx, idx, inst) {
                Ok(()) => {
                    let e = unsafe { &*entry_ptr(idx) };
                    if e.n_runs == 0 {
                        ctx.mark_failed(idx, "no measurements produced");
                    }
                }
                Err(reason) => ctx.mark_failed(idx, reason),
            }
        }
    }
}

pub fn compute_scalar_stats(entry: &Entry, mi: usize) -> Option<statslib::ScalarStats> {
    let (vals, n) = scalar_series(entry, mi);
    if n == 0 {
        return None;
    }
    let dir = entry.meta.metrics[mi].direction;
    Some(statslib::compute(&vals[..n], dir))
}
