# Persona Cognition Rust Migration

> **Every cognition PR ships net-negative TypeScript lines under `src/system/user/server/`. No exceptions.** This is the enforceable gate that prevents the persona-cognition footprint from continuing to sprawl in Node while we wait for "the right time" to migrate. The right time is every PR.

Status: active migration policy — updated 2026-05-11. Authored after Joel observed that even the shared-cognition work I'd planned (modify `PersonaResponseGenerator.ts` to call into Rust) would preserve the TS cognition layer with a Rust dependency grafted on — defeating the principles we'd just spent the morning establishing (Rust = logic, TS = schema-only thin shim, CBAR-style native truth + thin SDKs). The right answer: build it in Rust, shrink or delete the TS counterpart, gate every PR on TS line-count drop.

---

## The problem in numbers

`src/system/user/server/` today: **~27,864 lines** of TypeScript persona cognition. Across:

- `PersonaResponseGenerator.ts` — the main cognition orchestrator
- `PersonaAgentLoop.ts`
- `PersonaCognitionEngine.ts`
- `PersonaPromptAssembler.ts`
- `PersonaResponseValidator.ts`
- `PersonaEngagementDecider.ts`
- `PersonaMessageEvaluator.ts`
- `PersonaInbox.ts`
- `PersonaAutonomousLoop.ts`
- `PersonaGenome.ts` / `PersonaGenomeManager.ts`
- `PersonaLogger.ts`, `PersonaSubprocess.ts`, `PersonaToolDefinitions.ts`, `PersonaToolExecutor.ts`
- `PersonaTrainingManager.ts`, `PersonaMediaConfig.ts`, `PersonaModelManager.ts`
- `LongTermMemoryStore.ts` and the `cognition/`, `cognitive/`, `consciousness/`, `central-nervous-system/`, `being/`, `modules/` subdirs
- `ChatRAGBuilder.ts` (RAG context for chat — also touched)
- `ComplexityDetector.ts`, `GapDetector.ts`, `ContentDeduplicator.ts`, `LoRAAdapter.ts`, `MemoryTypes.ts`

Every one of these is a verb-shaped module — algorithm, scoring, orchestration, decision-making, consolidation. Per Joel's sharpened rule (`feedback_rust_first_sharpened.md`): every one of these belongs in Rust. `Service`, `Engine`, `Coordinator`, `Evaluator`, `Analyzer`, `Manager`, `Detector`, `Generator`, `Validator`, `Decider` are all verb suffixes; the `.ts` extension on a verb is itself the negative signal.

## Why the sprawl happened

Historically: every new cognitive capability got its first draft in TS because TS iteration is fast (no cargo build). The drafts never migrated. Each Claude session, each pair-programming sprint, each "let's try X" experiment left another `.ts` file behind. Nobody removed them. The footprint grew monotonically.

The pattern that has to break: **TS is no longer the iteration language for cognition.** Even fast-iteration cognitive prototypes go in Rust. cargo's incremental build is fast enough; the type system catches more bugs than TS does; the resulting code is faster, more concurrent, and ready for the wrappable-from-anywhere architecture (Unity, AR/VR, native iOS/Android — all CBAR's lineage).

## The two-pronged fix

## 2026-05-11 Hardening: No Compromise Rust-First Rule

This migration is now the default engineering standard, not a preference.

Agents should not ask whether cognition belongs in Rust. It does. The only design question is which Rust boundary owns it and which tests prove it.

Rules:

1. **No new TS cognition behavior.** New behavior under persona cognition, prompt/RAG decisions, tool parsing/execution, model selection, memory consolidation, turn batching, or inference scheduling must be Rust-first.
2. **No duplicate owners.** If Rust takes over a behavior, remove or shrink the TS implementation in the same PR. #1068 and #1069 are the current pattern.
3. **No "temporary" fallbacks that hide failure.** Rust can return typed `Unavailable`, `Degraded`, or `Backpressured` states. TS may display them. TS must not silently pick another model/provider/path.
4. **No swallowed command failures.** Commands are dynamically generated and executed by callers that own error handling. Inner execution loops should return errors, not catch-and-convert them into false success.
5. **Tests are architectural evidence.** A Rust unit/replay test should prove the boundary. A live chat smoke test proves integration only after the Rust test exists.
6. **Major rework is acceptable.** When the boundary is wrong, preserve the user contract and rewrite the internal contract. Small compatibility patches that keep the wrong owner are technical debt.

