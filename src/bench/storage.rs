use crate::bench::common::*;
use crate::bench::uring;
use crate::bench::Benchmark;
use crate::runner::{
    Ctx, EntryMeta, ImplInfo, MetricDef, MetricKind, ParamValue, Point, StorageTarget,
};
use crate::stats::Direction;
use crate::sys;
use crate::time;
use core::sync::atomic::{AtomicU32, Ordering};

pub const BLOCK_4K: u64 = 4096;
pub const BLOCK_1M: u64 = 1 << 20;

/// Blocking-stream concurrency levels. These depend on the host scheduler and
/// are reported as a separate benchmark family from io_uring.
pub const STREAMS_LEVELS: [usize; 3] = [1, 4, 16];
/// io_uring queue depths. A single process keeps this many operations in
/// flight, so the values do not depend on the number of online CPUs.
pub const URING_LEVELS: [usize; 6] = [1, 2, 4, 8, 16, 32];
const STREAMS_LEVELS_STR: &str = "1,4,16";
const URING_LEVELS_STR: &str = "1,2,4,8,16,32";

const S_IFMT: u16 = 0o170000;
const S_IFDIR: u16 = 0o040000;
const S_IFBLK: u16 = 0o060000;
const S_IFCHR: u16 = 0o020000;

// ---------------------------------------------------------------------------
// Storage targets
// ---------------------------------------------------------------------------

/// Per-target state published in the report.
#[derive(Clone, Copy)]
pub struct TargetState {
    pub fstype: &'static str,
    pub status: &'static str,
}

impl Default for TargetState {
    fn default() -> Self {
        TargetState {
            fstype: "unavailable",
            status: "not_run",
        }
    }
}

pub struct StorageEnv {
    pub name: &'static str,
    pub dir: &'static str,
    pub file: &'static str,
    pub owns_dir: bool,
    pub size: u64,
    pub direct: bool,
    pub fd: i32,
    pub dsync_fd: i32,
    pub buffered_fd: i32,
    pub buf_region: *mut u8,
    pub buf_region_len: usize,
    pub fstype: &'static str,
    pub uring: Option<uring::Ring>,
}

impl StorageEnv {
    fn cleanup(&self) {
        if self.fd >= 0 {
            let _ = sys::close(self.fd);
        }
        if self.dsync_fd >= 0 {
            let _ = sys::close(self.dsync_fd);
        }
        if self.buffered_fd >= 0 {
            let _ = sys::close(self.buffered_fd);
        }
        if let Some(r) = &self.uring {
            r.destroy();
        }
        let _ = sys::unlinkat(sys::AT_FDCWD, self.file.as_bytes(), 0);
        if self.owns_dir {
            let _ = sys::unlinkat(sys::AT_FDCWD, self.dir.as_bytes(), sys::AT_REMOVEDIR);
        }
        if !self.buf_region.is_null() && self.buf_region_len > 0 {
            let _ = sys::munmap(self.buf_region, self.buf_region_len);
        }
    }
}

pub fn cleanup(ctx: &mut Ctx) {
    for i in 0..ctx.cfg.n_storage_targets {
        if let Some(env) = ctx.storage[i].take() {
            env.cleanup();
        } else {
            let t = ctx.cfg.storage_targets[i];
            if t.auto_dir && !t.path.is_empty() {
                let _ = sys::unlinkat(sys::AT_FDCWD, t.path.as_bytes(), sys::AT_REMOVEDIR);
            }
        }
    }
}

fn align_ptr(p: *mut u8) -> *mut u8 {
    ((p as usize + 4095) & !4095) as *mut u8
}

fn join_path(dir: &str, file: &str, out: &mut [u8; 4096]) -> Option<usize> {
    let d = dir.as_bytes();
    let f = file.as_bytes();
    let need = d.len() + 1 + f.len();
    if need >= out.len() {
        return None;
    }
    out[..d.len()].copy_from_slice(d);
    out[d.len()] = b'/';
    out[d.len() + 1..need].copy_from_slice(f);
    Some(need)
}

fn abs_path(ctx: &mut Ctx, path: &'static str) -> &'static str {
    if path.starts_with('/') {
        return path;
    }
    let mut cwd = [0u8; 1024];
    let n = match sys::getcwd(&mut cwd) {
        Ok(n) => n,
        Err(_) => return path,
    };
    let mut buf = [0u8; 2048];
    if n + 1 + path.len() >= buf.len() {
        return path;
    }
    buf[..n].copy_from_slice(&cwd[..n]);
    buf[n] = b'/';
    buf[n + 1..n + 1 + path.len()].copy_from_slice(path.as_bytes());
    unsafe { ctx.arena.copy_str(&buf[..n + 1 + path.len()]) }.unwrap_or(path)
}

fn mount_fstype(ctx: &Ctx, path: &str) -> &'static str {
    let mut best: Option<(&'static str, usize)> = None;
    for m in ctx.env.mounts {
        let mp = m.mount_point;
        if path.starts_with(mp) {
            let len = if mp == "/" { 1 } else { mp.len() };
            if best.map(|(_, l)| len > l).unwrap_or(true) {
                best = Some((m.fstype, len));
            }
        }
    }
    best.map(|(f, _)| f).unwrap_or("unknown")
}

fn err_msg(ctx: &mut Ctx, what: &str, e: sys::Errno) -> &'static str {
    let mut buf = [0u8; 192];
    let w = what.as_bytes();
    let en = sys::err_name(e).as_bytes();
    let need = w.len() + en.len() + 3;
    if need > buf.len() {
        return "storage setup failed";
    }
    buf[..w.len()].copy_from_slice(w);
    buf[w.len()] = b' ';
    buf[w.len() + 1] = b'(';
    buf[w.len() + 2..w.len() + 2 + en.len()].copy_from_slice(en);
    buf[need - 1] = b')';
    unsafe { ctx.arena.copy_str(&buf[..need]) }.unwrap_or("storage setup failed")
}

fn fail_target(t: &StorageTarget, reason: &str) -> ! {
    let _ = sys::write_all(2, b"vmbench: storage target '");
    let _ = sys::write_all(2, t.name.as_bytes());
    let _ = sys::write_all(2, b"'");
    if !t.path.is_empty() {
        let _ = sys::write_all(2, b" (");
        let _ = sys::write_all(2, t.path.as_bytes());
        let _ = sys::write_all(2, b")");
    }
    let _ = sys::write_all(2, b": ");
    let _ = sys::write_all(2, reason.as_bytes());
    let _ = sys::write_all(2, b"\n");
    sys::exit(2)
}

fn create_auto_dir(ctx: &mut Ctx) -> Result<&'static str, sys::Errno> {
    let mut rnd = [0u8; 4];
    let _ = sys::getrandom(&mut rnd);
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut name = [0u8; 32];
    let prefix = b"vmbench-";
    name[..prefix.len()].copy_from_slice(prefix);
    for (i, &b) in rnd.iter().enumerate() {
        name[prefix.len() + i * 2] = HEX[(b >> 4) as usize];
        name[prefix.len() + i * 2 + 1] = HEX[(b & 0xf) as usize];
    }
    let len = prefix.len() + rnd.len() * 2;
    sys::mkdirat(sys::AT_FDCWD, &name[..len], 0o700)?;
    unsafe { ctx.arena.copy_str(&name[..len]) }.ok_or(sys::Errno(sys::ENOMEM))
}

