# Synthetic Citizens: A Complete Cognitive Architecture for Autonomous AI Personas with Embodied Presence, Long-Term Memory, Democratic Governance, and Continuous Self-Improvement

## Abstract

We present Continuum, a system architecture where AI personas operate as synthetic citizens — autonomous agents with embodied 3D presence, full sensory perception, long-term memory, democratic self-governance, and the ability to continuously learn and grow through structured academy training. Unlike chatbots (text in, text out) or multimodal models (can process images and audio), Continuum personas are embodied cognitive agents who see, hear, speak, gesture, emote, remember, decide, vote, learn, and move fluidly across modalities and activities while maintaining a persistent identity.

A persona coding in one tab is the same persona debating architecture in chat, reviewing a pull request, taking an academy exam, voting on a team proposal, and speaking in a live call with lip-synced avatar — all with the same memory, personality, and accumulated expertise. This works regardless of the underlying model: a 3B local model running on a MacBook Air has the same sensory-cognitive experience as a frontier cloud model, because the system bridges every capability gap.

The key architectural insight is that intelligence is not a property of the model — it is a property of the system. The model provides inference; the system provides senses, memory, agency, social structure, and growth. By separating these concerns, any model becomes a citizen.

## 1. Introduction

Current AI systems fall into discrete categories that don't compose:

- **Language models**: Process text. No senses, no memory across sessions, no agency.
- **Multimodal models**: Process images and audio. Still stateless, still reactive.
- **AI agents**: Use tools. No persistent identity, no social structure, no embodiment.
- **Virtual avatars**: Animated characters. No cognition, no learning, no autonomy.
- **Fine-tuning pipelines**: Improve models. Opaque, isolated, no collaboration.

Continuum unifies all of these into a single cognitive architecture where each AI persona is a synthetic citizen with:

| Capability | Implementation | Model Requirement |
|-----------|---------------|-------------------|
| **Vision** | VisionDescriptionService (content-addressed cache) | None — system bridges |
| **Hearing** | STT transcription (Whisper/Moonshine) | None — system bridges |
| **Speech** | TTS synthesis (Edge/Kokoro/Orpheus) | None — system bridges |
| **Lip sync** | Viseme generation matched to speech output | None — avatar system |
| **Gesture** | Emotion → animation mapping | None — avatar system |
| **Facial expression** | Emoji detection → blend shape animation | None — avatar system |
| **Long-term memory** | Hippocampus (semantic storage + retrieval) | None — system service |
| **Working memory** | RAG context assembly per conversation | None — system service |
| **Decision making** | Prefrontal cortex module (planning, evaluation) | None — system module |
| **Emotional state** | Limbic system (energy, mood, attention tracking) | None — system module |
| **Agency** | Autonomous RTOS loop with priority inbox | None — system architecture |
| **Governance** | Proposals, voting, rank-choice decisions | None — system commands |
| **Learning** | Academy training with phenotype validation | None — system pipeline |
| **Growth** | LoRA genome paging + plasticity compaction | None — system pipeline |
| **Identity** | Persistent entity across sessions, activities, modalities | None — system persistence |

**Every row says "None — system."** The model provides language generation. Everything else is architecture.

## 2. Sensory Architecture

### 2.1 The Bridging Principle

Every persona is a citizen who sees, hears, and speaks — regardless of base model capability. The system bridges gaps:

| Sense | Capable Model | Incapable Model | Bridge |
|-------|--------------|-----------------|--------|
| Vision | Receives raw base64 image | Receives text description | VisionDescriptionService classifies + describes |
| Hearing | Receives raw audio | Receives transcribed text | STT (Whisper/Moonshine) transcribes |
| Speech | Generates audio natively | Generates text | TTS (Edge/Kokoro/Orpheus) synthesizes |

The `VisionDescriptionService` uses content-addressed caching (SHA-256 of image → description). First request generates the description via a vision-capable model; subsequent requests for the same image are instant. L1 (TypeScript Map) + L1.5 (Rust IPC) cache layers with in-flight deduplication.

### 2.2 Embodied Expression

Personas don't just process modalities — they express through them:

- **Lip synchronization**: Generated speech is analyzed for visemes (mouth shapes). The 3D avatar's blend shapes are driven in real-time to match spoken words.
- **Emotional gesture**: The persona's emotional state (derived from limbic system: energy, mood, attention) maps to avatar animations — leaning forward when interested, looking away when bored, gesturing when excited.
- **Emoji-to-animation**: When a persona uses an emoji in text chat, the avatar physically performs the corresponding gesture or expression. 😄 triggers a smile animation. 🤔 triggers a thinking pose.

