# Flow inventory: every place that queues, gates or polls (2026-10-28)

A static read of `core/` at `fable/lanes-flow` 01ffb5ca0. No cargo was run; test-only code is excluded.
Each row gives: location | what it gates or queues | the resource it protects | whether a live conversation waits on it | its category.
Categories: **A** = flow primitive, **B** = context codec, **C** = real lock (keep), **D** = poll to make event-driven, **E** = dead or duplicate.
The plan is [FLOW-IMPLEMENTATION-PLAN](../architecture/FLOW-IMPLEMENTATION-PLAN.md).

**Found by the read:**
- **The lane admission is dead.** After 7f9226d89, `LanePriority`, the directed reserve, the work ticket and ambient aging (`resource_admission/lanes.rs:399-617`, `progress.rs:171-219`) have no production caller.
- **The in-flight gauge reads 0 forever.** `inflight_model_calls()` was fed only at `lanes.rs:490`. Its readers: `activity_gate.rs:173` (dreams), `placement_reservation.rs:93` and `sse_stream.rs:97/125`.
- **`directed_pending::wait`** (`directed_pending.rs:81`) has no production caller.
- The live node was rolled back to 4579b4711 for these.

## Inference and serving
- A `mind_gate.rs:28,51` (taken at `turn_admission.rs:280`): one act per mind, FIFO Semaphore(1) | mind sequence | Y
- A `turn_admission.rs:288` + `openai_adapter/runtime.rs:60`: adapter Semaphore(lanes), FIFO, no class | engine lanes | Y
- A `openai_adapter/provider.rs:202-240`: permit grow/shrink task | engine lanes | indirect
- A `slots/engine_generation.rs:83-140`: `admit_when_ready` | engine readiness | Y
- A `slots/engine_generation.rs:149,175,202,216`: RwLock drain barrier (write-preferring) | engine generation | Y
- A `turn_admission.rs:334-371`: park on `pool.released` when all slots are pinned | KV slots | Y
- A `slots/kv_slot_pool.rs:152-232`: lease, evict, retry | KV slots | Y
- C `slots/kv_slot_pool.rs:55,257`: per-slot op Semaphore(1) | one slot's IO | Y
- A `turn_admission.rs:434-466,676-701`: page save/restore before the turn | engine+disk | Y
- A `turn_admission.rs:485-556`: transient/scratch borrow | slot | probes
- A `slots/kv_slot_pool.rs:459,497`: paging JoinSet | disk | N
- C `slots/citizen_slots.rs:243`: single-flight seed | disk | N
- D `llama_server/progress.rs:395-420`: checkpoint wait loop | engine | Y
- A `serving_guard.rs:293`: `await_ready_serving` | lane | Y
- D `lane_send.rs:256-285`: linear backoff on 503 | engine | Y
- A `measured_hold.rs:94,234-289`: background defers while held | GPU | N
- E `openai_adapter/stream.rs:171`: duplicate `should_defer` backstop
- A `resource_admission/lanes.rs:10,384`: ambient permit (skip when busy) | lanes | ambient
- A `resource_admission/lanes.rs:17`: bounded wait for a starving mind | lanes | N
- E `resource_admission/lanes.rs:399-617`, `progress.rs:171-219`: dead QoS/aging (the blueprint)
- E `resource_admission/lanes.rs:512,561`: 1 s missed-wake guard
- E `resource_admission/lanes.rs:31-41` + `start_server.rs:55`: `DIRECTED_PENDING` install
- E `resource_admission/serving_lane_permit.rs:10-44`: `InflightModelCall` (BUG: reads 0)
- C `resource_admission/lanes.rs:107,184,207`: resize accounting
- A `resource_admission/gate.rs:48-125` (`generate_response.rs:101,366`): lease registry, max 4, refuses | lanes | Y
- A `training_admission.rs:76-146`: wait for training memory | VRAM | N
- A `serving_holds.rs:17,77` (`exam_serving.rs:151,163`): holds suppress re-home | lane | indirect
- A `lane_demand.rs:45,102`: quiesce overrides | lane plan | N
- A `backends/llamacpp_scheduler.rs:134,291,429`: in-process batch driver, 1 ms idle sleep | GPU | Y if used
- D `llama_cpp_backend.rs:626`: 2 ms spin
- D `llama_server/readiness.rs:196`, `launch.rs:101`, `snapshot.rs:453`, `adopted_retirement.rs:88`: load/liveness polls
- D `lane_process/kill9.rs:231`, `parse_tasklist_image_name.rs:101,243`: exit polls
- D `single_purpose_lane/memory.rs:116`: 5 s headroom poll
- D `stderr_relay.rs:242`, `follow.rs:58,116`: log follow
- D `airc_remote/transport.rs:279,346`: 2 s reply-recovery poll | remote reply | Y
- C `capacity/gossip.rs:318,349`
- A `coordinator.rs:96`, `coordinator_pool.rs:179`, `inference_coordinator_module.rs:151`: per-module max_concurrency (legacy route)
- A `plasticity/mod.rs:51`: compaction max 1
- A `embedding/try_neural_embedder.rs:92-117`: single-flight load + 60 s cooldown | embedder | first recall
- D `embedding_cache_max.rs:388`: flush interval
- A `planning/adaptive_throughput.rs:113,168`: `coalesce_by_identity` (the coalescing precedent)
- C `resource_admission/metrics.rs:207,407`, `coordinator.rs:361`, `prefill_knee.rs:174`: sample rings
- D `serving_daemon.rs:64` (5 s), `policy.rs:190`, `reconcile.rs:384-398,719`, `launch.rs:250`: reconcile ticks/cooldowns
- D `serving_daemon/academy_batch.rs:151,378,399`: Mutex + 100 ms polls
- C `serving_daemon/launch.rs:483,503,558`, `serving_daemon.rs:386`

