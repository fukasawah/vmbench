use crate::bench::common::*;
use crate::bench::Benchmark;
use crate::runner::{Ctx, EntryMeta, ImplInfo, ParamValue};

/// 16 KiB L1-resident scratch buffer for the composite workload.
static mut MIX_BUF: [u64; 2048] = [0x0123_4567_89ab_cdef; 2048];

#[inline(never)]
pub fn mix_buf_addr() -> usize {
    let p = core::ptr::addr_of_mut!(MIX_BUF) as *mut u64;
    unsafe {
        // Fault the pages in and make the content non-constant to the loader.
        let mut i = 0;
        while i < 2048 {
            core::ptr::write_volatile(p.add(i), 0x0123_4567_89ab_cdef ^ i as u64);
            i += 1;
        }
    }
    p as usize
}

// ---------------------------------------------------------------------------
// Composite mix kernel (fixed instruction sequence, documented below).
//
// Per iteration:
//   i1 += 1; i2 ^= i1; i3 *= 3; i4 = rotl(i4, 7)
//   f1 *= 1.0000001; f2 += f1
//   if (i1 & 1) != 0 { acc += i2 } else { acc -= i3 }
//   acc ^= buf[(i1 & 2047)]        (16 KiB L1 working set)
// ---------------------------------------------------------------------------

#[cfg(target_arch = "x86_64")]
#[inline(never)]
#[no_mangle]
pub unsafe extern "C" fn vmbench_k_mix_iter(iters: u64, seed: u64) -> u64 {
    let buf = mix_buf_addr();
    let out: u64;
    core::arch::asm!(
        "mov r8, r14",
        "mov r9, r14",
        "mov r10, r14",
        "mov r11, r14",
        "mov rax, r14",
        "test rcx, rcx",
        "jz 3f",
        "2:",
        "add r8, 1",
        "xor r9, r8",
        "imul r10, r10, 3",
        "rol r11, 7",
        "mulsd {f1}, {k}",
        "addsd {f2}, {f1}",
        "test r8, 1",
        "jz 4f",
        "add r11, r9",
        "jmp 5f",
        "4:",
        "sub r11, r10",
        "5:",
        "mov rdx, r8",
        "and rdx, 2047",
        "xor r11, [r15 + rdx*8]",
        "dec rcx",
        "jnz 2b",
        "3:",
        "movq rdx, {f1}",
        "xor r11, rdx",
        "movq rdx, {f2}",
        "xor r11, rdx",
        "xor r11, rax",
        "mov {out}, r11",
        in("r14") seed,
        in("r15") buf,
        in("rcx") iters,
        f1 = inout(xmm_reg) core::arch::x86_64::_mm_set1_pd(1.0000001) => _,
        f2 = inout(xmm_reg) core::arch::x86_64::_mm_set1_pd(1.0000001) => _,
        k = in(xmm_reg) core::arch::x86_64::_mm_set1_pd(1.0000001),
        out = lateout(reg) out,
        lateout("r8") _,
        lateout("r9") _,
        lateout("r10") _,
        lateout("r11") _,
        lateout("rax") _,
        lateout("rdx") _,
        options(nostack)
    );
    out
}

#[cfg(target_arch = "aarch64")]
#[inline(never)]
#[no_mangle]
pub unsafe extern "C" fn vmbench_k_mix_iter(iters: u64, seed: u64) -> u64 {
    let buf = mix_buf_addr();
    let out: u64;
    core::arch::asm!(
        "mov x8, x20",
        "mov x9, x20",
        "mov x10, x20",
        "mov x11, x20",
        "mov x0, x20",
        "cbz x2, 3f",
        "2:",
        "add x8, x8, #1",
        "eor x9, x9, x8",
        "mul x10, x10, x3",
        "ror x11, x11, #57",
        "fmul {f1:d}, {f1:d}, {k:d}",
        "fadd {f2:d}, {f2:d}, {f1:d}",
        "tst x8, #1",
        "b.eq 4f",
        "add x11, x11, x9",
        "b 5f",
        "4:",
        "sub x11, x11, x10",
        "5:",
        "and x5, x8, #2047",
        "ldr x6, [x21, x5, lsl #3]",
        "eor x11, x11, x6",
        "subs x2, x2, #1",
        "b.ne 2b",
        "3:",
        "fmov x6, {f1:d}",
        "eor x11, x11, x6",
        "fmov x6, {f2:d}",
        "eor x11, x11, x6",
        "eor x11, x11, x0",
        "mov {out}, x11",
        in("x20") seed,
        in("x21") buf,
        in("x2") iters,
        in("x3") 3u64,
        f1 = inout(vreg) 1.0000001f64 => _,
        f2 = inout(vreg) 1.0000001f64 => _,
        k = in(vreg) 1.0000001f64,
        out = lateout(reg) out,
        lateout("x0") _,
        lateout("x5") _,
        lateout("x6") _,
        lateout("x8") _,
        lateout("x9") _,
        lateout("x10") _,
        lateout("x11") _,
        options(nostack)
    );
    out
}

pub struct MixIter;

impl Benchmark for MixIter {
    fn meta(&self) -> &'static EntryMeta {
        static M: EntryMeta = EntryMeta {
            id: "cpu.mix.v1",
            version: 1,
            description: "Fixed composite integer/FP/branch/L1-load workload",
            imp: ImplInfo {
                source: "src/bench/cpu_mix.rs",
                kernel: "vmbench_k_mix_iter",
                algorithm: "per iter: i1+=1; i2^=i1; i3*=3; acc=rotl(acc,7); f1*=1.0000001; f2+=f1; branch on i1&1 (acc+=i2 else acc-=i3); acc^=buf[i1&2047] (16 KiB); init i1=i2=i3=acc=seed, f1=f2=1.0000001",
                isa: BASELINE_ISA,
            },
            metrics: &[M_ITER, M_NS_ITER, M_CYC_ITER],
        };
        &M
    }

    fn run(&self, ctx: &mut Ctx, entry: usize) -> Result<(), &'static str> {
        let cpu = ctx.default_cpu();
        ctx.pin_self(cpu);
        ctx.add_param(entry, "working_set_bytes", ParamValue::Int(16384));
        ctx.add_param(entry, "cpu", ParamValue::Int(cpu as i64));
        let seed = 0x0123_4567_89ab_cdefu64;
        ctx.measure(
            entry,
            ctx.cfg.target_run_ns,
            ctx.cfg.runs,
            |iters| {
                let _ = unsafe { vmbench_k_mix_iter(iters, seed) };
                iters
            },
            |ctx, e, s| {
                let secs = (s.duration_ns as f64) / 1e9;
                if secs > 0.0 {
                    ctx.scalar(e, "iterations_per_sec", s.units as f64 / secs);
                }
                ctx.scalar(e, "ns_per_iter", s.duration_ns as f64 / s.units.max(1) as f64);
                if crate::time::cycles_hz() > 0 {
                    ctx.scalar(
                        e,
                        "cycles_per_iter",
                        s.cycles as f64 / s.units.max(1) as f64,
                    );
                }
            },
        );
        Ok(())
    }
}
