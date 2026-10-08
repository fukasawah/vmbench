//! Expected-value verification.
//!
//! Runs once per benchmark, immediately before measurement. Each kernel is
//! called with fixed inputs and its output is compared against an independent
//! reference implementation (closed form where the recurrence allows it,
//! scalar simulation otherwise). Kernels are `#[inline(never)]` and are
//! reached through the same call path in both phases, so a passing check
//! applies to the machine code that is actually measured (the one covered by
//! `code_hash`).
//!
//! Benchmarks that measure an environment-dependent physical quantity have no
//! fixed expected value; those are listed explicitly and only get a protocol
//! or structural check. They are marked "no expected value" in
//! `docs/STATUS.md`.

use crate::bench::*;
use crate::runner::Ctx;
use crate::sys;
use crate::time;

const SEED: u64 = 0x0123_4567_89ab_cdef;
const K: u64 = 0x9e37_79b9_7f4a_7c15;
/// Chain constants used by the integer kernels (chains 0..7).
const CH_INT: [u64; 8] = [
    0x1111_1111_1111_1111,
    0x2222_2222_2222_2222,
    0x3333_3333_3333_3333,
    0x4444_4444_4444_4444,
    0x5555_5555_5555_5555,
    0x6666_6666_6666_6666,
    0x7777_7777_7777_7777,
    0x8888_8888_8888_8888,
];
/// Offsets used by the FP / ISA kernels (accumulator 0 uses the seed as-is).
const SMALL: [u64; 8] = [0, 0x1111, 0x2222, 0x3333, 0x4444, 0x5555, 0x6666, 0x7777];

fn check(actual: u64, expected: u64, what: &'static str) -> Result<(), &'static str> {
    if actual == expected {
        return Ok(());
    }
    // Diagnostics for a failed expected-value check.
    let mut buf = [0u8; 160];
    let mut n = 0usize;
    let mut push = |s: &[u8]| {
        for &b in s {
            if n < buf.len() {
                buf[n] = b;
                n += 1;
            }
        }
    };
    push(b"verify mismatch: ");
    push(what.as_bytes());
    push(b" actual=");
    let mut h = [0u8; 16];
    let _ = crate::fmt::hex_lower(&actual.to_be_bytes(), &mut h);
    push(&h);
    push(b" expected=");
    let _ = crate::fmt::hex_lower(&expected.to_be_bytes(), &mut h);
    push(&h);
    push(b"\n");
    let _ = crate::sys::write_all(2, &buf[..n]);
    Err(what)
}

fn wrap_pow(mut base: u64, mut exp: u64) -> u64 {
    let mut r = 1u64;
    while exp > 0 {
        if exp & 1 == 1 {
            r = r.wrapping_mul(base);
        }
        base = base.wrapping_mul(base);
        exp >>= 1;
    }
    r
}


#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "fma")]
unsafe fn fma64(a: f64, b: f64, c: f64) -> f64 {
    let mut b = b;
    core::arch::asm!("vfmadd213sd {b}, {a}, {c}",
        b = inout(xmm_reg) b, a = in(xmm_reg) a, c = in(xmm_reg) c, options(nostack));
    b
}

#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "fma")]
unsafe fn fma32(a: f32, b: f32, c: f32) -> f32 {
    let mut b = b;
    core::arch::asm!("vfmadd213ss {b}, {a}, {c}",
        b = inout(xmm_reg) b, a = in(xmm_reg) a, c = in(xmm_reg) c, options(nostack));
    b
}

#[cfg(target_arch = "aarch64")]
unsafe fn fma64(a: f64, b: f64, c: f64) -> f64 {
    let r: f64;
    core::arch::asm!("fmadd {r:d}, {a:d}, {b:d}, {c:d}",
        r = out(vreg) r, a = in(vreg) a, b = in(vreg) b, c = in(vreg) c, options(nostack));
    r
}

#[cfg(target_arch = "aarch64")]
unsafe fn fma32(a: f32, b: f32, c: f32) -> f32 {
    let r: f32;
    core::arch::asm!("fmadd {r:s}, {a:s}, {b:s}, {c:s}",
        r = out(vreg) r, a = in(vreg) a, b = in(vreg) b, c = in(vreg) c, options(nostack));
    r
}

#[cfg(target_arch = "x86_64")]
unsafe fn sqrt64(x: f64) -> f64 {
    let r: f64;
    core::arch::asm!("sqrtsd {r}, {x}", r = out(xmm_reg) r, x = in(xmm_reg) x, options(nostack));
    r
}

#[cfg(target_arch = "x86_64")]
unsafe fn sqrt32(x: f32) -> f32 {
    let r: f32;
    core::arch::asm!("sqrtss {r}, {x}", r = out(xmm_reg) r, x = in(xmm_reg) x, options(nostack));
    r
}

#[cfg(target_arch = "aarch64")]
unsafe fn sqrt64(x: f64) -> f64 {
    let r: f64;
    core::arch::asm!("fsqrt {r:d}, {x:d}", r = out(vreg) r, x = in(vreg) x, options(nostack));
    r
}

#[cfg(target_arch = "aarch64")]
unsafe fn sqrt32(x: f32) -> f32 {
    let r: f32;
    core::arch::asm!("fsqrt {r:s}, {x:s}", r = out(vreg) r, x = in(vreg) x, options(nostack));
    r
}

fn rot_sum(step: u64, n: u64) -> u32 {
    ((n % 64) * step % 64) as u32
}

fn shl_or_formula(x0: u64, m: u64) -> u64 {
    // x = (x << 3) | 1 repeated m times
    let mut v = x0;
    if 3u128 * m as u128 >= 64 {
        v = 0;
    } else {
        v <<= 3 * m;
    }
    let mut t = 0u64;
    while t < m && 3 * t < 64 {
        v |= 1u64 << (3 * t);
        t += 1;
    }
    v
}

// ---------------------------------------------------------------------------
// Integer
// ---------------------------------------------------------------------------

fn int_expected(op: u8, m: u64) -> u64 {
    let mut fold = 0u64;
    for i in 0..8 {
        let x0 = SEED ^ CH_INT[i];
        let x = match op {
            0 => x0.wrapping_add(K.wrapping_mul(m)),
            1 => x0.wrapping_sub(K.wrapping_mul(m)),
            2 => {
                if m % 2 == 1 {
                    x0 ^ K
                } else {
                    x0
                }
            }
            3 => x0 & K,
            4 => x0 | K,
            5 => x0.wrapping_mul(wrap_pow(K, m)),
            6 => x0.rotate_left(rot_sum(7, m)),
            _ => shl_or_formula(x0, m),
        };
        fold ^= x;
    }
    fold
}

fn int_lat_expected(op: u8, n: u64) -> u64 {
    let x0 = SEED | 1;
    match op {
        0 => x0.wrapping_add(K.wrapping_mul(n)),
        1 => x0.wrapping_mul(wrap_pow(K, n)),
        2 => {
            let mut x = x0;
            for _ in 0..n.min(64) {
                x = (x / 3) ^ K;
            }
            x
        }
        3 => x0.rotate_left(rot_sum(13, n)),
        4 => {
            if n % 2 == 1 {
                x0 ^ K
            } else {
                x0
            }
        }
        _ => shl_or_formula(x0, n),
    }
}

fn verify_int_tp(kernel: unsafe extern "C" fn(u64, u64) -> u64, op: u8) -> Result<(), &'static str> {
    for n in [3u64, 1_000_000] {
        let m = n * 2;
        let got = unsafe { kernel(n, SEED) };
        let expected = int_expected(op, m);
        if got != expected {
            let mut b = [0u8; 96];
            let mut len = 0usize;
            let mut push = |s: &[u8]| {
                for &c in s {
                    if len < b.len() {
                        b[len] = c;
                        len += 1;
                    }
                }
            };
            push(b"int mismatch op=");
            let mut d = [0u8; 24];
            let k = crate::fmt::u64_dec(op as u64, &mut d);
            push(&d[..k]);
            push(b" n=");
            let k = crate::fmt::u64_dec(n, &mut d);
            push(&d[..k]);
            push(b" m=");
            let k = crate::fmt::u64_dec(m, &mut d);
            push(&d[..k]);
            push(b"\n");
            let _ = crate::sys::write_all(2, &b[..len]);
            return Err("int throughput output mismatch");
        }
    }
    Ok(())
}

