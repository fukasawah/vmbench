#![allow(unused_mut, unused_assignments)]
use crate::bench::common::*;
use crate::bench::Benchmark;
use crate::runner::{Ctx, EntryMeta, ImplInfo, ParamValue};

pub const OPS_PER_ITER: u64 = 16;

// ---------------------------------------------------------------------------
// FP throughput kernels (inline asm, 8 independent accumulators).
// ---------------------------------------------------------------------------

#[cfg(target_arch = "x86_64")]
macro_rules! fp_tp_kernel {
    ($fname:ident, $mn:literal, $ty:ty, $setup:path, $extract:path) => {
        #[inline(never)]
        #[no_mangle]
        pub unsafe extern "C" fn $fname(iters: u64, seed: u64) -> f64 {
            let mut x0 = $setup(seed as $ty);
            let mut x1 = $setup((seed ^ 0x1111) as $ty);
            let mut x2 = $setup((seed ^ 0x2222) as $ty);
            let mut x3 = $setup((seed ^ 0x3333) as $ty);
            let mut x4 = $setup((seed ^ 0x4444) as $ty);
            let mut x5 = $setup((seed ^ 0x5555) as $ty);
            let mut x6 = $setup((seed ^ 0x6666) as $ty);
            let mut x7 = $setup((seed ^ 0x7777) as $ty);
            let k = $setup(1.0000001 as $ty);
            core::arch::asm!(
                "test {i}, {i}",
                "jz 3f",
                "2:",
                concat!(
                    $mn, " {x0}, {k}\n", $mn, " {x1}, {k}\n", $mn, " {x2}, {k}\n", $mn, " {x3}, {k}\n",
                    $mn, " {x4}, {k}\n", $mn, " {x5}, {k}\n", $mn, " {x6}, {k}\n", $mn, " {x7}, {k}\n",
                    $mn, " {x0}, {k}\n", $mn, " {x1}, {k}\n", $mn, " {x2}, {k}\n", $mn, " {x3}, {k}\n",
                    $mn, " {x4}, {k}\n", $mn, " {x5}, {k}\n", $mn, " {x6}, {k}\n", $mn, " {x7}, {k}\n",
                ),
                "dec {i}",
                "jnz 2b",
                "3:",
                concat!($mn, " {x0}, {x1}"),
                concat!($mn, " {x2}, {x3}"),
                concat!($mn, " {x4}, {x5}"),
                concat!($mn, " {x6}, {x7}"),
                concat!($mn, " {x0}, {x2}"),
                concat!($mn, " {x4}, {x6}"),
                concat!($mn, " {x0}, {x4}"),
                i = in(reg) iters,
                k = in(xmm_reg) k,
                x0 = inout(xmm_reg) x0 => x0,
                x1 = inout(xmm_reg) x1 => _,
                x2 = inout(xmm_reg) x2 => _,
                x3 = inout(xmm_reg) x3 => _,
                x4 = inout(xmm_reg) x4 => _,
                x5 = inout(xmm_reg) x5 => _,
                x6 = inout(xmm_reg) x6 => _,
                x7 = inout(xmm_reg) x7 => _,
                options(nostack)
            );
            $extract(x0) as f64
        }
    };
}

