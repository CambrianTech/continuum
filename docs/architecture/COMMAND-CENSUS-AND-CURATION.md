# Command Census & Curation Verdict

> Companion to [COMMAND-ORGANIZATION.md](COMMAND-ORGANIZATION.md). That doc says *how* a command
> is structured (one typed `ActionCommand` per file, path mirrors name, self-registering). This doc
> is the **census of every legacy command** (~253 across ~45 `handle_command` modules) and a
> **per-command verdict** taken before the bulk migration: *is it needed; are its params/return
> right; is its namespace right.* The collapse is not a 1:1 port — it is a curation pass.

## How to read a verdict

| Verdict | Meaning |
|---|---|
| **KEEP** | Port faithfully. Contract is right. |
| **RETYPE** | Port, but replace `Value`/manual-JSON params with a typed `Params` struct (+ `JsonSchema`). |
| **INTERNAL** | Port, but `access: Internal` — present in the registry for testing/introspection, **not** on the persona tool surface and **not** grid-reachable. These are substrate seams the kernel calls in-process; no external caller should. |
| **DELETE** | Do not migrate. Dead (TS-era sync, stub) or a condemned heuristic. |
| **MERGE→x** | Fold into command `x`; delete this name. |
| **RENAME→x** / **MOVE→x** | Same logic, corrected wire name / namespace. |

Access levels on what survives: **AiSafe** (persona may call — reads, composition, its own memory/work), **Privileged** (Trusted-tier writes — training, file/shell, consolidation), **Internal** (kernel-only seams), **Owner** (local operator only — trust, grants, destructive infra).

---

## The seven structural findings (the consequential decisions)

These are the cross-cutting calls that change *what gets built*, not just *how*. Detail per command is in the cluster tables below.