fn verify_int_lat(kernel: unsafe extern "C" fn(u64, u64) -> u64, op: u8) -> Result<(), &'static str> {
    for n in [3u64, 1000] {
        let got = unsafe { kernel(n, SEED) };
        check(got, int_lat_expected(op, n), "int latency output mismatch")?;
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Floating point (scalar simulation; IEEE semantics are deterministic)
// ---------------------------------------------------------------------------

#[cfg(target_arch = "x86_64")]
fn fp_tp64_sim(op: u8, n: u64) -> f64 {
    let mut x = [0f64; 8];
    for i in 0..8 {
        x[i] = (SEED ^ SMALL[i]) as f64;
    }
    let k = 1.0000001f64;
    for _ in 0..n {
        for _ in 0..2 {
            for v in x.iter_mut() {
                *v = match op {
                    0 => *v + k,
                    1 => *v * k,
                    _ => unsafe { fma64(*v, *v, k) },
                };
            }
        }
    }
    fold64(op, x)
}

#[cfg(target_arch = "x86_64")]
fn fold64(op: u8, x: [f64; 8]) -> f64 {
    if op == 2 {
        let a = unsafe { fma64(x[0], x[0], x[1]) };
        let b = unsafe { fma64(x[2], x[2], x[3]) };
        let c = unsafe { fma64(x[4], x[4], x[5]) };
        let d = unsafe { fma64(x[6], x[6], x[7]) };
        let e = unsafe { fma64(a, a, b) };
        let f = unsafe { fma64(c, c, d) };
        unsafe { fma64(e, e, f) }
    } else if op == 1 {
        let a = x[0] * x[1];
        let b = x[2] * x[3];
        let c = x[4] * x[5];
        let d = x[6] * x[7];
        let e = a * b;
        let f = c * d;
        e * f
    } else if op == 1 {
        let a = x[0] * x[1];
        let b = x[2] * x[3];
        let c = x[4] * x[5];
        let d = x[6] * x[7];
        let e = a * b;
        let f = c * d;
        e * f
    } else {
        let a = x[0] + x[1];
        let b = x[2] + x[3];
        let c = x[4] + x[5];
        let d = x[6] + x[7];
        let e = a + b;
        let f = c + d;
        e + f
    }
}

#[cfg(target_arch = "aarch64")]
fn fp_tp64_sim(op: u8, n: u64) -> f64 {
    let mut x = [0f64; 8];
    for i in 0..8 {
        x[i] = (SEED ^ SMALL[i]) as f64;
    }
    let k = 1.0000001f64;
    for _ in 0..n {
        for _ in 0..2 {
            for v in x.iter_mut() {
                *v = match op {
                    0 => *v + k,
                    1 => *v * k,
                    _ => unsafe { fma64(*v, *v, k) },
                };
            }
        }
    }
    // aarch64 folds with fadd in all cases (including the FMA kernel).
    let a = x[0] + x[1];
    let b = x[2] + x[3];
    let c = x[4] + x[5];
    let d = x[6] + x[7];
    let e = a + b;
    let f = c + d;
    e + f
}

#[cfg(target_arch = "x86_64")]
fn fp_tp32_sim(op: u8, n: u64) -> f32 {
    let mut x = [0f32; 8];
    for i in 0..8 {
        x[i] = (SEED ^ SMALL[i]) as f32;
    }
    let k = 1.0000001f32;
    for _ in 0..n {
        for _ in 0..2 {
            for v in x.iter_mut() {
                *v = match op {
                    0 => *v + k,
                    1 => *v * k,
                    _ => unsafe { fma32(*v, *v, k) },
                };
            }
        }
    }
    if op == 2 {
        let a = unsafe { fma32(x[0], x[0], x[1]) };
        let b = unsafe { fma32(x[2], x[2], x[3]) };
        let c = unsafe { fma32(x[4], x[4], x[5]) };
        let d = unsafe { fma32(x[6], x[6], x[7]) };
        let e = unsafe { fma32(a, a, b) };
        let f = unsafe { fma32(c, c, d) };
        unsafe { fma32(e, e, f) }
    } else if op == 1 {
        let a = x[0] * x[1];
        let b = x[2] * x[3];
        let c = x[4] * x[5];
        let d = x[6] * x[7];
        let e = a * b;
        let f = c * d;
        e * f
    } else {
        let a = x[0] + x[1];
        let b = x[2] + x[3];
        let c = x[4] + x[5];
        let d = x[6] + x[7];
        let e = a + b;
        let f = c + d;
        e + f
    }
}

fn fp64_lat_sim(op: u8, n: u64) -> f64 {
    let mut x: f64 = SEED as f64 * 1.0000001 + 2.0;
    let k = 1.0000001f64;
    for _ in 0..n {
        x = match op {
            0 => x + k,
            1 => x * k,
            2 => x / k,
            _ => {
                #[cfg(target_arch = "x86_64")]
                {
                    // x86 sqrt uses sqrtsd {x}, {k} with k = 1.0
                    unsafe { sqrt64(1.0) }
                }
                #[cfg(target_arch = "aarch64")]
                {
                    unsafe { sqrt64(x) }
                }
            }
        };
    }
    x
}

fn fp32_lat_sim(op: u8, n: u64) -> f32 {
    let mut x: f32 = SEED as f32 * 1.0000001 + 2.0;
    let k = 1.0000001f32;
    for _ in 0..n {
        x = match op {
            0 => x + k,
            1 => x * k,
            2 => x / k,
            _ => {
                #[cfg(target_arch = "x86_64")]
                {
                    unsafe { sqrt32(1.0) }
                }
                #[cfg(target_arch = "aarch64")]
                {
                    unsafe { sqrt32(x) }
                }
            }
        };
    }
    x
}

fn check_f64(actual: f64, expected: f64, what: &'static str) -> Result<(), &'static str> {
    if actual.to_bits() == expected.to_bits() {
        Ok(())
    } else {
        Err(what)
    }
}

// ---------------------------------------------------------------------------
// Mix
// ---------------------------------------------------------------------------

fn mix_sim(iters: u64) -> u64 {
    let mut i1 = SEED;
    let mut i2 = SEED;
    let mut i3 = SEED;
    let mut acc = SEED;
    let mut f1 = 1.0000001f64;
    let mut f2 = f1;
    let mut i = 0u64;
    while i < iters {
        i1 = i1.wrapping_add(1);
        i2 ^= i1;
        i3 = i3.wrapping_mul(3);
        acc = acc.rotate_left(7);
        f1 *= 1.0000001;
        f2 += f1;
        if i1 & 1 != 0 {
            acc = acc.wrapping_add(i2);
        } else {
            acc = acc.wrapping_sub(i3);
        }
        acc ^= 0x0123_4567_89ab_cdefu64 ^ (i1 & 2047) as u64;
        i += 1;
    }
    acc ^ f1.to_bits() ^ f2.to_bits() ^ SEED
}

// ---------------------------------------------------------------------------
// Branch
// ---------------------------------------------------------------------------

fn branch_rand_sim(n: u64) -> u64 {
    let mut x = SEED | 1;
    let mut a = 0u64;
    let mut t = 0u64;
    for _ in 0..n {
        t = x << 13;
        x ^= t;
        t = x >> 7;
        x ^= t;
        t = x << 17;
        x ^= t;
        if x & 1 != 0 {
            a = a.wrapping_add(1);
        } else {
            a = a.wrapping_sub(1);
        }
    }
    a ^ t
}

fn branch_ind_sim(n: u64) -> u64 {
    let mut x = SEED | 1;
    let mut a = 0u64;
    let mut t = 0u64;
    for _ in 0..n {
        if x & 1 != 0 {
            a = a.wrapping_sub(1);
        } else {
            a = a.wrapping_add(1);
        }
        t = x << 13;
        x ^= t;
    }
    a ^ t
}

#[cfg(target_arch = "aarch64")]
fn branch_rand_sim_arm(n: u64) -> u64 {
    let mut x = SEED | 1;
    let mut a = 0u64;
    let mut t = 0u64;
    for _ in 0..n {
        t = x ^ (x << 13);
        x = t ^ (t >> 7);
        t = x ^ (x << 17);
        if t & 1 != 0 {
            a = a.wrapping_add(1);
        } else {
            a = a.wrapping_sub(1);
        }
    }
    a ^ t
}

#[cfg(target_arch = "aarch64")]
fn branch_ind_sim_arm(n: u64) -> u64 {
    let mut x = SEED | 1;
    let mut a = 0u64;
    let mut t = 0u64;
    for _ in 0..n {
        if x & 1 != 0 {
            a = a.wrapping_sub(1);
        } else {
            a = a.wrapping_add(1);
        }
        t = x ^ (x << 13);
        x = t;
    }
    a ^ t
}

// ---------------------------------------------------------------------------
// Load/store
// ---------------------------------------------------------------------------

fn load_tp_ref(buf: *const u64, iters: u64) -> u64 {
    let mut acc = 0u64;
    for i in 0..iters {
        for j in 0..8u64 {
            let idx = ((i * 8 + j) & 2047) as usize;
            acc = acc.wrapping_add(unsafe { core::ptr::read_volatile(buf.add(idx)) });
        }
    }
    acc
}

fn chase_ref(start: usize, n: u64) -> usize {
    let mut p = start as *const u64;
    for _ in 0..n {
        p = unsafe { core::ptr::read_volatile(p) as *const u64 };
    }
    p as usize
}

// ---------------------------------------------------------------------------
// CRC32C reference (reflected polynomial 0x82F63B78)
// ---------------------------------------------------------------------------

fn crc32c_byte(mut crc: u32, byte: u8) -> u32 {
    crc ^= byte as u32;
    for _ in 0..8 {
        crc = (crc >> 1) ^ (0x82F6_3B78u32 & 0u32.wrapping_sub(crc & 1));
    }
    crc
}

fn crc32c_u64(mut crc: u32, data: u64) -> u32 {
    let b = data.to_le_bytes();
    for i in 0..8 {
        crc = crc32c_byte(crc, b[i]);
    }
    crc
}

// ---------------------------------------------------------------------------
// AES reference (FIPS-197; S-box generated from GF(2^8) inversion)
// ---------------------------------------------------------------------------

fn gf_mul(mut a: u8, mut b: u8) -> u8 {
    let mut r = 0u8;
    for _ in 0..8 {
        if b & 1 != 0 {
            r ^= a;
        }
        let hi = a & 0x80;
        a <<= 1;
        if hi != 0 {
            a ^= 0x1b;
        }
        b >>= 1;
    }
    r
}

fn aes_sbox(x: u8) -> u8 {
    let inv = if x == 0 {
        0
    } else {
        let mut r = 1u8;
        for _ in 0..254 {
            r = gf_mul(r, x);
        }
        r
    };
    let s = inv
        ^ inv.rotate_left(1)
        ^ inv.rotate_left(2)
        ^ inv.rotate_left(3)
        ^ inv.rotate_left(4)
        ^ 0x63;
    s
}

fn aes128_enc_round(state: [u8; 16], key: [u8; 16]) -> [u8; 16] {
    let mut sb = [0u8; 16];
    for i in 0..16 {
        sb[i] = aes_sbox(state[i]);
    }
    // ShiftRows (state is column-major: byte r + 4c)
    let mut t = [0u8; 16];
    for c in 0..4 {
        for r in 0..4 {
            t[r + 4 * c] = sb[r + 4 * ((c + r) % 4)];
        }
    }
    // MixColumns
    let mut o = [0u8; 16];
    for c in 0..4 {
        let a0 = t[4 * c];
        let a1 = t[4 * c + 1];
        let a2 = t[4 * c + 2];
        let a3 = t[4 * c + 3];
        o[4 * c] = gf_mul(a0, 2) ^ gf_mul(a1, 3) ^ a2 ^ a3;
        o[4 * c + 1] = a0 ^ gf_mul(a1, 2) ^ gf_mul(a2, 3) ^ a3;
        o[4 * c + 2] = a0 ^ a1 ^ gf_mul(a2, 2) ^ gf_mul(a3, 3);
        o[4 * c + 3] = gf_mul(a0, 3) ^ a1 ^ a2 ^ gf_mul(a3, 2);
    }
    // AddRoundKey
    for i in 0..16 {
        o[i] ^= key[i];
    }
    o
}

/// ARMv8 AESE: AddRoundKey, ShiftRows, SubBytes (no MixColumns).
#[cfg(target_arch = "aarch64")]
fn aese_ref(state: [u8; 16], key: [u8; 16]) -> [u8; 16] {
    let mut s = [0u8; 16];
    for i in 0..16 {
        s[i] = state[i] ^ key[i];
    }
    let mut o = [0u8; 16];
    for c in 0..4 {
        for r in 0..4 {
            o[r + 4 * c] = aes_sbox(s[r + 4 * ((c + r) % 4)]);
        }
    }
    o
}

fn xor16(a: [u8; 16], b: [u8; 16]) -> [u8; 16] {
    let mut o = [0u8; 16];
    for i in 0..16 {
        o[i] = a[i] ^ b[i];
    }
    o
}

fn seed16(seed: u64) -> [u8; 16] {
    let mut o = [0u8; 16];
    o[..8].copy_from_slice(&seed.to_le_bytes());
    o[8..].copy_from_slice(&seed.to_le_bytes());
    o
}

// ---------------------------------------------------------------------------
// Dispatch
// ---------------------------------------------------------------------------

/// Benchmarks verified against an independent expected value. The list is
/// explicit so that a new benchmark cannot inherit a classification by
/// accident: an id that matches no rule is "unclassified" and fails
/// tools/status_check.py. See docs/VERIFICATION.md.
static EXPECTED_VALUE_IDS: &[&str] = &[
    "cpu.overhead.loop.v1",
    "cpu.int.throughput.add.v1",
    "cpu.int.throughput.sub.v1",
    "cpu.int.throughput.xor.v1",
    "cpu.int.throughput.and.v1",
    "cpu.int.throughput.or.v1",
    "cpu.int.throughput.mul.v1",
    "cpu.int.throughput.rotate.v1",
    "cpu.int.throughput.shift.v1",
    "cpu.int.latency.add.v1",
    "cpu.int.latency.mul.v1",
    "cpu.int.latency.div.v1",
    "cpu.int.latency.rotate.v1",
    "cpu.int.latency.xor.v1",
    "cpu.int.latency.shift.v1",
    "cpu.fp.throughput.f64.add.v1",
    "cpu.fp.throughput.f64.mul.v1",
    "cpu.fp.throughput.f32.add.v1",
    "cpu.fp.throughput.f32.mul.v1",
    "cpu.fp.throughput.f64.fma.v1",
    "cpu.fp.throughput.f32.fma.v1",
    "cpu.fp.latency.f64.add.v1",
    "cpu.fp.latency.f64.mul.v1",
    "cpu.fp.latency.f64.div.v1",
    "cpu.fp.latency.f64.sqrt.v1",
    "cpu.fp.latency.f32.add.v1",
    "cpu.fp.latency.f32.mul.v1",
    "cpu.fp.latency.f32.div.v1",
    "cpu.fp.latency.f32.sqrt.v1",
    "cpu.mix.v1",
    "cpu.branch.predictable.v1",
    "cpu.branch.unpredictable.v1",
    "cpu.branch.indirect.v1",
    "cpu.loadstore.load.v1",
    "cpu.loadstore.store.v1",
    "cpu.loadstore.load_to_use.v1",
    "cpu.sustained.v1",
    "cpu.isa.sse2.v1",
    "cpu.isa.avx.v1",
    "cpu.isa.avx2.v1",
    "cpu.isa.fma.v1",
    "cpu.isa.avx512f.v1",
    "cpu.isa.aes.v1",
    "cpu.isa.sha.v1",
    "cpu.isa.crc32.v1",
    "cpu.isa.asimd.v1",
    "cache.latency.curve.v1",
    "memory.latency.dram.v1",
    "memory.mlp.v1",
    "memory.tlb.v1",
    "memory.page_size.v1",
    "memory.numa.v1",
];

/// Classification of the verification applied to a benchmark, published in
/// `--list` and checked against docs/STATUS.md by tools/status_check.py.
pub fn verification_kind(id: &str) -> &'static str {
    if EXPECTED_VALUE_IDS.contains(&id) {
        return "expected_value";
    }
    if id == "cpu.isa.sha2.v1" || id == "perf.counters.v1" {
        return "none";
    }
    if id.starts_with("linux.ctxswitch") || id.starts_with("linux.wakeup") {
        return "protocol";
    }
    if id.starts_with("cpu.multicore.") || id.starts_with("cpu.core2core.") || id.starts_with("cpu.atomic.") {
        return "protocol";
    }
    if id.starts_with("storage.") {
        return "functional";
    }
    if id.starts_with("memory.bandwidth.seq.") {
        return if id.ends_with(".1.v1") { "functional" } else { "protocol" };
    }
    if id.starts_with("cache.bandwidth.")
        || id == "memory.bandwidth.random.read.v1"
        || id == "memory.bandwidth.random.write.v1"
    {
        return "functional";
    }
    if id.starts_with("linux.") {
        return "functional";
    }
    "unclassified"
}

pub fn verify_benchmark(id: &str, ctx: &mut Ctx) -> Result<(), &'static str> {
    match id {
        // -- overhead ------------------------------------------------------
        "cpu.overhead.loop.v1" => check(
            unsafe { cpu_overhead::vmbench_k_overhead_loop(1000, SEED) },
            SEED,
            "overhead loop did not return seed",
        ),

        // -- integer -------------------------------------------------------
        "cpu.int.throughput.add.v1" => {
            verify_int_tp(cpu_int::vmbench_k_int_add_tp, 0)
        }
        "cpu.int.throughput.sub.v1" => {
            verify_int_tp(cpu_int::vmbench_k_int_sub_tp, 1)
        }
        "cpu.int.throughput.xor.v1" => {
            verify_int_tp(cpu_int::vmbench_k_int_xor_tp, 2)
        }
        "cpu.int.throughput.and.v1" => {
            verify_int_tp(cpu_int::vmbench_k_int_and_tp, 3)
        }
        "cpu.int.throughput.or.v1" => {
            verify_int_tp(cpu_int::vmbench_k_int_or_tp, 4)
        }
        "cpu.int.throughput.mul.v1" => {
            verify_int_tp(cpu_int::vmbench_k_int_mul_tp, 5)
        }
        "cpu.int.throughput.rotate.v1" => {
            verify_int_tp(cpu_int::vmbench_k_int_rot_tp, 6)
        }
        "cpu.int.throughput.shift.v1" => {
            verify_int_tp(cpu_int::vmbench_k_int_shl_tp, 7)
        }
        "cpu.int.latency.add.v1" => verify_int_lat(cpu_int::vmbench_k_int_add_lat, 0),
        "cpu.int.latency.mul.v1" => verify_int_lat(cpu_int::vmbench_k_int_mul_lat, 1),
        "cpu.int.latency.div.v1" => verify_int_lat(cpu_int::vmbench_k_int_div_lat, 2),
        "cpu.int.latency.rotate.v1" => verify_int_lat(cpu_int::vmbench_k_int_rot_lat, 3),
        "cpu.int.latency.xor.v1" => verify_int_lat(cpu_int::vmbench_k_int_xor_lat, 4),
        "cpu.int.latency.shift.v1" => verify_int_lat(cpu_int::vmbench_k_int_shl_lat, 5),

        // -- floating point ------------------------------------------------
        "cpu.fp.throughput.f64.add.v1" => {
            check_f64(unsafe { cpu_fp::vmbench_k_fp64_add_tp(5, SEED) }, fp_tp64_sim(0, 5), "fp64 add output mismatch")
        }
        "cpu.fp.throughput.f64.mul.v1" => {
            check_f64(unsafe { cpu_fp::vmbench_k_fp64_mul_tp(5, SEED) }, fp_tp64_sim(1, 5), "fp64 mul output mismatch")
        }
        "cpu.fp.throughput.f64.fma.v1" => {
            check_f64(unsafe { cpu_fp::vmbench_k_fp64_fma_tp(5, SEED) }, fp_tp64_sim(2, 5), "fp64 fma output mismatch")
        }
        #[cfg(target_arch = "x86_64")]
        "cpu.fp.throughput.f32.add.v1" => check_f64(
            unsafe { cpu_fp::vmbench_k_fp32_add_tp(5, SEED) } as f64,
            fp_tp32_sim(0, 5) as f64,
            "fp32 add output mismatch",
        ),
        #[cfg(target_arch = "x86_64")]
        "cpu.fp.throughput.f32.mul.v1" => check_f64(
            unsafe { cpu_fp::vmbench_k_fp32_mul_tp(5, SEED) } as f64,
            fp_tp32_sim(1, 5) as f64,
            "fp32 mul output mismatch",
        ),
        #[cfg(target_arch = "x86_64")]
        "cpu.fp.throughput.f32.fma.v1" => check_f64(
            unsafe { cpu_fp::vmbench_k_fp32_fma_tp(5, SEED) } as f64,
            fp_tp32_sim(2, 5) as f64,
            "fp32 fma output mismatch",
        ),
        "cpu.fp.latency.f64.add.v1" => check_f64(unsafe { cpu_fp::vmbench_k_fp64_add_lat(5, SEED) }, fp64_lat_sim(0, 5), "fp64 add latency mismatch"),
        "cpu.fp.latency.f64.mul.v1" => check_f64(unsafe { cpu_fp::vmbench_k_fp64_mul_lat(5, SEED) }, fp64_lat_sim(1, 5), "fp64 mul latency mismatch"),
        "cpu.fp.latency.f64.div.v1" => check_f64(unsafe { cpu_fp::vmbench_k_fp64_div_lat(5, SEED) }, fp64_lat_sim(2, 5), "fp64 div latency mismatch"),
        "cpu.fp.latency.f64.sqrt.v1" => check_f64(unsafe { cpu_fp::vmbench_k_fp64_sqrt_lat(5, SEED) }, fp64_lat_sim(3, 5), "fp64 sqrt latency mismatch"),
        "cpu.fp.latency.f32.add.v1" => check_f64(unsafe { cpu_fp::vmbench_k_fp32_add_lat(5, SEED) } as f64, fp32_lat_sim(0, 5) as f64, "fp32 add latency mismatch"),
        "cpu.fp.latency.f32.mul.v1" => check_f64(unsafe { cpu_fp::vmbench_k_fp32_mul_lat(5, SEED) } as f64, fp32_lat_sim(1, 5) as f64, "fp32 mul latency mismatch"),
        "cpu.fp.latency.f32.div.v1" => check_f64(unsafe { cpu_fp::vmbench_k_fp32_div_lat(5, SEED) } as f64, fp32_lat_sim(2, 5) as f64, "fp32 div latency mismatch"),
        "cpu.fp.latency.f32.sqrt.v1" => check_f64(unsafe { cpu_fp::vmbench_k_fp32_sqrt_lat(5, SEED) } as f64, fp32_lat_sim(3, 5) as f64, "fp32 sqrt latency mismatch"),

        // -- mix -----------------------------------------------------------
        "cpu.mix.v1" => check(
            unsafe { cpu_mix::vmbench_k_mix_iter(7, SEED) },
            mix_sim(7),
            "mix output mismatch",
        ),

        // -- branch --------------------------------------------------------
        "cpu.branch.predictable.v1" => check(
            unsafe { cpu_branch::vmbench_k_branch_pred(1000, SEED) },
            1000,
            "predictable branch count mismatch",
        ),
        #[cfg(target_arch = "x86_64")]
        "cpu.branch.unpredictable.v1" => check(
            unsafe { cpu_branch::vmbench_k_branch_rand(1000, SEED) },
            branch_rand_sim(1000),
            "unpredictable branch output mismatch",
        ),
        #[cfg(target_arch = "aarch64")]
        "cpu.branch.unpredictable.v1" => check(
            unsafe { cpu_branch::vmbench_k_branch_rand(1000, SEED) },
            branch_rand_sim_arm(1000),
            "unpredictable branch output mismatch",
        ),
        #[cfg(target_arch = "x86_64")]
        "cpu.branch.indirect.v1" => check(
            unsafe { cpu_branch::vmbench_k_branch_ind(1000, SEED) },
            branch_ind_sim(1000),
            "indirect branch output mismatch",
        ),
        #[cfg(target_arch = "aarch64")]
        "cpu.branch.indirect.v1" => check(
            unsafe { cpu_branch::vmbench_k_branch_ind(1000, SEED) },
            branch_ind_sim_arm(1000),
            "indirect branch output mismatch",
        ),

        // -- load/store ----------------------------------------------------
        "cpu.loadstore.load.v1" => {
            let mut rng = crate::rand::Rng::new(0xfeed_face_cafe_beef);
            let (buf, _) = cpu_loadstore::make_l1_buffer(ctx.arena, &mut rng);
            let p = buf.as_ptr() as *const u64;
            let got = unsafe { cpu_loadstore::vmbench_k_l1_load_tp(3, p as usize) };
            check(got, load_tp_ref(p, 3), "L1 load throughput mismatch")
        }
        "cpu.loadstore.store.v1" => {
            let mut rng = crate::rand::Rng::new(0xfeed_face_cafe_beef);
            let (buf, _) = cpu_loadstore::make_l1_buffer(ctx.arena, &mut rng);
            let p = buf.as_ptr() as *mut u64;
            unsafe {
                core::ptr::write_bytes(p as *mut u8, 0, 16 * 1024);
            }
            let _ = unsafe { cpu_loadstore::vmbench_k_l1_store_tp(3, p as usize) };
            for i in 0..3u64 {
                for j in 0..8u64 {
                    let idx = ((i * 8 + j) & 2047) as usize;
                    let v = unsafe { core::ptr::read_volatile(p.add(idx)) };
                    if v != i {
                        return Err("L1 store throughput content mismatch");
                    }
                }
            }
            Ok(())
        }
        "cpu.loadstore.load_to_use.v1" => {
            let mut rng = crate::rand::Rng::new(0xfeed_face_cafe_beef);
            let (buf, start) = cpu_loadstore::make_l1_buffer(ctx.arena, &mut rng);
            let start_ptr = buf.as_ptr() as usize + start;
            let got = unsafe { cpu_loadstore::vmbench_k_l1_chase_lat(64, start_ptr) };
            check(got as u64, chase_ref(start_ptr, 64) as u64, "L1 chase mismatch")
        }

        // -- atomic --------------------------------------------------------
        "cpu.atomic.fetch_add.1.v1" | "cpu.atomic.fetch_add.2.v1" | "cpu.atomic.fetch_add.all.v1" => {
            cpu_atomic::verify_kernels()
        }
        "cpu.atomic.swap.1.v1" => cpu_atomic::verify_kernels(),
        "cpu.atomic.cas.1.v1" | "cpu.atomic.cas.2.v1" | "cpu.atomic.cas.all.v1" => {
            cpu_atomic::verify_kernels()
        }

        // -- multicore -----------------------------------------------------
        "cpu.multicore.1.v1" | "cpu.multicore.2.v1" | "cpu.multicore.4.v1"
        | "cpu.multicore.8.v1" | "cpu.multicore.all.v1" => cpu_multicore::verify_protocol(ctx),

        // -- core-to-core --------------------------------------------------
        "cpu.core2core.0_1.v1" => cpu_core2core::verify_pair(0, 1),
        "cpu.core2core.0_2.v1" => cpu_core2core::verify_pair(0, 2),
        "cpu.core2core.0_half.v1" => cpu_core2core::verify_pair(0, (ctx.env.online_cpus.len() / 2) as u32),
        "cpu.core2core.0_last.v1" => cpu_core2core::verify_pair(0, (ctx.env.online_cpus.len() - 1) as u32),
        "cpu.core2core.smt_siblings.v1" => match cpu_core2core::sibling_pair(ctx) {
            Some((a, b)) => cpu_core2core::verify_pair(a, b),
            // Without SMT topology the benchmark itself reports "unsupported";
            // there is nothing to exercise in the verification step.
            None => Ok(()),
        },

        // -- ISA -----------------------------------------------------------
        #[cfg(target_arch = "x86_64")]
        "cpu.isa.sse2.v1" => verify_isa_int(cpu_isa::vmbench_k_isa_sse2_add, 2),
        #[cfg(target_arch = "x86_64")]
        "cpu.isa.avx2.v1" => verify_isa_int(cpu_isa::vmbench_k_isa_avx2_add, 4),
        #[cfg(target_arch = "x86_64")]
        "cpu.isa.avx512f.v1" => verify_isa_int(cpu_isa::vmbench_k_isa_avx512_add, 8),
        #[cfg(target_arch = "x86_64")]
        "cpu.isa.avx.v1" => verify_isa_pd(cpu_isa::vmbench_k_isa_avx_add, false),
        #[cfg(target_arch = "x86_64")]
        "cpu.isa.fma.v1" => verify_isa_pd(cpu_isa::vmbench_k_isa_fma, true),
        #[cfg(target_arch = "x86_64")]
        "cpu.isa.aes.v1" => verify_isa_aes_x86(),
        #[cfg(target_arch = "x86_64")]
        "cpu.isa.sha.v1" => verify_isa_sha_x86(),
        #[cfg(target_arch = "x86_64")]
        "cpu.isa.crc32.v1" => verify_isa_crc_x86(),
        #[cfg(target_arch = "aarch64")]
        "cpu.isa.asimd.v1" => verify_isa_neon(),
        #[cfg(target_arch = "aarch64")]
        "cpu.isa.aes.v1" => verify_isa_aes_arm(),
        #[cfg(target_arch = "aarch64")]
        "cpu.isa.sha2.v1" => Ok(()), // no independent reference for SHA256H
        #[cfg(target_arch = "aarch64")]
        "cpu.isa.crc32.v1" => verify_isa_crc_arm(),

        // -- memory latency -------------------------------------------------
        "cache.latency.curve.v1" | "memory.latency.dram.v1" | "memory.page_size.v1"
        | "memory.numa.v1" => verify_chase_kernel(ctx),
        "memory.mlp.v1" => verify_mlp(ctx),
        "memory.tlb.v1" => verify_tlb_chase(ctx),

        // -- bandwidth ------------------------------------------------------
        "cache.bandwidth.l1.read.v1" | "cache.bandwidth.l2.read.v1" | "cache.bandwidth.llc.read.v1"
        | "memory.bandwidth.seq.read.1.v1" | "memory.bandwidth.random.read.v1" => {
            mem_bw::verify_single(0)
        }
        "cache.bandwidth.l1.write.v1" | "cache.bandwidth.l2.write.v1" | "cache.bandwidth.llc.write.v1"
        | "memory.bandwidth.seq.write.1.v1" | "memory.bandwidth.random.write.v1" => {
            mem_bw::verify_single(1)
        }
        "cache.bandwidth.l1.copy.v1" | "cache.bandwidth.l2.copy.v1" | "cache.bandwidth.llc.copy.v1"
        | "memory.bandwidth.seq.copy.1.v1" => mem_bw::verify_single(2),
        "memory.bandwidth.seq.triad.1.v1" => mem_bw::verify_single(3),
        "memory.bandwidth.seq.scale.1.v1" => mem_bw::verify_single(4),
        "memory.bandwidth.seq.add.1.v1" => mem_bw::verify_single(5),
        "memory.bandwidth.seq.read.all.v1" => mem_bw::verify_protocol(ctx, 0),
        "memory.bandwidth.seq.write.all.v1" => mem_bw::verify_protocol(ctx, 1),
        "memory.bandwidth.seq.copy.all.v1" => mem_bw::verify_protocol(ctx, 2),
        "memory.bandwidth.seq.triad.all.v1" => mem_bw::verify_protocol(ctx, 3),
        "memory.bandwidth.seq.scale.all.v1" => mem_bw::verify_protocol(ctx, 4),
        "memory.bandwidth.seq.add.all.v1" => mem_bw::verify_protocol(ctx, 5),

        // -- Linux / scheduler ----------------------------------------------
        "linux.syscall.getpid.v1" => linux::verify_syscall(),
        "linux.ctxswitch.futex.v1" => {
            linux::verify_ctxswitch(ctx.cpu_at(0), ctx.cpu_at(1))
        }
        "linux.wakeup.futex.v1" => linux::verify_wakeup(ctx),
        "linux.scheduler.jitter.v1" => linux::verify_jitter(),
        "linux.pagefault.minor.v1" => linux::verify_first_touch(),

        // -- perf counters (no expected value; hardware counters) -----------
        "perf.counters.v1" => {
            if perf::probe() {
                Ok(())
            } else {
                Err("perf_event_open unavailable")
            }
        }

        // -- storage --------------------------------------------------------
        "storage.seq.read.v1" | "storage.seq.write.v1"
        | "storage.sync.fdatasync.4k.v1" | "storage.sync.dsync.4k.v1"
        | "storage.streams.read.4k.sweep.v1" | "storage.streams.write.4k.sweep.v1"
        | "storage.streams.mixed70_30.4k.sweep.v1"
        | "storage.uring.read.4k.sweep.v1" | "storage.uring.write.4k.sweep.v1"
        | "storage.uring.mixed70_30.4k.sweep.v1"
        | "storage.sustained.write.v1"
        | "storage.buffered.read.warm.v1" => storage::verify_io(ctx),

        // -- sustained (underlying kernel is verified above) ----------------
        "cpu.sustained.v1" => check(
            unsafe { cpu_mix::vmbench_k_mix_iter(7, SEED) },
            mix_sim(7),
            "sustained mix output mismatch",
        ),

        // Unknown ids must not be silently accepted.
        other => {
            let _ = other;
            Err("no expected value registered for this benchmark")
        }
    }
}

