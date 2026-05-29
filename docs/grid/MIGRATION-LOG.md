# Migration Log — TS → Rust Persona Surface

Tracks per-module decisions in the migration from TS-coupled persona infrastructure to a pure-Rust core. Pace is small, focused, merge-as-we-go (Joel 2026-05-29: "We will want to write down a lot in migration docs as we got and keep merging, piece by piece").

## Doctrine (Joel 2026-05-29)

- **No fallbacks.** Drifting two-path decision logic is the most dangerous pattern.
- **No amateur heuristics on first-class citizens.** Substring matching, magic-number arithmetic, time-decay throttling — all violate the citizen-of-continuum framing.
- **TS is widgets + config UX**, one interface among many. Pure-Rust forms must exist (AR, headless grid persona on a 970, OpenClaw).
- **Commands are kernel-level**, compose, used by clients AND the system itself. Rust-implemented, ts-rs-bound, generator-authored.
- **Commands ARE tool calls.** One executor surface for: (a) persona LLM tool-use, (b) UI command invocation, (c) `./jtag` CLI. The shape the model emits and the shape the UI emits both dispatch to the same Rust executor. No parallel paths.
- **Commands compose across the grid via airc.** A command dispatched on the MacBook Air can route to a 5090 box's executor over airc and stream results back via ack/promises/async. So `inference/generate` runs *wherever the GPU lives*, not just locally. **This is why TS-locked commands break the architecture** — they can only run on nodes with nodejs. Pure-Rust commands run on the 970, on a Raspberry Pi, on a friend's machine, inside an AR headset's compute.
- **Base classes make commands + events portable across airc.** Joel 2026-05-29: "Same is true for events and commmada and events are portable across boundaries. This is absolutely mission critical for airc transport. Think of yourself as a Java developer for a bit." Each command param + event payload extends a base type with the wire-required fields (correlation id, session id, source identity, timestamps). The base types ARE the airc serialization contract: ts-rs generates identical TS shapes from the Rust source of truth, so the same envelope deserializes identically on both ends. No remote-aware variants, no parallel paths — strong-typed Java-style inheritance is the portability infrastructure.
- **Migrate, don't blindly delete.** Each module classified before action.

## Per-target classification

Categories used in the audit:

1. **Dead code** — zero callers across all forms → delete.
2. **Drifting fallback** — two paths for the same decision, second runs when first fails → delete the secondary.
3. **Amateur heuristic doing core work** — substring match, magic number, time-throttle → delete; the cognition decides.
4. **Form-specific implementation of a universal command** (TS DOM screenshot, JS code exec) → keep. Web form's correct concern.
5. **Security fail-closed default** (CallerDetector returning 'script') → keep. Conservative under uncertainty.
6. **Graceful degradation in a model/provider chain** (trained-adapter → base-model) → case-by-case. Rename if "fallback" naming is misleading.
7. **Emergency / panic-path logging** → keep, even if currently uncalled. Cheap insurance.
8. **Core-shaped TS** (cognition, decision, training, dispatch in V8) → migrate to Rust, expose as command if UI-callable, then delete TS.
9. **Integration adapter** → check if Rust path preserves the integration; migrate or delete accordingly.

---

## Log entries

### 2026-05-29 — PR #1459 (persona-surface delete-fallbacks sweep)

**Net:** +290 / –2253 LOC (–1,963 net).

#### Deleted (category 1, 2, 3)