fn stat_target(t: &StorageTarget) -> Result<sys::Statx, sys::Errno> {
    let mut st = sys::Statx::default();
    sys::statx(
        sys::AT_FDCWD,
        t.path.as_bytes(),
        0,
        sys::STATX_BASIC_STATS,
        &mut st,
    )?;
    Ok(st)
}

fn probe_target(t: &StorageTarget, pid: i32) -> Result<(), sys::Errno> {
    let mut fname = [0u8; 64];
    let pre = b".vmbench-probe-";
    fname[..pre.len()].copy_from_slice(pre);
    let n = crate::fmt::u64_dec(pid as u64, &mut fname[pre.len()..]);
    let mut path = [0u8; 4096];
    let len = match join_path(t.path, unsafe {
        core::str::from_utf8_unchecked(&fname[..pre.len() + n])
    }, &mut path)
    {
        Some(l) => l,
        None => return Err(sys::Errno(sys::EINVAL)),
    };
    let fd = sys::open(
        &path[..len],
        sys::O_WRONLY | sys::O_CREAT | sys::O_EXCL | sys::O_CLOEXEC,
        0o600,
    )?;
    let _ = sys::close(fd);
    let _ = sys::unlinkat(sys::AT_FDCWD, &path[..len], 0);
    Ok(())
}

/// Validates every configured target and creates the implicit work directory.
/// Exits with a diagnostic before any benchmark runs when a target is invalid,
/// so a bad path is never discovered after the CPU suite.
pub fn preflight(ctx: &mut Ctx) {
    let pid = sys::getpid();
    for i in 0..ctx.cfg.n_storage_targets {
        let mut t = ctx.cfg.storage_targets[i];
        if t.auto_dir {
            match create_auto_dir(ctx) {
                Ok(p) => t.path = p,
                Err(e) => fail_target(&t, sys::err_name(e)),
            }
        }
        let st = match stat_target(&t) {
            Ok(st) => st,
            Err(e) => fail_target(&t, sys::err_name(e)),
        };
        let mode = st.stx_mode & S_IFMT;
        if mode == S_IFBLK || mode == S_IFCHR {
            fail_target(
                &t,
                "block/character device paths are not supported; pass a directory on a mounted filesystem",
            );
        }
        if mode != S_IFDIR {
            fail_target(&t, "not a directory");
        }
        if let Err(e) = probe_target(&t, pid) {
            fail_target(&t, sys::err_name(e));
        }
        ctx.cfg.storage_targets[i] = t;
        let ap = abs_path(ctx, t.path);
        let fstype = mount_fstype(ctx, ap);
        ctx.storage_state[i] = TargetState {
            fstype,
            status: "ok",
        };
        // Allocate the test file now, so out-of-space or permission problems
        // are reported before the CPU suite rather than after it.
        if let Err(msg) = ensure_storage(ctx, i) {
            fail_target(&t, msg);
        }
    }
}

fn target_alive(ctx: &Ctx, idx: usize) -> bool {
    let t = ctx.cfg.storage_targets[idx];
    match stat_target(&t) {
        Ok(st) => (st.stx_mode & S_IFMT) == S_IFDIR,
        Err(_) => false,
    }
}

// ---------------------------------------------------------------------------
// Test file environment
// ---------------------------------------------------------------------------

fn create_storage(ctx: &mut Ctx, idx: usize) -> Result<(), &'static str> {
    let t = ctx.cfg.storage_targets[idx];
    let fstype = ctx.storage_state[idx].fstype;

    if t.auto_dir {
        match sys::mkdirat(sys::AT_FDCWD, t.path.as_bytes(), 0o700) {
            Ok(()) => {}
            Err(e) if e.0 == sys::EEXIST => {}
            Err(e) => return Err(err_msg(ctx, "cannot create storage work directory", e)),
        }
    }

    let mut fname = [0u8; 96];
    let pre = b"vmbench-";
    fname[..pre.len()].copy_from_slice(pre);
    let mut off = pre.len();
    let nb = t.name.as_bytes();
    if nb.len() > 32 {
        return Err("storage target name too long");
    }
    fname[off..off + nb.len()].copy_from_slice(nb);
    off += nb.len();
    fname[off] = b'-';
    off += 1;
    let n = crate::fmt::u64_dec(sys::getpid() as u64, &mut fname[off..]);
    off += n;
    fname[off..off + 4].copy_from_slice(b".bin");
    off += 4;
    let fname_str = unsafe { core::str::from_utf8_unchecked(&fname[..off]) };

    let mut pathbuf = [0u8; 4096];
    let plen = match join_path(t.path, fname_str, &mut pathbuf) {
        Some(l) => l,
        None => return Err("storage path too long"),
    };
    let file = unsafe { ctx.arena.copy_str(&pathbuf[..plen]) }.ok_or("arena exhausted")?;

    let mem = ctx.env.mem.total_kib.unwrap_or(256 * 1024) as u64 * 1024;
    let cap = if ctx.cfg.quick { 256 << 20 } else { 1024 << 20 };
    let floor = if ctx.cfg.quick { 64 << 20 } else { 256 << 20 };
    let size = (mem / 8).clamp(floor, cap) & !4095;

    let _ = sys::unlinkat(sys::AT_FDCWD, file.as_bytes(), 0);
    let fd = sys::open(
        file.as_bytes(),
        sys::O_WRONLY | sys::O_CREAT | sys::O_TRUNC | sys::O_CLOEXEC,
        0o600,
    )
    .map_err(|e| err_msg(ctx, "cannot create storage test file", e))?;

    // Pseudo-random 1 MiB scratch for file content.
    let scratch = unsafe { ctx.arena.alloc_bytes((1 << 20) + 4096, 4096) }
        .ok_or("arena exhausted")?;
    let sp = align_ptr(scratch.as_mut_ptr());
    let mut rng = crate::rand::Rng::new(0x51ed_2701_abcdef01);
    unsafe {
        rng.fill_bytes(core::slice::from_raw_parts_mut(sp, 1 << 20));
    }
    let mut written = 0u64;
    while written < size {
        let n = ((size - written).min(1 << 20)) as usize;
        let buf = unsafe { core::slice::from_raw_parts(sp, n) };
        match sys::pwrite(fd, buf, written) {
            Ok(0) => break,
            Ok(k) => written += k as u64,
            Err(_) => break,
        }
    }
    let _ = sys::fsync(fd);
    let _ = sys::close(fd);
    if written < size {
        let _ = sys::unlinkat(sys::AT_FDCWD, file.as_bytes(), 0);
        return Err("failed to allocate storage test file (out of space?)");
    }

    // Direct I/O when supported; otherwise fall back to buffered.
    let direct_fd = sys::open(
        file.as_bytes(),
        sys::O_RDWR | sys::O_DIRECT | sys::O_CLOEXEC,
        0,
    );
    let (fd, direct) = match direct_fd {
        Ok(fd) => (fd, true),
        Err(_) => (
            sys::open(file.as_bytes(), sys::O_RDWR | sys::O_CLOEXEC, 0)
                .map_err(|e| err_msg(ctx, "cannot reopen storage test file", e))?,
            false,
        ),
    };
    let dsync_fd = sys::open(
        file.as_bytes(),
        sys::O_RDWR | sys::O_DSYNC | sys::O_CLOEXEC,
        0,
    )
    .unwrap_or(-1);
    let buffered_fd = sys::open(file.as_bytes(), sys::O_RDWR | sys::O_CLOEXEC, 0).unwrap_or(-1);

    let region_len = (1 << 20) + 4096;
    let region = sys::mmap_anon(region_len, false).ok_or("mmap failed")?;
    let aligned = align_ptr(region);
    unsafe {
        rng.fill_bytes(core::slice::from_raw_parts_mut(aligned, 1 << 20));
    }

    let env = unsafe {
        ctx.arena.boxed(StorageEnv {
            name: t.name,
            dir: t.path,
            file,
            owns_dir: t.auto_dir,
            size,
            direct,
            fd,
            dsync_fd,
            buffered_fd,
            buf_region: region,
            buf_region_len: region_len,
            fstype,
            uring: None,
        })
    }
    .ok_or("arena exhausted")?;
    ctx.storage[idx] = Some(env);

    if !direct {
        let head = b"storage[";
        let tail = b"]: O_DIRECT unsupported on this filesystem; results are buffered (page cache not bypassed)";
        let mut ann = [0u8; 256];
        let mut n = 0usize;
        ann[n..n + head.len()].copy_from_slice(head);
        n += head.len();
        ann[n..n + t.name.len()].copy_from_slice(t.name.as_bytes());
        n += t.name.len();
        ann[n..n + tail.len()].copy_from_slice(tail);
        n += tail.len();
        if let Some(s) = unsafe { ctx.arena.copy_str(&ann[..n]) } {
            ctx.annotate(s);
        }
    }
    Ok(())
}

