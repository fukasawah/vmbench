use crate::arch;
use crate::time::Timespec;
use core::ffi::c_void;

pub const AT_FDCWD: i32 = -100;
pub const AT_REMOVEDIR: i32 = 0x200;

pub const O_RDONLY: u32 = 0;
pub const O_WRONLY: u32 = 1;
pub const O_RDWR: u32 = 2;
pub const O_CREAT: u32 = 0o100;
pub const O_EXCL: u32 = 0o200;
pub const O_TRUNC: u32 = 0o1000;
#[cfg(target_arch = "x86_64")]
pub const O_DIRECT: u32 = 0o40000;
#[cfg(target_arch = "x86_64")]
pub const O_DIRECTORY: u32 = 0o200000;
// asm-generic fcntl.h: O_DIRECT and O_DIRECTORY are swapped relative to x86.
#[cfg(target_arch = "aarch64")]
pub const O_DIRECT: u32 = 0o200000;
#[cfg(target_arch = "aarch64")]
pub const O_DIRECTORY: u32 = 0o40000;
pub const O_DSYNC: u32 = 0o10000;
pub const O_CLOEXEC: u32 = 0o2000000;

pub const PROT_READ: u32 = 1;
pub const PROT_WRITE: u32 = 2;

pub const MAP_SHARED: u32 = 1;
pub const MAP_PRIVATE: u32 = 2;
pub const MAP_ANONYMOUS: u32 = 0x20;
pub const MAP_POPULATE: u32 = 0x8000;
pub const MAP_HUGETLB: u32 = 0x40000;
pub const MAP_NORESERVE: u32 = 0x4000;

pub const MADV_NORMAL: i32 = 0;
pub const MADV_RANDOM: i32 = 1;
pub const MADV_SEQUENTIAL: i32 = 2;
pub const MADV_WILLNEED: i32 = 3;
pub const MADV_DONTNEED: i32 = 4;
pub const MADV_HUGEPAGE: i32 = 14;
pub const MADV_NOHUGEPAGE: i32 = 15;
pub const MADV_DONTFORK: i32 = 10;
pub const MADV_DOFORK: i32 = 11;

pub const FUTEX_WAIT: i32 = 0;
pub const FUTEX_WAKE: i32 = 1;
pub const FUTEX_PRIVATE_FLAG: i32 = 128;

pub const WNOHANG: i32 = 1;
pub const SIGCHLD: usize = 17;

pub const RUSAGE_SELF: i32 = 0;
pub const RUSAGE_CHILDREN: i32 = -1;

pub const MPOL_DEFAULT: i32 = 0;
pub const MPOL_BIND: i32 = 2;
pub const MPOL_INTERLEAVE: i32 = 3;
pub const MPOL_LOCAL: i32 = 4;

pub const STATX_BASIC_STATS: u32 = 0x7ff;
pub const STATX_BTIME: u32 = 0x800;
pub const STATX_MNT_ID: u32 = 0x1000;
pub const STATX_DIOALIGN: u32 = 0x2000;

pub const PERF_TYPE_HARDWARE: u32 = 0;
pub const PERF_TYPE_SOFTWARE: u32 = 1;
pub const PERF_COUNT_HW_CPU_CYCLES: u64 = 0;
pub const PERF_COUNT_HW_INSTRUCTIONS: u64 = 1;
pub const PERF_COUNT_HW_BRANCH_INSTRUCTIONS: u64 = 4;
pub const PERF_COUNT_HW_BRANCH_MISSES: u64 = 5;
pub const PERF_COUNT_HW_CACHE_REFERENCES: u64 = 2;
pub const PERF_COUNT_HW_CACHE_MISSES: u64 = 3;

