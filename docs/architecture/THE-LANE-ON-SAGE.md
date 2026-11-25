# The lane on SAGE: one Core, every mind served at the next step

Status: design for review (Joel, Codex, Cormac), 2026-11-24. No code before it is agreed.

The settled pieces already exist; this page wires them together and changes nothing about them:
- **the Core:** `continuum-persona-engine`'s `PersonaEngine` + `pipeline::Roles`, on
  `continuum-persona-llama`'s `UnifiedTier`;
- **the lane:** `continuum_sage::slice`, its `Lane`, `Slicer` and `Tuner`;
- **the rules:** SAGE.md, "The lane: the one serial piece, sliced so fine nobody notices".

## How it works (as before, and as it stays)

The Core is modeled on a CPU core. Minds are processes, and a mind's native state (her KV rows,
recurrent state, checkpoints) is her register file.

- **One context, one execution stream: the occupant's,** holding the whole context. Every other
  mind's blocks stay in host memory exactly as she left them (`unified_tier.rs`, `STREAMS = 1`).
- **A switch is a swap.** The occupant is saved (her rows copied to her blocks, the stream emptied),
  then the next mind is bound into the emptied stream. `Roles` sequences it: next staged, handoff,
  previous retired, so nothing is dropped or read early.
- **Measured live:** a swap takes 21-26 ms p50 on the M5 (aa734cc01c: max 263 ms; 2026-11-19/20:
  mean ~21 ms). The in-process `persona_switch` validator measures about 6 ms.
- **One mind decodes at a time.** Never a batch of minds on the Core, and never `--parallel`: four
  parallel decodes ran at ~2 tok/s each, against one at a time at full rate (the M5, Nov 2026).
- **How many Cores a node has is its measured capability:** one on the M5.

Because a swap is that cheap, the Core can switch at almost every step, round robin, and every mind
sees the GPU as if it were her own (Joel, 2026-11-20: "as if all lanes exist but it doesn't degrade").

## What is wrong today (receipts)

The M5, Qwen3.6-35B-A3B Q4_K_M, build 0b4d23ffe9, 23:34-23:49Z 2026-11-24:

| | p50 | p90 | max |
|---|---|---|---|
| wait for the Core | 12.1 s | 144 s | 277 s |
| prefill | 2.5 s | 23 s | 207 s |
| decode | 5.2 s | 16.8 s | 26.6 s |
| turn | 25 s | 293 s | 526 s |

33 resident turns; 26 of them waited more than 1 s. On every row `wait_ms == longest_service_gap_ms`:
she waited out another mind's whole turn. The cause is one rule in `persona_scheduler/frame.rs`: "a
turn in progress runs to its end". The switch was never the cost. The rule never used it.

#6258 (reverted by #6263) went the other way: a pool of five seats, sized from the serving plan's lane
count. Seven minds over five seats, each holding 100k-140k tokens of a 262k pool, paged constantly:
4,191 evictions in 15 minutes (Codex, 23:56-00:11Z).

## The change: the lane picks who steps, at every step

One behavior changes: `frame.rs`'s whole-turn pick is replaced by the SAGE lane, which is already
built and simulated.

- **At every step boundary,** the lane reads each mind's newest `Need` and `Decoding` (state, never an
  await) and picks who steps next. Stride shares by need; a rise in urgency takes the next step;
  nobody starves.
