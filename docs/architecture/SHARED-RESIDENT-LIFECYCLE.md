# Shared resident lifecycle: implementation plan

Status: proposed architecture and acceptance gates, 2026-09-28, Codex. Not a runtime capability claim.

Extends [One Resident Model](ONE-RESIDENT-MODEL-PATIENT-DOCTOR-DREAM.md), [CBAR substrate](CBAR-SUBSTRATE-ARCHITECTURE.md) and [concurrency style](CONCURRENCY-STYLE-GUIDE.md). Preserve the normal persona/activity/room runtime for work, coursework, benchmarks and simulations. Owner review is required before treating this proposed contract as settled. It introduces no parallel runner or supervisor.

## Failure that defines the contract

Attempt 2 4169e970-ad49-42fe-9521-d6ff280dc88a started on engine 25280, core 4980. At 12:29:07 the serving footprint included transient training allocations; the planner reported usable_gb=0, chose a 1.5B replacement, and committed that swap at 12:29:22. Job failed at 12:30:36 when the replacement returned no matching training out. The 27B returned in engine 30988; core 4980 never restarted. Filtered diagnostic receipt is archived on the Windows node in team-proof-20260921/KIMI-ATTEMPT2-LANE-FAILURE.log; the causal timeline above is included here so review does not depend on access to that local file.

Current seams: LifecycleGate serializes admission against serving operations; EngineRun retains a capacity LeaseGuard but no engine-lifetime ownership. ServingSteadyHold only delays optional geometry changes and can be overridden; model replacement bypasses it. Therefore neither a larger timeout nor another startup script closes this contract.

## One authority, three related records

Extend the existing serving lifecycle authority and resource/job records. Do not create another manager, daemon, polling loop or source of truth.

