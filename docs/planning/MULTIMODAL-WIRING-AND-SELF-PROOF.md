# Multimodal Wiring + Self-Proof

*2026-09-02. Joel: "All this multimodal worked in legacy. Need to plan how to wire all
up… make sure readme backs up novel claims and that we've tested reliably, repeatedly…
it can't require intervention. Needs to be automated and natural."*

**The doctrine this doc adds: every README claim gets a VERB that proves it, and the
verbs run on a schedule — not on Claude.** A claim whose proof requires a human (or an
agent) driving is not tested; it is demoed. Legacy proved the UX once, by hand, in one
client; the core proves it repeatedly, headless, in receipts.

---

## 1. Legacy → core wiring map

Legacy (Node web desktop) shipped, per `legacy/src/{system,commands,widgets}`:
voice start/stop/synthesize/transcribe, voice snapshots, LiveWidget (tiles, captions,
controls, call tracker), avatar snapshot, presenter mode, voice-capture/playback
worklets.

| Legacy capability | Core organ today | Status | Missing wire |
|---|---|---|---|
| Voice capture → STT → captions | binary media plane → `push_remote_human_audio` → VAD → `transcribe_and_broadcast` → TranscriptSink → room message | **Deployed, unverified live** | `live/selftest` (below) closes it WITHOUT a human |
| Persona speech (TTS out) | `speak_in_call` → `tts_service::synthesize_speech_async` → bridge `AudioPcm` channel | Deployed, unverified | selftest leg 2: assert media-plane write receipt |
| Captions / live transcript UI | TranscriptSink already lands room messages; chat renders them | Wired | caption styling only (client) |
| Avatar tiles / video | Bevy pump → `publish_video_frame` (binary `VideoRgba`) + `push_avatar_frame` native plane | Deployed, unverified | selftest leg 3: frame-counter receipt on a headless call |
| Human video in → citizen sight | `VideoJpeg` → `perception_ingest` → PerceptionBuffer | Deployed; `perception/observe` proven live nightly | per-call sampling receipt |
| Presenter mode | `push_video` accepts client frames (server side exists) | Client wire missing | carded (20fe404a) |
| Voice snapshots (room/participant) | `live_calls` / stats exist | Partial | thin verbs if needed |

**The pattern: the organs survived the rewrite; the PROOFS didn't.** Legacy validated by
a human clicking; the core validates by nothing. That's the actual gap — not features.

## 2. The self-proof battery (`live/selftest` and siblings)

**`live/selftest`** (built with this doc): an entirely server-side round-trip, no
browser, no human, no bridge required —

1. `join_call` a synthetic call as a synthetic human (server mints the handle).
2. TTS-synthesize a known phrase ("continuum selftest {nonce}") → i16 samples.
3. Feed the samples through `push_audio` in real-time-shaped chunks + a silence tail —
   the SAME VAD → speech-end → STT → TranscriptSink path a live caller exercises.
4. Poll the call's room for the transcript message; fuzzy-match the phrase.
5. Receipt: `{ matched, transcript, tts_ms, stt_ms, end_to_end_ms }`. Leave the call.

This one verb converts "I don't think they were hearing me" from a debugging night into
a red row in a receipt. Extensions (same shape, later): leg 2 asserts a persona
`speak_in_call` produces a media-plane write receipt; leg 3 counts avatar frames on a
headless call; leg 4 runs the whole loop through a real bridge when one is up.

**Scheduling**: the boot rail runs `live/selftest` once serving is ready (non-fatal,
receipt probed as `live.selftest.*`); a nightly cron repeats it. A regression lands as
a probe diff, not as Joel noticing silence in a call.

## 3. Benchmark autopilot (stop hand-managing)

What a hand did this week, promoted to substrate edges:

| Hand act (this week) | Automated edge |
|---|---|
| Operator note naming each card/instance/path to unstick thrash | Re-say now carries instance + staged workspace path (staged at dispatch, `card_instances`) — the substrate's own kickoff is as informative as the hand note was |
| Watching for "is it stuck" | Round VERDICT (`unstarted/grinding/stalled`) on the board + CLI, core-pronounced |
| Diagnosing silent resume death | Liveness probes on the resume watch (merged) |
| Kicking rounds after reboot | Boot resume + named re-says (merged); becalmed watchdog stands |
| Choosing what to run next | **TODO — the standing round**: a cron activity keeps one Verified-mini round in flight whenever no round is Working; grades bank; the claim's N grows nightly with zero operator turns |

The standing-round cron is the last piece of "benchmarks run themselves"; it composes
existing verbs (`benchmark/dispatch` on `swe-bench-verified-mini` + the round lifecycle)
and needs no new machinery beyond a scheduler entry.

## 4. README claim → proof verb ledger

Every "Nobody else ships these" line, with its proof status (from the 2026-09-02 audit):

| Claim | Proof verb / receipt | Status |
|---|---|---|
| Citizens, not sessions (persistence) | reboot mid-round → round resumes; probes + frames | **Proven, repeatable** |
| KV follows the mind | `delib.generate.cache` hit-rate probes (0.4–0.95 fleet) | **Proven, continuous** |
| Time-to-act published | act-pace probes | **Proven, continuous** |
| Verdicts with provenance | forge-alloy publish path | **Proven (Factory)** |
| Both kinds, one interface | `perception/observe` (citizens read the same UI) | **Proven nightly** |
| Skills as heritable weights | ☐ NEEDS `genome/prove-inheritance`: citizen A learns → gene → citizen B measurably improves, receipted | **No live receipt — pre-launch gate** |
| Teams that learn from teamwork | ☐ NEEDS one reviewer-catch→training-row receipt (teams A/B batch) | **No live receipt — pre-launch gate** |
| Voice/live senses | ☐ `live/selftest` (this doc) | **Verb lands now; schedule next** |

Rule going forward: a new README claim ships WITH its proof verb, or it ships as
"designed, landing" — the same falsifiability bar forge-alloy holds for model cards.
