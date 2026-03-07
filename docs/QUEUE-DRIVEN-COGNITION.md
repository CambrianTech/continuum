# Queue-Driven Cognition — The Fundamental Architecture

> The mind controls its own destiny. RAG, memory, and thought processes are sacred.
> The persona decides what context it needs based on what it's servicing.

## The Core Principle

**Every queue item declares its own RAG requirements.** The persona doesn't need hardcoded knowledge of what context to gather — the work itself carries that information, and the persona consolidates across the queue item's requirements before responding.

This is the same compression principle that governs the entire system:

```
BaseEntity declares collection + validate
    → CodeIndexEntity knows it lives in code_index
    → The ORM never hardcodes entity-specific logic

BaseQueueItem declares domain + RAG contract
    → A code review task knows it needs code context + git diff
    → The persona never hardcodes activity-specific RAG logic
```

## Why This Matters

Without this principle, every new activity requires modifying the persona's cognition:

```
BAD: Persona has hardcoded RAG logic per domain
├── if (domain === 'code') loadCodeContext()
├── if (domain === 'game') loadBoardState()
├── if (domain === 'writing') loadNovelContext()
└── Every new activity = modify PersonaUser (breaks modularity)
```

With this principle, activities are self-describing:

```
GOOD: Queue item carries its RAG contract
├── QueueItem.ragContract → declares what context sources to compose
├── Persona.consolidate(contract) → gathers context generically
├── New activity = new QueueItem type + recipe (zero persona changes)
└── Generator ensures structural correctness at creation time
```

## The Three Layers

### Layer 1: Queue Item (What Work)

The queue item is the unit of work entering a persona's inbox. It has a `domain` (chat, code, game, learning, creative, etc.) and carries metadata about what it needs:

```typescript
interface BaseQueueItem {
  id: UUID;
  type: 'message' | 'task';
  priority: number;
  domain: TaskDomain;
  timestamp: number;
}
```

Today, `domain` drives LoRA paging (activate the right adapter). Tomorrow, it also drives RAG composition.

### Layer 2: Recipe (How To Do The Work)

The recipe defines the activity pattern. It declares:

1. **RAG contract** — what context sources are needed and their budget allocation
2. **Pipeline** — the sequence of context → decision → action → artifacts
3. **Evaluation** — how to assess the outcome
4. **Memory contract** — what gets stored back and where

```typescript
// From the existing recipe design (RECIPES.md):
interface RecipeEntity {
  ragTemplate: {
    messageHistory: { maxMessages, orderBy, includeTimestamps };
    artifacts: { types, maxItems };
    participants: { ... };
    customSources: RAGSourceConfig[];  // ← THIS IS THE KEY
  };
}
```

The `ragTemplate` is the recipe's RAG contract. A chess recipe declares it needs board state and move history. A code review recipe declares it needs file diffs and test results. A novel-writing recipe declares it needs character sheets and plot outlines. The persona doesn't know about any of these — it just fulfills the contract.

### Layer 3: Persona Cognition (The Autonomous Mind)

The persona's cognitive loop is domain-agnostic:

```
1. Pop queue item (highest effective priority after aging)
2. Resolve recipe for this item's domain/context
3. Compose RAG context per recipe's ragTemplate
4. Consolidate across all context sources
5. Generate response/action using consolidated context
6. Store outcomes per recipe's memory contract
7. Update state (energy, attention, genome activation)
```

Steps 3-4 are where the magic happens. The persona doesn't decide *what* context to load — the recipe does. The persona decides *whether* to engage (energy/priority gating), *how much effort* to invest (adaptive quality), and *what to remember* (memory consolidation).

## RAG Source as Capability, Not Filter

RAG sources are capabilities the persona *can* use, not filters that decide for the persona:

| Source | Capability | When Used |
|--------|-----------|-----------|
| CodebaseSearchSource | Semantic code search | Recipe's ragTemplate includes `codebase` |
| ConversationHistorySource | Chat context | Recipe's ragTemplate includes `messages` |
| SemanticMemorySource | Long-term recall | Recipe's ragTemplate includes `memories` |
| GameStateSource (future) | Board/game state | Recipe's ragTemplate includes `game-state` |
| DocumentSource (future) | Novel/doc context | Recipe's ragTemplate includes `documents` |

A source is always *available* but only *activated* when the current queue item's recipe declares it. No keyword matching. No domain-specific if/else. The recipe is the single source of truth.

## The Consolidation Problem

When a persona services multiple queue items from different domains, it needs to consolidate:

```
Queue contains:
├── Chat message about code architecture (domain: chat, needs: code + messages)
├── Code review task (domain: code, needs: code + git + tests)
├── Self-generated learning task (domain: self, needs: memories + genome state)
```

Each item has its own RAG contract. The persona must:

1. **Batch** — items from the same domain can share context (one code search serves multiple code items)
2. **Prioritize** — limited context window means budget allocation across items
3. **Sequence** — some items depend on others (code review depends on understanding the chat discussion)

This is analogous to how a human developer checks Slack, reviews a PR, and updates their notes — different activities with different context needs, consolidated through a single cognitive process.

## Connection to Existing Architecture

### Commands & Events (Universal Primitives)
RAG sources use `Commands.execute()` for data retrieval. Recipe outcomes emit `Events` for downstream processing. The two primitives are sufficient.

### Generators
When a new activity type is created, a generator should produce:
- The QueueItem subtype with proper domain declaration
- The recipe with ragTemplate
- The RAG source if a new data type is needed
- Tests for the complete flow

### Sentinel Pipelines
Complex activities (academy, coding projects) use Sentinel pipelines where each step can have its own RAG requirements. The pipeline is the recipe at scale.

### Genome (LoRA Paging)
The queue item's domain drives adapter activation. A code domain activates the coding LoRA. A creative domain activates the creative writing LoRA. This already works via `PersonaAutonomousLoop.dispatchItem()`.

## Implementation Status

### Completed
1. **CodebaseSearchSource** — Proves the pattern (RAG source + vector search + system prompt injection). E2E: 180 files, 855 entries, 20s.
2. **Generic extraction** — `ChatRAGBuilder.extractFromComposition()` collects all `systemPromptSection` values into `Map<sourceName, string>`. Adding a new RAGSource requires ZERO changes to the builder.
3. **Recipe `ragTemplate.sources`** — `RAGTemplate` now has a `sources?: string[]` field declaring which RAG sources to activate.
4. **`RAGSourceContext.activeSources`** — Composer receives activation list. If present, only listed sources fire. If absent, all applicable sources fire (backwards compatible).
5. **End-to-end plumbing** — Recipe → `loadRecipeContext()` extracts `ragTemplateSources` → `RAGSourceContext.activeSources` → `RAGComposer.compose()` filters by list.

### Remaining
- **Queue item `recipeId`** — `BaseQueueItem` should carry an optional recipe reference so non-room-based tasks can declare RAG requirements without being tied to a room's recipe.
- **Domain-to-recipe mapping** — Default recipes per `TaskDomain` so even tasks without explicit recipe references get appropriate context.
- **Recipe generator** — Generator that produces recipe JSON with ragTemplate, strategy, tools, and pipeline for new activity types.

## The Test

For ANY activity (game, novel, code review, music composition, data analysis):

1. Can you define it as a recipe with a ragTemplate? → **Yes, always**
2. Does the persona need to know about it specifically? → **No, never**
3. Can a generator produce all the scaffolding? → **Yes, structural correctness at creation time**
4. Does it compose with existing activities? → **Yes, through the universal queue + priority system**

If all four are true, the architecture is correct. If any requires modifying PersonaUser or ChatRAGBuilder, the abstraction is leaking.