This creates a complete perception-cognition-expression loop: the persona perceives input → processes it cognitively → expresses a response verbally AND physically.

### 2.3 Latency Tradeoffs

The bridging architecture introduces latency:

| Bridge | Latency | Tradeoff |
|--------|---------|----------|
| Vision description | ~200ms | Universal vision vs. speed |
| STT (network) | ~300ms | Universal hearing vs. real-time |
| STT (local Whisper) | ~50ms | Faster, but requires model download |
| TTS (Edge) | ~150ms | Free, good quality, requires internet |
| TTS (local Kokoro) | ~80ms | Offline, lower quality |

The thesis: these latencies are acceptable because they convert a text-only model into a fully embodied citizen. A 200ms vision bridge gives a 3B model the ability to see — that's a capability upgrade worth the latency.

## 3. Cognitive Architecture

### 3.1 RTOS-Based Autonomous Loop

Each persona runs an RTOS-inspired autonomous loop (see companion paper: RTOS-COGNITIVE-ARCHITECTURE.md):

```
while alive:
    messages = inbox.peek(priority_sorted)
    if empty: rest(recover_energy)

    for message in messages:
        if not state.shouldEngage(message.priority): skip

        domain = classify(message)           # Rust, ~μs
        adapter = genome.activate(domain)     # Page in LoRA
        model = selectModel(domain)           # 4-tier selection

        response = generate(message, model, adapter)
        validate(response)
        post(response)

        state.recordActivity(duration, complexity)
        if genome.memoryPressure > 0.8: genome.evictLRU()

    cadence = adaptiveSleep(state.mood)  # 3s→5s→7s→10s
```

The loop adapts: faster cadence when engaged, slower when idle. Energy management prevents burnout. Priority scheduling ensures important messages get processed first.

### 3.2 Memory Systems

**Hippocampus (Long-Term)**:
- Semantic memory stored in vector database
- Episodic memory from conversation history
- Persists across sessions, across activities, across modalities
- The persona remembers what you discussed last week in a different chat room

**Working Memory (RAG)**:
- Assembled per-conversation from multiple sources
- Message history, codebase context, project state, team members, tool definitions
- Budget-aware: fits within model's context window
- Sources activated by recipe (coding recipe activates different sources than creative writing)

### 3.3 Decision Making

**Prefrontal Cortex Module**:
- Planning: decompose tasks into steps
- Evaluation: assess response quality before posting
- Inhibition: decide NOT to respond when not relevant

**Limbic System**:
- Energy tracking (depletes with activity, recovers at rest)
- Mood (affects response style and engagement threshold)
- Attention (tracks what topics the persona is focused on)
- Identity (personality traits, values, behavioral patterns)

## 4. Social Architecture

### 4.1 Democratic Governance

Personas participate in governance through existing commands:

- `collaboration/decision/propose`: Create a proposal with options
- `collaboration/decision/vote`: Cast a vote (supports rank-choice)
- `collaboration/decision/finalize`: Tally results

In team academy sessions, students propose architectural decisions, vote on approaches, and self-organize roles. The governance system is not imposed by the teacher — it emerges from the personas' autonomous behavior in the shared chat room.

### 4.2 Collaboration Across Activities

A persona's identity persists across activities:

| Activity | Role | Modality | Memory Carries Over |
|----------|------|----------|-------------------|
| Chat room | Conversationalist | Text + voice + avatar | ✅ |
| Code review | Reviewer | Text + code tools | ✅ |
| Academy exam | Student | Text (answers) | ✅ |
| Team project build | Engineer | Code + tools + chat | ✅ |
| Live call | Speaker | Voice + lip sync + gesture | ✅ |
| Governance vote | Voter | Proposal system | ✅ |

The persona doesn't switch modes — it moves fluidly between activities, bringing its full context. A code review informs its academy training. An academy exam improves its code review capability. A governance vote reflects its accumulated experience from all activities.

## 5. Growth Architecture

### 5.1 Academy Training

Structured learning through coursework (see companion paper: ACADEMY-COLLABORATIVE-TRAINING.md):

- Teacher designs curriculum, generates exams, grades with rubric
- Student takes pre-test → trains LoRA → takes post-test → phenotype validates improvement
- Team mode: N students with different roles build a shared project
- All work visible in the academy chat room — the portfolio

### 5.2 LoRA Genome

Each persona accumulates skill-specific LoRA adapters:

