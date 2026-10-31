<!-- Plan line 3b. Design pass 2026-10-24 at canary c47aa6b24, fork e7e1077b3 (read-only, file:line evidence).
     Joel, 2026-10-24: "take it on full steam. Make it really optimized and advanced, well organized";
     "really optimize and validate by breaking up properly". Every slice below lands with its validator. -->

# Design: continuum's own inference engine (plan line 3b)

## 0. What the code shows

- **Qwen3.8-27B is a hybrid model.** Its GGUF arch is `qwen35`, which `llm_arch_is_hybrid` lists (llama-arch.cpp:1056). It has 64 blocks, and every 4th one is attention, so 16 attend (gguf_keys.rs:528-538). The KV cache costs 65,536 B/token at f16, about 34.8 KB at q8_0.
  - A partial `seq_rm` cannot roll back the recurrent state. The server therefore restores from checkpoints (server-context.cpp:3812-3912) and creates them at user-turn starts and at n-4 and n-4-ubatch (4003-4016, 4111).
  - Shifting `cache_reuse` is switched off for such contexts (1210-1212).
- **Checkpoints are the hidden memory cost.** e7e1077b3 measured 27 checkpoints taking 11.4 GB of sidecar next to a 2.2 GB state, and the save took 66-100 s. The checkpoints sit in host RAM, which on this machine is the same 64 GB the GPU uses.
- **The KV fits.** Four minds at 59k tokens is 4 × 59k × 34.8 KB, about 8.2 GB. With one unified pool, 4 resident minds fit next to the weights. Today's 2 slots come from the launch geometry and the checkpoint cost, not from a shortage of KV.
- **The in-process scheduler is a weaker copy of the fork.**
  - Every prefill starts at 0 and the KV is freed at the end of each sequence (llamacpp_scheduler.rs:30-32, 691).
  - Prefill fills `n_batch` in seq-id order before the decode tokens of higher seq ids (361-420). That is the starvation inproc_bench measured.
  - It has no MTP, no checkpoints, no jinja templates, no tool parsing, and none of the learning duties the fork's server has gained in its last 40 commits.
- **The fork's server is already a library.** `server-context` is a STATIC target "used by llama-server and CLI" (tools/server/CMakeLists.txt). The CLI drives it in-process through `server_response_reader` (server-context.h:107-115). Today core/llama builds with `LLAMA_BUILD_SERVER=OFF` (build.rs:46) but already links `llama-common` (316).
- **Weights reload today whenever the context changes.** `destroy()` frees the model along with the context (server-context.cpp:962-976), and slots are fixed from `n_parallel` at load time (1252). The core relaunches to grow the window (reconcile.rs:3-13).

## 1. Crate and modules

`core/continuum-serving` is the Serving module. Its mechanism lives in the fork as a C ABI shim, `tools/server/server-continuum.{h,cpp}`, which can reach `server_context_impl`. The safe Rust binding goes in `core/llama/src/server.rs`.

