# Capacity Fabric & Governor — the live, never-block, grid-elastic compute allocator

Status: DESIGN (2026-07-16). Supersedes the init-time framing in the pre-existing
`governor/` types. Converges — does NOT parallel — the primitives already in tree:
`governor/{types,local,policy_watcher,pressure_bridge}.rs`, `gpu/{metal_monitor,
nvidia_monitor,memory_manager,tracker}.rs`, `inference_capability/{probe,registry}.rs`,
`system_resources/{memory_pressure,disk_pressure,monitor,concurrency}.rs`, the
`PressureBroker`, `serving_plan.rs`, and `resource_admission.rs`. Read
`CBAR-SUBSTRATE-ARCHITECTURE.md` and `CONCURRENCY-STYLE-GUIDE.md` first — this obeys both.

---

## 0. The one invariant (everything else is a consequence)

> **Usable compute is a live, ever-changing quantity — never a fact established at init.
> No node controls its own machine (a game launches, the browser eats the GPU, the OS
> reclaims RAM) and no node controls the grid (peers join, leave, partition, die). Every
> consumer therefore holds capacity LOOSELY: it takes the best available *right now*,
> opportunistically upgrades, and drops back the instant the upgrade evaporates. Nothing
> ever blocks waiting for capacity that may never come.**

Two failure modes this exists to kill, both seen this session:

1. **Init-time freeze.** `classify_hardware() → policy file` answers "what is this
   machine" *once*. It cannot survive a peer leaving at minute 20, and it was wrong the
   instant Opera grabbed the GPU. The classifier stays — but only as one *live-refreshable
   input*, never the frozen answer.
2. **Block-and-wait.** A consumer that acquires a lease and *waits* until granted hangs
   forever when the grid it waited on vanishes. Acquire is always "give me the best you
   can right now" and always returns *something* (local, at some resolution).

The acceptance bar is behavioral, not structural: **if any scenario in §7 waits or hangs
instead of adapting, the design failed.**

---

## 1. Shape: one live watch, three sources, N loose consumers

```
   local probes  ─┐
   (Metal/CUDA/    │
    ROCm/CPU,      ├─►  Governor (RTOS service)  ──►  watch::Sender<CapacitySnapshot>
    live free +    │      • own tokio task                     │
    external       │      • tokio::time::interval tick         │  (re-published on EVERY
    pressure)      │      • + event-driven wake on any input   │   change, monotonic version)
                   │        change (pressure, membership)      │
   grid gossip   ──┘      • re-derives, never blocks           ▼
   (airc: peers'                                     ┌──────────────────────────┐
    advertised live                                  │ Consumers subscribe.      │
    capacity + trust)                                │ Each holds capacity LOOSE:│
                                                     │  serving, animation, TTS, │
                                                     │  STT, YOLO, semseg,       │
                                                     │  diffusion, Bevy, LiveKit │
                                                     └──────────────────────────┘
```

This is the exact RTOS module shape the CONCURRENCY-STYLE-GUIDE mandates (own task +
`interval` + `watch::Sender<Snapshot>` + never a lock across await). The governor is a
`ServiceModule`, a peer of the other monitors, not a god object.

---

## 2. Layer A — live capacity probe (adapter family, per backend)

The one thing that must be *true right now*: how much of each resource is **actually free
to us this instant**, accounting for consumers we don't own.

```rust
/// A live reading. NOT a boot classification. Re-taken on every governor tick and on
/// pressure events. `external_pressure` is the load we don't control (games, browser, OS).
pub struct DeviceCapacity {
    pub silicon: TargetSilicon,        // reuse governor/types.rs — the WHAT
    pub topology: Topology,            // Unified | Discrete{gpus:Vec<Gpu>} | CpuOnly
    pub gpu_total_bytes: u64,          // per-GPU on Discrete; the UMA slice on Unified
    pub gpu_free_bytes_live: u64,      // ← the load-bearing live number
    pub system_ram_free_bytes: u64,    // the CPU-serve fallback budget (4GB-Radeon path)
    pub external_pressure: Pressure,   // how much of the drop is NOT ours to reclaim
    pub thermal: ThermalSeverity,      // reuse — throttles concurrency, not just cadence
    pub power: PowerSource,            // battery → conservative
    pub taken_at_ms: u64,
}

pub trait CapacityProbe: Send + Sync {
    /// LIVE. Called every tick. Must be cheap + non-blocking (spawn_blocking if a syscall
    /// can stall). Returns what is free RIGHT NOW, external consumers subtracted.
    fn probe(&self) -> DeviceCapacity;
}
```

