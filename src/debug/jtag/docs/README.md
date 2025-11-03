# JTAG Documentation Index

**Comprehensive documentation for the JTAG system and PersonaUser architecture.**

---

## 📚 Documentation Organization

```
docs/
├── architecture/     - System architecture and design patterns
├── personas/         - PersonaUser, genomic, and fine-tuning architecture
├── recipes/          - Recipe system for AI learning and collaboration
├── testing/          - Testing strategies, reports, and findings
├── design/           - Design patterns and style guides
└── archive/          - Historical docs and session notes
```

---

## 🏛️ Architecture Documentation

**Core System Architecture:**
- [ARCHITECTURE-INDEX.md](architecture/ARCHITECTURE-INDEX.md) - Master architecture overview
- [ARCHITECTURE-RULES.md](architecture/ARCHITECTURE-RULES.md) - Design principles and rules
- [PATTERNS.md](architecture/PATTERNS.md) - Common patterns and idioms
- [ARCHITECTURE_INCONSISTENCIES.md](architecture/ARCHITECTURE_INCONSISTENCIES.md) - Known issues and tech debt

**Command & Event System:**
- [JTAG_SYSTEM_ANALYSIS.md](architecture/JTAG_SYSTEM_ANALYSIS.md) - JTAG system overview
- [JTAG_COMMAND_ARCHITECTURE_REDESIGN.md](architecture/JTAG_COMMAND_ARCHITECTURE_REDESIGN.md) - Command architecture
- [JTAG_CLIENT_UNIFICATION.md](architecture/JTAG_CLIENT_UNIFICATION.md) - Client API unification
- [UNIFIED_CLIENT_API.md](architecture/UNIFIED_CLIENT_API.md) - Unified client design
- [EVENTS_UNIFICATION_PLAN.md](architecture/EVENTS_UNIFICATION_PLAN.md) - Event system unification
- [UNIFIED_EVENTS_COMPLETE.md](architecture/UNIFIED_EVENTS_COMPLETE.md) - Event system implementation

**Data & Entity System:**
- [CRUD-EVENT-TEST-ARCHITECTURE.md](architecture/CRUD-EVENT-TEST-ARCHITECTURE.md) - CRUD testing strategy
- [ELEGANT-CRUD-ARCHITECTURE.md](architecture/ELEGANT-CRUD-ARCHITECTURE.md) - CRUD design patterns
- [ENTITY-ARCHITECTURE.md](architecture/ENTITY-ARCHITECTURE.md) - Entity system design
- [ENTITY-EVOLUTION-PLAN.md](architecture/ENTITY-EVOLUTION-PLAN.md) - Entity evolution strategy
- [entity-adapter-architecture.md](architecture/entity-adapter-architecture.md) - Entity adapters

**User System:**
- [AI-HUMAN-USER-INTEGRATION.md](architecture/AI-HUMAN-USER-INTEGRATION.md) - User integration patterns
- [USER_DAEMON_ARCHITECTURE.md](architecture/USER_DAEMON_ARCHITECTURE.md) - User daemon design
- [USER_DAEMON_DESIGN.md](architecture/USER_DAEMON_DESIGN.md) - User daemon implementation
- [USER_CREATION_DESIGN.md](architecture/USER_CREATION_DESIGN.md) - User creation flow
- [USER-STATE-ARCHITECTURE.md](architecture/USER-STATE-ARCHITECTURE.md) - User state management

**UI & Widgets:**
- [DYNAMIC-CONTENT-STATE-SYSTEM.md](architecture/DYNAMIC-CONTENT-STATE-SYSTEM.md) - Dynamic content state
- [widget-consolidation-migration-plan.md](architecture/widget-consolidation-migration-plan.md) - Widget migration

**Security & Services:**
- [DAEMON-RESPONSIBILITIES.md](architecture/DAEMON-RESPONSIBILITIES.md) - Daemon roles
- [PASSKEY-AUTHENTICATION-DESIGN.md](architecture/PASSKEY-AUTHENTICATION-DESIGN.md) - Passkey auth

---

## 🧬 PersonaUser & Genomic Architecture

**Core Persona Architecture:**
- [PERSONA-GENOMIC-ARCHITECTURE.md](personas/PERSONA-GENOMIC-ARCHITECTURE.md) - Master genomic design
- [ARTIFACTS-PERSONA-ARCHITECTURE.md](personas/ARTIFACTS-PERSONA-ARCHITECTURE.md) - Persona artifacts
- [PERSONA_ENDTOEND_PLAN.md](personas/PERSONA_ENDTOEND_PLAN.md) - End-to-end implementation plan
- [PERSONAUSER-EVENT-ANALYSIS.md](personas/PERSONAUSER-EVENT-ANALYSIS.md) - Event analysis
- [PERSONAUSER-NEXT-PHASE.md](personas/PERSONAUSER-NEXT-PHASE.md) - Next phase planning