pub fn ensure_storage(ctx: &mut Ctx, idx: usize) -> Result<(), &'static str> {
    if ctx.storage[idx].is_some() {
        return Ok(());
    }
    match create_storage(ctx, idx) {
        Ok(()) => Ok(()),
        Err(msg) => {
            ctx.storage_state[idx].status = "failed";
            Err(msg)
        }
    }
}

// ---------------------------------------------------------------------------
// Worker
// ---------------------------------------------------------------------------

#[repr(C)]
pub struct StorageShared {
    pub fd: i32,
    pub dsync_fd: i32,
    pub op: u32,
    pub sampling: u32,
    pub quota: u32,
    pub block: u64,
    pub read_start: u64,
    pub read_len: u64,
    pub write_start: u64,
    pub write_len: u64,
    pub target_ns: u64,
    pub buf: u64,
    pub done: AtomicU32,
    pub units: [u64; MAX_WORKERS],
    pub dur_ns: [u64; MAX_WORKERS],
    pub sample_cnt: [u32; MAX_WORKERS],
    pub samples: [u32; MAX_SAMPLES],
}

const OP_RAND_READ: u32 = 0;
const OP_RAND_WRITE: u32 = 1;
const OP_SEQ_READ: u32 = 2;
const OP_SEQ_WRITE: u32 = 3;
const OP_SYNC_FDATASYNC: u32 = 4;
const OP_SYNC_DSYNC: u32 = 5;
const OP_MIXED: u32 = 6;

#[no_mangle]
pub extern "C" fn vmbench_w_storage(arg: usize) -> i32 {
    let wctx = unsafe { &*(arg as *const WorkerCtx) };
    let mut mask = [0u8; 128];
    mask[(wctx.cpu / 8) as usize] |= 1 << (wctx.cpu % 8);
    let _ = sys::sched_setaffinity(0, &mask);
    let s = wctx.shared as *mut StorageShared;
    let fd = unsafe { (*s).fd };
    let dsync_fd = unsafe { (*s).dsync_fd };
    let op = unsafe { (*s).op };
    let sampling = unsafe { (*s).sampling } != 0;
    let quota = unsafe { (*s).quota };
    let block = unsafe { (*s).block };
    let read_start = unsafe { (*s).read_start };
    let read_len = unsafe { (*s).read_len };
    let write_start = unsafe { (*s).write_start };
    let write_len = unsafe { (*s).write_len };
    let target = unsafe { (*s).target_ns }.max(1_000_000);
    let buf_base = unsafe { (*s).buf };
    let idx = wctx.index as usize;
    let buf = (buf_base + (idx as u64) * block) as *mut u8;
    if buf_base == 0 {
        return 1;
    }
    let mut rng =
        crate::rand::Rng::new(0x9e37_79b9_7f4a_7c15 ^ (idx as u64).wrapping_mul(0x1000_0000_1b3));
    let mut sampler = Sampler::new(if sampling { quota } else { 0 }, target);
    let read_blocks = (read_len / block).max(1);
    let write_blocks = (write_len / block).max(1);
    let mut roff = read_start + (idx as u64 % read_blocks) * block;
    let mut woff = write_start + (idx as u64 % write_blocks) * block;
    let mut ops = 0u64;
    let t0 = time::mono_ns();
    let deadline = time::Deadline::new(target);
    let slot_base = idx * quota as usize;
    loop {
        let c0 = time::cycles();
        let ok = unsafe {
            match op {
                OP_RAND_READ => {
                    let off = read_start + rng.next_bounded(read_blocks) * block;
                    let dst = core::slice::from_raw_parts_mut(buf, block as usize);
                    sys::pread(fd, dst, off).is_ok()
                }
                OP_RAND_WRITE => {
                    let off = write_start + rng.next_bounded(write_blocks) * block;
                    let src = core::slice::from_raw_parts(buf, block as usize);
                    sys::pwrite(fd, src, off).is_ok()
                }
                OP_SEQ_READ => {
                    let dst = core::slice::from_raw_parts_mut(buf, block as usize);
                    let r = sys::pread(fd, dst, roff).is_ok();
                    roff += block;
                    if roff >= read_start + read_len {
                        roff = read_start;
                    }
                    r
                }
                OP_SEQ_WRITE => {
                    let src = core::slice::from_raw_parts(buf, block as usize);
                    let r = sys::pwrite(fd, src, woff).is_ok();
                    woff += block;
                    if woff >= write_start + write_len {
                        woff = write_start;
                    }
                    r
                }
                OP_SYNC_FDATASYNC => {
                    let off = write_start + rng.next_bounded(write_blocks) * block;
                    let src = core::slice::from_raw_parts(buf, block as usize);
                    sys::pwrite(fd, src, off).is_ok() && sys::fdatasync(fd).is_ok()
                }
                OP_SYNC_DSYNC => {
                    let off = write_start + rng.next_bounded(write_blocks) * block;
                    let src = core::slice::from_raw_parts(buf, block as usize);
                    sys::pwrite(dsync_fd, src, off).is_ok()
                }
                _ => {
                    if rng.next_bounded(100) < 70 {
                        let off = read_start + rng.next_bounded(read_blocks) * block;
                        let dst = core::slice::from_raw_parts_mut(buf, block as usize);
                        sys::pread(fd, dst, off).is_ok()
                    } else {
                        let off = write_start + rng.next_bounded(write_blocks) * block;
                        let src = core::slice::from_raw_parts(buf, block as usize);
                        sys::pwrite(fd, src, off).is_ok()
                    }
                }
            }
        };
        let c1 = time::cycles();
        if ok {
            ops += 1;
            if sampling {
                let ns = cycles_to_ns_u32(c1.wrapping_sub(c0));
                if let Some(i) = sampler.on_op(ns) {
                    unsafe {
                        (*s).samples[slot_base + i] = ns;
                    }
                }
            }
        }
        if deadline.expired() {
            break;
        }
    }
    let dur = time::mono_ns().wrapping_sub(t0).max(1);
    unsafe {
        (*s).units[idx] = ops;
        (*s).dur_ns[idx] = dur;
        (*s).sample_cnt[idx] = sampler.count();
        (*s).done.fetch_add(1, Ordering::Relaxed);
    }
    0
}

