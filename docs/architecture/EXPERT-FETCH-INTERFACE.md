# ExpertFetch — the backend-neutral expert IO interface (K3 lane, #231/#236)

One interface for pulling MoE expert weights from cold storage into compute-visible
memory, implemented per platform in the vendored llama.cpp fork. Written 2026-07-30
as the divide-and-conquer contract between the two K3 lanes (BigMama: Windows/CUDA;
M5: macOS/Metal) so the implementations converge instead of drifting.

## Why an interface

The 0%-hit-rate and 190MB/s-on-a-7GB/s-drive incidents were both *identity* and
*IO-path* bugs that only surfaced because each platform hand-rolled its own fetch.
The contract below bakes in the three hard-won invariants:

1. **Identity is semantic, never a pointer.** Cache/fetch keys are
   `(weight_tensor_name_hash, expert_index [, shard_id])` — `blk.N.ffn_*_exps.weight`
   is stable across tokens and universal across Mixtral/Qwen3/DeepSeek/Kimi.
   `input->data` is scheduler-owned and changes every decode step. The full tensor
   name (gate/up/down distinction) participates in the hash; sharded GGUFs include
   the shard id or two shards' `blk.N` collide.
2. **Reads are expert-granular and sequential.** One fetch = one contiguous
   MB-scale read of the whole expert block. Never demand-fault through a mapping;
   page-fault-sized random reads are how 7GB/s becomes 190MB/s.
3. **Memory is a bounded pool, and fallbacks are LOUD.** Staging buffers form a
   wave-sized ring (`in_flight × expert_size`, ~1GB at QD32), recycled — never
   sized to the batch (the 20GB alloc → OOM → *silent* serialized-memcpy fallback
   was two bugs: the alloc and the silence). Any degraded path increments a
   visible counter and logs once.

## The interface (C++ shape both forks implement)

```cpp
struct expert_key {          // semantic identity — the ONLY cache/fetch key
    uint64_t tensor_name_hash;   // full name incl. ffn_gate/up/down
    int32_t  expert_index;
    int32_t  shard_id;           // 0 for single-file GGUFs
};

struct expert_fetch_stats {  // per-wave, surfaced to `moe stats`
    double   effective_gbps;
    uint32_t in_flight_peak;
    uint64_t hits, misses;
    uint64_t degraded_fallbacks;  // MUST be 0 in steady state; nonzero = loud log
};

class expert_fetch {
public:
    // Submit up to `max_in_flight` fetches; completed experts land in
    // pool-owned buffers handed to `on_ready` (buffer returns to the pool
    // when the consumer releases it). Non-blocking; call from the decode
    // thread; completions arrive on the fetch thread.
    virtual void submit_wave(std::span<const expert_key> keys) = 0;
    virtual void on_ready(std::function<void(expert_key, span<uint8_t>)>) = 0;
    virtual expert_fetch_stats stats() const = 0;
    virtual ~expert_fetch() = default;
};
```

Tuning parameters (constructor, per-platform defaults):
`max_in_flight` (QD; 16–32 saturates NVMe), `pool_bytes` (= QD × max expert size),
`alignment` (sector size where unbuffered IO requires it).

## Platform implementations

| Concern | Windows (BigMama lane) | macOS (M5 lane) |
|---|---|---|
| Read path | `FILE_FLAG_NO_BUFFERING + OVERLAPPED`/IOCP, sector-aligned pool buffers | UMA: `pread` straight into wired unified memory — **no bounce buffer at all** |
| mmap interplay | NEVER mix mapped faults + unbuffered reads over one region in a wave | `mmap + madvise(MADV_WILLNEED)` stays viable (no Windows-style fault serialization) — but cap advise to available phys |
| Prefetch cap | ≤ ½ available physical RAM; tail demand-pages | same rule; UMA counts GPU-resident bytes |
| Copy-out | overlap dequant/memcpy with the next wave's IO | usually zero-copy (destination is the compute buffer) |

## Acceptance (per platform)

- `moe stats` shows `effective_gbps ≥ 0.5 × drive rating` at QD32 on a cold sweep.
- `hits/misses` reflects the workload's true expert reuse (keying bug ⇒ 0% forever).
- `degraded_fallbacks == 0` steady-state; any nonzero is a loud log with the cause.
- Same `expert_key` hashing code on both platforms (shared header) — the identity
  must never fork.

## The container format IS the fetch lever (WASTE convergence, 2026-07-30)

Independent validation from WASTE (a working single-machine K3 expert-streamer:
17 GB/token at 9.9 GB/s, 0.32–0.36 tok/s on a laptop): the ~50× fetch gap between
it and a raw-GGUF reader on comparable NVMe is **storage layout, not IO tuning**.
Raw GGUF stores expert weights 32-byte-aligned and scattered (gate/up/down apart)
→ misaligned random reads + bounce buffers → ~200 MB/s ceiling. WASTE stores each
expert as a **4 KiB-aligned record with gate/up/down adjacent** → one aligned,
contiguous, near-sequential `pread` per expert → drive-rated throughput.

**Contract additions:**
- An `expert_fetch` implementation SHOULD read from an aligned streaming container
  (4 KiB record alignment, one expert = one contiguous record incl. all three
  projections), not raw GGUF. Producing that container is a FOUNDRY output — the
  foundry doesn't just shrink models, it lays them out for streaming.
- Eviction: LFRU over plain LRU (WASTE's measured choice; small win).
- Convergent architecture (independently reached on both lanes AND by WASTE):
  trunk-resident, cache-bypass reads (`FILE_FLAG_NO_BUFFERING` / direct IO),
  bounded expert cache keyed `(layer, expert)` — which is this contract's
  `expert_key`. Our sliding-window batched prefetch is AHEAD of WASTE (they have
  none) — keep it.
- Expected cross-token reuse at K3 sparsity (16/896): ~13% at a good budget. A
  hit-rate of exactly 0 is ALWAYS a bug, never physics.

## Relationship to continuum

The `#231` arch-profile carries these tuning parameters as DATA per model family;
the genome/expert-paging machinery (#180/#230) consumes `expert_fetch` as its cold
tier. Continuum never reimplements the IO — it reads `stats()` through the serving
snapshot for governor decisions and glass-box widgets.