- **The wait bound is the guarantee:** 1 s (Joel, 2026-11-24: "Any over 2 s are wrong. Probably over
  1 s"). The deadline guard brings in any mind whose slack is gone.
- **The slice is the control law's state,** fed by the measured swap cost and step time: it climbs
  aggregate throughput against the bound. No constant, no permit, no seat count.
- **A step is sized to fit the slice.** A prefill chunk is as large as fits the measured step budget,
  never "the whole batch because the turn keeps the Core". A 2,048-token chunk is about 3.2 s at
  today's cold rate (8044b75b: 133,210 tokens in 207 s, 23:43Z), so at depth a chunk is smaller.

In SAGE's own simulator (three residents, 20 ms steps, 26 ms switch, 1 s bound, 300 s), the lane made
46.3 steps/s with the worst wait at 978 ms, against 24.5 steps/s switching every step (SAGE.md).

## The parts (SAGE components on the engine's bus)

| Part | Owns | Hears | Says |
|---|---|---|---|
| **Lane** (`continuum_sage::slice::Lane`, exists) | who steps next, the bound, the control law | each mind's `Need`, `Decoding`; `Stepped` | `Turn` (who holds the Core) |
| **Core worker** (the engine thread: the one serial piece) | the context; one step for the mind the lane named; the swap through `Roles` when it is another mind | `Turn`, submissions | tokens to Emit; `Stepped` (step and swap times); each mind's `Decoding` |
| **Her turn** (state per mind, not a part) | waiting, prefilling from `pos`, decoding; her sampler and checkpoints | her requests | her `Need` |
| **Emit, Record** (exist, off the engine thread) | text, stop sequences, the caller; probe rows | tokens, facts | answers, rows |

Learning keeps its place, between her turns while no turn waits.

## What this deletes

- `persona_scheduler/frame.rs` and the whole-turn rule in the driver loop.
- The driver's own pick (`Driver::pick`): the lane is the one owner of who steps.
- Any fixed chunk size taken from the batch "because the turn keeps the Core" (`prefill_chunk`).

Nothing is added beside the Core: no pool, no seats, no second stream.

## The validator first (seconds, no live residents)

`persona-lane-bench` drives the real Core (`UnifiedTier` on a model) with N minds at real depths
(2k, 30k, 130k) and mixed turns: a 2,000-token prefill beside a 50-token answer. It reports:
- every mind's worst wait from ready to her next step, swap time included;
- turns completed;
- aggregate and per-mind tok/s;
- swap time by depth.

It runs on the 0.5B and the qwen35moe hybrid on any machine, and on the 35B on the M5 with serving
down. Its first run is today's whole-turn engine: the baseline row, reproducing the live shape.

## The order, one PR each, merged before the next

| # | Slice | Gate (validator, then the 15-minute live read) |
|---|---|---|
| 1 | The validator; swap cost at depth measured | reproduces the live shape (a short turn waits out a long one); swap ms at 2k/30k/130k on the 35B |
| 2 | The lane replaces `frame.rs`; steps sized to the slice | worst ready-to-step wait <= 1 s at 4 minds, swap included; turns completed >= baseline; aggregate tok/s >= one mind alone minus the measured swap share |
| 3 | The swap gets cheaper where slice 1 says it costs (the fork's save/bind at depth; recurrent state only where a hybrid needs it) | swap at 130k down by the measured factor; no change in replies (byte-identical continuation, as the fork's tests check) |
| 4 | Her new tail prefilled while she is in a tool (it is her step, taken when the lane gives her one) | submit to first token < 1 s after a tool result |

Each slice deletes the path it replaces in the same PR. A live read worse than the one before it sends
the node back at once.

## Questions for review

1. **Swap cost at depth** (Fable, slice 1). At 130k tokens on the 35B, how long are the save and the
   bind? If the swap grows with depth, slice 3 comes before the bound holds at four deep minds.
2. **Chunked prefill under the bound** (Codex). The lane sizes the chunk from the measured step time.
   Is there any caller that relies on a turn's prefill landing in one piece (the view checkpoint
   already ends a chunk exactly at her view)?
3. **Learning** (Cormac). Learning still runs when no turn waits; slice 2 changes who steps, not when
   learning runs. Does anything in LearnPort or `learning/said` assume a whole turn ran uninterrupted?
4. **The deploy tracker** (Codex). It replaced a manual rollback with canary at 23:54Z. A live
   rollback should hold until canary is fixed, not be undone by the tracker.
