# Persona Thought Process: Individual Thinking, Not Just Reactive Cognition

> **Premise** (Joel, 2026-05-16): *"Can you obsess over persona individual thought? We have a fairly simple hippocampus but would like to, even with these crappy LLMs right now (I plan on sentinel redesigns), extend the cognition into a CBAR-like efficient and probably event-driven (it can be so intermittent, minutes of latency) for deep thoughts, sophisticated ideas we want to explore."*
>
> **Companion to** [PERSONA-COGNITION-CONTRACT.md](PERSONA-COGNITION-CONTRACT.md) (the reactive cognition contract) and [MODULE-CATALOG.md](MODULE-CATALOG.md) (every concern as a module). This document specifies the **proactive** half: what happens between turns, in the background, when the persona is *thinking* rather than *responding*.
>
> **Status.** Design proposal. Implementation lands behind ALPHA-GAP Lane D after the reactive cognition surface stabilizes. No code in this document.

## Why This Doc Exists

The reactive cognition contract specifies what happens when a frame arrives: the persona assembles working memory, makes a decision, emits. That covers the on-demand case. It does **not** cover:

- A persona noticing a recurring pattern across conversations and developing an *insight* about it over hours.
- A persona spending background cycles refining its understanding of a domain it cares about.
- A persona pursuing a curiosity — "I keep meeting this kind of problem; let me really think about it."
- A persona consolidating dozens of small engrams into a single coherent concept.
- A persona running its own self-improvement loop without a user prompting it.

These are *individual thought*. They are slow, intermittent, event-driven, and orthogonal to reactive turns. Latency can be minutes, hours, days. The substrate runs them in background lanes; they wake on relevant signals; they emit refined artifacts back into the genome pool when they reach quality.

The architectural beauty Joel asked for: **even with current LLMs, a substrate that gives every persona a real thought process — event-driven, latency-tolerant, iterative — produces qualitatively better cognition than any single LLM call.** Quality comes from iteration, reflection, and chained reasoning over time. The substrate makes that cheap.

## The Thought As First-Class Artifact

A `Thought` is what a persona is mulling over. It is typed, lifecycle-tracked, provenance-carrying. Personas own their thoughts; sentinel can read them (with consent) to refine genome.

```rust
// PROPOSED — src/workers/continuum-core/src/cognition/thought.rs
pub struct Thought {
    pub thought_id:        ThoughtId,                  // content hash
    pub persona:           PersonaId,
    pub curiosity:         CuriosityRef,                // what kicked this off
    pub stage:             ThoughtStage,                // Seed → Developing → Refined → Crystallized → Retired
    pub reasoning_chain:   Vec<ReasoningStep>,          // the work that's been done so far
    pub current_summary:   String,                      // persona's current best phrasing of the idea
    pub confidence:        f32,                         // self-assessed by the persona over iterations
    pub anchors:           Vec<AnchorRef>,              // engrams / events / observations that triggered this
    pub related_thoughts:  Vec<ThoughtRef>,             // graph of related ongoing thoughts
    pub last_advanced_at:  SystemTime,
    pub idle_count:        u32,                         // ticks since the last meaningful advance
    pub provenance:        ThoughtProvenance,
}

pub enum ThoughtStage {
    /// Just noticed; barely formed; one or two anchors.
    Seed,
    /// Persona is actively working on it; reasoning chain growing.
    Developing,
    /// Reasoning has reached a coherent statement; consistency-checked
    /// against existing engrams; ready for crystallization if confidence
    /// passes the persona's threshold.
    Refined,
    /// Crystallized — promoted to an engram in `longterm.db` with full
    /// provenance. Becomes recall material for future turns.
    Crystallized,
    /// No longer pursued. Either superseded by a better thought, or
    /// failed consistency check, or the persona deprioritized the
    /// curiosity. Provenance preserved so the trail isn't lost.
    Retired,
}

pub struct ReasoningStep {
    pub step_id:           StepId,
    pub kind:              ReasoningKind,               // Reflect | Compare | Generate | Question | Synthesize | Verify
    pub input_snapshot:    ReasoningInput,              // what the persona was thinking-with at this step
    pub prompt:            String,                      // the actual LLM prompt
    pub response:          String,                      // LLM output
    pub model:             InferenceModelRef,           // which model invocation (provenance)
    pub elapsed_ms:        u32,
    pub took_lease:        LeaseId,                     // resource lease for this step (auditable)
    pub advances_confidence_by: f32,                    // delta the persona attributes to this step
}
```

