use crate::bench::common::*;
use crate::bench::Benchmark;
use crate::runner::{Ctx, EntryMeta, ImplInfo, ParamValue};

// ---------------------------------------------------------------------------
// Branch kernels. All are inline asm so the branch really is a branch
// (the compiler cannot turn it into a cmov/select).
// ---------------------------------------------------------------------------

#[cfg(target_arch = "x86_64")]
#[inline(never)]
#[no_mangle]
pub unsafe extern "C" fn vmbench_k_branch_pred(iters: u64, seed: u64) -> u64 {
    let x = seed | 1;
    let mut a = 0u64;
    core::arch::asm!(
        "test {i}, {i}",
        "jz 3f",
        "2:",
        "cmp {x}, 0",
        "jle 4f",
        "add {a}, 1",
        "jmp 5f",
        "4:",
        "sub {a}, 1",
        "5:",
        "dec {i}",
        "jnz 2b",
        "3:",
        i = in(reg) iters,
        x = inout(reg) x => _,
        a = inout(reg) a,
        options(nostack)
    );
    a
}

#[cfg(target_arch = "x86_64")]
#[inline(never)]
#[no_mangle]
pub unsafe extern "C" fn vmbench_k_branch_rand(iters: u64, seed: u64) -> u64 {
    let x = seed | 1;
    let mut a = 0u64;
    let mut t = 0u64;
    core::arch::asm!(
        "test {i}, {i}",
        "jz 3f",
        "2:",
        "mov {t}, {x}",
        "shl {t}, 13",
        "xor {x}, {t}",
        "mov {t}, {x}",
        "shr {t}, 7",
        "xor {x}, {t}",
        "mov {t}, {x}",
        "shl {t}, 17",
        "xor {x}, {t}",
        "test {x}, 1",
        "jz 4f",
        "add {a}, 1",
        "jmp 5f",
        "4:",
        "sub {a}, 1",
        "5:",
        "dec {i}",
        "jnz 2b",
        "3:",
        i = in(reg) iters,
        x = inout(reg) x => _,
        a = inout(reg) a,
        t = out(reg) t,
        options(nostack)
    );
    a ^ t
}

#[cfg(target_arch = "x86_64")]
#[inline(never)]
#[no_mangle]
pub unsafe extern "C" fn vmbench_k_branch_ind(iters: u64, seed: u64) -> u64 {
    let x = seed | 1;
    let mut a = 0u64;
    let mut t = 0u64;
    let mut u = 0u64;
    core::arch::asm!(
        "test {i}, {i}",
        "jz 3f",
        "2:",
        "lea {t}, [rip + 4f]",
        "lea {u}, [rip + 5f]",
        "test {x}, 1",
        "cmovnz {t}, {u}",
        "jmp {t}",
        "4:",
        "add {a}, 1",
        "jmp 6f",
        "5:",
        "sub {a}, 1",
        "6:",
        "mov {t}, {x}",
        "shl {t}, 13",
        "xor {x}, {t}",
        "dec {i}",
        "jnz 2b",
        "3:",
        i = in(reg) iters,
        x = inout(reg) x => _,
        a = inout(reg) a,
        t = out(reg) t,
        u = out(reg) u,
        options(nostack)
    );
    let _ = u;
    a ^ t
}

#[cfg(target_arch = "aarch64")]
#[inline(never)]
#[no_mangle]
pub unsafe extern "C" fn vmbench_k_branch_pred(iters: u64, seed: u64) -> u64 {
    let x = seed | 1;
    let mut a = 0u64;
    core::arch::asm!(
        "cbz {i}, 3f",
        "2:",
        "cmp {x}, #0",
        "b.le 4f",
        "add {a}, {a}, #1",
        "b 5f",
        "4:",
        "sub {a}, {a}, #1",
        "5:",
        "subs {i}, {i}, #1",
        "b.ne 2b",
        "3:",
        i = in(reg) iters,
        x = inout(reg) x => _,
        a = inout(reg) a,
        options(nostack)
    );
    a
}