#[cfg(target_arch = "x86_64")]
macro_rules! fp_fma_kernel {
    ($fname:ident, $mn:literal, $ty:ty, $setup:path, $extract:path) => {
        #[target_feature(enable = "fma")]
        #[inline(never)]
        #[no_mangle]
        pub unsafe extern "C" fn $fname(iters: u64, seed: u64) -> f64 {
            let mut x0 = $setup(seed as $ty);
            let mut x1 = $setup((seed ^ 0x1111) as $ty);
            let mut x2 = $setup((seed ^ 0x2222) as $ty);
            let mut x3 = $setup((seed ^ 0x3333) as $ty);
            let mut x4 = $setup((seed ^ 0x4444) as $ty);
            let mut x5 = $setup((seed ^ 0x5555) as $ty);
            let mut x6 = $setup((seed ^ 0x6666) as $ty);
            let mut x7 = $setup((seed ^ 0x7777) as $ty);
            let k = $setup(1.0000001 as $ty);
            core::arch::asm!(
                "test {i}, {i}",
                "jz 3f",
                "2:",
                concat!(
                    $mn, " {x0}, {x0}, {k}\n", $mn, " {x1}, {x1}, {k}\n",
                    $mn, " {x2}, {x2}, {k}\n", $mn, " {x3}, {x3}, {k}\n",
                    $mn, " {x4}, {x4}, {k}\n", $mn, " {x5}, {x5}, {k}\n",
                    $mn, " {x6}, {x6}, {k}\n", $mn, " {x7}, {x7}, {k}\n",
                    $mn, " {x0}, {x0}, {k}\n", $mn, " {x1}, {x1}, {k}\n",
                    $mn, " {x2}, {x2}, {k}\n", $mn, " {x3}, {x3}, {k}\n",
                    $mn, " {x4}, {x4}, {k}\n", $mn, " {x5}, {x5}, {k}\n",
                    $mn, " {x6}, {x6}, {k}\n", $mn, " {x7}, {x7}, {k}\n",
                ),
                "dec {i}",
                "jnz 2b",
                "3:",
                concat!($mn, " {x0}, {x0}, {x1}"),
                concat!($mn, " {x2}, {x2}, {x3}"),
                concat!($mn, " {x4}, {x4}, {x5}"),
                concat!($mn, " {x6}, {x6}, {x7}"),
                concat!($mn, " {x0}, {x0}, {x2}"),
                concat!($mn, " {x4}, {x4}, {x6}"),
                concat!($mn, " {x0}, {x0}, {x4}"),
                i = in(reg) iters,
                k = in(xmm_reg) k,
                x0 = inout(xmm_reg) x0 => x0,
                x1 = inout(xmm_reg) x1 => _,
                x2 = inout(xmm_reg) x2 => _,
                x3 = inout(xmm_reg) x3 => _,
                x4 = inout(xmm_reg) x4 => _,
                x5 = inout(xmm_reg) x5 => _,
                x6 = inout(xmm_reg) x6 => _,
                x7 = inout(xmm_reg) x7 => _,
                options(nostack)
            );
            $extract(x0) as f64
        }
    };
}

#[cfg(target_arch = "aarch64")]
macro_rules! fp_tp_kernel {
    ($fname:ident, $mn:literal) => {
        #[inline(never)]
        #[no_mangle]
        pub unsafe extern "C" fn $fname(iters: u64, seed: u64) -> f64 {
            let mut x0 = seed as f64;
            let mut x1 = (seed ^ 0x1111) as f64;
            let mut x2 = (seed ^ 0x2222) as f64;
            let mut x3 = (seed ^ 0x3333) as f64;
            let mut x4 = (seed ^ 0x4444) as f64;
            let mut x5 = (seed ^ 0x5555) as f64;
            let mut x6 = (seed ^ 0x6666) as f64;
            let mut x7 = (seed ^ 0x7777) as f64;
            let k = 1.0000001f64;
            core::arch::asm!(
                "cbz {i}, 3f",
                "2:",
                concat!(
                    $mn, " {x0:d}, {x0:d}, {k:d}\n", $mn, " {x1:d}, {x1:d}, {k:d}\n",
                    $mn, " {x2:d}, {x2:d}, {k:d}\n", $mn, " {x3:d}, {x3:d}, {k:d}\n",
                    $mn, " {x4:d}, {x4:d}, {k:d}\n", $mn, " {x5:d}, {x5:d}, {k:d}\n",
                    $mn, " {x6:d}, {x6:d}, {k:d}\n", $mn, " {x7:d}, {x7:d}, {k:d}\n",
                    $mn, " {x0:d}, {x0:d}, {k:d}\n", $mn, " {x1:d}, {x1:d}, {k:d}\n",
                    $mn, " {x2:d}, {x2:d}, {k:d}\n", $mn, " {x3:d}, {x3:d}, {k:d}\n",
                    $mn, " {x4:d}, {x4:d}, {k:d}\n", $mn, " {x5:d}, {x5:d}, {k:d}\n",
                    $mn, " {x6:d}, {x6:d}, {k:d}\n", $mn, " {x7:d}, {x7:d}, {k:d}\n",
                ),
                "subs {i}, {i}, #1",
                "b.ne 2b",
                "3:",
                "fadd {x0:d}, {x0:d}, {x1:d}",
                "fadd {x2:d}, {x2:d}, {x3:d}",
                "fadd {x4:d}, {x4:d}, {x5:d}",
                "fadd {x6:d}, {x6:d}, {x7:d}",
                "fadd {x0:d}, {x0:d}, {x2:d}",
                "fadd {x4:d}, {x4:d}, {x6:d}",
                "fadd {x0:d}, {x0:d}, {x4:d}",
                i = in(reg) iters,
                k = in(vreg) k,
                x0 = inout(vreg) x0 => x0,
                x1 = inout(vreg) x1 => x1,
                x2 = inout(vreg) x2 => x2,
                x3 = inout(vreg) x3 => x3,
                x4 = inout(vreg) x4 => x4,
                x5 = inout(vreg) x5 => x5,
                x6 = inout(vreg) x6 => x6,
                x7 = inout(vreg) x7 => x7,
                options(nostack)
            );
            x0
        }
    };
}

