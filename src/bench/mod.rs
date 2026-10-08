pub mod cache_bw;
pub mod common;
pub mod cpu_atomic;
pub mod cpu_branch;
pub mod cpu_core2core;
pub mod cpu_fp;
pub mod cpu_int;
pub mod cpu_isa;
pub mod cpu_loadstore;
pub mod cpu_mix;
pub mod cpu_multicore;
pub mod cpu_overhead;
pub mod cpu_sustained;
pub mod linux;
pub mod mem_bw;
pub mod mem_latency;
pub mod perf;
pub mod storage;
pub mod uring;
pub mod verify;

use crate::runner::{Ctx, EntryMeta};

pub trait Benchmark: Sync {
    fn meta(&self) -> &'static EntryMeta;
    fn supported(&self, _ctx: &Ctx) -> Result<(), &'static str> {
        Ok(())
    }
    /// Number of result entries this benchmark produces. Storage benchmarks
    /// return one per configured storage target; everything else returns 1.
    fn instances(&self, _ctx: &Ctx) -> usize {
        1
    }
    fn run(&self, ctx: &mut Ctx, entry: usize) -> Result<(), &'static str>;
    /// Runs instance `instance` into its own entry. The default implementation
    /// forwards to `run` (single-instance benchmarks).
    fn run_at(&self, ctx: &mut Ctx, entry: usize, instance: usize) -> Result<(), &'static str> {
        let _ = instance;
        self.run(ctx, entry)
    }
    fn quick_skip(&self) -> bool {
        false
    }
}

static ALL: &[&dyn Benchmark] = &[
    &cpu_overhead::LoopOverhead,
    &cpu_int::IntTpAdd,
    &cpu_int::IntTpSub,
    &cpu_int::IntTpXor,
    &cpu_int::IntTpAnd,
    &cpu_int::IntTpOr,
    &cpu_int::IntTpMul,
    &cpu_int::IntTpRot,
    &cpu_int::IntTpShl,
    &cpu_int::IntLatAdd,
    &cpu_int::IntLatMul,
    &cpu_int::IntLatDiv,
    &cpu_int::IntLatRot,
    &cpu_int::IntLatXor,
    &cpu_int::IntLatShl,
    &cpu_fp::Fp64AddTp,
    &cpu_fp::Fp64MulTp,
    #[cfg(target_arch = "x86_64")]
    &cpu_fp::Fp32AddTp,
    #[cfg(target_arch = "x86_64")]
    &cpu_fp::Fp32MulTp,
    &cpu_fp::Fp64FmaTp,
    #[cfg(target_arch = "x86_64")]
    &cpu_fp::Fp32FmaTp,
    &cpu_fp::Fp64AddLat,
    &cpu_fp::Fp64MulLat,
    &cpu_fp::Fp64DivLat,
    &cpu_fp::Fp64SqrtLat,
    &cpu_fp::Fp32AddLat,
    &cpu_fp::Fp32MulLat,
    &cpu_fp::Fp32DivLat,
    &cpu_fp::Fp32SqrtLat,
    &cpu_mix::MixIter,
    &cpu_branch::BranchPred,
    &cpu_branch::BranchRand,
    &cpu_branch::BranchInd,
    &cpu_loadstore::L1LoadTp,
    &cpu_loadstore::L1StoreTp,
    &cpu_loadstore::L1ChaseLat,
    &cpu_multicore::MultiCore1,
    &cpu_multicore::MultiCore2,
    &cpu_multicore::MultiCore4,
    &cpu_multicore::MultiCore8,
    &cpu_multicore::MultiCoreAll,
    &cpu_atomic::AtomicAdd1,
    &cpu_atomic::AtomicXchg1,
    &cpu_atomic::AtomicCas1,
    &cpu_atomic::AtomicAdd2,
    &cpu_atomic::AtomicAddAll,
    &cpu_atomic::AtomicCas2,
    &cpu_atomic::AtomicCasAll,
    &cpu_core2core::Core2Core01,
    &cpu_core2core::Core2Core02,
    &cpu_core2core::Core2CoreHalf,
    &cpu_core2core::Core2CoreLast,
    &cpu_core2core::Core2CoreSiblings,
    &cpu_sustained::Sustained,
    #[cfg(target_arch = "x86_64")]
    &cpu_isa::IsaSse2Add,
    #[cfg(target_arch = "x86_64")]
    &cpu_isa::IsaAvxAdd,
    #[cfg(target_arch = "x86_64")]
    &cpu_isa::IsaAvx2Add,
    #[cfg(target_arch = "x86_64")]
    &cpu_isa::IsaFma,
    #[cfg(target_arch = "x86_64")]
    &cpu_isa::IsaAvx512Add,
    #[cfg(target_arch = "x86_64")]
    &cpu_isa::IsaAes,
    #[cfg(target_arch = "x86_64")]
    &cpu_isa::IsaSha,
    #[cfg(target_arch = "x86_64")]
    &cpu_isa::IsaCrc32,
    #[cfg(target_arch = "aarch64")]
    &cpu_isa::IsaNeonAdd,
    &mem_latency::CacheLatencyCurve,
    &mem_latency::DramLatency,
    &mem_latency::Mlp,
    &mem_latency::TlbCurve,
    &mem_latency::PageSize,
    &mem_latency::NumaLatency,
    &mem_bw::BwRead1,
    &mem_bw::BwReadAll,
    &mem_bw::BwWrite1,
    &mem_bw::BwWriteAll,
    &mem_bw::BwCopy1,
    &mem_bw::BwCopyAll,
    &mem_bw::BwTriad1,
    &mem_bw::BwTriadAll,
    &mem_bw::BwRandomRead,
    &mem_bw::BwRandomWrite,
    &mem_bw::BwScale1,
    &mem_bw::BwScaleAll,
    &mem_bw::BwAdd1,
    &mem_bw::BwAddAll,
    &cache_bw::CacheL1Read,
    &cache_bw::CacheL1Write,
    &cache_bw::CacheL1Copy,
    &cache_bw::CacheL2Read,
    &cache_bw::CacheL2Write,
    &cache_bw::CacheL2Copy,
    &cache_bw::CacheLlcRead,
    &cache_bw::CacheLlcWrite,
    &cache_bw::CacheLlcCopy,
    &linux::SyscallGetpid,
    &linux::ContextSwitchFutex,
    &linux::WakeupFutex,
    &linux::SchedulerJitter,
    &linux::PageFaultMinor,
    &perf::PerfCounters,
    &storage::StorageSeqRead,
    &storage::StorageSeqWrite,
    &storage::StorageSyncFdatasync,
    &storage::StorageSyncDsync,
    &storage::StorageStreamsRead,
    &storage::StorageStreamsWrite,
    &storage::StorageStreamsMixed,
    &storage::StorageUringRead,
    &storage::StorageUringWrite,
    &storage::StorageUringMixed,
    &storage::StorageSustainedWrite,
    &storage::StorageBufferedWarm,
    #[cfg(target_arch = "aarch64")]
    &cpu_isa::IsaAes,
    #[cfg(target_arch = "aarch64")]
    &cpu_isa::IsaSha,
    #[cfg(target_arch = "aarch64")]
    &cpu_isa::IsaCrc32,
];

pub fn registry() -> &'static [&'static dyn Benchmark] {
    ALL
}
