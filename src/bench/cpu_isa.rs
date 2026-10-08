use crate::bench::common::*;
use crate::bench::Benchmark;
use crate::runner::{Ctx, EntryMeta, ImplInfo, ParamValue};

// ---------------------------------------------------------------------------
// ISA-specific throughput kernels. These are separate from the baseline CPU
// scores: each extension gets its own benchmark id / implementation info, and
// unsupported extensions are reported as such.
// ---------------------------------------------------------------------------

#[cfg(target_arch = "x86_64")]
macro_rules! isa_x86_vec {
    ($fname:ident, $feat:literal, $rc:ident, $ty:ty, $setup:path, $cast:ty, $arr:ty, $mn:literal) => {
        #[target_feature(enable = $feat)]
        #[inline(never)]
        #[no_mangle]
        pub unsafe extern "C" fn $fname(iters: u64, seed: u64) -> u64 {
            let s = seed as i64;
            let mut x0 = $setup(s as $cast);
            let mut x1 = $setup((s ^ 0x1111) as $cast);
            let mut x2 = $setup((s ^ 0x2222) as $cast);
            let mut x3 = $setup((s ^ 0x3333) as $cast);
            let mut x4 = $setup((s ^ 0x4444) as $cast);
            let mut x5 = $setup((s ^ 0x5555) as $cast);
            let mut x6 = $setup((s ^ 0x6666) as $cast);
            let mut x7 = $setup((s ^ 0x7777) as $cast);
            let k = $setup(1 as $cast);
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
                k = in($rc) k,
                x0 = inout($rc) x0 => x0,
                x1 = inout($rc) x1 => _,
                x2 = inout($rc) x2 => _,
                x3 = inout($rc) x3 => _,
                x4 = inout($rc) x4 => _,
                x5 = inout($rc) x5 => _,
                x6 = inout($rc) x6 => _,
                x7 = inout($rc) x7 => _,
                options(nostack)
            );
            let arr: $arr = core::mem::transmute(x0);
            arr[0]
        }
    };
}

#[cfg(target_arch = "x86_64")]
macro_rules! isa_x86_vec3 {
    ($fname:ident, $feat:literal, $rc:ident, $ty:ty, $setup:path, $cast:ty, $arr:ty, $mn:literal) => {
        #[target_feature(enable = $feat)]
        #[inline(never)]
        #[no_mangle]
        pub unsafe extern "C" fn $fname(iters: u64, seed: u64) -> u64 {
            let s = seed as i64;
            let mut x0 = $setup(s as $cast);
            let mut x1 = $setup((s ^ 0x1111) as $cast);
            let mut x2 = $setup((s ^ 0x2222) as $cast);
            let mut x3 = $setup((s ^ 0x3333) as $cast);
            let mut x4 = $setup((s ^ 0x4444) as $cast);
            let mut x5 = $setup((s ^ 0x5555) as $cast);
            let mut x6 = $setup((s ^ 0x6666) as $cast);
            let mut x7 = $setup((s ^ 0x7777) as $cast);
            let k = $setup(1 as $cast);
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
                k = in($rc) k,
                x0 = inout($rc) x0 => x0,
                x1 = inout($rc) x1 => _,
                x2 = inout($rc) x2 => _,
                x3 = inout($rc) x3 => _,
                x4 = inout($rc) x4 => _,
                x5 = inout($rc) x5 => _,
                x6 = inout($rc) x6 => _,
                x7 = inout($rc) x7 => _,
                options(nostack)
            );
            let arr: $arr = core::mem::transmute(x0);
            arr[0]
        }
    };
}

