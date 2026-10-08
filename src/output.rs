use crate::env::EnvInfo;
use crate::fmt;
use crate::json::Out;
use crate::runner::{
    compute_scalar_stats, curve_runs, Ctx, Direction, Entry, MetricKind, ParamValue, Point, Status,
    MAX_CURVE_POINTS, MAX_RUNS,
};
use crate::stats;

pub const SUITE_VERSION: &str = env!("CARGO_PKG_VERSION");

fn dir_str(d: Direction) -> &'static str {
    match d {
        Direction::HigherBetter => "higher_better",
        Direction::LowerBetter => "lower_better",
        Direction::Neutral => "neutral",
    }
}

fn unavail(out: &mut Out, list: &[(&str, bool)]) {
    out.key("unavailable");
    out.begin_arr();
    for (name, present) in list {
        if !*present {
            out.elem();
            out.jstr(name);
        }
    }
    out.end();
}

pub fn write_report(
    out: &mut Out,
    ctx: &Ctx,
    started_realtime_ns: u64,
    duration_ns: u64,
    binary_hash: Option<[u8; 32]>,
    meta: Option<&str>,
) {
    let env = &ctx.env;
    out.begin_obj();
    out.key_u64("schema_version", 1);
    out.key_str("benchmark_suite_version", SUITE_VERSION);
    if let Some(m) = meta {
        out.key("meta");
        out.raw(m.as_bytes());
    }
    out.key("binary_sha256");
    match binary_hash {
        Some(h) => out.sha256(&h),
        None => out.null(),
    }
    out.key("build");
    out.begin_obj();
    out.key_str("rustc_version", env!("VMBENCH_RUSTC_VERSION"));
    out.key_str("llvm_version", env!("VMBENCH_LLVM_VERSION"));
    out.key_str("build_host", env!("VMBENCH_BUILD_HOST"));
    out.key_str("build_target", env!("VMBENCH_BUILD_TARGET"));
    out.key_str("opt_level", env!("VMBENCH_BUILD_OPT_LEVEL"));
    out.key_str("profile", env!("VMBENCH_BUILD_PROFILE"));
    out.key_str("package_version", SUITE_VERSION);
    out.end();

    out.key("run_id");
    write_run_id(out, &env.run_id);
    out.key("run_identity");
    out.begin_obj();
    out.key("id");
    write_run_id(out, &env.run_id);
    out.key_str("scheme", "random-128-bit");
    out.end();

    out.key("instance_identity");
    out.begin_obj();
    out.key("id");
    match env.instance_id {
        Some(h) => out.sha256(&h),
        None => out.null(),
    }
    out.key_str(
        "scheme",
        "sha256(domain||machine_id_hash||dmi_product_uuid_hash||hostname_hash)",
    );
    out.key("components");
    out.begin_obj();
    out.key("machine_id");
    match env.machine_id_hash {
        Some(h) => out.sha256(&h),
        None => out.null(),
    }
    out.key("dmi_product_uuid");
    match env.dmi.product_uuid_hash {
        Some(h) => out.sha256(&h),
        None => out.null(),
    }
    out.key("hostname");
    match env.hostname_hash {
        Some(h) => out.sha256(&h),
        None => out.null(),
    }
    out.end();
    unavail(
        out,
        &[
            ("machine_id", env.machine_id_hash.is_some()),
            ("dmi_product_uuid", env.dmi.product_uuid_hash.is_some()),
            ("hostname", env.hostname_hash.is_some()),
        ],
    );
    out.end();

    out.key("environment_identity");
    out.begin_obj();
    out.key("id");
    match env.environment_id {
        Some(h) => out.sha256(&h),
        None => out.null(),
    }
    out.key_str(
        "scheme",
        "sha256(domain||cpu_signature||topology||cache_topology||memory_size||hypervisor)",
    );
    out.key("components");
    out.begin_obj();
    out.key("cpu_signature");
    match env.cpu_signature_hash {
        Some(h) => out.sha256(&h),
        None => out.null(),
    }
    out.key("cpu_topology");
    match env.topology_hash {
        Some(h) => out.sha256(&h),
        None => out.null(),
    }
    out.key("cache_topology");
    match env.cache_hash {
        Some(h) => out.sha256(&h),
        None => out.null(),
    }
    out.key("memory_size");
    match env.memory_hash {
        Some(h) => out.sha256(&h),
        None => out.null(),
    }
    out.key("hypervisor");
    match env.hypervisor_hash {
        Some(h) => out.sha256(&h),
        None => out.null(),
    }
    out.end();
    unavail(
        out,
        &[
            ("cpu_signature", env.cpu_signature_hash.is_some()),
            ("cpu_topology", env.topology_hash.is_some()),
            ("cache_topology", env.cache_hash.is_some()),
            ("memory_size", env.memory_hash.is_some()),
            ("hypervisor", env.hypervisor_hash.is_some()),
        ],
    );
    out.end();

    out.key("boot_identity");
    out.begin_obj();
    out.key_opt_str("boot_id", env.boot_id);
    out.key_str("scheme", "raw-boot-id");
    out.end();

    out.key("started_at");
    {
        let mut tmp = [0u8; 40];
        let n = fmt::iso8601(started_realtime_ns, &mut tmp);
        out.jstr(core::str::from_utf8(&tmp[..n]).unwrap_or(""));
    }
    out.key("duration");
    out.begin_obj();
    out.key_u64("total_ns", duration_ns);
    out.key_f64("total_s", duration_ns as f64 / 1e9);
    out.end();

    write_system(out, env);
    write_cpu(out, env);
    write_memory(out, env);
    write_storage(out, env, ctx);
    write_noise(out, env);

    out.key("annotations");
    out.begin_arr();
    for a in ctx.annotations.as_slice() {
        out.elem();
        out.jstr(a);
    }
    out.end();

    out.key("benchmarks");
    out.begin_arr();
    for i in 0..ctx.n_entries {
        out.elem();
        let e = unsafe { &*crate::runner::entry_ptr(i) };
        write_entry(out, ctx, e);
    }
    out.end();

    out.end();
}

