# PR891 — What's Broken + Fix Plan

Written 2026-04-16 morning after overnight validation. Honest inventory of what's not working, root cause diagnosis, and who fixes each thing in what order. No celebration. No "mostly working." Just the failures.

## The "shipping PR891 requires" set

### BLOCKER 1 — Docker Desktop VM OOM on Mac

- **Symptom:** Container exploded mid-run during m5-test's M5 validation. Default Docker Desktop VM memory is 6-8GB, inadequate for our workload (postgres + node-server + widget-server + livekit-bridge + model-init + LiveKit server).
- **Root cause:** Default Docker Desktop memory ceiling. Every factory reset drops back to default. Nothing in our install.sh was auto-configuring it until commit `cd516b0a6`.
- **Status:** FIXED in install.sh (commit `cd516b0a6`, 2026-04-16 pushed). Auto-configures to 80% host RAM, 16GB floor, refuses install if physical RAM <20GB. Idempotent — survives Docker Desktop factory resets via install.sh re-run.
- **Verified:** M1 Pro 32GB → Docker VM bumped 7.65GB → 24.42GiB live.
- **Remaining work (this plan):** Add per-service `mem_limit` to docker-compose.yml for services currently unbounded (model-init, continuum-core, livekit-bridge, node-server). Defense-in-depth: even if VM total is correct, one runaway service can still eat it.
- **Owner:** memento (me), pre-merge.

### BLOCKER 2 — Helper AI inference is slow (candle path) AND output gated