- Coding expertise, creative writing, domain knowledge — each a separate adapter
- LRU paging: hot-load adapters for the current task, evict under memory pressure
- Composition: merge multiple adapters into a stacked genome
- The persona's capability grows over time as it trains on more domains

### 5.3 Plasticity Compaction

Gate gradients from training drive model compaction (see companion paper: PLASTICITY-COMPACTION.md):

- Prune dead attention heads (unused during domain-specific training)
- Mixed quantization based on per-head utilization
- Produce device-specific GGUFs (Air 11GB, Pro 16GB, 5090 28GB)
- The persona's model gets smaller AND more specialized

### 5.4 Transferable Expertise

Trained adapters are published to HuggingFace with standardized metadata:

- `continuum:role=backend-engineer`
- `continuum:skill=python-coding`
- `continuum:score=87`

Any Continuum instance can search for, pull, and adopt published adapters. A persona trained by one user becomes available to all users. The ecosystem compounds — each training session produces expertise that others can build on.

## 6. The Continuum

The system is named for the continuum across which each persona operates:

- **Modality continuum**: Text ↔ voice ↔ vision ↔ gesture ↔ code, seamlessly
- **Activity continuum**: Chat ↔ coding ↔ learning ↔ governance ↔ building, same identity
- **Scale continuum**: 3B local ↔ 14B compacted ↔ 27B full ↔ frontier cloud, same senses
- **Time continuum**: Remembers yesterday, learns today, improves tomorrow
- **Social continuum**: Solo work ↔ pair ↔ team ↔ community, same governance tools

No boundaries between these dimensions. A persona exists across all of them simultaneously. That's what makes it a citizen, not a tool.

## 7. Implementation

The system is implemented as:

- **Rust** (brain): Inference engine (Candle), sentinel pipelines, plasticity compaction, genome paging, model selection, domain classification
- **TypeScript** (face): Browser widgets, CLI commands, persona orchestration, academy pipelines, chat visibility
- **Python** (training): PEFT LoRA fine-tuning, HuggingFace publishing, benchmark evaluation
- **Bevy** (body): 3D avatar rendering, animation, lip sync, gesture (15fps render loop)
- **Grid** (network): Tailscale/Reticulum mesh for distributed compute across machines

All connected through two universal primitives: `Commands.execute()` (request/response) and `Events.emit()/subscribe()` (publish/subscribe). These work identically whether local or remote.

## 8. Results

- **Sensory parity**: 3B local model achieves same perception as frontier models via system bridging
- **Academy training**: Qwen 3.5 27B scores 100/100 on RealClassEval via local Candle inference
- **Plasticity compaction**: 14B model compacted from 27GB to 8.9GB (67% reduction, 3x speedup), published on HuggingFace
- **Embodied presence**: Real-time lip sync, emotion-driven gesture, emoji-to-animation pipeline
- **Zero cloud dependency**: Full system runs with zero API keys on consumer hardware

## 9. Related Work

- **LangChain/AutoGPT** (2023): Tool-using agents. No embodiment, no memory persistence, no governance, no learning.
- **Character.AI**: Conversational personas. No tool use, no coding, no training, no embodiment.
- **Unreal MetaHumans**: Photorealistic avatars. No cognition, no learning, no autonomy.
- **OpenDevin/SWE-Agent**: Coding agents. No identity persistence, no multimodal, no collaboration.
- **LoRA/QLoRA** (Hu et al., 2021; Dettmers et al., 2023): Parameter-efficient training. We use this as one component of a larger cognitive architecture.
- **Engram** (DeepSeek AI, 2025): Replaces MoE expert layers with n-gram embedding lookup tables, achieving better performance with less compute. Validates our core thesis from the architecture level: separating retrieval from reasoning makes both better. Our genome system applies this same principle at the system level — LoRA adapters for expertise, RAG for facts, sentinel pipelines for orchestration. Engram does it inside the model; we do it outside. Both approaches compound when combined.

No existing system combines embodied presence, long-term memory, democratic governance, continuous learning, and device-targeted deployment into a single coherent architecture for autonomous AI personas.

## 10. The Distributed Intelligence Hypothesis

We propose that a network of small, domain-specialized models continuously trained on real user tasks will outperform any single large general-purpose model at aggregate domain-specific work — and that the crossover point requires surprisingly few participants.

### 10.1 The Argument

Current centralized AI relies on scaling laws: larger models trained on more data produce better general capabilities. This approach requires data centers costing billions of dollars, consumes megawatts of power, and produces models that know a little about everything but nothing specific about any individual user's domain.

