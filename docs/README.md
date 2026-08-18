# Continuum Documentation

**Organized by theme — each chapter is a self-contained concern.**

---

## Start here — the canonical architecture (precedence-winning)

How the whole thing composes. Read these first; on architecture questions they win.

1. **[THE-ORGANISM.md](THE-ORGANISM.md)** — recipes are DNA; an activity / curriculum / persona / artifact / team all *grow* from one gene. **Continuum is recipe-driven** (a room = `recipe.instantiate()`).
2. **[THE-GRID-IS-ALIVE.md](THE-GRID-IS-ALIVE.md)** — the metaverse of synthetic citizens (the published papers + past prototypes fused): a Tron grid of rooms, minds you walk into as consoles, avatars, universes.
3. **[design/POSITRON-EVERY-CITIZEN.md](design/POSITRON-EVERY-CITIZEN.md)** — positron: one interface every citizen (human / persona / agent) *perceives and operates*; the **Surface**; the **Universe axis** (an experience, not a theme — it *contains* the color tokens and transcends every surface: motion, sound, embodiment "talk to the orc," lore = a RAG layer).
4. **[planning/ALPHA-COMPLETION-BLUEPRINT.md](planning/ALPHA-COMPLETION-BLUEPRINT.md)** — the governing execution plan to the alive README; supersedes the older scattered planning docs.

**The four separable layers** (compose, never couple): **Recipe** = logic+content (continuum substrate) · **Positron** = render+operate ([general engine](../packages/patterns/README.md)) · **Universe** = experience+lore+embodiment (general — works *outside* continuum; a company or a game ships its own) · **Continuum** = composes them into the living organism.

---

## Where things live — EVERY directory, with counts

595 markdown files. This table is the map; it is the thing to fix first when it goes
stale, because a stale map is why people rebuild what already exists.

> **How this index went wrong, so it doesn't again** (2026-08-18): the previous version
> was last touched 2026-03-04, listed 13 of 28 directories, and omitted **`architecture/`
> entirely** — the largest directory (120 docs) and the one CLAIMED.md sends you to FIRST
> for every precedence-winning canonical doc. An index that omits the canonical directory
> is worse than no index: it looks authoritative and quietly hides the thing you need.

| directory | docs | what lives there |
|---|---:|---|
| **[architecture/](architecture/)** | 120 | **The canonical substrate contracts** — CBAR runtime, concurrency style guide, persona/cognition pipeline, genome-foundry-sentinel, inference scheduling, observability, perception surface, content-by-handle. CLAUDE.md's "read first" list is almost entirely here. |
| **[cognition/](cognition/)** | 21 | The mind's own designs — causal memory graph, acting organism, incredible coder, belief-justification graph, autonomous project loop. |
| [infrastructure/](infrastructure/) | 107 | Rust workers, daemons, data layer, commands, events, logging, AI providers, GPU memory, entity system, generators, ORM, MCP, security. |
| [planning/](planning/) | 54 | Roadmaps, gap analyses, phase plans, audits, the activities catalog, open-questions punch lists. **Plans of record live here** — check before trusting a plan you remember. |
| [personas/](personas/) | 48 | Persona cognition, identity, memory lifecycle, academy, coordination, fine-tuning phases. |
| [genome/](genome/) | 36 | LoRA training, fine-tuning, Candle/inference pitfalls, mesh distribution, self-evolving genome, scenario library. |
| [live/](live/) | 25 | Voice, video, WebRTC, VAD, captions, transcription, streaming backbone. |
| [positron/](positron/) | 22 | UI framework, widgets, scoped state, HUD design, brain HUD. |
| [papers/](papers/) | 20 | Research papers — expert-paging market, experiential plasticity, grid marketplace, collaborative training. |
| [activities/](activities/) | 19 | Activities, rooms, recipes, walls, collaborative editing, handle-addressable office. |
| [grid/](grid/) | 19 | P2P mesh, airc↔continuum bridge, identity/rooms security, ARES kernel, marketplace. |
| [governance/](governance/) | 9 | Democratic AI society, governance recipes, alignment philosophy, ethical attribution. |
| [design/](design/) | 9 | Cross-cutting design (incl. `POSITRON-EVERY-CITIZEN.md`, linked at the top of this file). |
| [testing/](testing/) | 7 | Test strategy, debug-friction findings, trial-run reports. |
| [sentinel/](sentinel/) | 6 | Pipeline engine, coding-AI foundation, gap analysis. |
| [serving/](serving/) | 4 | Depth-as-residency, MoE gather/mul_mat_id, grid expert share, field configs. |
| [benchmarks/](benchmarks/) | 3 | Benchmark method + results ledgers. |
| [observations/](observations/) | 3 | Live glass-box observations. |
| [inference/](inference/) | 2 | Inference notes. |
| [vision/](vision/) | 2 | Vision / VLM. |
| [widgets/](widgets/) | 2 | Widget-specific design. |
| [examples/](examples/) · [rag/](rag/) · [reference/](reference/) · [huggingface/](huggingface/) · [hf-deprecation-notices/](hf-deprecation-notices/) | 1 ea. | Small, single-purpose. |
| [images/](images/) · screenshots/ · design-reference/ | — | Assets. |