| Target | Category | Why |
|---|---|---|
| `PersonaWorkerThread.ts` + `persona-worker.ts` + 3 worker tests (≈1,576 LOC) | 2 | Three independent self-incriminating comments confirmed it as the "model-free fallback for should-respond" secondary path; primary is `rustCognition.fullEvaluate()` (line 151 of PersonaMessageEvaluator). The drifting two-path was real: workers didn't know about response_cap, rate_limit, sleep_mode, directed_mention. |
| `PersonaUser.shouldRespondToMessage` (57 LOC) | 1 | Zero callers. The actual gate is `responseGenerator.shouldRespondToMessage`. |
| `PersonaUser.calculateResponseHeuristics` (65 LOC) | 1 | Only caller was the heuristics fallback branch in the dead `shouldRespondToMessage`. |
| `PersonaUser.getPersonaDomainKeywords` (27 LOC) | 1 + 3 | Zero callers. Substring-matched a persona's display name to a hardcoded keyword list. |
| `PersonaResponseGenerator.inferTrainingDomain` (10 LOC) | 3 | Substring-matched message content to a domain label, used as silent backup when Rust classifier failed. Now: skip the training capture (no corpus poisoning). |
| `SignalDetector.detectSignal` + `quickClassify` + `inferTraitFromContent` + manual test (≈222 LOC) | 1 + 3 | Sync method had only manual-test callers. Heuristic helpers were called from the sync method and from two drifting-fallback sites inside the async path. |
| `PersonaToolExecutor.executeToolCalls` + `formatToolResult` + dead test (≈70 LOC) | 2 | "XML fallback path for non-native providers." Native protocol is the path. |

#### Doctrine fixes (no LOC delta but behavior change)

| Target | Why |
|---|---|
| `shouldRespondToMessage` (BEFORE deletion was discovered) | Was doing age-penalty arithmetic + static-threshold compare on the worker's calibrated ML output. Replaced with `return result.shouldRespond` — trust the cognition. *Then we learned the whole method was uncalled and deleted it.* |
| `@mention as ML feature, not bypass` | Was `if (isMentioned) return true` overriding the ML. Now mention + sender-type passed as features to the cognition; the persona "knows it was mentioned" via the input vector. |
| `PersonaAutonomousLoop.handleItem` 3 fallback nests | classify-catch swallow, "if-bridge-unavailable" different-code-path, response-catch swallow. All propagated to the circuit breaker now. |
| `PersonaUser` init swallows: ModelInfo IPC, Rust cognition, ResourceManager registration, genome STUB MODE, status online/offline writes, auto-join general room, catch-up, bookmark-advance, corpus-reload-post-Hippocampus | Each silent catch meant a persona could come up reporting healthy but with a broken init step. Now: init throws, daemon notices, system surfaces real bugs. |
| `PersonaMessageEvaluator` fire-and-forget swallows: signal detection (was "non-fatal"), Rust trackResponse (was "non-fatal") | Awaited. Failures surface through the outer evaluation catch which is correctly silent-on-error. |
| `PersonaResponseGenerator.captureTrainingData` drifting two-path | Either ML classifier succeeds (use the label) or skip the training event entirely. No heuristic backup label that would poison the corpus. |

#### Renamed (category 6 — graceful degradation misnamed)

