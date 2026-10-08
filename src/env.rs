use crate::arch;
use crate::sha256::{self, Sha256};
use crate::sys;
use crate::time;
use crate::util::{self, Arena};

pub const MAX_CPUS: usize = 256;
pub const MAX_CACHES: usize = 1024;
pub const MAX_NODES: usize = 64;
pub const MAX_MOUNTS: usize = 64;
pub const MAX_BLOCKS: usize = 64;
pub const MAX_FLAGS: usize = 256;

#[derive(Clone, Copy, Default)]
pub struct CpuTopo {
    pub cpu: u32,
    pub core_id: Option<u64>,
    pub package_id: Option<u64>,
    pub die_id: Option<u64>,
    pub cluster_id: Option<u64>,
    pub thread_siblings: Option<&'static str>,
    pub core_siblings: Option<&'static str>,
}

#[derive(Clone, Copy, Default)]
pub struct CacheInfo {
    pub cpu: u32,
    pub level: u32,
    pub kind: &'static str,
    pub size_bytes: u64,
    pub line_size: u64,
    pub ways: u64,
    pub shared_cpu_list: &'static str,
}

#[derive(Clone, Copy, Default)]
pub struct NumaNode {
    pub id: u32,
    pub mem_total_kib: Option<u64>,
    pub cpus: &'static str,
}

#[derive(Clone, Copy, Default)]
pub struct MountInfo {
    pub source: &'static str,
    pub mount_point: &'static str,
    pub fstype: &'static str,
    pub options: &'static str,
}

#[derive(Clone, Copy, Default)]
pub struct BlockDevInfo {
    pub name: &'static str,
    pub size_sectors: Option<u64>,
    pub rotational: Option<bool>,
    pub logical_block_size: Option<u64>,
    pub physical_block_size: Option<u64>,
    pub nr_requests: Option<u64>,
    pub scheduler: Option<&'static str>,
    pub model: Option<&'static str>,
}

#[derive(Clone, Copy, Default)]
pub struct Pressure {
    pub avg10: f64,
    pub avg60: f64,
    pub avg300: f64,
    pub total_us: u64,
}

#[derive(Clone, Copy, Default)]
pub struct CgroupInfo {
    pub path: Option<&'static str>,
    pub cpu_max: Option<&'static str>,
    pub cpu_weight: Option<&'static str>,
    pub cpuset_cpus: Option<&'static str>,
    pub memory_max: Option<&'static str>,
    pub memory_current: Option<u64>,
}

#[derive(Clone, Copy, Default)]
pub struct DmiInfo {
    pub sys_vendor: Option<&'static str>,
    pub product_name: Option<&'static str>,
    pub product_uuid_hash: Option<[u8; 32]>,
    pub board_vendor: Option<&'static str>,
    pub bios_version: Option<&'static str>,
}

#[derive(Clone, Copy, Default)]
pub struct FreqInfo {
    pub cur_khz: Option<u64>,
    pub max_khz: Option<u64>,
    pub min_khz: Option<u64>,
    pub cpuid_base_mhz: Option<u64>,
    pub cpuid_max_mhz: Option<u64>,
    pub boost: Option<bool>,
}

#[derive(Clone, Copy, Default)]
pub struct CpuIds {
    pub vendor: Option<&'static str>,
    pub brand: Option<&'static str>,
    pub family: Option<u64>,
    pub model: Option<u64>,
    pub stepping: Option<u64>,
    pub microcode: Option<&'static str>,
    pub hypervisor: Option<&'static str>,
}

#[derive(Clone, Copy, Default)]
pub struct MemInfo {
    pub total_kib: Option<u64>,
    pub free_kib: Option<u64>,
    pub available_kib: Option<u64>,
    pub buffers_kib: Option<u64>,
    pub cached_kib: Option<u64>,
    pub swap_total_kib: Option<u64>,
    pub swap_free_kib: Option<u64>,
    pub dirty_kib: Option<u64>,
    pub writeback_kib: Option<u64>,
    pub anon_hugepages_kib: Option<u64>,
    pub hugepages_total: Option<u64>,
    pub hugepages_free: Option<u64>,
    pub hugepage_size_kib: Option<u64>,
}

pub struct EnvInfo {
    pub kernel_version: Option<&'static str>,
    pub kernel_release: Option<&'static str>,
    pub boot_id: Option<&'static str>,
    pub hostname: Option<&'static str>,
    pub distro: Option<&'static str>,
    pub uptime_s: Option<f64>,
    pub page_size: u64,
    pub clktck: u64,
    pub thp_enabled: Option<&'static str>,
    pub thp_defrag: Option<&'static str>,
    pub clock_source: Option<&'static str>,
    pub cgroup: CgroupInfo,
    pub dmi: DmiInfo,

    pub cpu_ids: CpuIds,
    pub online_cpus: &'static [u32],
    pub topology: &'static [CpuTopo],
    pub caches: &'static [CacheInfo],
    pub numa_nodes: &'static [NumaNode],
    pub cpu_flags: &'static [&'static str],
    pub isa_summary: &'static [(&'static str, bool)],
    pub freq: FreqInfo,
    pub cycles_hz: Option<u64>,
    pub cycles_source: &'static str,
    pub invariant_tsc: Option<bool>,

    pub mem: MemInfo,

    pub mounts: &'static [MountInfo],
    pub block_devices: &'static [BlockDevInfo],
    pub test_dir: Option<&'static str>,

    pub loadavg: [f64; 3],
    pub procs_running: Option<u64>,
    pub procs_blocked: Option<u64>,
    pub ctxt_per_sec: Option<f64>,
    pub interrupts_per_sec: Option<f64>,
    pub steal_ticks_total: Option<u64>,
    pub pressure_cpu: Option<Pressure>,
    pub pressure_mem: Option<Pressure>,
    pub pressure_io: Option<Pressure>,

    pub machine_id_hash: Option<[u8; 32]>,
    pub hostname_hash: Option<[u8; 32]>,
    pub instance_id: Option<[u8; 32]>,
    pub cpu_signature_hash: Option<[u8; 32]>,
    pub topology_hash: Option<[u8; 32]>,
    pub cache_hash: Option<[u8; 32]>,
    pub memory_hash: Option<[u8; 32]>,
    pub hypervisor_hash: Option<[u8; 32]>,
    pub environment_id: Option<[u8; 32]>,
    pub run_id: [u8; 16],
}

