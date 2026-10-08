//! Minimal io_uring engine used by the `storage.uring.*` benchmarks.
//!
//! Only what the benchmark needs is implemented: single ring, fixed-size
//! entries, `IORING_OP_READ` / `IORING_OP_WRITE` with a file offset. Ring
//! memory is mmap'ed from the fd returned by `io_uring_setup` and the
//! completion queue is drained after every `io_uring_enter`. The engine is
//! unavailable (and callers must report `unsupported`) when setup fails, for
//! example under seccomp or on kernels without io_uring.

use crate::bench::common::{cycles_to_ns_u32, io_stats, IoStats, Sampler, MAX_SAMPLES, MAX_QD};
use crate::rand::Rng;
use crate::sys;
use crate::time;
use core::sync::atomic::{AtomicI32, Ordering};

pub const OP_READ: u32 = 0;
pub const OP_WRITE: u32 = 1;
pub const OP_MIXED: u32 = 2;

const IORING_ENTER_GETEVENTS: u32 = 1;
const IORING_OP_READ: u8 = 22;
const IORING_OP_WRITE: u8 = 23;
const IORING_OFF_SQ_RING: u64 = 0;
const IORING_OFF_CQ_RING: u64 = 0x0800_0000;
const IORING_OFF_SQES: u64 = 0x1000_0000;
const SQE_SIZE: u64 = 64;
const CQE_SIZE: u64 = 16;

/// File offset used by the functional check (must be block aligned).
pub const CHECK_OFF: u64 = 12 * 4096;

#[repr(C)]
#[derive(Clone, Copy, Default)]
struct UringParams {
    sq_entries: u32,
    cq_entries: u32,
    flags: u32,
    sq_thread_cpu: u32,
    sq_thread_idle: u32,
    features: u32,
    wq_fd: u32,
    resv: [u32; 3],
    sq_off_head: u32,
    sq_off_tail: u32,
    sq_off_ring_mask: u32,
    sq_off_ring_entries: u32,
    sq_off_flags: u32,
    sq_off_dropped: u32,
    sq_off_array: u32,
    sq_off_resv1: u32,
    sq_off_user_addr: u64,
    cq_off_head: u32,
    cq_off_tail: u32,
    cq_off_ring_mask: u32,
    cq_off_ring_entries: u32,
    cq_off_overflow: u32,
    cq_off_cqes: u32,
    cq_off_flags: u32,
    cq_off_resv1: u32,
    cq_off_user_addr: u64,
}

/// Parent-side ring handle. Plain data (Copy) so it can be embedded in the
/// shared measurement struct and hashed as part of the benchmark kernel path.
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct Ring {
    pub fd: i32,
    pub _pad: u32,
    pub sq_ptr: u64,
    pub sq_len: usize,
    pub cq_ptr: u64,
    pub cq_len: usize,
    pub sqe_ptr: u64,
    pub sqes_len: usize,
    pub sq_head_ptr: u64,
    pub sq_tail_ptr: u64,
    pub sq_mask: u64,
    pub sq_entries: u64,
    pub sq_array_ptr: u64,
    pub cq_head_ptr: u64,
    pub cq_tail_ptr: u64,
    pub cq_mask: u64,
    pub cq_entries: u64,
    pub cqe_ptr: u64,
}

impl Ring {
    pub fn destroy(&self) {
        if self.fd < 0 {
            return;
        }
        if self.sq_ptr != 0 {
            let _ = sys::munmap(self.sq_ptr as *mut u8, self.sq_len);
        }
        if self.cq_ptr != 0 {
            let _ = sys::munmap(self.cq_ptr as *mut u8, self.cq_len);
        }
        if self.sqe_ptr != 0 {
            let _ = sys::munmap(self.sqe_ptr as *mut u8, self.sqes_len);
        }
        let _ = sys::close(self.fd);
    }
}