#[cfg(target_arch = "aarch64")]
macro_rules! fp_fma_kernel {
    ($fname:ident) => {
        #[inline(never)]
        #[no_mangle]
        pub unsafe extern "C" fn $fname(iters: u64, seed: u64) -> f64 {
            let mut x0 = seed as f64;
            let mut x1 = (seed ^ 0x1111) as f64;
            let mut x2 = (seed ^ 0x2222) as f64;
            let mut x3 = (seed ^ 0x3333) as f64;
            let mut x4 = (seed ^ 0x4444) as f64;
            let mut x5 = (seed ^ 0x5555) as f64;
            let mut x6 = (seed ^ 0x6666) as f64;
            let mut x7 = (seed ^ 0x7777) as f64;
            let k = 1.0000001f64;
            core::arch::asm!(
                "cbz {i}, 3f",
                "2:",
                concat!(
                    "fmadd {x0:d}, {x0:d}, {x0:d}, {k:d}\n", "fmadd {x1:d}, {x1:d}, {x1:d}, {k:d}\n",
                    "fmadd {x2:d}, {x2:d}, {x2:d}, {k:d}\n", "fmadd {x3:d}, {x3:d}, {x3:d}, {k:d}\n",
                    "fmadd {x4:d}, {x4:d}, {x4:d}, {k:d}\n", "fmadd {x5:d}, {x5:d}, {x5:d}, {k:d}\n",
                    "fmadd {x6:d}, {x6:d}, {x6:d}, {k:d}\n", "fmadd {x7:d}, {x7:d}, {x7:d}, {k:d}\n",
                    "fmadd {x0:d}, {x0:d}, {x0:d}, {k:d}\n", "fmadd {x1:d}, {x1:d}, {x1:d}, {k:d}\n",
                    "fmadd {x2:d}, {x2:d}, {x2:d}, {k:d}\n", "fmadd {x3:d}, {x3:d}, {x3:d}, {k:d}\n",
                    "fmadd {x4:d}, {x4:d}, {x4:d}, {k:d}\n", "fmadd {x5:d}, {x5:d}, {x5:d}, {k:d}\n",
                    "fmadd {x6:d}, {x6:d}, {x6:d}, {k:d}\n", "fmadd {x7:d}, {x7:d}, {x7:d}, {k:d}\n",
                ),
                "subs {i}, {i}, #1",
                "b.ne 2b",
                "3:",
                "fadd {x0:d}, {x0:d}, {x1:d}",
                "fadd {x2:d}, {x2:d}, {x3:d}",
                "fadd {x4:d}, {x4:d}, {x5:d}",
                "fadd {x6:d}, {x6:d}, {x7:d}",
                "fadd {x0:d}, {x0:d}, {x2:d}",
                "fadd {x4:d}, {x4:d}, {x6:d}",
                "fadd {x0:d}, {x0:d}, {x4:d}",
                i = in(reg) iters,
                k = in(vreg) k,
                x0 = inout(vreg) x0 => x0,
                x1 = inout(vreg) x1 => x1,
                x2 = inout(vreg) x2 => x2,
                x3 = inout(vreg) x3 => x3,
                x4 = inout(vreg) x4 => x4,
                x5 = inout(vreg) x5 => x5,
                x6 = inout(vreg) x6 => x6,
                x7 = inout(vreg) x7 => x7,
                options(nostack)
            );
            x0
        }
    };
}