fn write_run_id(out: &mut Out, id: &[u8; 16]) {
    let mut tmp = [0u8; 32];
    let n = fmt::hex_lower(id, &mut tmp);
    out.jstr(core::str::from_utf8(&tmp[..n]).unwrap_or(""));
}

fn write_system(out: &mut Out, env: &EnvInfo) {
    out.key("system");
    out.begin_obj();
    out.key_opt_str("kernel_version", env.kernel_version);
    out.key_opt_str("kernel_release", env.kernel_release);
    out.key_opt_str("distro", env.distro);
    // The raw hostname is deliberately not published; only its hash appears in
    // instance_identity. See README "プライバシー".
    out.key_opt_u64("page_size", Some(env.page_size));
    out.key_opt_u64("clktck", Some(env.clktck));
    out.key_opt_str("thp_enabled", env.thp_enabled);
    out.key_opt_str("thp_defrag", env.thp_defrag);
    out.key_opt_str("clock_source", env.clock_source);
    out.key_opt_f64("uptime_s", env.uptime_s);
    out.key("cgroup");
    out.begin_obj();
    // The raw cgroup path is not published (it can contain user identifiers).
    out.key_opt_str("cpu_max", env.cgroup.cpu_max);
    out.key_opt_str("cpu_weight", env.cgroup.cpu_weight);
    out.key_opt_str("cpuset_cpus", env.cgroup.cpuset_cpus);
    out.key_opt_str("memory_max", env.cgroup.memory_max);
    out.key_opt_u64("memory_current", env.cgroup.memory_current);
    out.end();
    out.key("dmi");
    out.begin_obj();
    out.key_opt_str("sys_vendor", env.dmi.sys_vendor);
    out.key_opt_str("product_name", env.dmi.product_name);
    out.key_opt_str("board_vendor", env.dmi.board_vendor);
    out.key_opt_str("bios_version", env.dmi.bios_version);
    out.key("product_uuid_hash");
    match env.dmi.product_uuid_hash {
        Some(h) => out.sha256(&h),
        None => out.null(),
    }
    out.end();
    out.end();
}

