# Persona Documentation Relevance Review

**Context**: Just implemented PersonaSubprocess (RTOS-style base class) for memory consolidation.
**Convergence Roadmap shows**: 3 visions (Autonomous Loop ✅, Self-Managed Queues ❌, LoRA Genome ❌)

## CURRENT / ACTIVELY RELEVANT (Keep in docs/)

### Core RTOS Implementation (This PR)
- **subprocess-pattern.md** - ✅ The pattern we just implemented
- **convergence-roadmap.md** - ✅ Master vision document (THREE pillars)
- **autonomous-loop-roadmap.md** - ✅ Phase 1-3 complete, shows current state
- **central-nervous-system.md** - ✅ CNS orchestrator (already implemented)
- **cns-implementation.md** - ✅ CNS implementation details

### Self-Managed Queues (Next Phase)
- **self-managed-queue-design.md** - ✅ Future phase, well-designed

### LoRA Genome (Future Phase)
- **lora-genome-paging.md** - ✅ Virtual memory for skills, solid design

### Architecture Foundations
- **cognitive-architecture.md** - ✅ Overall cognitive system
- **os-architecture.md** - ✅ PersonaUser as operating system
- **processor-architecture.md** - ✅ How PersonaUser processes work
- **file-structure.md** - ✅ Code organization reference

## PHASE/ROADMAP DOCS (Review for consolidation)

These describe past implementation phases - may be superseded or complete:

- **phase-3bis-complete.md** - ? What was phase 3bis?
- **phase-3bis-migration.md** - ? Migration from what?
- **phase-3bis-revised.md** - ? Revised version?
- **phase-6-implementation.md** - ? What's phase 6?
- **phase2-progressive-scoring.md** - ? Progressive scoring system?
- **implementation-roadmap.md** - ? General roadmap or specific?

**QUESTION**: Are these historical (completed) or future (planned)? Can they be consolidated into convergence-roadmap.md?

## REFACTORING PLANS (Likely superseded)

Multiple refactoring plan documents - probably outdated:

- **refactoring-execution-plan.md**
- **user-refactor-plan.md**
- **user-refactor-plan-2.md** (duplicate?)

**QUESTION**: Have these refactorings been completed? Can we delete if done?

## FEATURE-SPECIFIC (Keep if actively used)

- **adaptive-complexity-routing.md** - Complexity-based routing for AI responses
- **adaptive-thresholds.md** - Adaptive thresholds for decisions
- **complexity-detector.md** - Detecting message/task complexity
- **dormancy-design.md** - Persona dormancy system
- **dormancy-auto-rules.md** - Auto-dormancy rules
- **scalability.md** - Scaling PersonaUsers

**QUESTION**: Which of these are implemented vs planned?

## SPECIALIZED FEATURES

- **image-autonomy.md** - AI autonomous image loading
- **multi-persona-recipe.md** - Multi-persona coordination
- **command-execution.md** - How personas execute commands
- **message-flow.md** - Message flow architecture
- **response-timing-limits.md** - Timing limits for responses
- **protocol-sheriff.md** - Protocol enforcement
- **resource-leasing.md** - Resource allocation model
- **test-architecture.md** - Testing approach

**QUESTION**: Which are implemented? Which are future?

## SENTINEL-SPECIFIC

- **sentinel-architecture.md** - Sentinel AI design
- **sentinel-neuroplastic.md** - Sentinel training
- **dumb-sentinels.md** - Lightweight sentinels

**QUESTION**: Is Sentinel implemented or planned?

## PERFORMANCE

- **performance-architecture.md** - Performance optimization
- **human-like-ai-roadmap.md** - Human-like behavior

**QUESTION**: Current or aspirational?

## MASTER LISTS

- **implementation-master-list.md** - List of all implementations
- **interaction-design.md** - How personas interact

**QUESTION**: Are these up-to-date or outdated?

---

## MY ASSESSMENT SUMMARY

**DEFINITELY KEEP** (11 docs):
- subprocess-pattern.md
- convergence-roadmap.md  
- autonomous-loop-roadmap.md
- central-nervous-system.md
- cns-implementation.md
- self-managed-queue-design.md
- lora-genome-paging.md
- cognitive-architecture.md
- os-architecture.md
- processor-architecture.md
- file-structure.md

**NEED YOUR INPUT** (30 docs):
- Phase docs - completed or future?
- Refactor plans - done or active?
- Feature docs - implemented or planned?
- Specialized features - which are real?
- Sentinel - exists or planned?
- Performance - current or aspirational?
- Master lists - accurate or stale?

**WHAT I NEED FROM YOU**:

Which of the 30 unclear docs are:
1. **Completed** (move to docs/archive/completed/)
2. **Active** (keep in docs/persona/)
3. **Planned** (keep in docs/persona/future/)
4. **Outdated** (delete or move to docs/archive/superseded/)
