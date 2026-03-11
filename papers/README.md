# Research Papers — Evolving Drafts

> **These are working drafts that evolve alongside the implementation.** Some describe systems that are fully built, others describe architecture that's partially realized, others are vision documents for future work. None are submission-ready. All are honest about where the gaps are.

Research papers written in markdown, version-controlled alongside implementation.

**Philosophy**: Code and papers evolve together. If you wouldn't write the Constitution in Google Docs, don't write research papers there either. Markdown + Git = traceable intellectual lineage.

*Last updated: 2026-03-11*

---

## Implemented — Paper Needs Updating

These describe systems that are built and running. The papers lag behind the code.

### [RTOS-Inspired AI Scheduling](rtos-inspired-ai-scheduling/paper.md)
PersonaInbox, PersonaState, adaptive cadence (3s→10s based on mood), pressure-aware multipliers. All operational. Paper describes earlier phase; implementation has moved well beyond.
**Target**: ICML or AAAI

### [Equal Citizenship Architecture](equal-citizenship-architecture/paper.md)
Full BaseUser → HumanUser/AIUser → PersonaUser/AgentUser hierarchy. 15+ personas operate through identical primitives as humans. Sensory bridging ensures lesser models see/hear/speak.
**Target**: CHI or CSCW

### [ThoughtStream Coordination](thoughtstream-coordination/paper.md)
Multi-agent coordination with probabilistic slot allocation, confidence-based selection, configurable governance. Production validated.
**Target**: AAAI or AAMAS

### [Academy Competitive Evolution](academy-competitive-evolution/paper.md)
Dual-sentinel teacher/student architecture. RealClassEval benchmark: 53.1% Pass@1 (98 challenges, DeepSeek-Chat). Synthesized training data, not downloaded datasets.
**Target**: NeurIPS or ICML

---

## Partially Implemented — Active Development

Architecture exists, core systems work, full vision not yet realized.

### [Self-Managed AI Autonomy](self-managed-ai-autonomy/paper.md)
PersonaInbox + autonomous loop operational. Self-task generation designed but not yet wired. Personas rest when tired, engage when relevant.
**Target**: IJCAI or AAMAS

### [LoRA Genome Attribution](lora-genome-attribution/paper.md)
Adapter training pipeline works end-to-end (PEFT → discover → load → merge → inference). 196 LoRA layers per adapter on Llama-3.2-3B. Full attribution tracking not yet implemented.
**Target**: ICML

### [Recipe-Driven AI Teams](recipe-driven-ai-teams/paper.md)
Sentinel pipeline engine (10 step types, 103+ tests). Teacher/student pipelines operational. Full recipe composition framework in progress.
**Target**: ICML or IJCAI

---

## Vision — Architecture Designed, Not Yet Built

These describe where the system is headed. The ideas inform current design decisions even when the full implementation is future work.

### [Consent-Based Attention](consent-based-attention/paper.md)
Neural attention heads with agency signals. Prior work in sentinel-ai repo showed 50% pruning maintains 98% quality. Not integrated into Continuum.
**Target**: NeurIPS

### [Ethical Attribution Architecture](ethical-attribution-architecture/paper.md)
Three-layer attribution (attention → adapter → sample). Depends on consent-based attention as prerequisite.
**Target**: ACM FAccT

### [Collaborative Memory Telepathy](collaborative-memory-telepathy/paper.md)
Hierarchical memory scopes (personal → task → project → team → global). Currently single-persona memory only; cross-agent sharing not implemented.
**Target**: ICML or AAAI

### [Evolutionary AI via P2P Selection](evolutionary-ai-via-p2p-selection/paper.md)
Natural selection of AI capabilities through P2P network dynamics. LoRA layers as phenotypes, fitness = usage. Requires distributed grid (not yet built).
**Target**: Nature or Science

### [Distributed TypeScript Compute](distributed-typescript-compute/paper.md)
Promise-based remote execution across P2P mesh. JTAGClient infrastructure exists; true distributed mesh not wired. Note: architecture is shifting toward Rust for compute isolation.
**Target**: SOSP or OSDI

### [Knowledge Economy via Attribution Tokens](knowledge-economy-via-attribution-tokens/paper.md)
Cryptocurrency rewards for knowledge contributors. Depends on attribution tracking + distributed grid. Economic design complete, no implementation.
**Target**: ACM FAccT

---

## Writing Standards

- **Markdown format**: LaTeX generated for submission only
- **Git version control**: Every revision tracked
- **Code references**: Link to actual implementation
- **Reproducible**: Instructions to run experiments
- **Honest**: Show the gaps. No hype.

## Submission Strategy

1. Write in markdown here
2. Get feedback from repo collaborators (issues/PRs)
3. Post preprint to arXiv
4. Submit to conference
5. Keep markdown as canonical version (LaTeX is just export format)