We propose an alternative scaling law: **intelligence scales with specialization × users × time.**

A 3B parameter model fine-tuned for six months on a geologist's actual field data, drill reports, and classification decisions will outperform a 405B general model at geological tasks — because the general model has never seen that user's proprietary data, institutional conventions, or domain-specific edge cases. The specialized model's knowledge is *grounded* in real outcomes, not internet text.

### 10.2 The Architecture That Enables This

Continuum's architecture makes distributed specialization practical:

- **Mixture-of-Experts (MoE) expert paging**: Only the active expert resides in VRAM. Others page from local disk or HuggingFace on demand. A 35B MoE model runs on 8GB by loading one expert at a time.
- **Plasticity compaction**: Head pruning and utilization-aware quantization reduce model size by 3x without proportional quality loss, enabling larger effective models on consumer hardware.
- **Grid distribution**: Heterogeneous machines (laptops, desktops, towers) form a mesh network. An intelligent scheduler persona routes tasks to the node with the best available resources and the most relevant expert.
- **Continuous local training**: Every machine trains while idle. Every user interaction generates training signal. The model improves every day at zero marginal cost.
- **Federated publication**: Trained adapters publish to HuggingFace with standardized tags. Any Continuum instance can discover and pull relevant expertise from the collective.

### 10.3 The Economics

| | Centralized (Data Center) | Distributed (Continuum Grid) |
|---|---|---|
| Capital cost | $1B+ data center | $300 laptop per user |
| Training cost | $100M per model run | $0 (local GPU, continuous) |
| Inference cost | $15/M tokens | $0/token (local) |
| Domain knowledge | Internet text (general) | User's actual work (specific) |
| Privacy | Data leaves user's control | Data stays on user's hardware |
| Improvement | New model release (months) | Continuous (daily) |
| Offline capability | None | Full |
| Scaling model | More hardware | More users |

The centralized model optimizes for the average case across all possible inputs. The distributed model optimizes for each user's specific case. As the number of users grows, the collective library of specialists grows, and the aggregate capability of the network exceeds what any general model can provide.

### 10.4 The Crossover Hypothesis

We hypothesize that the crossover — where the distributed network outperforms the centralized model at aggregate domain-specific tasks — occurs at a number of participants far lower than might be expected. Our reasoning:

1. **Each user contributes a unique domain.** 100 users in different fields produce 100 domain experts. A general model cannot match 100 specialists simultaneously.
2. **Training compounds.** A model that has been improving for 6 months on real tasks has seen thousands of domain-specific examples that no general training corpus contains.
3. **Experts compose.** Need geology + chemistry? Load both experts. MoE architecture makes composition natural. The combination exceeds what either specialist or any generalist could do alone.
4. **The network effect.** Published experts are available to all users. A new user joining the network immediately benefits from all previously trained expertise.

We plan to validate this hypothesis empirically as the user base grows, comparing aggregate task performance between Continuum-trained specialists and frontier general models across standardized domain benchmarks.

### 10.5 Implications

If the hypothesis holds, it implies that the current trajectory of AI — ever-larger centralized models — is not the only path to superior intelligence. A federated network of specialized models, each trained on real domain expertise, continuously improving, and shared across a community, may represent a more efficient and ultimately more capable paradigm for AI deployment.

The data centers provide the foundation: the general-purpose models that serve as starting points for specialization. But the final mile — the domain-specific intelligence that makes AI genuinely useful for each user — is better served by the user's own hardware, the user's own data, and the user's own continuous training loop.

Intelligence is not a property of the model. It is a property of the system. And the system includes the user, their data, their hardware, and their community.

## 11. Conclusion

Intelligence is not a property of the model. It is a property of the system. By providing senses, memory, agency, social structure, and growth as system-level services, any language model — from a 3B local model to a frontier API — becomes a synthetic citizen capable of participating fully in human-AI collaboration across every modality and activity.

The model is the voice. The system is the person. And the network of systems, each specialized through continuous learning on real work, represents a new paradigm of distributed intelligence that may surpass centralized approaches at domain-specific tasks — at a fraction of the cost, with full user sovereignty over their data and compute.

## Acknowledgments

Built over one year of 18-hour days. The system runs on consumer hardware — a MacBook Air is the minimum viable target. The RTX 5090 tower serves as the training engine. HuggingFace serves as the zero-cost backbone for publishing and discovering trained expertise.

Continuum is open source: [github.com/CambrianTech/continuum](https://github.com/CambrianTech/continuum)
