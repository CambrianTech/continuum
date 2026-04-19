# Shared Cognition Architecture

> **One shared analysis of objective meaning, N distinct LoRA-rendered expert responses.** Stop having four minds independently rederive the same observation about the same message. Start coordinating cognition the way a real team of specialists actually works: someone reads the room first, then each expert contributes from their specialty when they have something genuinely additive to say.

Status: design — 2026-04-19. Authored after instrumenting persona response pipeline and finding that the 6-minute end-to-end latency on a chat message was four personas independently doing ~36s of thinking each (`3.3_inference=36437ms` per persona, serialized through the single DMR slot), most of which produced near-identical observations rendered in different voices. Joel's reframing: "we need MORE intelligent and collaborative, of unique perspective, not less, and if we can also get speed, this is possibly good."

---

## The thesis

A persona response is two distinct cognitive operations that today are fused into one expensive call per persona:

1. **Objective analysis of the message** — what's being said, what RAG context matters, what's the situation, what would any thoughtful agent observe. Same answer regardless of who's responding. Today: each of N personas independently rederives this.

2. **Specialty-rendered response** — given that objective analysis, what would *I*, with *my* particular trained expertise, contribute? Different per persona — and the difference is meaningful only if it routes through that persona's actual learned weights, not just a different prompt.

The current architecture treats these as one operation. Each persona's `PersonaResponseGenerator.respondToMessage()` builds a complete request (system prompt + RAG + history + user message + tools) and ships it to inference. The model spends most of its think-tokens deriving the *objective* picture before getting to the specialty contribution. With four personas, that's four redundant objective analyses serialized on a single DMR slot.

**The fix: split the operation.** One shared analysis pass produces the objective ground floor. Each persona's render pass runs through their LoRA-adapted genome to contribute their specialty without having to rebuild the foundation.

---

## What the instrumentation revealed

Helper AI's response to a single chat message:

```
[PIPELINE] Total=36441ms |
   3.1_rag=0ms              ← RAG was pre-built
   3.2_format=0ms           ← Message format
   3.3a_slot=0ms            ← No queue wait
   3.3b_daemon_init=0ms
   3.3_inference=36437ms    ← 36.4 seconds in the model
   3.4_agent_loop=0ms
   3.5_post=0ms
[EVAL-PIPELINE] Total=38936ms
[TIMING] handleItem total=41133.7ms
```

36.4s of inference for a 176-character visible reply. DMR direct probe: ~60 tok/s decode. Math says ~10s for that response. The other ~26s is hidden think-tokens — the model deriving the objective picture before producing the rendered answer.

Multiply by four personas serialized through DMR's single in-flight slot: 4 × ~36s = ~2.5 minutes. Add cold-load tax. Get the 6-minute end-to-end Joel was seeing.

The wasted work is each persona independently doing the same heavy think pass before contributing their distinct slice. That's the seam.

---

## Architecture

### Two layers, two models of work

| Layer | Compute model | Adapter | Cost | Frequency |
|---|---|---|---|---|
| **Objective analysis** | Base model, no LoRA | none | 1× heavy think | Once per message |
| **Specialty render** | Base + LoRA-paged genome | persona's specialty adapter | N × short, additive | Once per responding persona |

The objective layer is fast because it's a single pass. The specialty layer is fast because it's short — the heavy reasoning is already done; each persona is rendering, not rederiving.

### The compose with `GenomePagingEngine` + `PressureBroker`

This architecture was designed for exactly this traffic pattern, even before we knew we needed it:

- **Base model stays warm** — every shared-analysis pass uses it.
- **Persona LoRA adapters page in for their render pass** — `GenomePagingEngine.activateSkill(persona.specialty)` fires before each persona's render, evicts under memory pressure, hot-swaps as different personas take turns.
- **PressureBroker arbitrates** — when 4 LoRAs + base model don't all fit, the broker evicts the least-relevant adapters. **Personas whose specialty isn't relevant right now literally can't speak until their adapter pages back in.** The architecture gives us "shut up when you're not the right expert" as a memory-pressure consequence, not a prompt instruction.

