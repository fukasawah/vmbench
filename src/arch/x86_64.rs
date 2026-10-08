use core::arch::{asm, x86_64::__cpuid, x86_64::__cpuid_count};

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
        "syscall",
        inlateout("rax") n as isize => ret,
        in("rdi") a1,
        in("rsi") a2,
        in("rdx") a3,
        in("r10") a4,
        in("r8") a5,
        in("r9") a6,
        lateout("rcx") _,
        lateout("r11") _,
        options(nostack)
    );
    ret
}

#[inline(always)]
pub unsafe fn read_cycles() -> u64 {
    core::arch::x86_64::_rdtsc()
}

pub unsafe fn cycles_hz_hint() -> Option<u64> {
    let max = __cpuid(0x8000_0000).eax;
    if max >= 0x8000_0007 {
        let inv = __cpuid(0x8000_0007).edx & (1 << 8) != 0;
        if !inv {
            return None;
        }
    }
    let r15 = __cpuid_count(0x15, 0);
    if r15.eax != 0 && r15.ebx != 0 && r15.ecx != 0 {
        return Some((r15.ecx as u64) * (r15.ebx as u64) / (r15.eax as u64));
    }
    let max = __cpuid(0).eax;
    if max >= 0x16 {
        let r16 = __cpuid(0x16);
        if r16.eax != 0 {
            return Some(r16.eax as u64 * 1_000_000);
        }
    }
    None
}

#[derive(Clone, Copy, Default)]
pub struct IsaFlags {
    pub sse2: bool,
    pub sse3: bool,
    pub ssse3: bool,
    pub sse41: bool,
    pub sse42: bool,
    pub avx: bool,
    pub avx2: bool,
    pub fma: bool,
    pub bmi1: bool,
    pub bmi2: bool,
    pub avx512f: bool,
    pub avx512bw: bool,
    pub avx512vl: bool,
    pub avx512dq: bool,
    pub avx512vbmi: bool,
    pub avx512vnni: bool,
    pub aes: bool,
    pub sha: bool,
    pub crc32: bool,
    pub popcnt: bool,
    pub pclmulqdq: bool,
    pub invariant_tsc: bool,
}

pub unsafe fn detect_isa() -> IsaFlags {
    let mut f = IsaFlags::default();
    let l0 = __cpuid(0);
    let max_basic = l0.eax;
    if max_basic >= 1 {
        let l1 = __cpuid(1);
        f.sse2 = l1.edx & (1 << 26) != 0;
        f.sse3 = l1.ecx & (1 << 0) != 0;
        f.pclmulqdq = l1.ecx & (1 << 1) != 0;
        f.ssse3 = l1.ecx & (1 << 9) != 0;
        f.fma = l1.ecx & (1 << 12) != 0;
        f.sse41 = l1.ecx & (1 << 19) != 0;
        f.sse42 = l1.ecx & (1 << 20) != 0;
        f.popcnt = l1.ecx & (1 << 23) != 0;
        f.aes = l1.ecx & (1 << 25) != 0;
        f.crc32 = f.sse42;
        let osxsave = l1.ecx & (1 << 27) != 0;
        let avx_hw = l1.ecx & (1 << 28) != 0;
        let xcr0 = if osxsave { xgetbv(0) } else { 0 };
        f.avx = avx_hw && (xcr0 & 0b110) == 0b110;
        let has_zmm = (xcr0 & 0b1110_0000) == 0b1110_0000;
        if max_basic >= 7 {
            let l7 = __cpuid_count(7, 0);
            f.bmi1 = l7.ebx & (1 << 3) != 0;
            f.avx2 = f.avx && (l7.ebx & (1 << 5) != 0);
            f.bmi2 = l7.ebx & (1 << 8) != 0;
            f.avx512f = f.avx && has_zmm && (l7.ebx & (1 << 16) != 0);
            f.avx512dq = f.avx512f && (l7.ebx & (1 << 17) != 0);
            f.sha = l7.ebx & (1 << 29) != 0;
            f.avx512bw = f.avx512f && (l7.ebx & (1 << 30) != 0);
            f.avx512vl = f.avx512f && (l7.ebx & (1 << 31) != 0);
            f.avx512vbmi = f.avx512f && (l7.ecx & (1 << 1) != 0);
            f.avx512vnni = f.avx512f && (l7.ecx & (1 << 11) != 0);
        }
    }
    let max_ext = __cpuid(0x8000_0000).eax;
    if max_ext >= 0x8000_0007 {
        f.invariant_tsc = __cpuid(0x8000_0007).edx & (1 << 8) != 0;
    }
    f
}

