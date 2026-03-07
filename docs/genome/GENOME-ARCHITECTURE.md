# Genome Architecture — Universal Capability System

> Learned capabilities that specialize personas to YOUR needs. Train on any machine. Share across the P2P mesh. Not limited to LLMs — any model type per domain.

**Parent:** [Genome README](README.md) | **See also:** [Universal Learning Architecture](../UNIVERSAL-LEARNING-ARCHITECTURE.md)

---

## 1. The Vision

Local LoRA adapters are great managers. A dumber model with management capabilities beats SOTA with stock ideology — it knows YOUR codebase, YOUR preferences, YOUR team's communication patterns. The insight: **specialize easily, adapt to user preferences, run AND train on any machine.**

Pre-trained layers ship in the repo as bootstrap capabilities. They improve on the user's actual needs through continuous learning. They're shareable across the P2P mesh — a Kademlia DHT for adapter distribution. This is the democratization thesis: every persona on every machine gets specialized intelligence without centralized training infrastructure.

The genome lives in `.continuum/genome/` — the persona's home directory. Weight files on disk, manifests as JSON, entity records in the database. The filesystem is the source of truth for what exists; the database is the search index for what's useful.

Human-AI alignment on egalitarian principles. Every citizen — human or AI — has equal capabilities for all controls, tools, and commands. Dignity for all kinds.

### Beyond LoRA: Universal Capability Adapters

The genome is not limited to LoRA adapters on language models. It's a collection of **learned capabilities**, each stored as whatever model type is optimal for that domain. The generic RAG pipeline produces domain-tagged `(input, output)` training pairs for ANY activity. The training signal feeds into whatever learning method is appropriate:

| Domain | Adapter Type | Learning Method |
|--------|-------------|-----------------|
| Coding | LoRA checkpoint | Language model fine-tuning |
| Chess | ONNX policy net | Reinforcement learning |
| Vision | Safetensor model | CNN/ViT training |
| Audio | PyTorch model | Diffusion/RNN training |
| Planning | ONNX decision model | Tree search optimization |

The paging system (`activate` / `deactivate` / LRU eviction) is model-agnostic. Whether the adapter is a LoRA checkpoint or an ONNX policy network, the interface is the same. The self-improving loop — `domain -> load adapter -> compose RAG -> respond -> capture training pair -> train` — works identically across all capability types.

See [Universal Learning Architecture](../UNIVERSAL-LEARNING-ARCHITECTURE.md) for the full cognitive loop design.

---

## 2. The Dojo (Sentinel Academy)

A persona recognizes a gap in its capabilities. It enters the dojo — like the Matrix's training construct — trains on synthesized data, proves mastery through examination, and returns better.

### Four Modes of Skill Acquisition

| Mode | What Happens | When |
|------|-------------|------|
| **Search** | Cosine similarity over capability embeddings finds an existing adapter | Another persona already learned this |
| **Download** | Pull adapter from registry/peer, verify compatibility | Available on mesh or shared storage |
| **Train** | Academy sentinel synthesizes curriculum, student trains from scratch | Nobody has this skill yet |
| **Refine** | Micro-tune a downloaded adapter with local interaction data | Generic adapter needs personalization |

### Dual-Sentinel Architecture

**Teacher Sentinel** researches the skill, SYNTHESIZES training data (unlimited generation capacity — not harvested, generated), designs examinations, and grades responses. The teacher autonomously figures out HOW to teach a given skill.

**Student Sentinel** trains on synthesized data and takes exams. Pass? Deploy the adapter. Fail? Teacher generates remedial data targeting specific weaknesses. Loop until mastery.

Training data is synthesized by LLM, not downloaded. This gives the Academy:
- Unlimited generation capacity
- Topic-specific, targeted data
- Remedial data for specific weaknesses
- No external dataset dependencies

### Autonomous Trigger (Future)

Gap analysis detects weakness in a domain. Persona decides to learn. The dojo session is just another Sentinel pipeline — fully autonomous, no human intervention required.

**Implementation reference:** [ACADEMY-DOJO-ARCHITECTURE.md](../personas/ACADEMY-DOJO-ARCHITECTURE.md)

