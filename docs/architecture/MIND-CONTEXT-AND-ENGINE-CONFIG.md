# A mind's context and the engine it runs on

**Status:** DESIGN, co-authored (Fable, Codex), 2026-10-19. Nothing here is built.
Supersedes nothing yet; it consolidates four designs that each named part of this and
were built in pieces: [PERSONA-CONTEXT-PAGING](PERSONA-CONTEXT-PAGING.md),
[KV-CACHE-ECONOMY](KV-CACHE-ECONOMY.md), [KV-CACHE-AS-SHARED-BASE-LAYER](KV-CACHE-AS-SHARED-BASE-LAYER.md),
[CONTEXT-IS-A-CAPABILITY-AXIS](CONTEXT-IS-A-CAPABILITY-AXIS.md).

## Why

Joel, 2026-10-19: "Feels like it's just rigging up another way each time and not good
architecture." The receipts that day, on the M5 (one 27B, four residents):

| What the substrate did | Measured |
|---|---|
| Restored a mind's KV page, then reused none of it | `inference.restored_turn` cached 0 of 24,670 / 29,999 |
| Restore itself | `serving/cache-probe --roundtrip` 661 of 662 reused |
| Prompt head between consecutive acts | 31-60% shared; the results ring present one act, absent the next |
| Engine prefill / decode on the 27B | 105-115 tok/s / 2-13 tok/s per request |
| Same binary with the GPU matrix units enabled | prefill roughly doubled |

Each fix of the past week (sticky collapse, churn ordering, ring scope labels, a launcher
env var, lane-queue aging) patched a symptom of two faults. This document names the two
and the one owner each gets. It must hold on every machine we ship to: a 24 GB MacBook
Pro, the M5, a 5090, a phone. No number in it is a constant for one machine.

## Fault 1: a mind's context is re-rendered every act

Today each act assembles the prompt from about fifteen sources (results ring, workspace
map, board, roster, working memory, perception, open views) and we chase byte-stability
between renders. Any source that changes moves the prefix and the warm KV is lost. The
existing docs already asked for a deterministic head (KV-CACHE-ECONOMY §3.1) and declared
slot affinity (§3.2); both were built, and the render kept finding new ways to move.

### The design (Codex's correction 1, adopted whole)

- **History is append-only.** Perception, tool results and her own acts are events with
  source, version and supersession references.
- **The model view is an explicit, versioned materialization by epoch**, never an
  ever-growing transcript: a stable identity / policy / tool-schema prefix, then the
  ordered events of the current epoch.
- **A new epoch** is started by compaction, retrieval replacement, privacy revocation or
  a stale action prerequisite. Its invalidation is recorded once. Revocation and stale
  prerequisites never wait for a scheduled compaction.
- **Only deltas become tokens.** Ready artifacts are shared handles.
- **Generated boundary tokens are kept exactly**, including a cancelled generation's
  accepted prefix.
- **Identity is token-level, not string-level**: the same strings can tokenize
  differently when a chat template re-renders, so the materializer owns tokenization of
  its prefix.

Within an epoch the logical input adds only new segments. Backend reuse is measured
against its accepted token frontier; it may also replay a checkpoint tail. Her slot's
KV and recurrent state materialize that frontier, not merely a history event count.

### Continuation contract (Codex)

The existing cognition workspace owns the materialization; existing durable history
and experience remain the history owners. Do not add another event database. A segment
names its source event/version, dependencies and exact token span. Cache representations
once and share handles, including between consumers of the same perception artifact.

A continuation receipt binds the materialization epoch, engine epoch, exact model,
tokenizer, template and adapter identities, accepted token frontier, and recurrent/KV
state identity. Event and token frontiers differ: template boundaries, multimodal inputs
and speculative acceptance are not one-token-per-event operations. Retain the accepted
output exactly; cancellation must reconcile the recorded prefix with the engine state.
Already spoken output belongs in the continuing conversation. Private reasoning is not
published merely because its token identity is retained by the backend.

In the inspected fork, `tools/server/server-context.cpp` around 3780 selects reuse using
the common token prefix and retained memory position; hybrid/SWA rollback can restore a
checkpoint or reset to zero. Around 4144 checkpoints explicitly precede the current decode
batch. Live append from the accepted frontier can avoid rollback; reconstructed chat that
omits or reformats output cannot assume that. Record new-token prefill and replay-token
prefill separately. Cheap append is a contract to demonstrate, not a zero-replay promise.

Superseding an observation need not rewrite history: append its versioned replacement when
the old observation can remain visibly historical. Start a new epoch when the model view
must remove or replace prior tokens. An action validates its required dependency versions
before committing effects; merely appending a correction does not validate an old plan.
Privacy revocation invalidates affected views/caches immediately and follows the existing
retention/deletion policy; append-only history is not a prohibition on authorized erasure.

### Small windows and limited memory

The same contract applies on a phone and a large GPU. Allocate a bounded view from the
existing measured memory and context budget; keep durable source artifacts outside that
view. Compaction publishes a replacement epoch with source references, unresolved work
and provenance retained, rather than silently trimming the head. Reserve capacity for the
next action/result through the existing common budget owner. Measure epoch turnover cost,
replayed tokens and useful-response latency under pressure. Do not introduce a fixed
phone context cap, fixed turnover interval or universal minimum residency count.

### What this deletes (to confirm while designing)

Sticky ring collapse, the churn meter's ordering, per-workspace ring filtering and its
labels as a stability device, and the block-ordering code whose only job is stability.

## Fault 2: engine configuration is decided in five places

`lane_args`, `llama_server`, `backend_receipt`, `serving_plan` and `kv_cache_plan` each
decide part of the launch, with defaults and fallbacks between them. That is how the
GPU matrix units stayed off (opt-in in our engine build), draft placement stayed on the
engine's `auto`, a hung device probe moved a 27B lane to the CPU, and an explicit
`--parallel` silently turned off the shared KV pool.

