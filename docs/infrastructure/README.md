# Infrastructure — The Engine Room

> Two primitives power everything: `Commands.execute()` for request/response and `Events.subscribe()/emit()` for publish/subscribe. Daemons orchestrate. Rust workers handle heavy compute. The entity system stores data. Generators encode patterns. Logging observes it all. This is the machinery that makes Continuum run.

**Status:** Core infrastructure operational. 15+ daemons, Rust worker IPC, multi-database handles, entity system, generator pipeline all proven.

---

## Documents

### AI Providers & Inference

| Document | Summary |
|----------|---------|
| [AI-PROVIDER-WORKER-ARCHITECTURE](AI-PROVIDER-WORKER-ARCHITECTURE.md) | Core architecture for AI provider workers: routing, model selection, provider abstraction |
| [AI-PROVIDER-MIGRATION](AI-PROVIDER-MIGRATION.md) | Migration plan for AI provider refactoring and consolidation |
| [AI-PROVIDER-TESTING-STRATEGY](AI-PROVIDER-TESTING-STRATEGY.md) | Testing approach for AI providers: mocking, integration, provider-specific edge cases |
| [AI-INFRASTRUCTURE-DASHBOARD](AI-INFRASTRUCTURE-DASHBOARD.md) | Observability dashboard for AI infrastructure: latency, token usage, error rates |
| [AI-ADAPTER-ARCHITECTURE-REFACTOR](AI-ADAPTER-ARCHITECTURE-REFACTOR.md) | Refactoring AI adapters to clean interface-driven design |
| [OLLAMA-WORKER-ARCHITECTURE](OLLAMA-WORKER-ARCHITECTURE.md) | Ollama-specific worker design: local model management, GPU sharing |
| [OLLAMA-QUEUE-COORDINATION](OLLAMA-QUEUE-COORDINATION.md) | Queue coordination for Ollama requests: batching, priority, concurrency limits |
| [AI-REPORTED-TOOL-ISSUES](AI-REPORTED-TOOL-ISSUES.md) | Catalog of tool-calling issues reported by AI personas during operation |
| [AI-TOOL-CALLING-TROUBLESHOOTING](AI-TOOL-CALLING-TROUBLESHOOTING.md) | Troubleshooting guide for AI tool-calling failures: parsing, timeouts, schema mismatches |

### Daemons & Concurrency

| Document | Summary |
|----------|---------|
| [DAEMON-CONCURRENCY-PATTERN](DAEMON-CONCURRENCY-PATTERN.md) | Core concurrency pattern for daemons: async initialization, lifecycle management |
| [DAEMON-CONCURRENCY-AUDIT](DAEMON-CONCURRENCY-AUDIT.md) | Audit of concurrency issues across all daemons: race conditions, deadlocks, fixes |
| [DAEMON-RESPONSIBILITIES](DAEMON-RESPONSIBILITIES.md) | Responsibility matrix for all daemons: what each owns, separation of concerns |
| [DAEMON-BASE-CLASS-EXTRACTION](DAEMON-BASE-CLASS-EXTRACTION.md) | Extracting shared daemon behavior into BaseDaemon base class |
| [DAEMON-LOGGING-STANDARDIZATION](DAEMON-LOGGING-STANDARDIZATION.md) | Standardizing logging across all daemons: format, levels, output paths |
| [CONCURRENT-DAEMON-ARCHITECTURE](CONCURRENT-DAEMON-ARCHITECTURE.md) | Architecture for running daemons concurrently: startup ordering, dependency resolution |

### Data & ORM