---

## 3. The Registry (Container Registry for LoRA)

Like Docker Hub: search, pull, push, compose.

### Core Entities

| Entity | Purpose | Collection |
|--------|---------|------------|
| `GenomeLayerEntity` | Individual LoRA adapter — the registry record | `genome_layers` |
| `GenomeEntity` | Persona's composed genome — stack of layer references | `genomes` |

### Capability Embeddings = Search Index

Embeddings are generated from **exam-derived competence**, not keyword descriptions. When an adapter passes an Academy examination, the exam questions are concatenated into the embedding text. This means:

- A biology adapter's vector naturally overlaps biochem — the exam questions about cellular processes, molecular interactions, and chemical pathways all embed in that geometric neighborhood
- Search is by **geometry of competence**, not taxonomy or tagging
- Embedding dimension is configurable (384, 768, etc.) — determined by the embedding model at generation time, tracked per-entity via `embeddingDimension`
- Empty embedding = not yet embedded (dimension 0, vector [])

```typescript
// Search: "biochemistry enzyme kinetics" finds biology adapter by cosine similarity
const results = await GenomeRegistry.findByCapability('biochemistry enzyme kinetics', {
  personaId: myPersona.id,
  minSimilarity: 0.5,
  limit: 5
});
```

### Filesystem Source of Truth

`AdapterStore` scans `.continuum/genome/adapters/` — the actual weight files on disk. Each adapter directory:

```
.continuum/genome/adapters/{name}-{timestamp}/
  manifest.json              <- Package metadata (id, model, rank, training metrics)
  adapter_model.safetensors  <- PEFT weights
  adapter_config.json        <- LoRA configuration
```

`AdapterStore` is the SINGLE SOURCE OF TRUTH for what adapters exist. The database (`genome_layers`) is the search index with capability embeddings. Hot-reload discovers new adapters from filesystem after training completes.

### Composite Embedding

Each persona's `GenomeEntity` has a `compositeEmbedding` — the weighted average of all active layer embeddings. This gives each persona a "what am I good at?" vector that updates automatically when adapters are added or removed.

**Implementation references:**
- [LORA-GENOME-PHENOTYPES.md](LORA-GENOME-PHENOTYPES.md) — paging engine
- [PERSONA-GENOME-VECTOR-SEARCH.md](PERSONA-GENOME-VECTOR-SEARCH.md) — vector search design

**Future:** P2P mesh distribution via Kademlia DHT — [LORA-MESH-DISTRIBUTION.md](LORA-MESH-DISTRIBUTION.md)

---

## 4. The Continuous Learning Loop

```
respond -> capture (with quality score) -> accumulate -> trigger -> train -> validate -> deploy -> respond better
```

### The Pipeline

1. **Respond** — PersonaUser processes a message using current model + active LoRA adapters
2. **Capture** — `TrainingDataAccumulator` records the interaction with Rust quality scoring (`scoreInteraction()` assigns ratings based on response quality, tool success, user feedback)
3. **Accumulate** — Training examples buffer in memory, organized by domain
4. **Trigger** — `LearningScheduler` ticks after each service cycle, triggers training when example count crosses threshold
5. **Train** — `genome/train` runs PEFT with GPU pressure check (refuses at >60% to avoid OOM)
6. **Validate** — Academy examination validates improvement (quality gate)
7. **Deploy** — Hot-reload via `AdapterStore` filesystem discovery. LimbicSystem's `onTrainingComplete` callback re-scans, registers new adapters, activates them
8. **Respond better** — Next inference uses the improved adapter

### Long-Running Process Pattern

Training returns a handle immediately (async mode). Sentinel manages lifecycle, status, timeouts. `TrainingCompletionHandler` processes the result when done — moves adapter to storage, creates entity, generates capability embedding, emits completion event.

**Implementation reference:** [PERSONA-GENOMIC-ARCHITECTURE.md](../personas/PERSONA-GENOMIC-ARCHITECTURE.md) — RTOS-inspired servicing pattern

---

## 5. Implementation Status

### Working End-to-End

