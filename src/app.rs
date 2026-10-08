use crate::elfself::SelfImage;
use crate::env;
use crate::json::Out;
use crate::runner::{self, Config, Ctx, StorageTarget, Vec32, MAX_STORAGE_TARGETS};
use crate::sys;
use crate::time;
use crate::util::Arena;

static mut ARENA: Option<Arena> = None;
static mut QUIET: bool = false;

const HELP: &[u8] = b"vmbench - libc-free benchmark for Linux VPS / cloud VMs

Usage: vmbench [OPTIONS]

Options:
  --output <PATH>      write JSON report to PATH (default: stdout)
  --only <PREFIX,...>  run only benchmarks whose id starts with a prefix
  --skip <PREFIX,...>  skip benchmarks whose id starts with a prefix
  --quick              shorter measurement budget
  --pin <CPU>          pin single-threaded benchmarks to this CPU
  --no-pin             do not pin single-threaded benchmarks
  --sustained <SEC>    sustained-load measurement duration (default 10)
  --storage <NAME=PATH>  add a storage measurement target (repeatable).
                       PATH must be an existing directory on a mounted
                       filesystem; vmbench never formats or mounts anything.
                       Without --storage, a 'cwd' target is created in the
                       current directory (vmbench-<id>/).
  --no-cwd-storage     do not add the implicit cwd target
  --meta <JSON>        attach a JSON object as user metadata, e.g.
                       --meta '{\"service\":\"Azure VM\",\"series\":\"D\",\"version\":\"v5\"}'
  --list               print the benchmark registry as JSON and exit
  --quiet              suppress progress output on stderr
  -h, --help           print this help
  -V, --version        print version
";

fn valid_target_name(name: &str) -> bool {
    if name.is_empty() || name.len() > 32 {
        return false;
    }
    name.bytes().all(|b| {
        b.is_ascii_alphanumeric() || b == b'_' || b == b'.' || b == b'-'
    })
}

fn add_storage_target(cli: &mut Cli, arg: &'static str) {
    let Some(eq) = arg.find('=') else {
        let _ = sys::write_all(2, b"vmbench: --storage expects NAME=PATH\n");
        sys::exit(2);
    };
    let name = &arg[..eq];
    let path = &arg[eq + 1..];
    if !valid_target_name(name) {
        let _ = sys::write_all(2, b"vmbench: invalid storage target name (use [A-Za-z0-9_.-], max 32 chars)\n");
        sys::exit(2);
    }
    if path.is_empty() || path.len() > 3800 {
        let _ = sys::write_all(2, b"vmbench: invalid storage target path\n");
        sys::exit(2);
    }
    if cli.cfg.n_storage_targets >= MAX_STORAGE_TARGETS {
        let _ = sys::write_all(2, b"vmbench: too many --storage targets (max ");
        let mut tmp = [0u8; 4];
        let n = crate::fmt::u64_dec(MAX_STORAGE_TARGETS as u64, &mut tmp);
        let _ = sys::write_all(2, &tmp[..n]);
        let _ = sys::write_all(2, b")\n");
        sys::exit(2);
    }
    for t in &cli.cfg.storage_targets[..cli.cfg.n_storage_targets] {
        if t.name == name {
            let _ = sys::write_all(2, b"vmbench: duplicate storage target name: ");
            let _ = sys::write_all(2, name.as_bytes());
            let _ = sys::write_all(2, b"\n");
            sys::exit(2);
        }
    }
    cli.cfg.storage_targets[cli.cfg.n_storage_targets] = StorageTarget {
        name,
        path,
        auto_dir: false,
    };
    cli.cfg.n_storage_targets += 1;
}

pub fn progress_step(id: &str) {
    if unsafe { QUIET } {
        return;
    }
    let _ = sys::write_all(2, b"vmbench: running ");
    let _ = sys::write_all(2, id.as_bytes());
    let _ = sys::write_all(2, b"\n");
}

fn arena_init() -> &'static mut Arena {
    unsafe {
        let slot = &mut *core::ptr::addr_of_mut!(ARENA);
        *slot = Arena::new(512 * 1024 * 1024);
        match slot {
            Some(a) => a,
            None => crate::rt::fatal("failed to mmap arena"),
        }
    }
}