unsafe fn xgetbv(index: u32) -> u64 {
    let lo: u32;
    let hi: u32;
    asm!("xgetbv", in("ecx") index, out("eax") lo, out("edx") hi, options(nostack));
    ((hi as u64) << 32) | lo as u64
}

#[derive(Clone, Copy)]
pub struct CpuidIds {
    pub vendor: [u8; 12],
    pub brand: [u8; 48],
    pub family: u32,
    pub model: u32,
    pub stepping: u32,
    pub hypervisor: bool,
    pub hv_vendor: [u8; 12],
    pub hv_vendor_len: usize,
    pub base_mhz: u32,
    pub max_mhz: u32,
}

impl Default for CpuidIds {
    fn default() -> Self {
        CpuidIds {
            vendor: [0; 12],
            brand: [0; 48],
            family: 0,
            model: 0,
            stepping: 0,
            hypervisor: false,
            hv_vendor: [0; 12],
            hv_vendor_len: 0,
            base_mhz: 0,
            max_mhz: 0,
        }
    }
}

pub unsafe fn cpuid_ids() -> CpuidIds {
    let mut ids = CpuidIds::default();
    let l0 = __cpuid(0);
    let mut vendor = [0u8; 12];
    vendor[0..4].copy_from_slice(&l0.ebx.to_le_bytes());
    vendor[4..8].copy_from_slice(&l0.edx.to_le_bytes());
    vendor[8..12].copy_from_slice(&l0.ecx.to_le_bytes());
    ids.vendor = vendor;
    let l1 = __cpuid(1);
    let base = (l1.eax >> 8) & 0xf;
    let ext_base = (l1.eax >> 16) & 0xf;
    let family = if base == 0xf { base + ext_base } else { base };
    let model_base = (l1.eax >> 4) & 0xf;
    let model_ext = (l1.eax >> 16) & 0xf;
    let model = if base == 0x6 || base == 0xf {
        (model_ext << 4) | model_base
    } else {
        model_base
    };
    ids.family = family;
    ids.model = model;
    ids.stepping = l1.eax & 0xf;
    ids.hypervisor = l1.ecx & (1 << 31) != 0;
    if ids.hypervisor {
        let hv = __cpuid(0x4000_0000);
        let mut v = [0u8; 12];
        v[0..4].copy_from_slice(&hv.ebx.to_le_bytes());
        v[4..8].copy_from_slice(&hv.ecx.to_le_bytes());
        v[8..12].copy_from_slice(&hv.edx.to_le_bytes());
        ids.hv_vendor = v;
        ids.hv_vendor_len = 12;
    }
    let max_ext = __cpuid(0x8000_0000).eax;
    if max_ext >= 0x8000_0004 {
        let mut brand = [0u8; 48];
        for (i, leaf) in (0x8000_0002u32..=0x8000_0004).enumerate() {
            let r = __cpuid(leaf);
            brand[i * 16..i * 16 + 4].copy_from_slice(&r.eax.to_le_bytes());
            brand[i * 16 + 4..i * 16 + 8].copy_from_slice(&r.ebx.to_le_bytes());
            brand[i * 16 + 8..i * 16 + 12].copy_from_slice(&r.ecx.to_le_bytes());
            brand[i * 16 + 12..i * 16 + 16].copy_from_slice(&r.edx.to_le_bytes());
        }
        ids.brand = brand;
    }
    if l0.eax >= 0x16 {
        let r16 = __cpuid(0x16);
        ids.base_mhz = r16.eax;
        ids.max_mhz = r16.ebx;
    }
    ids
}

pub fn arch_name() -> &'static str {
    "x86_64"
}