#[cfg(target_arch = "x86_64")]
isa_x86_vec!(
    vmbench_k_isa_sse2_add,
    "sse2",
    xmm_reg,
    core::arch::x86_64::__m128i,
    core::arch::x86_64::_mm_set1_epi64x,
    i64,
    [u64; 2],
    "paddq"
);
#[cfg(target_arch = "x86_64")]
isa_x86_vec3!(
    vmbench_k_isa_avx_add,
    "avx",
    ymm_reg,
    core::arch::x86_64::__m256d,
    core::arch::x86_64::_mm256_set1_pd,
    f64,
    [u64; 4],
    "vaddpd"
);
#[cfg(target_arch = "x86_64")]
isa_x86_vec3!(
    vmbench_k_isa_avx2_add,
    "avx2",
    ymm_reg,
    core::arch::x86_64::__m256i,
    core::arch::x86_64::_mm256_set1_epi64x,
    i64,
    [u64; 4],
    "vpaddq"
);
#[cfg(target_arch = "x86_64")]
isa_x86_vec3!(
    vmbench_k_isa_fma,
    "fma",
    ymm_reg,
    core::arch::x86_64::__m256d,
    core::arch::x86_64::_mm256_set1_pd,
    f64,
    [u64; 4],
    "vfmadd213pd"
);
#[cfg(target_arch = "x86_64")]
isa_x86_vec3!(
    vmbench_k_isa_avx512_add,
    "avx512f",
    zmm_reg,
    core::arch::x86_64::__m512i,
    core::arch::x86_64::_mm512_set1_epi64,
    i64,
    [u64; 8],
    "vpaddq"
);

#[repr(align(16))]
struct Align16<T>(T);
static AES_RKEY: Align16<[u8; 16]> = Align16([1u8; 16]);

#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "aes")]
#[inline(never)]
#[no_mangle]
pub unsafe extern "C" fn vmbench_k_isa_aes(iters: u64, seed: u64) -> u64 {
    let s = seed as i64;
    let mut x0 = core::arch::x86_64::_mm_set1_epi64x(s);
    let mut x1 = core::arch::x86_64::_mm_set1_epi64x(s ^ 0x1111);
    let mut x2 = core::arch::x86_64::_mm_set1_epi64x(s ^ 0x2222);
    let mut x3 = core::arch::x86_64::_mm_set1_epi64x(s ^ 0x3333);
    let mut x4 = core::arch::x86_64::_mm_set1_epi64x(s ^ 0x4444);
    let mut x5 = core::arch::x86_64::_mm_set1_epi64x(s ^ 0x5555);
    let mut x6 = core::arch::x86_64::_mm_set1_epi64x(s ^ 0x6666);
    let mut x7 = core::arch::x86_64::_mm_set1_epi64x(s ^ 0x7777);
    // Load the 128-bit round key from memory: LLVM can mis-materialise a
    // constant vector operand as a zero-extended 64-bit move, which would
    // silently change the key.
    let key_ptr = core::ptr::addr_of!(AES_RKEY.0) as *const u8;
    let k = core::ptr::read_volatile(key_ptr as *const core::arch::x86_64::__m128i);
    let out: u64;
    core::arch::asm!(
        "test {i}, {i}",
        "jz 3f",
        "2:",
        "aesenc {x0}, {k}",
        "aesenc {x1}, {k}",
        "aesenc {x2}, {k}",
        "aesenc {x3}, {k}",
        "aesenc {x4}, {k}",
        "aesenc {x5}, {k}",
        "aesenc {x6}, {k}",
        "aesenc {x7}, {k}",
        "aesenc {x0}, {k}",
        "aesenc {x1}, {k}",
        "aesenc {x2}, {k}",
        "aesenc {x3}, {k}",
        "aesenc {x4}, {k}",
        "aesenc {x5}, {k}",
        "aesenc {x6}, {k}",
        "aesenc {x7}, {k}",
        "dec {i}",
        "jnz 2b",
        "3:",
        "pxor {x0}, {x1}",
        "pxor {x2}, {x3}",
        "pxor {x4}, {x5}",
        "pxor {x6}, {x7}",
        "pxor {x0}, {x2}",
        "pxor {x4}, {x6}",
        "pxor {x0}, {x4}",
        "movq {out}, {x0}",
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
        out = lateout(reg) out,
        options(nostack)
    );
    out
}