Backends (mostly already in tree — this trait *unifies* them):
- **Metal / Apple-M (UMA)** — `gpu/metal_monitor`. `recommendedMaxWorkingSetSize` minus
  live allocated (ours + external). This is where tonight's OOM lives: the external term
  must be *live*, or 4 prefills spike into the browser's GPU footprint.
- **Mac Intel + discrete Radeon (`MacIntelMetal`)** — discrete VRAM via Metal AND the
  32GB system RAM; when the 4GB VRAM can't hold the model, `Topology::CpuOnly`-fallback
  serves from RAM (the honest-degrade path already stubbed in `serving_plan`).
- **NVIDIA CUDA (multi-GPU)** — `gpu/nvidia_monitor` / nvidia-smi per GPU. `3× 1080ti`
  is `Discrete{gpus:[11GB,11GB,11GB]}` — the probe reports THREE pools, not one sum.
- **AMD ROCm**, **Intel/AMD Vulkan** — probe stubs, honest-degrade until wired.
- **CpuOnly** — `system_ram_free_bytes` is the whole budget.

Outlier-A / Outlier-B that prove the interface (CLAUDE.md discipline): **the 4GB Intel Mac
(CPU-fallback, tiny)** and **the 5090 (discrete, roomy)**. If one trait serves both, the
middle is trivial.

---

## 3. Layer B — the Governor service (continuous re-derivation)

Not a function. A task that owns a `watch::Sender<CapacitySnapshot>` and re-publishes
whenever *any* input changes:

```
CapacitySnapshot {
    local: DeviceCapacity,                 // this node, live
    peers: Vec<PeerCapacity>,              // grid, live (airc gossip; each carries trust + rtt)
    total_addressable: Budget,             // the "sum of all misfit parts" RIGHT NOW
    version: u64,                          // monotonic; consumers detect change
    membership_epoch: u64,                 // bumps on any join/leave — the resilience signal
}
```

- Woken by: the `interval` tick (baseline), **and** every `PressureSignal` from the
  `PressureBroker` (VRAMHigh, SystemMemHigh, Thermal, UserActive), **and** every airc
  membership event (peer up/down). Never polls the grid on a blocking call.
- The existing `GovernorPolicy` (cadence, speculation, consolidation) becomes a
  *projection* of the live snapshot, re-emitted on change — same `arc_swap` publish, but
  driven continuously, not written once at boot. `policy_version` already exists for this.
- **Hardware classification is an input, not the output.** `classify_hardware` runs to tag
  `silicon`/`topology`; the *budget* is re-probed live every tick.

---

## 4. Layer C — the Lease (a recallable loan, never a block)

The heart of "never wait." A lease is **best-effort and revocable**:

```rust
pub struct LeaseRequest {
    pub consumer: ConsumerId,           // serving, animation, tts, stt, yolo, semseg, bevy…
    pub class: ResourceClass,           // GpuCompute | GpuResidency | Cpu | ...
    pub want: Amount,                   // ideal
    pub floor: Amount,                  // minimum to do ANYTHING (else run local-degraded)
    pub priority: Priority,             // realtime > interactive > background
    pub placement: Placement,           // LocalOnly | PreferLocal | AnyReachable
}

pub enum Grant {
    Local { amount: Amount, guard: LeaseGuard },   // always available at SOME resolution
    Remote { peer: PeerId, amount: Amount, guard: LeaseGuard },
    Denied { degrade_to: Amount },                  // "here's what you CAN do right now"
}
```

Rules that make it resilient — **symmetric in both directions** (capacity shrinks AND
grows; on a GPU rig the idle→game→idle cycle frees big VRAM blocks mid-session and we must
*pounce*, not stay throttled at the config we picked while the game ran):
- **Acquire never blocks past a short deadline.** `try local now` → optionally `upgrade to
  a peer` → worst case `Denied{degrade_to}`. A caller *always* gets an actionable answer.