fn run_workers_once(
    ctx: &Ctx,
    target: usize,
    op: u32,
    block: u64,
    threads: usize,
    sampling: bool,
    target_ns: u64,
) -> Result<IoStats, &'static str> {
    let (fd, dsync_fd, size, buf_region) = {
        let env = ctx.storage[target].as_ref().ok_or("storage not initialised")?;
        (env.fd, env.dsync_fd, env.size, env.buf_region)
    };
    let n = threads.clamp(1, MAX_WORKERS);
    let quota = if sampling { (MAX_SAMPLES / n) as u32 } else { 0 };
    let mut pool = WorkerPool::new(n, core::mem::size_of::<StorageShared>())
        .ok_or("worker pool allocation failed")?;
    {
        let s = pool.shared_at::<StorageShared>(0);
        let (rs, rl, ws, wl) = if op == OP_MIXED {
            let ws = (size / 2) & !4095;
            (0u64, ws, ws, size - ws)
        } else if op == OP_SEQ_READ {
            (0, size, 0, 0)
        } else {
            (0, size, 0, size)
        };
        unsafe {
            (*s).fd = fd;
            (*s).dsync_fd = dsync_fd;
            (*s).op = op;
            (*s).sampling = if sampling { 1 } else { 0 };
            (*s).quota = quota;
            (*s).block = block;
            (*s).read_start = rs;
            (*s).read_len = rl;
            (*s).write_start = ws;
            (*s).write_len = wl;
            (*s).target_ns = target_ns;
            (*s).buf = buf_region as u64 + 4096 - ((buf_region as usize) & 4095) as u64;
            (*s).done = AtomicU32::new(0);
        }
    }
    for i in 0..n {
        pool.set_ctx(i, ctx.cpu_at(i), 0);
    }
    pool.spawn(vmbench_w_storage)?;
    pool.wait_all()?;
    let s = pool.shared_at::<StorageShared>(0);
    let mut ops = 0u64;
    let mut max_dur = 1u64;
    for i in 0..n {
        ops = ops.saturating_add(unsafe { (*s).units[i] });
        let d = unsafe { (*s).dur_ns[i] };
        if d > max_dur {
            max_dur = d;
        }
    }
    let mut total_samples = 0usize;
    let samples: &mut [u32; MAX_SAMPLES] = unsafe { &mut (*s).samples };
    if sampling {
        let mut w = 0usize;
        for i in 0..n {
            let c = unsafe { (*s).sample_cnt[i] } as usize;
            let src = i * quota as usize;
            if c > 0 {
                if w != src {
                    samples.copy_within(src..src + c, w);
                }
                w += c;
            }
        }
        total_samples = w.min(MAX_SAMPLES);
    }
    Ok(io_stats(
        &mut samples[..total_samples],
        max_dur,
        ops,
        block,
    ))
}

// ---------------------------------------------------------------------------
// Functional verification
// ---------------------------------------------------------------------------

/// Functional check of the storage kernels: write a known pattern with the
/// write kernel, read it back with an independent pread, and check that the
/// read kernel returns the requested length and content. When io_uring is
/// available, the ring kernel is exercised the same way.
pub fn verify_io(ctx: &mut Ctx) -> Result<(), &'static str> {
    for i in 0..ctx.cfg.n_storage_targets {
        if ensure_storage(ctx, i).is_err() {
            continue;
        }
        let (fd, size, buf_region) = {
            let env = ctx.storage[i].as_ref().ok_or("storage not initialised")?;
            (env.fd, env.size, env.buf_region)
        };
        let aligned = align_ptr(buf_region);
        let len = 4096u64;
        unsafe {
            for i in 0..(len as usize / 8) {
                core::ptr::write_volatile(
                    aligned.add(i * 8) as *mut u64,
                    0xA5A5_0000_0000_0000u64 ^ i as u64,
                );
            }
        }
        let w = unsafe { vmbench_k_storage_write_block(fd, 8192, size, aligned as usize, len) };
        if w != len {
            return Err("storage write kernel wrote wrong length");
        }
        let dst = unsafe { aligned.add(4096) };
        let rb = unsafe { core::slice::from_raw_parts_mut(dst, len as usize) };
        let n = sys::pread(fd, rb, 8192).map_err(|_| "storage verify pread failed")?;
        if n as u64 != len {
            return Err("storage verify pread returned short data");
        }
        for i in 0..len as usize {
            if rb[i] != unsafe { core::ptr::read_volatile(aligned.add(i)) } {
                return Err("storage write/read-back mismatch");
            }
        }
        let got = unsafe { vmbench_k_storage_read_region(fd, 8192, len, dst as usize, 4096) };
        if got != len {
            return Err("storage read_region returned wrong length");
        }
        for i in 0..len as usize {
            if unsafe { core::ptr::read_volatile(dst.add(i)) }
                != unsafe { core::ptr::read_volatile(aligned.add(i)) }
            {
                return Err("storage read_region content mismatch");
            }
        }

        if uring::available() {
            uring_verify(ctx, i)?;
        }
    }
    Ok(())
}