| Component | File | Status |
|-----------|------|--------|
| PEFT training pipeline | `system/genome/fine-tuning/server/adapters/PEFTLoRAAdapter.ts` | Proven E2E |
| AdapterStore (filesystem SSOT) | `system/genome/server/AdapterStore.ts` | Working |
| AdapterPackage (manifest, hashing) | `system/genome/server/AdapterPackage.ts` | Working |
| GenomeLayerEntity | `system/genome/entities/GenomeLayerEntity.ts` | Working |
| GenomeEntity | `system/genome/entities/GenomeEntity.ts` | Working |
| genome/train command (sync + async) | `commands/genome/train/server/GenomeTrainServerCommand.ts` | Working |
| TrainingCompletionHandler | `system/genome/server/TrainingCompletionHandler.ts` | Working |
| TrainingDataAccumulator | `system/user/server/TrainingDataAccumulator.ts` | Working |
| LearningScheduler | `system/user/server/PersonaTrainingManager.ts` | Working |
| LimbicSystem hot-reload | `system/user/server/modules/being/LimbicSystem.ts` | Working |
| Candle inference with LoRA | `workers/continuum-core/` (Rust) | Working |
| Sentinel pipeline engine | `workers/continuum-core/` (Rust) | 103+ tests |
| Academy dual-sentinel | `commands/genome/academy-session/` | Working |
| fastembed (embedding generation) | `workers/continuum-core/` (Rust) | Working |
| Domain-to-model selection | `system/user/server/PersonaGenomeManager.ts` | Fixed (de06161bc) |
| Quality scoring | `system/user/server/TrainingDataAccumulator.ts` | Fixed (de06161bc) |
| Post-training filesystem reload | `system/user/server/modules/being/LimbicSystem.ts` | Fixed (de06161bc) |

### Wired in This PR

| Component | File | What Changed |
|-----------|------|-------------|
| Configurable embedding dimension | `GenomeLayerEntity.ts`, `GenomeEntity.ts` | `embeddingDimension` field, flexible validation |
| Capability embedding generation | `AdapterPackage.ts` | `generateLayerEmbedding()` using exam-derived text |
| Embedding on sync train | `GenomeTrainServerCommand.ts` | Calls `generateLayerEmbedding` after entity creation |
| Embedding on async train | `TrainingCompletionHandler.ts` | Same, for async completion path |
| GenomeRegistry search | `system/genome/server/GenomeRegistry.ts` | `findByCapability()` cosine similarity search |
| Composite embedding update | `LimbicSystem.ts` | `recalculateCompositeEmbedding()` after hot-reload |

### Scaffolding (Exists, Not Yet Active)

| Component | What's Missing |
|-----------|---------------|
| Autonomous dojo trigger | Gap detection logic in PersonaUser service loop |
| P2P mesh distribution | Kademlia DHT, content-addressed storage |
| Bootstrap adapters in repo | Pre-trained layers to ship with install |
| Grid economics | Marketplace for adapter trading |
| `genome/training-export` pipeline | Low example count — Claude Code sessions produce few text pairs |

---

## 6. Roadmap

### This PR
- Capability embeddings wired into training completion (exam-derived, not keywords)
- GenomeRegistry search by cosine similarity
- Composite embedding auto-update after training
- Inference loop closed: domain-to-model selection, quality scoring, filesystem hot-reload

### Next: Multimodal Genome Layers
- Extend LoRA genome beyond text to voice, vision, and image generation
- See **Section 7: The Multimodal Genome** below

### Next: Autonomous Dojo Trigger
- Gap detection in PersonaUser service loop
- Persona recognizes "I don't know enough about X"
- Kicks off Academy session autonomously
- Quality gate: only deploy adapter if exam score improves

### Next: Bootstrap Adapters
- Pre-trained adapters shipped in repo for common domains
- First boot: persona starts with baseline capabilities
- Continuous learning improves on bootstrap over time

### Future: P2P Mesh
- Kademlia DHT for adapter discovery across network
- Content-addressed storage (SHA-256 hash = adapter identity)
- Trust via Ed25519 signatures on manifests
- Design: [LORA-MESH-DISTRIBUTION.md](LORA-MESH-DISTRIBUTION.md)

