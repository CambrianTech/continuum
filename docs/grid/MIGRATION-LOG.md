# Migration Log — TS → Rust Persona Surface

Tracks per-module decisions in the migration from TS-coupled persona infrastructure to a pure-Rust core. Pace is small, focused, merge-as-we-go (Joel 2026-05-29: "We will want to write down a lot in migration docs as we got and keep merging, piece by piece").

## Doctrine (Joel 2026-05-29)

- **No fallbacks.** Drifting two-path decision logic is the most dangerous pattern.
- **No amateur heuristics on first-class citizens.** Substring matching, magic-number arithmetic, time-decay throttling — all violate the citizen-of-continuum framing.
- **TS is widgets + config UX**, one interface among many. Pure-Rust forms must exist (AR, headless grid persona on a 970, OpenClaw).
- **Commands are kernel-level**, compose, used by clients AND the system itself. Rust-implemented, ts-rs-bound, generator-authored.
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