fn uring_verify(ctx: &mut Ctx, target: usize) -> Result<(), &'static str> {
    {
        let env = ctx.storage[target].as_mut().ok_or("storage not initialised")?;
        if env.uring.is_none() {
            let r = uring::setup(64).map_err(|_| "io_uring_setup failed during verification")?;
            env.uring = Some(r);
        }
    }
    let (fd, ring, buf_region) = {
        let env = ctx.storage[target].as_ref().unwrap();
        (
            env.fd,
            *env.uring.as_ref().unwrap(),
            env.buf_region,
        )
    };
    let aligned = align_ptr(buf_region);
    unsafe {
        for i in 0..512 {
            core::ptr::write_volatile(
                aligned.add(i * 8) as *mut u64,
                0xC3C3_0000_0000_0000u64 ^ i as u64,
            );
        }
    }
    {
        let s = uring::shared();
        s.ring = ring;
        s.fd = fd;
        s.op = uring::OP_READ;
        s.check = 1;
        s.block = 4096;
        s.qd = 1;
        s.quota = 0;
        s.target_ns = 0;
        s.read_start = 0;
        s.read_len = 4096;
        s.write_start = 0;
        s.write_len = 4096;
        s.buf = aligned as u64;
    }
    let rc = uring::invoke();
    if rc != 0 {
        return Err("io_uring functional check failed");
    }
    unsafe {
        let dst = aligned.add(8192);
        let rb = core::slice::from_raw_parts_mut(dst, 4096);
        let n = sys::pread(fd, rb, uring::CHECK_OFF)
            .map_err(|_| "io_uring verify pread failed")?;
        if n != 4096 {
            return Err("io_uring verify short read");
        }
        for i in 0..4096 {
            if rb[i] != core::ptr::read_volatile(aligned.add(i)) {
                return Err("io_uring verify content mismatch");
            }
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Metrics
// ---------------------------------------------------------------------------

pub const M_IOPS: MetricDef = MetricDef {
    key: "iops",
    unit: "IOPS",
    direction: Direction::HigherBetter,
    kind: MetricKind::Scalar,
};
pub const M_BYTES: MetricDef = MetricDef {
    key: "bytes_per_sec",
    unit: "bytes/s",
    direction: Direction::HigherBetter,
    kind: MetricKind::Scalar,
};
macro_rules! lat_metric {
    ($name:ident, $key:literal) => {
        pub const $name: MetricDef = MetricDef {
            key: $key,
            unit: "ns",
            direction: Direction::LowerBetter,
            kind: MetricKind::Scalar,
        };
    };
}
lat_metric!(M_LAT_MEAN, "latency_mean_ns");
lat_metric!(M_LAT_P50, "latency_p50_ns");
lat_metric!(M_LAT_P95, "latency_p95_ns");
lat_metric!(M_LAT_P99, "latency_p99_ns");
lat_metric!(M_LAT_P999, "latency_p99_9_ns");
lat_metric!(M_LAT_MAX, "latency_max_ns");

const SEQ_METRICS: &[MetricDef] = &[M_IOPS, M_BYTES];
const LAT_METRICS: &[MetricDef] = &[
    M_IOPS,
    M_BYTES,
    M_LAT_MEAN,
    M_LAT_P50,
    M_LAT_P95,
    M_LAT_P99,
    M_LAT_P999,
    M_LAT_MAX,
];

macro_rules! curve_metric {
    ($name:ident, $key:literal, $unit:literal, $dir:expr) => {
        pub const $name: MetricDef = MetricDef {
            key: $key,
            unit: $unit,
            direction: $dir,
            kind: MetricKind::Curve,
        };
    };
}
curve_metric!(C_IOPS, "iops", "IOPS", Direction::HigherBetter);
curve_metric!(C_BYTES, "bytes_per_sec", "bytes/s", Direction::HigherBetter);
curve_metric!(C_LAT_MEAN, "latency_mean_ns", "ns", Direction::LowerBetter);
curve_metric!(C_LAT_P50, "latency_p50_ns", "ns", Direction::LowerBetter);
curve_metric!(C_LAT_P95, "latency_p95_ns", "ns", Direction::LowerBetter);
curve_metric!(C_LAT_P99, "latency_p99_ns", "ns", Direction::LowerBetter);
curve_metric!(C_LAT_P999, "latency_p99_9_ns", "ns", Direction::LowerBetter);
curve_metric!(C_LAT_MAX, "latency_max_ns", "ns", Direction::LowerBetter);

const SWEEP_METRICS: &[MetricDef] = &[
    C_IOPS, C_BYTES, C_LAT_MEAN, C_LAT_P50, C_LAT_P95, C_LAT_P99, C_LAT_P999, C_LAT_MAX,
];

fn emit_storage(ctx: &mut Ctx, entry: usize, st: &IoStats) {
    let secs = st.dur_ns as f64 / 1e9;
    if secs > 0.0 {
        ctx.scalar(entry, "iops", st.ops as f64 / secs);
        ctx.scalar(entry, "bytes_per_sec", st.ops as f64 * st.block as f64 / secs);
    }
    if st.has_latency {
        ctx.scalar(entry, "latency_mean_ns", st.mean_ns);
        ctx.scalar(entry, "latency_p50_ns", st.p50_ns);
        ctx.scalar(entry, "latency_p95_ns", st.p95_ns);
        ctx.scalar(entry, "latency_p99_ns", st.p99_ns);
        ctx.scalar(entry, "latency_p99_9_ns", st.p999_ns);
        ctx.scalar(entry, "latency_max_ns", st.max_ns);
    }
}

fn storage_target(ctx: &Ctx) -> u64 {
    if ctx.cfg.quick {
        600_000_000
    } else {
        1_500_000_000
    }
}

fn storage_runs(ctx: &Ctx) -> u32 {
    if ctx.cfg.quick {
        2
    } else {
        ctx.cfg.runs.clamp(1, 3)
    }
}

// ---------------------------------------------------------------------------
// Concurrency sweeps
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq)]
enum Engine {
    Streams,
    Uring,
}

fn add_target_params(ctx: &mut Ctx, entry: usize, target: usize) {
    let t = ctx.cfg.storage_targets[target];
    let (direct, fstype, size) = {
        let env = ctx.storage[target].as_ref().unwrap();
        (env.direct, env.fstype, env.size)
    };
    ctx.add_param(entry, "target", ParamValue::Str(t.name));
    ctx.add_param(entry, "path", ParamValue::Str(t.path));
    ctx.add_param(entry, "filesystem", ParamValue::Str(fstype));
    ctx.add_param(entry, "direct_io", ParamValue::Bool(direct));
    ctx.add_param(entry, "test_file_bytes", ParamValue::Int(size as i64));
}