#[cfg(target_arch = "x86_64")]
fp_tp_kernel!(
    vmbench_k_fp64_add_tp,
    "addsd",
    f64,
    core::arch::x86_64::_mm_set1_pd,
    core::arch::x86_64::_mm_cvtsd_f64
);
#[cfg(target_arch = "x86_64")]
fp_tp_kernel!(
    vmbench_k_fp64_mul_tp,
    "mulsd",
    f64,
    core::arch::x86_64::_mm_set1_pd,
    core::arch::x86_64::_mm_cvtsd_f64
);
#[cfg(target_arch = "x86_64")]
fp_tp_kernel!(
    vmbench_k_fp32_add_tp,
    "addss",
    f32,
    core::arch::x86_64::_mm_set1_ps,
    core::arch::x86_64::_mm_cvtss_f32
);
#[cfg(target_arch = "x86_64")]
fp_tp_kernel!(
    vmbench_k_fp32_mul_tp,
    "mulss",
    f32,
    core::arch::x86_64::_mm_set1_ps,
    core::arch::x86_64::_mm_cvtss_f32
);
#[cfg(target_arch = "x86_64")]
fp_fma_kernel!(
    vmbench_k_fp64_fma_tp,
    "vfmadd213sd",
    f64,
    core::arch::x86_64::_mm_set1_pd,
    core::arch::x86_64::_mm_cvtsd_f64
);
#[cfg(target_arch = "x86_64")]
fp_fma_kernel!(
    vmbench_k_fp32_fma_tp,
    "vfmadd213ss",
    f32,
    core::arch::x86_64::_mm_set1_ps,
    core::arch::x86_64::_mm_cvtss_f32
);

#[cfg(target_arch = "aarch64")]
fp_tp_kernel!(vmbench_k_fp64_add_tp, "fadd");
#[cfg(target_arch = "aarch64")]
fp_tp_kernel!(vmbench_k_fp64_mul_tp, "fmul");
#[cfg(target_arch = "aarch64")]
fp_fma_kernel!(vmbench_k_fp64_fma_tp);

// ---------------------------------------------------------------------------
// ---------------------------------------------------------------------------
// FP latency kernels (single dependent chain, inline asm to keep exact
// machine code and avoid black_box spill/reload latency).
// ---------------------------------------------------------------------------

#[cfg(target_arch = "x86_64")]
macro_rules! fp_lat_asm {
    ($fname:ident, $mn:literal, $ty:ty, $k:expr) => {
        #[inline(never)]
        #[no_mangle]
        pub unsafe extern "C" fn $fname(iters: u64, seed: u64) -> f64 {
            let mut x: $ty = seed as $ty * 1.0000001 + 2.0;
            let k: $ty = $k;
            core::arch::asm!(
                "test {i}, {i}",
                "jz 3f",
                "2:",
                concat!($mn, " {x}, {k}"),
                "dec {i}",
                "jnz 2b",
                "3:",
                i = in(reg) iters,
                k = in(xmm_reg) k,
                x = inout(xmm_reg) x => x,
                options(nostack)
            );
            x as f64
        }
    };
}

#[cfg(target_arch = "aarch64")]
macro_rules! fp_sqrt_asm {
    ($fname:ident, $mn:literal, $ty:ty, $fmt:literal) => {
        #[inline(never)]
        #[no_mangle]
        pub unsafe extern "C" fn $fname(iters: u64, seed: u64) -> f64 {
            let mut x: $ty = seed as $ty * 1.0000001 + 2.0;
            core::arch::asm!(
                "cbz {i}, 3f",
                "2:",
                concat!($mn, " {x:", $fmt, "}, {x:", $fmt, "}"),
                "subs {i}, {i}, #1",
                "b.ne 2b",
                "3:",
                i = in(reg) iters,
                x = inout(vreg) x => x,
                options(nostack)
            );
            x as f64
        }
    };
}