#[cfg(target_arch = "aarch64")]
#[inline(never)]
#[no_mangle]
pub unsafe extern "C" fn vmbench_k_branch_rand(iters: u64, seed: u64) -> u64 {
    let x = seed | 1;
    let mut a = 0u64;
    let mut t = 0u64;
    core::arch::asm!(
        "cbz {i}, 3f",
        "2:",
        "eor {t}, {x}, {x}, lsl #13",
        "eor {x}, {t}, {t}, lsr #7",
        "eor {t}, {x}, {x}, lsl #17",
        "tst {t}, #1",
        "b.eq 4f",
        "add {a}, {a}, #1",
        "b 5f",
        "4:",
        "sub {a}, {a}, #1",
        "5:",
        "subs {i}, {i}, #1",
        "b.ne 2b",
        "3:",
        i = in(reg) iters,
        x = inout(reg) x => _,
        a = inout(reg) a,
        t = out(reg) t,
        options(nostack)
    );
    a ^ t
}

#[cfg(target_arch = "aarch64")]
#[inline(never)]
#[no_mangle]
pub unsafe extern "C" fn vmbench_k_branch_ind(iters: u64, seed: u64) -> u64 {
    let x = seed | 1;
    let mut a = 0u64;
    let mut t = 0u64;
    let mut u = 0u64;
    core::arch::asm!(
        "cbz {i}, 3f",
        "2:",
        "adr {t}, 4f",
        "adr {u}, 5f",
        "tst {x}, #1",
        "csel {t}, {u}, {t}, ne",
        "br {t}",
        "4:",
        "add {a}, {a}, #1",
        "b 6f",
        "5:",
        "sub {a}, {a}, #1",
        "6:",
        "eor {t}, {x}, {x}, lsl #13",
        "mov {x}, {t}",
        "subs {i}, {i}, #1",
        "b.ne 2b",
        "3:",
        i = in(reg) iters,
        x = inout(reg) x => _,
        a = inout(reg) a,
        t = out(reg) t,
        u = out(reg) u,
        options(nostack)
    );
    let _ = u;
    a ^ t
}

pub type BranchKernel = unsafe extern "C" fn(u64, u64) -> u64;

#[inline(never)]
pub fn call_branch(table: &'static [BranchKernel], idx: usize, iters: u64, seed: u64) -> u64 {
    let f = unsafe { core::ptr::read_volatile(&table[idx]) };
    unsafe { f(iters, seed) }
}

#[used]
static BRANCH_KERNELS: [BranchKernel; 3] = [
    vmbench_k_branch_pred,
    vmbench_k_branch_rand,
    vmbench_k_branch_ind,
];

static BRANCH_NAMES: [&str; 3] = [
    "vmbench_k_branch_pred",
    "vmbench_k_branch_rand",
    "vmbench_k_branch_ind",
];

macro_rules! branch_bench {
    ($struct_name:ident, $id:literal, $idx:literal, $kind:literal, $algo:literal) => {
        pub struct $struct_name;
        impl Benchmark for $struct_name {
            fn meta(&self) -> &'static EntryMeta {
                static M: EntryMeta = EntryMeta {
                    id: $id,
                    version: 1,
                    description: "Conditional/indirect branch throughput",
                    imp: ImplInfo {
                        source: "src/bench/cpu_branch.rs",
                        kernel: BRANCH_NAMES[$idx],
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
                ctx.add_param(entry, "kind", ParamValue::Str($kind));
                ctx.add_param(entry, "cpu", ParamValue::Int(cpu as i64));
                let seed = 0x0123_4567_89ab_cdefu64;
                ctx.measure(
                    entry,
                    ctx.cfg.target_run_ns,
                    ctx.cfg.runs,
                    |iters| {
                        let _ = call_branch(&BRANCH_KERNELS, $idx, iters, seed);
                        iters
                    },
                    |ctx, e, s| emit_rate(ctx, e, s),
                );
                Ok(())
            }
        }
    };
}

branch_bench!(
    BranchPred,
    "cpu.branch.predictable.v1",
    0,
    "predictable",
    "cmp x,0 / jle (always not taken); x constant"
);
branch_bench!(
    BranchRand,
    "cpu.branch.unpredictable.v1",
    1,
    "unpredictable",
    "xorshift64 then branch on bit 0 (random outcome)"
);
branch_bench!(
    BranchInd,
    "cpu.branch.indirect.v1",
    2,
    "indirect",
    "jmp through register selected by xorshift bit (indirect branch)"
);