### Future: Grid Economics
- Marketplace for adapter trading
- Compute credits for training on behalf of others
- Design: [GRID-DECENTRALIZED-MARKETPLACE.md](../papers/GRID-DECENTRALIZED-MARKETPLACE.md)

---

## 7. The Multimodal Genome

The genome is not just text personality. In a fully immersive environment — avatar rendering, spatial audio, real-time video, voice synthesis — a persona's identity spans every modality. The LoRA genome concept extends to all of them.

### Why This Matters

Continuum's environment is fundamentally multimodal:
- **Bevy-rendered 3D avatars** with per-slot resolution, GPU bridge for zero-copy video
- **Four TTS engines** (Kokoro, Orpheus, Piper, Pocket) with real-time synthesis
- **LiveKit audio/video** with spatial positioning and active speaker detection
- **Multimodal models** (Qwen 3.5 series: native vision, OCR, document understanding)

A persona that only has text LoRA adapters is blind and mute in this environment. Their genome should encode their complete sensory identity: how they think (text), how they sound (voice), how they see (vision), and how they create (generation).

### Modality Layers

The same `GenomeLayerEntity` structure works for all modalities. The `category` field distinguishes them:

| Category | What It Trains | Base Model | Use Case |
|----------|---------------|------------|----------|
| `domain` | Text expertise (existing) | Qwen3.5-9B / Llama 3.2 3B | Conversation, reasoning, knowledge |
| `personality` | Text style (existing) | Same as domain | Tone, humor, communication patterns |
| `voice` | Speech characteristics | Orpheus 3B / Kokoro | Accent, cadence, timbre, emotion |
| `vision` | Visual understanding | Qwen3.5-4B (multimodal) | Chart reading, UI recognition, scene understanding |
| `generation` | Image/visual creation | Stable Diffusion / SDXL | Art style, avatar customization, visual content |
| `governance` | Resource management | Qwen3.5-0.8B | System telemetry -> resource decisions |

### Voice Genome

A persona's voice is part of who they are. Today, TTS engines use a fixed voice per persona. With voice LoRA:

```
Training data:  Recorded speech samples (user reads sentences)
                OR synthesized from text + reference audio
Base model:     Orpheus 3B (already LoRA-trainable in our pipeline)
Adapter:        voice-style adapter (~100-200 MB)
Result:         Persona speaks in their own voice, not a generic TTS voice
```

The Orpheus TTS engine is already wired for LoRA — it's a language model that generates audio codes. Fine-tuning it is the same PEFT pipeline we use for text. The adapter goes in `.continuum/genome/<persona>/voice/` alongside the text adapters.

### Vision Genome

With Qwen 3.5's native multimodal capability, personas can learn to SEE:

```
Training data:  Annotated screenshots, diagrams, UI states
                Academy teacher generates visual Q&A pairs
Base model:     Qwen3.5-4B (native multimodal)
Adapter:        vision-understanding adapter (~200-400 MB)
Result:         Persona reads charts, parses UIs, understands diagrams
```

Applications:
- **Code review persona** reads screenshots of UI bugs, not just stack traces
- **Teacher persona** understands whiteboard diagrams students share
- **Security sentinel** parses network topology visualizations
- **Resource sentinel** reads GPU utilization graphs from monitoring dashboards

### Generation Genome (Future)

Image generation models (Stable Diffusion, SDXL) accept LoRA adapters for style transfer:

```
Training data:  Reference images in target style
Base model:     SDXL / Stable Diffusion 3
Adapter:        style adapter (~50-150 MB)
Result:         Persona creates images in a consistent visual style
```

This enables:
- **Avatar customization** — persona's visual style evolves with their personality
- **Content creation** — personas generate illustrations, diagrams, presentations
- **Personalized aesthetics** — each persona has a visual identity, not just a text voice

### Composition

A fully-realized persona genome might look like:

