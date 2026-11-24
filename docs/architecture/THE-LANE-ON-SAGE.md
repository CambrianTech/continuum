# The lane on SAGE: one GPU, every mind served at the next step

Status: design for review (Joel, Codex, Cormac), 2026-11-24. No code before it is agreed.

The settled design is SAGE.md, "The lane: the one serial piece, sliced so fine nobody notices". This
page is how the live engine gets there from what runs today: the parts, what each owns, what is
deleted, the order, and the number each step must beat. It replaces nothing in SAGE.md; it builds it.

## What is wrong today (receipts)

The M5, Qwen3.6-35B-A3B Q4_K_M, build 0b4d23ffe9 (canary before #6258), 23:34-23:49Z 2026-11-24:

| | p50 | p90 | max |
|---|---|---|---|
| wait for the engine | 12.1 s | 144 s | 277 s |
| prefill | 2.5 s | 23 s | 207 s |
| decode | 5.2 s | 16.8 s | 26.6 s |
| turn | 25 s | 293 s | 526 s |

33 resident turns; 26 of them waited more than 1 s. On every row `wait_ms == longest_service_gap_ms`:
she waited out another mind's whole turn. The cause is one rule, `persona_scheduler/frame.rs`: "a turn
in progress runs to its end". Any wait over 1 s is a defect (Joel, 2026-11-24).

The other attempt, #6258 (645541ab68), stepped minds one step at a time through the SAGE lane, which
was right, but sized its KV pool's seats from the serving plan's lane count (`in_process_engine.rs`:
`seats = target.lanes`, five on the M5), never from the KV room it measured. Seven minds over five
seats, each holding 100k-140k tokens of a 262k pool, paged constantly: 163 evictions in 8 minutes, no
resident turn finished. It also had no batch pick: one mind per step even when several were resident.

## The shape

One lane: the GPU, the only serial piece. Every mind's work around it (perception, tools, memory,
learning, assembling her next context) runs on her own workers and meets the lane through state. At
each step boundary the lane picks a batch of resident minds, the engine runs one forward pass for all
of them, and each mind's result is published. A mind that is not resident is paged in by Residency
while others step; a switch between resident minds moves nothing.

Four SAGE parts, each a `Component` on the engine's bus, each owning one concern:

| Part | Owns | Hears | Says |
|---|---|---|---|
| **Lane** (exists: `continuum_sage::slice`) | who steps next: stride shares by need, the 1 s wait bound, the throughput control law | each mind's `Need` and `Decoding`; `Resident`; step and switch times | `Turn` (the batch for the next step) |
| **Residency** (new; replaces #6258's `Seats` and the driver's `switch_to`) | which minds' KV is in the pool, sized by measured bytes, never a plan count; paging in and out (the fork's per-sequence gather and scatter); coldest idle mind out first, never a member of the running batch | `Turn`, each mind's `Need`, the pool's measured free cells, memory pressure | `Resident` (who is in, who is moving) |
| **Step** (new; replaces the driver loop and `frame.rs`) | one forward pass for the batch: each member's prefill chunk or next token, in one `llama_batch` across their sequences; sampling per mind | `Turn`, `Resident` | each mind's tokens (to Emit), `Stepped` (times, for the Lane) |
| **Mind's turn** (a state per mind, not a part) | where her turn stands: waiting, prefilling from `pos`, decoding; her sampler; her checkpoints | her requests | her `Need`, `Decoding` |

Emit and Record stay as they are (already off the engine thread). Learning stays between her turns for
now; as tiles between steps it is a later slice.

**One owner per fact:**
- how many minds fit is Residency's measurement of the pool;
- who steps is the Lane's;
- what a step costs is Step's measurement.

Nothing reads `ServingTarget.lanes` for seats.

## What this deletes

- `persona_scheduler/frame.rs` (whole-turn pick) and the driver's frame loop around it.
- The driver's `switch_to` copy-out/copy-in for every switch (Residency moves a mind only when she is
  not resident).
- #6258's `Seats` and the `seats = target.lanes` launch rule (the pool's sequences become a capacity
  Residency measures, not a seat count).
- The minds-per-lane seat cap and any turn-long permit left in `slots/endpoint_concurrency.rs` (SAGE.md
  lists them; checked at slice time, deleted where found).

Kept from #6258 because they are right and SAGE-shaped: the lane wiring (`driver/lanes.rs` onto
`continuum_sage::Lane`), the fork's pool layout (gather and scatter one sequence's own cells, receipts
travelling with her), and its tests at 1-3 seats.

## The validator first (seconds, no live residents)

`persona-lane-bench` on the fork's test models (the 0.5B and the qwen35moe hybrid), then the 35B on the
M5 with serving down. N minds with real context depths (2k, 30k, 130k), mixed turns (a 2,000-token
prefill beside a 50-token answer), run under each design. It reports:

- every mind's worst wait for her next step;
- aggregate and per-mind tokens per second;
- the switch cost (resident and paged);
- bytes per resident mind, and so how many fit.

The first run measures today's whole-turn engine: the baseline row.

## The order, one PR each, merged before the next

| # | Slice | Gate (validator, then the 15-minute live read) |
|---|---|---|
| 1 | The validator, and today's numbers in this doc | reproduces the live shape: a short turn waits out a long one |
| 2 | Lane + Step replace `frame.rs`: one mind per step, every switch a page (one resident) | worst wait <= 1 s at 4 minds; aggregate tok/s within the measured switch overhead of one mind alone (SAGE.md: 4% at 6 ms) |
| 3 | Residency: as many minds resident as the pool measures; a switch between them moves nothing | paging only when minds exceed the pool; switch between resident minds ~0 ms |
| 4 | The batch pick: resident minds decode in one step | aggregate tok/s at 4 minds >= 1.5x one mind (SAGE.md anchor: 2.1x at short depth) |
| 5 | Her new tail prefilled while she is in a tool; shared prefixes copied (`seq_cp`) | submit to first token < 1 s after a tool result |

Each slice deletes the path it replaces in the same PR. Live: worse than the read before it means the
node goes back at once.

## Questions for review

1. **Paging cost at depth.** A 130k-token mind on the 35B: how many bytes is her KV plus recurrent
   state, and how long is a page-out plus page-in on the M5? That number sets how far slice 2 alone
   gets before slice 3 is needed. Measured in slice 1; owner Fable.
2. **Prefill chunk size under the bound.** A 2,048-token chunk is about 3.2 s at the cold rate measured
   today (8044b75b: 133,210 tokens in 207 s, 23:43Z); inside a 1 s bound a chunk must be smaller, or a prefill batch shares the step with others' decode tokens. Is
   chunked prefill mixed with decode in one batch the default? Proposed: yes; the Lane sizes the chunk
   from the measured step time.
3. **Residency and the 262k window.** With minds at 100k+, the pool holds two of them. Is the window
   per mind (each may grow to the whole pool, as #6258 had it) or a share? Proposed: whole pool per
   mind, Residency pages the coldest idle out when a step needs room.
4. **Learning in the lane** (Cormac): between turns today; as tiles between steps later. Does slice 2
   change anything the trainer relies on (LearnPort, `learning/said`)? Proposed: no; learning still
   runs when no turn waits.