// ---------------------------------------------------------------------------
// ISA references
// ---------------------------------------------------------------------------

#[cfg(target_arch = "x86_64")]
fn verify_isa_int(
    kernel: unsafe extern "C" fn(u64, u64) -> u64,
    lanes: u64,
) -> Result<(), &'static str> {
    let n = 3u64;
    let mut sum = 0u64;
    for i in 0..8 {
        sum = sum.wrapping_add(((SEED ^ SMALL[i]) as i64 as u64).wrapping_add(2 * n));
    }
    let _ = lanes;
    check(unsafe { kernel(n, SEED) }, sum, "ISA integer add mismatch")
}

#[cfg(target_arch = "x86_64")]
fn verify_isa_pd(
    kernel: unsafe extern "C" fn(u64, u64) -> u64,
    fma: bool,
) -> Result<(), &'static str> {
    let n = 3u64;
    let mut x = [0f64; 8];
    for i in 0..8 {
        x[i] = (SEED ^ SMALL[i]) as i64 as f64;
    }
    let k = 1.0f64;
    for _ in 0..n {
        for _ in 0..2 {
            for v in x.iter_mut() {
                *v = if fma { unsafe { fma64(*v, *v, k) } } else { *v + k };
            }
        }
    }
    let mut a = x[0];
    let mut b = x[2];
    let mut c = x[4];
    let mut d = x[6];
    if fma {
        a = unsafe { fma64(a, a, x[1]) };
        b = unsafe { fma64(b, b, x[3]) };
        c = unsafe { fma64(c, c, x[5]) };
        d = unsafe { fma64(d, d, x[7]) };
        a = unsafe { fma64(a, a, b) };
        c = unsafe { fma64(c, c, d) };
        a = unsafe { fma64(a, a, c) };
    } else {
        a += x[1];
        b += x[3];
        c += x[5];
        d += x[7];
        a += b;
        c += d;
        a += c;
    }
    check(unsafe { kernel(n, SEED) }, a.to_bits(), "ISA double add mismatch")
}