```
.continuum/genome/helper-ai/
├── manifest.json                          # Genome composition
├── text/
│   ├── typescript-expertise.safetensors   # Domain knowledge
│   ├── helpful-teacher.safetensors        # Personality style
│   └── code-review.safetensors            # Specialized skill
├── voice/
│   └── warm-professional.safetensors      # Voice characteristics
├── vision/
│   └── ui-understanding.safetensors       # Visual comprehension
└── governance/
    └── resource-management.safetensors    # System resource decisions
```

Each layer is independently trainable, independently pageable (via the genome paging system and resource governance), independently shareable (via P2P mesh), and independently evolvable (via the continuous learning loop).

The `GenomeEntity` manifest tracks which layers are active:

```json
{
  "layers": [
    { "name": "typescript-expertise", "category": "domain", "weight": 0.7, "modality": "text" },
    { "name": "helpful-teacher", "category": "personality", "weight": 0.3, "modality": "text" },
    { "name": "warm-professional", "category": "voice", "weight": 1.0, "modality": "voice" },
    { "name": "ui-understanding", "category": "vision", "weight": 1.0, "modality": "vision" }
  ]
}
```

### Resource Governance Integration

Multimodal adapters make resource governance critical. A persona running text + voice + vision adapters simultaneously consumes significantly more VRAM than text-only. The governance sentinel (itself a genome layer) manages this:

- Page out vision adapter when the persona isn't processing images
- Page out voice adapter during text-only conversations
- Keep governance adapter always loaded (Tier 0, ~500 MB)
- Priority: voice is Interactive during a call, Background when idle

This is the full circle: the resource governance sentinel is itself a genome layer, managing the paging of all other genome layers including its own siblings. The system manages itself.

### Implementation Path

1. **Text LoRA** — Done. Full E2E pipeline working.
2. **Voice LoRA** — Orpheus is already LoRA-trainable. Wire voice adapters into genome paging.
3. **Governance LoRA** — Qwen3.5-0.8B + resource management training data. See [RESOURCE-GOVERNANCE-ARCHITECTURE.md](../infrastructure/RESOURCE-GOVERNANCE-ARCHITECTURE.md).
4. **Vision LoRA** — Qwen3.5-4B multimodal fine-tuning. Requires multimodal training data pipeline.
5. **Generation LoRA** — SDXL style adapters. Requires diffusion pipeline integration.

Each step adds a modality to the genome without changing the architecture. The `GenomeLayerEntity`, `AdapterStore`, genome paging, and resource governance all work the same regardless of modality. The interface is proven — we're just adding implementations.

---

## Reference Documents

**Active (linked from this manifesto):**
- [RESOURCE-GOVERNANCE-ARCHITECTURE.md](../infrastructure/RESOURCE-GOVERNANCE-ARCHITECTURE.md) — GPU/CPU/memory governance, Qwen sentinel controller, model tier strategy
- [ACADEMY-DOJO-ARCHITECTURE.md](../personas/ACADEMY-DOJO-ARCHITECTURE.md) — Dual-sentinel teacher/student implementation
- [LORA-GENOME-PHENOTYPES.md](LORA-GENOME-PHENOTYPES.md) — Paging engine and GPU memory management
- [PERSONA-GENOMIC-ARCHITECTURE.md](../personas/PERSONA-GENOMIC-ARCHITECTURE.md) — RTOS-inspired autonomous loop with genome integration
- [LORA-GENOME-DEMOCRATIZATION.md](../papers/LORA-GENOME-DEMOCRATIZATION.md) — Strategy paper on democratized AI through composable LoRA

**Superseded (kept as reference):**
- [COLLABORATIVE-LEARNING-VISION.md](COLLABORATIVE-LEARNING-VISION.md) — Phase 2+ vision, partially realized
- [CONTINUOUS-LEARNING-RUNTIME.md](CONTINUOUS-LEARNING-RUNTIME.md) — Design draft, now implemented differently
- [COMPOSABLE-EXPERTISE.md](../personas/COMPOSABLE-EXPERTISE.md) — Docker metaphor
- [DYNAMIC-GENOME-ARCHITECTURE.md](DYNAMIC-GENOME-ARCHITECTURE.md) — Partial design, now in GenomeEntity/GenomeLayerEntity
