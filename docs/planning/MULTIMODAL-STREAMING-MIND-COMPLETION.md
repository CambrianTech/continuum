# Multimodal streaming mind — completion plan

Owner: Codex. Updated 2026-10-02. This is the active delivery plan, not a new
architecture. Reuse the live WorkspaceCycle, CBAR stages, bus, admission,
scheduling, model bindings, genome, AIRC and existing avatar/live session code.
BIGGIEDESK installation belongs to the other Codex and is not this lane's gate.

Active execution receipt (09:14 UTC): supported install45808 EXIT0; core and both
CLI aliases verified7d432d161, read-only install check converged. Shutdown saved
all65 modules; core returned~29s with existing llama lane58057 ready. Exact-revision
PDF acceptance58182 and merged-source stream tests33479 are running; resume those
handles before duplicate work. Desktop native-path repairda876561d remains queued
for deployment. Native image output and real audio binding/voice remain OPEN.
PDF understanding previously passed on recoveredbe79804c7 in84.8s. Historical
handles below are superseded by this receipt and the external team-proof README.

## The outcome

Kimi can see and hear Joel continuously, speak in her bound model's native voice,
notice a newly shown object while speaking, yield to interruption, and continue
the same conversation and development activity. Good anime/3D avatars are enough
for this delivery. Photorealistic rendering is a separate later upgrade.

Two acceptance levels must stay distinct:

1. The substrate preserves native image/audio input and output, capability
   checks, media timing, cancellation and identity through supported adapters.
2. A real bound model and deployed consumers demonstrate the conversation.
   A synthetic transport fixture or an advertised capability cannot pass level 2.

## Historical baseline — evidence, not inferred completion

- PDF -> native image -> ai/generate passed twice on installed build84c18e611.
  Empty text layer, correct visual facts. Wall times 165/110 seconds: functional
  page understanding only; conversational latency is not achieved.
- Owned branch codex/native-model-binding, starting HEAD17c2096fa, has preserved
  uncommitted streaming changes. Incremental PCM, bounded rings, explicit loss,
  cancellation and remote text framing have targeted test receipts in README.
- Native image output is explicitly unimplemented in the current wire helper.
  Actual native audio input/output on a selected serving binding is unproved.
- persona/service_loop.rs presentation path explicitly cancels Media chunks.
  An audio cursor producing bytes therefore does not mean Kimi can speak.
- Remote terminal/reply ordering has code but lacks deterministic acceptance of
  both arrival orders. Binary media negotiation/delivery remains unfinished.
- Test session11883 owns current stream_sinks validation; resume it, do not build
  again. Log: native-media-drain-0042.log in team-proof-20260921.

## Delivery order and finish criteria

## Shared boundary design: AIRC is the event bus

Keep control, live media and durable outcomes distinct within the existing AIRC
contracts. Correlated StreamChunk events carry incremental output; the durable
reply settles the operation only after the stream's terminal marker. Do not make
the reply the mechanism for delivering audio or video. Existing ACL/identity and
request routing remain in force for remote inference.

Native media uses a binary body with explicit MIME, media sequence and presentation
timestamp, inside the existing stream ID/transport sequence/sender/room envelope.
Versioned consumer opt-in is a wire contract, not permission to infer model
capabilities. Local consumers share Arc-backed chunks; serialization occurs at
the peer boundary. Reject oversized or malformed media before forwarding it.

Backpressure is semantic: an independently decodable camera snapshot can replace
a stale snapshot under the existing latest-state mechanism. Ordered audio and
dependent encoded video cannot silently skip bytes; gaps cause an explicit stream
failure/reset. Interruption/control events must remain schedulable under media
load. Old speech needs a cancellation/generation identity so delayed remote output
cannot play after interruption. Use existing lifecycle/session identities before
adding fields or a parallel registry.

Layered cognition consumes the same causally attributed perception state. Fast
attention and deep work are scheduled roles within the existing mind; their model
bindings are explicit. New input can supersede a pending response while preserving
the task and its committed side effects. True in-generation input requires an
engine transport that supports it; otherwise report the precise supported
interruption/resume behavior rather than claiming duplex inference.

Rendering subscribes downstream to native speech and available expression/body
signals. It does not own conversation state or replace model speech with TTS.

### Existing causal pointers and time histories

Joel's CBAR clarification is authoritative: reference shared time-indexed media,
features and sensor history instead of copying snapshots into each thought. Reuse
Engram.context_id for scope and EngramGraph CausedBy/Produced edges at the site
where the cause is known (docs/cognition/CAUSAL-MEMORY-GRAPH.md;
cognition/act_observe/apply.rs already links admitted act receipts to chain.prior()).
Do not infer causal links from semantic similarity or create a parallel graph.