## Kernel
- E `concurrency/policy.rs:56-95`: `TokioConcurrencyPolicy` (no users)
- E `concurrency/priority_queue.rs:18-39`
- E `concurrency/message_processor.rs:39`
- A `runtime/per_key_gate.rs:109` (`training_trigger.rs:182,192`): per-key serial gate (precedent)
- B `runtime/shared_compute.rs:54`: compute once, share | CPU | Y
- C `paging/pool.rs:419,492`: single-flight loader
- A `runtime/runtime/shutdown_phase.rs:131-134,485,569`: per-module Semaphore(max_concurrency) | legacy route
- E `shutdown_phase.rs:446` + `ai_provider/a_i_provider_module.rs:16`: limiter bypassed by typed commands
- D `runtime/runtime/runtime.rs:189-279`: ServiceModule tick
- D `runtime/daemon.rs:250`: Daemon ticker
- C `runtime/admission_gate.rs` (`turn_ingress.rs:69`): shutdown door
- D `turn_ingress.rs:184` (200 ms), `continuum-mind/src/modules/cognition.rs:246-257` (50 ms): drain polls
- C `runtime/message_bus.rs:90,139`
- A `routing/airc_event_transport.rs:113,313`: bounded delivery 256 | network | Y
- C `routing/probe_router.rs:166`
- A `resources/daemon.rs:71,214,258`: ResourceDaemon | VRAM/RAM | N
- A `resources/broker.rs:41-71,255-266`, `throughput_lease.rs`: per-class throughput lanes
- A `resources/governor.rs:153`: ledger acquire
- D pressure monitors: `memory_pressure/poll_interval.rs:6`, `disk_pressure.rs:448`, `gpu/memory_manager.rs:262`, `gpu/monitor.rs:209`, `gpu/tracker.rs:226`, `absence_watch.rs:41`, `docker_tier_pool.rs:304`, `paging/broker.rs:125`
- D `bounded_command.rs:133,159,225`: try_wait polls
- C `orm/connection_manager.rs:64,179,202`
- A `orm/sqlite/open_connection.rs:79-127`: writer Mutex; round-robin readers block on a busy reader | disk | Y
- A `orm/migration.rs:453`: throttle
- C `identity/hands_gate.rs:52,121-198`; C `hands_lease.rs:12`
- D `pressure_broker_module.rs:129,307`

## Cognition
- A `llm_deliberation_faculty/admission.rs:44`: pre-gate `defer_while_held`
- A `turn_ledger.rs:11-13,162-186`: starving-mind bounded wait (aging)
- E `directed_pending.rs:35,81`: `wait()` has no caller
- D `activity_gate.rs:53,162`: 1 s gate tick
- A `activity_gate.rs:238-338`: dream cancellation (preemption)
- A `deferred_faculty.rs:81-127`: latest-wins watch (keep-latest precedent)
- B `cached_source.rs:150`: Mutex held across `deliver().await` | RAG | Y
- C `check_redundancy.rs:160` (+ `should_respond.rs:230`, `validate_response.rs:183`, `rate_proposals/orchestrator.rs:125`, `shared_analysis/mod.rs:297`, `generate_recipe/orchestrator.rs:150`, `tool_embedding.rs:296,390`, `inference_session.rs:351`, `request_replay.rs:313`): adapter registry RwLock read held across generation | Y
- A `act_observe/settle/tick_deadline.rs:33-70`: act deadline | Y
- A `workspace/capacity.rs:79-160`: parked until capacity changes
- B `deliberation_budget.rs:136-239`: room speech rings
- B `working_memory.rs:253,292`, `action_log.rs:67`, `bounded_room_ledger.rs:57`, `seen_ids.rs:13`, `recall_faculty.rs:243`, `rag_source_faculty.rs:163`
- C `dispatch_listener.rs:84`, `model_binding.rs:58`, `prompt_capture.rs:279`, `workspace_capture.rs:174`
- D `handoff.rs:365`: 5 ms poll | Y