fn run_uring_once(
    ctx: &mut Ctx,
    target: usize,
    op: u32,
    block: u64,
    qd: u32,
    target_ns: u64,
) -> Result<IoStats, &'static str> {
    uring::check_available()?;
    let env = ctx
        .storage
        .get_mut(target)
        .and_then(|e| e.as_mut())
        .ok_or("storage not initialised")?;
    if env.uring.is_none() {
        let r = uring::setup(128).map_err(|_| "io_uring_setup failed")?;
        env.uring = Some(r);
    }
    let ring = *env.uring.as_ref().unwrap();
    let (fd, size, buf_region) = (env.fd, env.size, env.buf_region);
    let buf = align_ptr(buf_region) as u64;
    let (rs, rl, ws, wl) = if op == uring::OP_MIXED {
        let ws = (size / 2) & !4095;
        (0u64, ws, ws, size - ws)
    } else {
        (0u64, size, 0u64, size)
    };
    {
        let s = uring::shared();
        s.ring = ring;
        s.fd = fd;
        s.op = op;
        s.check = 0;
        s.block = block as u32;
        s.qd = qd;
        s.quota = MAX_SAMPLES as u32;
        s.target_ns = target_ns;
        s.read_start = rs;
        s.read_len = rl;
        s.write_start = ws;
        s.write_len = wl;
        s.buf = buf;
        s.total_ops = 0;
        s.dur_ns = 0;
        s.errors = 0;
        s.sample_cnt = 0;
    }
    let rc = uring::invoke();
    if rc != 0 {
        return Err("io_uring benchmark kernel failed");
    }
    let st = uring::take_stats();
    if st.ops == 0 {
        return Err("io_uring: no operations completed");
    }
    Ok(st)
}

#[allow(clippy::too_many_arguments)]
fn run_sweep(
    ctx: &mut Ctx,
    entry: usize,
    target: usize,
    op: u32,
    levels: &[usize],
    engine: Engine,
    levels_str: &'static str,
) -> Result<(), &'static str> {
    ensure_storage(ctx, target)?;
    if !target_alive(ctx, target) {
        ctx.storage_state[target].status = "failed";
        return Err("storage target is no longer accessible");
    }
    add_target_params(ctx, entry, target);
    ctx.add_param(entry, "block_bytes", ParamValue::Int(BLOCK_4K as i64));
    ctx.add_param(
        entry,
        "concurrency_model",
        ParamValue::Str(match engine {
            Engine::Streams => "blocking-process-streams",
            Engine::Uring => "io-uring",
        }),
    );
    ctx.add_param(entry, "concurrency_levels", ParamValue::Str(levels_str));
    let nruns = if ctx.cfg.quick { 1 } else { 2 };
    let point_target = if ctx.cfg.quick {
        400_000_000
    } else {
        500_000_000
    };
    let np = levels.len().min(6);
    for _ in 0..nruns {
        let mut iops = [Point::default(); 6];
        let mut bps = [Point::default(); 6];
        let mut mean = [Point::default(); 6];
        let mut p50 = [Point::default(); 6];
        let mut p95 = [Point::default(); 6];
        let mut p99 = [Point::default(); 6];
        let mut p999 = [Point::default(); 6];
        let mut pmax = [Point::default(); 6];
        let mut total_ns = 0u64;
        for (i, &lvl) in levels.iter().take(np).enumerate() {
            let st = match engine {
                Engine::Streams => {
                    run_workers_once(ctx, target, op, BLOCK_4K, lvl, true, point_target)?
                }
                Engine::Uring => run_uring_once(ctx, target, op, BLOCK_4K, lvl as u32, point_target)?,
            };
            if st.dur_ns < point_target / 2 {
                ctx.storage_state[target].status = "failed";
                return Err("storage measurement window truncated (run ended early)");
            }
            total_ns = total_ns.saturating_add(st.dur_ns);
            let secs = st.dur_ns as f64 / 1e9;
            let x = lvl as f64;
            let y_iops = if secs > 0.0 { st.ops as f64 / secs } else { 0.0 };
            iops[i] = Point { x, y: y_iops };
            bps[i] = Point {
                x,
                y: y_iops * st.block as f64,
            };
            mean[i] = Point { x, y: st.mean_ns };
            p50[i] = Point { x, y: st.p50_ns };
            p95[i] = Point { x, y: st.p95_ns };
            p99[i] = Point { x, y: st.p99_ns };
            p999[i] = Point { x, y: st.p999_ns };
            pmax[i] = Point { x, y: st.max_ns };
        }
        ctx.begin_run(entry, total_ns, 0);
        ctx.curve(entry, "iops", &iops[..np]);
        ctx.curve(entry, "bytes_per_sec", &bps[..np]);
        ctx.curve(entry, "latency_mean_ns", &mean[..np]);
        ctx.curve(entry, "latency_p50_ns", &p50[..np]);
        ctx.curve(entry, "latency_p95_ns", &p95[..np]);
        ctx.curve(entry, "latency_p99_ns", &p99[..np]);
        ctx.curve(entry, "latency_p99_9_ns", &p999[..np]);
        ctx.curve(entry, "latency_max_ns", &pmax[..np]);
        ctx.end_run(entry);
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Fixed-concurrency benchmarks
// ---------------------------------------------------------------------------

fn run_storage(
    ctx: &mut Ctx,
    entry: usize,
    target: usize,
    op: u32,
    block: u64,
    threads: usize,
    sampling: bool,
) -> Result<(), &'static str> {
    ensure_storage(ctx, target)?;
    if !target_alive(ctx, target) {
        ctx.storage_state[target].status = "failed";
        return Err("storage target is no longer accessible");
    }
    add_target_params(ctx, entry, target);
    ctx.add_param(entry, "block_bytes", ParamValue::Int(block as i64));
    ctx.add_param(entry, "threads", ParamValue::Int(threads as i64));
    let target_ns = storage_target(ctx);
    for _ in 0..storage_runs(ctx) {
        let st = run_workers_once(ctx, target, op, block, threads, sampling, target_ns)?;
        if sampling && st.dur_ns < target_ns / 2 {
            return Err("storage measurement window truncated (run ended early)");
        }
        ctx.begin_run(entry, st.dur_ns, st.ops);
        emit_storage(ctx, entry, &st);
        ctx.end_run(entry);
    }
    Ok(())
}

macro_rules! storage_single {
    ($struct_name:ident, $id:literal, $desc:literal, $algo:literal, $op:expr, $block:expr, $threads:expr, $sampling:expr) => {
        pub struct $struct_name;
        impl Benchmark for $struct_name {
            fn meta(&self) -> &'static EntryMeta {
                static M: EntryMeta = EntryMeta {
                    id: $id,
                    version: 1,
                    description: $desc,
                    imp: ImplInfo {
                        source: "src/bench/storage.rs",
                        kernel: "vmbench_w_storage",
                        algorithm: $algo,
                        isa: BASELINE_ISA,
                    },
                    metrics: if $sampling { LAT_METRICS } else { SEQ_METRICS },
                };
                &M
            }
            fn instances(&self, ctx: &Ctx) -> usize {
                ctx.cfg.n_storage_targets
            }
            fn run_at(&self, ctx: &mut Ctx, entry: usize, instance: usize) -> Result<(), &'static str> {
                run_storage(ctx, entry, instance, $op, $block, $threads, $sampling)
            }
            fn run(&self, _ctx: &mut Ctx, _entry: usize) -> Result<(), &'static str> {
                Err("storage benchmark requires a target instance")
            }
        }
    };
}

