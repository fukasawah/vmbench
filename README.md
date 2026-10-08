# vmbench

Linux VPS / クラウド VM の実性能を測定し、CPU・キャッシュ・メモリの特性を
JSON で出力するベンチマークツールです。glibc / musl などの libc 実装差が
結果に混入しないことを最優先し、**libc を一切リンクしない no_std バイナリ**
として動作します。benchmark kernel は libc を呼ばず、heap 確保・formatting・
filesystem access も行いません。

- 対象: Linux x86_64 / AArch64 (VPS・クラウド VM を主対象、bare metal も可)
- 非依存: 外部コマンド、ネットワーク、provider API、IMDS を使用しない
- 権限: 原則 root 不要
- 配布: tar.gz を展開し `./vmbench` を実行するだけ

## 使い方

```bash
./vmbench                       # JSON を標準出力へ
./vmbench --output result.json  # JSON をファイルへ
./vmbench --quick               # 短時間モード
./vmbench --only cpu.int,memory # id prefix で絞り込み
./vmbench --skip cpu.sustained  # id prefix で除外
./vmbench --pin 0               # 単一スレッド系を CPU 0 に固定
./vmbench --sustained 30        # 継続負荷測定の秒数
./vmbench --storage eps1=/mnt/ephemeral-storage-1/tmp
                                # ストレージ計測先を追加 (繰り返し可)
./vmbench --no-cwd-storage      # 暗黙の cwd 計測先を追加しない
./vmbench --list                # benchmark レジストリ (code hash 付き) を出力
./vmbench --meta '{"service":"Azure VM","series":"D","version":"v5"}'
```

主なオプション: `--output` `--only` `--skip` `--quick` `--pin` `--no-pin`
`--sustained` `--storage` `--no-cwd-storage` `--meta` `--list` `--quiet`
`--help` `--version`

ストレージ計測先:
- `--storage NAME=PATH` で任意のマウント済みディレクトリを計測対象にできる
  (Azure の揮発性ローカルストレージ等)。NAME は `[A-Za-z0-9_.-]` 32 文字以内、
  最大 8 個
- 指定がない場合は `cwd` ターゲットが暗黙に追加され、カレントディレクトリに
  `vmbench-<id>/` という作業ディレクトリを作って計測し、終了時に削除する。
  `--storage` を指定しても暗黙の `cwd` は残る (`--no-cwd-storage` で除外)
- ファイルシステムは**事前に用意されたものを前提**とし、vmbench は
  mkfs・mount を行わない。カレントディレクトリ配下の作業ディレクトリ作成
  (暗黙 `cwd` のみ) 以外にディレクトリを作らない
- パスは全ベンチマーク開始前に検証される (存在、ディレクトリ、書き込み可否)。
  ブロック/キャラクタデバイスの直接指定は拒否する。実行中に対象が消えた場合は
  該当ストレージ benchmark の entry のみ `failed` になる
- 計測先は結果 JSON の `storage.targets[]` と各 entry の `parameters.target` に
  現れる

進捗は stderr に出力されます。stdout は常に JSON です。

`--meta` には JSON オブジェクトを渡します。値は任意の JSON（ネスト可）で、
検証後にそのまま `meta` として出力されます。サービス名・シリーズ・世代など、
後で結果をグループ化するための情報を想定しています。CPU コア数・メモリ量・
CPU 型番などは環境情報として別途自動記録されるため、`--meta` には入れません。
秘密情報は入れないでください（結果 JSON にそのまま含まれます）。

## 出力

トップレベル:

```text
schema_version / benchmark_suite_version / meta (--meta 指定時のみ) / binary_sha256 / build
run_id / run_identity / instance_identity / environment_identity / boot_identity
started_at / duration
system / cpu / memory / storage / noise
annotations / benchmarks[]
```

各 benchmark:

```json
{
  "id": "cpu.int.latency.v1",
  "version": 1,
  "description": "...",
  "implementation": {
    "source": "src/bench/cpu_int.rs",
    "kernel": "vmbench_k_int_add_lat",
    "algorithm": "...",
    "isa": "x86_64-baseline",
    "code_hash": "sha256:...",
    "code_hash_status": "ok"
  },
  "parameters": { "op": "add", "chain": 1, "cpu": 0 },
  "metrics": [ { "key": "...", "unit": "...", "direction": "...", "kind": "..." } ],
  "status": "ok",
  "runs": [ { "duration_ns": 0, "work_units": 0, "chunk_rate": {}, "values": {}, "curves": {} } ],
  "median": {}, "best": {}, "worst": {}, "variation_pct": {}, "count": {}
}
```

- 各 benchmark は独立した run を通常 5 回（`--quick` では 1 回）反復し、
  raw run をすべて保持します。
- 通常比較値は `median`、peak は `best`、`variation_pct` は
  `(max - min) / median * 100` です。
- 未対応の CPU 機能は `status: "unsupported"` と理由を記録します。
- 取得できない環境情報は `null` と `unavailable` リストで表現します。

## 測定方式

各 run は**時間予算方式**です。まず目標時間（通常 300ms、`--quick` では
80ms）の約 1/8 で終わる work chunk を実測で較正し、目標時間が経過するまで
chunk を繰り返して work unit を数えます。run の wall-clock 時間は構造的に
上限が決まるため、競合や noisy neighbor で遅い環境でも暴走しません。

- `runs[].duration_ns` は run 全体の実測時間、`runs[].work_units` は
  その間に実行した work unit（演算数・バイト数など）です。
- ループ内の時間判定は `rdtsc` / `cntvct_el0`（取得できない場合は
  `clock_gettime`）で行い、オーバーヘッドは無視できる範囲に収めています。
- `runs[].chunk_rate` に run 内 chunk ごとのレート分布（min/median/max）を
  記録します。throttling や外れ値の兆候を追加コストなしで確認できます。
- 依存チェーン型の latency 系は生値のみです。展開済み throughput 系には
  補助指標 `cycles_per_op_net`（空ループ実測値 `cpu.overhead.loop.v1` を
  unroll 数で割って差し引いた近似値）を併記します。生の
  `cycles_per_op` が主指標です。

## プライバシー

結果JSONに含めないもの:

- hostname の生値（`instance_identity` の SHA-256 のみ）
- `/etc/machine-id` の生値（SHA-256 のみ）
- DMI product UUID の生値（SHA-256 のみ）
- cgroup のパス（制限値 `cpu.max` 等のみ）

結果JSONに含まれるもの: カーネル/ディストリ情報、CPU の型番・トポロジ、
メモリ量、マウント情報、block device 名など、**マシン構成に関する情報**です。
マウント元にはネットワーク共有名などが含まれ得ます。公開範囲は利用側で
判断してください。

## Benchmark カテゴリ

- `cpu.int.*` 整数 throughput / latency
- `cpu.fp.*` 浮動小数点 throughput / latency
- `cpu.mix.v1` 固定複合 workload
- `cpu.branch.v1` 分岐 (predictable / unpredictable / indirect)
- `cpu.loadstore.v1` L1 load/store/load-to-use
- `cpu.isa.*` ISA 別 throughput
  (x86: SSE2/AVX/AVX2/FMA/AVX-512/AES/SHA/CRC32、
  AArch64: NEON/AES/SHA-256/CRC32。未対応は `unsupported`)