pub fn setup(entries: u32) -> Result<Ring, sys::Errno> {
    let mut p = UringParams::default();
    let fd = sys::io_uring_setup(entries, &mut p as *mut UringParams as *mut u8)?;
    let sq_len = (p.sq_off_array as usize + p.sq_entries as usize * 4 + 4095) & !4095;
    let cq_len = (p.cq_off_cqes as usize + p.cq_entries as usize * 16 + 4095) & !4095;
    let sqes_len = p.sq_entries as usize * SQE_SIZE as usize;
    let sq = match sys::mmap(
        core::ptr::null_mut(),
        sq_len,
        sys::PROT_READ | sys::PROT_WRITE,
        sys::MAP_SHARED | sys::MAP_POPULATE,
        fd,
        IORING_OFF_SQ_RING,
    ) {
        Ok(v) => v,
        Err(e) => {
            let _ = sys::close(fd);
            return Err(e);
        }
    };
    let cq = match sys::mmap(
        core::ptr::null_mut(),
        cq_len,
        sys::PROT_READ | sys::PROT_WRITE,
        sys::MAP_SHARED | sys::MAP_POPULATE,
        fd,
        IORING_OFF_CQ_RING,
    ) {
        Ok(v) => v,
        Err(e) => {
            let _ = sys::munmap(sq, sq_len);
            let _ = sys::close(fd);
            return Err(e);
        }
    };
    let sqes = match sys::mmap(
        core::ptr::null_mut(),
        sqes_len,
        sys::PROT_READ | sys::PROT_WRITE,
        sys::MAP_SHARED | sys::MAP_POPULATE,
        fd,
        IORING_OFF_SQES,
    ) {
        Ok(v) => v,
        Err(e) => {
            let _ = sys::munmap(sq, sq_len);
            let _ = sys::munmap(cq, cq_len);
            let _ = sys::close(fd);
            return Err(e);
        }
    };
    let sq_mask = unsafe {
        core::ptr::read_volatile((sq as usize + p.sq_off_ring_mask as usize) as *const u32) as u64
    };
    let cq_mask = unsafe {
        core::ptr::read_volatile((cq as usize + p.cq_off_ring_mask as usize) as *const u32) as u64
    };
    Ok(Ring {
        fd,
        _pad: 0,
        sq_ptr: sq as u64,
        sq_len,
        cq_ptr: cq as u64,
        cq_len,
        sqe_ptr: sqes as u64,
        sqes_len,
        sq_head_ptr: sq as u64 + p.sq_off_head as u64,
        sq_tail_ptr: sq as u64 + p.sq_off_tail as u64,
        sq_mask,
        sq_entries: p.sq_entries as u64,
        sq_array_ptr: sq as u64 + p.sq_off_array as u64,
        cq_head_ptr: cq as u64 + p.cq_off_head as u64,
        cq_tail_ptr: cq as u64 + p.cq_off_tail as u64,
        cq_mask,
        cq_entries: p.cq_entries as u64,
        cqe_ptr: cq as u64 + p.cq_off_cqes as u64,
    })
}

static PROBE: AtomicI32 = AtomicI32::new(0);
static PROBE_ERR: AtomicI32 = AtomicI32::new(0);

pub fn check_available() -> Result<(), &'static str> {
    if PROBE.load(Ordering::Relaxed) == 0 {
        let mut p = UringParams::default();
        match sys::io_uring_setup(8, &mut p as *mut UringParams as *mut u8) {
            Ok(fd) => {
                let _ = sys::close(fd);
                PROBE.store(1, Ordering::Relaxed);
            }
            Err(e) => {
                PROBE_ERR.store(e.raw(), Ordering::Relaxed);
                PROBE.store(2, Ordering::Relaxed);
            }
        }
    }
    if PROBE.load(Ordering::Relaxed) == 1 {
        Ok(())
    } else {
        Err(unavailable_reason())
    }
}

pub fn available() -> bool {
    check_available().is_ok()
}

fn unavailable_reason() -> &'static str {
    match PROBE_ERR.load(Ordering::Relaxed) {
        sys::ENOSYS => "io_uring unavailable (ENOSYS)",
        sys::EPERM => "io_uring unavailable (EPERM)",
        sys::EACCES => "io_uring unavailable (EACCES)",
        sys::ENOMEM => "io_uring unavailable (ENOMEM)",
        sys::EINVAL => "io_uring unavailable (EINVAL)",
        sys::EBADF => "io_uring unavailable (EBADF)",
        _ => "io_uring unavailable on this kernel/environment",
    }
}

#[repr(C)]
pub struct UringShared {
    pub ring: Ring,
    pub fd: i32,
    pub op: u32,
    pub check: u32,
    pub block: u32,
    pub qd: u32,
    pub quota: u32,
    _pad: u32,
    pub target_ns: u64,
    pub read_start: u64,
    pub read_len: u64,
    pub write_start: u64,
    pub write_len: u64,
    pub buf: u64,
    pub total_ops: u64,
    pub dur_ns: u64,
    pub errors: u64,
    pub sample_cnt: u32,
    _pad2: u32,
    pub submit_cycles: [u64; MAX_QD],
    pub samples: [u32; MAX_SAMPLES],
}

