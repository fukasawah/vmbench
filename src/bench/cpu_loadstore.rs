use crate::bench::common::*;
use crate::bench::Benchmark;
use crate::runner::{Ctx, EntryMeta, ImplInfo, ParamValue};
use crate::util::Arena;

pub const L1_BYTES: usize = 16 * 1024;
pub const L1_ENTRIES: usize = L1_BYTES / 8;

#[inline(never)]
#[no_mangle]
pub unsafe extern "C" fn vmbench_k_l1_load_tp(iters: u64, buf: usize) -> u64 {
    let p = buf as *const u64;
    let mut acc = 0u64;
    let mut i = 0u64;
    while i < iters {
        acc = acc.wrapping_add(core::ptr::read_volatile(p.add(((i * 8 + 0) & 2047) as usize)));
        acc = acc.wrapping_add(core::ptr::read_volatile(p.add(((i * 8 + 1) & 2047) as usize)));
        acc = acc.wrapping_add(core::ptr::read_volatile(p.add(((i * 8 + 2) & 2047) as usize)));
        acc = acc.wrapping_add(core::ptr::read_volatile(p.add(((i * 8 + 3) & 2047) as usize)));
        acc = acc.wrapping_add(core::ptr::read_volatile(p.add(((i * 8 + 4) & 2047) as usize)));
        acc = acc.wrapping_add(core::ptr::read_volatile(p.add(((i * 8 + 5) & 2047) as usize)));
        acc = acc.wrapping_add(core::ptr::read_volatile(p.add(((i * 8 + 6) & 2047) as usize)));
        acc = acc.wrapping_add(core::ptr::read_volatile(p.add(((i * 8 + 7) & 2047) as usize)));
        i += 1;
    }
    core::hint::black_box(acc)
}

#[inline(never)]
#[no_mangle]
pub unsafe extern "C" fn vmbench_k_l1_store_tp(iters: u64, buf: usize) -> u64 {
    let p = buf as *mut u64;
    let mut i = 0u64;
    while i < iters {
        core::ptr::write_volatile(p.add(((i * 8 + 0) & 2047) as usize), i);
        core::ptr::write_volatile(p.add(((i * 8 + 1) & 2047) as usize), i);
        core::ptr::write_volatile(p.add(((i * 8 + 2) & 2047) as usize), i);
        core::ptr::write_volatile(p.add(((i * 8 + 3) & 2047) as usize), i);
        core::ptr::write_volatile(p.add(((i * 8 + 4) & 2047) as usize), i);
        core::ptr::write_volatile(p.add(((i * 8 + 5) & 2047) as usize), i);
        core::ptr::write_volatile(p.add(((i * 8 + 6) & 2047) as usize), i);
        core::ptr::write_volatile(p.add(((i * 8 + 7) & 2047) as usize), i);
        i += 1;
    }
    core::hint::black_box(i)
}

#[inline(never)]
#[no_mangle]
pub unsafe extern "C" fn vmbench_k_l1_chase_lat(iters: u64, start: usize) -> u64 {
    let mut p = start as *const u64;
    let mut i = 0u64;
    while i < iters {
        p = core::ptr::read_volatile(p) as *const u64;
        i += 1;
    }
    p as u64
}

pub type LsKernel = unsafe extern "C" fn(u64, usize) -> u64;

#[used]
static LS_KERNELS: [LsKernel; 3] = [
    vmbench_k_l1_load_tp,
    vmbench_k_l1_store_tp,
    vmbench_k_l1_chase_lat,
];

static LS_NAMES: [&str; 3] = [
    "vmbench_k_l1_load_tp",
    "vmbench_k_l1_store_tp",
    "vmbench_k_l1_chase_lat",
];

pub fn make_l1_buffer(arena: &mut Arena, rng: &mut crate::rand::Rng) -> (&'static mut [u8], usize) {
    let buf = unsafe { alloc_buffer(arena, L1_BYTES, 64) }.unwrap_or_else(|| {
        crate::rt::fatal("arena exhausted for L1 buffer");
    });
    let start = build_pointer_cycle(arena, buf.as_mut_ptr(), L1_ENTRIES, 8, rng);
    (buf, start)
}

macro_rules! ls_bench {
    ($struct_name:ident, $id:literal, $idx:literal, $kind:literal, $units:expr, $desc:literal, $algo:literal) => {
        pub struct $struct_name;
        impl Benchmark for $struct_name {
            fn meta(&self) -> &'static EntryMeta {
                static M: EntryMeta = EntryMeta {
                    id: $id,
                    version: 1,
                    description: $desc,
                    imp: ImplInfo {
                        source: "src/bench/cpu_loadstore.rs",
                        kernel: LS_NAMES[$idx],
                        algorithm: $algo,
                        isa: BASELINE_ISA,
                    },
                    metrics: &[M_OPS, M_NS_OP, M_CYC_OP],
                };
                &M
            }
            fn run(&self, ctx: &mut Ctx, entry: usize) -> Result<(), &'static str> {
                let cpu = ctx.default_cpu();
                ctx.pin_self(cpu);
                ctx.add_param(entry, "kind", ParamValue::Str($kind));
                ctx.add_param(entry, "working_set_bytes", ParamValue::Int(L1_BYTES as i64));
                ctx.add_param(entry, "cpu", ParamValue::Int(cpu as i64));
                let mut rng = crate::rand::Rng::new(0xfeed_face_cafe_beef);
                let (buf, start) = make_l1_buffer(ctx.arena, &mut rng);
                let ptr = buf.as_mut_ptr() as usize;
                let start_ptr = ptr + start;
                ctx.measure(
                    entry,
                    ctx.cfg.target_run_ns,
                    ctx.cfg.runs,
                    |iters| {
                        let _ = call_ls(&LS_KERNELS, $idx, iters, if $idx == 2 { start_ptr } else { ptr });
                        if $idx == 2 {
                            iters
                        } else {
                            iters.saturating_mul($units)
                        }
                    },
                    |ctx, e, s| emit_rate(ctx, e, s),
                );
                Ok(())
            }
        }
    };
}

#[inline(never)]
pub fn call_ls(table: &'static [LsKernel], idx: usize, iters: u64, arg: usize) -> u64 {
    let f = unsafe { core::ptr::read_volatile(&table[idx]) };
    unsafe { f(iters, arg) }
}

ls_bench!(
    L1LoadTp,
    "cpu.loadstore.load.v1",
    0,
    "load",
    8,
    "L1 load throughput (16 KiB working set)",
    "8 independent volatile u64 loads per iteration"
);
ls_bench!(
    L1StoreTp,
    "cpu.loadstore.store.v1",
    1,
    "store",
    8,
    "L1 store throughput (16 KiB working set)",
    "8 independent volatile u64 stores per iteration"
);
ls_bench!(
    L1ChaseLat,
    "cpu.loadstore.load_to_use.v1",
    2,
    "load_to_use",
    1,
    "L1 dependent load latency (pointer chase, 16 KiB)",
    "single dependent pointer chase over a random cycle in 16 KiB"
);