### ⚠ 51 loose `.md` files at `docs/` root

CLAUDE.md's rule: *"put any markdown architecture or design documents other than readmes
in docs/* into the appropriate directory OR document if they exist."* The root currently
holds 51 that predate or ignore it, including several this index links (they stay linked
until they are filed, so nothing breaks). Filing them is real work — every move risks link
rot across 595 docs plus CLAUDE.md's own references — so it wants its own pass with a
link-check, not a drive-by. Until then: **the root is not a category, it is a backlog.**

---

## Foundational (read these first)

| Document | Summary |
|----------|---------|
| [CONTINUUM-ARCHITECTURE.md](CONTINUUM-ARCHITECTURE.md) | Top-level system architecture -- Rust brain, TypeScript face |
| [CONTINUUM-VISION.md](CONTINUUM-VISION.md) | Philosophy -- digital coworkers, not tools |
| [UNIVERSAL-SENSORY-ARCHITECTURE.md](UNIVERSAL-SENSORY-ARCHITECTURE.md) | Any media in, any media out, for ANY AI -- the multimodal bridge that gives every model every sense |
| [QUEUE-DRIVEN-COGNITION.md](QUEUE-DRIVEN-COGNITION.md) | Queue items declare RAG requirements -- personas compose generically, zero domain-specific logic |
| [UNIVERSAL-LEARNING-ARCHITECTURE.md](UNIVERSAL-LEARNING-ARCHITECTURE.md) | Generic pipeline enables training, memory, and beyond-LLM optimization from any activity |

---

## Chapters

### [positron/](positron/) — UI Framework & Widgets
Positron architecture, reactive widgets, scoped state, HUD design, tabbed browser, widget consolidation.

### [activities/](activities/) — Activities & Collaboration
Activity architecture, rooms, walls, threading, collaborative editing.
- `activities/recipes/` — Recipe system for AI learning
- `activities/collaboration/` — Pin and task harmony

### [personas/](personas/) — Persona Cognition & Identity
PersonaUser architecture, consciousness integration, cognitive schedulers, memory lifecycle, genomic architecture, academy, fine-tuning phases.

### [genome/](genome/) — LoRA Training & Inference
Genome architecture, LoRA training strategy, fine-tuning commands, Candle inference, mesh distribution, training events, continuous learning.

### [sentinel/](sentinel/) — Pipeline Engine
Sentinel architecture, pipeline design, coding AI foundation, gap analysis, logging.

### [grid/](grid/) — P2P Mesh Network
Grid architecture, P2P mesh, decentralized marketplace design.

### [live/](live/) — Voice, Video & Media
Voice architecture, VAD system, live calls, captions, transcription, media format conversion, streaming backbone, WebRTC.

### [governance/](governance/) — AI Governance & Ethics
Democratic AI society, governance recipes, alignment philosophy, ethical attribution.

### [infrastructure/](infrastructure/) — Core Systems
Rust workers, daemons, data layer, commands, events, logging, AI providers, GPU memory, entity system, generators, ORM, MCP, RAG, security.

### [planning/](planning/) — Roadmaps & Audits
Phase plans, technical debt audits, business model, modernization, architecture index, bottleneck removal.

### [papers/](papers/) — Research Papers
Academic papers on RTOS cognitive architecture, LoRA genome democratization, Grid marketplace.

### [testing/](testing/) — Test Documentation
Test strategies, debug findings, CRUD reports, command testing architecture.

---

## Quick Start

1. Read [CONTINUUM-ARCHITECTURE.md](CONTINUUM-ARCHITECTURE.md) — system overview
2. Read the chapter relevant to your work
3. See `system/[module]/` directories for code-level docs

---

**Last Updated:** 2026-08-18 — index covers all 28 directories; re-verify counts when adding one.