#[cfg(target_arch = "x86_64")]
mod nr {
    pub const READ: usize = 0;
    pub const WRITE: usize = 1;
    pub const CLOSE: usize = 3;
    pub const LSEEK: usize = 8;
    pub const MMAP: usize = 9;
    pub const MPROTECT: usize = 10;
    pub const MUNMAP: usize = 11;
    pub const PREAD64: usize = 17;
    pub const PWRITE64: usize = 18;
    pub const SCHED_YIELD: usize = 24;
    pub const MADVISE: usize = 28;
    pub const NANOSLEEP: usize = 35;
    pub const GETPID: usize = 39;
    pub const CLONE: usize = 56;
    pub const EXIT: usize = 60;
    pub const WAIT4: usize = 61;
    pub const UNAME: usize = 63;
    pub const FSYNC: usize = 74;
    pub const FDATASYNC: usize = 75;
    pub const FTRUNCATE: usize = 77;
    pub const GETRUSAGE: usize = 98;
    pub const SYSINFO: usize = 99;
    pub const GETEUID: usize = 107;
    pub const GETTID: usize = 186;
    pub const FUTEX: usize = 202;
    pub const SCHED_SETAFFINITY: usize = 203;
    pub const SCHED_GETAFFINITY: usize = 204;
    pub const GETDENTS64: usize = 217;
    pub const CLOCK_GETTIME: usize = 228;
    pub const CLOCK_GETRES: usize = 229;
    pub const EXIT_GROUP: usize = 231;
    pub const MBIND: usize = 237;
    pub const GET_MEMPOLICY: usize = 239;
    pub const OPENAT: usize = 257;
    pub const MKDIRAT: usize = 258;
    pub const UNLINKAT: usize = 263;
    pub const FALLOCATE: usize = 285;
    pub const PERF_EVENT_OPEN: usize = 298;
    pub const GETCPU: usize = 309;
    pub const GETRANDOM: usize = 318;
    pub const STATX: usize = 332;
    pub const GETCWD: usize = 79;
    pub const IO_URING_SETUP: usize = 425;
    pub const IO_URING_ENTER: usize = 426;
    pub const IO_URING_REGISTER: usize = 427;
}

#[cfg(target_arch = "aarch64")]
mod nr {
    pub const READ: usize = 63;
    pub const WRITE: usize = 64;
    pub const CLOSE: usize = 57;
    pub const LSEEK: usize = 62;
    pub const MMAP: usize = 222;
    pub const MPROTECT: usize = 226;
    pub const MUNMAP: usize = 215;
    pub const PREAD64: usize = 67;
    pub const PWRITE64: usize = 68;
    pub const SCHED_YIELD: usize = 124;
    pub const MADVISE: usize = 233;
    pub const NANOSLEEP: usize = 101;
    pub const GETPID: usize = 172;
    pub const CLONE: usize = 220;
    pub const EXIT: usize = 93;
    pub const WAIT4: usize = 260;
    pub const UNAME: usize = 160;
    pub const FSYNC: usize = 82;
    pub const FDATASYNC: usize = 83;
    pub const FTRUNCATE: usize = 46;
    pub const GETRUSAGE: usize = 165;
    pub const SYSINFO: usize = 179;
    pub const GETEUID: usize = 175;
    pub const GETTID: usize = 178;
    pub const FUTEX: usize = 98;
    pub const SCHED_SETAFFINITY: usize = 122;
    pub const SCHED_GETAFFINITY: usize = 123;
    pub const GETDENTS64: usize = 61;
    pub const CLOCK_GETTIME: usize = 113;
    pub const CLOCK_GETRES: usize = 114;
    pub const EXIT_GROUP: usize = 94;
    pub const MBIND: usize = 235;
    pub const GET_MEMPOLICY: usize = 237;
    pub const OPENAT: usize = 56;
    pub const MKDIRAT: usize = 34;
    pub const UNLINKAT: usize = 35;
    pub const FALLOCATE: usize = 47;
    pub const PERF_EVENT_OPEN: usize = 241;
    pub const GETCPU: usize = 168;
    pub const GETRANDOM: usize = 278;
    pub const STATX: usize = 291;
    pub const GETCWD: usize = 17;
    pub const IO_URING_SETUP: usize = 425;
    pub const IO_URING_ENTER: usize = 426;
    pub const IO_URING_REGISTER: usize = 427;
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub struct Errno(pub i32);

impl Errno {
    pub fn raw(self) -> i32 {
        self.0
    }
}

pub const EPERM: i32 = 1;
pub const ENOENT: i32 = 2;
pub const EINTR: i32 = 4;
pub const EIO: i32 = 5;
pub const EBADF: i32 = 9;
pub const EAGAIN: i32 = 11;
pub const ENOMEM: i32 = 12;
pub const EACCES: i32 = 13;
pub const EFAULT: i32 = 14;
pub const EBUSY: i32 = 16;
pub const EEXIST: i32 = 17;
pub const ENODEV: i32 = 19;
pub const ENOTDIR: i32 = 20;
pub const EISDIR: i32 = 21;
pub const EINVAL: i32 = 22;
pub const ENFILE: i32 = 23;
pub const EMFILE: i32 = 24;
pub const ENOSPC: i32 = 28;
pub const EROFS: i32 = 30;
pub const EPIPE: i32 = 32;
pub const ERANGE: i32 = 34;
pub const ENOSYS: i32 = 38;
pub const ENOTSUP: i32 = 95;
pub const EOVERFLOW: i32 = 75;

pub fn err_name(e: Errno) -> &'static str {
    match e.0 {
        EPERM => "EPERM",
        ENOENT => "ENOENT",
        EINTR => "EINTR",
        EIO => "EIO",
        EBADF => "EBADF",
        EAGAIN => "EAGAIN",
        ENOMEM => "ENOMEM",
        EACCES => "EACCES",
        EFAULT => "EFAULT",
        EBUSY => "EBUSY",
        EEXIST => "EEXIST",
        ENODEV => "ENODEV",
        ENOTDIR => "ENOTDIR",
        EISDIR => "EISDIR",
        EINVAL => "EINVAL",
        ENFILE => "ENFILE",
        EMFILE => "EMFILE",
        ENOSPC => "ENOSPC",
        EROFS => "EROFS",
        EPIPE => "EPIPE",
        ENOSYS => "ENOSYS",
        ENOTSUP => "ENOTSUP",
        EOVERFLOW => "EOVERFLOW",
        _ => "ERRNO",
    }
}

