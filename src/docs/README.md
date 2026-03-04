# Continuum Documentation

**Organized by theme — each chapter is a self-contained concern.**

---

## Structure

```
docs/
├── CONTINUUM-ARCHITECTURE.md     # Top-level system architecture
├── CONTINUUM-VISION.md           # Vision and philosophy
├── CONFIGURATION.md              # Setup and configuration
│
├── positron/          # UI framework, widgets, state, Positronic embodiment
├── activities/        # Activities, rooms, recipes, walls, collaboration
├── personas/          # Persona cognition, identity, memory, coordination
├── genome/            # LoRA, training, fine-tuning, inference, mesh distribution
├── sentinel/          # Pipeline engine, coding AI
├── grid/              # P2P mesh, Grid economics, Reticulum
├── live/              # Voice, video, WebRTC, VAD, captions, media
├── governance/        # AI governance, democratic society, ethics, alignment
├── infrastructure/    # Rust workers, daemons, data, commands, events, logging, GPU
├── planning/          # Roadmaps, audits, status, phases, debt, business model
├── papers/            # Research papers
├── testing/           # Test documentation
├── examples/          # Example implementations
├── images/            # Diagrams and visuals
└── screenshots/       # UI screenshots
```

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

**Last Updated:** 2026-03-04