struct Scratch {
    a: &'static mut [u8],
    b: &'static mut [u8],
}

fn read_all(buf: &mut [u8], path: &[u8]) -> Option<usize> {
    sys::read_file(path, buf).ok()
}

fn trim_slice(buf: &[u8], n: usize) -> &[u8] {
    util::trim_ascii(&buf[..n])
}

fn copy_text(arena: &mut Arena, bytes: &[u8]) -> Option<&'static str> {
    if bytes.is_empty() {
        return None;
    }
    unsafe { arena.copy_str(bytes) }
}

fn read_text(
    arena: &mut Arena,
    buf: &mut [u8],
    path: &[u8],
) -> Option<&'static str> {
    let n = read_all(buf, path)?;
    let t = trim_slice(buf, n);
    copy_text(arena, t)
}

fn find_line<'a>(buf: &'a [u8], key: &[u8]) -> Option<&'a [u8]> {
    for line in buf.split(|&c| c == b'\n') {
        let line = util::trim_ascii(line);
        if line.starts_with(key) {
            let mut rest = &line[key.len()..];
            rest = util::trim_ascii(rest);
            if rest.first() == Some(&b':') || rest.first() == Some(&b'=') {
                rest = util::trim_ascii(&rest[1..]);
            }
            return Some(rest);
        }
    }
    None
}

fn find_line_u64(buf: &[u8], key: &[u8]) -> Option<u64> {
    let v = find_line(buf, key)?;
    let tok = v.split(|&c| c == b' ' || c == b'\t').next()?;
    util::parse_u64(util::trim_ascii(tok))
}

fn cpu_dir_name<'a>(cpu: u32, buf: &'a mut [u8; 16]) -> &'a [u8] {
    buf[..3].copy_from_slice(b"cpu");
    let n = crate::fmt::u64_dec(cpu as u64, &mut buf[3..]);
    &buf[..3 + n]
}

fn node_dir_name<'a>(node: u32, buf: &'a mut [u8; 16]) -> &'a [u8] {
    buf[..4].copy_from_slice(b"node");
    let n = crate::fmt::u64_dec(node as u64, &mut buf[4..]);
    &buf[..4 + n]
}

fn parse_cpu_list(s: &[u8], out: &mut [u32]) -> usize {
    let mut n = 0;
    for part in s.split(|&c| c == b',') {
        let part = util::trim_ascii(part);
        if part.is_empty() {
            continue;
        }
        if let Some(dash) = part.iter().position(|&c| c == b'-') {
            let a = util::parse_u64(&part[..dash]);
            let b = util::parse_u64(&part[dash + 1..]);
            if let (Some(a), Some(b)) = (a, b) {
                let mut v = a;
                while v <= b && n < out.len() {
                    out[n] = v as u32;
                    n += 1;
                    v += 1;
                }
            }
        } else if let Some(v) = util::parse_u64(part) {
            if n < out.len() {
                out[n] = v as u32;
                n += 1;
            }
        }
    }
    n
}

fn for_each_dir(
    buf: &mut [u8],
    path: &[u8],
    mut f: impl FnMut(&[u8]),
) -> Option<()> {
    let fd = sys::open(path, sys::O_RDONLY | sys::O_DIRECTORY | sys::O_CLOEXEC, 0).ok()?;
    let mut result = Some(());
    loop {
        let n = match sys::getdents64(fd, buf) {
            Ok(0) => break,
            Ok(n) => n,
            Err(_) => {
                result = None;
                break;
            }
        };
        let mut off = 0;
        while off < n {
            let d = unsafe { &*(buf.as_ptr().add(off) as *const sys::LinuxDirent64) };
            let reclen = d.d_reclen as usize;
            if reclen == 0 || off + reclen > n {
                break;
            }
            let name_ptr = unsafe { buf.as_ptr().add(off + 19) };
            let name_len = reclen - 19;
            let raw = unsafe { core::slice::from_raw_parts(name_ptr, name_len) };
            let name = match raw.iter().position(|&c| c == 0) {
                Some(z) => &raw[..z],
                None => raw,
            };
            if name != b"." && name != b".." && !name.is_empty() {
                f(name);
            }
            off += reclen;
        }
    }
    let _ = sys::close(fd);
    result
}

fn join_path(out: &mut [u8], parts: &[&[u8]]) -> Option<usize> {
    let mut len = 0;
    for (i, p) in parts.iter().enumerate() {
        if i > 0 {
            if len >= out.len() {
                return None;
            }
            out[len] = b'/';
            len += 1;
        }
        if len + p.len() > out.len() {
            return None;
        }
        out[len..len + p.len()].copy_from_slice(p);
        len += p.len();
    }
    Some(len)
}

fn join_str(arena: &mut Arena, parts: &[&[u8]]) -> Option<&'static str> {
    let mut tmp = [0u8; 512];
    let n = join_path(&mut tmp, parts)?;
    unsafe { arena.copy_str(&tmp[..n]) }
}

fn hash_opt(parts: &[Option<[u8; 32]>], domain: &[u8]) -> Option<[u8; 32]> {
    let mut h = Sha256::new();
    h.update(domain);
    let mut any = false;
    for (i, p) in parts.iter().enumerate() {
        if let Some(x) = p {
            h.update(&[i as u8]);
            h.update(x);
            any = true;
        }
    }
    if any {
        Some(h.finish())
    } else {
        None
    }
}

