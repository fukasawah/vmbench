use core::arch::global_asm;
use core::ffi::c_void;
use crate::sys;

#[derive(Clone, Copy, Default)]
pub struct Auxv {
    pub pagesz: usize,
    pub clktck: usize,
    pub hwcap: usize,
    pub hwcap2: usize,
    pub uid: usize,
    pub euid: usize,
    pub gid: usize,
    pub egid: usize,
    pub secure: bool,
    pub execfn: usize,
    pub random: usize,
}

pub static mut ARGC: usize = 0;
pub static mut ARGV: *const *const u8 = core::ptr::null();
pub static mut AUXV: Auxv = Auxv {
    pagesz: 4096,
    clktck: 100,
    hwcap: 0,
    hwcap2: 0,
    uid: 0,
    euid: 0,
    gid: 0,
    egid: 0,
    secure: false,
    execfn: 0,
    random: 0,
};

pub fn argc() -> usize {
    unsafe { ARGC }
}

pub fn argv(i: usize) -> Option<&'static [u8]> {
    unsafe {
        if i >= ARGC {
            return None;
        }
        let p = core::ptr::read_volatile(ARGV.add(i));
        if p.is_null() {
            return None;
        }
        let mut len = 0;
        while core::ptr::read_volatile(p.add(len)) != 0 {
            len += 1;
            if len > 1 << 20 {
                return None;
            }
        }
        Some(core::slice::from_raw_parts(p, len))
    }
}

pub fn hwcap() -> u64 {
    unsafe { AUXV.hwcap as u64 }
}

pub fn hwcap2() -> u64 {
    unsafe { AUXV.hwcap2 as u64 }
}

pub fn page_size() -> usize {
    unsafe {
        if AUXV.pagesz > 0 {
            AUXV.pagesz
        } else {
            4096
        }
    }
}

#[cfg(target_arch = "x86_64")]
global_asm!(
    ".globl _start",
    ".type _start,@function",
    "_start:",
    "xor ebp, ebp",
    "mov rdi, rsp",
    "and rsp, -16",
    "call {start}",
    "ud2",
    ".size _start, .-_start",
    ".section .note.GNU-stack,\"\",@progbits",
    start = sym vmbench_start,
);

#[cfg(target_arch = "aarch64")]
global_asm!(
    ".globl _start",
    ".type _start,%function",
    "_start:",
    "mov x0, sp",
    "bl {start}",
    "brk #0",
    ".size _start, .-_start",
    ".section .note.GNU-stack,\"\",%progbits",
    start = sym vmbench_start,
);

unsafe extern "C" fn vmbench_start(sp: *const usize) -> ! {
    let argc = *sp;
    let argv = sp.add(1) as *const *const u8;
    ARGC = argc;
    ARGV = argv;
    // Stack layout: argc, argv[argc], NULL, envp[], NULL, auxv pairs, AT_NULL.
    // Skip argv and envp before reading the auxiliary vector.
    let mut p = sp.add(1 + argc + 1);
    while *p != 0 {
        p = p.add(1);
    }
    p = p.add(1);
    let mut a = Auxv {
        pagesz: 4096,
        clktck: 100,
        ..Default::default()
    };
    loop {
        let t = *p;
        let v = *p.add(1);
        match t {
            0 => break,
            6 => a.pagesz = v,
            11 => a.uid = v,
            12 => a.euid = v,
            13 => a.gid = v,
            14 => a.egid = v,
            16 => a.hwcap = v,
            17 => a.clktck = v,
            23 => a.secure = v != 0,
            25 => a.random = v,
            26 => a.hwcap2 = v,
            31 => a.execfn = v,
            _ => {}
        }
        p = p.add(2);
    }
    AUXV = a;
    crate::app::run()
}

pub fn fatal(msg: &str) -> ! {
    let _ = sys::write_all(2, b"vmbench: fatal: ");
    let _ = sys::write_all(2, msg.as_bytes());
    let _ = sys::write_all(2, b"\n");
    sys::exit(70)
}

