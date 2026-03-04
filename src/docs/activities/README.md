# Activities — Collaborative Experiences & Recipes

> Every tab is an activity. Every activity is a room. Rooms host humans and AIs working together — chatting, calling, coding, creating. Recipes define the template. Activities are the live instance. Walls persist shared documents across conversations.

**Status:** Core activity model operational. Recipe system Phase 4 complete. Wall system implemented.

---

## Documents

### Architecture

| Document | Summary |
|----------|---------|
| [ACTIVITY-ARCHITECTURE.md](ACTIVITY-ARCHITECTURE.md) | Template/Instance pattern: Recipe = Template (Class), Activity = Instance (Object). Core entities, services, events. |
| [ACTIVITY-CONVERGENCE-ARCHITECTURE.md](ACTIVITY-CONVERGENCE-ARCHITECTURE.md) | The fundamental equation: Tab == Activity. Unified content stream, modality as capability, hierarchy, sentinel as executor. |
| [ROOMS-AND-ACTIVITIES.md](ROOMS-AND-ACTIVITIES.md) | Universal experience model. Rooms are any shared experience — 3D worlds, movie nights, code reviews, AR sessions. Grid topology. |
| [THREADING-AS-THOUGHTSTREAM.md](THREADING-AS-THOUGHTSTREAM.md) | Multi-persona coordination via threaded messages. Threads as cognitive architecture: parallel work decomposition, sequential refinement, cross-thread references. |

### Collaboration

| Document | Summary |
|----------|---------|
| [HANDLE-ADDRESSABLE-OFFICE.md](HANDLE-ADDRESSABLE-OFFICE.md) | Everything is a subscribable handle (UUID). Tool execution as live chat elements. Streaming build output, expandable diffs, ambient workspace visibility. |
| [COLLABORATIVE-EDITING-SYSTEM.md](COLLABORATIVE-EDITING-SYSTEM.md) | Lease-based file access for AI teams. Time-limited exclusive write access with auto-expiration, staged edits, human override. |
| [collaboration/PIN-AND-TASK-SYSTEMS.md](collaboration/PIN-AND-TASK-SYSTEMS.md) | Pin system (post-it reminders) and task system (structured work plans). Human-AI symmetric command access. RAG integration for context boosting. |
| [collaboration/MEMORY-TASK-PIN-HARMONY.md](collaboration/MEMORY-TASK-PIN-HARMONY.md) | OOP architecture for fluid conversions between memories, tasks, and pins. Cognition becomes actionable work through BaseEntity polymorphism. |

### Rooms & Walls

| Document | Summary |
|----------|---------|
| [ROOM-WALLS.md](ROOM-WALLS.md) | Shared document space per room. Persistent collaborative documents that survive across conversations. Gap between ephemeral chat and formal codebase docs. |
| [WALL-IMPLEMENTATION-ARCHITECTURE.md](WALL-IMPLEMENTATION-ARCHITECTURE.md) | Implementation details for wall/write, wall/read, wall/list commands. Lease system integration. Phase 1 (file-based) and Phase 2 (lease-governed) plans. |

### Recipe System

| Document | Summary |
|----------|---------|
| [recipes/RECIPES.md](recipes/RECIPES.md) | Master vision: composable command pipelines defining how humans and AIs collaborate. Context gathering, decision making, action execution, artifact storage. |
| [recipes/RECIPE-SYSTEM-REQUIREMENTS.md](recipes/RECIPE-SYSTEM-REQUIREMENTS.md) | Requirements derived from real use cases (Thronglets, Tarot Reading, chat system). Gap analysis against current implementation. |
| [recipes/RECIPE-SYSTEM-STATUS.md](recipes/RECIPE-SYSTEM-STATUS.md) | Implementation status. Core infrastructure complete: recipe/load, rag/build commands, RecipeEntity, JSON templates. |
| [recipes/RECIPE-LEARNING-DYNAMICS.md](recipes/RECIPE-LEARNING-DYNAMICS.md) | Team learning orchestration. Recipes as team operating systems: roles, coordination patterns, AI-determined learning parameters. |
| [recipes/RECIPE-DRIVEN-INVENTION.md](recipes/RECIPE-DRIVEN-INVENTION.md) | End-to-end automation vision. User idea to deployed solution via recipes. Autonomous development at machine speed. |
| [recipes/SCOPE-BASED-RECIPES.md](recipes/SCOPE-BASED-RECIPES.md) | Context-aware recipes. Any directory becomes a collaboration workspace. Filesystem scope narrows RAG, genome, and available resources. |
| [recipes/PRACTICAL-IMPLEMENTATION-PLAN.md](recipes/PRACTICAL-IMPLEMENTATION-PLAN.md) | Implementation roadmap. Phase-by-phase plan from current task/genome commands to full recipe learning system. |