fn parse_pressure(buf: &[u8]) -> Option<Pressure> {
    let line = buf.split(|&c| c == b'\n').next()?;
    let mut p = Pressure::default();
    for tok in line.split(|&c| c == b' ') {
        let tok = util::trim_ascii(tok);
        if let Some(v) = tok.strip_prefix(b"avg10=") {
            p.avg10 = parse_f64(v);
        } else if let Some(v) = tok.strip_prefix(b"avg60=") {
            p.avg60 = parse_f64(v);
        } else if let Some(v) = tok.strip_prefix(b"avg300=") {
            p.avg300 = parse_f64(v);
        } else if let Some(v) = tok.strip_prefix(b"total=") {
            p.total_us = util::parse_u64(v).unwrap_or(0);
        }
    }
    Some(p)
}

fn parse_f64(b: &[u8]) -> f64 {
    let s = util::bytes_to_str(util::trim_ascii(b));
    let mut v = 0.0f64;
    let mut frac = 0.0f64;
    let mut div = 1.0f64;
    let mut seen_dot = false;
    let mut neg = false;
    for (i, c) in s.bytes().enumerate() {
        match c {
            b'-' if i == 0 => neg = true,
            b'.' => seen_dot = true,
            b'0'..=b'9' => {
                if seen_dot {
                    div *= 10.0;
                    frac += (c - b'0') as f64 / div;
                } else {
                    v = v * 10.0 + (c - b'0') as f64;
                }
            }
            _ => break,
        }
    }
    let r = v + frac;
    if neg {
        -r
    } else {
        r
    }
}

#[cfg(target_arch = "x86_64")]
fn hypervisor_name(cpuid: &arch::CpuidIds) -> Option<&'static str> {
    if !cpuid.hypervisor {
        return None;
    }
    let v = &cpuid.hv_vendor[..cpuid.hv_vendor_len];
    if v.starts_with(b"KVM") {
        Some("KVM")
    } else if v.starts_with(b"Microsoft") {
        Some("Microsoft Hyper-V")
    } else if v.starts_with(b"VMware") {
        Some("VMware")
    } else if v.starts_with(b"Xen") {
        Some("Xen")
    } else if v.starts_with(b"bhyve") {
        Some("bhyve")
    } else if v.starts_with(b"TCG") {
        Some("QEMU TCG")
    } else {
        Some("unknown")
    }
}

fn hypervisor_from_product_name(name: &str) -> Option<&'static str> {
    let n = name.as_bytes();
    if n.starts_with(b"KVM") {
        Some("KVM")
    } else if n.starts_with(b"Virtual Machine") || n.starts_with(b"VirtualBox") {
        Some("Microsoft Hyper-V")
    } else if n.starts_with(b"VMware") {
        Some("VMware")
    } else if n.starts_with(b"Standard PC") || n.starts_with(b"QEMU") {
        Some("QEMU")
    } else if n.starts_with(b"Xen") {
        Some("Xen")
    } else {
        None
    }
}

fn read_text_at(
    arena: &mut Arena,
    scratch: &mut [u8],
    base: &[u8],
    suffix: &[u8],
) -> Option<&'static str> {
    let mut path = [0u8; 512];
    let n = join_path(&mut path, &[base, suffix])?;
    read_text(arena, scratch, &path[..n])
}

fn read_u64_at(base: &[u8], suffix: &[u8]) -> Option<u64> {
    let mut path = [0u8; 512];
    let n = join_path(&mut path, &[base, suffix])?;
    read_u64_file(&path[..n])
}