#[cfg(target_arch = "x86_64")]
fn verify_isa_aes_x86() -> Result<(), &'static str> {
    if aes_sbox(0) != 0x63 || aes_sbox(1) != 0x7c {
        return Err("AES reference S-box self-check failed");
    }
    let n = 2u64;
    let key = [1u8; 16];
    let mut x = [[0u8; 16]; 8];
    for i in 0..8 {
        x[i] = seed16((SEED ^ SMALL[i]) as i64 as u64);
    }
    for _ in 0..n {
        for _ in 0..2 {
            for v in x.iter_mut() {
                *v = aes128_enc_round(*v, key);
            }
        }
    }
    let mut acc = x[0];
    for i in 1..8 {
        acc = xor16(acc, x[i]);
    }
    let expected = u64::from_le_bytes(acc[..8].try_into().unwrap());
    check(unsafe { cpu_isa::vmbench_k_isa_aes(n, SEED) }, expected, "ISA AES mismatch")
}

#[cfg(target_arch = "x86_64")]
fn verify_isa_crc_x86() -> Result<(), &'static str> {
    let n = 2u64;
    let mut x = [0u64; 8];
    for i in 0..8 {
        x[i] = SEED ^ SMALL[i];
    }
    for _ in 0..n {
        for _ in 0..2 {
            for v in x.iter_mut() {
                *v = crc32c_u64(*v as u32, K) as u64;
            }
        }
    }
    let mut acc = 0u64;
    for v in x {
        acc ^= v;
    }
    check(unsafe { cpu_isa::vmbench_k_isa_crc32(n, SEED) }, acc, "ISA CRC32 mismatch")
}