static mut SHARED: core::mem::MaybeUninit<UringShared> = core::mem::MaybeUninit::uninit();

fn shared_ptr() -> *mut UringShared {
    unsafe { (*core::ptr::addr_of_mut!(SHARED)).as_mut_ptr() }
}

pub fn shared() -> &'static mut UringShared {
    unsafe { &mut *shared_ptr() }
}

pub fn invoke() -> i32 {
    unsafe { vmbench_k_storage_uring_run(shared_ptr() as usize) }
}

pub fn take_stats() -> IoStats {
    let s = shared();
    let cnt = (s.sample_cnt as usize).min(MAX_SAMPLES);
    let dur_ns = s.dur_ns;
    let ops = s.total_ops;
    let block = s.block as u64;
    io_stats(&mut s.samples[..cnt], dur_ns, ops, block)
}

// ---------------------------------------------------------------------------
// Ring operations used from the measured kernel
// ---------------------------------------------------------------------------

#[inline]
unsafe fn ring_submit(r: &Ring, op: u8, fd: i32, addr: u64, len: u32, off: u64, user_data: u64) {
    let tail = core::ptr::read_volatile(r.sq_tail_ptr as *const u32);
    let idx = tail as u64 & r.sq_mask;
    let sqe = (r.sqe_ptr + idx * SQE_SIZE) as *mut u8;
    // Explicit field stores (no memset call, which the audit forbids).
    core::ptr::write_volatile(sqe, op);
    core::ptr::write_volatile(sqe.add(1), 0u8);
    core::ptr::write_volatile(sqe.add(2) as *mut u16, 0u16);
    core::ptr::write_volatile(sqe.add(4) as *mut i32, fd);
    core::ptr::write_volatile(sqe.add(8) as *mut u64, off);
    core::ptr::write_volatile(sqe.add(16) as *mut u64, addr);
    core::ptr::write_volatile(sqe.add(24) as *mut u32, len);
    core::ptr::write_volatile(sqe.add(28) as *mut u32, 0u32);
    core::ptr::write_volatile(sqe.add(32) as *mut u64, user_data);
    core::ptr::write_volatile(sqe.add(40) as *mut u64, 0u64);
    core::ptr::write_volatile(sqe.add(48) as *mut u64, 0u64);
    core::ptr::write_volatile(sqe.add(56) as *mut u64, 0u64);
    core::ptr::write_volatile((r.sq_array_ptr + idx * 4) as *mut u32, idx as u32);
    core::sync::atomic::fence(Ordering::Release);
    core::ptr::write_volatile(r.sq_tail_ptr as *mut u32, tail.wrapping_add(1));
}

#[inline]
unsafe fn ring_enter(r: &Ring, to_submit: u32, min_complete: u32) -> i32 {
    match sys::io_uring_enter(
        r.fd,
        to_submit,
        min_complete,
        IORING_ENTER_GETEVENTS,
        core::ptr::null_mut(),
        0,
    ) {
        Ok(v) => v,
        Err(_) => -1,
    }
}

#[inline]
unsafe fn submit_op(
    s: &UringShared,
    rng: &mut Rng,
    slot: u32,
    block: u64,
    read_blocks: u64,
    write_blocks: u64,
) -> u64 {
    let buf = s.buf + slot as u64 * block;
    let (opcode, off) = match s.op {
        OP_WRITE => (
            IORING_OP_WRITE,
            s.write_start + rng.next_bounded(write_blocks) * block,
        ),
        OP_MIXED => {
            if rng.next_bounded(100) < 70 {
                (
                    IORING_OP_READ,
                    s.read_start + rng.next_bounded(read_blocks) * block,
                )
            } else {
                (
                    IORING_OP_WRITE,
                    s.write_start + rng.next_bounded(write_blocks) * block,
                )
            }
        }
        _ => (
            IORING_OP_READ,
            s.read_start + rng.next_bounded(read_blocks) * block,
        ),
    };
    let c = time::cycles();
    ring_submit(&s.ring, opcode, s.fd, buf, block as u32, off, slot as u64);
    c
}

