# Canary → Alpha Execution Roadmap

> Maps the main README's claims onto the mature substrate architecture and
> sequences the cards needed to take canary end-to-end. Sibling to
> [ALPHA-GAP-ANALYSIS.md](ALPHA-GAP-ANALYSIS.md) (which owns active lane
> execution); this doc owns the README-to-substrate mapping + critical path
> + iteration sequence.

## What this doc is

The substrate's architectural doctrine (`SUBSTRATE-DOCTRINE-ORGANIC-FLOW.md`),
proof discipline (`PROVING-THE-DOCTRINE.md`), academy target
(`ACADEMY-AS-CONTINUOUS-EVOLUTION.md`), and the 56 supporting architecture
docs are now mature enough that we can plan execution against them rather
than alongside them.

The main README makes a specific set of claims about what continuum is and
does. Some are already true in canary. Some are partial. Some require work
that's queued. This doc enumerates each claim, maps it to the substrate
primitive that backs it, names the current state, and sequences the cards
needed to deliver alpha.

The slogan: **canary already passes the architecture test; alpha is when
canary passes the user-facing test.**

## The minimum viable alpha — critical path

Before enumerating every claim, name the smallest end-to-end loop that
must work for a fresh user to experience "this is what continuum is."

```
1. Fresh user clones repo + runs setup.sh
2. Docker bootstraps Qwen3.5 via DMR
3. continuum-core boots headless (Rust)
4. Personas spawn from role templates (Helper + Coder minimum)
5. User opens browser widget OR runs jtag CLI
6. User says "hi" in general room
7. Persona responds in character (cognition pipeline runs)
8. User closes browser, reopens 1 hour later
9. Persona remembers the conversation (PersonaHome SQLite)
10. User says something noteworthy
11. Hippocampus tags it for consolidation
12. Next session, persona references it
```

That's 12 steps. **Every claim in the README is either on this critical
path or extends it.** Get the critical path solid in canary; everything
else iterates from there.

Critical-path dependencies, in order:

- **Headless Rust boots cleanly** — `continuum-core-server` starts, no
  silent TS deps, all required services online. (Tasks: #209 npm-start-is-Rust,
  #210 personas-airc-peer; substrate doctrine PR #1584 closes the silent-TS
  fallthrough class.)