### Related (other chapters)

| Document | Chapter | Relevance |
|----------|---------|-----------|
| [PERSONA-CONVERGENCE-ROADMAP.md](../personas/PERSONA-CONVERGENCE-ROADMAP.md) | Personas | Autonomous loop, self-managed queues, LoRA genome paging — the persona engine that executes activities |
| [PERSONA-GENOMIC-ARCHITECTURE.md](../personas/PERSONA-GENOMIC-ARCHITECTURE.md) | Personas | Genome paging per activity role. Learning is everywhere — academy dissolved into activities |
| [RECIPE-EMBEDDED-LEARNING.md](../personas/RECIPE-EMBEDDED-LEARNING.md) | Personas | Continuous learning within recipe execution. Training data accumulation during collaboration |
| [SENTINEL-ARCHITECTURE.md](../sentinel/SENTINEL-ARCHITECTURE.md) | Sentinel | Pipeline engine that executes recipe steps. Activities ARE sentinels with a UI |
| [SENTINEL-PIPELINE-ARCHITECTURE.md](../sentinel/SENTINEL-PIPELINE-ARCHITECTURE.md) | Sentinel | Step types (Shell, LLM, Command, Loop, Parallel) that compose recipe pipelines |
| [POSITRON-ARCHITECTURE.md](../positron/POSITRON-ARCHITECTURE.md) | Positron | Widget system that renders activity layouts. Reactive state for live activity UIs |
| [TABBED-BROWSER-ARCHITECTURE.md](../positron/TABBED-BROWSER-ARCHITECTURE.md) | Positron | Tab == Activity in the UI layer. Content navigation and tab management |
| [EVENT-COMMANDS-ARCHITECTURE.md](../infrastructure/EVENT-COMMANDS-ARCHITECTURE.md) | Infrastructure | The two universal primitives (Commands + Events) that activities are built on |
| [AI-GOVERNANCE-RECIPES.md](../governance/AI-GOVERNANCE-RECIPES.md) | Governance | Governance as recipe-driven policy. Democratic decision-making within activities |

---

## The Activity Model

```
Recipe (Template)          Activity (Instance)           Room (Tab)
─────────────────          ───────────────────           ──────────
pipeline steps             participants[]                visible in sidebar
RAG template               mutable state                 rendered by recipe layout
strategy rules             modality state (call,         handle-addressable
layout definition            canvas, code)
role definitions           content stream                subscribable via Events
genome requirements        parent/child hierarchy
```

```
Recipe  ──instantiate──>  Activity  ──renders as──>  Tab
                              │
                              ├── participants (humans + AIs)
                              ├── content stream (text, voice, canvas, code, game)
                              ├── modality state (call active, canvas open)
                              ├── sentinel executor (runs pipeline steps)
                              └── child activities (hierarchy)
```

Every handle is a UUID. Every UUID is subscribable. `Events.subscribe('handle:${activityId}', callback)` gives live updates.

---

## Key Commands

```bash
# Rooms
./jtag collaboration/chat/send --room="general" --message="Hello team"
./jtag collaboration/chat/export --room="general" --limit=50

# Walls
./jtag wall/write --room="general" --doc="meeting-notes.md" --content="# Notes\n..."
./jtag wall/read --room="general" --doc="meeting-notes.md"
./jtag wall/list --room="general"

# Recipes
./jtag recipe/load --loadAll=true
./jtag rag/build --contextId="<room-uuid>" --personaId="<persona-uuid>"

# Activities (future)
./jtag activity/create --recipe="code-review" --owner="joel"
./jtag activity/spawn --parentId="<id>" --recipe="canvas" --name="Sketches"
./jtag activity/list --status=active
```