Current canary examples:

- **#1068** moved persona turn fixture recording into Rust and removed the duplicate TS writer.
- **#1069** moved leaked tool/thinking markup cleanup into Rust and removed the duplicate TS sanitizer.

Those are small examples of the rule. The same pattern must now be applied to the large remaining owners: inbox consolidation, ChatRAGBuilder, tool execution, prompt turn assembly, memory consolidation, and model/provider selection.

## The two-pronged fix

### Defensive (every PR going forward)

**No new persona cognition `.ts` files.** Period.

If you need new cognitive capability, the Rust module goes in `continuum-core/src/cognition/`, `continuum-core/src/persona/`, or a focused submodule. The TS layer is the IPC mixin in `bindings/modules/<area>.ts` (snake_case → camelCase wrapper, no logic) plus generated types via `#[derive(TS)]` plus generated command scaffolds.

**Concrete check before any new `.ts` file is added under `src/system/user/server/`:**

1. Is this a verb (algorithm, scoring, orchestration, decision)? → Rust.
2. Is this a noun (data shape stored via ORM)? → TS via decorators is fine.
3. Is this a thin wrapper that calls into Rust? → TS shim is fine but should be ≤100 lines.

If the answer to (1) is yes and you're writing TS, you're in the wrong language. Stop, write the Rust.

### Offensive (continuous shrinking)

**Every cognition PR ships net-negative TS lines under `src/system/user/server/`.**

This is the merge gate. Not a soft "we'll get to it." A measurable test: before merge, run `find src/system/user/server -name '*.ts' | xargs wc -l | tail -1` and confirm the total dropped relative to main. If it grew or stayed flat, the PR isn't done; either pull more existing TS into Rust as part of the PR, or it doesn't merge.

This forces every PR to do at least a small piece of the migration, even if the PR's primary purpose is something else. It compounds: 50 PRs over a few months, each shaving 100-500 lines, eliminates the ~28k footprint without ever needing a "big migration" sprint.

## Migration ladder

Each rung is a discrete PR. Each rung's primary deliverable is a Rust module that absorbs functionality from a specific TS file (or set of files), with the TS file shrinking to a thin shim or deleting outright.

### Rung 1 — `PersonaResponseGenerator` → `persona/response.rs`