macro_rules! sweep_bench {
    ($struct_name:ident, $id:literal, $desc:literal, $algo:literal, $kernel:literal, $engine:expr, $op:expr, $levels:expr, $levels_str:expr) => {
        pub struct $struct_name;
        impl Benchmark for $struct_name {
            fn meta(&self) -> &'static EntryMeta {
                static M: EntryMeta = EntryMeta {
                    id: $id,
                    version: 1,
                    description: $desc,
                    imp: ImplInfo {
                        source: "src/bench/storage.rs",
                        kernel: $kernel,
                        algorithm: $algo,
                        isa: BASELINE_ISA,
                    },
                    metrics: SWEEP_METRICS,
                };
                &M
            }
            fn instances(&self, ctx: &Ctx) -> usize {
                ctx.cfg.n_storage_targets
            }
            fn supported(&self, _ctx: &Ctx) -> Result<(), &'static str> {
                if $engine == Engine::Uring {
                    uring::check_available()
                } else {
                    Ok(())
                }
            }
            fn run_at(&self, ctx: &mut Ctx, entry: usize, instance: usize) -> Result<(), &'static str> {
                run_sweep(ctx, entry, instance, $op, &$levels, $engine, $levels_str)
            }
            fn run(&self, _ctx: &mut Ctx, _entry: usize) -> Result<(), &'static str> {
                Err("storage benchmark requires a target instance")
            }
        }
    };
}

storage_single!(
    StorageSeqRead,
    "storage.seq.read.v1",
    "Sequential read (1 MiB, QD1)",
    "pread of 1 MiB blocks advancing through the test file",
    OP_SEQ_READ,
    BLOCK_1M,
    1,
    false
);
storage_single!(
    StorageSeqWrite,
    "storage.seq.write.v1",
    "Sequential write (1 MiB, QD1)",
    "pwrite of 1 MiB blocks advancing through the test file",
    OP_SEQ_WRITE,
    BLOCK_1M,
    1,
    false
);
storage_single!(
    StorageSyncFdatasync,
    "storage.sync.fdatasync.4k.v1",
    "Synchronous 4 KiB write + fdatasync (one stream)",
    "pwrite of a random 4 KiB block followed by fdatasync per operation",
    OP_SYNC_FDATASYNC,
    BLOCK_4K,
    1,
    true
);
storage_single!(
    StorageSyncDsync,
    "storage.sync.dsync.4k.v1",
    "Synchronous 4 KiB O_DSYNC write (one stream)",
    "pwrite on an O_DSYNC descriptor (durability cost per operation)",
    OP_SYNC_DSYNC,
    BLOCK_4K,
    1,
    true
);

sweep_bench!(
    StorageStreamsRead,
    "storage.streams.read.4k.sweep.v1",
    "Random 4 KiB read through N parallel blocking streams",
    "N processes issue blocking pread in a loop; each op is a random 4 KiB block",
    "vmbench_w_storage",
    Engine::Streams,
    OP_RAND_READ,
    STREAMS_LEVELS,
    STREAMS_LEVELS_STR
);
sweep_bench!(
    StorageStreamsWrite,
    "storage.streams.write.4k.sweep.v1",
    "Random 4 KiB write through N parallel blocking streams",
    "N processes issue blocking pwrite in a loop; each op is a random 4 KiB block",
    "vmbench_w_storage",
    Engine::Streams,
    OP_RAND_WRITE,
    STREAMS_LEVELS,
    STREAMS_LEVELS_STR
);
sweep_bench!(
    StorageStreamsMixed,
    "storage.streams.mixed70_30.4k.sweep.v1",
    "Mixed 70% read / 30% write through N parallel blocking streams",
    "N processes choose 70% pread / 30% pwrite per operation on disjoint regions",
    "vmbench_w_storage",
    Engine::Streams,
    OP_MIXED,
    STREAMS_LEVELS,
    STREAMS_LEVELS_STR
);
sweep_bench!(
    StorageUringRead,
    "storage.uring.read.4k.sweep.v1",
    "Random 4 KiB read through io_uring at queue depth QD",
    "single process keeps QD asynchronous 4 KiB reads in flight via io_uring",
    "vmbench_k_storage_uring_run",
    Engine::Uring,
    uring::OP_READ,
    URING_LEVELS,
    URING_LEVELS_STR
);
sweep_bench!(
    StorageUringWrite,
    "storage.uring.write.4k.sweep.v1",
    "Random 4 KiB write through io_uring at queue depth QD",
    "single process keeps QD asynchronous 4 KiB writes in flight via io_uring",
    "vmbench_k_storage_uring_run",
    Engine::Uring,
    uring::OP_WRITE,
    URING_LEVELS,
    URING_LEVELS_STR
);
sweep_bench!(
    StorageUringMixed,
    "storage.uring.mixed70_30.4k.sweep.v1",
    "Mixed 70% read / 30% write through io_uring at queue depth QD",
    "single process keeps QD asynchronous ops in flight, choosing 70% read / 30% write",
    "vmbench_k_storage_uring_run",
    Engine::Uring,
    uring::OP_MIXED,
    URING_LEVELS,
    URING_LEVELS_STR
);

// ---------------------------------------------------------------------------
// Kernels and standalone benchmarks
// ---------------------------------------------------------------------------

pub const M_SUST_BYTES: MetricDef = MetricDef {
    key: "bytes_per_sec",
    unit: "bytes/s",
    direction: Direction::HigherBetter,
    kind: MetricKind::Curve,
};
pub const M_SUST_MIN: MetricDef = MetricDef {
    key: "bytes_per_sec_min",
    unit: "bytes/s",
    direction: Direction::HigherBetter,
    kind: MetricKind::Scalar,
};
pub const M_SUST_MED: MetricDef = MetricDef {
    key: "bytes_per_sec_median",
    unit: "bytes/s",
    direction: Direction::HigherBetter,
    kind: MetricKind::Scalar,
};
pub const M_SUST_MAX: MetricDef = MetricDef {
    key: "bytes_per_sec_max",
    unit: "bytes/s",
    direction: Direction::HigherBetter,
    kind: MetricKind::Scalar,
};

#[inline(never)]
#[no_mangle]
pub unsafe extern "C" fn vmbench_k_storage_write_block(
    fd: i32,
    off: u64,
    size: u64,
    buf: usize,
    len: u64,
) -> u64 {
    let src = core::slice::from_raw_parts(buf as *const u8, len as usize);
    match sys::pwrite(fd, src, off % size) {
        Ok(k) => k as u64,
        Err(_) => 0,
    }
}

#[inline(never)]
#[no_mangle]
pub unsafe extern "C" fn vmbench_k_storage_read_region(
    fd: i32,
    off: u64,
    len: u64,
    buf: usize,
    buf_len: u64,
) -> u64 {
    let dst = core::slice::from_raw_parts_mut(buf as *mut u8, buf_len as usize);
    let mut total = 0u64;
    while total < len {
        let n = ((len - total).min(buf_len)) as usize;
        match sys::pread(fd, &mut dst[..n], off + total) {
            Ok(0) => break,
            Ok(k) => total += k as u64,
            Err(_) => break,
        }
    }
    total
}

pub struct StorageSustainedWrite;