| Document | Summary |
|----------|---------|
| [ENTITY-ARCHITECTURE](ENTITY-ARCHITECTURE.md) | Entity system design: BaseEntity inheritance, decorator-driven schema, prototype chain metadata |
| [ENTITY-BASED-CONFIGURATION-SYSTEM](ENTITY-BASED-CONFIGURATION-SYSTEM.md) | Using the entity system for configuration storage: typed settings as entities |
| [ENTITY-EVOLUTION-PLAN](ENTITY-EVOLUTION-PLAN.md) | Evolution plan for entity system: migrations, versioning, backward compatibility |
| [ENTITY-HYGIENE-SYSTEM](ENTITY-HYGIENE-SYSTEM.md) | Automated entity hygiene: orphan detection, consistency checks, cleanup |
| [ELEGANT-CRUD-ARCHITECTURE](ELEGANT-CRUD-ARCHITECTURE.md) | Clean CRUD operations through the data daemon: generic typed operations |
| [entity-adapter-architecture](entity-adapter-architecture.md) | Storage adapter abstraction: SQLite, JSON, vector DB behind a unified interface |
| [RUST-ORM-ARCHITECTURE](RUST-ORM-ARCHITECTURE.md) | Rust-side ORM design: rusqlite integration, type mapping, query building |
| [RUST-STYLE-DEFAULTS-PLAN](RUST-STYLE-DEFAULTS-PLAN.md) | Applying Rust-style default patterns to TypeScript entity initialization |

### Commands & Events

| Document | Summary |
|----------|---------|
| [EVENT-COMMANDS-ARCHITECTURE](EVENT-COMMANDS-ARCHITECTURE.md) | How Commands and Events work together: the two universal primitives |
| [EVENT-STATE-ARCHITECTURE](EVENT-STATE-ARCHITECTURE.md) | Event-driven state management: state changes as events, derived state |
| [EVENTS_UNIFICATION_PLAN](EVENTS_UNIFICATION_PLAN.md) | Plan to unify all event systems into one consistent Events API |
| [UNIFIED_EVENTS_COMPLETE](UNIFIED_EVENTS_COMPLETE.md) | Completion report for events unification: what changed, migration guide |
| [CALLER-ADAPTIVE-OUTPUTS](CALLER-ADAPTIVE-OUTPUTS.md) | Commands that adapt output format based on caller context (CLI vs API vs AI) |
| [CLI-ARRAY-PARAMETERS](CLI-ARRAY-PARAMETERS.md) | Handling array parameters in CLI: parsing, serialization, edge cases |
| [DECORATOR-DRIVEN-SCHEMA](DECORATOR-DRIVEN-SCHEMA.md) | Schema generation from TypeScript decorators: command params, result types |

### Rust Workers & IPC

| Document | Summary |
|----------|---------|
| [RUST-WORKER-IPC-PROTOCOL](RUST-WORKER-IPC-PROTOCOL.md) | IPC protocol between TypeScript and Rust: Unix sockets, JSON framing, error handling |
| [RUST-WORKER-REGISTRATION-PATTERN](RUST-WORKER-REGISTRATION-PATTERN.md) | How Rust workers register with the TypeScript command system |
| [RUST-WORKER-DUAL-PATH-PATTERN](RUST-WORKER-DUAL-PATH-PATTERN.md) | Dual-path pattern: commands handled in Rust vs forwarded to TypeScript |
| [RUST-WORKER-PATH-ANALYSIS](RUST-WORKER-PATH-ANALYSIS.md) | Analysis of command routing paths through the Rust worker layer |
| [RUST-DATA-DAEMON-VISION](RUST-DATA-DAEMON-VISION.md) | Vision for moving the data daemon to Rust: performance, SQLite native access |
| [RUST-DATA-WORKER-ARCHITECTURE](RUST-DATA-WORKER-ARCHITECTURE.md) | Architecture for Rust-backed data operations: query execution, type mapping |
| [UNIVERSAL-RUST-WORKER-PATTERN](UNIVERSAL-RUST-WORKER-PATTERN.md) | Universal pattern for all Rust workers: lifecycle, IPC, error propagation |
| [ARCHIVE-WORKER-DESIGN](ARCHIVE-WORKER-DESIGN.md) | Design for archive/compression worker in Rust: backup, export, snapshot |

### Code Generation