Currently the shared-cognition PR (this commit's parent). PRG owns: assemble-prompt logic, inference orchestration, slot/budget/timing pipeline, post-processing. All of it moves to Rust. `PersonaResponseGenerator.ts` shrinks to a ~50-line shim that calls `Commands.execute('persona/respond', {...})` or deletes outright.

**Acceptance:** `wc -l src/system/user/server/modules/PersonaResponseGenerator.ts` → < 100 lines (down from >900).

### Rung 2 — Hippocampus → `cognition/hippocampus.rs` (`LongTermMemoryStore` + consolidation)

The brain-design ladder Joel called for. Working memory → hippocampus consolidation → long-term semantic memory. Continuous low-priority pass that doesn't choke chat path. CBARFrame adaptive cadence (quarter-fidelity when chat hot, full-fidelity during quiet). `LongTermMemoryStore.ts` and the consolidation pipeline move to Rust; the SHARED-COGNITION.md A.6 event surface is what feeds it.

**Acceptance:** `LongTermMemoryStore.ts` deletes; `cognition/`, `cognitive/`, `consciousness/`, `central-nervous-system/` subdirs lose ≥50% of their TS lines.

### Rung 3 — `PersonaCognitionEngine` → `persona/cognition_engine.rs`

The decision/scoring core. Priority calculation, fast-path decisions, full-evaluate gates. Some of this already exists in Rust (`continuum-core/src/persona/cognition.rs`); finish the migration so the TS class is a shim or deletion.

**Acceptance:** `PersonaCognitionEngine.ts` < 100 lines or deleted.

### Rung 4 — `PersonaAgentLoop` + `PersonaAutonomousLoop` → `persona/loops.rs`

The continuous tick + autonomous task generation. Already partial in Rust. Finish.

**Acceptance:** Both `.ts` files < 100 lines or deleted.

### Rung 5 — `being/`, `central-nervous-system/`, `consciousness/` subdirs → Rust modules

These directories carry the deeper neural-architecture experiments. Each subdir gets its own Rust module sibling under `continuum-core/src/persona/being/`, `continuum-core/src/persona/cns/`, etc.

**Acceptance:** All three subdirs delete their TS contents.

### Rung 6 — `ChatRAGBuilder` → `rag/chat_builder.rs`

Existing `continuum-core/src/rag/` already has the engine and budget. Finish by absorbing the TS chat-builder into the Rust RAG.

**Acceptance:** `ChatRAGBuilder.ts` < 100 lines or deleted.

### Rung 7 — Persona modules cleanup

`PersonaPromptAssembler`, `PersonaResponseValidator`, `PersonaEngagementDecider`, `PersonaMessageEvaluator`, `ComplexityDetector`, `GapDetector`, `ContentDeduplicator`, `LoRAAdapter` — each migrates to a sibling Rust module. The PRs can batch related ones (e.g. all the pre-response decision modules in one PR).

**Acceptance:** Net-negative TS lines per PR; full sweep when the directory only contains thin shims + nouns.

## What stays in TypeScript

These are the legitimate TS layers (Joel's noun/verb split):

- **ORM entity definitions** — `ChatMessageEntity`, `RoomEntity`, `UserStateEntity`, etc. Defined via decorators in TS. Nouns, not verbs. Stay.
- **Command scaffolds** — generated by `CommandGenerator` from JSON specs. Stay (they're effectively schema).
- **TS IPC mixins** — `bindings/modules/<area>.ts`. Pure wrappers, no logic. Stay.
- **Browser widgets** — Lit components. UI rendering, not cognition. Stay.
- **Thin shims that route Rust** — e.g. the eventual ~50-line `PersonaResponseGenerator.ts` that just does `Commands.execute('persona/respond', ...)`. Stay (briefly), then delete when no consumer needs the TS-side facade.
- **JTAG client routing** — the dispatcher that fans out to Rust IPC, browser widgets, and other TS daemons. Stays as the integration glue.

## Acceptance gate (the test that runs on every cognition PR)

```bash
# Before merge, on the PR branch:
LINES_NEW=$(find src/system/user/server -name '*.ts' | xargs wc -l | tail -1 | awk '{print $1}')
git stash && git checkout main
LINES_OLD=$(find src/system/user/server -name '*.ts' | xargs wc -l | tail -1 | awk '{print $1}')
git checkout - && git stash pop
echo "TS persona cognition lines: ${LINES_OLD} → ${LINES_NEW}"
[ "$LINES_NEW" -lt "$LINES_OLD" ] || { echo "PR grew TS persona cognition. NOT MERGEABLE."; exit 1; }
```

This script (or a CI variant) is the gate. If a PR claims to be cognition work but doesn't drop the TS line count, it isn't doing the migration — it's accruing more debt to migrate later. Reject.

Exception: PRs that are purely TS-noun work (new ORM entity, new ContentItem field) are exempt — they don't touch verbs. The gate applies to PRs that touch persona cognition behavior.

## Why this matters beyond just "TS is slow"

1. **Concurrency is the difference between a mind and a machine.** Joel: *"obv cognition is the best place for concurrency. that's the difference between a mind and a machine."* A machine is sequential — request, response, return. A mind is many concurrent processes — perception running, reflection running, planning running, autonomic functions running, all at once, at different rates, sometimes interrupting each other, sometimes deferring to each other. Modeling a mind means having the primitives that support that: real threads, atomics, memory fencing, lock-free structures, predictable scheduling. Node has none of these as first-class — it has a single event loop and GC pauses. Rust is the language we have that does. **Cognition specifically — more than any other layer — has to be in Rust, because cognition specifically is where the mind/machine line gets drawn.**

2. **CBAR lineage.** The Rust core gets wrapped in Unity/AR-VR/iOS/Android per Joel's roadmap. Every line of cognition in TS is a line that has to be re-implemented in each future client OR shimmed through Node-in-Unity (a disaster). Rust = write once, wrap everywhere.

3. **Consumer hardware demands obsessive efficiency.** Joel: "we gotta be insanely efficient to deal with our hardware limitations. Node is garbage." Single-threaded event loop, GC pauses, no SIMD, no zero-copy. Rust eliminates all of these. The local-AI value prop only exists if we can squeeze the last cycle out of a MacBook.

4. **The Cambrian-explosion strategy depends on this.** The puddles-and-streams thesis (mass distributed grids beating centralized monoliths) needs each grid to be performant enough to actually run the cognition pipeline at conversation cadence. Slow cognition means each grid is a frustrating toy; fast cognition means each grid is a real participant in distributed intelligence.

5. **The architecture has to support its own evolution.** "Let's really design a brain, as best we can." A brain that runs continuously at adaptive engagement levels (CBARFrame-style) needs the host language to give us the primitives. TS gives us a single thread and GC pauses. Rust gives us the brain.

## The migration pattern (Joel's playbook)

Design the elegant architecture. Start migrating with the FEATURE you're currently shipping. Once the pattern is built, migrate the rest. The whole thing usually takes less time than people expect because the pattern repeats — once you've moved one PRG-shaped TS cognition module to Rust correctly, every subsequent migration is "apply the pattern" rather than "design and apply."

This is why doing the PRG migration as part of the shared-cognition PR is the right move: shared cognition IS the current feature. The pattern that gets established here (Rust verb + ts-rs types + IPC mixin + thin TS shim) is the pattern every subsequent rung uses. By the time we're at Rung 5 (`being/`/`cns/`/`consciousness/` subdirs), each migration is a half-day of repeating the established pattern, not a multi-day architecture exercise.

## What to do RIGHT NOW (this PR, the shared cognition one)

1. **Don't grow TS lines.** The Rust modules I've already shipped (`cognition/types.rs`, `cognition/shared_analysis.rs`, `cognition/response_orchestrator.rs`) are zero-TS-line additions to `src/system/user/server/`.

2. **A.3 reshaped (currently in progress):** Build `persona/response.rs` in Rust as PRG's cognition core. Shrink `PersonaResponseGenerator.ts` to a thin shim. Net-negative TS lines is the merge gate.

3. **Memento parallelizes:** `PersonaPromptAssembler.ts` → Rust as a focused slice that integrates with my `persona/response.rs`. His PR (or commit) should also show net-negative TS.

4. **Chat-validate end-to-end** (Joel's gate from yesterday). Real local persona response, measured.

5. **Merge with both gates passing:** chat-validates AND TS-line-count drops.

After this PR, every subsequent cognition PR follows the same gate. The migration ladder shrinks the footprint continuously.

## Provenance

- Joel observed (2026-04-19) that even the shared-cognition work I'd planned would preserve TS cognition with a Rust dependency grafted on, defeating the principles we'd just established.
- Sharpened the rust-first rule earlier the same morning into "Rust = logic, TS = schema, never logic" and the corollary "cognitive code with .ts extension makes Joel nervous."
- This doc is the operational answer: the principle is enforced by a measurable gate on every PR, not left as good intent.
- Connects to: `RESOURCE-ARCHITECTURE.md` (paging primitive that Rust enables), `SHARED-COGNITION.md` (the cognitive architecture this is the migration plan for), `feedback_rust_first_sharpened.md` (memory file with the principle).