#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "sha")]
#[inline(never)]
#[no_mangle]
pub unsafe extern "C" fn vmbench_k_isa_sha(iters: u64, seed: u64) -> u64 {
    let s = seed as i32;
    let mut x0 = core::arch::x86_64::_mm_set1_epi32(s);
    let mut x1 = core::arch::x86_64::_mm_set1_epi32(s ^ 0x1111);
    let mut x2 = core::arch::x86_64::_mm_set1_epi32(s ^ 0x2222);
    let mut x3 = core::arch::x86_64::_mm_set1_epi32(s ^ 0x3333);
    let mut x4 = core::arch::x86_64::_mm_set1_epi32(s ^ 0x4444);
    let mut x5 = core::arch::x86_64::_mm_set1_epi32(s ^ 0x5555);
    let mut x6 = core::arch::x86_64::_mm_set1_epi32(s ^ 0x6666);
    let mut x7 = core::arch::x86_64::_mm_set1_epi32(s ^ 0x7777);
    let k = core::arch::x86_64::_mm_set1_epi32(1);
    let out: u64;
    core::arch::asm!(
        "test {i}, {i}",
        "jz 3f",
        "2:",
        "sha256msg1 {x0}, {k}",
        "sha256msg1 {x1}, {k}",
        "sha256msg1 {x2}, {k}",
        "sha256msg1 {x3}, {k}",
        "sha256msg1 {x4}, {k}",
        "sha256msg1 {x5}, {k}",
        "sha256msg1 {x6}, {k}",
        "sha256msg1 {x7}, {k}",
        "sha256msg1 {x0}, {k}",
        "sha256msg1 {x1}, {k}",
        "sha256msg1 {x2}, {k}",
        "sha256msg1 {x3}, {k}",
        "sha256msg1 {x4}, {k}",
        "sha256msg1 {x5}, {k}",
        "sha256msg1 {x6}, {k}",
        "sha256msg1 {x7}, {k}",
        "dec {i}",
        "jnz 2b",
        "3:",
        "pxor {x0}, {x1}",
        "pxor {x2}, {x3}",
        "pxor {x4}, {x5}",
        "pxor {x6}, {x7}",
        "pxor {x0}, {x2}",
        "pxor {x4}, {x6}",
        "pxor {x0}, {x4}",
        "movq {out}, {x0}",
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
        out = lateout(reg) out,
        options(nostack)
    );
    out
}

#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "sse4.2")]
#[inline(never)]
#[no_mangle]
pub unsafe extern "C" fn vmbench_k_isa_crc32(iters: u64, seed: u64) -> u64 {
    let mut x0 = seed;
    let mut x1 = seed ^ 0x1111;
    let mut x2 = seed ^ 0x2222;
    let mut x3 = seed ^ 0x3333;
    let mut x4 = seed ^ 0x4444;
    let mut x5 = seed ^ 0x5555;
    let mut x6 = seed ^ 0x6666;
    let mut x7 = seed ^ 0x7777;
    let out: u64;
    core::arch::asm!(
        "test {i}, {i}",
        "jz 3f",
        "2:",
        "crc32 {x0}, {k}",
        "crc32 {x1}, {k}",
        "crc32 {x2}, {k}",
        "crc32 {x3}, {k}",
        "crc32 {x4}, {k}",
        "crc32 {x5}, {k}",
        "crc32 {x6}, {k}",
        "crc32 {x7}, {k}",
        "crc32 {x0}, {k}",
        "crc32 {x1}, {k}",
        "crc32 {x2}, {k}",
        "crc32 {x3}, {k}",
        "crc32 {x4}, {k}",
        "crc32 {x5}, {k}",
        "crc32 {x6}, {k}",
        "crc32 {x7}, {k}",
        "dec {i}",
        "jnz 2b",
        "3:",
        "xor {x0}, {x1}",
        "xor {x2}, {x3}",
        "xor {x4}, {x5}",
        "xor {x6}, {x7}",
        "xor {x0}, {x2}",
        "xor {x4}, {x6}",
        "xor {x0}, {x4}",
        "mov {out}, {x0}",
        i = in(reg) iters,
        k = in(reg) 0x9e37_79b9_7f4a_7c15u64,
        x0 = inout(reg) x0 => x0,
        x1 = inout(reg) x1 => _,
        x2 = inout(reg) x2 => _,
        x3 = inout(reg) x3 => _,
        x4 = inout(reg) x4 => _,
        x5 = inout(reg) x5 => _,
        x6 = inout(reg) x6 => _,
        x7 = inout(reg) x7 => _,
        out = lateout(reg) out,
        options(nostack)
    );
    out
}