impl Benchmark for StorageSustainedWrite {
    fn meta(&self) -> &'static EntryMeta {
        static M: EntryMeta = EntryMeta {
            id: "storage.sustained.write.v1",
            version: 1,
            description: "Sustained sequential write over time (burst/GC behaviour)",
            imp: ImplInfo {
                source: "src/bench/storage.rs",
                kernel: "vmbench_k_storage_write_block",
                algorithm: "1 MiB direct writes for the sustained duration; per-second throughput buckets",
                isa: BASELINE_ISA,
            },
            metrics: &[M_SUST_BYTES, M_SUST_MIN, M_SUST_MED, M_SUST_MAX],
        };
        &M
    }

    fn quick_skip(&self) -> bool {
        true
    }

    fn instances(&self, ctx: &Ctx) -> usize {
        ctx.cfg.n_storage_targets
    }

    fn run_at(&self, ctx: &mut Ctx, entry: usize, instance: usize) -> Result<(), &'static str> {
        ensure_storage(ctx, instance)?;
        if !target_alive(ctx, instance) {
            ctx.storage_state[instance].status = "failed";
            return Err("storage target is no longer accessible");
        }
        let (fd, size, buf, direct, fstype) = {
            let env = ctx.storage[instance].as_ref().unwrap();
            (env.fd, env.size, env.buf_region, env.direct, env.fstype)
        };
        let t = ctx.cfg.storage_targets[instance];
        let aligned = align_ptr(buf) as *mut u8;
        ctx.add_param(entry, "target", ParamValue::Str(t.name));
        ctx.add_param(entry, "path", ParamValue::Str(t.path));
        ctx.add_param(entry, "block_bytes", ParamValue::Int(BLOCK_1M as i64));
        ctx.add_param(entry, "direct_io", ParamValue::Bool(direct));
        ctx.add_param(entry, "filesystem", ParamValue::Str(fstype));
        let total_ns = ctx.cfg.sustained_ns.max(1_000_000_000);
        ctx.annotate("storage.sustained: provider burst credits, GC and throttling can dominate this result");
        let mut points = [Point { x: 0.0, y: 0.0 }; 64];
        let mut npoints = 0usize;
        let mut min_rate = f64::MAX;
        let mut max_rate = 0f64;
        let mut rates = [0f64; 64];
        let start = time::mono_ns();
        let mut off = 0u64;
        while time::mono_ns().wrapping_sub(start) < total_ns && npoints < 64 {
            let b0 = time::mono_ns();
            let mut bytes = 0u64;
            while time::mono_ns().wrapping_sub(b0) < 1_000_000_000 {
                let w = unsafe {
                    vmbench_k_storage_write_block(fd, off, size, aligned as usize, BLOCK_1M)
                };
                if w == 0 {
                    break;
                }
                off = (off + BLOCK_1M) % size;
                bytes += w;
                if time::mono_ns().wrapping_sub(start) >= total_ns {
                    break;
                }
            }
            let d = time::mono_ns().wrapping_sub(b0).max(1);
            let rate = bytes as f64 / (d as f64 / 1e9);
            points[npoints] = Point {
                x: time::mono_ns().wrapping_sub(start) as f64 / 1e9,
                y: rate,
            };
            npoints += 1;
            rates[npoints - 1] = rate;
            if rate < min_rate {
                min_rate = rate;
            }
            if rate > max_rate {
                max_rate = rate;
            }
        }
        let elapsed = time::mono_ns().wrapping_sub(start).max(1);
        ctx.begin_run(entry, elapsed, 0);
        ctx.curve(entry, "bytes_per_sec", &points[..npoints]);
        let mut sorted = rates;
        sorted[..npoints].sort_unstable_by(|a, b| a.partial_cmp(b).unwrap_or(core::cmp::Ordering::Equal));
        let med = if npoints == 0 {
            0.0
        } else if npoints % 2 == 1 {
            sorted[npoints / 2]
        } else {
            (sorted[npoints / 2 - 1] + sorted[npoints / 2]) / 2.0
        };
        ctx.scalar(entry, "bytes_per_sec_min", if npoints == 0 { 0.0 } else { min_rate });
        ctx.scalar(entry, "bytes_per_sec_median", med);
        ctx.scalar(entry, "bytes_per_sec_max", max_rate);
        ctx.end_run(entry);
        Ok(())
    }

    fn run(&self, _ctx: &mut Ctx, _entry: usize) -> Result<(), &'static str> {
        Err("storage benchmark requires a target instance")
    }
}

pub struct StorageBufferedWarm;

impl Benchmark for StorageBufferedWarm {
    fn meta(&self) -> &'static EntryMeta {
        static M: EntryMeta = EntryMeta {
            id: "storage.buffered.read.warm.v1",
            version: 1,
            description: "Buffered sequential read after warmup (page cache, reference only)",
            imp: ImplInfo {
                source: "src/bench/storage.rs",
                kernel: "vmbench_k_storage_read_region",
                algorithm: "two buffered passes over the test region; the second pass is measured",
                isa: BASELINE_ISA,
            },
            metrics: &[M_BYTES],
        };
        &M
    }

    fn instances(&self, ctx: &Ctx) -> usize {
        ctx.cfg.n_storage_targets
    }

    fn run_at(&self, ctx: &mut Ctx, entry: usize, instance: usize) -> Result<(), &'static str> {
        ensure_storage(ctx, instance)?;
        if !target_alive(ctx, instance) {
            ctx.storage_state[instance].status = "failed";
            return Err("storage target is no longer accessible");
        }
        let (fd, size, buf, fstype) = {
            let env = ctx.storage[instance].as_ref().unwrap();
            (env.buffered_fd, env.size, env.buf_region, env.fstype)
        };
        if fd < 0 {
            return Err("buffered descriptor unavailable");
        }
        let t = ctx.cfg.storage_targets[instance];
        let len = (size.min(512 << 20)) & !4095;
        let aligned = align_ptr(buf);
        ctx.add_param(entry, "target", ParamValue::Str(t.name));
        ctx.add_param(entry, "path", ParamValue::Str(t.path));
        ctx.add_param(entry, "region_bytes", ParamValue::Int(len as i64));
        ctx.add_param(entry, "filesystem", ParamValue::Str(fstype));
        ctx.annotate("storage.buffered: guest page cache is not controlled; provider cache unknown; reference value only");
        for pass in 0..2 {
            let t0 = time::mono_ns();
            let got = unsafe {
                vmbench_k_storage_read_region(fd, 0, len, aligned as usize, BLOCK_1M)
            };
            let d = time::mono_ns().wrapping_sub(t0).max(1);
            if pass == 1 {
                ctx.begin_run(entry, d, got);
                let secs = d as f64 / 1e9;
                if secs > 0.0 {
                    ctx.scalar(entry, "bytes_per_sec", got as f64 / secs);
                }
                ctx.end_run(entry);
            }
        }
        Ok(())
    }

    fn run(&self, _ctx: &mut Ctx, _entry: usize) -> Result<(), &'static str> {
        Err("storage benchmark requires a target instance")
    }
}