pub fn collect(arena: &mut Arena) -> EnvInfo {
    let s1 = unsafe { arena.alloc_bytes(512 * 1024, 16) }.expect("arena");
    let s2 = unsafe { arena.alloc_bytes(512 * 1024, 16) }.expect("arena");
    let sc = Scratch { a: s1, b: s2 };
    let page_size = crate::rt::page_size() as u64;
    let clktck = unsafe {
        let a = &*core::ptr::addr_of!(crate::rt::AUXV);
        a.clktck as u64
    };
    let clktck = if clktck == 0 { 100 } else { clktck };

    let mut env = EnvInfo {
        kernel_version: None,
        kernel_release: None,
        boot_id: None,
        hostname: None,
        distro: None,
        uptime_s: None,
        page_size,
        clktck,
        thp_enabled: None,
        thp_defrag: None,
        clock_source: None,
        cgroup: CgroupInfo::default(),
        dmi: DmiInfo::default(),
        cpu_ids: CpuIds::default(),
        online_cpus: &[],
        topology: &[],
        caches: &[],
        numa_nodes: &[],
        cpu_flags: &[],
        isa_summary: &[],
        freq: FreqInfo::default(),
        cycles_hz: None,
        cycles_source: time::cycles_hz_source(),
        invariant_tsc: None,
        mem: MemInfo::default(),
        mounts: &[],
        block_devices: &[],
        test_dir: None,
        loadavg: [0.0; 3],
        procs_running: None,
        procs_blocked: None,
        ctxt_per_sec: None,
        interrupts_per_sec: None,
        steal_ticks_total: None,
        pressure_cpu: None,
        pressure_mem: None,
        pressure_io: None,
        machine_id_hash: None,
        hostname_hash: None,
        instance_id: None,
        cpu_signature_hash: None,
        topology_hash: None,
        cache_hash: None,
        memory_hash: None,
        hypervisor_hash: None,
        environment_id: None,
        run_id: [0u8; 16],
    };

    if let Some(v) = read_text(arena, sc.a, b"/proc/version") {
        env.kernel_version = Some(v);
    }
    if let Some(v) = read_text(arena, sc.a, b"/proc/sys/kernel/osrelease") {
        env.kernel_release = Some(v);
    }
    if let Some(v) = read_text(arena, sc.a, b"/proc/sys/kernel/random/boot_id") {
        env.boot_id = Some(v);
    }
    if let Some(v) = read_text(arena, sc.a, b"/proc/sys/kernel/hostname") {
        env.hostname = Some(v);
        env.hostname_hash = Some(sha256::hash(v.as_bytes()));
    }
    // distro
    if let Ok(n) = sys::read_file(b"/etc/os-release", sc.a) {
        if let Some(v) = find_line(&sc.a[..n], b"PRETTY_NAME") {
            let v = v.strip_prefix(b"\"").unwrap_or(v);
            let v = v.strip_suffix(b"\"").unwrap_or(v);
            env.distro = copy_text(arena, v);
        }
    }
    if env.distro.is_none() {
        if let Ok(n) = sys::read_file(b"/etc/lsb-release", sc.a) {
            if let Some(v) = find_line(&sc.a[..n], b"DISTRIB_DESCRIPTION") {
                let v = v.strip_prefix(b"\"").unwrap_or(v);
                let v = v.strip_suffix(b"\"").unwrap_or(v);
                env.distro = copy_text(arena, v);
            }
        }
    }
    // machine-id
    for p in [&b"/etc/machine-id"[..], b"/var/lib/dbus/machine-id"] {
        if let Ok(n) = sys::read_file(p, sc.a) {
            let t = util::trim_ascii(&sc.a[..n]);
            if !t.is_empty() {
                env.machine_id_hash = Some(sha256::hash(t));
                break;
            }
        }
    }
    // uptime
    if let Ok(n) = sys::read_file(b"/proc/uptime", sc.a) {
        let first = sc.a[..n].split(|&c| c == b' ').next().unwrap_or(&[]);
        env.uptime_s = Some(parse_f64(first));
    }
    // DMI
    let dmi_dir = b"/sys/class/dmi/id";
    env.dmi.sys_vendor = read_text_at(arena, sc.a, dmi_dir, b"sys_vendor");
    env.dmi.product_name = read_text_at(arena, sc.a, dmi_dir, b"product_name");
    env.dmi.board_vendor = read_text_at(arena, sc.a, dmi_dir, b"board_vendor");
    env.dmi.bios_version = read_text_at(arena, sc.a, dmi_dir, b"bios_version");
    {
        let mut p = [0u8; 512];
        if let Some(n) = join_path(&mut p, &[dmi_dir, b"product_uuid"]) {
            if let Ok(nr) = sys::read_file(&p[..n], sc.a) {
                let t = util::trim_ascii(&sc.a[..nr]);
                if !t.is_empty() && !util::eq_ignore_case(t, "none") {
                    env.dmi.product_uuid_hash = Some(sha256::hash(t));
                }
            }
        }
    }

    // hypervisor fallback for environments without CPUID hypervisor leaf
    if env.cpu_ids.hypervisor.is_none() {
        if let Some(v) = read_text(arena, sc.a, b"/sys/hypervisor/type") {
            env.cpu_ids.hypervisor = Some(match v {
                "xen" => "Xen",
                "kvm" => "KVM",
                "hv" => "Microsoft Hyper-V",
                other => other,
            });
        }
    }
    if env.cpu_ids.hypervisor.is_none() {
        if let Some(p) = env.dmi.product_name {
            env.cpu_ids.hypervisor = hypervisor_from_product_name(p);
        }
    }

    // THP / clock source / cgroup / pressure / stat / meminfo
    env.thp_enabled = read_text(
        arena,
        sc.a,
        b"/sys/kernel/mm/transparent_hugepage/enabled",
    );
    env.thp_defrag = read_text(
        arena,
        sc.a,
        b"/sys/kernel/mm/transparent_hugepage/defrag",
    );
    env.clock_source = read_text(
        arena,
        sc.a,
        b"/sys/devices/system/clocksource/clocksource0/current_clocksource",
    );

    // memory info
    if let Ok(n) = sys::read_file(b"/proc/meminfo", sc.a) {
        let m = &sc.a[..n];
        let g = |k: &[u8]| find_line_u64(m, k);
        env.mem.total_kib = g(b"MemTotal");
        env.mem.free_kib = g(b"MemFree");
        env.mem.available_kib = g(b"MemAvailable");
        env.mem.buffers_kib = g(b"Buffers");
        env.mem.cached_kib = g(b"Cached");
        env.mem.swap_total_kib = g(b"SwapTotal");
        env.mem.swap_free_kib = g(b"SwapFree");
        env.mem.dirty_kib = g(b"Dirty");
        env.mem.writeback_kib = g(b"Writeback");
        env.mem.anon_hugepages_kib = g(b"AnonHugePages");
        env.mem.hugepages_total = g(b"HugePages_Total");
        env.mem.hugepages_free = g(b"HugePages_Free");
        env.mem.hugepage_size_kib = g(b"Hugepagesize");
        env.memory_hash = env.mem.total_kib.map(|t| sha256::hash(&t.to_le_bytes()));
    }

    // loadavg
    if let Ok(n) = sys::read_file(b"/proc/loadavg", sc.a) {
        let line = &sc.a[..n];
        let mut it = line.split(|&c| c == b' ');
        for i in 0..3 {
            env.loadavg[i] = it.next().map(parse_f64).unwrap_or(0.0);
        }
    }
    // /proc/stat
    if let Ok(n) = sys::read_file(b"/proc/stat", sc.a) {
        let m = &sc.a[..n];
        let mut total_steal = 0u64;
        let mut total_ticks = 0u64;
        for line in m.split(|&c| c == b'\n') {
            if line.starts_with(b"cpu") && !line.starts_with(b"cpuidle") {
                let mut fields = line.split(|&c| c == b' ').filter(|f| !f.is_empty());
                let _name = fields.next();
                let mut idx = 0;
                for f in fields {
                    if let Some(v) = util::parse_u64(f) {
                        total_ticks = total_ticks.saturating_add(v);
                        if idx == 7 {
                            total_steal = total_steal.saturating_add(v);
                        }
                    }
                    idx += 1;
                }
            } else if line.starts_with(b"ctxt ") {
                let v = util::parse_u64(util::trim_ascii(&line[5..])).unwrap_or(0);
                env.ctxt_per_sec = Some(v as f64);
            } else if line.starts_with(b"intr ") {
                let v = util::parse_u64(util::trim_ascii(&line[5..])).unwrap_or(0);
                env.interrupts_per_sec = Some(v as f64);
            } else if line.starts_with(b"procs_running ") {
                env.procs_running = util::parse_u64(util::trim_ascii(&line[14..]));
            } else if line.starts_with(b"procs_blocked ") {
                env.procs_blocked = util::parse_u64(util::trim_ascii(&line[14..]));
            }
        }
        let _ = total_ticks;
        env.steal_ticks_total = Some(total_steal);
    }
    // pressure
    if let Ok(n) = sys::read_file(b"/proc/pressure/cpu", sc.a) {
        env.pressure_cpu = parse_pressure(&sc.a[..n]);
    }
    if let Ok(n) = sys::read_file(b"/proc/pressure/memory", sc.a) {
        env.pressure_mem = parse_pressure(&sc.a[..n]);
    }
    if let Ok(n) = sys::read_file(b"/proc/pressure/io", sc.a) {
        env.pressure_io = parse_pressure(&sc.a[..n]);
    }

    // cgroup v2
    if let Ok(n) = sys::read_file(b"/proc/self/cgroup", sc.a) {
        let m = &sc.a[..n];
        if let Some(line) = m.split(|&c| c == b'\n').find(|l| l.starts_with(b"0::")) {
            let rel = util::trim_ascii(&line[3..]);
            env.cgroup.path = copy_text(arena, rel);
        }
    }
    if let Some(rel) = env.cgroup.path {
        let base = b"/sys/fs/cgroup";
        let relb = rel.as_bytes();
        let mut path = |name: &[u8]| -> Option<&'static str> {
            let mut tmp = [0u8; 512];
            let n = join_path(&mut tmp, &[base, relb, name])?;
            let mut b2 = [0u8; 512];
            b2[..n].copy_from_slice(&tmp[..n]);
            read_text(arena, sc.b, &b2[..n])
        };
        env.cgroup.cpu_max = path(b"cpu.max");
        env.cgroup.cpu_weight = path(b"cpu.weight");
        env.cgroup.cpuset_cpus = path(b"cpuset.cpus.effective");
        env.cgroup.memory_max = path(b"memory.max");
        if let Some(v) = path(b"memory.current") {
            env.cgroup.memory_current = util::parse_u64(v.as_bytes());
        }
    }

    // CPU identification
    #[cfg(target_arch = "x86_64")]
    unsafe {
        let ids = arch::cpuid_ids();
        let vendor_len = ids.vendor.iter().position(|&c| c == 0).unwrap_or(12);
        env.cpu_ids.vendor = copy_text(arena, &ids.vendor[..vendor_len]);
        let brand_len = ids.brand.iter().position(|&c| c == 0).unwrap_or(48);
        env.cpu_ids.brand = copy_text(arena, util::trim_ascii(&ids.brand[..brand_len]));
        env.cpu_ids.family = Some(ids.family as u64);
        env.cpu_ids.model = Some(ids.model as u64);
        env.cpu_ids.stepping = Some(ids.stepping as u64);
        env.cpu_ids.hypervisor = hypervisor_name(&ids);
        env.freq.cpuid_base_mhz = if ids.base_mhz > 0 {
            Some(ids.base_mhz as u64)
        } else {
            None
        };
        env.freq.cpuid_max_mhz = if ids.max_mhz > 0 {
            Some(ids.max_mhz as u64)
        } else {
            None
        };
        let isa = arch::detect_isa();
        env.invariant_tsc = Some(isa.invariant_tsc);
        env.isa_summary = build_isa_summary_x86(&isa, arena);
    }
    #[cfg(target_arch = "aarch64")]
    {
        let isa = arch::detect_isa();
        env.isa_summary = build_isa_summary_arm(&isa, arena);
    }

    // /proc/cpuinfo fallback fields
    if let Ok(n) = sys::read_file(b"/proc/cpuinfo", sc.a) {
        let m = &sc.a[..n];
        if env.cpu_ids.vendor.is_none() {
            if let Some(v) = find_line(m, b"vendor_id") {
                env.cpu_ids.vendor = copy_text(arena, v);
            }
        }
        if env.cpu_ids.brand.is_none() {
            if let Some(v) = find_line(m, b"model name") {
                env.cpu_ids.brand = copy_text(arena, v);
            }
        }
        if let Some(v) = find_line(m, b"microcode") {
            env.cpu_ids.microcode = copy_text(arena, v);
        }
        if env.cpu_ids.family.is_none() {
            if let Some(v) = find_line(m, b"cpu family") {
                env.cpu_ids.family = util::parse_u64(v);
            }
        }
        if env.cpu_ids.model.is_none() {
            if let Some(v) = find_line(m, b"model") {
                env.cpu_ids.model = util::parse_u64(v);
            }
        }
        if env.cpu_ids.stepping.is_none() {
            if let Some(v) = find_line(m, b"stepping") {
                env.cpu_ids.stepping = util::parse_u64(v);
            }
        }
        if env.cpu_ids.vendor.is_none() {
            if let Some(v) = find_line(m, b"CPU implementer") {
                env.cpu_ids.vendor = copy_text(arena, v);
            }
        }
        if env.cpu_ids.brand.is_none() {
            if let Some(v) = find_line(m, b"Hardware") {
                env.cpu_ids.brand = copy_text(arena, v);
            }
        }
        if let Some(v) = find_line(m, b"flags") {
            let flags: &'static mut [&'static str] = unsafe { arena.alloc_slice(MAX_FLAGS) }.unwrap();
            let mut cnt = 0;
            for tok in v.split(|&c| c == b' ') {
                if cnt >= MAX_FLAGS {
                    break;
                }
                if let Some(s) = copy_text(arena, tok) {
                    flags[cnt] = s;
                    cnt += 1;
                }
            }
            env.cpu_flags = &flags[..cnt];
        }
        if let Some(v) = find_line(m, b"Features") {
            let flags: &'static mut [&'static str] = unsafe { arena.alloc_slice(MAX_FLAGS) }.unwrap();
            let mut cnt = 0;
            for tok in v.split(|&c| c == b' ') {
                if cnt >= MAX_FLAGS {
                    break;
                }
                if let Some(s) = copy_text(arena, tok) {
                    flags[cnt] = s;
                    cnt += 1;
                }
            }
            env.cpu_flags = &flags[..cnt];
        }
    }

    // microcode sysfs
    if env.cpu_ids.microcode.is_none() {
        env.cpu_ids.microcode = read_text(
            arena,
            sc.a,
            b"/sys/devices/system/cpu/cpu0/microcode/version",
        );
    }

    // online CPUs
    {
        let cpus: &'static mut [u32] = unsafe { arena.alloc_slice(MAX_CPUS) }.unwrap();
        let mut n = 0usize;
        if let Ok(nread) = sys::read_file(b"/sys/devices/system/cpu/online", sc.a) {
            n = parse_cpu_list(util::trim_ascii(&sc.a[..nread]), cpus);
        }
        if n == 0 {
            // fall back to sched_getaffinity
            let mut mask = [0u8; 128];
            if let Ok(bytes) = sys::sched_getaffinity(0, &mut mask) {
                for i in 0..bytes * 8 {
                    if mask[i / 8] & (1 << (i % 8)) != 0 && n < MAX_CPUS {
                        cpus[n] = i as u32;
                        n += 1;
                    }
                }
            }
        }
        env.online_cpus = &cpus[..n];
    }

    // topology
    {
        let topo: &'static mut [CpuTopo] = unsafe { arena.alloc_slice(MAX_CPUS) }.unwrap();
        let mut cnt = 0;
        let cpus = env.online_cpus;
        for &cpu in cpus {
            let mut path = [0u8; 128];
            let mut cpu_buf = [0u8; 16];
            let cpu_s = cpu_dir_name(cpu, &mut cpu_buf);
            let n = join_path(
                &mut path,
                &[b"/sys/devices/system/cpu", cpu_s],
            )
            .unwrap_or(0);
            let base = &path[..n];
            let mut t = CpuTopo {
                cpu,
                ..CpuTopo::default()
            };
            let mut p2 = [0u8; 160];
            let f = |name: &[u8], out: &mut [u8; 160]| -> Option<usize> {
                let n = join_path(out, &[base, b"topology", name])?;
                Some(n)
            };
            if let Some(n) = f(b"core_id", &mut p2) {
                t.core_id = read_u64_file(&p2[..n]);
            }
            if let Some(n) = f(b"physical_package_id", &mut p2) {
                t.package_id = read_u64_file(&p2[..n]);
            }
            if let Some(n) = f(b"die_id", &mut p2) {
                t.die_id = read_u64_file(&p2[..n]);
            }
            if let Some(n) = f(b"cluster_id", &mut p2) {
                t.cluster_id = read_u64_file(&p2[..n]);
            }
            if let Some(n) = f(b"thread_siblings_list", &mut p2) {
                t.thread_siblings = read_text(arena, sc.b, &p2[..n]);
            }
            if let Some(n) = f(b"core_siblings_list", &mut p2) {
                t.core_siblings = read_text(arena, sc.b, &p2[..n]);
            }
            topo[cnt] = t;
            cnt += 1;
        }
        env.topology = &topo[..cnt];
    }

    // caches
    {
        let caches: &'static mut [CacheInfo] = unsafe { arena.alloc_slice(MAX_CACHES) }.unwrap();
        let mut cnt = 0;
        'cpu: for &cpu in env.online_cpus {
            let mut cdir = [0u8; 128];
            let mut cpu_buf = [0u8; 16];
            let cpu_s = cpu_dir_name(cpu, &mut cpu_buf);
            let n = join_path(
                &mut cdir,
                &[
                    b"/sys/devices/system/cpu",
                    cpu_s,
                    b"cache",
                ],
            )
            .unwrap_or(0);
            let base = &cdir[..n];
            let mut indices: [u8; 32] = [0; 32];
            let mut nidx = 0usize;
            let list_buf: &mut [u8] = &mut *sc.a;
            let _ = for_each_dir(list_buf, base, |name| {
                if nidx < indices.len() {
                    if let Some(rest) = name.strip_prefix(b"index") {
                        if let Some(v) = util::parse_u64(rest) {
                            indices[nidx] = v as u8;
                            nidx += 1;
                        }
                    }
                }
            });
            for &idx in &indices[..nidx] {
                if cnt >= MAX_CACHES {
                    break 'cpu;
                }
                let mut idir = [0u8; 160];
                let mut idx_buf = [0u8; 16];
                idx_buf[..5].copy_from_slice(b"index");
                let ilen = crate::fmt::u64_dec(idx as u64, &mut idx_buf[5..]);
                let idx_name = &idx_buf[..5 + ilen];
                let n = join_path(&mut idir, &[base, idx_name]).unwrap_or(0);
                let ibase = &idir[..n];
                let mut c = CacheInfo {
                    cpu,
                    ..CacheInfo::default()
                };
                let mut p2 = [0u8; 200];
                let mut get = |name: &[u8]| -> Option<&'static str> {
                    let n = join_path(&mut p2, &[ibase, name])?;
                    read_text(arena, sc.b, &p2[..n])
                };
                c.level = get(b"level")
                    .and_then(|s| util::parse_u64(s.as_bytes()))
                    .unwrap_or(0) as u32;
                c.kind = get(b"type").unwrap_or("unknown");
                c.size_bytes = get(b"size").map(parse_size).unwrap_or(0);
                c.line_size = get(b"coherency_line_size")
                    .and_then(|s| util::parse_u64(s.as_bytes()))
                    .unwrap_or(0);
                c.ways = get(b"ways_of_associativity")
                    .and_then(|s| util::parse_u64(s.as_bytes()))
                    .unwrap_or(0);
                c.shared_cpu_list = get(b"shared_cpu_list").unwrap_or("");
                caches[cnt] = c;
                cnt += 1;
            }
        }
        env.caches = &caches[..cnt];
    }

    // NUMA nodes
    {
        let nodes: &'static mut [NumaNode] = unsafe { arena.alloc_slice(MAX_NODES) }.unwrap();
        let mut cnt = 0;
        let mut ids: [u8; MAX_NODES] = [0; MAX_NODES];
        let mut nids = 0usize;
        let _ = for_each_dir(sc.a, b"/sys/devices/system/node", |name| {
            if nids < MAX_NODES {
                if let Some(rest) = name.strip_prefix(b"node") {
                    if let Some(v) = util::parse_u64(rest) {
                        ids[nids] = v as u8;
                        nids += 1;
                    }
                }
            }
        });
        for &id in &ids[..nids] {
            let mut ndir = [0u8; 128];
            let mut id_buf = [0u8; 16];
            let id_s = node_dir_name(id as u32, &mut id_buf);
            let n = join_path(&mut ndir, &[b"/sys/devices/system/node", id_s])
                .unwrap_or(0);
            let nbase = &ndir[..n];
            let mut node = NumaNode {
                id: id as u32,
                ..NumaNode::default()
            };
            let mut p2 = [0u8; 160];
            if let Some(n) = join_path(&mut p2, &[nbase, b"meminfo"]) {
                if let Ok(nr) = sys::read_file(&p2[..n], sc.b) {
                    node.mem_total_kib = find_line_u64(&sc.b[..nr], b"MemTotal");
                }
            }
            if let Some(n) = join_path(&mut p2, &[nbase, b"cpulist"]) {
                node.cpus = read_text(arena, sc.b, &p2[..n]).unwrap_or("");
            }
            nodes[cnt] = node;
            cnt += 1;
        }
        env.numa_nodes = &nodes[..cnt];
    }

    // cpufreq
    for (key, dst) in [
        (&b"/sys/devices/system/cpu/cpu0/cpufreq/scaling_cur_freq"[..], 0),
        (&b"/sys/devices/system/cpu/cpu0/cpufreq/cpuinfo_max_freq"[..], 1),
        (&b"/sys/devices/system/cpu/cpu0/cpufreq/cpuinfo_min_freq"[..], 2),
    ] {
        let v = read_u64_file(key);
        match dst {
            0 => env.freq.cur_khz = v,
            1 => env.freq.max_khz = v,
            _ => env.freq.min_khz = v,
        }
    }
    for p in [
        &b"/sys/devices/system/cpu/cpufreq/boost"[..],
        b"/sys/devices/system/cpu/intel_pstate/no_turbo",
    ] {
        if let Some(v) = read_u64_file(p) {
            env.freq.boost = Some(if p.ends_with(b"no_turbo") {
                v == 0
            } else {
                v != 0
            });
            break;
        }
    }

    // mounts
    {
        let mounts: &'static mut [MountInfo] = unsafe { arena.alloc_slice(MAX_MOUNTS) }.unwrap();
        let mut cnt = 0;
        if let Ok(n) = sys::read_file(b"/proc/mounts", sc.a) {
            for line in sc.a[..n].split(|&c| c == b'\n') {
                if cnt >= MAX_MOUNTS {
                    break;
                }
                let mut it = line.split(|&c| c == b' ');
                let source = it.next().unwrap_or(&[]);
                let mp = it.next().unwrap_or(&[]);
                let fs = it.next().unwrap_or(&[]);
                let opts = it.next().unwrap_or(&[]);
                if source.is_empty() || fs.is_empty() {
                    continue;
                }
                if is_pseudo_fs(fs) {
                    continue;
                }
                mounts[cnt] = MountInfo {
                    source: copy_text(arena, source).unwrap_or(""),
                    mount_point: copy_text(arena, mp).unwrap_or(""),
                    fstype: copy_text(arena, fs).unwrap_or(""),
                    options: copy_text(arena, opts).unwrap_or(""),
                };
                cnt += 1;
            }
        }
        env.mounts = &mounts[..cnt];
    }

    // block devices
    {
        let devs: &'static mut [BlockDevInfo] = unsafe { arena.alloc_slice(MAX_BLOCKS) }.unwrap();
        let mut cnt = 0;
        let mut names: [&'static str; MAX_BLOCKS] = [""; MAX_BLOCKS];
        let mut nn = 0;
        let _ = for_each_dir(sc.a, b"/sys/block", |name| {
            if nn < MAX_BLOCKS {
                if !name.starts_with(b"loop") && !name.starts_with(b"ram") {
                    if let Some(s) = copy_text(arena, name) {
                        names[nn] = s;
                        nn += 1;
                    }
                }
            }
        });
        for name in &names[..nn] {
            let mut base = [0u8; 128];
            let n = join_path(&mut base, &[b"/sys/block", name.as_bytes()]).unwrap_or(0);
            let base = &base[..n];
            let mut d = BlockDevInfo {
                name,
                ..BlockDevInfo::default()
            };
            d.size_sectors = read_u64_at(base, b"size");
            d.rotational = read_u64_at(base, b"queue/rotational").map(|v| v != 0);
            d.logical_block_size = read_u64_at(base, b"queue/logical_block_size");
            d.physical_block_size = read_u64_at(base, b"queue/physical_block_size");
            d.nr_requests = read_u64_at(base, b"queue/nr_requests");
            d.scheduler = read_text_at(arena, sc.b, base, b"queue/scheduler");
            d.model = read_text_at(arena, sc.b, base, b"device/model");
            devs[cnt] = d;
            cnt += 1;
        }
        env.block_devices = &devs[..cnt];
    }

    // test directory selection
    env.test_dir = Some(".");

    // identities
    let mut cpu_sig = Sha256::new();
    cpu_sig.update(b"vmbench.cpu-signature.v1");
    if let Some(v) = env.cpu_ids.vendor {
        cpu_sig.update(v.as_bytes());
    }
    for v in [env.cpu_ids.family, env.cpu_ids.model, env.cpu_ids.stepping] {
        if let Some(v) = v {
            cpu_sig.update(&v.to_le_bytes());
        }
    }
    if let Some(v) = env.cpu_ids.microcode {
        cpu_sig.update(v.as_bytes());
    }
    let mut flags_sig = Sha256::new();
    for f in env.cpu_flags {
        flags_sig.update(f.as_bytes());
        flags_sig.update(b",");
    }
    let flags_hash = flags_sig.finish();
    cpu_sig.update(&flags_hash);
    env.cpu_signature_hash = Some(cpu_sig.finish());

    let mut topo = Sha256::new();
    topo.update(b"vmbench.topology.v1");
    for t in env.topology {
        if let Some(s) = t.thread_siblings {
            topo.update(s.as_bytes());
        }
        topo.update(b";");
    }
    env.topology_hash = Some(topo.finish());

    let mut cache = Sha256::new();
    cache.update(b"vmbench.cache.v1");
    for c in env.caches {
        cache.update(&[c.level as u8]);
        cache.update(c.kind.as_bytes());
        cache.update(&c.size_bytes.to_le_bytes());
        cache.update(&c.line_size.to_le_bytes());
        cache.update(c.shared_cpu_list.as_bytes());
        cache.update(b";");
    }
    env.cache_hash = Some(cache.finish());

    env.hypervisor_hash = env
        .cpu_ids
        .hypervisor
        .map(|h| sha256::hash(h.as_bytes()));

    env.instance_id = hash_opt(
        &[
            env.machine_id_hash,
            env.dmi.product_uuid_hash,
            env.hostname_hash,
        ],
        b"vmbench.instance.v1",
    );
    env.environment_id = hash_opt(
        &[
            env.cpu_signature_hash,
            env.topology_hash,
            env.cache_hash,
            env.memory_hash,
            env.hypervisor_hash,
        ],
        b"vmbench.environment.v1",
    );

    // run identity
    let mut rng = crate::rand::Rng::from_entropy();
    rng.fill_bytes(&mut env.run_id);

    env.cycles_hz = {
        let hz = time::cycles_hz();
        if hz > 0 {
            Some(hz)
        } else {
            None
        }
    };
    env
}