#[cfg(target_arch = "aarch64")]
macro_rules! isa_arm_vec {
    ($fname:ident, $feat:literal, $ty:ty, $setup:path, $mn:literal) => {
        #[target_feature(enable = $feat)]
        #[inline(never)]
        #[no_mangle]
        pub unsafe extern "C" fn $fname(iters: u64, seed: u64) -> u64 {
            let s = seed;
            let mut x0 = $setup(s as u32);
            let mut x1 = $setup((s ^ 0x1111) as u32);
            let mut x2 = $setup((s ^ 0x2222) as u32);
            let mut x3 = $setup((s ^ 0x3333) as u32);
            let mut x4 = $setup((s ^ 0x4444) as u32);
            let mut x5 = $setup((s ^ 0x5555) as u32);
            let mut x6 = $setup((s ^ 0x6666) as u32);
            let mut x7 = $setup((s ^ 0x7777) as u32);
            let k = $setup(1);
            core::arch::asm!(
                "cbz {i}, 3f",
                "2:",
                concat!(
                    $mn, " {x0}.4s, {x0}.4s, {k}.4s\n", $mn, " {x1}.4s, {x1}.4s, {k}.4s\n",
                    $mn, " {x2}.4s, {x2}.4s, {k}.4s\n", $mn, " {x3}.4s, {x3}.4s, {k}.4s\n",
                    $mn, " {x4}.4s, {x4}.4s, {k}.4s\n", $mn, " {x5}.4s, {x5}.4s, {k}.4s\n",
                    $mn, " {x6}.4s, {x6}.4s, {k}.4s\n", $mn, " {x7}.4s, {x7}.4s, {k}.4s\n",
                    $mn, " {x0}.4s, {x0}.4s, {k}.4s\n", $mn, " {x1}.4s, {x1}.4s, {k}.4s\n",
                    $mn, " {x2}.4s, {x2}.4s, {k}.4s\n", $mn, " {x3}.4s, {x3}.4s, {k}.4s\n",
                    $mn, " {x4}.4s, {x4}.4s, {k}.4s\n", $mn, " {x5}.4s, {x5}.4s, {k}.4s\n",
                    $mn, " {x6}.4s, {x6}.4s, {k}.4s\n", $mn, " {x7}.4s, {x7}.4s, {k}.4s\n",
                ),
                "subs {i}, {i}, #1",
                "b.ne 2b",
                "3:",
                concat!($mn, " {x0}.4s, {x0}.4s, {x1}.4s"),
                concat!($mn, " {x2}.4s, {x2}.4s, {x3}.4s"),
                concat!($mn, " {x4}.4s, {x4}.4s, {x5}.4s"),
                concat!($mn, " {x6}.4s, {x6}.4s, {x7}.4s"),
                concat!($mn, " {x0}.4s, {x0}.4s, {x2}.4s"),
                concat!($mn, " {x4}.4s, {x4}.4s, {x6}.4s"),
                concat!($mn, " {x0}.4s, {x0}.4s, {x4}.4s"),
                i = in(reg) iters,
                k = in(vreg) k,
                x0 = inout(vreg) x0 => x0,
                x1 = inout(vreg) x1 => _,
                x2 = inout(vreg) x2 => _,
                x3 = inout(vreg) x3 => _,
                x4 = inout(vreg) x4 => _,
                x5 = inout(vreg) x5 => _,
                x6 = inout(vreg) x6 => _,
                x7 = inout(vreg) x7 => _,
                options(nostack)
            );
            let arr: [u32; 4] = core::mem::transmute(x0);
            arr[0] as u64
        }
    };
}

#[cfg(target_arch = "aarch64")]
isa_arm_vec!(
    vmbench_k_isa_neon_add,
    "neon",
    core::arch::aarch64::uint32x4_t,
    core::arch::aarch64::vdupq_n_u32,
    "add"
);