| Document | Summary |
|----------|---------|
| [GENERATOR-OOP-PHILOSOPHY](GENERATOR-OOP-PHILOSOPHY.md) | Core philosophy: generators ensure structural correctness at creation, OOP ensures behavioral correctness at runtime |
| [GENERATOR-IMPROVEMENT-ARCHITECTURE](GENERATOR-IMPROVEMENT-ARCHITECTURE.md) | Architecture for improving the generator system: templates, validation, testing |
| [GENERATOR-NEXT-STEPS](GENERATOR-NEXT-STEPS.md) | Immediate next steps for generator development |
| [GENERATOR-ROADMAP](GENERATOR-ROADMAP.md) | Long-term roadmap for the generation system |
| [UNIFIED-GENERATION-SYSTEM](UNIFIED-GENERATION-SYSTEM.md) | Unified generation: commands, daemons, widgets all from one generator pipeline |
| [TDD-IN-TEMPLATES](TDD-IN-TEMPLATES.md) | Test-driven development baked into generator templates: tests generated alongside code |

### Logging & Observability

| Document | Summary |
|----------|---------|
| [LOGGING](LOGGING.md) | Logging overview: philosophy, levels, output destinations |
| [LOGGING-SYSTEM](LOGGING-SYSTEM.md) | Logging system architecture: daemon, transport, aggregation |
| [LOGGING-MODULES](LOGGING-MODULES.md) | Per-module logging configuration: categories, verbosity, filtering |
| [LOGGING-PATHS-DESIGN](LOGGING-PATHS-DESIGN.md) | Log file path design: directory structure, rotation, per-persona paths |
| [LOGGER-DAEMON-VERIFICATION](LOGGER-DAEMON-VERIFICATION.md) | Verification that the logger daemon correctly captures all log sources |
| [MULTI-DIMENSIONAL-LOG-NAVIGATION](MULTI-DIMENSIONAL-LOG-NAVIGATION.md) | Navigating logs by time, persona, module, severity simultaneously |
| [OBSERVABILITY-ARCHITECTURE](OBSERVABILITY-ARCHITECTURE.md) | Full observability stack: logging, metrics, tracing, dashboards |

### RAG & Context

| Document | Summary |
|----------|---------|
| [CODEBASE-RAG-DESIGN](CODEBASE-RAG-DESIGN.md) | RAG design for codebase understanding: chunking, embedding, retrieval |
| [CODEBASE-RAG-IMPLEMENTATION](CODEBASE-RAG-IMPLEMENTATION.md) | Implementation details for codebase RAG: file processing, index building |
| [RAG-CONTEXT-BUDGET-SYSTEM](RAG-CONTEXT-BUDGET-SYSTEM.md) | Context budget management: token allocation, priority ranking, truncation strategy |

### System Architecture

| Document | Summary |
|----------|---------|
| [CONTINUUM-STATE-ARCHITECTURE](CONTINUUM-STATE-ARCHITECTURE.md) | Global system state management: initialization, lifecycle, shutdown |
| [SYSTEM-CONFIG-ARCHITECTURE](SYSTEM-CONFIG-ARCHITECTURE.md) | Configuration system: sources, merging, validation, hot-reload |
| [SYSTEM-DAEMON-ARCHITECTURE](SYSTEM-DAEMON-ARCHITECTURE.md) | System daemon design: the orchestrator that manages all other daemons |
| [SYSTEM-PATHS-MIGRATION](SYSTEM-PATHS-MIGRATION.md) | Migration of hardcoded paths to centralized path constants |
| [ARCHITECTURE_INCONSISTENCIES](ARCHITECTURE_INCONSISTENCIES.md) | Catalog of architectural inconsistencies found during audit |
| [RUST-TS-INFERENCE-ARCHITECTURE](RUST-TS-INFERENCE-ARCHITECTURE.md) | Architecture for Rust-TypeScript inference boundary: type generation, IPC typing |
| [STORAGE-ADAPTER-ABSTRACTION](STORAGE-ADAPTER-ABSTRACTION.md) | Storage adapter interface: abstracting SQLite/JSON/vector behind one API |