#[cfg(target_arch = "aarch64")]
macro_rules! fp_lat_asm {
    ($fname:ident, $mn:literal, $ty:ty, $k:expr, $fmt:literal) => {
        #[inline(never)]
        #[no_mangle]
        pub unsafe extern "C" fn $fname(iters: u64, seed: u64) -> f64 {
            let mut x: $ty = seed as $ty * 1.0000001 + 2.0;
            let k: $ty = $k;
            core::arch::asm!(
                "cbz {i}, 3f",
                "2:",
                concat!($mn, " {x:", $fmt, "}, {x:", $fmt, "}, {k:", $fmt, "}"),
                "subs {i}, {i}, #1",
                "b.ne 2b",
                "3:",
                i = in(reg) iters,
                k = in(vreg) k,
                x = inout(vreg) x => x,
                options(nostack)
            );
            x as f64
        }
    };
}

#[cfg(target_arch = "x86_64")]
fp_lat_asm!(vmbench_k_fp64_add_lat, "addsd", f64, 1.0000001);
#[cfg(target_arch = "aarch64")]
fp_lat_asm!(vmbench_k_fp64_add_lat, "fadd", f64, 1.0000001, "d");
#[cfg(target_arch = "x86_64")]
fp_lat_asm!(vmbench_k_fp64_mul_lat, "mulsd", f64, 1.0000001);
#[cfg(target_arch = "aarch64")]
fp_lat_asm!(vmbench_k_fp64_mul_lat, "fmul", f64, 1.0000001, "d");
#[cfg(target_arch = "x86_64")]
fp_lat_asm!(vmbench_k_fp64_div_lat, "divsd", f64, 1.0000001);
#[cfg(target_arch = "aarch64")]
fp_lat_asm!(vmbench_k_fp64_div_lat, "fdiv", f64, 1.0000001, "d");
#[cfg(target_arch = "x86_64")]
fp_lat_asm!(vmbench_k_fp64_sqrt_lat, "sqrtsd", f64, 1.0);
#[cfg(target_arch = "aarch64")]
fp_sqrt_asm!(vmbench_k_fp64_sqrt_lat, "fsqrt", f64, "d");
#[cfg(target_arch = "x86_64")]
fp_lat_asm!(vmbench_k_fp32_add_lat, "addss", f32, 1.0000001);
#[cfg(target_arch = "aarch64")]
fp_lat_asm!(vmbench_k_fp32_add_lat, "fadd", f32, 1.0000001, "s");
#[cfg(target_arch = "x86_64")]
fp_lat_asm!(vmbench_k_fp32_mul_lat, "mulss", f32, 1.0000001);
#[cfg(target_arch = "aarch64")]
fp_lat_asm!(vmbench_k_fp32_mul_lat, "fmul", f32, 1.0000001, "s");
#[cfg(target_arch = "x86_64")]
fp_lat_asm!(vmbench_k_fp32_div_lat, "divss", f32, 1.0000001);
#[cfg(target_arch = "aarch64")]
fp_lat_asm!(vmbench_k_fp32_div_lat, "fdiv", f32, 1.0000001, "s");
#[cfg(target_arch = "x86_64")]
fp_lat_asm!(vmbench_k_fp32_sqrt_lat, "sqrtss", f32, 1.0);
#[cfg(target_arch = "aarch64")]
fp_sqrt_asm!(vmbench_k_fp32_sqrt_lat, "fsqrt", f32, "s");
// ---------------------------------------------------------------------------
// Registry plumbing
// ---------------------------------------------------------------------------

pub type FpKernel = unsafe extern "C" fn(u64, u64) -> f64;

#[inline(never)]
pub fn call_fp(table: &'static [FpKernel], idx: usize, iters: u64, seed: u64) -> f64 {
    let f = unsafe { core::ptr::read_volatile(&table[idx]) };
    unsafe { f(iters, seed) }
}

#[cfg(target_arch = "x86_64")]
#[used]
static FP_TP_KERNELS: [FpKernel; 6] = [
    vmbench_k_fp64_add_tp,
    vmbench_k_fp64_mul_tp,
    vmbench_k_fp32_add_tp,
    vmbench_k_fp32_mul_tp,
    vmbench_k_fp64_fma_tp,
    vmbench_k_fp32_fma_tp,
];