#[inline]
fn ret(r: isize) -> Result<isize, Errno> {
    if r < 0 {
        Err(Errno(-(r as i32)))
    } else {
        Ok(r)
    }
}

pub fn read(fd: i32, buf: &mut [u8]) -> Result<usize, Errno> {
    unsafe { ret(arch::syscall3(nr::READ, fd as usize, buf.as_mut_ptr() as usize, buf.len())) }.map(|v| v as usize)
}

pub fn write(fd: i32, buf: &[u8]) -> Result<usize, Errno> {
    unsafe { ret(arch::syscall3(nr::WRITE, fd as usize, buf.as_ptr() as usize, buf.len())) }.map(|v| v as usize)
}

pub fn write_all(fd: i32, mut buf: &[u8]) -> Result<(), Errno> {
    while !buf.is_empty() {
        match write(fd, buf) {
            Ok(0) => return Err(Errno(EIO)),
            Ok(n) => buf = &buf[n..],
            Err(e) => return Err(e),
        }
    }
    Ok(())
}

pub fn openat(dirfd: i32, path: &[u8], flags: u32, mode: u32) -> Result<i32, Errno> {
    if path.len() >= 4000 {
        return Err(Errno(EINVAL));
    }
    let mut buf = [0u8; 4096];
    buf[..path.len()].copy_from_slice(path);
    buf[path.len()] = 0;
    unsafe {
        ret(arch::syscall4(
            nr::OPENAT,
            dirfd as usize,
            buf.as_ptr() as usize,
            flags as usize,
            mode as usize,
        ))
    }
    .map(|v| v as i32)
}

pub fn open(path: &[u8], flags: u32, mode: u32) -> Result<i32, Errno> {
    openat(AT_FDCWD, path, flags, mode)
}

pub fn mkdirat(dirfd: i32, path: &[u8], mode: u32) -> Result<(), Errno> {
    if path.len() >= 4000 {
        return Err(Errno(EINVAL));
    }
    let mut buf = [0u8; 4096];
    buf[..path.len()].copy_from_slice(path);
    buf[path.len()] = 0;
    unsafe {
        ret(arch::syscall3(
            nr::MKDIRAT,
            dirfd as usize,
            buf.as_ptr() as usize,
            mode as usize,
        ))
    }
    .map(|_| ())
}

pub fn close(fd: i32) -> Result<(), Errno> {
    unsafe { ret(arch::syscall1(nr::CLOSE, fd as usize)) }.map(|_| ())
}

pub fn lseek(fd: i32, offset: i64, whence: i32) -> Result<u64, Errno> {
    unsafe { ret(arch::syscall3(nr::LSEEK, fd as usize, offset as usize, whence as usize)) }
        .map(|v| v as u64)
}