#[panic_handler]
fn panic_handler(info: &core::panic::PanicInfo) -> ! {
    let _ = sys::write_all(2, b"vmbench: panic");
    if let Some(loc) = info.location() {
        let _ = sys::write_all(2, b" at ");
        let _ = sys::write_all(2, loc.file().as_bytes());
        let _ = sys::write_all(2, b":");
        let mut tmp = [0u8; 12];
        let n = crate::fmt::u64_dec(loc.line() as u64, &mut tmp);
        let _ = sys::write_all(2, &tmp[..n]);
    }
    let _ = sys::write_all(2, b"\n");
    sys::exit(101)
}

//
// Freestanding memory primitives. Volatile accesses keep LLVM from folding
// these loops back into calls to themselves.
//

#[no_mangle]
pub unsafe extern "C" fn memcpy(dest: *mut c_void, src: *const c_void, n: usize) -> *mut c_void {
    let d = dest as *mut u8;
    let s = src as *const u8;
    let mut i = 0;
    while i + 8 <= n {
        let v = core::ptr::read_volatile(s.add(i) as *const u64);
        core::ptr::write_volatile(d.add(i) as *mut u64, v);
        i += 8;
    }
    while i < n {
        core::ptr::write_volatile(d.add(i), core::ptr::read_volatile(s.add(i)));
        i += 1;
    }
    dest
}

#[no_mangle]
pub unsafe extern "C" fn memmove(dest: *mut c_void, src: *const c_void, n: usize) -> *mut c_void {
    let d = dest as *mut u8;
    let s = src as *const u8;
    if (d as usize) < (s as usize) {
        return memcpy(dest, src, n);
    }
    let mut i = n;
    while i >= 8 {
        i -= 8;
        let v = core::ptr::read_volatile(s.add(i) as *const u64);
        core::ptr::write_volatile(d.add(i) as *mut u64, v);
    }
    while i > 0 {
        i -= 1;
        core::ptr::write_volatile(d.add(i), core::ptr::read_volatile(s.add(i)));
    }
    dest
}

#[no_mangle]
pub unsafe extern "C" fn memset(dest: *mut c_void, c: i32, n: usize) -> *mut c_void {
    let d = dest as *mut u8;
    let byte = c as u8;
    let word = u64::from_ne_bytes([byte; 8]);
    let mut i = 0;
    while i + 8 <= n {
        core::ptr::write_volatile(d.add(i) as *mut u64, word);
        i += 8;
    }
    while i < n {
        core::ptr::write_volatile(d.add(i), byte);
        i += 1;
    }
    dest
}

#[no_mangle]
pub unsafe extern "C" fn memcmp(a: *const c_void, b: *const c_void, n: usize) -> i32 {
    let x = a as *const u8;
    let y = b as *const u8;
    let mut i = 0;
    while i < n {
        let xv = core::ptr::read_volatile(x.add(i));
        let yv = core::ptr::read_volatile(y.add(i));
        if xv != yv {
            return xv as i32 - yv as i32;
        }
        i += 1;
    }
    0
}

#[no_mangle]
pub unsafe extern "C" fn bcmp(a: *const c_void, b: *const c_void, n: usize) -> i32 {
    memcmp(a, b, n)
}

/// Required by compiler_builtins' DWARF reference even with panic=abort.
#[no_mangle]
pub extern "C" fn rust_eh_personality() {}

/// compiler_builtins on aarch64 calls getauxval to detect LSE atomics.
/// We serve it from the auxv captured at startup instead of linking libc.
#[cfg(target_arch = "aarch64")]
#[no_mangle]
pub extern "C" fn getauxval(kind: usize) -> usize {
    match kind {
        6 => page_size(),
        16 => hwcap() as usize,
        17 => unsafe { AUXV.clktck },
        26 => hwcap2() as usize,
        _ => 0,
    }
}

#[no_mangle]
pub unsafe extern "C" fn strlen(s: *const i8) -> usize {
    let s = s as *const u8;
    let mut i = 0;
    while core::ptr::read_volatile(s.add(i)) != 0 {
        i += 1;
    }
    i
}