- `cpu.multicore.v1` 同一 kernel の 1/2/4/8/all スケーリング
- `cpu.atomic.v1` atomic 加算 / exchange / CAS (uncontended / contended)
- `cpu.core2core.v1` cache line ping-pong レイテンシ (CPU pair 別)
- `cpu.sustained.v1` 継続負荷中の throughput 変動 (時間依存、通常スコアと分離)
- `cpu.overhead.loop.v1` 空ループ（throughput系の `cycles_per_op_net` の基準）
- `cache.latency.curve.v1` working set sweep による pointer-chase latency curve
- `cache.bandwidth.{l1,l2,llc}.{read,write,copy}.v1` cache階層別帯域
- `memory.latency.dram.v1` DRAM レイテンシ
- `memory.mlp.v1` Memory-Level Parallelism (独立 chain 数 sweep)
- `memory.tlb.v1` page-granular 依存 pointer chase（page-walk コスト）。
  同一ページ集合を独立ロードで走る参考カーブ `ns_per_access_independent` も出力
  （MLPがpage walkを隠すため、両者の差がTLB効果を示す）
- `memory.page_size.v1` 4 KiB vs THP
- `memory.numa.v1` NUMA local/remote レイテンシ (topology 公開時のみ)
- `memory.bandwidth.seq.v1` read/write/copy/triad/scale/add (single / all threads)。
  ワーキングセットは総LLCの4倍以上（RAMの1/3で上限）。all threadsは
  全スレッドが同一リージョンを位相をずらしてストリームする
- `memory.bandwidth.random.v1` cache-line 単位の pseudo-random access

### ストレージ

各計測先ディレクトリに `vmbench-<target>-<pid>.bin`（通常 256 MiB〜1 GiB、
`--quick` では 64〜256 MiB）を作成し、pseudo-random な実データで埋めてから
測定します。終了時に削除します（強制終了された場合は残ることがあります）。
raw block device へは書き込まず、ファイルシステム経由でのみ測定します。

測定方式は2系統あり、**同一 benchmark の全点を同一方式で測ります**。

- `storage.streams.{read,write,mixed70_30}.4k.sweep.v1`
  並列 blocking IO。同時プロセス数 1, 4, 16 の3点。ホストのスケジューラと
  コア数に依存するため io_uring とは比較しない
- `storage.uring.{read,write,mixed70_30}.4k.sweep.v1`
  単一プロセスの io_uring。in-flight QD 1, 2, 4, 8, 16, 32 の6点。
  io_uring が使えない環境では `unsupported`（フォールバックしない）
- 各点は `iops` / `bytes_per_sec` / tail latency
  (mean/p50/p95/p99/p99.9/max) を curve として返す。QD16 は曲線上の1点
- `storage.seq.read/write.v1` 1 MiB sequential (QD1)
- `storage.sync.fdatasync.4k.v1` / `storage.sync.dsync.4k.v1` durability コスト
- `storage.sustained.write.v1` 継続書き込み（時間依存、通常と分離）
- `storage.buffered.read.warm.v1` page cache 参考値

主比較値は Direct I/O です。O_DIRECT 非対応のファイルシステムでは
buffered にフォールバックし、annotation を付けます。guest page cache は
制御できますが provider 側 cache・burst credit は制御できないため、
結果には annotation を付けています。

測定区間について: latency サンプルは run 全体へ均一に間引き記録し、
サンプル数上限で run を打ち切ることはありません。sampling 系の run は
時間予算の半分未満で終わった場合 `failed` になります (fail-closed)。

### Linux / scheduler 診断

CPUスコアとは別分類です。hypervisor / noisy neighbor / scheduler 特性を
見るための値です。

- `linux.syscall.getpid.v1` syscall round-trip
- `linux.ctxswitch.futex.v1` futex ping-pong の context switch コスト
- `linux.wakeup.futex.v1` futex wakeup latency 分布
- `linux.scheduler.jitter.v1` nanosleep 間隔の jitter
- `linux.pagefault.minor.v1` anonymous first-touch の minor fault コスト

### Performance counter

- `perf.counters.v1` perf_event の cycles / instructions / IPC / branches /
  branch miss / cache references / cache miss。PMU 非公開環境では
  `unsupported` になります（総合スコアには使用しません）。

`--list` と `docs/BENCHMARKS.md` に全 benchmark の
`id / source / kernel / algorithm / isa / code_hash` を出力します。