pub fn pread(fd: i32, buf: &mut [u8], offset: u64) -> Result<usize, Errno> {
    unsafe {
        ret(arch::syscall4(
            nr::PREAD64,
            fd as usize,
            buf.as_mut_ptr() as usize,
            buf.len(),
            offset as usize,
        ))
    }
    .map(|v| v as usize)
}

pub fn pwrite(fd: i32, buf: &[u8], offset: u64) -> Result<usize, Errno> {
    unsafe {
        ret(arch::syscall4(
            nr::PWRITE64,
            fd as usize,
            buf.as_ptr() as usize,
            buf.len(),
            offset as usize,
        ))
    }
    .map(|v| v as usize)
}

pub fn mmap(
    addr: *mut c_void,
    len: usize,
    prot: u32,
    flags: u32,
    fd: i32,
    offset: u64,
) -> Result<*mut u8, Errno> {
    unsafe {
        ret(arch::syscall6(
            nr::MMAP,
            addr as usize,
            len,
            prot as usize,
            flags as usize,
            fd as usize,
            offset as usize,
        ))
    }
    .map(|v| v as *mut u8)
}

pub fn mmap_anon(len: usize, shared: bool) -> Option<*mut u8> {
    let flags = MAP_ANONYMOUS | if shared { MAP_SHARED } else { MAP_PRIVATE };
    mmap(core::ptr::null_mut(), len, PROT_READ | PROT_WRITE, flags, -1, 0).ok()
}

pub fn munmap(addr: *mut u8, len: usize) -> Result<(), Errno> {
    unsafe { ret(arch::syscall2(nr::MUNMAP, addr as usize, len)) }.map(|_| ())
}

pub fn mprotect(addr: *mut u8, len: usize, prot: u32) -> Result<(), Errno> {
    unsafe { ret(arch::syscall3(nr::MPROTECT, addr as usize, len, prot as usize)) }.map(|_| ())
}

pub fn madvise(addr: *mut u8, len: usize, advice: i32) -> Result<(), Errno> {
    unsafe { ret(arch::syscall3(nr::MADVISE, addr as usize, len, advice as usize)) }.map(|_| ())
}

pub fn madvise_hugepage(addr: *mut u8, len: usize) {
    let _ = madvise(addr, len, MADV_HUGEPAGE);
}

pub fn clock_gettime(clk: i32, ts: &mut Timespec) -> Result<(), Errno> {
    unsafe { ret(arch::syscall2(nr::CLOCK_GETTIME, clk as usize, ts as *mut Timespec as usize)) }
        .map(|_| ())
}

pub fn nanosleep(req: &Timespec) -> Result<(), Errno> {
    unsafe { ret(arch::syscall2(nr::NANOSLEEP, req as *const Timespec as usize, 0)) }.map(|_| ())
}

pub fn sched_setaffinity(pid: i32, mask: &[u8]) -> Result<(), Errno> {
    unsafe {
        ret(arch::syscall3(
            nr::SCHED_SETAFFINITY,
            pid as usize,
            mask.len(),
            mask.as_ptr() as usize,
        ))
    }
    .map(|_| ())
}

pub fn sched_getaffinity(pid: i32, mask: &mut [u8]) -> Result<usize, Errno> {
    unsafe {
        ret(arch::syscall3(
            nr::SCHED_GETAFFINITY,
            pid as usize,
            mask.len(),
            mask.as_mut_ptr() as usize,
        ))
    }
    .map(|v| v as usize)
}

pub fn sched_yield() {
    let _ = unsafe { ret(arch::syscall0(nr::SCHED_YIELD)) };
}

