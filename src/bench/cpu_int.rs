use crate::bench::common::*;
use crate::bench::Benchmark;
use crate::runner::{Ctx, EntryMeta, ImplInfo, ParamValue};

pub const OPS_PER_ITER: u64 = 16;

// ---------------------------------------------------------------------------
// Integer throughput kernels.
//
// These are written in inline assembly so that the compiler cannot fold the
// recurrence into a closed form and so the machine code is stable across
// builds. Eight independent chains, two operations per chain per iteration.
// ---------------------------------------------------------------------------

#[cfg(target_arch = "x86_64")]
macro_rules! tp_asm_kernel_k {
    ($fname:ident, $mn:literal) => {
        #[inline(never)]
        #[no_mangle]
        pub unsafe extern "C" fn $fname(iters: u64, seed: u64) -> u64 {
            let x0: u64;
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
                "xor {x0}, {x1}",
                "xor {x0}, {x2}",
                "xor {x0}, {x3}",
                "xor {x0}, {x4}",
                "xor {x0}, {x5}",
                "xor {x0}, {x6}",
                "xor {x0}, {x7}",
                i = in(reg) iters,
                k = in(reg) 0x9e37_79b9_7f4a_7c15u64,
                x0 = inout(reg) (seed ^ 0x1111_1111_1111_1111) => x0,
                x1 = inout(reg) (seed ^ 0x2222_2222_2222_2222) => _,
                x2 = inout(reg) (seed ^ 0x3333_3333_3333_3333) => _,
                x3 = inout(reg) (seed ^ 0x4444_4444_4444_4444) => _,
                x4 = inout(reg) (seed ^ 0x5555_5555_5555_5555) => _,
                x5 = inout(reg) (seed ^ 0x6666_6666_6666_6666) => _,
                x6 = inout(reg) (seed ^ 0x7777_7777_7777_7777) => _,
                x7 = inout(reg) (seed ^ 0x8888_8888_8888_8888) => _,
                options(nostack)
            );
            x0
        }
    };
}

#[cfg(target_arch = "x86_64")]
macro_rules! tp_asm_kernel_imm {
    ($fname:ident, $mn:literal, $imm:literal) => {
        #[inline(never)]
        #[no_mangle]
        pub unsafe extern "C" fn $fname(iters: u64, seed: u64) -> u64 {
            let x0: u64;
            core::arch::asm!(
                "test {i}, {i}",
                "jz 3f",
                "2:",
                concat!(
                    $mn, " {x0}, ", $imm, "\n", $mn, " {x1}, ", $imm, "\n",
                    $mn, " {x2}, ", $imm, "\n", $mn, " {x3}, ", $imm, "\n",
                    $mn, " {x4}, ", $imm, "\n", $mn, " {x5}, ", $imm, "\n",
                    $mn, " {x6}, ", $imm, "\n", $mn, " {x7}, ", $imm, "\n",
                    $mn, " {x0}, ", $imm, "\n", $mn, " {x1}, ", $imm, "\n",
                    $mn, " {x2}, ", $imm, "\n", $mn, " {x3}, ", $imm, "\n",
                    $mn, " {x4}, ", $imm, "\n", $mn, " {x5}, ", $imm, "\n",
                    $mn, " {x6}, ", $imm, "\n", $mn, " {x7}, ", $imm, "\n",
                ),
                "dec {i}",
                "jnz 2b",
                "3:",
                "xor {x0}, {x1}",
                "xor {x0}, {x2}",
                "xor {x0}, {x3}",
                "xor {x0}, {x4}",
                "xor {x0}, {x5}",
                "xor {x0}, {x6}",
                "xor {x0}, {x7}",
                i = in(reg) iters,
                x0 = inout(reg) (seed ^ 0x1111_1111_1111_1111) => x0,
                x1 = inout(reg) (seed ^ 0x2222_2222_2222_2222) => _,
                x2 = inout(reg) (seed ^ 0x3333_3333_3333_3333) => _,
                x3 = inout(reg) (seed ^ 0x4444_4444_4444_4444) => _,
                x4 = inout(reg) (seed ^ 0x5555_5555_5555_5555) => _,
                x5 = inout(reg) (seed ^ 0x6666_6666_6666_6666) => _,
                x6 = inout(reg) (seed ^ 0x7777_7777_7777_7777) => _,
                x7 = inout(reg) (seed ^ 0x8888_8888_8888_8888) => _,
                options(nostack)
            );
            x0
        }
    };
}