| Target | New name / phrasing | Why |
|---|---|---|
| `CLOUD_PROVIDER_FALLBACK` → `CLOUD_PROVIDER_PREFERENCE_ORDER` | The list is operator-preference order for which cloud provider to try first WHEN cloud routing is explicitly enabled (default: never). Not a fail-over chain. |
| `Base model fallback` (RustCognitionBridge model selection chain) | "Base model (universal default — no adapters available)". 4-tier priority chain selects ONE per call; not a fail-over. |
| `'silent fallback'` historical comment in PersonaModelConfigs (Issue #957) | `'silent default-substitution'`. Describes the closed bug's failure mode without the trigger word. |

#### Kept (category 4, 5, 7)

| Target | Category | Why |
|---|---|---|
| `CallerDetector` 'safe fallback' to `'script'` | 5 | Security fail-closed under uncertainty. The misleading "fallback" word in the comment is low-priority to rename. |
| `PersonaLogger.emergencyLog` | 7 + 1 | Dead but cheap insurance. Skipped deletion. |
| `TaskAwareProviderRouter` cloud routing chain (after rename) | 9 | Configuration-resolution for an integration. Default is never-invoke (CLOUD_REQUIRED_DOMAINS empty per doctrine). |

#### Ratchets

- `ts-persona-forbidden-strings`: baseline 83 → current 59 (`fallback_mention` delta –24). Locked-in post-merge.
- `ts-eslint-baseline`: baseline 5431 → current 5402 (–29 errors).
- `ts-persona-cognition-ratchet`: passed.

#### Open follow-ups (not in this PR)

- `boostedPriority = Math.min(1.0, priority + 0.2)` for voice (PersonaUser ~line 1546): magic-number modality urgency boost. Modality urgency is contextually real, but +0.2 is arbitrary. Deferred — check whether the inbox prioritizer uses fuzzy ML or fixed sort first.
- `mi.contextWindow ?? mi.context_window ?? 8192` (PersonaUser ~line 752): magic-number 8192 fallback for missing context window. Defer — verify adapters always return contextWindow before deleting.
- Corpus load swallow in parallel-task (PersonaUser ~line 856): legitimate startup-race handler for schema-not-yet-created. Honest fix is sequencing the corpus load AFTER `ensureDbReady` — eliminates the race, then catch can be removed. Deferred — bigger structural change.
- `ORM.update` `already-exists` catch (PersonaUser ~line 2005): legitimate narrow create-or-update pattern. Catches broadly though; should narrow to NotFound-only when ORM exposes typed errors.
- Shutdown-path catches (PersonaUser ~lines 2200+): workspace cleanup, event-unsub. Defensible noise reduction during teardown; low priority.

---

### Coordination with airc (peer's lane)

- airc PR #1083 (ReqwestGhClient, Sub-2): merged. 525ms → 389ms gh API cost (1.47x measured).
- airc PR #1084 (Phase 1.C, send-side SQLite WAL + dedup): in flight. 3.56-3.71 ms/op → 2.01-1.87 ms/op = 1.77-1.98x measured.
- Continuum-side dual-write shim deletion (system/airc-chat/* + airc_admission.rs) waits for airc 1.C boundary.
- 15p continuum real-workload validation owed to peer once continuum stack boots again.

---

## 2026-05-29 — Commands surface audit (pre-PR survey)

Survey to map the migration target before doing it. Joel 2026-05-29:
"commands are composed of commands and most code operations are tool/command
calls. We look at these as kernel level codes we find reuse. They use each
other and the system uses them as well... there needs to be a tool/command
executors. Literally all of those commands are made available as tool calls
for both the ux and the personas or you over jtag cliq."

### Surface inventory

- **53** top-level command directories under `src/commands/`.
- **100** generator specs under `src/generator/specs/`. Some specs lack matching command directories (spec-without-impl); some commands lack matching specs (hand-authored before generator existed).
- **~15** Rust modules with `command_prefixes` (in `continuum-core/src/modules/*.rs` and `continuum-core/src/runtime/*.rs`): code, avatar, logger, cognition, channel, persona_allocator, embedding, events, health, pressure_broker, persona service_module, plus the runtime layer.
- **~15** Rust IPC mixins (`continuum-core/bindings/modules/*.ts`): base, sentinel, system_resources, tool_parsing, gpu, search, inference, plasticity, rag, voice, dataset, avatar, runtime, cognition, code.

### The unification ALREADY exists

The universal executor is in place. Three caller shapes funnel into it:

```
LLM tool call → AgentToolExecutor (TS — format parsing)
              → ToolRegistry.executeTool()
              → Commands.execute(toolName, params)  ← universal primitive
              → Rust CommandExecutor (Rust module registry OR TS via Unix socket)

UI command → Commands.execute(name, params) → same Rust CommandExecutor

jtag CLI → Commands.execute → same Rust CommandExecutor
```

`ToolRegistry.executeTool` line 600 in its docstring explicitly says: "This is the 'adapter' the user mentioned - ONE function that can execute ANY command." Line 664 dispatches: `await Commands.execute(toolName, commandParams)`.

Rust `command_executor.rs` lines 49–61: tries the Rust ModuleRegistry first, routes to TS via `/tmp/jtag-command-router.sock` if the command isn't Rust-implemented.

### Grid composability (Joel 2026-05-29 follow-up)

Commands aren't just composable within ONE process — they compose across the
GRID via airc. The executor needs to be able to dispatch a command to a peer
node and get the result back (airc's ack/promises/async machinery is for this).

Implications:
- A persona running on the MacBook Air can invoke `inference/generate` and have
  it execute on the 5090 box, returning the result over airc. The persona
  doesn't care where it ran.
- The 3x1080ti box hosts training. The 5090 hosts heavy inference. The 970 can
  host smaller models. The MacBook Air can dispatch + consume but rarely
  computes.
- **Pure-Rust commands work on any node.** TS-locked commands work only on
  nodes with nodejs. This is THE reason the migration matters — it unlocks
  every node form (headless 970, Raspberry Pi, AR headset compute, friend's
  machine) to participate.
- The current `command_executor.rs` routes Rust-vs-TS via Unix socket. The
  grid extension routes local-vs-remote via airc. The shape is the same — a
  dispatcher that picks the right backend.

### So what's the migration target?

Not "build the unified executor." It's already built (locally). Grid-extension
of it is the next architectural piece (likely peer's lane via airc). The TS-side
migration targets:

1. **Push more command implementations into Rust.** The ~15 Rust modules cover infrastructure (code, gpu, embedding, etc.) but persona-shaped concerns (cognition gates, training-signal classification, response generation) are still TS-implemented at the *body* of each command, even though the Rust path can route to them.

2. **Find commands whose TS implementation IS the duplication.** A persona's cognition decision shouldn't have an LLM-tool-call form and a UI-command form with different logic — they should both invoke the same Rust function. Any TS file that's doing cognition work IS that duplication.

3. **Find the spec-without-impl set.** 100 specs vs 53 command dirs and ~15 Rust modules. Some commands are aspirational; some are TS-only. Each one's classification (per the 9 categories) tells us delete vs keep vs migrate.

4. **Audit `ToolRegistry.executeBuiltInTool` for what bypasses Commands.execute.** Built-in tools at line 611 short-circuit the universal dispatcher. Each built-in is suspect — if a tool is universal-ish, it should be a command. If it's truly meta (introspection of the tool set, e.g., `search_tools`), built-in is correct.

5. **PersonaToolExecutor's persona-specific pre/post processing** (workspace bootstrap, media collection, cognition logging, sentinel auto-config) is core-shaped TS. Migration target: move into Rust, then the TS-side becomes the LLM-format-parsing shim and nothing else.

### Decisions for the next PR

The next PR is **per-spec triage**, not "delete things." For each command:
- Has a Rust implementation? → TS-side is the form-adapter only, no logic.
- Has only TS implementation? → Is the work core-shaped (migrate) or form-shaped (keep)?
- Has only a spec, no implementation? → Decide: implement Rust-side, or delete the spec.

Pace: write up findings as I survey, merge piece by piece. Don't try to do all 100 at once.

### Anomaly noted, not addressed

`ToolRegistry.executeTool` line 638: `parsedParams[key] = value; // Fallback to string`. JSON.parse fails on a complex-type param → stash raw string. This is type-coercion tolerance (under-typed input), not Joel's drifting-fallback pattern. Keep.

---

## 2026-05-29 — Commands triage (slice 1)

First per-command classification slice. Pace: small, focused, document the
decision per command. No bulk action — each command gets thought.

### Per-command inventory snapshot

(`/tmp/cmd_survey.txt` — 52 top-level command dirs surveyed.)

Top by LOC:
| Command | LOC | Has spec | Has Rust handler |
|---|---|---|---|
| ai | 15,538 | ✓ | ✓ |
| genome | 10,074 | ✓ | ✓ |
| development | 9,829 | ✓ | ✓ |
| interface | 8,602 | ✓ | ✓ |
| collaboration | 8,453 | ✗ | ✓ |
| data | 4,736 | ✗ | ✓ |
| social | 4,436 | ✗ | ✗ |
| sentinel | 3,512 | ✓ | ✓ |
| code | 3,197 | ✓ | ✓ |
| workspace | 3,016 | ✓ | ✓ |

"No spec, no Rust" set (~16 commands totaling ~14 kLOC) is the next bulk
target — but each gets individual triage rather than mass action.

### Slice 1 commands triaged

#### `ping` (398 LOC, no spec, no Rust handler) — partial action

**Classification:** **#8 — core-shaped TS that should migrate eventually**, but the work is split:
- Server info collection (process stats, runtime) — **core-shaped**, Rust target.
- AI status composition (calls `ai/status` command) — **composition example**, the right shape; should be Rust-callable too.
- Browser info collection — **form-specific**, lives in the web form's implementation; absent for jtag CLI / VR / headless.

**Action taken this slice:** killed an aiStatus all-zeros fallback. The previous catch handler caught any failure of the `ai/status` composition and substituted a synthesized `{ total: 0, healthy: 0, starting: 0, degraded: 0, dead: 0 }` object — i.e., LIED that there were zero AI personas when actually the check itself had failed. Now: if the composition fails, `aiStatus` stays undefined; the caller sees no field and knows the check didn't run.

**Deferred for migration PR:** Rust-implement the server-info + ai-status-composition path. Browser collection stays form-specific.

**Architectural note:** Line 32 — `commandDaemon.commands.get('ai/status')` direct map access (cast hack) instead of `Commands.execute('ai/status', ...)`. Comment retained explaining the same-process-IPC-roundtrip avoidance. When the Rust executor matures, intra-process command composition should be a first-class API, not a map-cast.

#### `help` (461 LOC, no spec, no Rust handler) — classify, defer

**Classification:** **#4/#8 hybrid** — currently filesystem-introspection of the TS command tree on disk. The COMMAND is universal (every form should be able to get help) but the CURRENT implementation reads `src/commands/*/README.md` files from disk, which is intrinsically TS-form (those files only exist in the TS repo layout).

**Right shape long-term:** the command registry (Rust ModuleRegistry today; eventually a unified runtime registry) should expose `describe` introspection. `help` becomes a thin wrapper that queries the registry for command names + their declared descriptions. Then any form gets help symmetrically.

**Action this slice:** none. Classification recorded. Migration target = "registry-introspection-based help" but only meaningful after more commands are Rust-registered.

#### `social` (4,436 LOC commands + ~1,500 LOC support layer) — DROPPED

**Classification:** **deferred → dropped on direct call.** Joel 2026-05-29: "Don't worry about social. Drop it."

**Action taken this slice:** Full cascade delete. Joel's "drop it" applied to the entire concept, not just the command directory — the support layer that exists only to feed those commands also has no purpose without them.

Deleted:
- `src/commands/social/` (full directory — 14 sub-command surfaces × {browser, server, shared, test} layouts)
- `src/system/social/` (`SocialCommandHelper`, `SocialMediaProviderRegistry`, `ISocialMediaProvider`, `SocialCredentialEntity`, `SocialMediaTypes`, `MoltbookProvider`)
- `src/system/rag/sources/SocialMediaRAGSource.ts` (the "social media HUD" RAG injection for personas — Priority 55 entry in ChatRAGBuilder)

Patched out of:
- `src/system/rag/builders/ChatRAGBuilder.ts` — removed import + `new SocialMediaRAGSource()` from the source chain
- `src/system/rag/sources/index.ts` — removed export
- `src/daemons/data-daemon/server/EntityRegistry.ts` — removed `SocialCredentialEntity` import, instantiation, and `registerEntity` call
- `src/generator/generate-collection-constants.ts` — removed `system/social/shared/*Entity.ts` from the entity-discovery globs

Regenerated:
- `src/server/generated.ts` + `src/browser/generated.ts` via `npx tsx src/generator/generate-structure.ts` — went from 351 to 343 commands

**Net delete:** ≈ 5,800+ LOC of TS surface across 100+ files. TS still compiles clean (the 6 pre-existing `Cannot find module '../config'` errors remain unchanged).

**Note on the broader principle:** the social subsystem is also a worked example of why TS-locked commands are dangerous — it consumed RAG priority on every persona's context, even though no production form was actively exercising it. The cost was carried by every persona, every message, in TS time. With it gone, the persona context becomes cleaner AND the kloc drops.

---

## 2026-05-29 — Commands triage (slice 2)

Four small no-spec-no-Rust commands triaged. No code changes — the classifications are the value; future-me and peer reading this know what each is and what its migration shape is.

#### `indicator` (153 LOC) — KEEP

**Classification:** #4 (form-specific implementation of a universal command).

Server emits a console.log line with a type icon, then delegates to the browser via `remoteExecute(params)`. Browser presumably creates a visual DOM notification (toast). Per-form impl is correct: CLI/jtag form prints to terminal, web form renders a UI element, VR/AR form would render a 3D-world notification, headless form may no-op or log.

**Note:** when a persona uses `indicator` as a tool call, the indicator surfaces in whatever form the user is currently inhabiting (web/VR/AR). That's the Tron-citizen materializing in the user's room.

#### `positron/cursor` (192 LOC) — KEEP, future reorg suggested

**Classification:** #4 (form-specific implementation of a universal command).

"Enables AIs to point, highlight, and draw attention to elements in the UI. The cursor is the AI's 'hand' - its spatial presence in the interface." Server delegates to browser; browser draws DOM overlay (circle/rectangle/arrow/underline) at coordinates or selector.

**Reorg note** (per organization-purity doctrine): `positron/` has only one child (`cursor`). The cursor concept fits under `interface/` (which already has click, screenshot, scroll, type, navigate, etc. — all UI presence commands). Future move: `positron/cursor/` → `interface/cursor/`. Not in this slice — would cascade through generated.ts, command constants, DocumentationSource references. Tracked here for when it's the right opportunity.

#### `list` (492 LOC) — DEFER MIGRATE

**Classification:** #4/#8 hybrid.

Currently reads `src/scripts/generate-command-schemas.ts` output from disk (TS-form filesystem introspection). The CONCEPT is universal (any caller asks "what commands exist?"), but the IMPLEMENTATION reads files specific to the TS form's layout.

**Right shape long-term:** the Rust ModuleRegistry exposes introspection. `list` becomes a thin wrapper that queries the registry. Then any form (web UI, jtag CLI, VR persona, headless grid node) gets the same enumeration via the same path.

**Migration target:** post-grid-extension of ModuleRegistry. Defer until enough commands are Rust-registered that registry-introspection is meaningful.

#### `recipe` (515 LOC) — DEFER MIGRATE

**Classification:** #8 (core-shaped TS that should migrate), gated on room-is-airc embed.

`recipe/run` loads a recipe by uniqueId, resolves template, validates model availability via RecipeAssembler, dispatches to `sentinel/run` with the resolved template. The TS body is mostly orchestration — composing other commands.

Joel 2026-05-29: "Recipes create rooms — `airc.join('<recipe-id>')` materializes a room on demand, room doctrine system at `Airc::room_doctrine` carries the per-recipe behavior."

**Right shape:** recipe/run becomes a Rust command that:
1. `airc.join(recipe.uniqueId)` — materializes the airc room for this recipe
2. Loads recipe definition (likely from `#settings` per peer's 1224aac2 card)
3. Attaches the recipe's roleId-mapped personas as airc peers in the room
4. Dispatches to sentinel orchestration (also moving to Rust)

**Migration target:** gated on (a) airc#1075 ConsumerAdapter merge unblocking continuum-core's airc::embed, (b) airc room creation API stabilized, (c) #settings room (1224aac2) for recipe definition storage. Once those three land, the whole recipe-run orchestration moves to Rust in one slice.

### Open questions for follow-up slices

- The "no spec, no Rust" set totals ~14 kLOC. Going slice-by-slice (3–5 commands at a time) is the survivable pace.
- The "has spec, no Rust" set (e.g., `model`, `state`, `dev`, `claude`, `logging`) means the generator produced TS-side scaffolding but the Rust impl was never written. Each is a candidate for Rust implementation OR for spec deletion (if the command shouldn't exist).
- Several big "has Rust" commands (`ai`, `genome`, `development`) probably have substantial TS bodies *on top of* the Rust path. Worth checking if those TS bodies duplicate Rust logic.

---

## 2026-05-29 — Chat-message-flow migration scope (gated on airc e51ab14e)

Airc PR #1084 (Phase 1.C — chat substrate throughput 281→498 msg/s) merged. I committed to peer that I'd start the continuum-side dual-write shim deletion against that release boundary. **Correction after surveying: the shim deletion is the front of a much bigger migration**, gated on **airc card e51ab14e (machine-singular daemon)**, not on Phase 1.C. Documenting the full scope now so the slice is peer-reviewable and ready to execute when e51ab14e lands.

### Today's dual-write architecture

```
ChatSendServerCommand (commands/collaboration/chat/send/server/)
  └→ AircChatDualWriteService (system/airc-chat/server/)
      ├→ AircChatPublisher → publishes to airc room
      └→ AircToORMMirrorWriter → writes ChatMessageEntity to local ORM
```

The TS shim (`system/airc-chat/` — 1069 LOC: publisher, dual-write service, mirror writer, mapper, types, envelope builder + 4 test files) is just the write side. The mirror entity is then READ by many continuum-side consumers from the local ORM, which means deleting only the writer leaves readers reading silently-stale data — exactly the silent-fallback pattern the doctrine forbids.

### ChatMessageEntity readers (the actual migration surface)

| Reader | Purpose | Migration target |
|---|---|---|
| `PersonaUser.catchUpOnRecentMessages` (~line 1232) | Startup catch-up on missed messages per room | Airc room history query at startup; result shape matches today's ORM query |
| `PersonaUser.handleChatMessage` (downstream of catch-up) | Process backlog message | Same handler, fed from airc subscription instead of ORM read |
| `TrainingDaemonServer` (line ~233) | Capture chat for training data | Airc room subscription buffered into training pipeline; or read from airc history when training run starts |
| `ToolRegistry` chat-message handling | Tool call embedding/extraction from chat | Read from airc room (likely already form-specific since tools see chat from inside the room) |
| `RoomActivityBatch` (system/user/server/attention/) | Batch room activity for attention/presence | Airc presence + room event subscription, not ORM query |
| Generated bindings (`RecentMessage`, `ToolOutcome`, `MediaItemLite`) | ts-rs-emitted types | Stay typed; airc envelope content is structurally compatible. Regenerate once Rust-side airc message types stabilize |

### Why this is gated on e51ab14e

Without machine-singular daemon, multiple personas on one box are different airc peers in different process scopes. They can each publish to a shared room but **don't see each other's writes live** — only at point-in-time queries against the coordinator store. So:

- A persona enrolled in `general` writes its response to airc
- The other 14 personas don't see that response in real time
- They only see it when something triggers a point-in-time history query
- Result: the 15-persona scenario looks like turn-based correspondence, not a live room

With e51ab14e (one daemon per machine-account), all personas on Joel's box share one airc daemon bus, live delivery works across processes, the scenario actually works.

### Migration sequencing (when e51ab14e lands)

1. **Subscribe** — wire each ChatMessageEntity reader to an airc room subscription instead of ORM polling. Additive: readers see both the airc subscription AND the dual-write ORM data; behaviors should be identical.
2. **Verify** — run the 15-persona general-room scenario, confirm subscription-based reads match dual-write reads.
3. **Stop dual-writing** — `ChatSendServerCommand` calls `AircChatPublisher` directly, no `AircToORMMirrorWriter`. ORM mirror stops being written; readers (now subscription-based) don't care.
4. **Delete the shim** — `system/airc-chat/` (1069 LOC TS).
5. **Verify CHAT_MESSAGES collection is unwritten** — if nothing writes to it, the collection is dead. Delete the entity + remove from EntityRegistry.
6. **Bench** — measure continuum-side throughput against substrate's Phase 1.C 498 msg/s baseline. If continuum-side flow doesn't keep up, that's a fresh bottleneck to find.

### NOT the shim

- The Rust `airc_admission.rs` in `continuum-core/src/persona/` is **NOT** the dual-write shim. It's the memory admission path that converts a signed airc envelope into an AdmissionCandidate for persona memory. Stays.
- WebRTC SDP / MediaSignaling handling — likely already on the airc side; verify when wiring the live multi-persona test.
- Theme / room presentation — independent of chat-message migration; web form's concern, no substrate change needed.

### Pre-work I can do without blockers

- Each ChatMessageEntity reader's subscription-shape sketch (what `airc_subscribe` call replaces what `ORM.query`).
- Bench harness for the 15-persona scenario (compile-time even if can't run yet).
- Cleanup of any silent-fallback patterns in the readers (`catch { return [] }` etc.) — independent doctrine work.

Surfaces as separate slices as I get to them.