fn read_u64_file(path: &[u8]) -> Option<u64> {
    let mut buf = [0u8; 128];
    let n = sys::read_file(path, &mut buf).ok()?;
    util::parse_u64(util::trim_ascii(&buf[..n]))
}

fn parse_size(s: &str) -> u64 {
    let b = s.as_bytes();
    let mut num = 0u64;
    let mut i = 0;
    while i < b.len() && b[i].is_ascii_digit() {
        num = num * 10 + (b[i] - b'0') as u64;
        i += 1;
    }
    let suffix = &s[i..];
    match suffix {
        "K" | "kB" => num * 1024,
        "M" | "MB" => num * 1024 * 1024,
        "G" | "GB" => num * 1024 * 1024 * 1024,
        _ => num,
    }
}

fn fmt_u32<'a>(v: u32, buf: &'a mut [u8; 10]) -> &'a [u8] {
    let n = crate::fmt::u64_dec(v as u64, buf);
    &buf[..n]
}

fn is_pseudo_fs(fs: &[u8]) -> bool {
    const PSEUDO: &[&[u8]] = &[
        b"proc",
        b"sysfs",
        b"devpts",
        b"devtmpfs",
        b"cgroup",
        b"cgroup2",
        b"pstore",
        b"securityfs",
        b"debugfs",
        b"tracefs",
        b"configfs",
        b"fusectl",
        b"mqueue",
        b"bpf",
        b"autofs",
        b"binfmt_misc",
        b"rpc_pipefs",
        b"nsfs",
    ];
    PSEUDO.contains(&fs)
}