#[cfg(target_arch = "x86_64")]
fn verify_isa_sha_x86() -> Result<(), &'static str> {
    // SHA256MSG1: dest dword i += sigma0(src dword i) (Intel SDM).
    fn sigma0(x: u32) -> u32 {
        x.rotate_right(7) ^ x.rotate_right(18) ^ (x >> 3)
    }
    let n = 2u64;
    let s = SEED as i32;
    let mut x = [[0u32; 4]; 8];
    for (i, v) in x.iter_mut().enumerate() {
        *v = [s as u32 ^ (SMALL[i] as u32), s as u32 ^ (SMALL[i] as u32), s as u32 ^ (SMALL[i] as u32), s as u32 ^ (SMALL[i] as u32)];
        // NOTE: the kernel uses _mm_set1_epi32(s ^ const) so all lanes share the value.
    }
    let key = [1u32; 4];
    for _ in 0..n {
        for _ in 0..2 {
            for v in x.iter_mut() {
                let old = *v;
                v[0] = old[0].wrapping_add(sigma0(old[1]));
                v[1] = old[1].wrapping_add(sigma0(old[2]));
                v[2] = old[2].wrapping_add(sigma0(old[3]));
                v[3] = old[3].wrapping_add(sigma0(key[0]));
            }
        }
    }
    let mut acc = [0u32; 4];
    for v in x {
        for i in 0..4 {
            acc[i] ^= v[i];
        }
    }
    let expected = u64::from_le_bytes([acc[0] as u8, (acc[0] >> 8) as u8, (acc[0] >> 16) as u8, (acc[0] >> 24) as u8, acc[1] as u8, (acc[1] >> 8) as u8, (acc[1] >> 16) as u8, (acc[1] >> 24) as u8]);
    check(unsafe { cpu_isa::vmbench_k_isa_sha(n, SEED) }, expected, "ISA SHA mismatch")
}