#[cfg(target_arch = "aarch64")]
#[used]
static FP_TP_KERNELS: [FpKernel; 3] = [
    vmbench_k_fp64_add_tp,
    vmbench_k_fp64_mul_tp,
    vmbench_k_fp64_fma_tp,
];

#[used]
static FP_LAT_KERNELS: [FpKernel; 8] = [
    vmbench_k_fp64_add_lat,
    vmbench_k_fp64_mul_lat,
    vmbench_k_fp64_div_lat,
    vmbench_k_fp64_sqrt_lat,
    vmbench_k_fp32_add_lat,
    vmbench_k_fp32_mul_lat,
    vmbench_k_fp32_div_lat,
    vmbench_k_fp32_sqrt_lat,
];

#[cfg(target_arch = "x86_64")]
static FP_KERNEL_NAMES: [&str; 6] = [
    "vmbench_k_fp64_add_tp",
    "vmbench_k_fp64_mul_tp",
    "vmbench_k_fp32_add_tp",
    "vmbench_k_fp32_mul_tp",
    "vmbench_k_fp64_fma_tp",
    "vmbench_k_fp32_fma_tp",
];

#[cfg(target_arch = "aarch64")]
static FP_KERNEL_NAMES: [&str; 3] = [
    "vmbench_k_fp64_add_tp",
    "vmbench_k_fp64_mul_tp",
    "vmbench_k_fp64_fma_tp",
];

static FP_LAT_KERNEL_NAMES: [&str; 8] = [
    "vmbench_k_fp64_add_lat",
    "vmbench_k_fp64_mul_lat",
    "vmbench_k_fp64_div_lat",
    "vmbench_k_fp64_sqrt_lat",
    "vmbench_k_fp32_add_lat",
    "vmbench_k_fp32_mul_lat",
    "vmbench_k_fp32_div_lat",
    "vmbench_k_fp32_sqrt_lat",
];

macro_rules! fp_tp_bench {
    ($struct_name:ident, $id:literal, $idx:literal, $ty:literal, $op:literal) => {
        pub struct $struct_name;
        impl Benchmark for $struct_name {
            fn meta(&self) -> &'static EntryMeta {
                static M: EntryMeta = EntryMeta {
                    id: $id,
                    version: 1,
                    description: "Floating-point throughput",
                    imp: ImplInfo {
                        source: "src/bench/cpu_fp.rs",
                        kernel: FP_KERNEL_NAMES[$idx],
                        algorithm: "8 independent scalar accumulators, 16 unrolled ops per iteration; units = 16 ops/iter",
                        isa: BASELINE_ISA,
                    },
                    metrics: &[M_OPS, M_NS_OP, M_CYC_OP, M_CYC_OP_NET],
                };
                &M
            }
            fn run(&self, ctx: &mut Ctx, entry: usize) -> Result<(), &'static str> {
                let cpu = ctx.default_cpu();
                ctx.pin_self(cpu);
                ctx.add_param(entry, "type", ParamValue::Str($ty));
                ctx.add_param(entry, "op", ParamValue::Str($op));
                ctx.add_param(entry, "chains", ParamValue::Int(8));
                ctx.add_param(entry, "cpu", ParamValue::Int(cpu as i64));
                let seed = 0x0123_4567_89ab_cdefu64;
                ctx.measure(
                    entry,
                    ctx.cfg.target_run_ns,
                    ctx.cfg.runs,
                    |iters| {
                        let _ = call_fp(&FP_TP_KERNELS, $idx, iters, seed);
                        iters.saturating_mul(OPS_PER_ITER)
                    },
                    |ctx, e, s| emit_rate_net(ctx, e, s, 16),
                );
                Ok(())
            }
        }
    };
}

