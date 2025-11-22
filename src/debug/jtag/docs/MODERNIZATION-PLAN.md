# Documentation Modernization Plan

**Date**: 2025-11-22
**Goal**: Update existing docs/ with latest architecture from .doc-staging/

## Strategy

**Approach**: Category by category, modernize docs/ with latest:
- RTOS subprocess pattern (PersonaSubprocess)
- Hippocampus memory consolidation (MemoryConsolidationSubprocess)
- Multi-layer genome composition (N-layer PEFT, not single layer)
- Autonomous loop + convergence (3 pillars)
- Adaptive complexity (SPIKE escalation)

## Phase 1: Persona (CURRENT)

### Latest Architecture (from .doc-staging/persona/)

**Key concepts to integrate**:
1. **RTOS Autonomous Loop** (autonomous-loop-roadmap.md)
   - PersonaInbox with priority queue
   - PersonaState with energy/mood tracking
   - Adaptive cadence polling (3s → 10s based on state)
   - Graceful degradation under load

2. **Convergence** (convergence-roadmap.md)
   - Integration of 3 pillars: autonomous loop + self-tasks + genome paging
   - ONE method (`serviceInbox()`) integrates all three visions

3. **LoRA Genome Paging** (lora-genome-paging.md)
   - Virtual memory pattern for skills
   - LRU eviction when memory full
   - Page adapters in/out based on task domain

4. **Adaptive Complexity** (adaptive-complexity-routing.md)
   - SPIKE escalation (straightforward/moderate/nuanced)
   - Complexity-aware model selection
   - Dynamic layer weighting

5. **Multi-Layer Genome** (from .doc-staging/genome/)
   - N-layer PEFT composition (not single layer)
   - Hot-swappable phenotypes
   - Dynamic weight adjustment

6. **Hippocampus Memory** (from .doc-staging/memory/)
   - MemoryConsolidationSubprocess pattern
   - Working memory → Pattern detection → Long-term storage
   - Non-blocking observation

### Existing docs/personas/ to Modernize

**Outdated/Status docs** (28 docs found):
- PERSONAUSER-NEXT-PHASE.md - "THIS WEEKEND" planning from Oct 5 ❌ DELETE
- PHASE-7-STATUS.md - Status tracking ❌ DELETE
- PHASE-7-ROADMAP.md - Old roadmap ❌ DELETE
- TESTING-GENOME-TRAINING.md - Testing notes ❌ DELETE
- PERSONA_ENDTOEND_PLAN.md - Implementation plan ❌ DELETE
- UNSLOTH-SETUP.md - Setup guide ❌ DELETE
- SENTINEL-INTEGRATION-QUICKSTART.md - Quickstart guide ❌ DELETE
- REPRODUCIBLE-EXPERIMENTS-PLAN.md - Experiment plan ❌ DELETE

**Architecture docs needing updates**:
- PERSONA-GENOMIC-ARCHITECTURE.md - Update with multi-layer composition ✏️
- MULTI-MODEL-PERSONA-ARCHITECTURE.md - Update with RTOS loop ✏️
- FINE-TUNING-STRATEGY.md - Update with latest adapter patterns ✏️
- GENOME-MANAGER-INTEGRATION.md - Update with GenomeDaemon RTOS ✏️
- ARTIFACTS-PERSONA-ARCHITECTURE.md - Check if still relevant ✏️

**Still relevant** (check these):
- ACADEMY_ARCHITECTURE.md
- ACADEMY_GENOMIC_DESIGN.md
- GENOME-REVOLUTION.md
- RECIPE-EMBEDDED-LEARNING.md
- VINE-DIESEL-PERSONA-DESIGN.md (example persona)

### Action Plan for Persona

**Step 1**: Delete status/plan docs (8 docs) ❌
**Step 2**: Review architecture docs (5 docs) and update with:
  - RTOS autonomous loop pattern
  - Multi-layer genome composition
  - Convergence of 3 pillars
  - Hippocampus memory pattern
**Step 3**: Check relevance of remaining docs (5 docs)
**Step 4**: Create navigation/index for docs/personas/

## Phase 2: Memory (NEXT)

### Latest Architecture (from .doc-staging/memory/)

**Key concepts**:
- **Hippocampus pattern**: MemoryConsolidationSubprocess ✅ IMPLEMENTED
- **RTOS pattern**: PersonaSubprocess base class ✅ IMPLEMENTED
- **Non-blocking observation**: Peek at inbox without blocking ✅
- **Pattern-driven consolidation**: Cosine similarity, cluster detection ✅

### Existing docs/ to check

*To be determined after persona phase*

## Phase 3: Genome (AFTER MEMORY)

### Latest Architecture (from .doc-staging/genome/)

**Key concepts**:
- **Multi-layer composition**: N layers active simultaneously (not single layer)
- **PEFT integration**: Python integration exists, TypeScript wrapper needed
- **GenomeDaemon**: RTOS subprocess for non-blocking composition
- **Adapter-driven**: 5 adapter interfaces (Backend, Storage, Compositor, Eviction, Trainer)

## Phase 4: Cognition (AFTER GENOME)

### Latest Architecture (from .doc-staging/cognition/)

**Key concepts**:
- **DecisionAdapterChain**: Implemented ✅
- **PersonaSelfState**: Implemented ✅
- **WorkingMemoryManager**: Implemented ✅
- **SimplePlanFormulator**: Implemented ✅

## Phase 5: Coordination (AFTER COGNITION)

### Latest Architecture (from .doc-staging/coordination/)

**Key concepts**:
- **ChatCoordinationStream**: Implemented ✅ (342 lines)
- **Simple rules** (Phase 1): @mentions, rate limiting
- **Future vision** (Phase 2): RoomCoordinator AI orchestrator

## Phase 6: Commands (AFTER COORDINATION)

### Latest Architecture (from .doc-staging/commands/)

**Key concepts**:
- **Command architecture**: GOLD STANDARD docs exist ✅
- **RAG hierarchy**: Reference implementation ✅
- **Type safety patterns**: Rust-like strictness ✅

## Phase 7: Architecture (FINAL)

### Latest Architecture (from .doc-staging/architecture/)

**Key concepts**:
- **Event architecture**: Event-driven system
- **RAG patterns**: Context-aware RAG
- **Security**: Security architecture
- **Resource management**: Resource management patterns

## Modernization Principles

1. **Delete status/history** - No "Phase X status", "roadmap", "plan" docs in final docs/
2. **Keep architecture/vision** - Preserve how systems work, not implementation tracking
3. **Update with latest** - Integrate RTOS, hippocampus, multi-layer genome, convergence
4. **Create navigation** - Index files to help find docs
5. **One category at a time** - Don't try to do everything at once

## Success Criteria

**For each category**:
- ✅ Status/history docs deleted
- ✅ Architecture docs updated with latest concepts
- ✅ Navigation/index created
- ✅ Cross-references updated
- ✅ No duplicate information

**Overall**:
- Clean, modern docs/ reflecting current architecture
- Easy to find information
- Clear distinction between current vs future
- Up-to-date with RTOS, hippocampus, multi-layer genome

## Next Action

Start with **Phase 1: Persona** - delete status docs, update architecture docs with RTOS/convergence/multi-layer concepts.
