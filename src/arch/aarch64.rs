use core::arch::asm;

#[inline(always)]
pub unsafe fn raw_syscall6(
    n: usize,
    a1: usize,
    a2: usize,
    a3: usize,
    a4: usize,
    a5: usize,
    a6: usize,
) -> isize {
    let ret: isize;
    asm!(
        "svc 0",
        inlateout("x0") a1 as isize => ret,
        in("x1") a2,
        in("x2") a3,
        in("x3") a4,
        in("x4") a5,
        in("x5") a6,
        in("x8") n,
        options(nostack)
    );
    ret
}

#[inline(always)]
pub unsafe fn read_cycles() -> u64 {
    let v: u64;
    asm!("mrs {v}, cntvct_el0", v = out(reg) v, options(nomem, nostack, preserves_flags));
    v
}

pub unsafe fn cycles_hz_hint() -> Option<u64> {
    let v: u64;
    asm!("mrs {v}, cntfrq_el0", v = out(reg) v, options(nomem, nostack, preserves_flags));
    if v >= 1_000_000 && v <= 10_000_000_000 {
        Some(v)
    } else {
        None
    }
}

#[derive(Clone, Copy, Default)]
pub struct IsaFlags {
    pub fp: bool,
    pub asimd: bool,
    pub aes: bool,
    pub pmull: bool,
    pub sha1: bool,
    pub sha2: bool,
    pub crc32: bool,
    pub atomics: bool,
    pub fp16: bool,
    pub dotprod: bool,
    pub sve: bool,
    pub sve2: bool,
    pub i8mm: bool,
    pub bf16: bool,
}

pub fn detect_isa() -> IsaFlags {
    let hw = crate::rt::hwcap();
    let hw2 = crate::rt::hwcap2();
    IsaFlags {
        fp: hw & (1 << 0) != 0,
        asimd: hw & (1 << 1) != 0,
        aes: hw & (1 << 3) != 0,
        pmull: hw & (1 << 4) != 0,
        sha1: hw & (1 << 5) != 0,
        sha2: hw & (1 << 6) != 0,
        crc32: hw & (1 << 7) != 0,
        atomics: hw & (1 << 8) != 0,
        fp16: hw & (1 << 9) != 0,
        dotprod: hw & (1 << 20) != 0,
        sve: hw & (1 << 22) != 0,
        sve2: hw2 & (1 << 1) != 0,
        i8mm: hw2 & (1 << 7) != 0,
        bf16: hw2 & (1 << 8) != 0,
    }
}

pub fn arch_name() -> &'static str {
    "aarch64"
}