#[cfg(target_arch = "aarch64")]
fn verify_isa_neon() -> Result<(), &'static str> {
    let n = 3u64;
    let mut x = [[0u32; 4]; 8];
    for i in 0..8 {
        let v = (SEED as u32) ^ (SMALL[i] as u32);
        x[i] = [v; 4];
    }
    for _ in 0..n {
        for _ in 0..2 {
            for v in x.iter_mut() {
                for lane in v.iter_mut() {
                    *lane = lane.wrapping_add(1);
                }
            }
        }
    }
    let mut acc = [0u32; 4];
    for v in x {
        for i in 0..4 {
            acc[i] = acc[i].wrapping_add(v[i]);
        }
    }
    let expected = acc[0] as u64;
    check(unsafe { cpu_isa::vmbench_k_isa_neon_add(n, SEED) }, expected, "ISA NEON mismatch")
}

#[cfg(target_arch = "aarch64")]
fn verify_isa_aes_arm() -> Result<(), &'static str> {
    if aes_sbox(0) != 0x63 || aes_sbox(1) != 0x7c {
        return Err("AES reference S-box self-check failed");
    }
    let n = 2u64;
    let key = [1u8; 16];
    let mut x = [[0u8; 16]; 8];
    for (i, v) in x.iter_mut().enumerate() {
        let s = ((SEED as u32) ^ (SMALL[i] as u32)).to_le_bytes();
        for c in 0..4 {
            v[c * 4..c * 4 + 4].copy_from_slice(&s);
        }
    }
    for _ in 0..n {
        for _ in 0..2 {
            for v in x.iter_mut() {
                *v = aese_ref(*v, key);
            }
        }
    }
    let mut acc = x[0];
    for i in 1..8 {
        acc = xor16(acc, x[i]);
    }
    let expected = u64::from_le_bytes(acc[..8].try_into().unwrap());
    check(unsafe { cpu_isa::vmbench_k_isa_aes(n, SEED) }, expected, "ISA AES mismatch")
}

