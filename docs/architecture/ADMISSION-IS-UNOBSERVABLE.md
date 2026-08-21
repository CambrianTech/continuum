# The 90s watchdog is killing live prefills, and the starvation is its shadow

**Status:** measured defect + design, 2026-08-21, M5, core `eb33af7b3`, lane
`Qwen3.8-27B` on 4 slots, 58,112-token served window.

> **RESOLUTION LOG (same day, measured on each successive build):**
>
> | Build | Change | Live proof |
> |---|---|---|
> | `9e2f95820` | `stream_liveness` phase machine — prefill gets the bulk budget | retries 44 → **0** |
> | `312c4a9c8` | `prefill.rescued` / `prefill.complete` probes | 4 rescued rows, each `would_have_died=1` — 163–192s prefills now complete |
> | `6dea7c435` | queue/ingest split in the receipts | `queued_ms=0` on ALL streams — §2's slot-theft-during-queue hypothesis **dead**; residual is ingest-share under continuous batching (~95–100 tok/s on big prompts) |
> | `cfe606c55` | §"one more root": the `/props` 503 boot-race latch — **slot affinity had been silently OFF on virtually every boot since 2026-07-11** | `slot_affinity.pinned` ×3 (first pins ever observed); `prefill.complete` with `cached: 1155 / total: 1159` — **~99.7% KV reuse on a byte-stable prompt** |
>
> The serving governor also replanned mid-day to `lanes: 4, window: 125,184` (from
> 58,112), so the §1 demands now FIT (`over_window` 0.70–0.78). The remaining open
> measurement is a citizen's own second-turn reuse through her pinned slot, and a
> completed `persona.turn.*` on the citizen-driver bench path.

**The one-line version:** citizens demand 1.3–2.1× the served window, prefill of a
window-sized prompt takes ~170s at the measured rate, our liveness watchdog gives it 90s,
and every turn dies and retries forever. The ambient-permit starvation everyone can see is
the *shadow* of that — three permits held ~90s each by turns that can never finish.

**Why this doc exists in this shape (Joel, mid-investigation):** *"This should have been
something delivered across the boundary."* Every number below I computed by hand — parsing
a JSONL ledger, correlating classes, timing an HTTP stream with a throwaway script. The
substrate knows all of it and delivers none of it. The design in §4 is about that, not
about adding one more counter.

---

## 1. The measured chain

Every row live from this box, deltas not cumulative totals.

| # | Fact | Measurement |
|---|---|---|
| 1 | Serving is healthy | `serving/status` → `ready: true`, `lanes: 4`, window 58,112 |
| 2 | Demand exceeds the window on most turns | `over_window` = **1.31, 1.62, 1.83, 2.12** (Kira 97,359 / Atlas 94,370 / Joaquin 88,475 tok) |
| 3 | Prefill rate, measured directly | 603 tok → **17.7s** TTFB; 5,728 tok → **31.1s** ⇒ ~**382 tok/s** + ~16s fixed |
| 4 | ⇒ a window-sized prompt cannot deliver first byte in 90s | 58,112 tok ⇒ ~**170s**; the watchdog fires at **90s**. Crossover ≈ **28,000 tokens** |
| 5 | The watchdog fires, naming the lane dead | 44× `persona.settle.deliberation_retry`, *"inference lane went silent for 90s (no bytes at all) — the slot HAD started our work and then stopped mid-stream, so the backend is stuck or dead"* |
| 6 | The retry is byte-identical | Atlas `demand_tokens: 94370` at −27.3 min **and** −6.4 min — same prompt, same outcome |
| 7 | Permits are held across that wait | `run_self_cycle` holds the ambient permit; `service_loop.rs:559` |
| 8 | ⇒ starvation, perfectly periodic | **+4 yields/min, exactly, for 25 consecutive minutes** (total_yields 40→84) |
| 9 | Zero successful self-cycles in that window | last `selftick.perceive` −16 min, last `persona.act.*` −42 min |
| 10 | Zero room turns, ever | `persona.turn.*` = **0 across the ledger's 868 minutes** — grep-confirmed, not a filter artifact |

**Periodicity is the tell.** Contention is bursty; exactly +4/min for 25 minutes is four
citizens each yielding once per beat, with **zero** winners. The pool is 3 permits
(`lanes − 1`). Three are held continuously by turns burning 90s to fail.

---

## 2. What is proven, and what is not

**Proven:** items 1–10. In particular #4 is now a measurement, not arithmetic from a
remembered rate — I timed two prompt sizes against the live lane.