#[cfg(target_arch = "x86_64")]
fn build_isa_summary_x86(
    f: &arch::IsaFlags,
    arena: &mut Arena,
) -> &'static [(&'static str, bool)] {
    let list: &'static mut [(&'static str, bool)] = unsafe { arena.alloc_slice(32) }.unwrap();
    let items: [(&'static str, bool); 22] = [
        ("sse2", f.sse2),
        ("sse3", f.sse3),
        ("ssse3", f.ssse3),
        ("sse4.1", f.sse41),
        ("sse4.2", f.sse42),
        ("avx", f.avx),
        ("avx2", f.avx2),
        ("fma", f.fma),
        ("bmi1", f.bmi1),
        ("bmi2", f.bmi2),
        ("avx512f", f.avx512f),
        ("avx512bw", f.avx512bw),
        ("avx512vl", f.avx512vl),
        ("avx512dq", f.avx512dq),
        ("avx512vbmi", f.avx512vbmi),
        ("avx512vnni", f.avx512vnni),
        ("aes", f.aes),
        ("pclmulqdq", f.pclmulqdq),
        ("sha", f.sha),
        ("crc32", f.crc32),
        ("popcnt", f.popcnt),
        ("invariant_tsc", f.invariant_tsc),
    ];
    list[..items.len()].copy_from_slice(&items);
    &list[..items.len()]
}

#[cfg(target_arch = "aarch64")]
fn build_isa_summary_arm(
    f: &arch::IsaFlags,
    arena: &mut Arena,
) -> &'static [(&'static str, bool)] {
    let list: &'static mut [(&'static str, bool)] = unsafe { arena.alloc_slice(32) }.unwrap();
    let items: [(&'static str, bool); 14] = [
        ("fp", f.fp),
        ("asimd", f.asimd),
        ("aes", f.aes),
        ("pmull", f.pmull),
        ("sha1", f.sha1),
        ("sha2", f.sha2),
        ("crc32", f.crc32),
        ("atomics", f.atomics),
        ("fp16", f.fp16),
        ("dotprod", f.dotprod),
        ("sve", f.sve),
        ("sve2", f.sve2),
        ("i8mm", f.i8mm),
        ("bf16", f.bf16),
    ];
    list[..items.len()].copy_from_slice(&items);
    &list[..items.len()]
}