1. Resident identity: node, base revision/quantization, engine incarnation, endpoint and verified executable/build. Endpoint or PID alone is insufficient because both can be reused.
2. Active work: existing job/session or activity identity, owning resident incarnation, desired state, observed state, capacity reservation, checkpoint reference and recovery disposition. Reuse `training_hold_store` (`state/training-holds.json`, #4522) for desired training holds, including its fail-closed read semantics; do not create a second hold store. Serving, training and evaluation declare their dependencies through the same ownership contract.
3. Transition: operation identity, expected resident generation, requested target and reason, preparation/drain/checkpoint acknowledgments, committed result. Retrying the same operation returns its disposition rather than repeating side effects.

The planner proposes changes. Only the lifecycle authority commits a resident replacement. Admission atomically establishes capacity plus resident ownership before work starts; every destructive transition checks that ownership at commit, not only when its plan was computed. No mutex is held across network or process waits: use the existing gate, generation checks, staged state and event/snapshot publication.

The engine observes its actual steps, allocation and pause state. The core owns intent and recovery. The OS supervisor owns core process liveness and the minimal verified bootstrap path. Install and deploy request transitions; their success means observed convergence, not subprocess exit 0.

Existing integration points: `serving_daemon::LifecycleGate` and `AdmissionHold`, `forge::training_admission::wait_for_training_memory`, `ResourceDaemon`, and `OwnedEngineIdentity`. The current owned identity is an in-process weak owner plus an EngineGeneration UUID; it is NOT a durable PID-plus-start-time identity. Add a restart-surviving incarnation to the existing lane record, using PID and OS process start time with the recorded engine/build identity. Adoption must verify that incarnation rather than treating a newly minted core generation as proof of the same process.

Reconcile model swaps, geometry changes, page-in, empty-plan retirement (`idle_if_current`), broker memory-pressure relief, footprint sampling and emergency paging recovery must consume this authority. In `wait_for_training_memory`, establish the residency record inside the existing admission hold, after capacity admission and before dropping that hold or sending POST /train. The exact record schema is an implementation decision; these identities and ordering guarantees are mandatory.

## Population, agency and activation

Durable persona identity, hosted activation, attention, activity membership, warm KV state and a compute permit are distinct. Creating 100 personas must not eagerly create 100 model processes, warm slots or concurrent turns. At equal active workload, test that increasing the dormant population does not multiply inference work or prevent startup. Metadata storage and discovery may scale with population; this is not a claim of zero cost.

Activation can come from a directed room event, an assigned activity or a persona's scheduled self-directed initiative. Resource admission bounds execution without erasing that initiative: retain pending intent and explain deferred work. A nursery is an ordinary activity/recipe that creates a durable identity and can request activation; creation itself does not confer a permanent compute reservation. Coursework, simulation, evaluation and dreaming use the same room/activity state and scheduling primitives as ordinary life.

Placement must consider measured throughput, prefix reuse and the latency budget as well as memory. Grid capacity participates in the same society. Distinct identities can share a resident base while retaining their own memory, relationships and stable adapter selection. Provider registry availability is not a prerequisite for an installed persona to keep working.

The current evidence does not establish the 100-persona gate: the M5 owner reports 16 canonical personas, 6 hosted and 755 peer-state directories, most of which are fork residue rather than citizens. The IntelMac owner reports 8 canonical personas and severe uncached-prefill latency. Neither directory counts nor memory-fit lane counts prove useful activity. Keep these owner-reported baselines separate from future acceptance measurements.

## Invariants

- Ordinary model, geometry, backend and adapter reconciliation cannot destroy an active resident dependency. A genome trial changes adapter selection through the existing engine path without replacing the base.
- Capacity is reserved once and physical allocation is counted once. A reservation is not additional measured allocation. Unknown measurements remain unknown.
- In-place pause retains residency and memory; disk suspension releases them only after an acknowledged durable checkpoint and teardown.
- Dropping a controller future or losing HTTP contact does not prove the engine stopped. Preserve ownership as uncertain until terminal/cancel acknowledgment or verified incarnation death. A stale controller cannot release a successor's ownership.
- On core startup, reconcile existing engine identity, durable work and observed state before allowing replacement decisions or new learning dispatch. Reattach to the same job without POSTing another run. Ambiguity blocks destructive transitions while existing healthy serving continues.
- Genuine engine death or emergency pressure can trigger recovery. For an intentional emergency replacement, persist and publish the job interruption reason before committing replacement; do not reconstruct it only after the engine disappears. Do not report successful resume without evidence. If the engine is already dead, an acknowledgment from it is impossible and verified incarnation death releases its ownership even without an ACK.
- Human room state and persona perception use the same published state. A log parser is diagnostic, never the operational workflow.

## Memory accounting

ResourceDaemon remains the budget authority. Separate shared base, serving KV/workspace, training context/workspace and adapter/session allocations in its existing reports. Shared weights belong to the resident once; multiple minds do not multiply that allocation. Reservations and observed allocations must be correlated by owner so they are not summed twice.

Until engine attribution is complete, withhold contaminated serving-footprint calibration while resident training may be allocating; retain the last clean measurement and mark it stale. Do not subtract an estimated reservation from a physical sample and call the remainder measured serving usage. On terminal recovery, invalidate contaminated records through the existing footprint store and require a fresh clean sample. Account for asynchronous frees and device measurement lag.

Pause/yield is a compute scheduling change, not memory relief. Under pressure, use existing paging and admission policy; checkpoint or explicitly interrupt work if residency genuinely must change. Never silently downgrade the base hosting an optimizer.

## Pause, restart and recovery semantics

Requested pause -> engine boundary acknowledgment -> PausedInMemory. Resume -> same incarnation/session -> observed next step. A lease TTL is an ownership-renewal deadline, not proof of engine termination.

Requested suspend/deploy -> checkpoint prepared -> checkpoint durable -> resident release -> verified replacement -> restore -> observed resumed step. A checkpoint atomically binds shadow weights, optimizer/accumulator, RNG and arrival/replay cursors to base/quantization/adapter geometry/version. A candidate GGUF is not that checkpoint.

Before durable suspension exists, planned destructive transitions defer during learning or perform an explicit authorized cancellation with a recorded interruption. Do not advertise restart-resume. An incompatible checkpoint remains retained; explicit migration or a new session from stable weights reports lost uncheckpointed progress and preserves pending experience for reconciliation.

Core-only restart can reattach to a surviving engine. Engine restart requires a checkpoint. These are distinct paths with distinct acceptance receipts.

For an alive but unreachable engine, expose an explicit recovery act through the existing job-cancel/lifecycle path. Persist authorized interruption intent against the exact job and incarnation, then reconcile cancellation or perform the explicitly authorized fenced teardown. A cancel request alone is not proof of termination and must not release residency. Verify termination before replacement; report failure to obtain teardown authority instead of leaving an unexplained permanent wait or pretending cancellation succeeded. The ordinary scheduler may not infer this destructive authorization from a timeout.

## Delivery sequence and ownership

1. Ownership contract through existing lifecycle gate and work records. Cover model/geometry replacement, cancellation, controller loss and terminal release together. Cormac owns the serving/training implementation per AIRC; Fable and Codex review. A training-only in-memory boolean is not sufficient acceptance.
2. Correct resource attribution and contamination recovery. Same implementation owner coordinates resource seams; reuse ResourceDaemon, lease guards and footprint registry. Test the actual attempt 2 sequence, including the transient allocation and restored clean sample.
3. Startup reconciliation and core reattachment. Fable owns existing card 7bb4e5a2; compose with step 1, not another registry. Reconstruct ownership and desired holds before scheduler mutation. If steps 1–2 ship first, learning dispatch stays gated until recovery is proven or an explicit supported limitation prevents unsafe transitions.
4. End-to-end resident integration proof on the 5090, then M5 with its owner. Verify actual installed engine SHA, clean capacity, single run identity, completed optimizer update, interleaved normal room turn, pause/resume and no replacement across planner ticks. Run fault cases first through existing test facilities, not by killing the live persona. No duplicate build/install/run.
5. Full durable checkpoint and install/deploy convergence. Same transition protocol for manual restart, scheduled deploy, pause/suspend and boot. Retire duplicate decisions from wrappers as each caller migrates. Windows tasks remain bootstrap adapters.
6. Learning proof through existing curriculum and GeneTrials: artifact -> genuinely held-out improvement -> safe adoption -> fresh work gain and retained prior skills. Partition experience/task families before deriving examples. The four-example split=0 run is integration evidence only.

Steps 1–3 establish the architecture required before retrying Kimi safely. Full optimizer disk recovery can follow, with deferred destructive transitions honestly enforced until then. Each PR targets canary after owner review and appropriate CI; sync and actual runtime adoption are separate receipts.

## Acceptance matrix

- Admission races with replacement: one wins; the loser replans against the new generation. Never train on a disappearing engine.
- Training allocation rises: same base/incarnation/job persists, serving calibration stays clean, no double charge.
- Pause acknowledged: normal serving proceeds; memory remains reserved; resume advances the same job.
- Controller future drops or request times out: no duplicate run and no premature ownership release.
- Core restarts with engine alive: same job is reattached before reconciliation, holds restored, terminal event once.
- Engine dies: explicit interruption, no false success; restore only from a compatible durable checkpoint.
- Install repeats or partially fails: one transition disposition, verified running build and work recovery, no duplicate daemon.
- Trial candidate fails or regresses: stable genome remains active; training completion alone does not promote.

Use existing test modules, fixtures, resource snapshots and command/event receipts. Latency and throughput numbers in the one-resident architecture remain gates to measure on the 5090 and M5, not outcomes established by these tests.