fn write_cpu(out: &mut Out, env: &EnvInfo) {
    out.key("cpu");
    out.begin_obj();
    out.key_str("architecture", crate::arch::arch_name());
    out.key_opt_str("vendor", env.cpu_ids.vendor);
    out.key_opt_str("brand", env.cpu_ids.brand);
    out.key_opt_u64("family", env.cpu_ids.family);
    out.key_opt_u64("model", env.cpu_ids.model);
    out.key_opt_u64("stepping", env.cpu_ids.stepping);
    out.key_opt_str("microcode", env.cpu_ids.microcode);
    out.key_opt_str("hypervisor", env.cpu_ids.hypervisor);
    out.key_u64("online_cpus", env.online_cpus.len() as u64);
    out.key("online_cpu_list");
    out.begin_arr();
    for c in env.online_cpus {
        out.elem();
        out.u64(*c as u64);
    }
    out.end();
    out.key("isa");
    out.begin_obj();
    for (name, present) in env.isa_summary {
        out.key_bool(name, *present);
    }
    out.end();
    out.key("flags");
    out.begin_arr();
    for f in env.cpu_flags {
        out.elem();
        out.jstr(f);
    }
    out.end();
    out.key("frequency_khz");
    out.begin_obj();
    out.key_opt_u64("scaling_cur", env.freq.cur_khz);
    out.key_opt_u64("cpuinfo_max", env.freq.max_khz);
    out.key_opt_u64("cpuinfo_min", env.freq.min_khz);
    out.key_opt_u64("cpuid_base_mhz", env.freq.cpuid_base_mhz);
    out.key_opt_u64("cpuid_max_mhz", env.freq.cpuid_max_mhz);
    out.key_opt_bool("boost", env.freq.boost);
    out.end();
    out.key_opt_u64("cycles_hz", env.cycles_hz);
    out.key_str("cycles_source", env.cycles_source);
    out.key_opt_bool("invariant_tsc", env.invariant_tsc);
    out.key("topology");
    out.begin_arr();
    for t in env.topology {
        out.elem();
        out.begin_obj();
        out.key_u64("cpu", t.cpu as u64);
        out.key_opt_u64("core_id", t.core_id);
        out.key_opt_u64("package_id", t.package_id);
        out.key_opt_u64("die_id", t.die_id);
        out.key_opt_u64("cluster_id", t.cluster_id);
        out.key_opt_str("thread_siblings", t.thread_siblings);
        out.key_opt_str("core_siblings", t.core_siblings);
        out.end();
    }
    out.end();
    out.key("caches");
    out.begin_arr();
    for c in env.caches {
        out.elem();
        out.begin_obj();
        out.key_u64("cpu", c.cpu as u64);
        out.key_u64("level", c.level as u64);
        out.key_str("type", c.kind);
        out.key_u64("size_bytes", c.size_bytes);
        out.key_u64("line_size", c.line_size);
        out.key_u64("ways", c.ways);
        out.key_str("shared_cpu_list", c.shared_cpu_list);
        out.end();
    }
    out.end();
    out.key("numa_nodes");
    out.begin_arr();
    for n in env.numa_nodes {
        out.elem();
        out.begin_obj();
        out.key_u64("id", n.id as u64);
        out.key_opt_u64("mem_total_kib", n.mem_total_kib);
        out.key_str("cpus", n.cpus);
        out.end();
    }
    out.end();
    out.end();
}

fn write_memory(out: &mut Out, env: &EnvInfo) {
    out.key("memory");
    out.begin_obj();
    out.key_opt_u64("total_kib", env.mem.total_kib);
    out.key_opt_u64("free_kib", env.mem.free_kib);
    out.key_opt_u64("available_kib", env.mem.available_kib);
    out.key_opt_u64("buffers_kib", env.mem.buffers_kib);
    out.key_opt_u64("cached_kib", env.mem.cached_kib);
    out.key_opt_u64("swap_total_kib", env.mem.swap_total_kib);
    out.key_opt_u64("swap_free_kib", env.mem.swap_free_kib);
    out.key_opt_u64("dirty_kib", env.mem.dirty_kib);
    out.key_opt_u64("writeback_kib", env.mem.writeback_kib);
    out.key_opt_u64("anon_hugepages_kib", env.mem.anon_hugepages_kib);
    out.key_opt_u64("hugepages_total", env.mem.hugepages_total);
    out.key_opt_u64("hugepages_free", env.mem.hugepages_free);
    out.key_opt_u64("hugepage_size_kib", env.mem.hugepage_size_kib);
    out.end();
}