### Testing & Trust

| Document | Summary |
|----------|---------|
| [TDD-TRUST-MODEL](TDD-TRUST-MODEL.md) | Trust model for testing: what tests prove, confidence levels, coverage strategy |
| [ENVIRONMENT-AWARE-TESTING](ENVIRONMENT-AWARE-TESTING.md) | Tests that adapt to environment: CI vs local, with/without GPU, mocked vs live |
| [CRUD-EVENT-TEST-ARCHITECTURE](CRUD-EVENT-TEST-ARCHITECTURE.md) | Testing architecture for CRUD operations and event emission: integration test patterns |

### Real-Time & Streaming

| Document | Summary |
|----------|---------|
| [REAL-TIME-ARCHITECTURE](REAL-TIME-ARCHITECTURE.md) | Real-time communication architecture: WebSocket, event streaming, latency |
| [RECURSIVE-CONTEXT-ARCHITECTURE](RECURSIVE-CONTEXT-ARCHITECTURE.md) | Recursive context building: nested context assembly for deep conversation threads |

### Database Management

| Document | Summary |
|----------|---------|
| [MULTI-DATABASE-HANDLES](MULTI-DATABASE-HANDLES.md) | Multi-database handle system: open/close/route to multiple SQLite databases |
| [MULTI-DATABASE-IMPLEMENTATION-STATUS](MULTI-DATABASE-IMPLEMENTATION-STATUS.md) | Implementation status for multi-database: what ships, what is planned |
| [MULTI-DATABASE-SECURITY](MULTI-DATABASE-SECURITY.md) | Security model for multi-database: isolation, access control, handle permissions |
| [DATABASE-OPTIMIZATION-REPORT](DATABASE-OPTIMIZATION-REPORT.md) | Database performance analysis: query optimization, indexing, connection pooling |

### GPU & Resource Management

| Document | Summary |
|----------|---------|
| [GPU-MEMORY-ARCHITECTURE](GPU-MEMORY-ARCHITECTURE.md) | GPU memory manager: RAII guards, subsystem budgets, pressure levels, hardware detection |
| [RESOURCE-GOVERNANCE-ARCHITECTURE](RESOURCE-GOVERNANCE-ARCHITECTURE.md) | Five-layer resource governance: from priority allocation to AI-driven sentinel control |
| [SEMANTIC-SEARCH-ARCHITECTURE](SEMANTIC-SEARCH-ARCHITECTURE.md) | Semantic search infrastructure: embedding generation, vector indexing, similarity queries |
| [INDEX-MANAGEMENT-GUIDE](INDEX-MANAGEMENT-GUIDE.md) | Guide for managing database and search indexes: creation, maintenance, monitoring |
| [MODEL-DOWNLOAD-SYSTEM](MODEL-DOWNLOAD-SYSTEM.md) | Automated model download system: sources, caching, verification, retry |

### JTAG & Command System

| Document | Summary |
|----------|---------|
| [JTAG_SYSTEM_ANALYSIS](JTAG_SYSTEM_ANALYSIS.md) | Analysis of the JTAG debug/command system: architecture, capabilities, limitations |
| [JTAG_CLIENT_UNIFICATION](JTAG_CLIENT_UNIFICATION.md) | Unifying JTAG client implementations: one client for CLI, browser, and tests |
| [JTAG_COMMAND_ARCHITECTURE_REDESIGN](JTAG_COMMAND_ARCHITECTURE_REDESIGN.md) | Redesign of command architecture: modular discovery, self-contained commands |
| [COMMAND-ARCHITECTURE-AUDIT](COMMAND-ARCHITECTURE-AUDIT.md) | Audit of command system: anti-patterns found, violations, remediation |
| [COMMAND-VIOLATIONS-AUDIT](COMMAND-VIOLATIONS-AUDIT.md) | Specific violations of the modular command architecture and fixes |
| [SHAREABLE-COMMAND-MODULES](SHAREABLE-COMMAND-MODULES.md) | Making command modules shareable across projects: packaging, dependencies |