pub fn clone_process(
    stack_top: *mut u8,
    arg: usize,
    entry: extern "C" fn(usize) -> i32,
) -> Result<i32, Errno> {
    #[cfg(target_arch = "x86_64")]
    unsafe {
        let r: isize;
        core::arch::asm!(
            "syscall",
            "test rax, rax",
            "jnz 2f",
            "mov rdi, r12",
            "call r13",
            "mov edi, eax",
            "mov eax, 60",
            "syscall",
            "2:",
            inlateout("rax") nr::CLONE as isize => r,
            inlateout("rdi") SIGCHLD => _,
            in("rsi") stack_top as usize,
            in("rdx") 0usize,
            in("r10") 0usize,
            in("r8") 0usize,
            inlateout("r12") arg => _,
            inlateout("r13") entry as usize => _,
            lateout("rcx") _,
            lateout("r11") _,
        );
        ret(r).map(|v| v as i32)
    }
    #[cfg(target_arch = "aarch64")]
    unsafe {
        let r: isize;
        core::arch::asm!(
            "svc 0",
            "cbnz x0, 2f",
            "mov x0, x10",
            "blr x9",
            "mov x8, #93",
            "svc 0",
            "2:",
            inlateout("x0") SIGCHLD as isize => r,
            in("x1") stack_top as usize,
            in("x2") 0usize,
            in("x3") 0usize,
            in("x4") 0usize,
            in("x8") nr::CLONE,
            inlateout("x9") entry as usize => _,
            inlateout("x10") arg => _,
        );
        ret(r).map(|v| v as i32)
    }
}

pub fn wait4(pid: i32, status: &mut i32, options: i32) -> Result<i32, Errno> {
    unsafe {
        ret(arch::syscall4(
            nr::WAIT4,
            pid as usize,
            status as *mut i32 as usize,
            options as usize,
            0,
        ))
    }
    .map(|v| v as i32)
}

/// Waits for a child and returns its exit status word.
pub fn wait_child(pid: i32) -> Result<i32, Errno> {
    let mut status = 0i32;
    loop {
        match wait4(pid, &mut status, 0) {
            Ok(_) => return Ok(status),
            Err(e) if e.0 == EINTR => continue,
            Err(e) => return Err(e),
        }
    }
}

pub fn exit(code: i32) -> ! {
    unsafe {
        arch::syscall1(nr::EXIT_GROUP, code as usize);
        core::hint::unreachable_unchecked();
    }
}

pub fn getpid() -> i32 {
    unsafe { ret(arch::syscall0(nr::GETPID)).unwrap_or(1) as i32 }
}

pub fn gettid() -> i32 {
    unsafe { ret(arch::syscall0(nr::GETTID)).unwrap_or(1) as i32 }
}

pub fn geteuid() -> u32 {
    unsafe { ret(arch::syscall0(nr::GETEUID)).unwrap_or(0) as u32 }
}

pub fn getcpu(cpu: &mut u32, node: &mut u32) -> Result<(), Errno> {
    unsafe {
        ret(arch::syscall3(
            nr::GETCPU,
            cpu as *mut u32 as usize,
            node as *mut u32 as usize,
            0,
        ))
    }
    .map(|_| ())
}

pub fn getrandom(buf: &mut [u8]) -> Result<usize, Errno> {
    unsafe { ret(arch::syscall3(nr::GETRANDOM, buf.as_mut_ptr() as usize, buf.len(), 0)) }
        .map(|v| v as usize)
}

pub fn futex_wait(addr: &core::sync::atomic::AtomicU32, expected: u32) -> Result<(), Errno> {
    unsafe {
        ret(arch::syscall6(
            nr::FUTEX,
            addr as *const _ as usize,
            FUTEX_WAIT as usize,
            expected as usize,
            0,
            0,
            0,
        ))
    }
    .map(|_| ())
}

pub fn futex_wake(addr: &core::sync::atomic::AtomicU32, n: i32) -> Result<i32, Errno> {
    unsafe {
        ret(arch::syscall6(
            nr::FUTEX,
            addr as *const _ as usize,
            FUTEX_WAKE as usize,
            n as usize,
            0,
            0,
            0,
        ))
    }
    .map(|v| v as i32)
}

#[repr(C)]
pub struct LinuxDirent64 {
    pub d_ino: u64,
    pub d_off: i64,
    pub d_reclen: u16,
    pub d_type: u8,
    pub d_name: [u8; 0],
}