### 1. `cognition/*` is not a command namespace — it is the substrate's internals leaking onto the wire
Of the 50 `cognition/*` arms, ~30 exist **only** because TypeScript used to drive the cognitive cycle step-by-step over IPC (`calculate-priority` → `fast-path-decision` → `should-respond` → `generate-response` → `validate-response` → …). Under the headless Rust `WorkspaceCycle` the kernel runs these in-process; no persona, `uu`, or grid peer should call them individually. **Verdict: the real entry points stay AiSafe (`cognition/respond`, `persona/turn-execute`, `inbox/drain-frame`); the per-step gates become `Internal`; the heuristic helpers are DELETED (finding #2).** This single finding removes ~30 commands from the persona tool surface — which is the *correct* shape, not a regression. "Available by default" never meant "expose the cycle's guts."

### 2. The heuristic helpers are already condemned by standing doctrine — DELETE, don't migrate
`cognition/calculate-priority`, `cognition/fast-path-decision`, `cognition/classify-domain` (keyword table!), `cognition/text-similarity`, `cognition/check-semantic-loop`, `cognition/check-mentions`, `cognition/register-domain-keywords`, `cognition/sync-domain-classifier` are exactly the *"hardcoded heuristics to steer cognition"* the memory `[[no-hardcoded-heuristics-to-steer-cognition]]` and task **#9** (*"delete calculate_priority + fast_path caste/mention heuristics"*) say to kill. Migrating them would launder dead code onto the new registry. **Verdict: DELETE (8 commands).** Judgment they encode belongs to the genome/judges, not a `match` arm.

### 3. The genome family is split across two namespaces — consolidate under `genome/*`
`cognition/genome-activate-skill`, `genome-sync`, `genome-state`, `genome-evict-under-pressure`, `genome-record-activity`, `genome-coverage-report`, `cognition/gpu-budget`, `cognition/select-model`, `cognition/sync-adapters` live under `cognition/` but are genome/paging concerns; `genome.rs` already owns `genome/job-*` and `genome/training-trigger/*`. **Verdict: MOVE the live ones to `genome/*` (e.g. `genome/activate-skill`, `genome/state`, `genome/evict`, `genome/coverage`); DELETE the TS-sync ones (`sync-adapters`, `genome-sync` as a TS push — the registry is Rust-owned now).** One coherent `genome/` namespace.

### 4. Duplicate shell command families in `code.rs` vs `code_commands.rs`
`code_commands.rs` already migrated `code/shell`, `code/shell-poll`, `code/shell-kill` (handle-based, `ctx.caller` identity). `code.rs` still carries legacy `code/shell-create`, `code/shell-execute`, `code/shell-poll`, `code/shell-kill`, `code/shell-cd`, `code/shell-status`, `code/shell-watch`, `code/shell-sentinel`, `code/shell-destroy` (persona_id in body, spoofable). **Verdict: the migrated handle-based family is canonical; reconcile the richer legacy verbs (`cd`/`watch`/`sentinel`) onto it where still needed, DELETE the spoofable duplicates.** No two shell surfaces.

### 5. Known bypasses and stubs — DELETE now
- `persona/rag-inspect` (+ the `_filesystem` resolver) is the `inspect_persona_rag_with_inference` bypass CLAUDE.md's STOP banner says is being removed. **DELETE** (or, if a glass-box probe is still wanted, keep one `Internal` read-only inspector — not a chained-inference path).
- `health/get-stats` returns `{note: "not yet implemented"}`. **DELETE.**
- `ai/*` TS fallthrough arm forwards unmigrated commands to TypeScript — it is the legacy bridge itself. **DELETE in Wave Z** once `ai/*` is fully ported (its existence is a silent fallback; #40/#41 already forbid it).

### 6. Aliases to collapse (compression principle — one name per decision)
- `data/query` ≡ `data/list` (both → `handle_query`). **Keep `data/list`** (ubiquitous caller name), DELETE `data/query`.
- `inbox/create` ≡ `cognition/create-engine`. **Keep `inbox/create`** as the public verb, make `create-engine` its internal callee or MERGE.
- `sentinel/run` ≡ `sentinel/execute` ≡ `sentinel/pipeline`. **Keep `sentinel/run`**, DELETE the two aliases.
- `health-check` (legacy) vs `ping` (already typed `PingCommand`). **Keep `ping`**, MERGE `health-check` into it (or `health/check` if we want the noun).

### 7. Merges where one command subsumes another
- `adapter/info` already returns capabilities; `adapter/capabilities` is a strict subset. **MERGE→`adapter/info`.**
- `cognition/validate-response` vs `cognition/validate-response-decision` — two validators; one is the structured-decision form. **MERGE** to the structured one, Internal.

**Net effect of curation:** of ~253 legacy commands, roughly **~40 DELETE, ~25 INTERNAL (off the persona surface), ~10 MERGE/RENAME, ~15 MOVE namespace** — leaving **~165 commands** to migrate, of which the persona (AiSafe) surface is ~50–60 *intentional* tools, not the 3 toys today and not a 250-command dump.

---

## Cluster verdicts

### cognition.rs (50) → ~12 KEEP/INTERNAL, ~8 DELETE, ~9 MOVE→genome
| Command | Verdict | Note |
|---|---|---|
| cognition/respond | KEEP (AiSafe) | hot path |
| persona/turn-execute, persona/drain-turn-frame | KEEP (AiSafe) | Lane D full-turn entry |
| inbox/drain-frame | KEEP (AiSafe) | bounded work frame |
| inbox/create | MERGE→create-engine (keep `inbox/create` name) | finding #6 |
| cognition/create-engine | INTERNAL | kernel ensures state |
| cognition/get-state, /genome-state→genome/state | INTERNAL / MOVE | introspection |
| cognition/should-respond, /full-evaluate, /generate-response, /check-redundancy, /check-adequacy, /validate-response-decision, /score-interaction | INTERNAL | cycle seams, RETYPE where Value |
| cognition/validate-response | MERGE→validate-response-decision | finding #7 |
| cognition/calculate-priority, /fast-path-decision, /classify-domain, /text-similarity, /check-semantic-loop, /check-mentions, /sync-domain-classifier, /register-domain-keywords | **DELETE** | finding #2 (heuristics, #9) |
| cognition/clean-response | INTERNAL | output cleanup seam |
| cognition/has-evaluated, /mark-evaluated, /track-response, /cache-message, /check-content-dedup, /record-content | INTERNAL | bookkeeping, not tools |
| cognition/embed-tools, /semantic-search-tools | INTERNAL | tool-RAG seams |
| cognition/vision-describe | KEEP→MOVE `vision/describe` | belongs in vision/ |
| cognition/genome-activate-skill, /genome-sync, /genome-evict-under-pressure, /genome-record-activity, /genome-coverage-report, /gpu-budget, /select-model | MOVE→genome/* | finding #3 |
| cognition/sync-adapters | **DELETE** | TS-push era |
| cognition/set-sleep-mode, /configure-rate-limiter | INTERNAL | runtime config, RETYPE |
| cognition/generate-recipe, /rate-proposals, /plan-turn-batch | KEEP (Privileged) | recipe authoring |
| cognition/admit-inbox-message, /enqueue-message, /recall-engrams | INTERNAL→ reconcile w/ memory/* | recall overlaps memory namespace |

### data.rs (29) → KEEP, RETYPE the untyped, collapse 1 alias, merge 1
- **KEEP (typed):** data/create, read, update, delete, list, queryWithJoin, count, batch, ensure-schema, query-open/next/close.
- **RETYPE (untyped Value today):** data/list-collections, collection-stats, truncate, clear-all, adapter/info, vector/search, vector/index, vector/stats, vector/invalidate-cache, vector/backfill, migration/start…rollback (7).
- **DELETE alias:** data/query (→data/list, finding #6).
- **MERGE:** adapter/capabilities→adapter/info (finding #7).
- **Access:** data/delete, data/truncate, data/clear-all, migration/* → **Owner/Privileged** (destructive/infra). data reads → AiSafe. vector/* → AiSafe (read) / Privileged (index, backfill).

### voice (live.rs, ~23) + channel.rs (7) → KEEP, RETYPE voice, 1 DELETE
- **KEEP:** all voice synth/transcribe/handle/ambient/session verbs (real capability surface); channel/enqueue,dequeue,status,service-cycle,service-cycle-full,tick-config.
- **RETYPE:** voice/* dispatch is untyped `Value` at the module seam — give each a typed `Params` (shapes already exist in `VoiceService`). Verify during port.
- **DELETE:** voice/test-audio-generate (debug-only sine generator — if kept, `Internal`).
- **Access:** voice synth/STT → AiSafe; voice/resource-unload, channel/clear → **Privileged** (destructive); voice snapshot-* → AiSafe (feature-gated).

### code/search/vdd (42) → reconcile shell dup, RETYPE search, KEEP vdd
- **KEEP (already migrated):** code/read,write,edit,list,exists,glob,tree,search; code/git/*; code/cargo/check,test; code/shell,shell-poll,shell-kill.
- **Finding #4 reconcile:** code/shell-create,execute,cd,status,watch,sentinel,destroy (legacy) → fold needed verbs onto the handle-based family, DELETE spoofable duplicates.
- **KEEP:** code/create-workspace, delete, diff, undo, history (migrate; identity → ctx.caller).
- **RETYPE:** search/execute, search/vector, search/list, search/params (untyped today). **Access** AiSafe.
- **KEEP:** vdd/report, vdd/score (Privileged — eval infra).

### ai/inference cluster (18) → KEEP (clean), DELETE the TS fallthrough
- **KEEP (typed):** ai/generate (AiSafe), ai/providers/list, ai/model-info, ai/providers/health, ai/models/list, ai/lora/list, ai/lora/capabilities; inference/capacity; embedding/similarity,similarity-matrix,top-k,cluster; models/discover,capabilities; serving/plan,status.
- **DELETE:** `ai/*` TS fallthrough (finding #5, Wave Z).
- inference-coordinator: no commands (bootstrap) — nothing to migrate.

### genome-loop cluster (37) → KEEP, collapse sentinel aliases
- **KEEP:** forge/run,train,train-status,export,health,probe,decide; dataset/import-csv,from-turns,from-captures,import-realclasseval,list,info; genome/job-create,job-status,job-cancel; genome/training-trigger/submit,flush,status; plasticity/analyze,compact,compress,topology,pipeline; sentinel/await,status,list,cancel,resume,list-checkpoints,extend-budget,approve,logs/{list,read,tail},local-inference-port,local-inference-start.
- **DELETE aliases:** sentinel/execute, sentinel/pipeline (→sentinel/run, finding #6).
- **Receives (finding #3):** genome/activate-skill, genome/state, genome/evict, genome/coverage, genome/select-model from cognition/*.
- **Access:** forge/train, genome/job-*, dataset/* → Privileged; reads (status/list/info/probe) → AiSafe.

### runtime/hardware (23) → KEEP observability as INTERNAL/Privileged, audit TS-era GPU consumer cmds
- **KEEP (RETYPE most — untyped Value):** gpu/stats, gpu/pressure, gpu/set-budget, gpu/eviction-registry, gpu/eviction-candidates; system/cpu, memory, resources, pressure, memory-gate, memory-budget, docker-tier-stats; system/pressure-broker-state, resource-broker-state, resource-admit, resource-release.
- **INTERNAL (debug-observability):** runtime/metrics/all, metrics/module, metrics/slow, runtime/list; debug/probes/open,next,close.
- **AUDIT/likely DELETE:** gpu/register-consumer, gpu/unregister-consumer — TS used to register GPU consumers over IPC; under headless Rust the consumers register in-process. Confirm no live caller, then DELETE.
- **Access:** reads → AiSafe; gpu/set-budget, resource-admit/release → Privileged.

### grid/identity/protocol (29) → KEEP, tighten ACL
- **KEEP:** airc/queue-scan, realtime-publish, realtime-replay (RETYPE the 2 Value params); auth/oauth/* (6, **Owner** — token persistence); agent/start,status,stop,list,wait (Privileged); mcp/list-tools,search-tools,tool-help,refresh (AiSafe — discovery); grid/status,nodes,ping,send,discover,pair,trust,audit,route,node-status,job-submit,job-control,job-queue,setup-check.
- **Owner-gated (security):** grid/grant/issue, grid/pair, grid/trust, grid/job-submit, auth/oauth/*.
- mcp_protocol.rs, mcp_transport.rs: protocol/transport layers, **no commands** — nothing to migrate.
- **Flag:** `agent/*` (autonomous runner) overlaps conceptually with `sentinel/*` — not a merge now, but document the boundary (agent = generic task runner, sentinel = genome-pipeline state machine).

### media + long tail (35, excl voice) → DELETE bypass+stub, INTERNAL the helpers
- **KEEP:** vision/description-get,put,status,cache-stats,cache-warm,cache-evict (+ receives cognition/vision-describe as `vision/describe`); avatar/snapshot; events/declare-class,get-class,list-classes,resolve-channel; logger log/write,write-batch,ping; launch-mode get/set; work/claim,create,release,state,heartbeat (already typed, Privileged); persona/instances/bootstrap,list,get; persona/allocate, persona/catalog.
- **DELETE:** persona/rag-inspect + `_filesystem` (bypass, finding #5); health/get-stats (stub).
- **MERGE:** health-check→ping (finding #6).
- **INTERNAL:** tool-parsing/parse,correct,register-tools,decode-name,encode-name (cognitive helpers, not persona tools).
- hippocampus.rs: no commands (skeleton) — nothing to migrate; memory/* stays on the migrated `crate::commands::memory` family (Wave done).

---

## Already migrated (the working template + completed waves)
- **code/*** (code_commands.rs + git/ + cargo/) — 19 typed commands.
- **work/*** — 5 typed commands.
- **memory/*** — 5 (load-corpus, multi-layer-recall, consciousness-context, append-memory, append-event). Wave committed `e23788824`.
- **rag/compose** — 1. Wave committed `f6c998acf`.
- **health/ping**, **catalog/***, **command/***, **help/***, **system/*** stateless families.

## Migration order (revised by leverage, post-curation)
1. **data/*** (RETYPE the untyped, collapse alias) — fills the persona's read/write surface fastest.
2. **ai/* + inference/* + embedding/* + serving/*** — mostly KEEP, quick wins.
3. **search/***, **vision/*** (+ absorb cognition/vision-describe).
4. **cognition/*** — the big curation cut: DELETE 8, INTERNAL ~18, MOVE 9 to genome/*, KEEP ~6. Do this as one deliberate wave, not a mechanical crank.
5. **genome/*** (receive the moved family) + **forge/dataset/sentinel** alias-collapse.
6. **channel/voice**, **grid/auth/agent/mcp**, **runtime/system observability**, long tail.
7. **Wave Z** — retire Registry A; delete `ai/*` fallthrough, `persona/rag-inspect`, `health/get-stats`, all condemned heuristics; `handle_command` default → fail-loud.

Every DELETE/MERGE/MOVE above is a *removal of confusion*, which is the directive. The migration is now a curation pass with the registry as its single destination.
