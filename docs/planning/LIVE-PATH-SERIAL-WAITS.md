# The live path's serial waits (audit, 2026-11-22)

Joel, 2026-11-22: "Worry some of the scheduling will be too serial and locked still ... Make sure at
least the timing is there for identification of bottlenecks." "Think of what truly depends on
something and whether it can be working from a cache or a window, or sleep." An await is not
forbidden, only usually wrong: the test for every wait is what the consumer truly depends on.

Read-only audit of `fable/refusal-at-door` at 5ef00bb3cb (three parallel reads: engine admission,
the mind's side, SAGE and timing coverage). Claims marked **verified** were re-read at the cited
lines; the rest are the audits' reads, with inferences labelled. The design these feed is SAGE.md,
"The lane".

## The worst offenders, ranked by likely latency (inferred from code shape, not yet measured)

| # | Wait | Holds back | Truly depends? | Fix shape |
|---|---|---|---|---|
| 1 | **Decode permit taken first and held through readiness, slot lease, page save/restore, prompt count, POST and the whole decode** (`continuum-engine/src/inference/turn_admission.rs:412-437`, one permit per endpoint `slots/endpoint_concurrency.rs:46-47`). **Verified.** | every mind: one mind's swap and whole generation block every other mind from starting | only the engine's decode time | the lane (SAGE.md): permit per slice at dispatch; paging, counting and POST before it |
| 2 | **Page save then restore over llama-server HTTP**: a size quote, the save, the restore (`turn_admission.rs:620-629`, `page_quote.rs:41`), all under #1 | every mind (through #1) | the evictee's save never blocks this turn; restore can run ahead | in-process persona save/bind; write-behind save on its own worker; restore before dispatch |
| 3 | **Measured hold**: one global cell; background minds wait up to 4 h while a benchmark runs (`continuum-cognition/.../admission.rs:57-61` → `measured_hold.rs:253-290`) | every background mind | not at all | delete (plan line 0: no benchmark rule in the mind) |
| 4 | **Global clone mutex** held across `gh repo clone`, up to 900 s (`continuum-mind/src/work/card_staging.rs:283`). **Verified.** | any two minds claiming in different new repos | per repo only | a clone worker keyed by repo that publishes "checkout ready" |
| 5 | **Window grant waits for the serving daemon's 5 s tick** (`continuum-serving/.../serving_daemon.rs:64`, **verified**; `mind_window.rs:58-83`), and the act's `ActHold` is held from render through lane, permit and generation (`contribute.rs:50…471`); waiters are `BehindActs` (`fair.rs:30`) | every ungranted mind; a held seat is never reclaimed | cache: render against her last grant (`reserve_attempt` already refuses a stale one) | grants published on events (demand, receipt, act end); the hold taken at dispatch |
| 6 | **Engine gate read lock held for the whole generation** (`slots/engine_generation.rs:154`, **verified**); one queued engine transition blocks every new admission until all in-flight generations drain | every new turn | readiness is a state read | readiness as a watched state; drain by epoch |
| 7 | **The room feed waits for each resident's senses in turn** (`continuum-sage/src/dataflow/stream.rs:88-99`, **verified**; emitter `continuum-mind/.../senses/feed.rs:74`); every component intake holds one waiting input (`component/mod.rs:350`, **verified**) | every resident's perception, behind the slowest | no: speech lands in a bounded per-room view, the forward to admission is already lossy | per-room revision as State (senses page from the transcript), or a windowed per-subscriber intake; acts on a separate reliable lane |
| 8 | **Synchronous continuation save on the Senses worker** (`senses/worker.rs:96` → `mind_state.rs:110-120`: JSON, mkdir, write, rename on an async worker). **Verified.** | stretches #7 by disk latency | the reply does; her other inputs do not | `spawn_blocking` or a save stage; the oneshot carries the outcome |
| 9 | **Stall verdict from another mind's slot**: an HTTP poll of other holders' slots on a sleep loop; no advance abandons her act (`admission_clock.rs:236-265`, `tick_deadline.rs:69-115`) | her whole act is lost on another's progress | the engine's published progress | subscribe to engine progress events |
| 10 | **All-pinned slot wake re-runs the whole admission** (lane, permit, readiness) for every waiter (`turn_admission.rs:489-526`) | thundering herd | rest-until-enough on the slot only | keep the slot queue, do not redo earlier waits |
| 11 | **Lane taken, dropped and taken again** around the one-act gate (`admission.rs:129-162`, `turn_admission.rs:383-406`): two FIFO queues in front of a one-permit bound | queue time | the permit is the real bound | the mind gate before any lane; the lane folded into the permit |
| 12 | Smaller: fixed 1 s perceive settle (`wake_select.rs:81`); spill mutex across `fs::remove_file` (`spill.rs:217`); registry read guard across a generation (`should_respond.rs:230`, `tool_embedding.rs:293`); 50 ms drain poll (`modules/cognition.rs:254`) | seconds or less | cache / window / event | settle on input rate; unlink after the lock; clone the `Arc` out; subscribe to drained |

Not on the path: `continuum-sage/src/flow.rs` (`Flow`/`Grant`) has no production consumer; the
in-process persona scheduler's pick keeps a running turn by policy (`persona_scheduler/frame.rs`),
the in-process form of #1, replaced by the lane.

## Timing: what is measured, and the gaps

Measured today: mind gate (`inference.mind_gate.waited`), decode permit
(`inference.decode_permit.turn_waited`, no page or request id), slot affinity and slot permit, page
action (`inference.kv_page.action`), prompt quote, prefill (`inference.prefill.complete`), the engine
turn (`persona.engine.turn`: wait, switch, prefill, decode, longest service gap), the whole act
(`persona.act.pace`: act, model, residue).

**No timing:** the feed fan-out and senses intake (#7); wake to turn start; the `ActHold` duration;
prompt assembly; the endpoint gate read (#6); `busy_until_release`; the page size quote; POST to
headers on success; each tool call; the reply post; the clone lock wait (#4); the spill and registry
locks. SAGE component telemetry exists (`sage/telemetry`) but nothing turns Observe on in
production, and it times the handler, not the time an input sat in the intake.

**The deeper gap:** the existing wait probes key on different ids (page, slot, request class, persona,
room), so no act can be followed end to end; no per-turn timeline exists.

## One wait instrument

1. **`Wait` in `continuum-base`, beside `probe!`**: `Wait::begin(seam)` → `.granted()` (emits
   `waited_ms`) → drop (emits `held_ms`), one probe class `wait` with `seam`, `act`, `phase`,
   `depth`, plus always-on per-seam count, sum and max (replacing the `WaitRing`s).
2. **The act id from context**: a typed `ActId` on the per-act span (`time_probe!("persona.respond")`),
   inherited by every `wait` row beneath it; the engine's request id joins through
   `persona.engine.turn`.
3. **SAGE instrumented once**: time the room wait in `StageInput::send` (`stage.rs:187`) with
   producer and consumer names, stamp each delivery's enqueue instant and record intake dwell; add
   `last_wait`/`max_wait` to `Telemetry`; Observe reachable from a command.
4. **Existing wait probes migrate into `Wait`**; guards added at every gap above.
5. **The per-act timeline is then a query**: `wait` and timing rows grouped by act, in order;
   unmeasured time between rows shows as an explicit hole.

## Keeping new serial waits out

1. A SAGE property test: one emitter, two subscribers, one stalled; the healthy one receives within
   a bound. It fails today (#7) and pins the fix.
2. The `Wait` counters by (producer, consumer): a replay or test run asserts no producer waits on a
   consumer longer than that consumer's handle time.
3. Component-level replay: a captured room transcript through the feed into N senses, one made slow;
   the others' wake latency stays flat.
4. A source ratchet: new `hear::<`/`.tell(...).await` sites carry a `// depends:` line or fail CI.

## Text and promises on the same path (sweep, 2026-11-22)

Joel: binary streams and binary saves are the default, text only where absolutely necessary ("I bet
we find FEW cases"); a straight copy lets the OS and hardware move it (DMA, mmap, sendfile) with the CPU
only setting up. "Promises are stupid too. Use streams": an await on a promise is only for a small,
one-off result, never anything continuous.

The few text edges that stay: `continuum-gateway/src/wire/*` (OpenAI-compatible JSON for external
drivers), the cloud provider adapters, the hands' GitHub REST calls, and text shown to the model. Every
other site is ours.

**Text encodings, most expensive first** (sizes and frequencies inferred unless stated):

| # | Site | What | How often | Becomes |
|---|---|---|---|---|
| 1 | `inference/request_body.rs`, `ai/openai_adapter/stream.rs:67-406` | the whole prompt as a JSON tree, then bytes, to our own fork | per act, hundreds of KB | binary turn frames: token ids or the epoch's appended tail |
| 2 | `inference/serving_guard.rs:76-89` (**verified**) | the same body POSTed again only to count tokens; the engine re-templates and re-tokenizes it | per act and retry | the count in the generate stream's first frame, or our own tokenizer |
| 3 | `inference/lane_send.rs:520-527` (**verified**) | the whole body parsed back to JSON to change `max_tokens`, then encoded again | per act | set on the typed request before any encoding |
| 4 | `continuum-capture/src/prompt_capture.rs:237` (**verified**) | the full record encoded only to measure its length | per captured act | deleted |
| 5 | `prompt_capture_store.rs:263` | full request and response as JSONL on disk | per act | versioned binary segments |
| 6 | `inference/sse_stream.rs:651-663`, `wire_tap.rs:96-98` | each SSE event turned into a String and parsed as JSON | **per token** | typed token frames from our engine |
| 7 | `continuum-core/src/ipc/ws.rs:256-268` | each token delta sent to clients as JSON text | **per token x clients** | binary WebSocket frames |
| 8 | `continuum-kernel/src/runtime/in_process_transport.rs:82-124`, `command_executor.rs:675` | every tool call's arguments and result as a JSON tree in-process | per tool call | typed values by `Arc` |
| 9 | `tool_executor/spill.rs:463-495` | a result rendered as JSON for the model, then parsed back from that text | per tool call | keep the typed facts; text only for the model |
| 10 | `continuum-positron/src/state.rs:131`, `ws.rs:234` | ViewState converted to JSON at store time, then text per client | per revision | typed at store, binary on the wire |
| 11 | `routing/probe_file_sink.rs:290-330` | every probe field `format!`ed into a JSONL line, flushed per event | per step | a binary ring buffer |
| 12 | `durable_history.rs:96-131`, `message_bus.rs:324,396` | chat rows and bus events as JSON in-process | per event | typed SAGE topics |
| 13 | `turn_admission.rs:734-873`, `page_quote.rs:41` | page save/restore/size as JSON control bodies and receipts, plus a polled JSON `/slots` | per page op | in-process persona save/bind; the KV bytes a straight copy (mmap, DMA) |
| 14 | `mind_state.rs:116` (pretty), `working_set.rs`, `saved_ledger.rs`, `recorder.rs`, `rag_capture.rs` | her state, ledgers and captures as JSON on disk | per act or page op | one versioned binary codec, written behind |
| 15 | `airc_transport.rs:385`, `command_ingress.rs:146-689`, `prepare_unix_socket_path.rs:183-358` | grid command and event bodies, and the CLI socket, as JSON (`Body::Binary` already exists) | per call | binary |
| 16 | `live/transport/call_server/handle_connection.rs:143-152` | a second audio path: base64 PCM inside JSON text, beside the binary frame protocol at `:186-196`; transcripts and call messages as JSON (`:95,129`) | per audio chunk | deleted; binary frames only (BigMama's area) |

Also: errors carried as `String` on the admission and generation path (`admit_turn -> Result<_, String>`),
branched on by text; they become typed errors.

**Promises** (each becomes a topic its producer publishes and its consumers subscribe to):

| # | Site | What flows | Becomes |
|---|---|---|---|
| 1 | `ServiceModule::handle_command` (`service_module.rs:275`), `command_executor.rs:318 execute()` | every module command and tool call as awaited request/response; the root of the rest | intents in, state out: hands hear `ToolCall`, publish `ToolResult{call_id}` |
| 2 | `openai_adapter/provider.rs:276`, `llm_deliberation_faculty/request.rs` | the whole generation as one awaited future beside the token sink | a `TurnStream`: tokens, then `TurnEnd` |
| 3 | `turn_admission.rs:287 admit_turn`, `mind_gate.rs:61` | an awaited admission per act | a per-mind `Seat` state |
| 4 | `unresolved_page.rs:105`, `grant_handoff.rs:55,148` | page resolution and grant reclaim by oneshot | per-slot `PageState` and the window-grant state |
| 5 | `senses/worker.rs:34`, `focus/continue_.rs:148` | "continuation saved" by oneshot | `MindStateSaved{rev}` on her bus |
| 6 | `ipc/protocol.rs:15`, `ws.rs:294-316`, `provider_bridge.rs:57,95`, `airc_transport.rs:349-385` | client, provider and grid commands as request/response by correlation id | intents; results as ViewState revisions or room topics |
| 7 | `persona_scheduler.rs:228,301`, `driver/ending.rs`, `academy_batch.rs`, boot and lifecycle oneshots | suspend, settle, batches, ready | state watches (the `EngineState` watch exists) |

`continuum-sage/src/flow.rs` (`submit` -> oneshot) has no caller outside SAGE: delete it with the lane.

## Order

1. The `Wait` instrument and the act id (so every fix below has a before and after).
2. #7 and #8 with the failing SAGE test (SAGE is mine; one resident's disk write stalls all
   perception today).
3. #1, #2, #6, #10, #11 through the lane (SAGE.md): the live decode loop drives it with the
   in-process swap; the turn-long permit, HTTP paging and second lane are deleted.
4. #3 deleted; #4 a per-repo clone worker; #5 grants on events; #9 progress subscription.
5. Each change gets its 15-minute live read with the per-act timeline.