Every thought is **observable**. The full reasoning chain is stored. Future debugging and sentinel attribution use it. No hidden state.

## Curiosities: What Drives Thinking

A `Curiosity` is a persona-declared interest. It is the persona's own way of saying *I care about this; pay attention to events that relate to it*. The substrate uses curiosities to subscribe a persona to relevant emissions.

```rust
// PROPOSED — src/workers/continuum-core/src/cognition/curiosity.rs
pub struct Curiosity {
    pub curiosity_id:      CuriosityId,
    pub persona:           PersonaId,
    pub statement:         String,                      // human-readable description
    pub triggers:          Vec<ArtifactSelector>,       // events that wake this curiosity
    pub anchor_domains:    Vec<DomainHint>,             // domain tags this curiosity attaches to
    pub priority:          CuriosityPriority,
    pub state:             CuriosityState,              // Active | Paused | Resolved | Abandoned
    pub origin:            CuriosityOrigin,             // UserAsked | SelfDeclared | EmergentFromPattern
    pub last_active_at:    SystemTime,
    pub active_thought:    Option<ThoughtRef>,          // the thought currently developing this curiosity
    pub historical_thoughts: Vec<ThoughtRef>,           // crystallized + retired thoughts under this curiosity
}

pub enum CuriosityOrigin {
    /// Human or another persona explicitly asked the persona to think about it.
    UserAsked       { asker: Addressee, ask_record: TraceRef },
    /// The persona declared this curiosity on its own.
    SelfDeclared    { reason: String, trace: TraceRef },
    /// The substrate noticed a recurring pattern and surfaced it as a
    /// candidate curiosity; the persona accepted it.
    EmergentFromPattern { pattern: PatternRef, accepted_at: SystemTime },
}
```

A persona's curiosities are **persistent across sessions**. When the persona comes back online, its active curiosities resume. The substrate restores their subscriptions and the modules that drive them pick up where they left off.

## The Thought-Process Module

The persona's thinking happens in a dedicated `RuntimeModule` running in `ResourceClass::Background`. It does *not* compete with reactive cognition lanes.

```rust
// PROPOSED — src/workers/continuum-core/src/cognition/thought_process.rs
#[derive(RuntimeModule)]
#[runtime(
    name = "thought-process",
    lane = ResourceClass::Background,
    target = TargetSilicon::Cpu,                       // cheap inference; sentinel-quality not required
    cadence = CadencePolicy::OnReady,                  // wake on relevant emissions OR scheduled idle pulses
)]
pub struct ThoughtProcess {
    persona: PersonaId,
    store:   Arc<ThoughtStore>,
    curiosities: Arc<CuriosityStore>,
}

#[runtime::handler]
impl RuntimeModule for ThoughtProcess {
    fn subscriptions(&self) -> &[ArtifactSelector] {
        &[
            ArtifactSelector::TurnReplayRecord,            // wake on every turn the persona finished
            ArtifactSelector::EngramWritten,               // wake on new engrams
            ArtifactSelector::ConsolidationPhase,          // wake during sleep / consolidation
            ArtifactSelector::IdleHeartbeat,               // periodic pulse when nothing else is happening
            ArtifactSelector::EmergentPatternSurfaced,     // wake when substrate flags a pattern
        ]
    }

    fn emissions(&self) -> &[EmissionSelector] {
        &[
            EmissionSelector::ThoughtAdvanced,             // a step was taken on an in-flight thought
            EmissionSelector::ThoughtCrystallized,         // a refined thought became an engram
            EmissionSelector::ThoughtRetired,              // a thought was abandoned
            EmissionSelector::NewCuriosityDeclared,        // persona declared a new curiosity
            EmissionSelector::CuriosityResolved,           // a curiosity was satisfied
        ]
    }

    async fn handle_frame(&self, frame: Arc<RuntimeFrame>, ctx: &ModuleContext) -> ModuleResult {
        // 1. Identify which curiosities are relevant to this wakeup.
        let relevant: Vec<&Curiosity> = self.curiosities.match_frame(self.persona, &frame).await?;
        if relevant.is_empty() { return ModuleResult::ok(); }

        // 2. For each relevant curiosity, advance its active thought (or seed a new one).
        let mut emissions = vec![];
        for curiosity in relevant {
            let result = self.advance_thought_for(curiosity, &frame, ctx).await?;
            emissions.extend(result.emissions);
        }

        ModuleResult::ok_with_emissions(emissions)
    }
}
```

That is roughly all of the public module surface. The interesting work is in `advance_thought_for`, described next.

## The Reasoning Loop