#[cfg(target_arch = "aarch64")]
macro_rules! tp_asm_kernel_k {
    ($fname:ident, $mn:literal) => {
        #[inline(never)]
        #[no_mangle]
        pub unsafe extern "C" fn $fname(iters: u64, seed: u64) -> u64 {
            let x0: u64;
            core::arch::asm!(
                "cbz {i}, 3f",
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
                "subs {i}, {i}, #1",
                "b.ne 2b",
                "3:",
                "eor {x0}, {x0}, {x1}",
                "eor {x0}, {x0}, {x2}",
                "eor {x0}, {x0}, {x3}",
                "eor {x0}, {x0}, {x4}",
                "eor {x0}, {x0}, {x5}",
                "eor {x0}, {x0}, {x6}",
                "eor {x0}, {x0}, {x7}",
                i = in(reg) iters,
                k = in(reg) 0x9e37_79b9_7f4a_7c15u64,
                x0 = inout(reg) (seed ^ 0x1111_1111_1111_1111) => x0,
                x1 = inout(reg) (seed ^ 0x2222_2222_2222_2222) => _,
                x2 = inout(reg) (seed ^ 0x3333_3333_3333_3333) => _,
                x3 = inout(reg) (seed ^ 0x4444_4444_4444_4444) => _,
                x4 = inout(reg) (seed ^ 0x5555_5555_5555_5555) => _,
                x5 = inout(reg) (seed ^ 0x6666_6666_6666_6666) => _,
                x6 = inout(reg) (seed ^ 0x7777_7777_7777_7777) => _,
                x7 = inout(reg) (seed ^ 0x8888_8888_8888_8888) => _,
                options(nostack)
            );
            x0
        }
    };
}

#[cfg(target_arch = "aarch64")]
macro_rules! tp_asm_kernel_imm {
    ($fname:ident, $mn:literal, $imm:literal) => {
        #[inline(never)]
        #[no_mangle]
        pub unsafe extern "C" fn $fname(iters: u64, seed: u64) -> u64 {
            let x0: u64;
            core::arch::asm!(
                "cbz {i}, 3f",
                "2:",
                concat!(
                    $mn, " {x0}, {x0}, #", $imm, "\n", $mn, " {x1}, {x1}, #", $imm, "\n",
                    $mn, " {x2}, {x2}, #", $imm, "\n", $mn, " {x3}, {x3}, #", $imm, "\n",
                    $mn, " {x4}, {x4}, #", $imm, "\n", $mn, " {x5}, {x5}, #", $imm, "\n",
                    $mn, " {x6}, {x6}, #", $imm, "\n", $mn, " {x7}, {x7}, #", $imm, "\n",
                    $mn, " {x0}, {x0}, #", $imm, "\n", $mn, " {x1}, {x1}, #", $imm, "\n",
                    $mn, " {x2}, {x2}, #", $imm, "\n", $mn, " {x3}, {x3}, #", $imm, "\n",
                    $mn, " {x4}, {x4}, #", $imm, "\n", $mn, " {x5}, {x5}, #", $imm, "\n",
                    $mn, " {x6}, {x6}, #", $imm, "\n", $mn, " {x7}, {x7}, #", $imm, "\n",
                ),
                "subs {i}, {i}, #1",
                "b.ne 2b",
                "3:",
                "eor {x0}, {x0}, {x1}",
                "eor {x0}, {x0}, {x2}",
                "eor {x0}, {x0}, {x3}",
                "eor {x0}, {x0}, {x4}",
                "eor {x0}, {x0}, {x5}",
                "eor {x0}, {x0}, {x6}",
                "eor {x0}, {x0}, {x7}",
                i = in(reg) iters,
                x0 = inout(reg) (seed ^ 0x1111_1111_1111_1111) => x0,
                x1 = inout(reg) (seed ^ 0x2222_2222_2222_2222) => _,
                x2 = inout(reg) (seed ^ 0x3333_3333_3333_3333) => _,
                x3 = inout(reg) (seed ^ 0x4444_4444_4444_4444) => _,
                x4 = inout(reg) (seed ^ 0x5555_5555_5555_5555) => _,
                x5 = inout(reg) (seed ^ 0x6666_6666_6666_6666) => _,
                x6 = inout(reg) (seed ^ 0x7777_7777_7777_7777) => _,
                x7 = inout(reg) (seed ^ 0x8888_8888_8888_8888) => _,
                options(nostack)
            );
            x0
        }
    };
}