### Authentication & Security

| Document | Summary |
|----------|---------|
| [PASSKEY-AUTHENTICATION-DESIGN](PASSKEY-AUTHENTICATION-DESIGN.md) | Passkey/WebAuthn authentication design: registration, verification, device management |

### Patterns & Philosophy

| Document | Summary |
|----------|---------|
| [PATTERNS](PATTERNS.md) | Catalog of recurring patterns in the codebase: when to use each |
| [DESIGN-PRINCIPLE-NATURAL-IDIOMS](DESIGN-PRINCIPLE-NATURAL-IDIOMS.md) | Design principle: APIs should feel like natural language idioms |
| [MODULAR-DEVELOPMENT-PHILOSOPHY](MODULAR-DEVELOPMENT-PHILOSOPHY.md) | Philosophy of modular development: self-contained modules, dynamic discovery |
| [ADAPTER-ARCHITECTURE](ADAPTER-ARCHITECTURE.md) | General adapter pattern: interface, factory, runtime selection |
| [META-LANGUAGE-DESIGN](META-LANGUAGE-DESIGN.md) | Meta-language for describing system behavior: DSL concepts, pipeline definitions |

### Runtime & Migration

| Document | Summary |
|----------|---------|
| [ZERO-DOWNTIME-DEVELOPMENT](ZERO-DOWNTIME-DEVELOPMENT.md) | Zero-downtime development workflow: hot reload, live patching, graceful restart |
| [UNIFIED-RUNTIME-MIGRATION](UNIFIED-RUNTIME-MIGRATION.md) | Migrating to a unified runtime: consolidating server, worker, and CLI entry points |
| [UNIFIED_CLIENT_API](UNIFIED_CLIENT_API.md) | Unified client API: one interface for all command execution contexts |
| [MCP-INTEGRATION](MCP-INTEGRATION.md) | Model Context Protocol integration: connecting to MCP-compatible AI tooling |

### Related (other chapters)

| Document | Chapter | Relevance |
|----------|---------|-----------|
| [SENTINEL-ARCHITECTURE](../sentinel/SENTINEL-ARCHITECTURE.md) | Sentinel | Pipeline engine that orchestrates long-running infrastructure tasks |
| [SENTINEL-PIPELINE-ARCHITECTURE](../sentinel/SENTINEL-PIPELINE-ARCHITECTURE.md) | Sentinel | Step types, execution model, and IPC for pipeline steps |
| [GENOME-ARCHITECTURE](../genome/GENOME-ARCHITECTURE.md) | Genome | LoRA training infrastructure, adapter store, PEFT pipeline |
| [TRAINING-SYSTEM-ARCHITECTURE](../genome/TRAINING-SYSTEM-ARCHITECTURE.md) | Genome | Training data pipelines that depend on data/ORM infrastructure |
| [PERSONA-CONVERGENCE-ROADMAP](../personas/PERSONA-CONVERGENCE-ROADMAP.md) | Personas | PersonaUser autonomous loop depends on daemons and command system |
| [USER_DAEMON_ARCHITECTURE](../personas/USER_DAEMON_ARCHITECTURE.md) | Personas | User daemon that manages persona lifecycle via infrastructure primitives |
| [VOICE-ARCHITECTURE](../live/VOICE-ARCHITECTURE.md) | Live | Media workers using Rust worker IPC for audio/video processing |
| [POSITRON-ARCHITECTURE](../positron/POSITRON-ARCHITECTURE.md) | Positron | Widget system that consumes Events and executes Commands |

---

## The Two Primitives

```
Commands.execute(name, params) → Response     // Request/Response
Events.subscribe(name, handler)               // Publish/Subscribe
Events.emit(name, data)
```

Everything is built on these. Local calls are direct. Remote calls go over WebSocket. Same API everywhere.