struct Cli {
    cfg: Config,
    output: Option<&'static str>,
    meta: Option<&'static str>,
    list: bool,
    help: bool,
    version: bool,
}

fn parse_args(arena: &mut Arena) -> Cli {
    let mut cli = Cli {
        cfg: Config::default(),
        output: None,
        meta: None,
        list: false,
        help: false,
        version: false,
    };
    let mut i = 1usize;
    while let Some(arg) = crate::rt::argv(i) {
        let s = crate::util::bytes_to_str(arg);
        let mut next_value = |name: &str| -> &'static str {
            i += 1;
            match crate::rt::argv(i) {
                Some(v) => unsafe { arena.copy_str(v) }.unwrap_or(""),
                None => {
                    let _ = sys::write_all(2, b"vmbench: missing value for ");
                    let _ = sys::write_all(2, name.as_bytes());
                    let _ = sys::write_all(2, b"\n");
                    sys::exit(2);
                }
            }
        };
        match s {
            "--output" | "-o" => cli.output = Some(next_value("--output")),
            "--only" => cli.cfg.only = Some(next_value("--only")),
            "--skip" => cli.cfg.skip = Some(next_value("--skip")),
            "--quick" => cli.cfg.quick = true,
            "--list" => cli.list = true,
            "--quiet" => cli.cfg.no_progress = true,
            "--no-pin" => cli.cfg.no_pin = true,
            "--pin" => {
                let v = next_value("--pin");
                cli.cfg.pin_cpu = crate::util::parse_u64(v.as_bytes()).map(|x| x as u32);
            }
            "--sustained" => {
                let v = next_value("--sustained");
                if let Some(sec) = crate::util::parse_u64(v.as_bytes()) {
                    cli.cfg.sustained_ns = sec.saturating_mul(1_000_000_000).max(1_000_000_000);
                }
            }
            "--meta" => cli.meta = Some(next_value("--meta")),
            "--storage" => {
                let v = next_value("--storage");
                add_storage_target(&mut cli, v);
            }
            "--no-cwd-storage" => cli.cfg.no_cwd_storage = true,
            "-h" | "--help" => cli.help = true,
            "-V" | "--version" => cli.version = true,
            other => {
                let _ = sys::write_all(2, b"vmbench: unknown option: ");
                let _ = sys::write_all(2, other.as_bytes());
                let _ = sys::write_all(2, b"\n\n");
                let _ = sys::write_all(2, HELP);
                sys::exit(2);
            }
        }
        i += 1;
    }
    if cli.cfg.quick {
        cli.cfg.target_run_ns = 80_000_000;
        cli.cfg.max_bench_ns = 3_000_000_000;
        cli.cfg.sustained_ns = cli.cfg.sustained_ns.min(3_000_000_000);
        cli.cfg.runs = 1;
    }

    // Implicit cwd target unless the user excluded it or named one explicitly.
    let mut has_cwd = false;
    for t in &cli.cfg.storage_targets[..cli.cfg.n_storage_targets] {
        if t.name == "cwd" {
            has_cwd = true;
        }
    }
    if !cli.cfg.no_cwd_storage && !has_cwd && cli.cfg.n_storage_targets < MAX_STORAGE_TARGETS {
        cli.cfg.storage_targets[cli.cfg.n_storage_targets] = StorageTarget {
            name: "cwd",
            path: "",
            auto_dir: true,
        };
        cli.cfg.n_storage_targets += 1;
    }
    cli
}

fn build_code_hashes(
    arena: &mut Arena,
    image: Option<&SelfImage>,
) -> &'static [(&'static str, Option<[u8; 32]>)] {
    let reg = crate::bench::registry();
    let list: &'static mut [(&'static str, Option<[u8; 32]>)] =
        unsafe { arena.alloc_slice(reg.len().max(1)) }.unwrap();
    let mut n = 0usize;
    'outer: for b in reg {
        let k = b.meta().imp.kernel;
        for item in list[..n].iter() {
            if item.0 == k {
                continue 'outer;
            }
        }
        let h = image.and_then(|i| i.symbol_hash(k.as_bytes()).map(|(h, _)| h));
        list[n] = (k, h);
        n += 1;
    }
    &list[..n]
}

fn output_buffer() -> Out {
    let len = 128 * 1024 * 1024;
    match sys::mmap_anon(len, false) {
        Some(p) => unsafe { Out::new(p, len) },
        None => crate::rt::fatal("failed to mmap output buffer"),
    }
}