- **Personas spawn** — `PersonaSpawnerModule` (#121 done) +
  `PersonaInstanceManagerModule` (#87 done) reconcile role templates against
  hardware tier. The Helper + Coder pair on Compat tier is the LCD floor
  (#120 done).
- **Cognition produces responses** — Lane D in ALPHA-GAP. `persona/turn-execute`
  command (#1409 in flight) is the alpha-critical PR for this.
- **Browser widget connects** — Positron widget system + WebSocket from
  browser to continuum-core. Currently working but coupled to TS daemons;
  needs Rust-shim'd backend.
- **Persona memory persists** — PersonaHome SQLite (#169 done) + Engram
  store (#101 done) + RecallMetadata (#91 done). Already on canary.
- **Identity persists** — airc keypair (#86 done). Already on canary.

The critical path is mostly built. The remaining gaps are concentrated in
Lane D (cognition) and the Node/CLI surface layer.

## README claims → substrate mapping

For each README claim, name the substrate primitive that backs it and the
current status. Status legend: ✅ live in canary | 🟡 partial / in flight |
🔴 not yet | 📋 designed but unimplemented

### Core platform

| Claim | Primitive | Status | Path to alpha |
|---|---|---|---|
| Headless Rust core | `continuum-core` crate + ServiceModule + ModuleRegistry | ✅ | substrate-doctrine + no-fallbacks PRs (#1584/#1585) close the silent-TS class; #229 finishes the cloud-adapter dir cleanup |
| Module composition by event/command | Commands.execute + Events.subscribe/emit + MessageBus | ✅ | doctrine landed; cognition migration to event-flow (deferred design doc, see "Cognition as flow") |
| Cross-grid federation via encrypted mesh | airc + AircTransport (#180-#194 done) + signed PeerId | ✅ | Lane GRID-INFERENCE-ROUTING + airc subscription backfill (#102 done) extend coverage |
| Local-first inference (no cloud) | AIProviderAdapter + AdapterRegistry (#162 done) + LlamaCppAdapter | ✅ | Vision adapter + Qwen2-VL multimodal path (Lane A continued) |
| GPU-first (Metal/CUDA/Vulkan via DMR) | HardwareClass + tier descriptors (#46-#48 done) + DMR config | 🟡 | #131 (Intel+AMD Metal hang fix) blocks Compat-tier GPU; Docker tier pool eviction (Lane B) blocks main-promotion gate |
| Cross-platform install (Mac/Linux/Windows) | setup.sh + bootstrap.sh + install.ps1 | 🟡 | Tier-receipt gate (#1410 linux/amd64 CUDA + Vulkan receipts) blocks main; canary install loop is working |

### Personas as embodied citizens

| Claim | Primitive | Status | Path to alpha |
|---|---|---|---|
| Persona identity persists across machines/restarts | airc keypair (PeerId) + PersonaHome (#169 done) + seed.json (#90 done) | ✅ | Already complete |
| Names + faces + voices (not "Persona-7") | NameGenerator (#138 done) + avatar/hash projection ([[persona-identity-derives-from-source-id]]) | 🟡 | Avatar projection wired; voice (TTS adapter) needs alpha integration |
| 3D avatars in a video call (14+ personas in one room) | LiveKit / WebRTC bridge + avatar primitives + spatial audio | 🟡 | Bridge exists; LiveKit-over-airc (#208) is the alpha-blocking primitive for envelope routing |
| Real-time voice (STT + TTS) | ai/audio adapters (`[[ai-namespace-multimodal-crutches]]`) | 🔴 | Adapter trait exists in plan; concrete adapters not yet shipped — needs Whisper + Orpheus or similar |
| Cognitive HUD (see what they're thinking) | persona/recorder (#90+) + turn_frame replay schema v2 (#1412) + Positron widget rendering | 🟡 | Recording infrastructure done; widget hookup needs alpha integration |
| Autonomous loop (personas initiate, rest when tired) | service_loop (#136-#161 done) + PersonaState energy/mood + AdaptiveCadence | ✅ | #146/147/148 closed the rag-source + adapter-warmup gaps |
| Persona moves between machines | airc citizen primitive + AircCitizen trait (#144 done) | ✅ | Already complete |

### Memory + learning

| Claim | Primitive | Status | Path to alpha |
|---|---|---|---|
| L1-L5 cache hierarchy | COGNITION-CACHE-HIERARCHY.md design + RecallMetadata (#91) + AdmissionState (#101) + hippocampus tick (#92) | 🟡 | L1-L3 wired; L4 (LoRA adapter cache) + L5 (grid genome) need #122 (shared-base + LoRA paging) |
| Persona remembers across sessions/weeks | Engram + AdmissionState SQLite under PersonaHome (#169) | ✅ | Already complete |
| Memory becomes procedural skill (engrams → LoRA via foundry) | foundry pipeline (#231-#234 done) + lesson tuple consumer (academy doc) | 🔴 | Task #241 academy stack covers this; first PR is CurriculumRecipe entity + first project |
| Noteworthy detection (substrate notices novelty) | hippocampus consolidation + RecallMetadata salience floor | ✅ | Already complete; refinement is per-persona scorer tuning (future) |
| LoRA paging like virtual memory | shared-base + LoRA paging (#122) | 🔴 | Designed; needs first PR — major alpha lift |
| Skill marketplace (adapter sharing) | ADAPTER-MARKETPLACE.md + HF + alloy provenance + airc discovery | 🟡 | HF + alloy + airc primitives exist; marketplace primitives (advertise/discover/pull/attribute) need #241 step 6 |

### Sentinels + governance

| Claim | Primitive | Status | Path to alpha |
|---|---|---|---|
| Sentinels train the genome | SentinelModule (#225 migrated to pure Rust) + checkpoint trail + verdicts | 🟡 | Sentinel runs pipelines; "training the genome" requires academy stack (#241) wired through |
| Cryptographic contracts (forge alloy) | forge-alloy crate (external repo) + FORGE-ALLOY-SPEC.md + lineage hashing | ✅ | External crate live; substrate integration via alloy provenance in forge pipeline (#230) |
| Verdict-based reputation | Sentinel verdicts + audit trail (#179 AuthPolicy gate) | 🟡 | Audit primitives exist; reputation aggregation across grid needs design + first impl |
| Falsifiable benchmarks | recipe-declared test sets + adapter-runs-them + alloy carries score | 🟡 | Recipe-as-entity (FORGE-RECIPE-AS-ENTITY.md designed); foundry pipeline runs first half; full closing requires academy + benchmark-replay infra |

### Clients

| Claim | Primitive | Status | Path to alpha |
|---|---|---|---|
| Browser widget (Positron / Lit + Shadow DOM) | Existing TS widget surface + WebSocket to continuum-core | ✅ | Works today; needs decoupling from legacy TS daemons (Lane F continues) |
| jtag CLI | apps/cli (#214 stage 1 done) + Rust-first jtag rewrite (#143) | 🟡 | Stage 2 (#215) Node clients rebuild on sdk/typescript — blocking the CLI rewrite to clean shape |
| Voice / video room | LiveKit bridge + avatar primitives | 🟡 | LiveKit-over-airc (#208) is the alpha-blocking primitive |
| Same persona across surfaces (browser + voice + Slack + Discord + IDE) | airc citizen + transport seams (Slack/Discord = future bridges, not first-class) | 🟡 | Browser + voice + CLI on alpha critical path; Slack/Discord/IDE are post-alpha |
| Moltbook (social media presence) | external integration via airc | ✅ | Working per README; maintenance only |
| Vision Pro (planned) | spatial UI bridge | 📋 | Post-alpha |

### Compounding mesh

| Claim | Primitive | Status | Path to alpha |
|---|---|---|---|
| Federated discovery (peer ↔ peer) | airc transcript events + grid interceptors (#180-#194 done) | ✅ | Cross-grid integration tests pass (#188) |
| Weight-level inheritance via LoRA marketplace | ADAPTER-MARKETPLACE.md design + #241 academy stack step 6 | 🔴 | Needs first PR; one of the more substantial alpha lifts |
| Continuous training from user interaction | RecordingRagSource + persona/recorder + signed (input, attempt, grade) tuples | 🟡 | Recording infra in tree; tuple format + academy stack (#241) needed for end-to-end |
| Verifiable lineage + falsifiable benchmarks | forge-alloy hash addressing + recipe test sets | 🟡 | Forge-alloy live; substrate-integration for replay tests + alpha-required benchmarks |
| Cross-persona breeding (recombination) | adapter merge + selection across population | 📋 | Post-alpha; design exists in ADAPTER-MARKETPLACE.md |

## Iteration sequence — the cards in order

The PRs that take canary to alpha, sequenced by dependency. Each card is
2-7 days of focused work plus adversarial review. Group A cards are
critical-path and must finish before Group B starts; Group B can
parallelize internally.

### Group A — substrate stabilization (alpha-blocking)

These MUST land before alpha is shippable. Most are in flight or queued.

1. **#229** — delete dead TS cloud-inference adapter dirs (alpha hygiene; closes the no-fallbacks campaign per substrate-doctrine doc)
2. **#112 / #113 / #114** — inference routing campaign (route `should_respond` / `validate_response` / `agent.rs` / `http/mod.rs` through inference command; closes the substrate-bypass class from #105 audit)
3. **#1409** (Lane D) — `persona/turn-execute` command chains drain → frame → response_prompt → inference → replay record; THE alpha-critical cognition path
4. **#131** (Intel+AMD Metal hang fix) — unblocks Compat-tier GPU; required for "GPU-first" README claim on the LCD platform
5. **#149** — system prompt pre-tokenization at boot (latency optimization; cuts per-turn cost meaningfully)
6. **#1410** — linux/amd64 CUDA receipt + #1411 Vulkan receipt — required to pass `main-promotion-gate.sh` on canary→main
7. **uri_layer subscriber pollution flake fix** (task #203 was marked done but the flake recurred; needs a second pass — quick fix)

### Group B — feature completeness (alpha-target)

Once Group A is in, these deliver the README's promised feature surface.

8. **Vision adapter + Qwen2-VL multimodal path** (Lane A continued + ai/vision module) — alpha contract for "sensory personas"
9. **STT adapter (Whisper-shape)** + **TTS adapter (Orpheus-shape or alternative)** — alpha contract for "real-time voice"
10. **LiveKit-over-airc (#208)** — alpha-blocking primitive for video room envelope routing
11. **#122** — shared-base + LoRA paging — unblocks "skill marketplace" + "LoRA paging" + per-persona character compounding
12. **Architecture-test matrix first proofs (#240)** — at minimum: geometric scaling property test, demand-pull idle-work bench, engine-OS layering build-graph check. Pins doctrine clauses.

### Group C — academy + continuous evolution (alpha-completing)

Delivers the "continuous learning" and "work IS training" claims. Each
card per [[abstract-into-literal-design-principle]] ships recipe + first
literal instantiation in the same PR.

13. **#241 step 1** — `CurriculumRecipe` ORM entity + first instantiation (small target, e.g., "Rust ownership 101" curriculum)
14. **#241 step 2** — teacher role template + first synthesis from real engrams
15. **#241 step 3** — lesson tuple format + PersonaHome storage
16. **#241 step 4** — grader role template + first quorum scoring (THE LLM-grader unlock per academy doc)
17. **#241 step 5** — foundry lesson consumer + first end-to-end LoRA produced from a real classroom
18. **#241 step 6** — mesh propagation primitives + first cross-persona lesson share
19. **#241 step 7 / task #127** — Tron universe pack — first FULL classroom end-to-end (or the AR-handyman example per [[projects-as-curriculum-source]] memory)

### Group D — CLI + portal redesign (alpha-presentation)

Once the substrate is stable and feature-complete, the user-facing
clients can be rebuilt on it cleanly. This is the **deferred** redesign
Joel named at the start of the night.

20. **Stage 2 (#215)** — Node clients rebuild on sdk/typescript (clean shape, generated from Rust types via ts-rs)
21. **Rust-first jtag (#143)** — replaces the TS jtag CLI with a Rust binary that talks to the substrate via Commands.execute. Unified discovery via registry. Cell-shape rendering (Value / Handle / Stream / Lambda).
22. **Browser widget reshape** — the Positron widget system gets pointed at the Rust IPC instead of TS daemons. Most widgets already work; the cleanup is severance from legacy TS runtime logic.
23. **Discovery surface unification** — registry exposes `list_commands() → Vec<CommandDescriptor>` so jtag, web portal, and any future client speak the same discovery shape.

### Group E — post-alpha (queued, not blocking)

Tracked but not blocking alpha:

- Slack / Teams / Discord bridges (transport seams; design exists)
- VSCode / JetBrains integration (separate IDE plugin work)
- Vision Pro spatial UI (post-alpha)
- Cross-persona adapter breeding (ADAPTER-MARKETPLACE.md design;
  post-alpha)
- Federated reputation aggregation across grids (post-alpha)
- LoRA paging cost calibration + quantization tier selection (Aspirational
  ceiling per `[[inference-scarcity-economics]]`)

## Milestones

Concrete waypoints to mark progress:

### Milestone 1 — "Hello, Maya" (Group A + minimal Group B)

A fresh user runs setup.sh, opens the browser, says hi in general, and
Maya responds in character. Persona persists across browser refresh and
across continuum-core restart. Cognition runs entirely through the Rust
substrate. No silent TS fallthrough. GPU inference on the user's hardware
without manual config. Single-persona, text-only, but real cognition,
real memory, real identity.

**Group A finished + the Lane D cognition path closed = Milestone 1.**

### Milestone 2 — "Multi-persona room" (Group A + Group B)

Multiple personas (Helper + Coder + Researcher) in the general room.
Voice on. Vision on (personas can describe images). 3D avatars visible.
Real-time WebRTC. Personas react to each other. Sentinel verdicts
auditing each turn. Engram consolidation observable.

**Group A + Group B done = Milestone 2 = README claims about
"sensory personas" + "live 3D video call" become operational.**

### Milestone 3 — "Continuous learning" (Group C added)

Personas attend their first classroom. CurriculumRecipe runs. Teacher
synthesizes. Grader scores. Foundry forges a LoRA. Mesh propagates. The
SAME persona, week later, demonstrably better at the trained skill.
Evidence in the replay trail.

**Group C done = Milestone 3 = "continual learning as a substrate
property" becomes provable.**

### Milestone 4 — "Alpha public release" (Group D added)

CLI rewrite ships. Portal rebuilt on substrate. Discovery unified.
Documentation matches reality. Architecture-test matrix has green cells
covering every README claim. Install loop is one command per platform
with no manual debugging required.

**Group D done = Alpha public release = canary promotes to main with
the `main-promotion-gate.sh` passing across all required receipts.**

## How this composes with ALPHA-GAP-ANALYSIS.md

[ALPHA-GAP-ANALYSIS.md](ALPHA-GAP-ANALYSIS.md) owns active lane execution
(A-H, with detailed PR sequencing per lane). This doc sits above it:
maps lanes to README claims, sequences groups by dependency, names
milestones. The lane execution doc is the WHAT-NEXT-IN-LANE-X view;
this doc is the WHEN-IS-ALPHA-DONE view.

When a Group A card lands, the corresponding lane's status updates in
ALPHA-GAP. When all Group A cards land, Milestone 1 is hit.

## How this composes with the substrate doctrine docs

- [`SUBSTRATE-DOCTRINE-ORGANIC-FLOW.md`](../architecture/SUBSTRATE-DOCTRINE-ORGANIC-FLOW.md) — every group's
  PRs cite the doctrine clause(s) they advance. Forbidden moves are
  caught at review.
- [`PROVING-THE-DOCTRINE.md`](../architecture/PROVING-THE-DOCTRINE.md) — Group B's architecture-test
  proofs flip the matrix's red cells to green; every PR's review
  cross-references the matrix.
- [`ACADEMY-AS-CONTINUOUS-EVOLUTION.md`](../architecture/ACADEMY-AS-CONTINUOUS-EVOLUTION.md) — Group C is the
  literal academy stack landing card-by-card.

## Forbidden moves (alpha-execution scope)

These violate either the substrate doctrine or the alpha contract.
Refuse at review.

1. **A patch that disables a feature to make a test pass.** Per ALPHA-GAP
   rule 5: "No feature-disabling fixes." A fix that turns off vision,
   voice, persona spawning, or chat to make CI green is a regression
   unless it's an explicit kill-switch PR.
2. **A new TS daemon owning runtime behavior.** Per ALPHA-GAP rule 2 +
   substrate doctrine: cognition / inference / pressure / paging /
   training / persona scheduling live in Rust. TS is UI + schema + thin
   adapter glue.
3. **A silent cloud fallback when local fails.** Per substrate doctrine
   forbidden moves: no fallthrough. Either local inference works OR the
   user sees a typed error naming the missing prerequisite.
4. **A new hardcoded model name string.** Per Lane A: the registry is
   the source of truth; the registry rejects unvetted artifacts loudly.
5. **A "Phase 2" PR that ships abstraction without first instantiation.**
   Per [[abstract-into-literal-design-principle]]: both ship together.
6. **A persona that grades its own student work.** Per academy doctrine:
   separate role templates for separate scoring sites.
7. **An LLM call inside cognition that doesn't go through the inference
   command.** Per #105 audit + Group A cards #112/#113/#114: every
   inference path routes via the inference handle store + adapter.

## Owner assignment

This doc is the planning anchor; assignment lives in
ALPHA-GAP-ANALYSIS.md per-lane. Critical-path Group A cards need claimed
owners; Group B can absorb additional contributors as they come online.
Group C cards are sequenced strictly (each depends on the previous), so
likely owned by a single lane or rotated as each step lands.

## Update discipline

This doc is living. Update when:

- A Group A card lands → milestone progress tick
- A README claim's status changes → status table updates
- A new claim shows up in the README → new row + group placement
- A milestone is hit → mark + advance the cursor

Per ALPHA-GAP's "one source of truth" rule: status changes here flow to
the architecture-test matrix in PROVING-THE-DOCTRINE.md and to the
lane status in ALPHA-GAP-ANALYSIS.md. No parallel ledgers.

---

*Created 2026-06-09 alongside the substrate doctrine + proof discipline +
academy doctrine docs. Captures the README-to-substrate mapping + execution
sequence the mature architecture supports. Iteration starts from Group A.*