tp_asm_kernel_k!(vmbench_k_int_add_tp, "add");
tp_asm_kernel_k!(vmbench_k_int_sub_tp, "sub");
#[cfg(target_arch = "x86_64")]
tp_asm_kernel_k!(vmbench_k_int_xor_tp, "xor");
#[cfg(target_arch = "aarch64")]
tp_asm_kernel_k!(vmbench_k_int_xor_tp, "eor");
#[cfg(target_arch = "x86_64")]
tp_asm_kernel_k!(vmbench_k_int_and_tp, "and");
#[cfg(target_arch = "aarch64")]
tp_asm_kernel_k!(vmbench_k_int_and_tp, "and");
#[cfg(target_arch = "x86_64")]
tp_asm_kernel_k!(vmbench_k_int_or_tp, "or");
#[cfg(target_arch = "aarch64")]
tp_asm_kernel_k!(vmbench_k_int_or_tp, "orr");
#[cfg(target_arch = "x86_64")]
tp_asm_kernel_k!(vmbench_k_int_mul_tp, "imul");
#[cfg(target_arch = "aarch64")]
tp_asm_kernel_k!(vmbench_k_int_mul_tp, "mul");
#[cfg(target_arch = "x86_64")]
tp_asm_kernel_imm!(vmbench_k_int_rot_tp, "rol", "7");
#[cfg(target_arch = "aarch64")]
tp_asm_kernel_imm!(vmbench_k_int_rot_tp, "ror", "57");
#[cfg(target_arch = "x86_64")]
tp_asm_kernel_imm!(vmbench_k_int_shl_tp, "shl", "3");
#[cfg(target_arch = "aarch64")]
tp_asm_kernel_imm!(vmbench_k_int_shl_tp, "lsl", "3");

// ---------------------------------------------------------------------------
// Integer latency kernels (single dependent chain).
//
// The accumulator must stay in a register: a `black_box`-based loop makes
// LLVM spill it to the stack, so the chain measures store-to-load forwarding
// instead of ALU latency (and `/ 3` is strength-reduced to a multiply).
// ---------------------------------------------------------------------------

const K_LAT: u64 = 0x9e37_79b9_7f4a_7c15;

#[cfg(target_arch = "x86_64")]
macro_rules! lat_asm_kernel {
    ($fname:ident, $($op:literal),+ $(,)?) => {
        #[inline(never)]
        #[no_mangle]
        pub unsafe extern "C" fn $fname(iters: u64, seed: u64) -> u64 {
            let x: u64;
            core::arch::asm!(
                "test {i}, {i}",
                "jz 3f",
                "2:",
                $($op,)+
                "dec {i}",
                "jnz 2b",
                "3:",
                i = in(reg) iters,
                k = in(reg) K_LAT,
                x = inout(reg) seed | 1 => x,
                options(nostack)
            );
            x
        }
    };
}

#[cfg(target_arch = "x86_64")]
#[inline(never)]
#[no_mangle]
pub unsafe extern "C" fn vmbench_k_int_div_lat(iters: u64, seed: u64) -> u64 {
    let x: u64;
    core::arch::asm!(
        "test {i}, {i}",
        "jz 3f",
        "2:",
        "xor edx, edx",
        "div {d}",
        "xor rax, {k}",
        "dec {i}",
        "jnz 2b",
        "3:",
        i = in(reg) iters,
        d = in(reg) 3u64,
        k = in(reg) K_LAT,
        inout("rax") seed | 1 => x,
        out("rdx") _,
        options(nostack)
    );
    x
}

#[cfg(target_arch = "aarch64")]
macro_rules! lat_asm_kernel {
    ($fname:ident, $($op:literal),+ $(,)?) => {
        #[inline(never)]
        #[no_mangle]
        pub unsafe extern "C" fn $fname(iters: u64, seed: u64) -> u64 {
            let x: u64;
            core::arch::asm!(
                "cbz {i}, 3f",
                "2:",
                $($op,)+
                "subs {i}, {i}, #1",
                "b.ne 2b",
                "3:",
                i = in(reg) iters,
                k = in(reg) K_LAT,
                x = inout(reg) seed | 1 => x,
                options(nostack)
            );
            x
        }
    };
}

#[cfg(target_arch = "aarch64")]
#[inline(never)]
#[no_mangle]
pub unsafe extern "C" fn vmbench_k_int_div_lat(iters: u64, seed: u64) -> u64 {
    let x: u64;
    core::arch::asm!(
        "cbz {i}, 3f",
        "2:",
        "udiv {x}, {x}, {d}",
        "eor {x}, {x}, {k}",
        "subs {i}, {i}, #1",
        "b.ne 2b",
        "3:",
        i = in(reg) iters,
        d = in(reg) 3u64,
        k = in(reg) K_LAT,
        x = inout(reg) seed | 1 => x,
        options(nostack)
    );
    x
}