#[cfg(target_arch = "aarch64")]
#[target_feature(enable = "aes")]
#[inline(never)]
#[no_mangle]
pub unsafe extern "C" fn vmbench_k_isa_aes(iters: u64, seed: u64) -> u64 {
    let mut x0 = core::arch::aarch64::vdupq_n_u32(seed as u32);
    let mut x1 = core::arch::aarch64::vdupq_n_u32((seed ^ 0x1111) as u32);
    let mut x2 = core::arch::aarch64::vdupq_n_u32((seed ^ 0x2222) as u32);
    let mut x3 = core::arch::aarch64::vdupq_n_u32((seed ^ 0x3333) as u32);
    let mut x4 = core::arch::aarch64::vdupq_n_u32((seed ^ 0x4444) as u32);
    let mut x5 = core::arch::aarch64::vdupq_n_u32((seed ^ 0x5555) as u32);
    let mut x6 = core::arch::aarch64::vdupq_n_u32((seed ^ 0x6666) as u32);
    let mut x7 = core::arch::aarch64::vdupq_n_u32((seed ^ 0x7777) as u32);
    let k = core::arch::aarch64::vdupq_n_u32(0x0101_0101);
    core::arch::asm!(
        "cbz {i}, 4f",
        "3:",
        concat!(
            "aese {x0}.16b, {k}.16b\n", "aese {x1}.16b, {k}.16b\n",
            "aese {x2}.16b, {k}.16b\n", "aese {x3}.16b, {k}.16b\n",
            "aese {x4}.16b, {k}.16b\n", "aese {x5}.16b, {k}.16b\n",
            "aese {x6}.16b, {k}.16b\n", "aese {x7}.16b, {k}.16b\n",
            "aese {x0}.16b, {k}.16b\n", "aese {x1}.16b, {k}.16b\n",
            "aese {x2}.16b, {k}.16b\n", "aese {x3}.16b, {k}.16b\n",
            "aese {x4}.16b, {k}.16b\n", "aese {x5}.16b, {k}.16b\n",
            "aese {x6}.16b, {k}.16b\n", "aese {x7}.16b, {k}.16b\n",
        ),
        "subs {i}, {i}, #1",
        "b.ne 3b",
        "4:",
        "eor {x0}.16b, {x0}.16b, {x1}.16b",
        "eor {x2}.16b, {x2}.16b, {x3}.16b",
        "eor {x4}.16b, {x4}.16b, {x5}.16b",
        "eor {x6}.16b, {x6}.16b, {x7}.16b",
        "eor {x0}.16b, {x0}.16b, {x2}.16b",
        "eor {x4}.16b, {x4}.16b, {x6}.16b",
        "eor {x0}.16b, {x0}.16b, {x4}.16b",
        i = in(reg) iters,
        k = in(vreg) k,
        x0 = inout(vreg) x0 => x0,
        x1 = inout(vreg) x1 => _,
        x2 = inout(vreg) x2 => _,
        x3 = inout(vreg) x3 => _,
        x4 = inout(vreg) x4 => _,
        x5 = inout(vreg) x5 => _,
        x6 = inout(vreg) x6 => _,
        x7 = inout(vreg) x7 => _,
        options(nostack)
    );
    let arr: [u32; 4] = core::mem::transmute(x0);
    (arr[0] as u64) | ((arr[1] as u64) << 32)
}