- **Grants are recallable (shrink).** The `LeaseGuard` carries a `revoked` watch. When
  local pressure rises (browser/game grabs GPU) or the granting peer drops
  (membership_epoch bumps), the guard fires `revoked` — the consumer **degrades in place**
  on the next unit of work. Never force-killed mid-token; it just doesn't get the *next*
  slice at high res.
- **Grants are upgradeable (grow) — first-class, not an afterthought.** The same
  `LeaseGuard` carries an `offer` watch: when the snapshot's budget rises (game closed, a
  peer joined, pressure released), the governor signals "more is available." A running
  consumer re-requests on its next unit and is granted more — serving adds lanes / lifts
  the model tier / raises resolution, a deep-lane that was local-degraded re-leases the
  grid, all *live*, no restart. **A machine that freed 20GB ten minutes ago must not still
  be serving at the throttled config.** Same watch, opposite sign.
- **This subsumes tonight's bug.** Serving asks for a lease sized to N concurrent prefill
  spikes; the governor grants only what fits *live* free GPU minus external pressure. No
  `MAX_LANES` constant, no static reserve. The lane/concurrency numbers *fall out* of the
  grant and re-fall-out when the grant is recalled.

`resource_admission.rs` (the ambient permit + directed-lane reservation + inflight gauge I
built) is the **local execution half** of this — it becomes the LocalOnly grant path.
`EVAL-PREEMPTION-LEASE.md` (the fleet quiesce) is a *policy* expressed as leases.

---

## 5. Layer D — derived serving config (the decoupling, live)

Given a live grant, serving derives — and RE-derives on every recall/grow:
- **Model tier** — largest that fits the *live* GPU budget (or CPU-serve on the 4GB path).
- **Residency (KV slots)** — how many warm slots the resident budget holds. Cheap to keep.
- **Concurrency (prefill spikes)** — **separate number**, = `floor(live_free_gpu /
  per_prefill_spike_cost)`. This is the decoupling: 4 warm slots can coexist with a
  concurrency cap of 2 when the browser is hungry, and rise to 4 when it's not — *live*.
- **Window** — from the residency budget after reserving the *measured* (not guessed)
  prefill spike cost. The window-scaled term the OOM taught us — but sourced from the
  harness (§7), never a constant.

Nothing here is a literal. Every quantity is a function of the current grant.

**Reaction-cost ladder (grow AND shrink) — react as fast as the knob is cheap, no faster.**
Capacity on a GPU rig can flap (game opens/closes repeatedly); reloading a model on every
blink is worse than staying throttled. So each knob's responsiveness is tuned to its cost,
with hysteresis rising with cost ([[never-thrash-sticky-hysteresis-on-every-lane]]):

| Knob | Cost to change | Reaction |
|---|---|---|
| Concurrency cap (prefill spikes) | ~free (semaphore resize) | **instant** — the sharp safety valve; shed first on shrink, add first on grow |
| Avatar fps / resolution, TTS quality | cheap | fast, no hysteresis |
| KV slot count (`--parallel` relaunch) | medium | debounced — sustained change only |
| Model tier (full reload) / grid re-home | expensive | strong hysteresis — only on a large, *sustained* delta |

Shrink has one exception: a hard OOM-imminent signal bypasses hysteresis and cuts the cheap
knob (concurrency) *now* — safety preempts smoothness. Everything else eases.

---

## 6. Layer E — policy seam (algorithm now → learned / persona later)

*How* to spend the budget — what to preempt, the degradation order — is a swappable trait,
not hardcoded:

```rust
pub trait AllocationPolicy: Send + Sync {
    fn allocate(&self, snapshot: &CapacitySnapshot, demands: &[LeaseRequest]) -> Plan;
    fn degrade_order(&self) -> &[ConsumerId];   // shed avatar-fidelity → TTS → deep-lane → …
}
```

- **Bootstrap (outlier A):** a deterministic priority+fit allocator. Realtime never
  starves; background yields first.
