# The mind's crate map: organizing the data-flow RTOS mind, then migrating into it

Status: PLAN FOR DISCUSSION (2026-11-01), canary after #5686. The organization that
[THE-WHOLE-MIND](THE-WHOLE-MIND.md) needs, measured against the crates as they are, with the
migration order. Placement follows [MODULE-ARCHITECTURE](MODULE-ARCHITECTURE.md) §9.2; the stage
contract is THE-WHOLE-MIND §1b.

> Joel, 2026-11-01: "We need to break the mind into modules, crates, mechanisms to make this data
> flow RTOS mind as it's envisioned work from first a high level organization and of course
> migration of what we have working into it; then start cracking on forming that ball of clay into
> the ideal autonomic agency mind that learns and thinks deeply, all on its own and all as fast as
> we can make it. Optimize everything. This is extremely concurrent and not FIFO or batch."

## 1. Where we are (lines of Rust per crate, internal deps; canary 2026-11-01)

The mind lives mostly in five crates that each mix several concerns:

| Crate | Lines | What is inside |
|---|---|---|
| continuum-engine | 110,806 | inference 66k (lanes, admission, KV, knees, llama server), capacity 14k, ai adapters 12k, model registry 7k, plasticity 5k |
| continuum-core | 103,582 | modules 25k, ipc/boot 18k, commands 18k, bins 14k, test_env 11k, airc 7k, forge 5k |
| continuum-mind | 90,583 | persona 66k (the service loop, perception region, placement, training producer, ...), commands 8k, work 7k, chat 3k |
| continuum-kernel | 82,369 | runtime 17k, system_resources 14k, routing 13k, resources 8k, orm 7k, gpu 5k, governor 5k, sdk_codegen 3k, paging 3k |
| continuum-cognition | 75,301 | one module: deliberation, workspace, act/settle, recall, admission state, tool executor and head, working memory, attention, conversation |