#[cfg(target_arch = "aarch64")]
#[target_feature(enable = "sha2")]
#[inline(never)]
#[no_mangle]
pub unsafe extern "C" fn vmbench_k_isa_sha(iters: u64, seed: u64) -> u64 {
    let mut x0 = core::arch::aarch64::vdupq_n_u32(seed as u32);
    let mut x1 = core::arch::aarch64::vdupq_n_u32((seed ^ 0x1111) as u32);
    let mut x2 = core::arch::aarch64::vdupq_n_u32((seed ^ 0x2222) as u32);
    let mut x3 = core::arch::aarch64::vdupq_n_u32((seed ^ 0x3333) as u32);
    let mut x4 = core::arch::aarch64::vdupq_n_u32((seed ^ 0x4444) as u32);
    let mut x5 = core::arch::aarch64::vdupq_n_u32((seed ^ 0x5555) as u32);
    let mut x6 = core::arch::aarch64::vdupq_n_u32((seed ^ 0x6666) as u32);
    let mut x7 = core::arch::aarch64::vdupq_n_u32((seed ^ 0x7777) as u32);
    let k = core::arch::aarch64::vdupq_n_u32(1);
    core::arch::asm!(
        "cbz {i}, 4f",
        "3:",
        concat!(
            "sha256h {x0:q}, {x0:q}, {k}.4s\n", "sha256h {x1:q}, {x1:q}, {k}.4s\n",
            "sha256h {x2:q}, {x2:q}, {k}.4s\n", "sha256h {x3:q}, {x3:q}, {k}.4s\n",
            "sha256h {x4:q}, {x4:q}, {k}.4s\n", "sha256h {x5:q}, {x5:q}, {k}.4s\n",
            "sha256h {x6:q}, {x6:q}, {k}.4s\n", "sha256h {x7:q}, {x7:q}, {k}.4s\n",
            "sha256h {x0:q}, {x0:q}, {k}.4s\n", "sha256h {x1:q}, {x1:q}, {k}.4s\n",
            "sha256h {x2:q}, {x2:q}, {k}.4s\n", "sha256h {x3:q}, {x3:q}, {k}.4s\n",
            "sha256h {x4:q}, {x4:q}, {k}.4s\n", "sha256h {x5:q}, {x5:q}, {k}.4s\n",
            "sha256h {x6:q}, {x6:q}, {k}.4s\n", "sha256h {x7:q}, {x7:q}, {k}.4s\n",
        ),
        "subs {i}, {i}, #1",
        "b.ne 3b",
        "4:",
        "eor {x0}.16b, {x0}.16b, {x1}.16b",
        "eor {x2}.16b, {x2}.16b, {x3}.16b",
        "eor {x4}.16b, {x4}.16b, {x5}.16b",
        "eor {x6}.16b, {x6}.16b, {x7}.16b",
        "eor {x0}.16b, {x0}.16b, {x2}.16b",
        "eor {x4}.16b, {x4}.16b, {x6}.16b",
        "eor {x0}.16b, {x0}.16b, {x4}.16b",
        i = in(reg) iters,
        k = in(vreg) k,
        x0 = inout(vreg) x0 => x0,
        x1 = inout(vreg) x1 => _,
        x2 = inout(vreg) x2 => _,
        x3 = inout(vreg) x3 => _,
        x4 = inout(vreg) x4 => _,
        x5 = inout(vreg) x5 => _,
        x6 = inout(vreg) x6 => _,
        x7 = inout(vreg) x7 => _,
        options(nostack)
    );
    let arr: [u32; 4] = core::mem::transmute(x0);
    arr[0] as u64
}

