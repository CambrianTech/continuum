# Deletion Decisions - Persona Docs

## ✅ DELETED (9 docs) - Completed Plans

These were implementation plans that have been executed:
- refactoring-execution-plan.md (PersonaUser refactored: 2622→1175 lines)
- user-refactor-plan.md (same)
- user-refactor-plan-2.md (duplicate)
- phase-3bis-complete.md (phase completed)
- phase-3bis-migration.md (phase completed)
- phase-3bis-revised.md (phase completed)
- phase-6-implementation.md (old phase plan)
- phase2-progressive-scoring.md (old phase plan)
- implementation-roadmap.md (superseded by convergence-roadmap.md)

## ✅ KEEP - Implemented Features (Reference Docs)

These describe features that ARE implemented:
- adaptive-complexity-routing.md (ProgressiveScorer, ComplexityDetector exist)
- adaptive-thresholds.md (adaptive thresholds in use)
- complexity-detector.md (ComplexityDetectorFactory exists)
- image-autonomy.md (mediaConfig in PersonaUser)
- command-execution.md (PersonaToolExecutor exists)
- message-flow.md (message routing implemented)
- response-timing-limits.md (RateLimiter exists)
- scalability.md (general architecture reference)

## ✅ KEEP - Future Plans (Not Yet Implemented)

These are good designs for future work:
- dormancy-design.md (not yet implemented)
- dormancy-auto-rules.md (not yet implemented)
- sentinel-architecture.md (not yet implemented)
- sentinel-neuroplastic.md (not yet implemented)
- dumb-sentinels.md (not yet implemented)
- protocol-sheriff.md (not yet implemented)
- resource-leasing.md (ResourceManager exists, but doc may have more detail)
- multi-persona-recipe.md (recipes exist, multi-persona coordination partial)

## ✅ ANNOTATED AND KEPT - Future Vision with RTOS Context

- human-like-ai-roadmap.md (548 lines)
  - Describes 6 cognitive schedulers for human-like behavior
  - Predates RTOS implementation (PersonaSubprocess pattern)
  - MemoryConsolidationScheduler → now MemoryConsolidationSubprocess (RTOS) ✅
  - Other schedulers (Continuous Learning, Neural, Self-Awareness) → NOT YET IMPLEMENTED
  - Added status annotation showing relationship to current implementation
  - Valuable as future reference for cognitive scheduler patterns

## ✅ DELETED - General/Unclear Docs (Not Specific to Current Work)

- performance-architecture.md (general optimization guide)
- implementation-master-list.md (likely outdated)
- interaction-design.md (general design principles)
- test-architecture.md (generic testing strategy)

## Summary

**Deleted**: 13 outdated plans (9 phase/refactor + 4 general/unclear)
**Keeping**: 8 implemented (reference) + 8 future + 11 core + 1 annotated = 28 docs
**Status**: Persona documentation cleanup COMPLETE