fn write_storage(out: &mut Out, env: &EnvInfo, ctx: &Ctx) {
    out.key("storage");
    out.begin_obj();
    out.key("mounts");
    out.begin_arr();
    for m in env.mounts {
        out.elem();
        out.begin_obj();
        out.key_str("source", m.source);
        out.key_str("mount_point", m.mount_point);
        out.key_str("fstype", m.fstype);
        out.key_str("options", m.options);
        out.end();
    }
    out.end();
    out.key("block_devices");
    out.begin_arr();
    for d in env.block_devices {
        out.elem();
        out.begin_obj();
        out.key_str("name", d.name);
        out.key_opt_u64("size_sectors", d.size_sectors);
        out.key_opt_bool("rotational", d.rotational);
        out.key_opt_u64("logical_block_size", d.logical_block_size);
        out.key_opt_u64("physical_block_size", d.physical_block_size);
        out.key_opt_u64("nr_requests", d.nr_requests);
        out.key_opt_str("scheduler", d.scheduler);
        out.key_opt_str("model", d.model);
        out.end();
    }
    out.end();
    out.key_opt_str("test_dir", env.test_dir);
    out.key("targets");
    out.begin_arr();
    for i in 0..ctx.cfg.n_storage_targets {
        let t = ctx.cfg.storage_targets[i];
        let st = ctx.storage_state[i];
        out.elem();
        out.begin_obj();
        out.key_str("name", t.name);
        out.key_str("path", t.path);
        out.key_str("fstype", st.fstype);
        out.key_str("status", st.status);
        out.end();
    }
    out.end();
    out.end();
}

fn write_noise(out: &mut Out, env: &EnvInfo) {
    out.key("noise");
    out.begin_obj();
    out.key("loadavg");
    out.begin_arr();
    for v in env.loadavg {
        out.elem();
        out.f64(v);
    }
    out.end();
    out.key_opt_u64("procs_running", env.procs_running);
    out.key_opt_u64("procs_blocked", env.procs_blocked);
    out.key_opt_f64("ctxt_total", env.ctxt_per_sec);
    out.key_opt_f64("intr_total", env.interrupts_per_sec);
    out.key_opt_u64("steal_ticks_total", env.steal_ticks_total);
    for (name, p) in [
        ("pressure_cpu", env.pressure_cpu),
        ("pressure_memory", env.pressure_mem),
        ("pressure_io", env.pressure_io),
    ] {
        out.key(name);
        match p {
            Some(p) => {
                out.begin_obj();
                out.key_f64("avg10", p.avg10);
                out.key_f64("avg60", p.avg60);
                out.key_f64("avg300", p.avg300);
                out.key_u64("total_us", p.total_us);
                out.end();
            }
            None => out.null(),
        }
    }
    out.end();
}

fn write_entry(out: &mut Out, ctx: &Ctx, e: &Entry) {
    let meta = e.meta;
    out.begin_obj();
    out.key_str("id", meta.id);
    out.key_u64("version", meta.version as u64);
    out.key_str("description", meta.description);
    out.key("implementation");
    out.begin_obj();
    out.key_str("source", meta.imp.source);
    out.key_str("source_revision", env!("VMBENCH_SOURCE_REVISION"));
    out.key_str("kernel", meta.imp.kernel);
    out.key_str("algorithm", meta.imp.algorithm);
    out.key_str("isa", meta.imp.isa);
    out.key("code_hash");
    match ctx.code_hash(meta.imp.kernel) {
        Some(h) => {
            out.sha256(&h);
            out.key_str("code_hash_status", "ok");
        }
        None => {
            out.null();
            out.key_str("code_hash_status", "unavailable");
        }
    }
    out.end();
    out.key("parameters");
    out.begin_obj();
    for p in &e.params[..e.n_params] {
        out.key(p.key);
        match p.value {
            ParamValue::Int(v) => out.i64(v),
            ParamValue::Num(v) => out.f64(v),
            ParamValue::Str(s) => out.jstr(s),
            ParamValue::Bool(b) => out.bool(b),
        }
    }
    out.end();
    out.key("metrics");
    out.begin_arr();
    for m in meta.metrics {
        out.elem();
        out.begin_obj();
        out.key_str("key", m.key);
        out.key_str("unit", m.unit);
        out.key_str("direction", dir_str(m.direction));
        out.key_str(
            "kind",
            match m.kind {
                MetricKind::Scalar => "scalar",
                MetricKind::Curve => "curve",
            },
        );
        out.end();
    }
    out.end();
    out.key_str(
        "status",
        match e.status {
            Status::Ok => "ok",
            Status::Unsupported => "unsupported",
            Status::VerifyFailed => "verification_failed",
            Status::Failed => "failed",
        },
    );
    if !e.reason.is_empty() {
        out.key_str("reason", e.reason);
    }
    out.key("runs");
    out.begin_arr();
    for r in &e.runs[..e.n_runs] {
        out.elem();
        out.begin_obj();
        out.key_u64("duration_ns", r.duration_ns);
        out.key_u64("work_units", r.work_units);
        if r.chunks > 0 {
            out.key("chunk_rate");
            out.begin_obj();
            out.key_f64("min", r.chunk_rate_min);
            out.key_f64("median", r.chunk_rate_median);
            out.key_f64("max", r.chunk_rate_max);
            out.key_u64("count", r.chunks as u64);
            out.end();
        }
        out.key("values");
        out.begin_obj();
        for (mi, m) in meta.metrics.iter().enumerate() {
            if r.present & (1 << mi) == 0 || m.kind != MetricKind::Scalar {
                continue;
            }
            out.key(m.key);
            out.f64(r.scalars[mi]);
        }
        out.end();
        out.key("curves");
        out.begin_obj();
        for (mi, m) in meta.metrics.iter().enumerate() {
            if r.present & (1 << mi) == 0 || m.kind != MetricKind::Curve {
                continue;
            }
            out.key(m.key);
            write_points(out, r.curves[mi].points());
        }
        out.end();
        out.end();
    }
    out.end();
    if e.status == Status::Ok && e.n_runs > 0 {
        write_stats(out, e);
    }
    out.end();
}