fn finish(out: &Out, path: Option<&str>) -> ! {
    if let Some(p) = path {
        let fd = match sys::open(
            p.as_bytes(),
            sys::O_WRONLY | sys::O_CREAT | sys::O_TRUNC | sys::O_CLOEXEC,
            0o644,
        ) {
            Ok(fd) => fd,
            Err(e) => {
                let _ = sys::write_all(2, b"vmbench: cannot open output file: ");
                let _ = sys::write_all(2, sys::err_name(e).as_bytes());
                let _ = sys::write_all(2, b"\n");
                sys::exit(1);
            }
        };
        if let Err(e) = sys::write_all(fd, out.as_slice()) {
            let _ = sys::write_all(2, b"vmbench: write failed: ");
            let _ = sys::write_all(2, sys::err_name(e).as_bytes());
            let _ = sys::write_all(2, b"\n");
            sys::exit(1);
        }
        let _ = sys::close(fd);
        if unsafe { !QUIET } {
            let _ = sys::write_all(2, b"vmbench: wrote ");
            let mut tmp = [0u8; 20];
            let n = crate::fmt::u64_dec(out.len() as u64, &mut tmp);
            let _ = sys::write_all(2, &tmp[..n]);
            let _ = sys::write_all(2, b" bytes to ");
            let _ = sys::write_all(2, p.as_bytes());
            let _ = sys::write_all(2, b"\n");
        }
    } else if let Err(e) = sys::write_all(1, out.as_slice()) {
        let _ = sys::write_all(2, b"vmbench: write failed: ");
        let _ = sys::write_all(2, sys::err_name(e).as_bytes());
        let _ = sys::write_all(2, b"\n");
        sys::exit(1);
    }
    sys::exit(0)
}

pub fn run() -> ! {
    time::calibrate_cycles();
    let arena = arena_init();
    let cli = parse_args(arena);
    if cli.help {
        let _ = sys::write_all(1, HELP);
        sys::exit(0);
    }
    if cli.version {
        let _ = sys::write_all(1, b"vmbench ");
        let _ = sys::write_all(1, crate::output::SUITE_VERSION.as_bytes());
        let _ = sys::write_all(1, b"\n");
        sys::exit(0);
    }
    if let Some(m) = cli.meta {
        if let Err(e) = crate::meta::validate_object(m) {
            let _ = sys::write_all(2, b"vmbench: ");
            let _ = sys::write_all(2, e.as_bytes());
            let _ = sys::write_all(2, b"\n");
            sys::exit(2);
        }
    }
    unsafe {
        QUIET = cli.cfg.no_progress;
    }

    let started_realtime = time::realtime_ns();
    let started_mono = time::mono_ns();

    let image_owned = SelfImage::load();
    let image: Option<&'static SelfImage> = match image_owned {
        Some(img) => unsafe { arena.boxed(img).map(|r| &*r) },
        None => None,
    };
    let env = env::collect(arena);
    let code_hashes = build_code_hashes(arena, image);
    let annotations = unsafe {
        arena
            .boxed(Vec32 {
                items: [""; 32],
                len: 0,
            })
            .unwrap()
    };
    let mut ctx = Ctx {
        cfg: cli.cfg.clone(),
        arena,
        env,
        n_entries: 0,
        code_hashes,
        self_image: image,
        annotations,
        progress: !cli.cfg.no_progress,
        loop_overhead_cycles: None,
        storage: core::array::from_fn(|_| None),
        storage_state: [crate::bench::storage::TargetState::default(); MAX_STORAGE_TARGETS],
    };

    if cli.list {
        let mut out = output_buffer();
        crate::output::write_benchmark_list(&mut out, &ctx);
        finish(&out, cli.output);
    }

    runner::run_all(&mut ctx);
    crate::bench::storage::cleanup(&mut ctx);

    let duration = time::mono_ns().wrapping_sub(started_mono);
    let mut out = output_buffer();
    let binary_hash = ctx.self_image.map(|i| i.whole_hash());
    crate::output::write_report(
        &mut out,
        &ctx,
        started_realtime,
        duration,
        binary_hash,
        cli.meta,
    );
    finish(&out, cli.output);
}