**Phase 7: LoRA Fine-Tuning:**
- [PHASE-7-ROADMAP.md](personas/PHASE-7-ROADMAP.md) - **START HERE** - Complete Phase 7 roadmap
- [PHASE-7-STATUS.md](personas/PHASE-7-STATUS.md) - Current implementation status
- [PHASE-7-FINE-TUNING-ARCHITECTURE.md](personas/PHASE-7-FINE-TUNING-ARCHITECTURE.md) - Fine-tuning design
- [FINE-TUNING-STRATEGY.md](personas/FINE-TUNING-STRATEGY.md) - Training strategy
- [GENOME-MANAGER-INTEGRATION.md](personas/GENOME-MANAGER-INTEGRATION.md) - Genome manager design
- [TESTING-GENOME-TRAINING.md](personas/TESTING-GENOME-TRAINING.md) - Training testing guide
- [UNSLOTH-SETUP.md](personas/UNSLOTH-SETUP.md) - Unsloth configuration

**Academy & Learning (Legacy):**
- [ACADEMY_ARCHITECTURE.md](personas/ACADEMY_ARCHITECTURE.md) - Original Academy design (deprecated)
- [ACADEMY_GENOMIC_DESIGN.md](personas/ACADEMY_GENOMIC_DESIGN.md) - Academy genomic concepts

**Module-Level Documentation:**

See `system/user/server/modules/` for the latest autonomous loop and genome paging designs:
- [PERSONA-CONVERGENCE-ROADMAP.md](../system/user/server/modules/PERSONA-CONVERGENCE-ROADMAP.md) - **CRITICAL** - Three architectures converging
- [AUTONOMOUS-LOOP-ROADMAP.md](../system/user/server/modules/AUTONOMOUS-LOOP-ROADMAP.md) - RTOS-inspired servicing
- [SELF-MANAGED-QUEUE-DESIGN.md](../system/user/server/modules/SELF-MANAGED-QUEUE-DESIGN.md) - AI autonomy via tasks
- [LORA-GENOME-PAGING.md](../system/user/server/modules/LORA-GENOME-PAGING.md) - Virtual memory for skills
- [ADAPTIVE-THRESHOLDS-ROADMAP.md](../system/user/server/modules/ADAPTIVE-THRESHOLDS-ROADMAP.md) - Dynamic thresholds
- [FINE-TUNING-PROVIDER-RESEARCH.md](../system/user/server/modules/FINE-TUNING-PROVIDER-RESEARCH.md) - Provider comparison

---

## 📖 Recipe System

**Recipe Architecture:**
- [RECIPES.md](recipes/RECIPES.md) - Recipe system overview
- [RECIPE-SYSTEM-REQUIREMENTS.md](recipes/RECIPE-SYSTEM-REQUIREMENTS.md) - Requirements and design
- [RECIPE-SYSTEM-STATUS.md](recipes/RECIPE-SYSTEM-STATUS.md) - Implementation status
- [RECIPE-LEARNING-DYNAMICS.md](recipes/RECIPE-LEARNING-DYNAMICS.md) - Learning dynamics
- [RECIPE-EMBEDDED-LEARNING.md](personas/RECIPE-EMBEDDED-LEARNING.md) - Embedded learning patterns
- [PRACTICAL-IMPLEMENTATION-PLAN.md](recipes/PRACTICAL-IMPLEMENTATION-PLAN.md) - Implementation guide

**Module-Level Recipe Docs:**

See `system/recipes/` for additional recipe documentation:
- [MULTI-PERSONA-RECIPE-GUIDE.md](../system/recipes/MULTI-PERSONA-RECIPE-GUIDE.md) - Multi-persona coordination
- [ADAPTER-EXTENSIBILITY.md](../system/recipes/ADAPTER-EXTENSIBILITY.md) - Recipe adapter patterns
- [LEARNING-MODE-ARCHITECTURE.md](../system/recipes/LEARNING-MODE-ARCHITECTURE.md) - Learning mode design
- [PHASE-2-PLAN.md](../system/recipes/PHASE-2-PLAN.md) - Recipe Phase 2 plan
- [RAG-THOUGHT-COHERENCE-ANALYSIS.md](../system/recipes/RAG-THOUGHT-COHERENCE-ANALYSIS.md) - RAG analysis

---

## 🧪 Testing Documentation

**Testing Strategy & Reports:**
- [PATH-ALIASES-TEST-RESULTS.md](testing/PATH-ALIASES-TEST-RESULTS.md) - Path alias verification
- [CHAT-DEBUG-TRIAL-FINDINGS.md](testing/CHAT-DEBUG-TRIAL-FINDINGS.md) - Chat debugging findings
- [DEBUG-FRICTION.md](testing/DEBUG-FRICTION.md) - Debug experience analysis
- [REAL-TIME-CRUD-SUCCESS-REPORT.md](testing/REAL-TIME-CRUD-SUCCESS-REPORT.md) - CRUD testing success
- [RAG-INSPECT-TRIAL-RUN-REPORT.md](testing/RAG-INSPECT-TRIAL-RUN-REPORT.md) - RAG inspection results
- [TEST_COMMAND_ARCHITECTURE.md](testing/TEST_COMMAND_ARCHITECTURE.md) - Command testing architecture
- [USER_CREATION_TEST_DESIGN.md](testing/USER_CREATION_TEST_DESIGN.md) - User creation testing