## 比較

```bash
tools/summary.py result.json                    # 代表指標の要約
tools/summary.py --baseline ref.json result.json  # カテゴリ指数 (幾何平均比)
```

代表指標だけを人間単位で一覧し、A/B では SPEC / Geekbench と同じ「テスト別
基準比の幾何平均」でカテゴリ指数を出す。ストレージは計測先 (target) と方式
(streams / io_uring) ごとに分離する。設計と主要ベンチマークとの対応は
`docs/COMPARISON.md` にまとめている。

## 再現性

- `binary_sha256`: 実行ファイル全体の SHA-256
- `build`: rustc / LLVM / target / opt-level
- `implementation.source_revision`: ビルド時の git リビジョン (取得不能時は
  `unavailable`)
- 各 benchmark の `implementation.code_hash`: kernel 関数の機械語 SHA-256
  (実行時に `/proc/self/exe` を解析して算出)

kernel の機械語が変わった場合、同じ benchmark version として扱わないでください。
`tools/audit.sh` が独立実装 (Python) で hash を再計算して照合します。

## 測定前の期待値検証

各 benchmark は測定の前に、同じ kernel 関数を固定入力で実行し、独立実装した
参照計算と機械的に照合します。不一致の場合は測定を行わず
`status: "verification_failed"` と理由を記録し、数値を出力しません。
検証対象は `verification_kind` として `--list` と `docs/STATUS.md` に出力します。

- `expected_value`: 参照計算と bit 単位で一致することを要求
- `functional`: 戻り値の型・範囲や副作用（書き込み内容）を独立実装で検査
- `protocol`: 期待値が原理的に存在しないため、プロトコル完了とカウンタ整合のみ確認
- `none`: 検証なし（環境依存の差分そのものが測定対象。理由を STATUS.md に記載）

検証自体が機能することは `tools/mutation_test.sh` で確認します。kernel の定数を
1つ改変したビルドを作り、該当 benchmark が `verification_failed` になることを
要求します。新しい benchmark を追加するときに満たすべき要件と CI による強制は
`docs/VERIFICATION.md` に定めています。

## ビルド

Linux 上で stable Rust を使用します。libc はリンクしません。
MSRV は 1.89（AVX-512 intrinsics の安定化に合わせています）。

```bash
cargo build --release
tools/audit.sh                       # 自己完結性・code hash 検証
tools/build.sh                       # audit + docs 生成 + tar.gz 作成
tools/build.sh aarch64-unknown-linux-gnu
```

aarch64 へのクロスビルドには `rust-lld` が必要です
(`$HOME/.rustup/toolchains/*/lib/rustlib/*/bin` に PATH を通すか、
`~/.cargo/bin` へ symlink してください)。

GitHub Actions で per-target バイナリをビルドしてリリースします。

- `.github/workflows/ci.yml`: push / PR で x86_64 と aarch64（qemu）の
  ビルド・audit・期待値検証・mutation test・MSRV ビルドを実行
- `.github/workflows/release.yml`: `v*` タグで
  `vmbench-<version>-<target>.tar.gz`（+ SHA-256）を作成し GitHub Release へ添付

## 環境識別

物理ホストの特定は目的としません。「別の時間に同じ VPS で実行された結果」を
関連付けるため、以下を分離して保持します。

- `instance_identity`: machine-id / DMI UUID / hostname の SHA-256 を合成
  (raw identifier は保存しない)
- `environment_identity`: CPU signature / topology / cache topology /
  memory size / hypervisor を合成
- `boot_identity`: boot_id (raw)
- `run_identity`: 128-bit 乱数

各要素の hash と取得可否も `components` / `unavailable` として保持します。

## 非目標

ネットワーク性能測定、provider API 依存の情報取得、結果の外部送信、価格取得、
ランキング生成、DB 投入、他ベンチマークの実行・流用は行いません。

## ライセンス

MIT OR Apache-2.0