**NOT proven — the mechanism behind #5.** The error's `started` branch means
`last_progress` *did* advance: `prompt_progress` frames arrived, then stopped for 90s
while prefill was still incomplete. Something suspends an in-flight prefill. The leading
candidate is **slot theft**: `slot_prompt_similarity` defaults to 0.1 and we never pin
`id_slot`, so a request routes to whichever free slot shares the longest prefix, and four
citizens sharing a ~7,600-token tool head are all above that threshold. A long prefill is
a long exposure window for reassignment. That reconciles with
[[llama-server-already-does-slot-affinity-and-conversation-save-restore]] and makes
**declared slot affinity the keystone**, as the KV work already concluded for independent
reasons. It is a hypothesis. The discriminator is §4's first instrument.

**Retracted during this investigation** (see §5): "the ambient pool is hardcoded to 1";
"`set_served_lane_count` forgets to grow it"; "a lazy pool seals at the boot floor"
(the fallback is `MAX_LANES` = 8, not 1); "citizens took zero turns because the pool is
saturated by long *successful* turns" (nothing succeeds); "we never request
`return_progress`" (we do — `openai_adapter.rs:1990`).

---

## 3. Why this is one defect and not three

The KV finding from this morning and this one are the same failure at two scales:

```
KV prefix reuse = 0%   (measured: cachedTokens 0, 57,005 tok re-prefilled over 4 acts)
   → every turn re-prefills from scratch
   → prefill duration scales with the whole prompt, not the delta
   → first byte lands beyond ANY fixed watchdog at window scale
   → watchdog kills it at 90s, stamps "backend is stuck or dead"
   → retry re-enters with the identical prompt, identical outcome
   → permit held ~90s per attempt × 3 permits = the starvation everyone sees
```

Fixing the watchdog alone makes turns take ~170s instead of failing — better, not good.
Fixing KV reuse alone still leaves a fixed 90s cliff for any cold prompt. **Both, and the
ordering fix already landed (`eb33af7b3`) is the cheaper half.**

---

## 4. Design — deliver it across the boundary

The rule this all obeys, already written as law in KV-CACHE-ECONOMY §6b: **a control path
may never depend on a signal that only exists as text.** Three instruments, in order.

### 4.0 Where the schema lives (Joel, 2026-08-21)

*"Work more like Java or well defined schema across boundaries, ideally from a rust struct
or something inside Airc envelopes and events."* Not backwards — **that architecture is
already in the tree, and these facts simply never got defined as schema at all.** Two
planes, and the choice between them is "who must see this":

| Plane | Source of truth | Generates | Use for |
|---|---|---|---|
| **airc wire** | `airc-wire/src/schema/wire.fbs` (FlatBuffers via `planus`) — its own Cargo.toml calls it "the source of truth" | Rust today; C++/TS/Swift/Kotlin from the **same file**, no extra tool | facts that cross NODES |
| **ts-rs** | the Rust struct, `#[derive(TS)]` | `protocol/typescript/**` (~500 types live) | facts a client renders in-process |

**One correction to the tooling, in service of the same principle.** For C++ we should not
reach for cbindgen here: FlatBuffers is *already* a language-neutral IDL, so a table in
`wire.fbs` yields C++ for free. cbindgen is Rust→C ABI only, one direction, POD-friendly
and awkward around `String`/`Vec`/`Option` — and standing it up beside the `.fbs` would
create **a second definition of the same fact**, which is exactly what the compression
principle forbids. Right instinct, and the better tool is already here.