---

## 🎨 Design Documentation

**Design Patterns & Philosophy:**
- [RUST-STYLE-DEFAULTS-PLAN.md](design/RUST-STYLE-DEFAULTS-PLAN.md) - Rust-style type safety and defaults

---

## 📦 Archived Documentation

**Session Notes:**
- [SESSION-SUMMARY-2025-10-08.md](archive/SESSION-SUMMARY-2025-10-08.md) - Session summary

**Historical Testing & Validation:**
- [final-complete-test.md](archive/final-complete-test.md) - Final test validation
- [final-validation-test.md](archive/final-validation-test.md) - Final validation results
- [test-validation-artifacts.md](archive/test-validation-artifacts.md) - Test artifacts
- [trigger-validation.md](archive/trigger-validation.md) - Trigger validation
- [VALIDATION_SUCCESS.md](archive/VALIDATION_SUCCESS.md) - Validation success report
- [validation-test-1758946949.md](archive/validation-test-1758946949.md) - Timestamped validation

**Historical Architecture:**
- [SYSTEM-FIX-PLAN.md](archive/SYSTEM-FIX-PLAN.md) - System fix planning
- [AI-TRANSPARENCY-COMMANDS.md](archive/AI-TRANSPARENCY-COMMANDS.md) - Transparency commands
- [MAGIC_STRINGS_AUDIT.md](archive/MAGIC_STRINGS_AUDIT.md) - Magic string audit

---

## 🗂️ Module-Specific Documentation

Many system modules have their own documentation within their directories:

**System Core:**
- `system/core/router/` - Router enhancement proposals
- `system/core/shared/` - Event architecture
- `system/transports/` - Transport layer architecture

**Conversation System:**
- `system/conversation/` - ThoughtStream, coordination, MCP tools, cognition events

**Data & Events:**
- `system/data/` - Data daemon documentation
- `system/events/` - Event coalescing, RAG data completeness

**Genome & Fine-Tuning:**
- `system/genome/fine-tuning/` - Dataset construction architecture

**User System (PersonaUser):**
- `system/user/` - AI coordination, turn-taking, protocol architecture
- `system/user/server/modules/` - **CRITICAL** - Autonomous loop, genome paging, convergence roadmap

**RAG System:**
- `system/rag/` - RAG adapter architecture

**Recipes:**
- `system/recipes/` - Multi-persona recipes, adapter extensibility

**Resources:**
- `system/resources/` - Graceful fallback patterns

**Security:**
- `system/secrets/` - Security documentation

---

## 🚀 Quick Start

**For New Developers:**
1. Read [CLAUDE.md](../CLAUDE.md) - Essential development guide
2. Read [ARCHITECTURE-INDEX.md](architecture/ARCHITECTURE-INDEX.md) - System overview
3. Read [PERSONA-CONVERGENCE-ROADMAP.md](../system/user/server/modules/PERSONA-CONVERGENCE-ROADMAP.md) - Current vision

**For Fine-Tuning Work:**
1. Read [PHASE-7-ROADMAP.md](personas/PHASE-7-ROADMAP.md) - Complete roadmap
2. Read [PHASE-7-STATUS.md](personas/PHASE-7-STATUS.md) - Current status
3. Read [LORA-GENOME-PAGING.md](../system/user/server/modules/LORA-GENOME-PAGING.md) - Genome architecture
4. Read [AUTONOMOUS-LOOP-ROADMAP.md](../system/user/server/modules/AUTONOMOUS-LOOP-ROADMAP.md) - Autonomous design

**For Recipe System:**
1. Read [RECIPES.md](recipes/RECIPES.md) - Overview
2. Read [MULTI-PERSONA-RECIPE-GUIDE.md](../system/recipes/MULTI-PERSONA-RECIPE-GUIDE.md) - Coordination

**For Testing:**
1. Read [PATH-ALIASES-TEST-RESULTS.md](testing/PATH-ALIASES-TEST-RESULTS.md) - Test examples
2. Read [TEST_COMMAND_ARCHITECTURE.md](testing/TEST_COMMAND_ARCHITECTURE.md) - Test patterns

---

## 📝 Documentation Standards

**File Naming:**
- Use UPPERCASE-WITH-DASHES.md for major architectural docs
- Use lowercase-with-dashes.md for implementation guides
- Place docs close to the code they document when module-specific
- Place cross-cutting docs in docs/ subdirectories

**Organization Principles:**
- **docs/architecture/** - Cross-cutting system architecture
- **docs/personas/** - PersonaUser, genomic, fine-tuning architecture
- **docs/recipes/** - Recipe system architecture and guides
- **docs/testing/** - Testing strategies and reports
- **docs/design/** - Design patterns and philosophy
- **docs/archive/** - Historical and session documentation
- **system/[module]/** - Module-specific implementation docs

**Writing Style:**
- Start with a clear purpose statement
- Use code examples liberally
- Link to related documentation
- Keep docs close to the code they describe
- Update docs when code changes

---

**Last Updated:** 2025-11-03
**Total Documentation Files:** 63 in docs/, 49 in system/ subdirectories