fn write_points(out: &mut Out, pts: &[Point]) {
    out.begin_arr();
    for p in pts.iter().take(MAX_CURVE_POINTS) {
        out.elem();
        out.begin_obj();
        out.key_f64("x", p.x);
        out.key_f64("y", p.y);
        out.end();
    }
    out.end();
}

fn write_stats(out: &mut Out, e: &Entry) {
    for which in 0..4 {
        let name = match which {
            0 => "median",
            1 => "best",
            2 => "worst",
            _ => "variation_pct",
        };
        out.key(name);
        out.begin_obj();
        for (mi, m) in e.meta.metrics.iter().enumerate() {
            match m.kind {
                MetricKind::Scalar => {
                    if let Some(s) = compute_scalar_stats(e, mi) {
                        out.key(m.key);
                        out.f64(match which {
                            0 => s.median,
                            1 => s.best,
                            2 => s.worst,
                            _ => s.variation_pct,
                        });
                    }
                }
                MetricKind::Curve => {
                    let (runs, n) = curve_runs(e, mi);
                    if n == 0 {
                        continue;
                    }
                    out.key(m.key);
                    write_curve_stats(out, &runs[..n], m.direction, which);
                }
            }
        }
        out.end();
    }
    out.key("count");
    out.begin_obj();
    for (mi, m) in e.meta.metrics.iter().enumerate() {
        if m.kind != MetricKind::Scalar {
            continue;
        }
        if let Some(s) = compute_scalar_stats(e, mi) {
            out.key_u64(m.key, s.n as u64);
        }
    }
    out.end();
}

fn write_curve_stats(out: &mut Out, runs: &[&[Point]], dir: Direction, which: usize) {
    let npoints = runs[0].len().min(MAX_CURVE_POINTS);
    out.begin_arr();
    for i in 0..npoints {
        let mut ys = [0f64; MAX_RUNS];
        let mut m = 0;
        for r in runs {
            if i < r.len() {
                ys[m] = r[i].y;
                m += 1;
            }
        }
        let s = stats::compute(&ys[..m], dir);
        out.elem();
        out.begin_obj();
        out.key_f64("x", runs[0][i].x);
        out.key_f64(
            "y",
            match which {
                0 => s.median,
                1 => s.best,
                2 => s.worst,
                _ => s.variation_pct,
            },
        );
        out.end();
    }
    out.end();
}

pub fn write_benchmark_list(out: &mut Out, ctx: &Ctx) {
    out.begin_obj();
    out.key_str("benchmark_suite_version", SUITE_VERSION);
    out.key("binary_sha256");
    match ctx.self_image {
        Some(img) => out.sha256(&img.whole_hash()),
        None => out.null(),
    }
    out.key("benchmarks");
    out.begin_arr();
    for b in crate::bench::registry() {
        out.elem();
        let meta = b.meta();
        out.begin_obj();
        out.key_str("id", meta.id);
        out.key_u64("version", meta.version as u64);
        out.key_str("description", meta.description);
        out.key_str("source", meta.imp.source);
        out.key_str("source_revision", env!("VMBENCH_SOURCE_REVISION"));
        out.key_str("kernel", meta.imp.kernel);
        out.key_str("algorithm", meta.imp.algorithm);
        out.key_str("isa", meta.imp.isa);
        out.key("code_hash");
        match ctx.code_hash(meta.imp.kernel) {
            Some(h) => out.sha256(&h),
            None => out.null(),
        }
        out.key_str(
            "verification",
            crate::bench::verify::verification_kind(meta.id),
        );
        out.end();
    }
    out.end();
    out.end();
}