#[cfg(target_arch = "aarch64")]
#[target_feature(enable = "crc")]
#[inline(never)]
#[no_mangle]
pub unsafe extern "C" fn vmbench_k_isa_crc32(iters: u64, seed: u64) -> u64 {
    let mut x0 = seed as u32;
    let mut x1 = (seed ^ 0x1111) as u32;
    let mut x2 = (seed ^ 0x2222) as u32;
    let mut x3 = (seed ^ 0x3333) as u32;
    let mut x4 = (seed ^ 0x4444) as u32;
    let mut x5 = (seed ^ 0x5555) as u32;
    let mut x6 = (seed ^ 0x6666) as u32;
    let mut x7 = (seed ^ 0x7777) as u32;
    core::arch::asm!(
        "cbz {i}, 4f",
        "3:",
        "crc32x {x0:w}, {x0:w}, {k}",
        "crc32x {x1:w}, {x1:w}, {k}",
        "crc32x {x2:w}, {x2:w}, {k}",
        "crc32x {x3:w}, {x3:w}, {k}",
        "crc32x {x4:w}, {x4:w}, {k}",
        "crc32x {x5:w}, {x5:w}, {k}",
        "crc32x {x6:w}, {x6:w}, {k}",
        "crc32x {x7:w}, {x7:w}, {k}",
        "crc32x {x0:w}, {x0:w}, {k}",
        "crc32x {x1:w}, {x1:w}, {k}",
        "crc32x {x2:w}, {x2:w}, {k}",
        "crc32x {x3:w}, {x3:w}, {k}",
        "crc32x {x4:w}, {x4:w}, {k}",
        "crc32x {x5:w}, {x5:w}, {k}",
        "crc32x {x6:w}, {x6:w}, {k}",
        "crc32x {x7:w}, {x7:w}, {k}",
        "subs {i}, {i}, #1",
        "b.ne 3b",
        "4:",
        "eor {x0:w}, {x0:w}, {x1:w}",
        "eor {x2:w}, {x2:w}, {x3:w}",
        "eor {x4:w}, {x4:w}, {x5:w}",
        "eor {x6:w}, {x6:w}, {x7:w}",
        "eor {x0:w}, {x0:w}, {x2:w}",
        "eor {x4:w}, {x4:w}, {x6:w}",
        "eor {x0:w}, {x0:w}, {x4:w}",
        i = in(reg) iters,
        k = in(reg) 0x9e37_79b9_7f4a_7c15u64,
        x0 = inout(reg) x0 => x0,
        x1 = inout(reg) x1 => _,
        x2 = inout(reg) x2 => _,
        x3 = inout(reg) x3 => _,
        x4 = inout(reg) x4 => _,
        x5 = inout(reg) x5 => _,
        x6 = inout(reg) x6 => _,
        x7 = inout(reg) x7 => _,
        options(nostack)
    );
    x0 as u64
}

pub type IsaKernel = unsafe extern "C" fn(u64, u64) -> u64;

#[inline(never)]
pub fn call_isa(table: &'static [IsaKernel], idx: usize, iters: u64, seed: u64) -> u64 {
    let f = unsafe { core::ptr::read_volatile(&table[idx]) };
    unsafe { f(iters, seed) }
}

#[cfg(target_arch = "x86_64")]
#[used]
static ISA_KERNELS: [IsaKernel; 8] = [
    vmbench_k_isa_sse2_add,
    vmbench_k_isa_avx_add,
    vmbench_k_isa_avx2_add,
    vmbench_k_isa_fma,
    vmbench_k_isa_avx512_add,
    vmbench_k_isa_aes,
    vmbench_k_isa_sha,
    vmbench_k_isa_crc32,
];

#[cfg(target_arch = "aarch64")]
#[used]
static ISA_KERNELS: [IsaKernel; 4] = [
    vmbench_k_isa_neon_add,
    vmbench_k_isa_aes,
    vmbench_k_isa_sha,
    vmbench_k_isa_crc32,
];

#[cfg(target_arch = "x86_64")]
static ISA_NAMES: [&str; 8] = [
    "vmbench_k_isa_sse2_add",
    "vmbench_k_isa_avx_add",
    "vmbench_k_isa_avx2_add",
    "vmbench_k_isa_fma",
    "vmbench_k_isa_avx512_add",
    "vmbench_k_isa_aes",
    "vmbench_k_isa_sha",
    "vmbench_k_isa_crc32",
];

#[cfg(target_arch = "aarch64")]
static ISA_NAMES: [&str; 4] = [
    "vmbench_k_isa_neon_add",
    "vmbench_k_isa_aes",
    "vmbench_k_isa_sha",
    "vmbench_k_isa_crc32",
];

fn isa_supported(ctx: &Ctx, name: &str) -> bool {
    ctx.env
        .isa_summary
        .iter()
        .any(|(n, v)| *n == name && *v)
}