pub fn getdents64(fd: i32, buf: &mut [u8]) -> Result<usize, Errno> {
    unsafe { ret(arch::syscall3(nr::GETDENTS64, fd as usize, buf.as_mut_ptr() as usize, buf.len())) }
        .map(|v| v as usize)
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct StatxTimestamp {
    pub tv_sec: i64,
    pub tv_nsec: u32,
    pub reserved: i32,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct Statx {
    pub stx_mask: u32,
    pub stx_blksize: u32,
    pub stx_attributes: u64,
    pub stx_nlink: u32,
    pub stx_uid: u32,
    pub stx_gid: u32,
    pub stx_mode: u16,
    pub spare0: [u16; 1],
    pub stx_ino: u64,
    pub stx_size: u64,
    pub stx_blocks: u64,
    pub stx_attributes_mask: u64,
    pub stx_atime: StatxTimestamp,
    pub stx_btime: StatxTimestamp,
    pub stx_ctime: StatxTimestamp,
    pub stx_mtime: StatxTimestamp,
    pub stx_rdev_major: u32,
    pub stx_rdev_minor: u32,
    pub stx_dev_major: u32,
    pub stx_dev_minor: u32,
    pub stx_mnt_id: u64,
    pub stx_dio_mem_align: u32,
    pub stx_dio_offset_align: u32,
    pub spare3: [u64; 12],
}

impl Default for Statx {
    fn default() -> Self {
        unsafe { core::mem::zeroed() }
    }
}

pub fn statx(dirfd: i32, path: &[u8], flags: i32, mask: u32, st: &mut Statx) -> Result<(), Errno> {
    if path.len() >= 4000 {
        return Err(Errno(EINVAL));
    }
    let mut buf = [0u8; 4096];
    buf[..path.len()].copy_from_slice(path);
    buf[path.len()] = 0;
    unsafe {
        ret(arch::syscall5(
            nr::STATX,
            dirfd as usize,
            buf.as_ptr() as usize,
            flags as usize,
            mask as usize,
            st as *mut Statx as usize,
        ))
    }
    .map(|_| ())
}

pub fn ftruncate(fd: i32, len: u64) -> Result<(), Errno> {
    unsafe { ret(arch::syscall2(nr::FTRUNCATE, fd as usize, len as usize)) }.map(|_| ())
}

pub fn fallocate(fd: i32, mode: i32, offset: u64, len: u64) -> Result<(), Errno> {
    unsafe {
        ret(arch::syscall4(
            nr::FALLOCATE,
            fd as usize,
            mode as usize,
            offset as usize,
            len as usize,
        ))
    }
    .map(|_| ())
}

pub fn fsync(fd: i32) -> Result<(), Errno> {
    unsafe { ret(arch::syscall1(nr::FSYNC, fd as usize)) }.map(|_| ())
}

pub fn fdatasync(fd: i32) -> Result<(), Errno> {
    unsafe { ret(arch::syscall1(nr::FDATASYNC, fd as usize)) }.map(|_| ())
}

pub fn unlinkat(dirfd: i32, path: &[u8], flags: i32) -> Result<(), Errno> {
    if path.len() >= 4000 {
        return Err(Errno(EINVAL));
    }
    let mut buf = [0u8; 4096];
    buf[..path.len()].copy_from_slice(path);
    buf[path.len()] = 0;
    unsafe {
        ret(arch::syscall3(
            nr::UNLINKAT,
            dirfd as usize,
            buf.as_ptr() as usize,
            flags as usize,
        ))
    }
    .map(|_| ())
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct Sysinfo {
    pub uptime: i64,
    pub loads: [u64; 3],
    pub totalram: u64,
    pub freeram: u64,
    pub sharedram: u64,
    pub bufferram: u64,
    pub totalswap: u64,
    pub freeswap: u64,
    pub procs: u16,
    pub pad: u16,
    pub totalhigh: u64,
    pub freehigh: u64,
    pub mem_unit: u32,
    pub _f: [u8; 0],
}

pub fn sysinfo(info: &mut Sysinfo) -> Result<(), Errno> {
    unsafe { ret(arch::syscall1(nr::SYSINFO, info as *mut Sysinfo as usize)) }.map(|_| ())
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct Timeval {
    pub tv_sec: i64,
    pub tv_usec: i64,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct Rusage {
    pub ru_utime: Timeval,
    pub ru_stime: Timeval,
    pub ru_maxrss: i64,
    pub ru_ixrss: i64,
    pub ru_idrss: i64,
    pub ru_isrss: i64,
    pub ru_minflt: i64,
    pub ru_majflt: i64,
    pub ru_nswap: i64,
    pub ru_inblock: i64,
    pub ru_oublock: i64,
    pub ru_msgsnd: i64,
    pub ru_msgrcv: i64,
    pub ru_nsignals: i64,
    pub ru_nvcsw: i64,
    pub ru_nivcsw: i64,
}

pub fn getrusage(who: i32, ru: &mut Rusage) -> Result<(), Errno> {
    unsafe { ret(arch::syscall2(nr::GETRUSAGE, who as usize, ru as *mut Rusage as usize)) }
        .map(|_| ())
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct Utsname {
    pub sysname: [u8; 65],
    pub nodename: [u8; 65],
    pub release: [u8; 65],
    pub version: [u8; 65],
    pub machine: [u8; 65],
    pub domainname: [u8; 65],
}

impl Default for Utsname {
    fn default() -> Self {
        unsafe { core::mem::zeroed() }
    }
}

pub fn getcwd(buf: &mut [u8]) -> Result<usize, Errno> {
    unsafe { ret(arch::syscall2(nr::GETCWD, buf.as_mut_ptr() as usize, buf.len())) }.map(|v| v as usize)
}

pub fn uname(u: &mut Utsname) -> Result<(), Errno> {
    unsafe { ret(arch::syscall1(nr::UNAME, u as *mut Utsname as usize)) }.map(|_| ())
}

pub fn mbind(
    addr: *mut u8,
    len: usize,
    mode: i32,
    nodemask: &[u64],
    flags: u32,
) -> Result<(), Errno> {
    unsafe {
        ret(arch::syscall6(
            nr::MBIND,
            addr as usize,
            len,
            mode as usize,
            nodemask.as_ptr() as usize,
            (nodemask.len() * 64) as usize,
            flags as usize,
        ))
    }
    .map(|_| ())
}

pub fn get_mempolicy(
    mode: &mut i32,
    nodemask: &mut [u64],
    addr: *mut u8,
    flags: u32,
) -> Result<(), Errno> {
    unsafe {
        ret(arch::syscall5(
            nr::GET_MEMPOLICY,
            mode as *mut i32 as usize,
            nodemask.as_mut_ptr() as usize,
            (nodemask.len() * 64) as usize,
            addr as usize,
            flags as usize,
        ))
    }
    .map(|_| ())
}

#[allow(dead_code)]
pub fn perf_event_open(attr: *mut u8, pid: i32, cpu: i32, group_fd: i32, flags: u64) -> Result<i32, Errno> {
    unsafe {
        ret(arch::syscall5(
            nr::PERF_EVENT_OPEN,
            attr as usize,
            pid as usize,
            cpu as usize,
            group_fd as usize,
            flags as usize,
        ))
    }
    .map(|v| v as i32)
}

#[allow(dead_code)]
pub fn io_uring_setup(entries: u32, params: *mut u8) -> Result<i32, Errno> {
    unsafe { ret(arch::syscall2(nr::IO_URING_SETUP, entries as usize, params as usize)) }
        .map(|v| v as i32)
}

#[allow(dead_code)]
pub fn io_uring_enter(
    fd: i32,
    to_submit: u32,
    min_complete: u32,
    flags: u32,
    sig: *mut u8,
    sigsz: usize,
) -> Result<i32, Errno> {
    unsafe {
        ret(arch::syscall6(
            nr::IO_URING_ENTER,
            fd as usize,
            to_submit as usize,
            min_complete as usize,
            flags as usize,
            sig as usize,
            sigsz,
        ))
    }
    .map(|v| v as i32)
}

#[allow(dead_code)]
pub fn io_uring_register(fd: i32, opcode: u32, arg: *mut u8, nr_args: u32) -> Result<i32, Errno> {
    unsafe {
        ret(arch::syscall4(
            nr::IO_URING_REGISTER,
            fd as usize,
            opcode as usize,
            arg as usize,
            nr_args as usize,
        ))
    }
    .map(|v| v as i32)
}

/// Reads an entire file into `buf`. Returns the number of bytes read.
pub fn read_file(path: &[u8], buf: &mut [u8]) -> Result<usize, Errno> {
    let fd = open(path, O_RDONLY | O_CLOEXEC, 0)?;
    let mut total = 0;
    let r = loop {
        if total >= buf.len() {
            break Ok(total);
        }
        match read(fd, &mut buf[total..]) {
            Ok(0) => break Ok(total),
            Ok(n) => total += n,
            Err(e) => break Err(e),
        }
    };
    let _ = close(fd);
    r
}