Current MediaChunk.presentation_time_us is relative media time, not an engram ID,
wall clock or cross-node synchronization proof. The outer stream correlation is
an operation join, not by itself a sensory-history reference. Consumer integration
must carry the existing activity/trigger references through that operation and
resolve delayed results against their originating perception, not current state.
Preserve clock domain and synchronization uncertainty at cross-node boundaries.
Interpolate continuous pose/sensor samples only across valid bracketed history;
never interpolate discrete speech, thought or action causality. Regression gate:
late output after a perception change still resolves to its original source and
cannot be presented as a response to the newer event.

## Delivery milestones

### 1. Finish the shared stream change already in progress

Work in ai/adapter.rs, ai/stream_sinks.rs, inference/sse_stream.rs,
inference/native_output.rs, inference/airc_remote and routing/command_handler.rs.
Complete deterministic terminal-before-reply and reply-before-terminal coverage
in the existing fixture. Verify partial media arrives before completion, dropped
consumers cancel, stalls release admission, overflow/gaps fail, and errors never
become successful partial outputs. Text drains must explicitly refuse media.

Finish: focused tests pass on final source; commit current coherent changes and
rewrite PR4648 around streaming, not its obsolete whole-response implementation.
Install through the resolved supported installer and verify runtime SHA and one
real consumer. Passing source tests alone does not close adoption.

### 2. Establish the native model binding and four media directions

Use existing Model/ModelInfo once per binding, shared by reference/Arc with
provider adapters. Record actual engine transport support alongside declared
capabilities. Exercise native image input, audio input, image output and audio
output independently. A model may support a subset; unsupported directions fail
explicitly, without automatic model switching, captions, transcription or TTS.
Select and validate an explicitly supported binding for each acceptance case;
do not guess support from model names or provider-wide flags.

Finish: real input/output artifacts, model/engine identities, hashes and transport
receipts for all four directions across explicitly configured supported bindings.
For conversation, identify a native audiovisual streaming binding and its duplex
limits. If the engine cannot receive input during output, expose that limitation;
do not label cancellation/restart as full-duplex model inference.

### 3. Wire native output into the existing live consumer

Replace the explicit unsupported Media branch in the persona presentation path
only when the live session can consume it. Reuse live/session/orchestrator.rs,
live/transport/call_server.rs, audio buffer/mixer and AvatarRenderer controls.
modules/live_session_consumer.rs governs resource residency, not playback.
Keep media sequence, presentation time, persona/model/genome binding and session
correlation through playback. Drive the existing lip/expression/body controls from
available native signals. Define explicit behavior for missing gesture signals;
do not fabricate a demonstrated model capability.

Finish: hear actual bound-model audio before generation completes; verify byte
provenance, bounded playback buffering, synchronized avatar, interruption flushing
old audio, and correct completion/error delivery. No stock voice substitution.
Source inspection found that mixer.rs currently warns and drops excess AI samples
on ring overflow. Native streaming must surface that loss as an explicit outcome
or apply bounded producer flow control; that legacy behavior cannot pass this gate.
The current native PCM wire is 24kHz, mono s16le; the call mixer is 16kHz. The
consumer must explicitly negotiate a supported format or use a declared stateful
streaming sample-rate adapter preserving media time. Reinitializing a batch
resampler for each packet loses continuity. Audio samples must retain bound-model
provenance; this format boundary never authorizes voice synthesis or substitution.

### 4. Feed ongoing perception into the event-based mind

Reuse admission -> WorkspaceCycle -> faculties -> act/observe and existing
amygdala/JEV/CBAR attention mechanisms. Inspect their actual live wiring before
editing. Camera/screen/PDF and audio events carry timestamps, provenance and
correlation into the same activity. Long work returns handles and emits progress,
interruption and completion. No new polling mind or second conversation loop.

Finish: while Kimi is speaking, show a different object and interrupt her; the
meaningful response reflects both events. Then resume the original task with its
workspace, pending action receipts and conversation intact. No canned response
counts as useful responsiveness; no lost or duplicated actions.

### 5. Validate scaling and layering under real work

Reuse existing admission, scheduling, placement, genome paging and AIRC routing.
Keep urgent perception/conversation schedulable while deeper coding or learning
continues. Bind each participating model explicitly; a change in placement or
capability cannot silently replace Kimi's voice or modality. Use existing grid
nodes when ready, without making peer installer work our prerequisite.

Finish: repeat the same conversation acceptance while a real long operation is
active; demonstrate timely attention, eventual background completion, persistent
identity and explicit remote failure/recovery. Native media must survive the
actual peer hop with sequence and terminal receipts, not just gist coordination.

## One repeatable live acceptance session

1. Start from the existing Kimi activity and working avatar. Record binding,
   runtime SHA, activity/session IDs and active background job.
2. Present a graphics-only PDF or new object and ask a question whose answer
   depends on its pixels. Speak naturally; retain native audio input evidence.