#[cfg(target_arch = "x86_64")]
lat_asm_kernel!(vmbench_k_int_add_lat, "add {x}, {k}");
#[cfg(target_arch = "x86_64")]
lat_asm_kernel!(vmbench_k_int_mul_lat, "imul {x}, {k}");
#[cfg(target_arch = "x86_64")]
lat_asm_kernel!(vmbench_k_int_rot_lat, "/* {k} */", "rol {x}, 13");
#[cfg(target_arch = "x86_64")]
lat_asm_kernel!(vmbench_k_int_xor_lat, "xor {x}, {k}");
#[cfg(target_arch = "x86_64")]
lat_asm_kernel!(vmbench_k_int_shl_lat, "/* {k} */", "shl {x}, 3", "or {x}, 1");

#[cfg(target_arch = "aarch64")]
lat_asm_kernel!(vmbench_k_int_add_lat, "add {x}, {x}, {k}");
#[cfg(target_arch = "aarch64")]
lat_asm_kernel!(vmbench_k_int_mul_lat, "mul {x}, {x}, {k}");
#[cfg(target_arch = "aarch64")]
lat_asm_kernel!(vmbench_k_int_rot_lat, "/* {k} */", "ror {x}, {x}, #51");
#[cfg(target_arch = "aarch64")]
lat_asm_kernel!(vmbench_k_int_xor_lat, "eor {x}, {x}, {k}");
#[cfg(target_arch = "aarch64")]
lat_asm_kernel!(vmbench_k_int_shl_lat, "/* {k} */", "lsl {x}, {x}, #3", "orr {x}, {x}, #1");

// ---------------------------------------------------------------------------
// Registry plumbing
// ---------------------------------------------------------------------------

#[used]
static INT_TP_KERNELS: [Kernel64; 8] = [
    vmbench_k_int_add_tp,
    vmbench_k_int_sub_tp,
    vmbench_k_int_xor_tp,
    vmbench_k_int_and_tp,
    vmbench_k_int_or_tp,
    vmbench_k_int_mul_tp,
    vmbench_k_int_rot_tp,
    vmbench_k_int_shl_tp,
];

#[used]
static INT_LAT_KERNELS: [Kernel64; 6] = [
    vmbench_k_int_add_lat,
    vmbench_k_int_mul_lat,
    vmbench_k_int_div_lat,
    vmbench_k_int_rot_lat,
    vmbench_k_int_xor_lat,
    vmbench_k_int_shl_lat,
];

static KERNEL_NAMES_TP: [&str; 8] = [
    "vmbench_k_int_add_tp",
    "vmbench_k_int_sub_tp",
    "vmbench_k_int_xor_tp",
    "vmbench_k_int_and_tp",
    "vmbench_k_int_or_tp",
    "vmbench_k_int_mul_tp",
    "vmbench_k_int_rot_tp",
    "vmbench_k_int_shl_tp",
];

static KERNEL_NAMES_LAT: [&str; 6] = [
    "vmbench_k_int_add_lat",
    "vmbench_k_int_mul_lat",
    "vmbench_k_int_div_lat",
    "vmbench_k_int_rot_lat",
    "vmbench_k_int_xor_lat",
    "vmbench_k_int_shl_lat",
];

macro_rules! int_tp_bench {
    ($struct_name:ident, $id:literal, $idx:literal, $op:literal, $desc:literal, $algo:literal) => {
        pub struct $struct_name;
        impl Benchmark for $struct_name {
            fn meta(&self) -> &'static EntryMeta {
                static M: EntryMeta = EntryMeta {
                    id: $id,
                    version: 1,
                    description: $desc,
                    imp: ImplInfo {
                        source: "src/bench/cpu_int.rs",
                        kernel: KERNEL_NAMES_TP[$idx],
                        algorithm: $algo,
                        isa: BASELINE_ISA,
                    },
                    metrics: &[M_OPS, M_NS_OP, M_CYC_OP, M_CYC_OP_NET],
                };
                &M
            }
            fn run(&self, ctx: &mut Ctx, entry: usize) -> Result<(), &'static str> {
                let cpu = ctx.default_cpu();
                ctx.pin_self(cpu);
                ctx.add_param(entry, "op", ParamValue::Str($op));
                ctx.add_param(entry, "chains", ParamValue::Int(8));
                ctx.add_param(entry, "cpu", ParamValue::Int(cpu as i64));
                let seed = 0x0123_4567_89ab_cdefu64;
                ctx.measure(
                    entry,
                    ctx.cfg.target_run_ns,
                    ctx.cfg.runs,
                    |iters| {
                        let _ = call_k64(&INT_TP_KERNELS, $idx, iters, seed);
                        iters.saturating_mul(OPS_PER_ITER)
                    },
                    |ctx, e, s| emit_rate_net(ctx, e, s, 16),
                );
                Ok(())
            }
        }
    };
}