// The isa_summary keys are the CPU feature names (e.g. "avx2"), not the
// benchmark ids, so `supported` uses a small mapping passed via $feat.
macro_rules! isa_bench2 {
    ($struct_name:ident, $idx:literal, $feat:literal, $op:literal, $desc:literal, $isa_str:literal) => {
        pub struct $struct_name;
        impl Benchmark for $struct_name {
            fn meta(&self) -> &'static EntryMeta {
                static M: EntryMeta = EntryMeta {
                    id: concat!("cpu.isa.", $feat, ".v1"),
                    version: 1,
                    description: $desc,
                    imp: ImplInfo {
                        source: "src/bench/cpu_isa.rs",
                        kernel: ISA_NAMES[$idx],
                        algorithm: "8 independent vector accumulators, 16 unrolled ops per iteration; units = 16 vector ops/iter",
                        isa: $isa_str,
                    },
                    metrics: &[M_OPS, M_NS_OP, M_CYC_OP, M_CYC_OP_NET],
                };
                &M
            }
            fn supported(&self, ctx: &Ctx) -> Result<(), &'static str> {
                if isa_supported(ctx, $feat) {
                    Ok(())
                } else {
                    Err("extension not supported by cpu")
                }
            }
            fn run(&self, ctx: &mut Ctx, entry: usize) -> Result<(), &'static str> {
                let cpu = ctx.default_cpu();
                ctx.pin_self(cpu);
                ctx.add_param(entry, "op", ParamValue::Str($op));
                ctx.add_param(entry, "cpu", ParamValue::Int(cpu as i64));
                let seed = 0x0123_4567_89ab_cdefu64;
                ctx.measure(
                    entry,
                    ctx.cfg.target_run_ns,
                    ctx.cfg.runs,
                    |iters| {
                        let _ = call_isa(&ISA_KERNELS, $idx, iters, seed);
                        iters.saturating_mul(16)
                    },
                    |ctx, e, s| emit_rate_net(ctx, e, s, 16),
                );
                Ok(())
            }
        }
    };
}

#[cfg(target_arch = "x86_64")]
isa_bench2!(IsaSse2Add, 0, "sse2", "paddq", "SSE2 packed 64-bit add throughput", "x86_64-sse2");
#[cfg(target_arch = "x86_64")]
isa_bench2!(IsaAvxAdd, 1, "avx", "vaddpd", "AVX packed double add throughput", "x86_64-avx");
#[cfg(target_arch = "x86_64")]
isa_bench2!(IsaAvx2Add, 2, "avx2", "vpaddq", "AVX2 packed 64-bit add throughput", "x86_64-avx2");
#[cfg(target_arch = "x86_64")]
isa_bench2!(IsaFma, 3, "fma", "vfmadd213pd", "FMA packed double throughput", "x86_64-fma");
#[cfg(target_arch = "x86_64")]
isa_bench2!(
    IsaAvx512Add,
    4,
    "avx512f",
    "vpaddq",
    "AVX-512 packed 64-bit add throughput",
    "x86_64-avx512f"
);
#[cfg(target_arch = "x86_64")]
isa_bench2!(IsaAes, 5, "aes", "aesenc", "AES-NI aesenc throughput", "x86_64-aes");
#[cfg(target_arch = "x86_64")]
isa_bench2!(IsaSha, 6, "sha", "sha256msg1", "SHA-NI sha256msg1 throughput", "x86_64-sha");
#[cfg(target_arch = "x86_64")]
isa_bench2!(
    IsaCrc32,
    7,
    "crc32",
    "crc32q",
    "SSE4.2 CRC32C throughput",
    "x86_64-crc32"
);

#[cfg(target_arch = "aarch64")]
isa_bench2!(
    IsaNeonAdd,
    0,
    "asimd",
    "add.4s",
    "NEON packed 32-bit add throughput",
    "aarch64-neon"
);
#[cfg(target_arch = "aarch64")]
isa_bench2!(
    IsaAes,
    1,
    "aes",
    "aese",
    "ARMv8 AES aese throughput",
    "aarch64-aes"
);
#[cfg(target_arch = "aarch64")]
isa_bench2!(
    IsaSha,
    2,
    "sha2",
    "sha256h",
    "ARMv8 SHA-256 sha256h throughput",
    "aarch64-sha2"
);
#[cfg(target_arch = "aarch64")]
isa_bench2!(
    IsaCrc32,
    3,
    "crc32",
    "crc32x",
    "ARMv8 CRC32 throughput",
    "aarch64-crc32"
);