## Persona loop and mind
- A `service_loop/wake_select.rs:23,52-112`: biased select per mind (the per-owner dispatcher today) | Y
- A `serve_persona_loop_inner.rs:158-173`: `Wake::Tick` runs the self-turn inline; a directed line waits | Y
- A `wake_select.rs:107,118`: quiesce gate
- C `serve_persona_loop_inner.rs:141`: shutdown door
- B `serve_persona_loop_inner.rs:232` + `loop_dedup.rs:149`: loop-filler defer | Y
- C `serve_persona_loop_inner.rs:330`, `obligation.rs:377-382`: obligation write | Y
- A `turn_cycle.rs:160-176`: `active_claims().await` bypass | Y
- A `turn_cycle.rs:179`: ambient permit or skip
- A `burst_fingerprint.rs:310`: `hold_ambient_turn_for`
- A `burst_fingerprint.rs:100,126`: 250 ms token flush | stream/TTS | Y
- A `wake_backlog.rs:27-29,174,217-238,280`: pending-input custody, per-beat sleep | Y
- A `persona/inbox.rs:47` (`enqueue_message.rs:56`, `turn_frame/execute.rs:148,367`): BinaryHeap inbox | Y
- A `channel_queue.rs`, `channel_registry.rs:45`, `modules/channel.rs:57,162,206`: per-domain queue, 60 s tick
- E `persona/service_module.rs:644`: max 1 + 250 ms tick over an empty map
- A `airc_persona_conversation/polled.rs:491,607` (`INBOX_CAPACITY` 8192): live inbox | Y
- D `polled.rs:16,684`, `inbox_capacity.rs:175,214`: 60 s catch-up | Y
- D `inbox_capacity.rs:24`: resubscribe backoff | Y
- B `polled.rs:80-82,348,415`: text/seen rings, backlogs
- A `perception_feed.rs:212-229`: `try_send` drops the NEW event when full | Y
- C `perception_region.rs:174`, `stream_rail.rs:42`
- D `command_inbound_pump.rs:415,438,545`; D `persona_airc_runtime.rs:313,796`
- A `substrate_governor.rs:124,280,400` + kernel `cadence_table.rs`, `share_controller.rs`: 1 s pass, sequential region ticks
- A `airc_runtime_registry.rs:402-432`: fleet quiesce
- A `placement_reservation.rs:41,165,213-283`: seat lease + backoff
- A `placement_switch/decision.rs:31`, `grid/placement.rs:39`, `placement/opportunity.rs:44`: move cooldowns
- A `card_carry.rs:15` + `modules/card_carry.rs:50`
- B `message_cache.rs:94,186`, `work_board/events.rs:83,130`
- D `chat/chat_data_handle.rs:430-439`

## Perception, RAG, memory, embedding, store
- A `continuum-embedding/src/module.rs:106`: embedding max 1 | Y by command
- B `continuum-memory/src/memory/mod.rs:302-330`: up to 64 embeds inline per recall | Y
- C `memory/cache.rs:29`
- A `store/data/max_concurrent_queries.rs:12,148`, `data_state.rs:99,152,248`: query Semaphore(16) | Y
- C `data_state_2.rs:344`; C `adapters.rs:42`, `data_state_2.rs:513-624`

## Tools and hands
- A `tool_executor/command_executor.rs:615`: unbounded `join_all` | Y
- C `hands/code/ports.rs:227,248`
- D shell polls: `shell_session.rs:149,278`, `run_shell_command.rs:401`, `code_shell_kill_result.rs:297`, `modules/code.rs:326`, `python_adapter.rs:252` | Y