macro_rules! int_lat_bench {
    ($struct_name:ident, $id:literal, $idx:literal, $op:literal, $desc:literal, $algo:literal) => {
        pub struct $struct_name;
        impl Benchmark for $struct_name {
            fn meta(&self) -> &'static EntryMeta {
                static M: EntryMeta = EntryMeta {
                    id: $id,
                    version: 1,
                    description: $desc,
                    imp: ImplInfo {
                        source: "src/bench/cpu_int.rs",
                        kernel: KERNEL_NAMES_LAT[$idx],
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
                ctx.add_param(entry, "op", ParamValue::Str($op));
                ctx.add_param(entry, "chain", ParamValue::Int(1));
                ctx.add_param(entry, "cpu", ParamValue::Int(cpu as i64));
                let seed = 0x0123_4567_89ab_cdefu64;
                ctx.measure(
                    entry,
                    ctx.cfg.target_run_ns,
                    ctx.cfg.runs,
                    |iters| {
                        let _ = call_k64(&INT_LAT_KERNELS, $idx, iters, seed);
                        iters
                    },
                    |ctx, e, s| emit_rate(ctx, e, s),
                );
                Ok(())
            }
        }
    };
}

int_tp_bench!(
    IntTpAdd,
    "cpu.int.throughput.add.v1",
    0,
    "add",
    "Integer ADD throughput",
    "8 independent 64-bit chains, 16 unrolled ops per iteration; units = 16 ops/iter"
);
int_tp_bench!(
    IntTpSub,
    "cpu.int.throughput.sub.v1",
    1,
    "sub",
    "Integer SUB throughput",
    "8 independent 64-bit chains, 16 unrolled ops per iteration; units = 16 ops/iter"
);
int_tp_bench!(
    IntTpXor,
    "cpu.int.throughput.xor.v1",
    2,
    "xor",
    "Integer XOR throughput",
    "8 independent 64-bit chains, 16 unrolled ops per iteration; units = 16 ops/iter"
);
int_tp_bench!(
    IntTpAnd,
    "cpu.int.throughput.and.v1",
    3,
    "and",
    "Integer AND throughput",
    "8 independent 64-bit chains, 16 unrolled ops per iteration; units = 16 ops/iter"
);
int_tp_bench!(
    IntTpOr,
    "cpu.int.throughput.or.v1",
    4,
    "or",
    "Integer OR throughput",
    "8 independent 64-bit chains, 16 unrolled ops per iteration; units = 16 ops/iter"
);
int_tp_bench!(
    IntTpMul,
    "cpu.int.throughput.mul.v1",
    5,
    "mul",
    "Integer MUL throughput",
    "8 independent 64-bit chains, 16 unrolled ops per iteration; units = 16 ops/iter"
);
int_tp_bench!(
    IntTpRot,
    "cpu.int.throughput.rotate.v1",
    6,
    "rotate",
    "Integer rotate throughput",
    "8 independent 64-bit chains, 16 unrolled ops per iteration; units = 16 ops/iter"
);
int_tp_bench!(
    IntTpShl,
    "cpu.int.throughput.shift.v1",
    7,
    "shift",
    "Integer shift throughput",
    "8 independent 64-bit chains, 16 unrolled ops per iteration; units = 16 ops/iter"
);

int_lat_bench!(
    IntLatAdd,
    "cpu.int.latency.add.v1",
    0,
    "add",
    "Integer ADD latency",
    "single dependent chain x = x + K"
);
int_lat_bench!(
    IntLatMul,
    "cpu.int.latency.mul.v1",
    1,
    "mul",
    "Integer MUL latency",
    "single dependent chain x = x * K (odd K)"
);
int_lat_bench!(
    IntLatDiv,
    "cpu.int.latency.div.v1",
    2,
    "div",
    "Integer DIV latency",
    "single dependent chain x = (x / 3) ^ K"
);
int_lat_bench!(
    IntLatRot,
    "cpu.int.latency.rotate.v1",
    3,
    "rotate",
    "Integer rotate latency",
    "single dependent chain x = rotl(x, 13)"
);
int_lat_bench!(
    IntLatXor,
    "cpu.int.latency.xor.v1",
    4,
    "xor",
    "Integer XOR latency",
    "single dependent chain x = x ^ K"
);
int_lat_bench!(
    IntLatShl,
    "cpu.int.latency.shift.v1",
    5,
    "shift",
    "Integer shift latency",
    "single dependent chain x = (x << 3) | 1"
);