macro_rules! fp_lat_bench {
    ($struct_name:ident, $id:literal, $idx:literal, $ty:literal, $op:literal, $algo:literal) => {
        pub struct $struct_name;
        impl Benchmark for $struct_name {
            fn meta(&self) -> &'static EntryMeta {
                static M: EntryMeta = EntryMeta {
                    id: $id,
                    version: 1,
                    description: "Floating-point latency",
                    imp: ImplInfo {
                        source: "src/bench/cpu_fp.rs",
                        kernel: FP_LAT_KERNEL_NAMES[$idx],
                        algorithm: $algo,
                        isa: BASELINE_ISA,
                    },
                    metrics: &[M_OPS, M_NS_OP, M_CYC_OP],
                };
                &M
            }
            fn run(&self, ctx: &mut Ctx, entry: usize) -> Result<(), &'static str> {
                let cpu = ctx.default_cpu();
                ctx.pin_self(cpu);
                ctx.add_param(entry, "type", ParamValue::Str($ty));
                ctx.add_param(entry, "op", ParamValue::Str($op));
                ctx.add_param(entry, "chain", ParamValue::Int(1));
                ctx.add_param(entry, "cpu", ParamValue::Int(cpu as i64));
                let seed = 0x0123_4567_89ab_cdefu64;
                ctx.measure(
                    entry,
                    ctx.cfg.target_run_ns,
                    ctx.cfg.runs,
                    |iters| {
                        let _ = call_fp(&FP_LAT_KERNELS, $idx, iters, seed);
                        iters
                    },
                    |ctx, e, s| emit_rate(ctx, e, s),
                );
                Ok(())
            }
        }
    };
}

fp_tp_bench!(Fp64AddTp, "cpu.fp.throughput.f64.add.v1", 0, "f64", "add");
fp_tp_bench!(Fp64MulTp, "cpu.fp.throughput.f64.mul.v1", 1, "f64", "mul");
#[cfg(target_arch = "x86_64")]
fp_tp_bench!(Fp32AddTp, "cpu.fp.throughput.f32.add.v1", 2, "f32", "add");
#[cfg(target_arch = "x86_64")]
fp_tp_bench!(Fp32MulTp, "cpu.fp.throughput.f32.mul.v1", 3, "f32", "mul");
#[cfg(target_arch = "x86_64")]
fp_tp_bench!(Fp64FmaTp, "cpu.fp.throughput.f64.fma.v1", 4, "f64", "fma");
#[cfg(target_arch = "aarch64")]
fp_tp_bench!(Fp64FmaTp, "cpu.fp.throughput.f64.fma.v1", 2, "f64", "fma");
#[cfg(target_arch = "x86_64")]
fp_tp_bench!(Fp32FmaTp, "cpu.fp.throughput.f32.fma.v1", 5, "f32", "fma");

fp_lat_bench!(
    Fp64AddLat,
    "cpu.fp.latency.f64.add.v1",
    0,
    "f64",
    "add",
    "single dependent chain x = x + 1.0000001"
);
fp_lat_bench!(
    Fp64MulLat,
    "cpu.fp.latency.f64.mul.v1",
    1,
    "f64",
    "mul",
    "single dependent chain x = x * 1.0000001"
);
fp_lat_bench!(
    Fp64DivLat,
    "cpu.fp.latency.f64.div.v1",
    2,
    "f64",
    "div",
    "single dependent chain x = x / 3 + 2"
);
fp_lat_bench!(
    Fp64SqrtLat,
    "cpu.fp.latency.f64.sqrt.v1",
    3,
    "f64",
    "sqrt",
    "single dependent chain x = sqrt(x) + 1"
);
fp_lat_bench!(
    Fp32AddLat,
    "cpu.fp.latency.f32.add.v1",
    4,
    "f32",
    "add",
    "single dependent chain x = x + 1.0000001"
);
fp_lat_bench!(
    Fp32MulLat,
    "cpu.fp.latency.f32.mul.v1",
    5,
    "f32",
    "mul",
    "single dependent chain x = x * 1.0000001"
);
fp_lat_bench!(
    Fp32DivLat,
    "cpu.fp.latency.f32.div.v1",
    6,
    "f32",
    "div",
    "single dependent chain x = x / 3 + 2"
);
fp_lat_bench!(
    Fp32SqrtLat,
    "cpu.fp.latency.f32.sqrt.v1",
    7,
    "f32",
    "sqrt",
    "single dependent chain x = sqrt(x) + 1"
);