## Live media
- A `call_server/text_preview_length.rs:9-13` + `call_manager.rs:476`: STT Semaphore(2), drops the utterance | Y
- A `audio/stt/whisper.rs:29,169`: one Whisper Mutex | Y
- A `audio/tts/kokoro.rs:27,311`, `piper.rs:22,84`, `orpheus.rs:118,790`, `pocket.rs:47,385`: one Mutex per TTS model | Y
- C `call_manager.rs:112,126`; C calls/participants RwLocks (about 50 sites); C `audio/router.rs:122,130`; C `vad/production.rs:79,179`
- B `transcription_entry.rs:13`, `live_kit_agent_manager.rs:560`, `perception_buffer.rs:72`
- D `voice_module/session.rs:119,148,209`: fixed 1 s/3 s/2 s-per-agent sleeps at call start | Y
- D `audio/resource_lifecycle.rs:31-47,134,173`
- A `avatar/render_loop.rs:28,214,329,449`: avatar slot pool, 100 ms waits | Y
- D `video/source.rs:92,154,160`, `bevy_renderer/api.rs:150,191`, `consumers/avatar.rs:115,149,258`
- A `livekit-bridge/src/agent.rs:48,137,595`, `server.rs:36,42`: video Mutex, 1 fps throttle, unbounded channels | Y
- A `ipc/start_server/live_call.rs:219`: human audio mpsc(128) | Y

## AIRC and grid
- A `ipc/command_ingress.rs:439,686`, `served_generations.rs:169`: leased-in admission, refuses past served lanes | Y
- B `airc/realtime_store.rs:124`
- D `inbound_attach.rs:109-130`, `reattach.rs:48,174`, `daemon_liveness.rs:52,85`, `discovery_aggregate.rs:122,245,302`, `fd_relief.rs:18`
- A `ipc/ws.rs:188,203`: ws mpsc(64) | Y
- D `grid_allocator.rs:586`, `grid/mod.rs:201`, `connection.rs:33,37`, `capacity/gossip.rs:299`, `modules/grid_capacity.rs:86`
- C `transports/tailscale.rs:39,52`, `grid/audit.rs:67,92`, `provider_bridge.rs:261`

## Genome, training, dreams, academy
- D `training_trigger.rs:908`: 30 s tick
- A `genome/lease_pull.rs:404-418`: per-peer Semaphore
- D `lease_pull.rs:447`, `grid_peer_adapter.rs:486`: 10 ms polls
- C `job_board.rs:146`, `job_actor.rs:263`, `native_jobs.rs:149`, `examiner/run.rs:39`
- D `engine_lora_adapter/train_status.rs:727-780`: 2 s steering poll; hold read every tick
- E `governed_training_free.rs:100`: `hold_training` (no callers)
- A `engine_lane.rs:66-72`, `engine_lora_fine_tuner.rs:44,513`, `cuda_lora_adapter.rs:195`, `mlx_lora_adapter.rs:334,752`: trainer admission/memory waits
- D `genome_fitness_sentinel.rs:50`, `training_completion_sentinel.rs:90`
- A `dream_consolidation/region.rs:84,92,169,278`, `consolidate.rs:8`: dream cooldowns
- A `dream_consolidation/distiller.rs:391`: `defer_while_held`
- D `eval/warm_gate.rs:42`, `fork_eval_cycle_waiting.rs:31`
- A `eval/cognition_eval.rs:23`, `exam_serving.rs:109`: fleet quiesce
- A `swe_bench/fetch_hf_rows.rs:152-199`: rate-limit backoff | external API

## Sentinel, positron, miscellany
- A `sentinel_module.rs:437,480`, `steps/llm.rs:146-152`, `executor.rs:505` (the watchdog is D)
- D positron sampling ticks: `positron_presence.rs:321,615`, `positron_metrics_source.rs:119`, `positron_live_source.rs:53`, `positron_serving_source.rs:256`, `vitals_emitter.rs:264`
- A `continuum-positron/src/broadcast.rs:126,159`, `session_task.rs:152-181`: latest-wins watch + `min_interval` sleep | UI | Y
- B `positron_source/rather.rs:329`; C `positron_nav_source.rs:326,421`
- D `memory_guard.rs:15,54`, `rehome_reconciler.rs:43`, `ai_provider/block.rs:30,80`, `auth.rs:653`, `floor_fetch.rs:122`, `deploy_tracker.rs:409`, `benchmark_grade.rs:63`, `rest_seats.rs:194`
- A `forge/custodian_supervisor.rs:138`, `bin/forge_custodian.rs:98,116`: slots Semaphore
- E `inference-grpc/src/priority_queue.rs:193-291`, `worker_pool.rs:61-198`: unused (its own header says so)

Totals: A 74, B 13, C 28, D 34, E 11 (160).