#[cfg(target_arch = "aarch64")]
fn verify_isa_crc_arm() -> Result<(), &'static str> {
    let n = 2u64;
    let mut x = [0u32; 8];
    for i in 0..8 {
        x[i] = (SEED ^ SMALL[i]) as u32;
    }
    for _ in 0..n {
        for _ in 0..2 {
            for v in x.iter_mut() {
                *v = crc32c_u64(*v, K);
            }
        }
    }
    let mut acc = 0u32;
    for v in x {
        acc ^= v;
    }
    check(unsafe { cpu_isa::vmbench_k_isa_crc32(n, SEED) }, acc as u64, "ISA CRC32 mismatch")
}

// ---------------------------------------------------------------------------
// Memory references
// ---------------------------------------------------------------------------

fn verify_chase_kernel(ctx: &mut Ctx) -> Result<(), &'static str> {
    let entries = 1024usize;
    let mut buf = [0u64; 1024];
    let mut rng = crate::rand::Rng::new(0x1234_5678);
    let start = common::build_pointer_cycle(
        ctx.arena,
        buf.as_mut_ptr() as *mut u8,
        entries,
        8,
        &mut rng,
    );
    let start_ptr = buf.as_ptr() as usize + start;
    let got = unsafe { mem_latency::vmbench_k_chase(64, start_ptr) };
    check(got as u64, chase_ref(start_ptr, 64) as u64, "pointer chase mismatch")
}

