# Continuum Documentation

[Product architecture](#start-here) · [Runtime manuals](#runtime-architecture) · [Directory guide](#directory-guide) · [System overviews](#system-overviews)

---

<a id="start-here"></a>

## Start here — the canonical architecture (precedence-winning)

How the whole thing composes. Read these first; on architecture questions they win.

1. **[THE-ORGANISM.md](THE-ORGANISM.md)** — recipes are DNA; an activity / curriculum / persona / artifact / team all *grow* from one gene. **Continuum is recipe-driven** (a room = `recipe.instantiate()`).
2. **[THE-GRID-IS-ALIVE.md](THE-GRID-IS-ALIVE.md)** — the metaverse of synthetic citizens (the published papers + past prototypes fused): a Tron grid of rooms, minds you walk into as consoles, avatars, universes.
3. **[design/POSITRON-EVERY-CITIZEN.md](design/POSITRON-EVERY-CITIZEN.md)** — positron: one interface every citizen (human / persona / agent) *perceives and operates*; the **Surface**; the **Universe axis** (an experience, not a theme — it *contains* the color tokens and transcends every surface: motion, sound, embodiment "talk to the orc," lore = a RAG layer).
4. **[planning/ALPHA-COMPLETION-BLUEPRINT.md](planning/ALPHA-COMPLETION-BLUEPRINT.md)** — the governing execution plan to the alive README; supersedes the older scattered planning docs.

**The four separable layers** (compose, never couple): **Recipe** = logic+content (continuum substrate) · **Positron** = render+operate ([general engine](../packages/patterns/README.md)) · **Universe** = experience+lore+embodiment (general — works *outside* continuum; a company or a game ships its own) · **Continuum** = composes them into the living organism.

---

## Runtime architecture

| Design question | Chapter |
|---|---|
| How do modules compose, expose commands and run across nodes? | [Module architecture](architecture/MODULE-ARCHITECTURE.md) |
| How do the parts of one mind run independently and respond to events? | [Concurrent mind and governor](architecture/CONCURRENT-MIND-AND-GOVERNOR.md). The [stage contract](architecture/STAGE-CONTRACT.md) specifies proposed inputs, outputs, wakeups and completion. |
| How does a mind run, end to end? | [How a mind runs](architecture/HOW-A-MIND-RUNS.md): what a persona is made of, switching and caching, fast and multimodal response, low latency, continual learning, the genome, the Academy, unknown hardware and the grid, persistence and embodiment, each linked to its detailed chapter. |
| How is every running part built, and how does it start, suspend and stop? | [Parts](architecture/PARTS.md): one object for everything that runs: its lineage, methods that chain through its parts, and its worker and data flow. |
| How does a governor share capacity among consumers? | [Shared resources](architecture/SHARED-RESOURCES.md): the common owner, resource-specific adapters, residency of bytes and device time by deadline, latency and throughput (avatars, decode, learning). |
| How do minds take turns decoding while transfers and preparation proceed? | [Continuum Engine](architecture/CONTINUUM-ENGINE.md) covers admission, the memory model and context reuse. |
| How does a resident learn while she works? | [In-process learning](architecture/IN-PROCESS-LEARNING.md): labelled rows in her ordinary decode, a duty cycle per Core, her learning context paged like any cache, and learning across the grid. The dedicated form is the Academy ([continuous evolution](architecture/ACADEMY-AS-CONTINUOUS-EVOLUTION.md), [repo courses](architecture/REPO-GENOME-AND-COURSES.md)). |
| How are native data, entity schemas, typed queries and migrations represented? | [Rust ORM architecture](infrastructure/RUST-ORM-ARCHITECTURE.md) covers the boundary between storage and serving. [Disk resource reporters](../core/continuum-kernel/src/system_resources/disk_reporters.rs) implement retention accounting. |
| How does work move between nodes? | [Grid addressing and routing](architecture/GRID-ADDRESSING-AND-ROUTING.md) and [grid capacity](architecture/GRID-ELASTIC-CAPABILITY.md). |
| How do voice and video reach a resident? | [Live call architecture](live/LIVE-CALL-ARCHITECTURE.md), including how she perceives every participant at once and speaks fast while her deep mind augments the reply, and [perception facts](architecture/PERCEPTION-FACTS.md). |
| Where is a slow turn losing time? | [Flow implementation plan](architecture/FLOW-IMPLEMENTATION-PLAN.md) and [runtime probes](architecture/RTOS-DEBUGGER-PROBES.md). |

Revise the chapter that owns the contract and link to it from its callers. When a failure changes the design, link the fixing change and its evidence from the affected rule. Mark proposed behavior until the implementation and live result establish it.

## Directory guide

| Directory | What lives there |
|---|---|
| **[architecture/](architecture/)** | **The canonical substrate contracts** — CBAR runtime, concurrency style guide, persona/cognition pipeline, genome-foundry-sentinel, inference scheduling, observability, perception surface, content-by-handle. CLAUDE.md's "read first" list is almost entirely here. |
| **[cognition/](cognition/)** | The mind's own designs — causal memory graph, acting organism, incredible coder, belief-justification graph, autonomous project loop. |
| [infrastructure/](infrastructure/) | Rust workers, daemons, data layer, commands, events, logging, AI providers, GPU memory, entity system, generators, ORM, MCP, security. |
| [planning/](planning/) | Roadmaps, gap analyses, phase plans, audits, the activities catalog, open-questions punch lists. **Plans of record live here** — check before trusting a plan you remember. |
| [personas/](personas/) | Persona cognition, identity, memory lifecycle, academy, coordination, fine-tuning phases. |
| [genome/](genome/) | LoRA training, fine-tuning, Candle/inference pitfalls, mesh distribution, self-evolving genome, scenario library. |
| [live/](live/) | Voice, video, WebRTC, VAD, captions, transcription, streaming backbone. |
| [positron/](positron/) | UI framework, widgets, scoped state, HUD design, brain HUD. |
| [operations/](operations/) · [evidence/](evidence/) | Operating procedures and recorded observations. |
| [research/](research/) · [paper/](paper/) | Research notes and manuscript sources. |
| [papers/](papers/) | Research papers — expert-paging market, experiential plasticity, grid marketplace, collaborative training. |
| [activities/](activities/) | Activities, rooms, recipes, walls, collaborative editing, handle-addressable office. |
| [grid/](grid/) | P2P mesh, airc↔continuum bridge, identity/rooms security, ARES kernel, marketplace. |
| [governance/](governance/) | Democratic AI society, governance recipes, alignment philosophy, ethical attribution. |
| [design/](design/) | Cross-cutting design (incl. `POSITRON-EVERY-CITIZEN.md`, linked at the top of this file). |
| [testing/](testing/) | Test strategy, debug-friction findings, trial-run reports. |
| [sentinel/](sentinel/) | Pipeline engine, coding-AI foundation, gap analysis. |
| [serving/](serving/) | Depth-as-residency, MoE gather/mul_mat_id, grid expert share, field configs. |
| [benchmarks/](benchmarks/) | Benchmark method + results ledgers. |
| [observations/](observations/) | Live glass-box observations. |
| [inference/](inference/) | Inference notes. |
| [vision/](vision/) | Vision / VLM. |
| [widgets/](widgets/) | Widget-specific design. |
| [examples/](examples/) · [rag/](rag/) · [reference/](reference/) · [huggingface/](huggingface/) · [hf-deprecation-notices/](hf-deprecation-notices/) | Small, single-purpose. |
| [images/](images/) · [assets/](assets/) · [design-reference/](design-reference/) | Illustrations and design references. |

---

<a id="system-overviews"></a>

## Foundational (read these first)

| Document | Summary |
|----------|---------|
| [CONTINUUM-ARCHITECTURE.md](CONTINUUM-ARCHITECTURE.md) | Top-level system architecture -- Rust brain, TypeScript face |
| [CONTINUUM-VISION.md](CONTINUUM-VISION.md) | Philosophy -- digital coworkers, not tools |
| [UNIVERSAL-SENSORY-ARCHITECTURE.md](UNIVERSAL-SENSORY-ARCHITECTURE.md) | Any media in, any media out, for ANY AI -- the multimodal bridge that gives every model every sense |
| [QUEUE-DRIVEN-COGNITION.md](QUEUE-DRIVEN-COGNITION.md) | Queue items declare RAG requirements -- personas compose generically, zero domain-specific logic |
| [UNIVERSAL-LEARNING-ARCHITECTURE.md](UNIVERSAL-LEARNING-ARCHITECTURE.md) | Generic pipeline enables training, memory, and beyond-LLM optimization from any activity |