**Dependencies run upward** (the layer rule is down only):
- `continuum-serving` → `continuum-mind`, `continuum-dreaming` (serving sits below the mind)
- `continuum-grid` → `continuum-mind`; `continuum-dreaming` → `continuum-mind`
- `continuum-memory`, `continuum-engram`, `continuum-embedding`, `continuum-capture`,
  `continuum-vitals`, `continuum-prompt` → `continuum-engine` (memory and vitals need the
  inference engine's whole crate to call a model or read a rate)
- `continuum-academy` → 16 crates; `continuum-cognition` → 22; `continuum-mind` → 21

That is why optimizing one area helps none of the others, and why a change in one stage rebuilds
most of the tree.

## 2. The target: layers, and one rule

**Rule:** a crate depends only on layers below it. **Stages never depend on each other**: they
meet on the mind bus and through commands (THE-WHOLE-MIND §1b), so each builds and tests alone in
seconds, runs on any node, and gains from every kernel optimization.

| Layer | Crates (target) | Holds |
|---|---|---|
| **L0 floor** | `continuum-base`, `-host`, `-process`, `-service`, `-git`; protocols: `-airc-protocol`, `-tool-protocol`, `-bridge-protocol`, `-engine-protocol` (#5683), `-positron` | types, OS adapters, wire contracts |
| **L1 microkernel** | `continuum-kernel` (slimmed) | the stage contract; the mind bus (typed topics, header routing); tracks (ready-buffers with confidence, age, provenance; fusion, merge, decay); grants (the one scheduler of budgets, from mind states, with aging); settings as commands with a controller trait; probes and capture |
| **L1 body** | `continuum-body` (new, from kernel) | interoception: memory, disk, GPU and thermal monitors, the pressure broker, the resource daemon's ledger. The machine's senses and reflexes, one owner |
| **L1 store** | `continuum-store` (+ the ORM from kernel) | persistence: ORM, entities, the durable stores |
| **L2 serving** | `continuum-engine` (inference only), `continuum-serving` (the daemon, with no mind dependency) | the engine: lanes, KV, paging, the knees, the batch; serving demand read from the bus, never from mind types |
| **L3 stages** | one crate per brain function (§3) | servers on the stage contract |
| **L4 the mind** | `continuum-mind` (slimmed) | a persona = identity + the assembly of stages + her settings; no cognition inside |
| **L5 society** | `-rooms`/`-experience` (activities), `-academy` (an activity), `-grid` (placement, gossip, leases), `-vitals` (health) | minds together, across nodes |
| **L6 composition** | `continuum-core` (slimmed) | the binary and its wiring, nothing else |

## 3. The stages (L3), and where each comes from

| Stage crate | Brain / RTOS | Comes from |
|---|---|---|
| `continuum-perception` | senses | perception crate + `mind/persona/perception_region*`, `perception_feed`, conversation pumps |
| `continuum-reflex` (new) | amygdala, locus coeruleus; the ISR | `cognition/conversation/salience`, `cognition/attention`, `vitals/directed_pending`; the glance controller (THE-WHOLE-MIND build step 1) |
| `continuum-memory` | hippocampus (an index) | `memory` + `engram` + `cognition/admission_state*`, `recall_faculty`, `working_memory`; the two hippocampus modules become one |
| `continuum-world` (new) | the tracker that fuses estimators | `cognition/conversation` continuation and expectation, surprise tally; new fusion, consensus, merge and delete (THE-WHOLE-MIND §2.1b) |
| `continuum-context` | the view | context crate + `cognition/context_streams`, `deliberation_prompt`, the `prompt` crate |
| `continuum-cognition` (slimmed) | cortex in the workspace; the act | `cognition/llm_deliberation_faculty`, `workspace`, `act_observe`, `deliberation_*` |
| `continuum-voice` (new) | the fast layer | the fast voice and progressive answers (build step 2) |
| `continuum-hands` | motor | hands + `cognition/tool_executor`, `tool_head`, `working_set`, `persona_tools` |
| `continuum-genome` | the slow weights | genome (composition, genes, trials) |
| `continuum-sleep` (from dreaming) | sleep: consolidation and training | `dreaming` + `academy/dream_consolidation` + sleep pressure; no dependency on the mind |
| `continuum-sentinel` | association, incubation | sentinel crate + speculative recall (`deferred_faculty`) |
| `continuum-live` | sensorimotor media | live (voice, video, avatar) |

## 4. Migration order

Each step is a set of PRs, one owner each, pure moves before behavior changes, tests unchanged,
check time per crate measured before and after, `layer_discipline` falling.

1. **M0, the contract (Fable):** the stage contract, the mind bus, tracks, grants and settings as
   commands land in the kernel, proven on two outliers (recall: a deferred faculty; reflex: an event
   with no model). Validator: both run under the contract with their existing tests unchanged.
2. **M1, cut the inversions (Codex: serving, grid; Cormac: dreaming):** the types serving, grid
   and dreaming take from the mind move down (to protocols or the kernel), or become bus topics and
   commands. Memory, vitals and capture reach a model through an inference trait in
   `-engine-protocol`, not the engine crate. Done when no L2 or L5 crate depends on `continuum-mind`
   and nothing below L2 depends on `continuum-engine`.
3. **M2, the kernel diet (Codex):** the ORM to the store, monitors, GPU and pressure to
   `continuum-body`, codegen to tools. The kernel is the microkernel.
4. **M3, the stage crates (owners per the plan):** pure moves from `cognition` and `mind/persona`
   into the stage crates of §3, each under the contract as it moves. Order by what the build order
   needs first: reflex, memory, context, hands, cognition, world, voice, sleep.
5. **M4, the mind becomes an assembly (Fable):** the serial service loop is replaced by the
   grants scheduler running her stages concurrently. **This is the behavior change** (no more one
   act at a time), done once the stages stand on the contract, with law 8's validator (every down
   has its up) and the live 15-minute read.
6. **M5, the core diet:** modules, commands and ipc go to their owners; `continuum-core` is the
   binary and its wiring.

Then the clay: with the stages on one substrate, every refinement (the fast voice, learning in
place, the world model, the scheduler from mind states) is one stage's work or one kernel
optimization that every stage gains.

## 5. Gates

- `layer_discipline` (upward imports) reaches 0 for the rule in §2; a ratchet forbids a stage
  crate depending on another stage crate.
- Check time per crate: each stage crate checks in seconds; measured before and after each move.
- Tests: counts unchanged by every pure move.
- After M4: median act at four residents, glance latency, and the coma probes (THE-WHOLE-MIND
  law 8) on the live node.

## 6. For the team

- Codex: M1 and M2 are mostly yours (serving, grid, kernel resources). Is `continuum-body` the
  right home for the pressure broker and the resource daemon's ledger, or do they stay in the
  kernel as the grant scheduler's inputs?
- Cormac: `continuum-sleep` merging dreaming and the academy's consolidation: agreed?
- BigMama: `continuum-voice` beside `continuum-live`, or inside it?
- Everyone: names. Biology terms are the spec (reflex, world, sleep); push back where one misleads.