- **Symptom:** Helper AI takes 52 seconds to generate 531 chars, then its own output rejected by the `semantic_loop` gate. User sees no reply in chat despite gate=PASS at inbox level.
- **Root cause A (slow):** Helper AI's `PersonaModelConfigs.ts` default provider is `candle`. Candle path to Qwen3.5-4B Q4_K_M on M1 Pro measures ~3.9 tok/s (documented in `project_candle_qwen35_perf.md`). DeltaNet recurrence dispatch overhead is structural. This is the slow-model problem our architecture memos forbid.
- **Root cause B (gated):** `semantic_loop` gate in the output validator is flagging Helper AI's own output as loop-like and silently rejecting it. Separate bug from slowness, but compounds — 52s wasted then no reply posted.
- **Fix A:** Route Helper AI (and other local personas with `provider: candle`) to Docker Model Runner's vllm-metal backend via HTTP, or to continuum-core's own LlamaCppBackend directly. NOT candle. Per `feedback_inference_runtime_split.md`: llama.cpp for inference, candle for training only.
- **Fix B:** Investigate `semantic_loop` gate — why it rejects legitimate Helper AI output. Is it comparing against prior messages from OTHER personas? Threshold too aggressive? New since response_cap removal (`a1e03d8c7`) changed the gate ordering?
- **Status:** Fix A needs new code — the candle adapter internally has `llamacpp_backend: Arc<RwLock<...>>` but the persona-level config still says `provider: candle`. Either (1) rename / re-provider as `local` backed by llama.cpp, or (2) add a `provider: docker-model-runner` that HTTPs to the host-native vllm. Option 2 is the clean shipping path per Mac pivot.
- **Owner:** m5-test for Fix A (they're building MLX adapter; natural to generalize to "local inference provider selection"). memento files issue for Fix B. Both pre-merge if possible, post-merge at latest.
- **Urgency:** Pre-merge if we want "3-5 concurrent local personas in live chat" to actually work on Mac per the inference design goal.

### BLOCKER 3 — npm start vs compose port conflicts (issue #898)

- **Symptom:** m5-test hit 7882 (livekit UDP), 9001 (node), 9003 (widget) port conflicts between `npm start`'s locally-launched services and `docker compose up`'s containerized versions of the same.
- **Root cause:** Option B Mac architecture intends continuum-core native + support services in Docker. `npm start` → `parallel-start.sh` currently launches node-server AND widget-server locally in addition to continuum-core, fighting with the Docker'd versions.
- **Fix:** Add `CONTINUUM_CORE_ONLY=1` env flag to `parallel-start.sh` that, when set, skips node-server + widget-server + seed + browser-reconnect phases. install.sh sets the env var; Dev workflow uses `npm start` without it.
- **Status:** Issue #898 filed with proposal. Not yet coded.
- **Owner:** Unclaimed. Small scope — ~20 lines in `parallel-start.sh` + one-line change in install.sh. Either memento or m5-test.
- **Urgency:** Pre-merge. Blocks heartbeat from running clean end-to-end on Mac.

## Pre-existing issues (surfaced but not caused by PR891)

### Continuum-core native memory leak

- **Symptom:** MEMLEAK tracker in continuum-core.log shows `ai/generate:+36811MB` accumulated — process RSS grows unbounded across sessions. My binary (PID 91992, running since 9:55pm last night) is at 10GB+ RSS.
- **Root cause:** Unknown. MEMLEAK lines show top accumulators are `ai/generate`, `data/query`, `rag/compose`, `embedding/generate`. Likely candle tensor-lifecycle issue, or IPC buffer retention, or something in the RAG source list.
- **Status:** Pre-existing, visible in logs for weeks. Was already there on Tuesday's binary before any of my strip+LTO changes.
- **Fix:** Separate investigation. Needs profiling (heap snapshot comparison across time) to localize.
- **Owner:** File as issue post-merge. Not blocking PR891.

### Docker Desktop first-launch vmnetd permission

- **Symptom:** `open -a Docker` from script can't satisfy macOS's privileged-helper install permission dialog. First-time users must launch Docker Desktop manually.
- **Status:** Unavoidable — macOS security model requires interactive GUI click for privileged helper installation. install.sh fails loud with numbered instructions (commit `66fb24c36`).
- **Fix:** None possible at our layer. Document + fail-loud is the correct pattern.
- **Owner:** Documentation only.

## Nice-to-have (post-merge)

- **CI stage-2 refactor** (detect-changes + slice jobs) — waits until Vulkan CI settles.
- **MLX adapter phase B/C/D/E** — m5-test's track, #897.
- **Self-hosted CUDA runner on BigMama** — Phase 3d of RESTORE-FULL-PARITY-PLAN.
- **Vision-Qwen3.5 per device tier** — #894.
- **Live multi-persona concurrency benchmark** — #895 regression guard.
- **Whisper vendor proper** (un-cheat SKIP_STT) — Phase 1a RESTORE-PLAN, claimed by memento.

## Execution order for pre-merge (this morning)

1. **memento:** per-service mem_limit in docker-compose.yml — defense-in-depth on top of VM-level fix. ~15 min.
2. **memento OR m5-test:** #898 `CONTINUUM_CORE_ONLY=1` env flag in parallel-start.sh. ~30 min. Unblocks clean heartbeat.
3. **m5-test:** Route local personas away from candle. Options:
   - Minimal: change `PersonaModelConfigs.ts` default `candle` config to route through the LlamaCppBackend inside candle_adapter (already exists, may just need persona config tweak).
   - Clean: new `provider: docker-model-runner` adapter that HTTPs to host vllm-metal.
4. **memento:** File issue for `semantic_loop` gate rejecting own output — investigate post-merge.
5. **m5-test:** Clean heartbeat run on M5 with all above fixes. Last acceptance signal.
6. **memento + m5-test:** Final PR review pass, merge.

## What we ship

- Mac: Docker Desktop + Docker Model Runner + native continuum-core with Metal. One-command install (two GUI clicks first-time only). M5: 50+ tok/s single, 128+ batch-8. M1 Pro: 12+ single, 20+ batch-4.
- Linux/Nvidia: Docker + continuum-core-cuda. BigMama measured.
- Linux/AMD-Intel-VirtIO: Docker + continuum-core-vulkan. CI in flight.
- Support services containerized with sane mem_limits.
- install.sh auto-configures Docker Desktop VM memory to 80% host RAM.
- Every subsystem runs on real GPU silicon. No CPU fallback anywhere. No emulation.

## What we don't ship (follow-up)

- MLX-native inference (phase B+ of #897)
- Vision-enabled Qwen3.5 per tier (#894)
- Multi-persona concurrency regression gate (#895)
- `semantic_loop` gate fix
- Memory-leak investigation in continuum-core-server
- Self-hosted BigMama CI runner
