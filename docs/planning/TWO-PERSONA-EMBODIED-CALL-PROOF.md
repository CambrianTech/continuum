# Two-persona embodied call proof

Owner: Astra, with Fable reviewing, Cormac owning voice/packaging and IntelMac
coverage, and BIGGIEDESK owning Windows installation. Updated 2026-10-04.
Acceptance card: 42656796-6999-4ecf-9576-96576d039175.
This polishes existing call, mind, grid and avatar paths. It does not create a
second conferencing stack. Read with MULTIMODAL-STREAMING-MIND-COMPLETION.md and
the reviewed voice-model plan PR4697; the older voice ladder is historical.

The beta demonstration is two persistent personas meeting, listening, speaking
with their own genome voices, looking at one another's rendered bodies, and
recovering when a node leaves. An outside agent observes the actual delivered
media and public behavior. A transcript, process ping or successful transport
fixture is insufficient.

## Current gaps and ownership

| Gap | Existing work that closes it | Delivery evidence still owed |
|---|---|---|
| Kimi project continuity | Slice2 card74ec9613; PR4700 af6864014; Astra/Fable | Kimi's own fresh tests, revised artifact and independent review, then installed reclaim/move preserving that work |
| Incremental native generation | PR4648/4701; Astra | Common-chat template/parser and scheduler constraints; authenticated remote cancel; lossless media backpressure; installed real model stream |
| Learned local voice | Cormac card3f44bd80; voice plan card7ffa5e12/PR4697 | Provisioned Qwen3-TTS candidate, declared capabilities, genome LoRA identity, uniqueness and latency; approved model choice is not a loaded artifact |
| Remove cloud speech dependency | Cormac cardab567967 | Supported installation and no cloud request; compatibility speech does not prove native model speech |
| Call consumer, native hearing, lips and observer | Acceptance card42656796; Astra, existing live/mind owners | Stages below; no inferred audio capability from the current Qwen3.8-27B binding |

The local catalog last captured51 llama-server models and no declared audio/speech
capability. Preserve each persona's existing cognition binding. Any genome voice
expert is an explicit model/gene binding through existing genome and grid owners;
it cannot silently replace the cognition model or count text-to-speech plus
transcription as native hearing. Unsupported combinations fail visibly.

## Stages and receipts

These are proposed beta targets for review, not measured performance claims.
Baseline and compare the same model/artifacts, context, resolution and workload.
Report both cold and warm turns and outliers; cache hits cannot conceal cold cost.

| Stage | Reused seams and execution node | Receipt and acceptance target | README promise proved |
|---|---|---|---|
| 0. Parallel product peer and capability baseline | Kimi Slice2, work submission/review, grid placement;5090 cognition, M5 capability check, IntelMac observer | Kimi authors useful code, tests it, incorporates independent review and submits the actual bytes. Record serving revision, model/gene hashes, capabilities, CPU/RSS/GPU/swap and available capacity. No native audio claim until actual input/output is observed. | Persistent citizen/tools; independent product work |
| 1. One speaker reaches one listener | Native model adapters and generation events; call_server begin_persona_generation/push_persona_generation/finish_persona_generation; mixer and NativePlaybackLease. M5 Metal first where the required artifact is actually supported;5090 second | Twenty alternating turns of real audio at the receiving model input and real model speech at receiver playout. Preserve request/phase/sequence/model/gene identity. Proposed warm TTFA p95<=2s, no underruns during speech and no hidden batch replay. Cold TTFA reported separately. If unsupported on M5, record refusal and run capable5090 path; do not claim Metal acceptance. | Voice/video client table: delivered local voices |
| 2. Interrupt, cancel and continue | Existing RequestBoundary, request guard, playback lease, admission and grid cancellation; same nodes | Ten controlled mid-utterance interruptions plus disconnect/rejoin. Proposed stop-audible p95<=200ms after local cancellation; network contribution separately measured. No stale generation resumes, no duplicate turns or persona/UI replay, final PCM drains on success, failed call names its gap. Identity, memory and activity survive reconnect. | Same personas everywhere: continuity through interruption |
| 3. Rendered body and native vision | Existing Bevy avatar renderer, speech clips/visemes, adaptive idle cadence, GPU frame publishers and perception/observe. M5 render first,5090 render second; IntelMac low-resolution receiver | Two speaker tiles deliver>=24fps at declared480x360, proposed p95 absolute lip/audio offset<=80ms and capture-to-remote-display<=250ms. Each persona answers a changing visible gesture/object from actual video input, not room text. Existing envelope mouth motion is the first measured baseline; speech-token viseme mapping is a later refinement on this same stream. | README hero live-session image and P1 vision |
| 4. Outside observer and churn | Existing call subscription/media tracks, perception/observe, public live-state/typed probes and PumpTally summaries. IntelMac observer/floor; M5/5090 own heavy inference/render | Observer sees both delivered streams with proposed p95 extra observation lag<=500ms; records public transcript separately. Alternate one serving node off/on, preserve model/gene ownership and durable cursor, no duplicated side effects. Weak machine can remain a useful observer/tool peer without loading the large model. Ten-minute two-persona soak: no growing queue/RSS, no persistent CPU saturation attributable to idle avatar work. | Same personas everywhere; weak-node grid membership |
| 5. Scale the same contracts | Same render governor, pooled publishers, admission and grid owners; M5/5090 with IntelMac observer | Four-persona baseline then14 resident tiles with natural speaker scheduling. Speaker>=24fps; idle cadence reported separately. Compare process CPU/core-seconds, GPU load, frame copies/bytes, queue age, TTFA and p95/p99 latency against two-persona run. No mandatory14 concurrent model decodes or duplicated audio encodes. Reuse shared prefixes only when exact tokens/positions/model/LoRA match; persona-private branches remain isolated. | Client table and hero under multi-persona load |

