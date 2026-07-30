# WASTE extract — mined reference for the container + cache port (#268)

Mined 2026-07-30 from sqliteai/waste (Apache-2.0) for BigMama's lane + the foundry.
Durable in-repo because airc transport flaps — pull canary, don't rely on chat.

## ExpertRec (FORMAT.md) — the 50x lever
### Expert bank record (`experts-L{n}.bin`)

```
[4 KiB-aligned]
ExpertRec {
  u32 magic 'WEXP', u16 layer, u16 expert_id
  u8  fmt (VQ3R|VQ2R), u8 flags, u16 codebook_id
  u32 gate_off, up_off, down_off, correction_off   // within record
  u32 record_4k_blocks
  -- gate indices | up indices | down indices | per-channel corrections --
}
```

One `pread` of `record_4k_blocks * 4096` bytes yields the whole expert.
On K3 that record is **12 406 784 bytes, exactly 3029 pages** — which is
what makes O_DIRECT possible, and why `bank_open` checks the alignment
rather than assuming it: a record that is not a page multiple makes every
read fail `EINVAL` instead of merely running slow.

Records for the same layer are contiguous and sorted by expert id.

- Records per layer contiguous, sorted by expert_id; bank_open VERIFIES 4KiB alignment (EINVAL loud).
- K3 record = 12,406,784 B = exactly 3029 pages. One pread per expert.
- Container = directory: manifest.json, trunk.bin, experts-L{n}.bin per layer (THE grid shard unit), codebooks.bin, usage.waste (runtime routing hotlist = sentinel-PGO analog).

## Cache semantics (ecache.h, port these as-is)
```c
    int32_t  key;        /* layer<<16 | expert, or -1 when empty            */
    uint32_t hits;       /* LFRU frequency term                             */
    uint64_t last;       /* LFRU recency term                               */
    uint8_t *data;
} waste_eslot;

typedef struct {
    waste_eslot *slot;
    int32_t *hash;       /* open addressing: hash -> slot index, -1 empty   */
    int n_slots, hash_mask;
    size_t rec_bytes, budget_bytes;
    uint64_t clock, hits, misses, bytes_read, evictions;
    unsigned rng;
    int policy;          /* 0 = LFRU, 1 = LRU                               */
} waste_ecache;

/* O_DIRECT requires the destination buffer to be aligned to the device's
 * logical block size, so record buffers come from here rather than malloc.
 * Expert records are whole 4 KiB pages by construction, which covers both
 * the 512- and 4096-byte cases; on macOS the alignment is merely harmless. */
/* 16 KiB, the Apple Silicon page: O_DIRECT wants 512 or 4096, and Metal's
 * newBufferWithBytesNoCopy wants a whole page — one alignment serves both,
 * which is what lets the GPU read trunk weights the CPU already has. */
#define WASTE_DIO_ALIGN 16384
void *waste_dio_alloc(size_t n);
void  waste_dio_free(void *p);

/* budget_bytes 0 disables caching (every access reads). Returns 0 on ok. */
int  waste_ecache_init(waste_ecache *c, size_t budget_bytes, size_t rec_bytes,
```
- Key: layer<<16|expert, ONE per logical expert (gate/up/down bundled in the record).
- LFRU, random-sample victims: at small fractions LRU collapses to 5% where LFRU holds 29% (their Gate 2).
- DIO align 16KiB = Apple Silicon page: serves O_DIRECT AND Metal newBufferWithBytesNoCopy (GPU zero-copy).
- THE CLIFF (their Gate 5): hit rate EXACTLY 0 until budget > one token's working set, then climbs.
  K3 @ raw MXFP4 (~600MB/expert): cliff ≈ 290GB — unreachable. @ VQ3 (~12.4MB): ≈ 6GB — trivial.
  => retention is a FORMAT problem, not a code problem. Container first.
- Non-uniform per-expert bits: measured and DROPPED (importance spread 1.01–1.15x). Don't build it.
- Warm-start: persist/restore usage (waste_ecache_save_usage/warm) — kills the cold ramp only.
