#[cfg(target_arch = "x86_64")]
mod x86_64;
#[cfg(target_arch = "x86_64")]
pub use x86_64::*;

#[cfg(target_arch = "aarch64")]
mod aarch64;
#[cfg(target_arch = "aarch64")]
pub use aarch64::*;

#[inline(always)]
pub unsafe fn syscall0(n: usize) -> isize {
    raw_syscall6(n, 0, 0, 0, 0, 0, 0)
}

#[inline(always)]
pub unsafe fn syscall1(n: usize, a1: usize) -> isize {
    raw_syscall6(n, a1, 0, 0, 0, 0, 0)
}

#[inline(always)]
pub unsafe fn syscall2(n: usize, a1: usize, a2: usize) -> isize {
    raw_syscall6(n, a1, a2, 0, 0, 0, 0)
}

#[inline(always)]
pub unsafe fn syscall3(n: usize, a1: usize, a2: usize, a3: usize) -> isize {
    raw_syscall6(n, a1, a2, a3, 0, 0, 0)
}

#[inline(always)]
pub unsafe fn syscall4(n: usize, a1: usize, a2: usize, a3: usize, a4: usize) -> isize {
    raw_syscall6(n, a1, a2, a3, a4, 0, 0)
}

#[inline(always)]
pub unsafe fn syscall5(n: usize, a1: usize, a2: usize, a3: usize, a4: usize, a5: usize) -> isize {
    raw_syscall6(n, a1, a2, a3, a4, a5, 0)
}

#[inline(always)]
pub unsafe fn syscall6(
    n: usize,
    a1: usize,
    a2: usize,
    a3: usize,
    a4: usize,
    a5: usize,
    a6: usize,
) -> isize {
    raw_syscall6(n, a1, a2, a3, a4, a5, a6)
}