/// Submits one SQE and waits for its completion. Returns the CQE result.
unsafe fn submit_wait_one(r: &Ring, op: u8, fd: i32, addr: u64, len: u32, off: u64) -> i32 {
    ring_submit(r, op, fd, addr, len, off, 0xFFFF_FFFF);
    loop {
        let _ = ring_enter(r, 1, 1);
        let head = core::ptr::read_volatile(r.cq_head_ptr as *const u32);
        let tail = core::ptr::read_volatile(r.cq_tail_ptr as *const u32);
        if head == tail {
            continue;
        }
        let cqe = (r.cqe_ptr + (head as u64 & r.cq_mask) * CQE_SIZE) as *const u8;
        let res = core::ptr::read_volatile(cqe.add(8) as *const i32);
        core::ptr::write_volatile(r.cq_head_ptr as *mut u32, head.wrapping_add(1));
        return res;
    }
}

unsafe fn uring_check(s: &mut UringShared) -> i32 {
    let r = s.ring;
    let block = s.block as u64;
    if submit_wait_one(&r, IORING_OP_WRITE, s.fd, s.buf, block as u32, CHECK_OFF) < 0 {
        return 1;
    }
    if submit_wait_one(&r, IORING_OP_READ, s.fd, s.buf + block, block as u32, CHECK_OFF) < 0 {
        return 2;
    }
    let a = s.buf as *const u8;
    let b = (s.buf + block) as *const u8;
    let mut i = 0u64;
    while i < block {
        if core::ptr::read_volatile(a.add(i as usize)) != core::ptr::read_volatile(b.add(i as usize)) {
            return 3;
        }
        i += 1;
    }
    0
}

/// Measured kernel: drives `qd` in-flight asynchronous operations until the
/// time budget expires, recording one latency sample per completed operation.
#[no_mangle]
pub unsafe extern "C" fn vmbench_k_storage_uring_run(arg: usize) -> i32 {
    let s = &mut *(arg as *mut UringShared);
    if s.check != 0 {
        return uring_check(s);
    }
    let ring = s.ring;
    let block = s.block as u64;
    let qd = s.qd.clamp(1, MAX_QD as u32);
    let read_blocks = (s.read_len / block).max(1);
    let write_blocks = (s.write_len / block).max(1);
    let mut rng = Rng::new(0x243f_6a88_85a3_08d3);
    let mut sampler = Sampler::new(s.quota, s.target_ns);
    let deadline = time::Deadline::new(s.target_ns);
    let t0 = time::mono_ns();
    let mut in_flight: u32 = 0;
    let mut pending: u32 = 0;
    let mut ops: u64 = 0;
    let mut errors: u64 = 0;

    while in_flight < qd {
        let slot = in_flight;
        s.submit_cycles[slot as usize] =
            submit_op(s, &mut rng, slot, block, read_blocks, write_blocks);
        in_flight += 1;
        pending += 1;
    }

    loop {
        let wait = if in_flight > 0 { 1 } else { 0 };
        if pending > 0 || wait > 0 {
            let _ = ring_enter(&ring, pending, wait);
            pending = 0;
        }
        loop {
            let head = core::ptr::read_volatile(ring.cq_head_ptr as *const u32);
            let tail = core::ptr::read_volatile(ring.cq_tail_ptr as *const u32);
            if head == tail {
                break;
            }
            let cqe = (ring.cqe_ptr + (head as u64 & ring.cq_mask) * CQE_SIZE) as *const u8;
            let res = core::ptr::read_volatile(cqe.add(8) as *const i32);
            let slot = core::ptr::read_volatile(cqe as *const u64) as u32;
            let c1 = time::cycles();
            core::ptr::write_volatile(ring.cq_head_ptr as *mut u32, head.wrapping_add(1));
            if res >= 0 {
                ops += 1;
                let c0 = s.submit_cycles[(slot as usize).min(MAX_QD - 1)];
                let ns = cycles_to_ns_u32(c1.wrapping_sub(c0));
                if let Some(i) = sampler.on_op(ns) {
                    s.samples[i] = ns;
                }
            } else {
                errors += 1;
            }
            if deadline.expired() {
                in_flight = in_flight.saturating_sub(1);
            } else {
                s.submit_cycles[slot as usize] =
                    submit_op(s, &mut rng, slot, block, read_blocks, write_blocks);
                pending += 1;
            }
        }
        if in_flight == 0 {
            break;
        }
    }

    let dur = time::mono_ns().wrapping_sub(t0).max(1);
    s.total_ops = ops;
    s.errors = errors;
    s.dur_ns = dur;
    s.sample_cnt = sampler.count();
    0
}