### The design

- **One typed `EngineConfig`**, derived from (a) what the engine REPORTS about the device
  through a query, never a parsed log (KV-CACHE-ECONOMY §6b) and never a chip name, and
  (b) the seated minds and the measured memory budget.
- **It covers every launch decision**: supported compute operations/dtypes, placement of
  base and draft, KV layout, sequence capacity and checkpoint policy. Sequence capacity
  and active compute capacity differ. An extra slot needs a declared purpose and memory
  budget; `seated minds + 1` is not a universal rule.
- **The engine's own startup receipt is checked against it.** Any mismatch refuses the
  launch, with its cause. There is no fallback path: a miss is a failure.
- **The engine's continuous batching is the decode scheduler.** The core keeps every
  admitted resident's state warm when its budget permits and feeds it. One admission owner, not two in series (today:
  `resource_admission` and `turn_admission`).

### Owner and capability contract (Codex)

Put the resolved serving configuration under existing `inference/planning`. Canonical
model metadata, engine capability evidence, measured resource budget and workload demand
are inputs. Existing serving and KV planners contribute facts/constraints to that owner;
they do not independently overwrite resolved choices. `lane_args` serializes the result,
`llama_server` launches it, and `backend_receipt` checks observed adoption. No sixth policy
layer or duplicate device classifier. Trace each migrated decision and remove its old
default/override in the same change.

Capabilities distinguish compiled support, support for this operation/model/dtype/device,
selected configuration, and observed execution. A `has_tensor` boolean cannot express
that contract. Unknown or unsupported REQUIRED capability refuses with a typed cause.
Optional alternatives are chosen explicitly during resolution with their reason; never
silently substitute CPU or a smaller model after launching. A cache miss with a valid
recovery path is an observable recovery, not automatically a failed conversation.

The inspected `/props` implementation (`get_res_props`, around 5161) reports total slots,
weight buffers, memory breakdown, build info and chat-template capabilities. It does not
provide the complete compute-operation capability matrix needed here. Extend the engine's
typed report, with a prelaunch probe form where selection depends on capabilities and a
postlaunch realized-config receipt. Do not infer adoption from flags or log substrings.

Admission owns resource commitments, while continuous batching schedules admitted
sequences. Independent CPU/I/O/tool/artifact preparation does not hold a whole-turn GPU
lease. Phase-specific transient resources release at a backend-confirmed safe frontier;
KV/state residency lasts until its own eviction/release boundary. These are different
lifetimes, not two queues serializing the same turn.

### Evidence schema to agree before implementation

These are proposed additions to the existing engine report, not a second probe service.
Source inspected: engine revision `50b02b8bb55102665500327b2152fe7e3348e1bc`.

| Evidence | Minimum identity and meaning |
|---|---|
| Device snapshot | Engine build/probe version, stable device identity when available, sample time, free/total memory, backend type, allocator/buffer capabilities. Unknown identity remains explicit. |
| Operation support | Model/operation signature including dtype and shape constraints, device, supported/unsupported/unknown, reason. This describes eligibility, not speed or actual kernel selection. |
| Resolved plan | Configuration identity and input snapshot identities, required choices, selected alternatives and reasons, residency/compute commitments. |
| Adoption receipt | Engine instance identity, resolved-plan identity, actual placement/allocation and execution evidence; each observation says whether it is selected configuration or measured execution. |
| Continuation receipt | View and engine epochs, model/tokenizer/template/adapter identities, source/artifact identities for multimodal spans, accepted token frontier and state handle, reused/new/replayed counts and miss reason. |

The fork already exposes native primitives in `ggml/include/ggml-backend.h`:
`ggml_backend_dev_get_props` supplies memory, type and allocator/asynchrony capabilities;
`ggml_backend_dev_supports_op` takes an actual tensor operation, rather than a universal
device feature flag; `ggml_backend_dev_supports_buft` reports buffer compatibility.
Use these sources for the typed report. They do not prove tensor-unit selection or
latency. A backend-specific execution receipt may still be needed for that claim.
The current server's weight-buffer and memory-breakdown fields are measured allocations;
retain them instead of inventing duplicate placement estimates.

A memory snapshot is not a reservation. The admission owner must validate/commit against
current pressure before launch; a changed budget returns a typed replan/refusal outcome.
The plan cannot manufacture stable device identity from a marketing name. Likewise a
state handle is not transferable across engine instances unless its compatibility and
restore contract explicitly establish that transfer.

## Tests are contracts

- A required capability is resolved and adopted, or the launch refuses with its cause.
- Appending an event never changes the token prefix of the current epoch.
- A new epoch records its invalidation exactly once.
- Every reuse miss reports its cause, replay cost and recovery outcome.
- Cancellation preserves the accepted frontier; incompatible minds/adapters cannot reuse state.
- Resolve, serialize, launch and observe agree on the same configuration identity.
- Late artifacts retain source identity; effects reject changed required dependencies.

Machine numbers belong in receipts on the cards, never in tests.

## Open questions (Codex)

1. Do appends land past the fork's last recurrent checkpoint cheaply on hybrid models, or
   does checkpoint spacing (today `--checkpoint-min-step 4028`) bound reuse per act?
2. Which backend APIs can populate each required operation capability, and how are unknown
   values represented before load? The current props fields alone are insufficient.
3. A small window (a 24 GB Pro, a phone): how often must an epoch turn over, and what does
   that cost per act?

## Build order

Not before both authors sign off. Then: `EngineConfig` first (small, measurable on every
node we have), then the epoch materializer with slots kept warm. Each step ships with
before/after receipts on every node class we can reach.