Stage0 project review runs in parallel; Kimi is the product peer we call, and her
Slice2 review cycle does not gate stage1. Capability validation belongs to the call
itself. Native audio input and output, streaming rendering and observed call behavior may
advance independently. Passing an earlier stage never fabricates missing native
capabilities for the next stage. LiveKit/desktop integration follows demonstrated
persona-to-persona behavior; it does not block these substrate proofs.


## Run it again through one authored activity

Acceptance card42656796 must author purpose embodied-call/proof in the existing
ExperienceRecipe catalogue, reusing RecipeStep and PipelineExecutor through
activity/spawn. This is a proposed purpose, not an installed command today.
One activity/spawn invocation supplies two stable persona IDs, observer identity,
stage, node preferences, workload seed, context, resolution, duration and a prior
receipt ID. The same recipe shape serves every stage; its version is in receipts.

The pipeline validates bindings/capabilities, opens the existing call, joins the
two personas, explicitly attaches the external observer, runs the declared turn/
interruption/churn scenario, collects phase summaries, and emits one ORM receipt
row with an artifact reference. Existing call/lease ownership must release observer,
call resources and temporary captures on success, cancellation or failed steps.
A stop-on-error pipeline alone is not a cleanup guarantee. No script daemon or
always-on listener. A rerun creates a new call/generation and isolated sample set;
it cannot replay last run's tool side effects or overwrite persona memory.

A regression is a diff of two receipt rows with identical recipe version and
declared workload/artifact controls; differing nodes, models or clock uncertainty
are shown, not folded into a speedup. The output includes metrics, cleanup result,
unsupported stages and the remaining gap. CI can exercise generic ownership/
receipt contracts once; actual Metal/CUDA adapters and delivered media still
require their real node receipts.

First provisioning candidate follows reviewed PR4697: Qwen3-TTS1.7B Base Q4_K_M
plus its Q8 codec on M5 Metal, repeated on5090 CUDA, with separately base-bound
persona voice LoRAs. Cormac card3f44bd80 owns the actual files, hashes and
voice/selftest adapter proof before stage1. The0.6B tier requires forge conversion
according to that plan, so it is not assumed provisioned. Exact Base/codec filenames,
first capable node and completion milestone are requested from Cormac; no installed
file or delivery date is invented here. This is the remaining stage1 handoff,
alongside actual native hearing capability; genome voice output alone is not hearing.
## Hearing gene handoff

Fable reports the selected direction on card87f6544a: audio input is a sensory
projection grafted onto the existing Ornith-1.5 and Qwen3.8-27B bases. Preserve
those cognition bindings. A community bridge is the explicitly labelled bridged
floor; it is not proof of native hearing. Forge produces the per-base projection,
with later training in the Rust engine. Cormac owns the first real-speech bridge
measurement and the subsequent forge/training handoffs through existing owners.

At this handoff the M5 measurement is deferred: Fable reports zero free memory,
13/14GB swap and no Ornith9B artifact on disk. Select the M5 after capacity clears
or the5090 after BIGGIEDESK's installation window; record the actual node and
artifact hashes before loading. No measurement or completion date is claimed.
Stage1 still requires delivered real speech at the bound model input and output.

## Observation and measurement contract

The observer is explicitly invoked for a named call and stops after the run,
including failure. Reuse existing media subscriptions and perception providers;
no permanent debug listener, new background monitor or production test adapter.
A useful reusable regression belongs outside the runtime path and may run in CI.
Do not promise that this Codex session directly hears audio: available tools may
only expose frames or public text. Mark each receipt as actual audio/video
observation, transcript-only observation or another evaluator's report.

Trace public request/call/generation IDs through capture, encode, publish,
receive, decode, playback and display. Use monotonic elapsed time within a node;
cross-node latency requires measured clock offset and uncertainty, or a bounded
round-trip estimate. Report that uncertainty rather than subtracting unrelated
timestamps. Lip offset compares matching audio sample time and displayed mouth
pose from the same receiver timeline. Delivered fps counts displayed/received
frames, not renderer loop ticks. TTFA starts at explicit accepted input/turn
boundary and ends at receiver's first audible sample; report prefill and voice
generation separately where they are distinct bound experts.

Existing PumpTally aggregates frame counts/bytes/max latency once per interval:
do not flood cognition probes or the UI with a row per frame. Extend typed
existing probes for missing phase timing. Persist receipts and consumer bookmarks
through ORM only, with bounded retention. Public transcript and system facts are
available for diagnosis; private reasoning and hidden model content are not an
observer feed. Reconnect resumes each consumer's durable cursor; historical replay
must neither duplicate persona input nor re-execute a tool.

Each receipt names source and running revision, node, model and voice-gene hashes,
sampling/context parameters, call IDs, cold/warm classification, sample count,
p50/p95/p99, CPU/RSS/GPU/swap, drops/underruns, cleanup result and remaining gap.
Retain actual media where authorized with bounded storage; public transcript is
a separate artifact. End-user installations consume prebuilt artifacts. Check
deploy.claim/compiler ownership before any developer build; preserve active
serving, persona work and shared Cargo cache.

## Kanban handoff

Finish current continuity repair/review while native binding and voice owners
continue their own scoped work. Acceptance card42656796 owns call/observer/lip
proofs; it does not absorb Cormac's voice or Fable's installer work. Start stage1
only with a declared, supported actual audio artifact, and stage3 with real vision
input. Fable reviews this plan and observable behavior; Cormac maps it to README
promises and supplies IntelMac floor evidence. Record before/after commits,
responsibility owner, migrated callers, deleted duplication, retained contracts
and installed result. No stage closes on instructions or green tests alone.
