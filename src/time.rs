use crate::arch;
use crate::sys;

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct Timespec {
    pub sec: i64,
    pub nsec: i64,
}

pub const CLOCK_REALTIME: i32 = 0;
pub const CLOCK_MONOTONIC: i32 = 1;
pub const CLOCK_MONOTONIC_RAW: i32 = 4;

pub fn now_ns(clk: i32) -> u64 {
    let mut ts = Timespec::default();
    match sys::clock_gettime(clk, &mut ts) {
        Ok(()) => (ts.sec as u64) * 1_000_000_000 + ts.nsec as u64,
        Err(_) => 0,
    }
}

pub fn mono_ns() -> u64 {
    now_ns(CLOCK_MONOTONIC_RAW)
}

pub fn realtime_ns() -> u64 {
    now_ns(CLOCK_REALTIME)
}

#[inline(always)]
pub fn cycles() -> u64 {
    unsafe { arch::read_cycles() }
}

pub fn sleep_ns(ns: u64) {
    let ts = Timespec {
        sec: (ns / 1_000_000_000) as i64,
        nsec: (ns % 1_000_000_000) as i64,
    };
    let _ = sys::nanosleep(&ts);
}

static mut CYCLES_HZ: u64 = 0;
static mut CYCLES_HZ_SOURCE: &str = "unavailable";

pub fn cycles_hz() -> u64 {
    unsafe { CYCLES_HZ }
}

pub fn cycles_hz_source() -> &'static str {
    unsafe { CYCLES_HZ_SOURCE }
}

fn busy_wait_ns(ns: u64) {
    let start = mono_ns();
    let mut x: u64 = 0x12345678;
    while mono_ns().wrapping_sub(start) < ns {
        for _ in 0..64 {
            x = x.wrapping_mul(6364136223846793005).wrapping_add(1);
        }
        core::hint::black_box(x);
    }
}

pub fn calibrate_cycles() {
    let hint = unsafe { arch::cycles_hz_hint() };
    let t0 = mono_ns();
    let c0 = cycles();
    let mut spin = 0u64;
    while mono_ns().wrapping_sub(t0) < 20_000_000 {
        spin = spin.wrapping_add(cycles() ^ 0x55);
    }
    let t1 = mono_ns();
    let c1 = cycles();
    core::hint::black_box(spin);
    let dt = t1.wrapping_sub(t0);
    let dc = c1.wrapping_sub(c0);
    let measured = if dt > 0 {
        ((dc as u128 * 1_000_000_000u128) / dt as u128) as u64
    } else {
        0
    };
    let (hz, src) = match hint {
        Some(h) if h > 1_000_000 && ratio_close(h, measured, 30) => (h, "arch-hint"),
        _ if measured > 1_000_000 => (measured, "measured"),
        _ => (0, "unavailable"),
    };
    unsafe {
        CYCLES_HZ = hz;
        CYCLES_HZ_SOURCE = src;
    }
    let _ = busy_wait_ns; // keep helper referenced
}

fn ratio_close(a: u64, b: u64, pct: u64) -> bool {
    if a == 0 || b == 0 {
        return false;
    }
    let diff = if a > b { a - b } else { b - a };
    diff * 100 <= a.max(b) * pct
}

/// Wall-clock deadline checked with the native cycle counter when available
/// (rdtsc / cntvct_el0, a few dozen cycles) and with clock_gettime otherwise.
/// The final measured duration still comes from clock_gettime, so this only
/// controls when a measurement loop stops.
pub struct Deadline {
    start_c: u64,
    start_ns: u64,
    limit_c: u64,
    target_ns: u64,
    use_cycles: bool,
}

impl Deadline {
    pub fn new(target_ns: u64) -> Deadline {
        let hz = cycles_hz();
        let use_cycles = hz > 0;
        let limit_c = if use_cycles {
            ((target_ns as u128 * hz as u128) / 1_000_000_000u128) as u64
        } else {
            0
        };
        Deadline {
            start_c: cycles(),
            start_ns: mono_ns(),
            limit_c,
            target_ns,
            use_cycles,
        }
    }

    #[inline]
    pub fn expired(&self) -> bool {
        if self.use_cycles {
            cycles().wrapping_sub(self.start_c) >= self.limit_c
        } else {
            mono_ns().wrapping_sub(self.start_ns) >= self.target_ns
        }
    }
}

pub fn ns_per_cycle_x1000() -> Option<u64> {
    let hz = cycles_hz();
    if hz == 0 {
        None
    } else {
        Some((1_000_000_000_000u128 / hz as u128) as u64)
    }
}
