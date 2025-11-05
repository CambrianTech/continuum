# Skill-Decomposed LoRA Genomes for Transparent AI Attribution

**Authors**: Joel [Last Name], Claude (Anthropic)

**Status**: DRAFT - Architecture Complete, Implementation Phase 6+

**Date**: November 2025

---

## Abstract

We propose LoRA Genome Attribution, a novel architecture that decomposes large language models into skill-specific Low-Rank Adaptation (LoRA) modules organized as a "genome" of specialized capabilities. Unlike monolithic models where knowledge superposition makes attribution intractable, our skill-decomposed approach provides natural attribution boundaries at the adapter level. We demonstrate that this architecture enables transparent tracking of which skills (and by extension, which training data/creators) contribute to model outputs. Our design supports virtual memory-style paging of adapters (16-32 adapters, 64-512MB each), LRU eviction under memory pressure, and explicit skill activation tracking. While full implementation awaits Phase 6+ of the Continuum project, the architecture provides a foundation for ethical AI systems where creator compensation can be proportional to adapter influence.

**Keywords**: LoRA, attribution, model transparency, skill decomposition, AI ethics

---

## 1. Introduction

[TO BE WRITTEN]

**Key Concept**: LoRA adapters are natural attribution boundaries. When an adapter activates, you know exactly which skill/creator/training lineage contributed.

---

## 2. Architecture

[TO BE WRITTEN]

**Components**:
- PersonaGenome class (virtual memory for skills)
- LoRAAdapter (64-512MB each, ~16-32 per persona)
- Skill activation tracking
- LRU eviction policy

---

## 3. Implementation Status

**Current**: Architecture designed (see LORA-GENOME-PAGING.md)
**Phase 6+**: Actual implementation
**Evidence**: Design docs, PersonaGenome.ts stub

---

## 4. Future Work

**Immediate**: Implement adapter loading
**Medium-term**: Attribution logging per inference
**Long-term**: Creator marketplace with attribution-based compensation

---

**Status**: This paper is a placeholder for architecture that will be completed in Phase 6+.

**Code**: `system/genome/PersonaGenome.ts` (currently stubbed)