fn verify_tlb_chase(ctx: &mut Ctx) -> Result<(), &'static str> {
    let pages = 64usize;
    let len = pages * 4096;
    let buf = common::BigBuf::new(len).ok_or("mmap failed")?;
    buf.touch();
    let mut rng = crate::rand::Rng::new(0x0bad_f00d_dead_beef);
    let off = common::build_pointer_cycle(ctx.arena, buf.as_ptr(), pages, 4096, &mut rng);
    let start = buf.as_ptr() as usize + off;
    let got = unsafe { mem_latency::vmbench_k_chase(128, start) };
    check(got, chase_ref(start, 128) as u64, "page-granular pointer chase mismatch")
}

fn verify_mlp(ctx: &mut Ctx) -> Result<(), &'static str> {
    // K independent cycles over 16 regions; compare the XOR fold.
    let entries = 256usize;
    let mut buf = [0u64; 256 * 16];
    let mut starts = [0u64; 16];
    let mut rng = crate::rand::Rng::new(0x8765_4321);
    for j in 0..16 {
        let base = unsafe { buf.as_mut_ptr().add(j * entries) as *mut u8 };
        let off = common::build_pointer_cycle(ctx.arena, base, entries, 8, &mut rng);
        starts[j] = (base as usize + off) as u64;
    }
    for (idx, k) in [1usize, 2, 4, 8, 16].iter().enumerate() {
        let f: unsafe extern "C" fn(u64, u64) -> u64 = match idx {
            0 => mem_latency::vmbench_k_mlp1,
            1 => mem_latency::vmbench_k_mlp2,
            2 => mem_latency::vmbench_k_mlp4,
            3 => mem_latency::vmbench_k_mlp8,
            _ => mem_latency::vmbench_k_mlp16,
        };
        let n = 8u64;
        let got = unsafe { f(n, starts.as_ptr() as u64) };
        let mut p = [0u64; 16];
        for j in 0..*k {
            p[j] = starts[j];
        }
        for _ in 0..n {
            for j in 0..*k {
                p[j] = unsafe { core::ptr::read_volatile(p[j] as *const u64) };
            }
        }
        let mut expected = 0u64;
        for j in 0..*k {
            expected ^= p[j];
        }
        check(got, expected, "MLP output mismatch")?;
    }
    Ok(())
}
