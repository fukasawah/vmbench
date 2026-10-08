use crate::bench::common::*;
use crate::bench::Benchmark;
use crate::runner::{Ctx, EntryMeta, ImplInfo, ParamValue};

// ---------------------------------------------------------------------------
// Loop overhead reference.
//
// The throughput kernels use an unrolled loop of 16 operations plus one
// loop-control pair (dec+jnz / subs+b.ne). This kernel measures that control
// pair alone so throughput results can also report an overhead-corrected
// "net" value. The correction is an approximation (superscalar overlap and
// port pressure are not additive) and is only applied to the unrolled
// throughput kernels, never to latency chains.
// ---------------------------------------------------------------------------

#[cfg(target_arch = "x86_64")]
#[inline(never)]
#[no_mangle]
pub unsafe extern "C" fn vmbench_k_overhead_loop(iters: u64, seed: u64) -> u64 {
    core::arch::asm!(
        "test {i}, {i}",
        "jz 4f",
        "3:",
        "dec {i}",
        "jnz 3b",
        "4:",
        i = in(reg) iters,
        options(nostack)
    );
    core::hint::black_box(seed)
}

#[cfg(target_arch = "aarch64")]
#[inline(never)]
#[no_mangle]
pub unsafe extern "C" fn vmbench_k_overhead_loop(iters: u64, seed: u64) -> u64 {
    core::arch::asm!(
        "cbz {i}, 4f",
        "3:",
        "subs {i}, {i}, #1",
        "b.ne 3b",
        "4:",
        i = in(reg) iters,
        options(nostack)
    );
    core::hint::black_box(seed)
}

pub type OverheadKernel = unsafe extern "C" fn(u64, u64) -> u64;

#[used]
static OVERHEAD_KERNELS: [OverheadKernel; 1] = [vmbench_k_overhead_loop];

pub fn overhead_kernel() -> OverheadKernel {
    unsafe { core::ptr::read_volatile(&OVERHEAD_KERNELS[0]) }
}

pub struct LoopOverhead;

impl Benchmark for LoopOverhead {
    fn meta(&self) -> &'static EntryMeta {
        static M: EntryMeta = EntryMeta {
            id: "cpu.overhead.loop.v1",
            version: 1,
            description: "Loop-control overhead reference for throughput kernels",
            imp: ImplInfo {
                source: "src/bench/cpu_overhead.rs",
                kernel: "vmbench_k_overhead_loop",
                algorithm: "empty loop: one dec+jnz (x86) / subs+b.ne (aarch64) per iteration",
                isa: BASELINE_ISA,
            },
            metrics: &[M_OPS, M_NS_OP, M_CYC_OP],
        };
        &M
    }

    fn run(&self, ctx: &mut Ctx, entry: usize) -> Result<(), &'static str> {
        let cpu = ctx.default_cpu();
        ctx.pin_self(cpu);
        ctx.add_param(entry, "cpu", ParamValue::Int(cpu as i64));
        let f = overhead_kernel();
        ctx.measure(
            entry,
            ctx.cfg.target_run_ns,
            ctx.cfg.runs,
            |iters| {
                let _ = unsafe { f(iters, 0) };
                iters
            },
            |ctx, e, s| emit_rate(ctx, e, s),
        );
        Ok(())
    }
}