| Module | Single owner of | Seam trait |
|---|---|---|
| `lifecycle` | Model loaded once with mmap; context generations | `EngineLifecycle { reconfigure(Geometry) -> PauseReceipt; geometry() }` |
| `seq_table` | Mind (`PageKey`, page_key.rs) → seq id, residency Gpu/Ram/Disk, token frontier, gene set, epoch | `SequenceTable { locate(PageKey) -> Residency; bind/unbind }` |
| `scheduler` | Policy only: temperature as priority, aging, and which minds stay resident. The mechanism stays the fork's `update_slots`: continuous batching plus the time-fair interleave (3263, 3613) | `Admit { admit(ActRequest, Temperature) -> Ticket }` |
| `pager` | `state_seq_get/set_data_ext` to RAM; spill to disk in today's format | `Pager { page_out(seq, Tier); page_in(PageKey) -> PageIn }` |
| `prefix` | Shared heads (see below) | `PrefixShare { share(HeadId, SeqId) }` |
| `checkpoints` | Where, how many and which go to disk (oldest plus newest N, e7e1077b3), within a measured byte budget | `CheckpointPolicy` |
| `genes` | Adapter registry; homogeneity groups (a slot's KV is cleared when its adapters change, 1697-1700) | `GeneSet` |
| `sampling` | Typed parameters into `common_sampler`, grammar and stop; no logic of its own | — |
| `speculative` | MTP configuration and acceptance probes; the logic is `common_speculative` | — |
| `template` | One fidelity owner: `common_chat_templates_apply` / `common_chat_parse` (common/chat.cpp, common/jinja) | `Render` |
| `http` | `server_routes` from `llama-server-impl` in the same process, so external drivers like Hermes share one engine and one budget | — |
| `port` | What minds call: binary frames, no HTTP or JSON | `ServingPort { act(PageKey, ActRequest) -> TokenStream; warm/cool(PageKey, Temperature); capacity() -> CapacityReport }` |
| `probes` | Metrics: reuse, page-in ms, wait, footprint | — |

**Prefix sharing on a hybrid model.**
- With a unified KV, `seq_cp` only rewrites cell metadata (llama-kv-cache.cpp:462-466).
- The recurrent `seq_cp` shares only the source's tail cell, which is the state at the source's end (llama-memory-recurrent.cpp:251-271).
- So a shared head is either a dedicated "head seq" ending exactly at the boundary, or `seq_cp` of the attention cells `[0,p)` plus the head's checkpoint loaded with `PARTIAL_ONLY`.
- Prefer the checkpoint route. A shared tail cell combined with MTP rollback hits `GGML_ABORT("cannot write shared recurrent state with different rollback indices")` (774).

**Page format.** Disk pages stay `llama_state_seq_save_file` output plus the `.ckpt` (0x434B5054) and `.lora` sidecars (2716-2826). `a-<persona>.bin` and `LEDGER_FORMAT 2` (saved_ledger.rs:36-62) then carry over unchanged. One gap: the `.bin` holds only `ctx_tgt` (2809), so the MTP draft KV comes back cold.

## 1a. `MindMemory`: the `seq_table` and `pager` owner over today's llama-server (2026-10-27)

**Why now.** On 2026-10-27 a mind's KV memory broke four ways in one afternoon on the M5:
- the tool head changed at launch;
- a save was cut by the core's exit;
- a resident was left unsaved in her slot at the stop;
- the boot plan was computed before the ledger was credited.

Each was patched in one lifecycle path (#5538, #5540). The inventory that followed, at canary 107af9167, found 41 paths and 9 decisions made in 2 to 6 places each:
- who may evict a resident;
- when a page is restorable;
- the save-the-evictee procedure, written three times;
- save-all before the engine dies, four times, and missing in idle retirement;
- recording residents lost unsaved;
- quarantine;
- slot ownership;
- scratch policy;
- page-dir ownership.

Joel: "you can't get anything to work unless there is one daemon and centralized location." So `seq_table` and `pager` (§1) are built FIRST, as one owner speaking HTTP to today's llama-server (step 1 of §2). When S1–S5 land, its mechanism changes and its callers do not.

**One owner per engine endpoint: `MindMemory`.** It holds the slot attribution, the saved set, the ledger file, page IO and the quarantine. Nothing else calls `/slots/*` or writes the ledger. It runs as a `ServiceModule` in the canonical shape (CONCURRENCY-STYLE-GUIDE). Its stop is its own `SettleBeforeStop`.

| Request | Replaces | Contract |
|---|---|---|
| `seat(PageKey, Purpose) -> Seat` | `lease` + `pin_slot` + `plan_paging` + the turn's paging closure (turn_admission.rs:344-472); `borrow_slot_for_transient` (:517); `warm_ahead`'s paging; `cache_probe`'s raw `/slots` IO | RAII. She holds a pinned slot with her KV in it (resident, or restored from her page), or the seat says why she starts cold. `Purpose` is Turn, WarmAhead or Transient: a Transient seat may borrow the cheapest resident's slot, which is saved first. |
| `pause(Why) -> PauseReceipt` | `checkpoint_before_replacement`, `checkpoint_owned_restore`, `checkpoint_adopted_before_replacement`, `checkpoint_every_endpoint` (stop), and the missing save in `idle_if_current` | Closes seating and drains the seats, then saves every resident with a confirmed save. A resident whose save fails is recorded as lost, by name. Every engine retirement goes through it: relaunch, academy, idle, adopted retirement, stop. |
| `resume(PauseReceipt, Generation)` / `adopt(Generation)` | `ready()`'s pool swap and ledger carry or load; `adopt_engine_ledger`; `resume_suspension` | Boot, relaunch and adopt load the one ledger by the one rule (fingerprint plus incarnation), and seat nothing. |
| `forget(PageKey, Why)` | `note_page_lost` on a refused restore; nothing today on an in-place gene change | A page whose KV no longer means what it did (a gene change, a refused restore) stops being restorable. |

**Inside it, each decision exactly once:**
- `evict(slot)`: the single save-the-evictee procedure (invalidate, save, confirm).
- `restorable(key)`: the single rule for when a page is offered.
- the ledger written only on its own transitions;
- quarantine owned and cleared by it;
- the scratch policy decided once, in `seat`.

**Deleted when callers move:**
- the paging block of `turn_admission.rs` and `borrow_slot_for_transient`;
- `slots/page_io.rs` (#5540's gate, which becomes `MindMemory`'s own stop);
- the four `checkpoint_*` variants;
- public `take_resident` / `forget_resident` / `plan_paging` / `note_saved` / `note_page_lost`;
- `trim_page_store_after_save`, which has no caller; the page store's trim moves into the owner or is deleted.

**The API (published for the callers' migration; Codex's review of 2bd37de22 is built into the types).**

```rust
/// Which engine process a receipt or seat belongs to: its generation, and for an adopted
/// engine the OS incarnation (pid, start time) verified independently of any launch handle.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct EngineIdentity { pub generation: Uuid, pub incarnation: Option<(u32, u64)> }

pub enum Purpose { Turn, WarmAhead, Transient }

/// Where a seated mind's KV came from. Every cold seat names why.
pub enum Resumed { Resident, Restored, Cold(ColdCause) }
pub enum ColdCause { FirstVisit, PageRefused, DiscardedUnsaved(LossCause), Quarantined }

/// RAII. The slot is pinned while the seat lives. `completed` commits the mind's attribution
/// (and her tail size, the eviction price); a seat dropped without it detaches the slot.
/// A Transient seat never holds a mind: it borrows the cheapest resident's slot, which is
/// saved (confirmed) before the seat is granted, and is not a driver session (drivers seat
/// as Turn with `PageKey::driver`).
pub struct Seat { /* slot, key, resumed, engine */ }
impl Seat {
    pub fn slot(&self) -> u32;
    pub fn resumed(&self) -> &Resumed;
    pub fn engine(&self) -> EngineIdentity;
    pub fn completed(self, tail_tokens: u64);
}

pub enum PauseWhy { Relaunch, Academy, Idle, AdoptedRetirement, Stop }
pub enum LossCause { SaveFailed(String), DeadlineSpent, EngineExited, PagingUncertain }

/// Not Clone, not serializable: one pause, one consumer. Bound to the engine it paused.
pub struct PauseReceipt { /* engine, why, ... */ }
impl PauseReceipt {
    pub fn engine(&self) -> EngineIdentity;
    pub fn saved(&self) -> &[PageKey];                    // confirmed by the engine's ack
    pub fn lost(&self) -> &[(PageKey, LossCause)];        // named, and recorded for the successor
    pub fn unresolved(&self) -> &[PageKey];               // IO whose outcome is unknown: quarantined
}

pub enum Attach { Launched, Adopted { incarnation: (u32, u64) } }

impl MindMemory {
    /// The only way onto a slot. Refused while paused, or once the deadline passes.
    pub async fn seat(&self, key: Option<PageKey>, purpose: Purpose, deadline: Instant) -> Result<Seat, SeatRefused>;

    /// Close seating, wait for every open seat to close (a seat closes BEFORE its slot is
    /// detached or saved), then save every resident with a confirmed save. Returns only when
    /// no page IO it started is in flight: IO still running at the deadline is listed
    /// `unresolved` and the endpoint is quarantined, so a retirement can never race it.
    pub async fn pause(&self, why: PauseWhy, deadline: Instant) -> Result<PauseReceipt, PauseRefused>;

    /// Reopen the SAME engine (an academy batch done). Refused for a receipt of another
    /// engine, for a quarantined endpoint, and for a Stop pause (an operator stop stays stopped).
    pub fn resume(&self, receipt: PauseReceipt) -> Result<(), ResumeRefused>;

    /// The engine may now be killed. Consumes the receipt; its losses go to the ledger the
    /// successor reads. Refused for a receipt whose engine is not the current one.
    pub fn retire(&self, receipt: PauseReceipt) -> Result<Retired, RetireRefused>;

    /// Boot, relaunch or adopt: bind a new engine and load the one ledger by the one rule
    /// (page-format fingerprint; residents in place only for an Adopted engine whose
    /// incarnation matches the ledger's). Seats nothing.
    pub async fn attach(&self, engine: EngineIdentity, how: Attach) -> Result<Attached, AttachRefused>;

    /// A page whose KV no longer means what it did is never offered again.
    pub fn forget(&self, key: PageKey, why: ForgetWhy);

    /// For the one scheduler (G2, Codex): per mind, residency, frontier and last activity,
    /// residents and drivers alike, as a watched snapshot.
    pub fn snapshot(&self) -> tokio::sync::watch::Receiver<MemorySnapshot>;
}
```

**Validator (S0 for this owner).** `turn_admission/tests/lifecycle.rs` already runs a fake engine whose slots hold `mind:tokens`, through turns, a stop and a boot. It moves to `MindMemory`'s own tests and gains:
- a relaunch;
- an adoption;
- idle retirement;
- an in-place gene change;
- transient borrowing.

The validator drives the actual retirement callers (relaunch, academy, idle, adopted retirement, stop) through `pause` and then `retire` or `resume`, never the checkpoint helper alone, including a second stop after an adoption (Codex).

Each scenario asserts where every mind resumes, in seconds, before any live read.

**Order and owners.**
1. `MindMemory` with `seat` and its own stop, with turn, warm-ahead and transient moved onto it (Fable).
2. `pause` and `resume`, with relaunch, academy, idle and adopted retirement moved onto them (Codex: the relaunch call sites are S4's).
3. `forget` on gene change (Fable).

Each step deletes what it replaces in the same PR, and the lifecycle validator gains that step's scenario first.

## 1b. What concurrent minds cost on the M5: measured 2026-10-28

These findings come from engine-c 741b0175b with Qwen3.6-35B-A3B Q4_K_M, on 64 GB of unified memory. Every number is a receipt, and each one answers "is it llama.cpp, the server, or continuum?"

**Decode across layers** (short prompts, 256-token decodes; t/s per stream / aggregate):

| Streams | llama-batched-bench (no server) | Our server, no draft | Our server, MTP draft n_max 4 |
|---|---|---|---|
| 1 | 59 / 59 | 57 / 57 | 80 / 80 |
| 2 | 45 / 90 | 44 / 89 | 42 / 84 |
| 4 | 31 / 124 | 31 / 122 | 23 / 91 |

- Nothing serializes: the server batches like stock llama.cpp.
- Batching amortizes weight reads, so the aggregate climbs. On this MoE model it reaches only about 2.1× at four streams, because each sequence routes to its own experts.
- The MTP draft spends the same spare capacity that batching does. It wins alone and loses under concurrency.
- Fix: `inference::speculation` drafts only while a turn decodes alone, set per request.

**Live decode by concurrent engine work** (16k `tg_3s` samples, 2026-10-27):

| What else is running | Median t/s |
|---|---|
| Nothing | 20.6 |
| 1 other decoding | 11.2 |
| 2 others decoding | 8.7 |
| 3 others decoding | 8.3 |
| Another slot prefilling | 14.7 |
| Any page save or restore | 4.6–5.6 |

A page save or restore stalls every stream, because SLOT_SAVE and SLOT_RESTORE run on the server's main loop.

**Page IO, isolated against live:**

| | Bytes | Save | Restore |
|---|---|---|---|
| Isolated (99k tokens, SSD) | 0.7 GB | 0.22 s (3.3 GB/s) | 0.1 s |
| Live (92 saves, 95 restores) | 1.88 GB, of which 1.24 GB is checkpoint sidecar | 3.05 s median (0.59 GB/s) | 1.38 s (1.32 GB/s) |

- The engine's IO code is fast.
- Live pages carry about 2.5× the bytes, mostly checkpoints of the hybrid model's recurrent state.
- Live pages also move 5–6× slower. The suspect is memory pressure: 8.4 GB of swap in use, with the engine's 17 GB `--cache-ram` and `--ctx-checkpoints 32` beside the 22 GB model and 4 × 159k KV. This is being sampled.
- The size quote is cheap and is ruled out.

**What follows:**
1. Size the prompt cache and checkpoint retention from one measured memory budget, so the engine never pushes the machine into swap.
2. Pages carry only the checkpoints a resume needs.
3. Page IO goes RAM-first and off the main loop (S1), so a switch never stalls the other minds.
4. Expert paging for capacity on unified memory (card 5eedda3e).
5. A long-context sweep: per-sequence KV reads are what batching cannot amortize.

## 1c. The engine is fed by queues and signals, never held by permits (2026-10-28)

Joel, 2026-10-28: "Its got to flow or have MANY queues"; "Semaphore really only exists for queue
inserts and signals typically. Otherwise you lock up every thread." The CBAR shape applied to
serving: stages run concurrently, work is submitted, the engine drains what is ready.

**What it replaces, measured on the M5 (176 acts, build 7ace74020).** A resident's model call
passed four permit layers that each counted the same engine slots and were held for the whole
call: cognition's serving lane (one of four reserved for directed calls, lent only to calls under
60 s, which a 71 s median act never is), a prefill throttle (1 to 2 permits for 72% of the time,
guarding a compute-buffer spike one engine never draws), the adapter's concurrency permit, and the
slot pool. Waiting on the first two: a median 25.7 s plus 24.6 s, in a 155 s median act, while the
engine itself was occupied 99.6% of the time and four streams decode 3.4x one stream at 64k. The
locks, not the engine, made minds wait. Each was added after an incident instead of fixing it.

**The rule.**
- A mind *submits* a generation (request plus a reply handle) and keeps flowing: tools,
  perception and the prefill of her appended items run while she waits.
- **One dispatcher per engine**, woken by events only: a submission arrived, a slot finished, a
  page landed. It fills free slots from per-class queues (directed, work, background), oldest first
  within a class with aging, so nothing starves. Priority is queue order, never a reserved slot.
- **Admission is memory, once.** `MindMemory::seat` admits a mind by KV bytes. No per-call permit
  counts slots a second time.
- A semaphore only signals: an item entered a queue, or a slot freed. No permit is held across a
  model call, a tool, or an await on another subsystem.
- The engine's own continuous batching is the concurrency: every seated sequence with tokens to
  process rides the same forward pass.

**Classes, the GCD way** (Joel, 2026-10-28: "no one is ever not getting their task done so a dream
still happens just irregularly and doesn't eat into work"). Each mind is a serial queue (one act at
a time) that targets a quality-of-service class; the engine is the concurrent pool beneath them.

| class | work | rule |
|---|---|---|
| interactive | a directed reply | the next free slot; preempts a lower class at a decode-step boundary |
| work | a resident's act on her card, with her tools | runs whenever ready; never waits behind the classes below |
| utility | ambient thought, perception, gene exams | fills what the two above leave |
| background | dreams, training, consolidation | runs in the gaps; yields at the next step to anything above |

Background is throttled, never starved: a waiting task's effective priority ages upward, so a dream
gets its step even on a busy node, irregularly and in small pieces, and it only ever takes a slot
no higher class wants now (preempted by `page_out`, ~0.1 s, the moment one does). Training is the
same: the engine's idle task, never a hold on serving.

**One owner, one heap.** The dispatcher task owns its ready heap and receives events over a channel
(submitted, slot freed, page landed, priority aged); it pops only when a slot is free. The kernel's
`concurrency::ConcurrentPriorityQueue` forwards each item to an unbounded output channel as soon as
it is signalled, so it does not keep priority order under load; the dispatcher does not build on it.

**Done so far** (`fable/lanes-flow`): the reserved lane is lent whenever nothing directed is
pending; the prefill throttle is deleted; deliberation and dreams take no lane. Left: the adapter's
concurrency permit and the slot pool become the dispatcher above, the ambient anti-stampede permit
becomes a class in it, and the act clock's queue-time registration goes with the lane.

## 2. Reuse versus write

| Option | Speed | Effort | Risk |
|---|---|---|---|
| A. Grow `llamacpp_scheduler` into a full engine in Rust | Saves about 1 ms of HTTP per call, nothing that matters against 60 s | Months: rewrite checkpoints, MTP, templates, tool and reasoning parsing, learning custody | Template and hybrid correctness |
| B. Link `server-context` as a library and add mechanism verbs | Identical engine and every fork feature | Weeks: about 1-2k lines of C++ verbs plus the Rust host | C++ exceptions crossing FFI; private impl |
| C. llama-server plus new HTTP endpoints | Same as B | Lowest | No Rust policy; JSON on the hot path |

**Recommendation: B, in two steps.** The new verbs go into the fork's `server-context` first:
- `page_out`/`page_in` to a named RAM page, with disk spill;
- `reconfigure` without reloading the model;
- per-task priority;
- `share_head`.

Step 1 exposes them over today's llama-server HTTP, which is option C, so the gains land before the host changes. Step 2 has a Rust `continuum-serving` binary link `server-context` and `llama-server-impl`. Rust owns the policy modules and serves the binary port, and the same process serves the OpenAI HTTP adapter. That keeps the endpoint working with exactly one copy of the weights. Hosting llama-server next to an engine inside the core would load the weights twice, which 64 GB cannot afford.

`llamacpp_scheduler` stays for small and embedding models. inproc_bench becomes the validator harness, retargeted to `ServingPort`.

## 3. Memory model

- **Budget.** After weights load, the budget is the device's measured free memory (`ggml_backend_dev_get_props`) minus the measured compute buffers (`memory_breakdown`, server-context.h meta). It is split into:
  - the KV pool, measured as `get_size_ext` per token;
  - fixed recurrent cost per sequence, measured as `get_size_ext(PARTIAL_ONLY)`; the recurrent size is `n_seq_max` cells (llama-model.cpp:2489);
  - checkpoints, at their measured bytes;
  - the RAM page tier.
- **Sizing comes from measurement, not counts.** `n_ctx` is the KV pool divided by bytes per token. With a unified KV, any seq can grow to `n_ctx` (llama-context.cpp:286), so windows are never fixed per slot and never shrink. Sequence capacity `n_seq_max` is the #5188 contract: the most sequences whose fixed cost fits. Admission seats a mind when her frontier fits; otherwise it pages a victim out.
- **Victims.** Evict the lowest score among unpinned minds. The score is temperature (with aging) weighed against cost: the bytes to save versus tail tokens divided by the measured prefill rate. The page goes to RAM first, which is a memcpy on unified memory. RAM spills to disk on pressure or by temperature, and the disk write runs off the engine thread on bytes already in RAM.
- **"A relaunch is a pause"** means:
  1. stop admitting and drain at a ubatch boundary;
  2. `get_data` every resident seq (target, draft and checkpoints) into RAM;
  3. `llama_free` both contexts, keeping the model and its mmap;
  4. `llama_init_from_model` with the new geometry;
  5. `set_data` everything back and resume.

  A KV-type change is classified as an intentional cold start. A process exit saves to disk. The next process maps the weights warm from the page cache and pages minds in lazily by temperature.

## 4. Speed levers, ranked by gain per effort

1. **[actions] moved to the tail (mind side).** The ring sits mid-prompt and forces 16-29k tokens of re-prefill per act (MIND-CONTEXT §22-48). Measure: replay reuse 35% → ≥90%, and fresh tokens per act.
2. **No re-prefill on mind switch.** Keep 4 seqs resident and use RAM paging. Today each switch costs a 66-100 s save, or 70-160 s of re-prefill at about 190 tok/s. Measure: page-in ms, fresh tokens per act, and `PageIn::Restored`/`Resident` share (turn_admission.rs).
3. **Batched decode of N minds.** Measure: aggregate tok/s at 1/2/4 minds against solo.
4. **Host-RAM KV cache.** This is the mechanism behind lever 2 once the pool is over budget. Measure: RAM restore ms against disk restore ms.
5. **MTP.** Measure: acceptance and decode tok/s, especially right after a page-in, because the draft comes back cold.
6. **Prefill during tool calls.** Measure: submit-to-first-token.
7. **Prefix copy.** About 0.5 GB per mind, and it only helps a cold mind's or a fork's first act. Measure: first-act prefill tokens.
8. **KV quantization below q8_0.** Little gain once 4 minds fit. Measure: footprint, plus a quality A/B.

## 5. Build order

| # | Slice | Validates with | Owner |
|---|---|---|---|
| S0 | `serving-replay` validator: captured act sequences against a small hybrid GGUF, in seconds, with no residents | reuse, fresh tokens, page-ins, relaunches = 0 | Fable |
| S1 | Fork: RAM page verbs, checkpoint budget, `.dft` sidecar | S0, plus restore ms | Fable |
| S2 | Fork: `reconfigure` without reloading the model | 100 resize cycles with flat footprint | Fable |
| S3 | Fork: per-task priority and aging in slot selection; `CapacityReport` | starvation fixture | Fable (mechanism), Codex (policy) |
| S4 | Core: admission consumes `CapacityReport`; relaunch call sites (serving_daemon/launch.rs:221, reconcile.rs) call `reconfigure` | no relaunch while serving | Codex |
| S5 | `continuum-serving` host and `ServingPort`; M5 side-by-side against llama-server (4 minds, growing contexts) | act time, wait, reuse, footprint | Fable |
| S6 | Per-node `EngineConfig.engine` switch; llama-server stays the adapter for external drivers until S5 wins | line 3b gate | Codex (config), Fable |
| S7 | Prefix share | first-act prefill | Fable |

Lever 1, the [actions] move, runs in parallel on the mind side under line 2 (Fable + Codex).

### S2 in detail: `reconfigure`, grow or shrink live (Joel, 2026-10-30: "a truly dynamic llama.cpp that can reduce OR EXPAND slots is crucial"; "make it organized, not some mess")

**Why:** a user starts a game or Photoshop and the engine must give memory back; they quit and it takes it
again (ONE-BUDGET.md, acceptance: the user's own load). The fork today can only do it by relaunch: its sleep
path calls `destroy()` then `load_model()` (server-context.cpp:978-993), which frees and reloads the weights.

**Shape.** Our code in the fork is held to our rules: the logic lives once, in its own module, and the
6,495-line `server-context.cpp` gains only a hook.
- `server-reconfigure.{h,cpp}`: one entry, `reconfigure(const Geometry & target) -> ReconfigureOutcome`.
  - `struct Geometry { int32_t n_ctx; int32_t n_seq; }` in;
  - `ReconfigureOutcome` out: `Applied { Geometry live; }`, or `Refused { std::vector<SlotRefusal> }` where
    `SlotRefusal { int32_t slot; int32_t tokens_held; int32_t tokens_fit; }` names each slot whose state the
    target cannot hold. Typed in the core path; JSON only at the HTTP edge (`POST /reconfigure`).
- `server-slot-page.{h,cpp}`: the one state mover. `SlotPage page_out(slot)` (target, draft and checkpoints via
  `llama_state_seq_get_data`) and `page_in(slot, const SlotPage &)`. The existing `/slots?action=save|restore`
  become callers of it (disk is a `SlotPage` written to a file), so there is one copy of the mover, not two.
- Weights and contexts are separate lifetimes, today fused in three places: `common_init_result` owns the target
  model and context together (`common.cpp:1404`); `common_speculative_init_result` both loads the draft model and
  creates the draft/MTP context (`speculative.cpp`); `load_model` builds weights, contexts, slots and batch in one
  pass (server-context.cpp:1030-1300). The split: `load_weights()` (target model, draft model, mmproj: these
  survive) and `build_contexts(Geometry)` (target and draft contexts, speculative state, slots, batch: these are
  rebuilt). `load_model` = `load_weights` + `build_contexts`; `destroy` = `destroy_contexts` + `free_weights`;
  `reconfigure` = page out, `destroy_contexts`, `build_contexts(target)`, page in. The two common-side owners
  split the same way, so the context is created in exactly one function per kind.
- The slot page reuses what exists: `server_prompt_cache_state` (prompt tokens with their host-memory
  checkpoints, `{main, drft}` state bytes, adapter set) is the page. Its capture (`server_slot::prompt_save`,
  server-context.cpp:276-300) and restore (`server_prompt_cache::load`, server-task.cpp:1895-1930) become
  `page_out` and `page_in` over an exact slot, and the prompt cache calls them. Checkpoints are host memory
  already, so they survive a rebuild untouched.
- The hook: a `SERVER_TASK_TYPE_RECONFIGURE` handled in the task phase, the safe point the fork already argues for
  `SLOT_SAVE` (server-context.cpp:2685): no `llama_decode` in flight, every sequence's KV at a consistent
  inter-batch boundary. The safe point is between BATCHED decode steps, never "between generations": every
  slot is paged out including unfinished generations, the contexts are rebuilt, everything is paged in, and
  each generation continues from its next token with its host-side sampling state. Waiting for generations to
  end would let four busy writers, or one ten-minute generation, hold the person's game hostage (Cormac on
  #5607). Drafting restarts on the rebuilt speculative object. Slot save and restore serialize through the
  same safe point. New work waits in the queue; nothing is
  cancelled. Slot ids stay stable: a smaller `n_seq` removes the highest ids, and a slot among them, busy or
  not, is paged out now with its unfinished generation (contract 2), named in the outcome; it continues when
  residency places it again.

**Contract.**
1. The model and its mmap are never freed; only contexts are (sequence in section 3, "a relaunch is a pause").
2. Shrink never truncates. A slot whose state does not fit the target (a smaller `n_ctx`, or an id past a
   smaller `n_seq`) is paged out now, through the same mover (RAM, then disk by temperature), never cut. The
   outcome's `SlotRefusal` names it so the core knows; residency (L4) then chooses by temperature between
   keeping it paged until room returns and shrinking that mind's view at her next epoch start (Cormac on
   #5607). Nothing waits for an epoch to free the person's memory.
3. Grow and shrink are the same path.
4. Each reconfigure reports its cost: pause seconds against tokens held, per backend, which L4 and L6 price.
   Footprint is measured as `phys_footprint` on Metal and device memory on CUDA.
5. The applied geometry is reported (`/props`, the `CapacityReport` of S3), so serving and every mind's
   binding read the live window; the core never infers it.
6. A KV type change is not a reconfigure; it is the intentional cold start (#5188).

**Validator (seconds, small hybrid GGUF, no residents):** four generations in progress across a reconfigure
finish token-identical to an unreconfigured control; and 100 cycles alternating grow and shrink with four
sequences holding state: footprint flat (phys_footprint before and after within the measured noise), weights
never reloaded (load counter), every sequence's state byte-identical across the round trip (its next token
matches an unreconfigured control), a shrink below a held state pages that slot out and names it, and no request
lost (queued requests all complete). **Core (S4, Codex):** the relaunch call sites (`reconcile.rs` re-home,
`launch.rs:221`) call `reconfigure`; a plan below the live geometry now reconfigures too, which is what
gives memory back to the user's app.

## 6. Risks

- **Metal and unified memory.** RAM pages, checkpoints, KV and weights all draw on one physical pool, under macOS's GPU wired limit. If the budget is not one pool, the machine swaps and everything stalls. `get_data` has to synchronize the context first. `LLAMA_STATE_SEQ_FLAGS_ON_DEVICE` (llama.h:1024) suits speculative checkpoints only.
- **Hybrid checkpoints.** Checkpoints are about 420 MB each (from e7e1077b3's numbers), so their count must be budgeted. Partial `seq_cp` is wrong for recurrent state. The shared-tail abort at llama-memory-recurrent.cpp:774 is a hazard.
- **Template fidelity.** Keep one renderer, common/chat. Test: captured requests produce identical tokens through llama-server and through the port.
- **LoRA per-seq mixing.** The server requires equal adapters to batch (server-context.cpp:432-437). Each mind's seq keeps her own gene set. Mixing different genes in one ubatch needs per-token adapter kernels, so defer it and measure what group serialization costs.
- **Crash isolation: run the engine as a separate process, not inside the core.**
  - The engine aborts on paths we exercise: line 774, and the failed reload on wake at server-context.cpp:988.
  - The core redeploys several times a day. An engine inside the core would lose all GPU KV and reload the weights on every deploy.
  - The ledger's incarnation and resident adoption (saved_ledger.rs) already let a restarted core take over a live engine with warm KV.
  - The IPC cost is one request per act plus 30-120 token frames a second over a unix socket: microseconds against a 60 s act.
  - MODULE-ARCHITECTURE §9.2 rule 2 asks for one protocol per seam, local or remote.
  - The in-process `ServingPort` implementation exists only for the validator and tests.