- **Learned / persona (outlier B):** an ML policy OR a persona-in-charge (#126 self-
  scaling) that *decides* allocation and **remembers what worked** — every allocation +
  outcome is a recorded decision (VDD replayable), so the policy is trainable. "Learn the
  approach taken" = the policy is data, not a branch.

Same seam serves ML compute layers themselves later (the allocator can be a model).

---

## 7. Layer F — resilience is the normal case (the VDD gate)

These are the *tests*, not edge handling. Each asserts **keeps-working + re-flows + never
hangs**, mid-session, no restart:

1. **Yank the network mid-session** → snapshot collapses to `{self}`, membership_epoch
   bumps, remote leases recall, serving drops to local at lower resolution, keeps serving.
   Zero hang.
2. **Kill 2 of N peers holding leases** → their grants recall via membership_epoch;
   consumers degrade in place; work re-lands on survivors. No stall, no lost session.
3. **Add 2 nodes 20 min in** → gossip join bumps membership_epoch; capacity watch grows;
   deep-lane leases start landing on them. No restart, no re-init.
4. **Opera/a game eats the GPU mid-turn** (tonight's OOM) → local probe's
   `gpu_free_bytes_live` drops; the *next* grant shrinks; concurrency backs off live; no
   OOM. The static reserve that was right at boot and wrong now cannot exist here.
5. **A game CLOSES mid-session → capacity GROWS** (the common GPU-rig case) → local probe's
   free budget rises; the governor offers; serving scales UP live — more lanes, a bigger
   model tier, higher-res avatar — and any grid-offloaded deep work re-homes local. No
   restart, and it must not linger at the throttled config it held while the game ran.
6. **Two-tier under all of the above** → the fast local reflexive lane NEVER waits and
   NEVER recalls; the deep/grid lane is opportunistic and merges *if* it arrives. Pulling
   the grid degrades depth, never responsiveness.

The scenario matrix is a fixture set (`DeviceCapacity` + membership timelines) driving the
governor + lease + serving, asserting the behavioral bar. It also times/benchmarks each so
regressions in latency are caught, not just correctness.

---

## 8. Build order (single-machine first, outlier-first, TDD/VDD)

1. **`CapacityProbe` trait + `DeviceCapacity` + 2 outlier backends** (Metal-live-external
   on this box; CpuOnly on the 4GB Intel path) + unit tests that assert sane live numbers.
   *This is where the OOM actually gets fixed — the live external term.*
2. **Governor service**: own task, `watch::Sender<CapacitySnapshot>`, woken by interval +
   PressureBroker + (stub) membership. Re-derives continuously. Tests: a pressure event
   re-publishes a smaller budget within one tick.
3. **Lease layer**: recallable `Grant` + `LeaseGuard.revoked`. Wire `resource_admission`
   as the LocalOnly path. Tests: revoke → consumer sees it on next unit, never blocks.
4. **Serving derives from the live grant** (decoupled residency/concurrency). Kill the
   `MAX_LANES` constant → derived. Gate on the large-prompt burst harness.
5. **Scenario matrix (§7) as the VDD gate** — the four resilience tests, red first.
6. **Grid**: airc membership events + `PeerCapacity` gossip + Remote grants (this week).
7. **Policy seam**: deterministic bootstrap, then learned/persona.

The single-machine slices (1–5) are load-bearing for the grid ones (6–7): a node can't
honestly offer or lease capacity on the mesh until it can continuously, truthfully answer
"what can I do *this instant*" — and adapt when the answer changes underneath it.

---

## 9. The Simulator — one engine that is the VDD gate AND the training gym

Complexity this large is untestable on live hardware alone: non-deterministic, slow, can't
cover the fleet, and you can't pull a real network cable in CI. So the scenario matrix (§7)
is driven by a **deterministic simulator that runs the SAME allocator as production.**

**The move that makes it trustworthy:** the governor reads two traits and cannot tell sim
from prod —
- `CapacitySource` — prod: live Metal/CUDA/ROCm probes + airc gossip. sim: a scenario timeline.
- `Clock` — prod: real time. sim: a virtual clock that runs a 20-min scenario in ms.

There is **no second implementation of the allocator** to drift from reality. A sim scenario
that reproduces the OOM is a real regression test forever; tonight's bug would have been red
in CI before it ever touched Metal.

### Scenario = data (JSON/struct schema)
```json
{ "name": "game-closes-during-video-call",
  "nodes": [{ "id": "this", "silicon": "apple-m", "gpu_gb": 55, "ram_gb": 64 },
            { "id": "peer-5090", "silicon": "nvidia-cuda", "gpu_gb": 32 }],
  "consumers": [{ "id": "asha", "kind": "persona-coding" },
                { "id": "avatar", "kind": "avatar-render" },
                { "id": "tts", "kind": "tts" }],
  "timeline": [
    { "t_ms": 0,       "workload":    { "consumer": "asha", "task": "hard-rs" } },
    { "t_ms": 30000,   "external_gpu":{ "node": "this", "consume_gb": 20, "label": "game" } },
    { "t_ms": 600000,  "constraint":  { "kind": "video-call", "response_budget_ms": 800 } },
    { "t_ms": 900000,  "external_gpu":{ "node": "this", "release_gb": 20, "label": "game-closed" } },
    { "t_ms": 1200000, "membership":  { "join": "peer-5090" } },
    { "t_ms": 1800000, "network":     "partition-all" } ],
  "score": ["perceived_p99_ms","avatar_dropped_frames","coding_pass_rate","oom_count","thrash_count"] }
```

### OOP extension points — one trait per axis (this is the "powerful later")
- `Device` — Metal/CUDA/ROCm/CPU; models free/consume + external pressure. New backend = new impl.
- `Consumer` + a **quality model** — PersonaCoding, AvatarRender, Tts, Stt, Vision. Given the
  granted capacity → emits (latency, quality) signals. New consumer = new impl.
- `NetworkModel` — links, rtt, bandwidth, partition (grid-lease cost + reachability).
- `AllocationPolicy` — the thing under test: deterministic ⇄ learned ⇄ persona-in-charge.
- `Scorer` / `Metric` — computes perception-quality from the recorded run. The objective.

Add a device, a consumer, a metric, a scenario, or swap the policy — all via the trait, no
rewrite. That's the "more ML sophistication later" guarantee, structural not aspirational.

### The scorer: a TREE — compositional, gated, context-weighted, non-linear, cognitive at the apex
The objective is NOT a flat weighted sum. It's a tree whose *structure* encodes honest
truths about what actually ruins an experience — so the score refuses to call a broken whole
"good" just because the parts were:

- **Compositional.** Component `Metric`s (see / speak / listen / render / code-quality /
  latency) → per-experience `Score`s (live-room, code-gen, solo) → one overall `Score`. Each
  level is a function of the level below; small scores feed big ones, and bigger algorithms
  read the smaller results.
- **Gated (weakest-link), not additive, for critical faculties.** Pull a persona's ability to
  *see / speak / listen / render* in a live room and the room score is GATED near-zero — a
  **holistic failure** — *even if every independent component was excellent*. A mute avatar
  in a conversation is not "0.9 minus a bit"; it's broken. `product` / `min` semantics for the
  requirements an experience cannot survive without.
- **Context-weighted (the weights ARE the mode).** Live-room weights responsiveness heavily
  and tolerates deep-lane slowness; **code/project generation weights QUALITY + sophistication
  heavily** (real websites, apps, documentary video — the best we can produce) and barely
  penalizes latency. Same metrics, different weights per situation. Asymmetric: a deep coder
  taking **2× longer** during a live session is a minor ding; a **laggy live interaction is
  severe**. It is far worse to lag the conversation than to slow the thinking.
- **Non-linear / robust penalties (RANSAC-robust cost).** Bad choices — non-working software,
  a laggy live room — fall off a cliff, not a gentle slope. Failure is heavily penalized so no
  amount of component polish rescues a broken whole. **OOM = hard fail (−∞).**
- **Cognitive at the apex — a SOTA judge, glass-boxed, distilled (THE differentiator).** The
  overall-experience rank is the home for the best cognition we can lease (a SOTA model, or a
  strong local one). It reads the component scores + the ACTUAL artifacts — the rendered site,
  the conversation clip, the avatar video — and renders an honest gestalt verdict. Three things
  make it a differentiator, not just a scorer:
  1. **Judgment beats formulas on the whole.** Competitors optimize a metric; we ask a real
     mind "did this actually feel good / is this app actually good?" — taste, coherence,
     believability, the uncanny-valley miss a formula can't capture.
  2. **Glass box (observability-as-substrate).** The verdict + its REASONING is captured via a
     `CaptureSink` — replayable, inspectable: *"code compiled and passed, but the site's UX is
     confused and the avatar's lip-sync drifted → 0.4."* An auditable rationale, not a
     black-box number. (`OBSERVABILITY-AS-SUBSTRATE.md`.)
  3. **Distilled to a local judge (built-to-teach).** Every captured (artifacts → verdict +
     reasoning) pair is training data. A cheap LOCAL apex scorer learns to approximate the SOTA
     judge; over time the SOTA is leased only for the hard/novel gestalts and the local one
     handles the routine — the same "SOTA teaches, local wins" loop as the genome, and
     [[intelligence-is-a-resolution-field-shared-across-the-mesh]]: spend the high-res mind at
     the ONE joint that needs it.

  The score the optimizer consumes stays a scalar; underneath it is honest, transparent,
  trainable cognition. Formula-scored components → a cognitively-scored, glass-boxed whole.

### Training gym + the calibration loop (why it isn't a toy)
The simulator is an RL environment: `env.step(action) → (state, reward)`. A growing scenario
library (weakness→generator: every live incident becomes a scenario) trains the learned
`AllocationPolicy` against the perception reward, deterministically (seeded → replayable).
And crucially the loop closes: the **real benchmark ledger** (decode tok/s, pass rate, latency
per config — what we measure live) **calibrates the consumers' quality models**, so the sim's
fidelity rises with every real run:

> live measurement → calibrate sim → train policy in sim → deploy → measure → recalibrate.

The policy gets smarter without risking a real machine; the sim gets truer with every real
turn. Start with the deterministic policy + the §7 scenarios red-first, grow the library and
the metric set, and plug the learned policy once the gym has enough calibrated scenarios.

## 10. The negotiation economy — airc leases with richer metrics (P2P endgame)

The airc layer is already a **negotiation system** (rooms, agreements, per-peer trust). When
the grid goes true P2P, capacity allocation and airc negotiation converge into ONE mechanism:
a placement is an *agreement between peers*, and lease/hire of anything — GPU lanes, command
execution, storage, a persona's attention — is the same system with **different metrics**
([[grid-agreements-swappable-policy-deterministic-rails]]).

The RANSAC contract makes this free: price, trust, and reputation are just MORE SCALARS
folding into the same `Score` the optimizer already fits. The negotiator (deterministic →
learned → persona, #103) changes; the rails never do.

**The node "resume" is already being recorded.** No new bookkeeping is needed — the glass-box
trace IS the track record:

| Resume line | Derived from (already captured) |
|---|---|
| Reliability ("honors leases") | `stranded_lanes` attributed per peer — did it vanish mid-lease? |
| Quality delivered ("what it's like to run here") | `mean_experience` of placements it served |
| Stability ("doesn't churn") | its reachability history + capacity wobble in the snapshots |
| Honesty ("offers what it has") | offered `PeerCapacity` vs. per-node OOMs on its grants |

A peer's rating/review/resume is a **projection of its `PlacementDecision` history** — signed,
replayable, auditable, exactly like a persona's action receipts. Fake resumes fail against
lived traces (the receipts-grounded-honesty principle, applied to machines). Price then enters
as a per-peer term on the lease request; trust gates WHO may be offered a placement at all
(the airc trust bridge, #38); and the market's search problem ("find me the cheapest
trustworthy node that can hold a 24B with 4 lanes") is the same shop-the-market shape as the
genome exchange ([[search-then-ab-dont-start-from-zero]]) — one economy, compute and skills as
two goods on it.

Sequencing: nothing here blocks the current build order (§8). The seam requirement is only
that `LeaseRequest`/`Score` stay growable (they are: plain structs, add fields) and that
placements keep tracing per-peer facts (they do: `NodeVerdict`). The economy is a policy +
metric upgrade on rails that are already live.