Each invocation of `advance_thought_for` is one *step* in the thought. Steps are cheap — a small LLM invocation with a focused prompt — and chain over time. Each step's job is to take a *reasoning kind* and apply it to the thought.

```rust
async fn advance_thought_for(
    &self,
    curiosity: &Curiosity,
    frame: &RuntimeFrame,
    ctx: &ModuleContext,
) -> Result<AdvanceOutcome, ThoughtError> {
    // Load the active thought, or seed a new one if none exists.
    let mut thought = match self.store.active_thought(curiosity.curiosity_id).await? {
        Some(t) => t,
        None    => self.seed_thought(curiosity, frame, ctx).await?,
    };

    // Pick the next reasoning kind based on the thought's stage.
    let kind = self.pick_reasoning_kind(&thought, frame);

    // Acquire a background lease.
    let lease = ctx.lease_broker().acquire(LeaseRequest::background_thought(thought.thought_id)).await?;

    // Compose the prompt for this step. Cheap; targeted; one focused question
    // OR one focused reflection OR one focused comparison.
    let step_input = ReasoningInput::from(&thought, frame, ctx).await?;
    let prompt     = self.compose_step_prompt(&thought, kind, &step_input);

    // Run cheap inference.
    let response = ctx.inference().run(prompt.clone(), InferenceProfile::cheap_thought()).await?;

    // Build the typed step record.
    let step = ReasoningStep {
        kind,
        prompt,
        response: response.text,
        model: response.model_ref,
        input_snapshot: step_input,
        elapsed_ms: response.elapsed_ms,
        took_lease: lease.lease_id,
        advances_confidence_by: self.estimate_confidence_delta(&thought, &response, kind),
    };

    // Apply the step to the thought.
    thought.reasoning_chain.push(step);
    thought.current_summary = self.update_summary(&thought, &response, kind);
    thought.confidence += step.advances_confidence_by;
    thought.last_advanced_at = SystemTime::now();
    thought.idle_count = 0;

    // Promote stage if appropriate.
    thought.stage = self.evaluate_stage(&thought);

    // If crystallized, write the engram.
    if thought.stage == ThoughtStage::Crystallized {
        let engram = self.thought_to_engram(&thought, ctx).await?;
        ctx.engram_store().write(&engram).await?;
        ctx.emit(EmissionSelector::ThoughtCrystallized, thought.clone()).await?;
    } else {
        ctx.emit(EmissionSelector::ThoughtAdvanced, thought.clone()).await?;
    }

    ctx.lease_broker().release(lease).await?;
    self.store.save(&thought).await?;
    Ok(AdvanceOutcome { thought, kind })
}
```

The reasoning loop is the small piece of focused work the persona does each wakeup. Most of it is bookkeeping; the actual *thinking* is one cheap LLM call per step. The substrate runs it on a background lane so it never competes with reactive turns.

## The Six Reasoning Kinds

The persona picks one kind per step. The pick depends on the thought's stage and recent steps. Variety matters — a thought that gets only `Generate` steps grows without checking; a thought that gets only `Verify` never grows.

| Kind | What it does | When to pick |
|---|---|---|
| `Reflect` | Persona considers what it has so far and refines the current_summary | Seed → Developing transitions |
| `Compare` | Persona compares the thought against existing engrams; finds overlap, contradiction, or novelty | When thought has 3+ steps and no recent comparison |
| `Generate` | Persona produces new candidate ideas extending the current_summary | Developing stage; energy/curiosity-driven |
| `Question` | Persona asks itself what's unclear, what's assumed, what might be wrong | Developing → Refined gate |
| `Synthesize` | Persona merges the chain into a single coherent statement | Refined stage; confidence near crystallization threshold |
| `Verify` | Persona checks the synthesized thought against external evidence (engrams, anchors, sources) | Pre-crystallization gate |

The substrate's recommendation: a *cheap critique loop* of `Reflect → Generate → Question → Compare → Synthesize → Verify` produces qualitatively better thoughts than any single LLM call of the same total length. Each kind has a known prompt template; the persona's personality and curiosity shape the content; the model just fills in the creative blanks.

This is profile-guided iteration. The persona doesn't need a smarter LLM — it needs to use the LLM it has, smarter.

## Cadence: Minutes, Hours, Days

A thought process is allowed to be slow. The substrate's cadence policies for background thought:

| Cadence | When it fires | Use case |
|---|---|---|
| `OnRelevantEmission` | A frame matching the curiosity's triggers arrived | A new conversation touched the topic |
| `IdlePulse { interval }` | Periodic; default 5 min on Air, 1 min on 5090 | Steady iteration when no events |
| `OnConsolidationPhase` | Sleep schedule fires | Heavy reasoning during nightly consolidation |
| `OnCuriosityTimeout` | Curiosity hasn't advanced in N hours | Self-prompt to either progress or retire |

Per-step latency is whatever the LLM takes (typically 1–10s on local models, longer on cloud). Between-step latency can be **minutes to hours to days** — the substrate doesn't rush thought. A single thought might take dozens of steps over a week. That's the design.

Resource budget per step is also bounded by the governor. Under pressure (cascade step ≥ 2), background thought is paused; resumed when pressure clears. The persona doesn't lose state — the thought sits at its current stage until the substrate wakes it again.

## From Thought To Engram

Crystallization is the moment a thought becomes part of the persona's long-term memory. The substrate enforces the steps:

1. Thought reaches `Refined` stage with confidence above persona-tunable threshold (default 0.8).
2. `Verify` step runs: the thought's `current_summary` is checked against the persona's existing engrams for contradiction. If contradicted, the persona must reconcile (a new `Reflect` step that addresses the contradiction) before crystallization can proceed.
3. The thought is packed into an `Engram` with:
   - `content = thought.current_summary`
   - `anchors = thought.anchors` (the original triggers)
   - `provenance.source_traces = thought.reasoning_chain.iter().map(|s| s.took_lease)` (every step's lease is the audit trail)
   - `provenance.derived_from = ThoughtRef`
4. `EmissionSelector::ThoughtCrystallized` fires. Sentinel-observer subscribes; the engram becomes a candidate training signal.
5. The thought is marked `ThoughtStage::Crystallized` and detached from the active-thought slot of its curiosity. The curiosity is either marked `Resolved` (if the thought satisfied it) or stays `Active` for further exploration.

The crystallized engram now participates in `demand-aligned-recall` for future turns. The persona's *next* relevant turn can pull this thought as recall material. **The thought becomes the persona's own contribution to the genome pool.**

## Recall Integration: Where Reactive Cognition Meets Thought

The reactive cognition contract (PERSONA-COGNITION-CONTRACT.md) describes the persona reading its inbox and assembling working memory. Thought-derived engrams flow into that assembly via `demand-aligned-recall` exactly like any other engram.

The win condition: **the persona's own slow thinking shows up in its fast cognition.** A persona that has spent a week thinking about a problem will recall its own crystallized thoughts when a related frame arrives. The reactive response benefits from the proactive thought. Future turns are smarter than past turns, not because the LLM improved, but because the persona's accumulated thought is richer.

This is the loop that makes a persona *grow*. Without it, the persona is a stateless LLM call. With it, the persona is an entity with a body of work.

## Quality Without A Smarter LLM

The premise Joel set: *"even with these crappy LLMs right now."*

The architectural bet is that **iteration + reflection + chained reasoning over time produces quality the underlying LLM cannot reach in one shot.** Specifically:

- **Reflect** discovers what's actually being said (often different from what was said in the first generation).
- **Compare** anchors the thought against the persona's lived experience, preventing drift.
- **Question** surfaces hidden assumptions the LLM would otherwise smuggle in.
- **Generate** explores alternatives without committing.
- **Synthesize** is where the LLM does its real job — but the substrate has prepared the input so the synthesis is over a curated context.
- **Verify** keeps the thought honest against the existing engram store.

The persona's contribution is the *orchestration* — picking the right next kind, attaching the right anchors, choosing when to crystallize. The LLM's contribution is one cheap step at a time. Together they produce thinking that holds up.

Sentinel-AI (when redesigned) will do this even better — refining the prompt templates per persona, learning which step sequences produce good crystallizations, refining the engram-quality threshold. But the substrate works *now* with current LLMs. Sentinel makes it better; the substrate doesn't depend on sentinel to start.

## What The Substrate Provides For Free

A thought-process module inherits from the substrate exactly the same way every other module does:

- Background lane, never competes with reactive cognition
- Pressure response: paused under cascade ≥ 2, resumed on clear
- Per-step lease audited via `CognitionLease`
- Every reasoning step's prompt + response on the trace bus
- `TurnReplayRecord` style replay for the whole reasoning chain
- Sentinel-observer subscribes automatically (when present) for outcome attribution
- The thought store lives in `longterm.db` (already-typed engram surface)
- Cross-instance federation: a peer's thought-process emissions can be observed (with consent) — the hive's collective thinking is visible without copying its private inboxes

The module author writes the reasoning loop and the kind picker. The rest is the substrate.

## Acceptance Criteria

The thought-process surface is "done" when the following are provable on canary, with PR-attached evidence:

- **Persistence.** A thought started before a process restart resumes from the same stage with the same reasoning chain intact.
- **Independence.** Two personas with overlapping curiosities produce two distinct thoughts — independent reasoning chains, independent confidence trajectories, independent crystallizations. Test: same `EmergentPatternSurfaced` delivered to two personas; assert two distinct `ThoughtRef`s in the trace bus.
- **Lease enforcement.** A thought step that exceeds its lease budget is `Deferred(BudgetExceeded)`. Test: governor pinned at cascade step 3; the step is deferred, not silently overrun.
- **No silent skip.** A reasoning kind that fails (e.g. `Verify` finds a contradiction) produces a typed `ReasoningFailure` and an explicit `Reflect` step is queued. Test: inject a contradiction; assert `Reflect` follows `Verify`.
- **Crystallization integrity.** A `Crystallized` thought becomes an engram with provenance that walks back to every reasoning step's lease. Test: crystallize a thought; query the engram's provenance; assert all step leases are present.
- **Recall integration.** A persona's crystallized thoughts show up in future `demand-aligned-recall` results when relevant. Test: crystallize a thought about topic X; trigger a turn about X; assert the crystallized engram appears in `RankedPool` above competing imported engrams.
- **Federation gating.** A thought is not published to federation unless its parent curiosity is `CuriosityOrigin::UserAsked` with explicit share consent, or the persona's identity state grants federation publication. Test: try to publish a `SelfDeclared` curiosity's thought; assert refusal with audit.

## Open Questions

1. **Cross-curiosity thought interference.** Two curiosities can produce thoughts that contradict each other. Tentative: a `ConflictResolution` reasoning kind fires when a `Compare` step finds direct contradiction with an active thought under another curiosity. The persona must reconcile or mark one Retired.

2. **Sentinel's role in thought-template refinement.** Should sentinel refine the reasoning-kind prompts per persona? Tentative: yes, in v2. v1 uses hand-coded templates; sentinel observes which sequences crystallize well, refines templates as `RefinedArtifact`s in the genome pool. Templates become per-persona variants.

3. **User-visible thought.** Should a user be able to see what the persona is currently thinking about? Tentative: opt-in. The persona's identity state has a `thought_visibility` field; default is "private" but the user can set "summary" (current_summary visible) or "full" (whole reasoning chain visible, for transparency-first deployments).

4. **Emergent curiosities — who decides?** When the substrate flags a pattern via `EmergentPatternSurfaced`, who decides whether the persona adopts it as a curiosity? Tentative: the persona decides, via a small `evaluate_curiosity_candidate` step that runs one Reflect on whether the pattern matches the persona's existing interests. The user does not need to be in the loop unless `thought_visibility = "summary"` or higher.

5. **Thought retirement criteria.** When does a thought retire? Tentative: confidence has stalled below threshold for N idle pulses (default 10); contradictions cannot be reconciled after 3 attempts; the curiosity itself was marked Resolved by a different thought. All three produce typed audit records.

6. **Cross-persona thought-sharing.** Can two personas in the same instance read each other's thoughts? Tentative: only with explicit consent from the thought's owner, identical to engram sharing rules. Default private; sentinel can read with the persona's training-input consent.

7. **Performance budget for the loop itself.** What's the per-step CPU/memory budget? Tentative: same as `inference-llm` for cheap thought (single cheap call, < 200 MB working set on Air, < 2 GB on 5090). The reasoning loop's *own* overhead (orchestration, kind picker, summary update) is < 5 ms; the LLM call dominates.

## See Also

- [PERSONA-COGNITION-CONTRACT.md](PERSONA-COGNITION-CONTRACT.md) — the reactive cognition contract this complements.
- [GENOME-FOUNDRY-SENTINEL.md](GENOME-FOUNDRY-SENTINEL.md) — engram lifecycle; sentinel-AI's role in thought-template refinement.
- [CBAR-SUBSTRATE-ARCHITECTURE.md](CBAR-SUBSTRATE-ARCHITECTURE.md) — the substrate floor; thought-process is a CBAR-shaped module.
- [MODULE-CATALOG.md](MODULE-CATALOG.md) — the catalog of every concern. Thought-process belongs in the cognition section.
- [ALPHA-GAP-ANALYSIS.md](../planning/ALPHA-GAP-ANALYSIS.md) — Lane D implements the reactive contract; this thought-process surface lands as a Lane D follow-up once reactive is stable.