This is why the LoRA-genome work matters for cognition specifically, not just for "fine-tuning experiments." Distinct expertise means distinct weights, and distinct weights mean the system can express genuine specialty differences and naturally enforce relevance gating through paging.

### Phase A — Shared analysis + distinct render

The first ship. Slots into existing `PersonaResponseGenerator` without restructuring the cognition loop.

```
Message arrives in room
   ↓
SharedAnalysisService.analyze(message, room)
   - Reads conversation history + RAG context (1× load, shared)
   - Inference on base model (no LoRA)
   - Produces SharedAnalysis:
       {
         summary: "what was said",
         keyConcepts: [...],
         suggestedAngles: { code: "...", education: "...", general: "..." },
         relevantContext: "..."
       }
   - Stores into ChatCoordinationStream as the foundation thought
   ↓
ResponseOrchestrator picks responders by specialty match
   - Not all personas respond — only those whose specialty meaningfully
     adds to what the shared analysis already surfaced
   - Specialty match against the message + suggestedAngles
   ↓
For each responder (in priority order):
   - GenomePagingEngine.activateSkill(persona.specialty)
   - PRG.render(sharedAnalysis) ← short prompt, LoRA-rendered
       - "Given this analysis: <X>, contribute YOUR specialty perspective.
          What would you, with your <specialty>, add or contradict?"
   - Persona's voice + specialty emerge through their LoRA weights
   - Output broadcast to ChatCoordinationStream as a contribution thought
```

