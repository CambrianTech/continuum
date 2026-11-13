# Engine patterns: the substrate is a game engine's renderer

Joel, 2026-11-13: "use the graphics pipeline approach and study the unreal engine optimizations";
"Carmack level efficiencies, avoiding squares even in the GPU"; "never parse"; "avoid rasterization
and cpu/gpu transfer whenever possible"; "use raw blobs and bytes unless you truly need the
encoding"; "do not waste cpu ever for anything it is unnecessary for". This is the review standard
for every data, storage, retrieval and serving change. A change that cannot say which row it
follows, and show its end-to-end timing and output quality before and after, is not ready.

## 1. What the CPU does per byte

The first question for any data path: what does the CPU touch per byte, and can it be zero?

- **Bytes stay bytes.** A producer's native bytes are stored and moved as they are (a BLOB beside a
  typed descriptor: element type, shape, byte order, producer identity). No translation on write,
  read or in between. A typed view over bytes, never a walk that converts them.
- **No per-element CPU loops.** A load is one bulk memcpy or zero copies. Converting, normalizing or
  validating element by element on the CPU is a design bug.
- **Translation, when unavoidable, happens on the GPU** inside the kernel that consumes the data
  (f16/int8 to f32, dequantization, layout), never as a CPU pass and never per use.
- **Never parse until the absolute last step.** JSON only at a true external boundary, or where the
  store must index inside a document (then the store's binary JSON). Rows decode straight to the
  typed columns a caller asked for; a payload stays opaque until its addressed consumer opens it.
- **No SQL outside the ORM, no Python anywhere.** The schema declares the indexes and projections, so
  filter, sort, count and limit run inside the store. Rust only; bash only for instance scripts.
- **No transfers that are not needed.** On unified memory (the M5) the stored pages are the GPU
  buffer (mmap + a no-copy Metal buffer). On a discrete GPU (the 5090) one asynchronous DMA upload,
  overlapped with compute and double-buffered. The CPU never stages data and reads back only the
  answer.

## 2. Carmack rules for the math

- **Never compute what does not change the answer.** Ranking needs order, not distance: squared
  distances, never a square root; for cosine, store unit vectors (normalize once at write) so
  similarity is a dot product with no norms, roots or divides; normalize the query once per search.
- **Hoist everything invariant out of the inner loop.** A per-candidate recompute of the query's
  norm, or of a candidate's norm that never changes, is the anti-pattern
  (`continuum-kernel/src/orm/vector.rs::similarity::cosine`, `continuum-store` `data_state.rs`
  cosine_similarity, both found 2026-11-13).
- **Coarse first, exact only where it decides.** A sign-bit sketch with popcount (12 64-bit ops for
  768 dims) culls; exact dot products run on the shortlist; partial top-k selection, never a sort.
- **Spend only the precision the consumer perceives**, with the error bound stated and checked by
  a validator (recall@k against the exact answer), never assumed.
- **No branches, asserts or allocation in inner loops.** Validate once at the boundary.

## 3. Unreal Engine systems and their counterparts here

| Unreal | What it does | Ours |
|---|---|---|
| Render Dependency Graph (RDG) | Passes declare the resources they read and write; the graph schedules them, inserts barriers, aliases transient memory, overlaps independent passes on async compute | An act is a graph: perception, retrieval, prefill, decode, tools. Transient buffers come from per-act arenas, prefill overlaps the tool call, no hand-sequenced awaits (CBAR-SUBSTRATE-ARCHITECTURE) |
| Virtual texturing / streaming pool | Only visible pages resident inside a fixed pool; under pressure the mip bias rises (coarser), never a failure | KV pages and context resident by predicted need inside one memory budget; under pressure the view collapses to coarser levels and the engine's host cache gives memory back (fork #110, card 494c99f1). A fixed `--cache-ram` is a texture pool that never evicts (the M5, 2026-11-13: 60 MB free, prefill 92 tok/s against ~190) |
| Nanite clusters, LOD by screen-space error | Detail chosen per cluster by how much it matters on screen | The context codec's levels (whole, summary, folded) chosen per item by predicted relevance (MIND-CONTEXT-AND-ENGINE-CONFIG) |
| GPU-driven culling (HZB, cluster culling, indirect draws) | The GPU culls millions of instances hierarchically; the CPU never touches per-object work | Vector search as one GPU dispatch over the resident matrix: sketch cull, exact dot on survivors, in-kernel top-k; only k indices and scores come back |
| Instancing | One mesh, many draws | One shared prefix (system prompt, tools) in KV, copied across minds (`seq_cp`), never re-prefilled |
| Temporal reuse (TAA, TSR) | Reuse last frame, render only the delta | Epoch context: an act prefills only its appended tail on the cached prefix |
| Dynamic resolution, scalability profiles | Hold the frame-time target by scaling quality; settings per hardware tier | Hold the act-time target by scaling window and precision; capability profiles per node (M5, 5090, IntelMac) |
| Async loading | Assets stream in behind handles; nothing blocks the frame | Handles and events for every long operation; nothing blocks an act |
| Compute over rasterization | A compute pass where a raster pass would redraw | Avatar frames from compute or reused tiles; nothing redrawn that did not change |

## 4. How a change proves itself

Frame time is act time. Every change here posts, on a live node and on a replay validator:
act latency (median and p90), the stage it targets (prefill tok/s, decode tok/s, search latency,
store op p50/p99), memory (free, compressed, swap, phys_footprint), and output quality (recall@k,
review outcome). A health line's label is not a measurement.

## 5. Order of work (2026-11-13)

1. The streaming pool: fork #110 (live `/prompt-cache/limit`) + the core reclaim, so the engine's
   cache gives memory back under pressure. Measured on the M5: prefill tok/s, decode tok/s, free
   memory, act latency.
2. GPU-driven vector culling: unit vectors at write as raw f16/f32 bytes, a no-copy GPU buffer, one
   GEMV dispatch with fused top-k and a popcount prefilter. Measured: 10k/100k search latency and
   recall@k.
3. The act as a dependency graph with transient arenas and prefill overlapped with tools.