**So `AdmissionSnapshot` and `LaneProgress` are `.fbs` tables**, with the Rust generated
rather than hand-written — cross-grid glass boxes (#283) need them from another node, which
settles the plane. And note what the schema carries today: **only `Envelope` and `Header`**,
so every payload is opaque bytes. Typed payload tables *are* #445 ("every publish declares
its class in a HEADER; subscribers filter daemon-side before fanout/decode") — this work
completes that card rather than sitting beside it.

**Upstream schemas are not ours.** llama.cpp's `prompt_progress` / `/slots` shapes belong
to llama.cpp; we parse them into our table at the adapter and never let their JSON shape
leak inward. That boundary is a translation, not a generation — and it is the one place
where hand-written types are correct.

### 4a. The watchdog waits on PROGRESS, with the lane's own expectation

The signal already crosses the boundary correctly — `return_progress: true` makes
llama-server emit `prompt_progress {total, cache, processed, time_ms}` in-band on the SSE
stream, and we request and parse it (`openai_adapter.rs:1990`, `:2683`). The machinery is
right. **The budget is wrong**: a fixed 90s applied to a variable-size prefill.

Derive it instead: `expected_prefill_ms = (total − cache) / measured_tok_per_s`, and let
the budget be a multiple of that, floored at today's 90s. Every term is already on the
frame — `total` and `cache` come from the server, the rate is the catalog expectation
(#441) or the observed rate from prior frames. A slot that *stops advancing `processed`*
still fails at the same 90s, so the #385 wedge detector keeps its teeth; a slot legitimately
ingesting 58k tokens stops being executed for the crime of a big prompt.

This is the same lesson as the readiness smoke (below): the constant encodes an assumption
about work size that the work itself reports.

### 4b. Admission state becomes a snapshot, not a yield counter

To ask "why did this citizen not take a turn" you need four numbers that are all
process-private today: `lane_count()` (private fn), `ambient_installed` / `serving_installed`
/ `nondirected_installed` (`AtomicUsize`, **written and never read**), and live permit
occupancy (semaphore internals; `available_permits()` is used only in tests).

The one public signal, `ambient_yields()`, is a monotonic counter of *failures* at ≤1
row/min. It told me starvation was happening and nothing about why — and its cumulative
total across reboots is what made me misread the rate three times.

```rust
pub struct AdmissionSnapshot {
    pub served_lanes: usize,        // 0 = serving has NEVER published (≠ one lane)
    pub ambient_budget: usize,      // == ambient_installed, today unreadable
    pub ambient_held: usize,        // the number that would have ended this in 5 seconds
    pub nondirected_budget: usize,
    pub nondirected_held: usize,
    pub ambient_yields_total: usize,
}
```

`ambient_held` is the whole point: 3-of-3 held with zero completions is a *stuck holder*,
and 3-of-3 held with completions flowing is healthy contention. Nothing in the tree can
currently tell those apart. Note the fields already exist as private atomics — this is a
**publisher, not a schema change**.

Per CONCURRENCY-STYLE-GUIDE: `watch::Sender<AdmissionSnapshot>`, published on change plus
the governor tick, never per-yield (per-yield was correctly rejected at ~92 rows/min,
#399). `LaneAdmission` already owns every field.

### 4c. Readiness stops rediscovering thinking-ness by experiment

`serving.smoke.think_retry` fired **355 times**, byte-identical every time
(`quick_tokens: 24, reasoning_len: 97, retry_budget: 768`). On a thinking model the quick
budget is structurally incapable of answering, so every readiness check burns two
generations on lanes citizens are starving for. Thinking-ness is a **known property of the
catalog Model row**, beside `tool_protocol` and `sampling` (#76, #294). Read it; keep the
`ThinkStarved` retry only as the fallback for rows that don't say.

### 4d. The acceptance test

From BENCHMARKS-ARE-ADAPTERS: *can a citizen in the room perceive this through the same
ViewState pipe the human's screen uses?* Today "why am I not getting turns" is answerable
by neither the citizen nor the operator without a debugger and a hand-written parser.

---

## 5. The method failures, recorded because they were the expensive part

Seven wrong reads in one session, all one shape: **I concluded about what lay outside the
window I actually looked at.**

1. `pgrep` found no solve process → "the run finished." It runs *inside* the core.
2. `/slots` → HTTP 000 → "the lane is dead." `/health`, `/props`, `/v1/models` all 200.
3. Ambient pool read as constant `1` → live-derived since 8/17.
4. `set_served_lane_count` read to line 416 → the `ambient` grow is at 423.
5. **Cumulative probe counts read as current rate** — the exact trap my own plan (Phase 0c)
   was written to prevent. `total_yields: 84` in-process vs 258 rows accumulated across
   reboots.
6. **My own analysis script used `capturedAtMs`** (the CLI's normalized key) against a file
   whose field is `captured_at_ms` — every row parsed as timestamp 0, and I nearly reported
   "no probe activity in 60 minutes" as a finding about the *system*.
7. "We never request `return_progress`" → we request it, parse it, and the watchdog is
   built around it.

Each was caught by one more command. #6 is the one worth generalizing: **an instrument I
wrote myself gets the same scrutiny as the system under test** — it produced a
system-shaped conclusion from a bug in my own five lines.
[[an-absence-is-an-unfinished-measurement]]: *a window is not the world, and that includes
the window your own tool opened.*

---

## 6. Work order

| # | Work | Gate |
|---|---|---|
| 1 | Derive the prefill budget from `prompt_progress` (§4a) | A 58k-token prompt completes instead of failing at 90s |
| 2 | Discriminate the mid-prefill stall (§2) — log `id_slot` per frame; does our slot change? | Slot theft confirmed or killed |
| 3 | If confirmed: declare `id_slot` affinity per resident citizen | Prefill is not preemptible mid-flight |
| 4 | `AdmissionSnapshot` + `watch` publisher (§4b) | One command prints `ambient_held` on a live box |
| 5 | Catalog-driven smoke budget (§4c) | One generation per readiness check, not two |
| 6 | Re-measure `delib.generate.cache` `hit_rate` against the pinned `684c24f8a` baseline | The KV ordering fix proven live |

Step 1 is the unblock — everything else is measuring a system that can currently complete
zero turns. Steps 2–3 are the keystone the KV work already pointed at from the other side.