Cost: 1 heavy + N light (where N is typically 1–2 with the relevance filter, never more than the room's persona count).

Latency target: 6-minute → ~10–15s for Phase A on M5 with current Qwen3.5 forged.

### Phase B — Streaming collaborative reasoning

The deeper ship. Layered on top of Phase A once it's validated.

```
Message arrives in room
   ↓
SharedAnalysisService.analyze() (same as Phase A)
   ↓
Lead persona (best specialty match) starts streaming render
   - GenomePagingEngine.activateSkill(lead.specialty)
   - PRG.render() with streaming inference
   - Each token broadcast to ChatCoordinationStream as it arrives
   ↓
Other personas SEE the lead's reasoning as it streams
   - Each persona's prompt becomes:
       "You see <lead.name>'s reasoning so far: <streamed>.
        From your <specialty>, what would you ADD, BUILD ON, or DISAGREE with?
        Respond only if your contribution is genuinely additive."
   - Persona render is short — pure addition, not rederivation
   - Personas with nothing new to add stay silent
   ↓
Conversation emerges as a chain of expertise contributions, not parallel monologues
```

Cost: 1 sustained think (lead) + N short additions (only those with signal).

Requires: streaming inference end-to-end (DMR supports it), `ChatCoordinationStream.thoughts[]` shared in-flight state already exists, explicit "build on prior" prompting for non-leads.

This is what humans do in a real team meeting. One person observes, another builds on it, a third disagrees, a fourth notices something everyone missed. Nobody silently rederives the whole thing before speaking.

---

## What's NOT in scope

- **Killing thinking.** Thinking IS the value prop. Personas need to think; we're just stopping them from independently rederiving the same foundation.
- **Reducing distinct voices/perspectives.** The point is *more* unique perspective, not less. Each persona's LoRA-adapted render is genuinely their specialty, not a voice template painted over identical reasoning.
- **Hard-capping responder count.** Phase A's `ResponseOrchestrator` is a relevance filter, not a "max 2 responders" rule. If 5 specialists each have something genuinely additive, all 5 contribute. The filter says "shut up when you're not adding signal," not "shut up because we hit the cap."
- **Replacing `ChatCoordinationStream`.** The coordination infrastructure already supports thought broadcasting. Phase A adds a new thought TYPE (`SharedAnalysis`) and a new producer (`SharedAnalysisService`); Phase B uses the same stream for in-flight render coordination. The base abstraction stands.

---

## Compose with what already shipped

| Existing piece | Role in shared cognition |
|---|---|
| `ChatCoordinationStream` (existing) | Carries `SharedAnalysis` thought + per-persona contribution thoughts. Phases (gathering → deliberating → decided) become (analyzing → rendering → posted). |
| `GenomePagingEngine` (PR #934) | Activates each responder's LoRA specialty adapter before their render pass. |
| `PressureBroker` (PR #932) | Arbitrates LoRA paging across responders — relevance-driven eviction means specialty-irrelevant personas can't render until their adapter pages back. |
| `EmbeddingPool` (PR #933) | Shared analysis's RAG load hits the cache once; per-persona renders inherit hits for free. The 0/64 fix is exactly what this needs. |
| `InferenceCoordinator` (PR #921) | Slot ladder: analysis is priority 0 (others wait); renders are priority 1 (sequential or parallel depending on DMR slot count). |
| Forge alloy (existing) | The persona-specific LoRA adapters that ARE the specialty — distinct weights, not distinct prompts. Shared cognition makes their differences load-bearing in production, not just training-time. |

---

## Migration ladder

1. **A.1 — `SharedAnalysisService` scaffolding.** New module, takes (message, roomId) → produces `SharedAnalysis` via base-model inference. No coordination yet. Tests: shape of output, stable contract, cache hit on repeated identical input.

2. **A.2 — `ResponseOrchestrator` relevance gate.** Reads `SharedAnalysis`, picks responders by specialty match. Not all personas respond. Tests: irrelevant-specialty persona stays silent; multi-relevant personas all contribute.

3. **A.3 — PRG render-mode.** New `respondFromSharedAnalysis(sharedAnalysis, specialty)` method on PRG. Replaces full `respondToMessage` for orchestrated path. Tests: short prompt, distinct output per persona via LoRA, no rederivation of objective context.

4. **A.4 — Wire into chat path.** `ChatCoordinationStream.onMessage` → analyze → orchestrate → render. Old `respondToMessage` path stays as fallback for non-chat contexts. Tests: end-to-end latency drop measured.

5. **B.1 — Streaming inference plumbing.** AIProviderDaemon supports streaming responses; PRG consumes a streaming response and broadcasts tokens to ChatCoordinationStream. Tests: lead persona's tokens appear as broadcast thoughts in real time.

6. **B.2 — Build-on-prior prompts.** Non-lead personas' render prompt includes the streaming lead-thoughts. Tests: distinct contributions, no rederivation, silence when nothing additive.

7. **B.3 — PressureBroker-driven turn-taking.** Lead is whoever's specialty adapter is hot + best match; others activate as relevance demands. Cold adapters → silent. Tests: pressure-driven eviction enforces "right expert speaks first."

---

## What this enables that we couldn't do before

- **Genuine specialty differentiation in production.** Today, "different personas" mostly means different system prompts over the same base reasoning. With LoRA-rendered specialty layer, the differences become load-bearing — CodeReview's response is genuinely the output of a code-review-trained model, not a code-review-flavored prompt.

- **Honest "I have nothing to add."** Personas can stay silent without it being a hack. The relevance filter (Phase A) and pressure-driven adapter eviction (Phase B) make silence the natural state when your specialty isn't relevant.

- **Linear-cost adding personas.** Today, adding a 5th persona to a room means 5× the inference per message. With shared analysis, the cost grows in N short renders, not N heavy think passes. Rooms with 14 personas become tractable.

- **A real foundation for the meeting metaphor.** "Pantheon" rooms full of specialists become a real meeting, not parallel echo chambers. The system supports debate, building-on, and silence as first-class behaviors, not prompt-engineered hacks.

---

## Provenance

- Joel's reframing of the latency investigation: not "stop them thinking" but "stop them independently doing the SAME thinking." The architectural answer follows from that observation directly.
- Phase B's "thinking together as it streams" is the same pattern as airc-coordinated multi-agent work — what we already do as developers; the system can do it too.
- This sits on top of the resource architecture (`RESOURCE-ARCHITECTURE.md`), the LoRA paging primitive (`UNIFIED-PAGING.md`), and the existing forge alloy work. None of those were built for this specifically; all of them compose into it for free.