3. Change what is visible while she speaks. Interrupt and correct her. Verify
   perception changes her response and obsolete queued speech stops.
4. Pause, continue, and return to the original work. Verify context and action
   continuity, then repeat during background work and through an eligible node.
5. Save correlated event times, media hashes, first useful response, first audio,
   interruption-to-silence, queue wait, prefill, output rate, A/V skew, buffer high
   water mark and eventual job completion. Keep private reasoning out of receipts.

Initial engineering targets for the local conversation are useful response/audio
within 1 second p95 and interruption-to-silence within 250 ms p95. These are
proposed targets, not current measurements or promises. Report misses and their
measured causes; do not weaken the test to declare success. Capture at least 20
interaction events for an initial latency distribution; do not infer p95 from a
single demo. Keep throughput/remote results separate from local results.

## Delivery discipline

Work one milestone to deployed evidence before expanding scope. Record source,
test, installed-runtime and consumer statuses separately. Preserve active jobs;
inspect deploy.claim and compiler processes before building. Use focused existing
fixtures, not broad repeated suites. No more installer-review detours, duplicate
daemons, voice substitutes, or claims that a schema/capture is a finished feature.

Joel's explicit delivery requirement: a milestone is not a stopping point until
the supported `continuum install` path has deployed it and its actual running
consumer has exercised it. Record the source revision, embedded/runtime revision,
consumer result and limits. A successful source test or a staged executable is
not delivery. Keep incomplete milestones OPEN, including native conversation.

Current execution (2026-10-02 05:04 UTC): PCM format boundary, stateful resampler,
CallManager ownership and future-drop cancellation are implemented; focused tests
passed. Foundation deployed through continuum install as84a362971/build5911,
core and CLI aliases verified. Real PDF visual understanding passed on that build.
No active build/install/test handle remains. Deployed whole-response native output
refuses missing streaming ownership; undeclared AudioInput refuses before generation.
These refusals prevent substitution; they do not prove native audio generation.
Next implement per-inference identity/completion delivery to the persona presentation
owner and explicitly supported native model binding. The cognition request still
sets `native_output: None`; real native voice acceptance remains OPEN. No local
exposed native-audio model was established. Do not bypass the forwarder's media
refusal or interpret whole-turn channel closure as successful inference completion.

## CBAR temporal assembly clarification (Joel, 2026-10-02)

The real-time assembly advances continuously using valid results of different
ages. Retain originating time/scene/intention references, validity intervals,
dependencies and supersession state on contributions; reuse the existing causal
spine. Expensive perception/render results may be remapped against current
tracked geometry instead of blocking the frame. Motion, gaze, breathing and
speech have independent progress; reuse the legacy processes. Cancellation of
an intention invalidates its pending arm motion or queued speech. Interpolate
continuous state only where valid; never fabricate a new semantic observation.
Already played audio is committed, while unplayed audio remains cancellable.
Deep work may take seconds or minutes without freezing participation. The
latency targets measure useful interaction and interruption, not completion of
every deliberation. Validate composition with delayed results and a cancelled
motion intention alongside live speech; do not create another bus or mind.

Conversation layering is a clarification, not a pivot or substitute for native
multimodal completion. Explicit fast and deep model bindings may contribute to
one persona's evolving response through the existing event/cognition substrate.
Merge semantic contributions into revisable future speech; do not acoustically
mix competing sentences. Track the audio playback commit horizon. Motion can
blend compatible body regions; conflicting intentions supersede or cancel, and
lips follow the actual native audio timeline. Voice identity remains bound;
no automatic model fallback or stock TTS. Reuse legacy breathing/nod/blink
processes. Current work closes typed native playback before model-layer wiring.

Joel further clarified control composition: independent processes modify one
voice/body control state, including overlapping influences on the same joint or
articulation. This is not a set of separate audio tracks. Learned composition
and discriminator feedback may train synchrony/expression/identity, through the
existing genome/academy owners with held-out evaluation. Supported bound-model
controls are required; LoRA does not create an unsupported runtime interface.
This extends the native foundation, not a substitute for finishing it.

Live prerequisite receipt (03:10 UTC): installed ai/models/list exposes51 models
on llama-server, none with declared native audio input/output. Resolve a genuinely
capable binding/transport before claiming native voice acceptance; source flags
cannot manufacture support. Per-inference identity/completion must reach the
presentation owner: the current forwarder covers a whole act/observe turn and
channel closure alone cannot authorize successful PCM tail completion.

05:24 execution override: cancellation repair source pending, sole focused validation46325. Resume README handles before any build. Turn-forwarder owner test passed; local presentation retirement extension now under test. Installed foundation remains84a362971.

05:54 source advance: per-attempt request boundaries now wrap bound cognition calls on the shared ring; tests pending install45853 completion. Presentation media binding and negotiated remote lifecycle remain OPEN. Do not infer success from presentation closure.
