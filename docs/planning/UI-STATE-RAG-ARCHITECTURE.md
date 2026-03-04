# UI State → RAG Architecture

## Problem

AI assistants (PersonaUsers) embedded in widgets need contextual awareness of UI state to provide relevant help. Currently they only see chat messages - they don't know:
- What screen the user is viewing
- Current widget state (configured providers, errors, pending changes)
- User actions (clicks, inputs, navigation)
- Real-time changes

## Solution: "UI State as Data"

Treat UI state like any other entity in the data layer. This gives us:
- **Events** for real-time changes (like React's useEffect)
- **Queries** for current state
- **Auto-embeddings** for semantic RAG search
- **History** for temporal context

## Architecture

```
┌─────────────────┐     Events      ┌──────────────────┐
│  Widget         │ ──────────────► │   ui_state       │
│  (publishState) │  data:ui_state  │   collection     │
└─────────────────┘    :updated     │  (auto-embedded) │
                                    └────────┬─────────┘
                                             │
        ┌────────────────────────────────────┤
        │                                    │
        ▼                                    ▼
┌───────────────┐                   ┌─────────────────┐
│ PersonaInbox  │                   │ RAGBuilder      │
│ (event → task)│                   │ +loadUIState()  │
└───────────────┘                   └─────────────────┘
```

### Two Data Paths

1. **Push (Events)**: UI change → event → PersonaInbox task
   - Persona notices something changed
   - Can proactively offer help
   - Priority calculated like chat messages

2. **Pull (Query)**: RAG build → ui_state collection
   - Persona understands current context when responding
   - Semantic search across all widget states
   - Cross-widget awareness

## Data Model

### UIStateEntity

```typescript
interface UIStateEntity extends BaseEntity {
  // Identity
  widgetType: string;           // 'settings', 'chat', 'academy', etc.
  widgetInstanceId: string;     // Unique widget instance
  userId: UUID;                 // Which user's UI
  sessionId: UUID;              // Which browser session

  // State (widget-specific)
  state: Record<string, unknown>;

  // For RAG embedding
  summary: string;              // Human-readable state summary

  // Metadata
  timestamp: number;
  version: number;              // For optimistic updates
}
```

### Example: Settings Widget State

```typescript
{
  widgetType: 'settings',
  widgetInstanceId: 'settings-main',
  userId: 'joel-uuid',
  sessionId: 'browser-session-uuid',

  state: {
    providers: [
      { name: 'Anthropic', key: 'ANTHROPIC_API_KEY', status: 'operational', responseTime: 715, configured: true },
      { name: 'OpenAI', key: 'OPENAI_API_KEY', status: 'operational', responseTime: 602, configured: true },
      { name: 'xAI', key: 'XAI_API_KEY', status: 'rate-limited', configured: true },
      { name: 'Together', key: 'TOGETHER_API_KEY', status: 'untested', configured: false, pendingKey: true }
    ],
    pendingChanges: ['TOGETHER_API_KEY'],
    errors: [],
    currentSection: 'cloud-providers'
  },

  // Auto-generated for RAG
  summary: "Settings: Anthropic operational (715ms), OpenAI operational (602ms), xAI rate-limited, Together pending new key"
}
```

### Example: Game Widget State

```typescript
{
  widgetType: 'game',
  widgetInstanceId: 'chess-game-123',
  userId: 'joel-uuid',

  state: {
    gameType: 'chess',
    turn: 'white',
    moveCount: 24,
    position: 'fen-string',
    lastMove: 'e4e5',
    timeRemaining: { white: 300, black: 280 },
    status: 'in-progress'
  },

  summary: "Chess game move 24, white to play, 5min remaining, last move e4-e5"
}
```

## BaseWidget Integration

```typescript
abstract class BaseWidget extends HTMLElement {
  private uiStateEntityId?: UUID;
  private statePublishDebounce?: NodeJS.Timeout;

  /**
   * Publish current UI state - called by widgets after state changes
   * Debounced to avoid flooding during rapid updates
   */
  protected publishUIState(state: object): void {
    if (this.statePublishDebounce) {
      clearTimeout(this.statePublishDebounce);
    }

    this.statePublishDebounce = setTimeout(async () => {
      const summary = this.generateStateSummary(state);

      if (this.uiStateEntityId) {
        // Update existing
        await Commands.execute('data/update', {
          collection: 'ui_state',
          id: this.uiStateEntityId,
          data: { state, summary, timestamp: Date.now() }
        });
      } else {
        // Create new
        const result = await Commands.execute('data/create', {
          collection: 'ui_state',
          data: {
            widgetType: this.widgetName,
            widgetInstanceId: this.instanceId,
            userId: this.userId,
            sessionId: this.sessionId,
            state,
            summary,
            timestamp: Date.now()
          }
        });
        this.uiStateEntityId = result.id;
      }
    }, 100); // 100ms debounce
  }

  /**
   * Override in widgets to generate human-readable summary for RAG
   */
  protected generateStateSummary(state: object): string {
    return `${this.widgetName}: ${JSON.stringify(state).slice(0, 200)}`;
  }

  /**
   * Cleanup on disconnect
   */
  disconnectedCallback(): void {
    if (this.uiStateEntityId) {
      Commands.execute('data/delete', {
        collection: 'ui_state',
        id: this.uiStateEntityId
      });
    }
  }
}
```

## RAG Integration

### New RAG Source: loadUIState()

Added to ChatRAGBuilder (or new UIAwareRAGBuilder):

```typescript
private async loadUIState(contextId: UUID, userId: UUID): Promise<UIStateContext> {
  // Get current UI state for this user
  const result = await Commands.execute('data/list', {
    collection: 'ui_state',
    filter: { userId },
    orderBy: [{ field: 'timestamp', direction: 'desc' }],
    limit: 5  // Last 5 widget states
  });

  return {
    currentStates: result.items,
    summaries: result.items.map(s => s.summary).join('\n')
  };
}
```

### RAGContext Extension

```typescript
interface RAGContext {
  // Existing...
  domain: RAGDomain;
  identity: PersonaIdentity;
  conversationHistory: LLMMessage[];
  artifacts: RAGArtifact[];
  privateMemories: PersonaMemory[];
  recipeStrategy: RecipeStrategy;

  // NEW
  uiState?: UIStateContext;
}
```

### System Prompt Integration

```typescript
function buildSystemPrompt(context: RAGContext): string {
  let prompt = context.identity.systemPrompt;

  if (context.uiState?.summaries) {
    prompt += `\n\n## Current UI Context\n${context.uiState.summaries}`;
  }

  return prompt;
}
```

## Event Flow: Persona Reacts to UI Changes

### 1. Widget publishes state change

```typescript
// In SettingsWidget after test completes
this.publishUIState({
  providers: this.getProviderStates(),
  pendingChanges: Array.from(this.pendingChanges.keys())
});
```

### 2. Data layer emits event

```typescript
// DataDaemon automatically emits
Events.emit('data:ui_state:updated', { entity, changes });
```

### 3. PersonaInbox receives (if subscribed)

```typescript
// Persona subscribes to UI state changes for its room
Events.subscribe('data:ui_state:updated', (event) => {
  if (event.entity.widgetType === this.activeRoom?.widgetContext) {
    this.inbox.enqueue({
      type: 'ui-state-change',
      priority: 0.3,  // Lower than direct mentions
      data: event
    });
  }
});
```

### 4. Persona may proactively help

```typescript
// In serviceInbox, persona sees UI state change
// Could generate proactive message:
"I noticed your xAI key is being rate-limited. This usually resolves in a few minutes,
or you can check your usage at https://console.x.ai/billing"
```

## Semantic Search Across UI State

Because ui_state entities are auto-embedded, personas can search semantically:

```typescript
// "What is the user struggling with?"
const relevant = await Commands.execute('data/vector-search', {
  collection: 'ui_state',
  query: 'errors problems issues struggling',
  limit: 5
});

// Returns: xAI rate-limited state, any error states, etc.
```

## Future Extensions

### Cross-Widget Awareness

Settings assistant knows about chat context:
```
"I see you were discussing API pricing in chat. The providers you're configuring
here (Anthropic, OpenAI) have different pricing models..."
```

### User Action Tracking

```typescript
{
  widgetType: 'settings',
  state: {
    // ...
    recentActions: [
      { action: 'click', target: 'test-anthropic', timestamp: 1234 },
      { action: 'input', target: 'TOGETHER_API_KEY', value: 'sk-***', timestamp: 1235 },
      { action: 'click', target: 'test-together', timestamp: 1236 }
    ]
  }
}
```

### Multi-Session Awareness

Same user on multiple devices:
```typescript
filter: { userId, sessionId: { $ne: currentSession } }
// "You have the same page open in another tab..."
```

## Implementation Phases

### Phase 1: Foundation
- [ ] Create UIStateEntity type
- [ ] Add ui_state collection to EntityRegistry
- [ ] Implement BaseWidget.publishUIState()

### Phase 2: RAG Integration
- [ ] Add loadUIState() to RAGBuilder
- [ ] Extend RAGContext with uiState
- [ ] Update system prompt builder

### Phase 3: Event-Driven Awareness
- [ ] PersonaInbox subscribes to ui_state events
- [ ] Priority calculation for UI events
- [ ] Proactive assistance triggers

### Phase 4: Semantic Search
- [ ] Auto-embedding for ui_state summaries
- [ ] Vector search integration
- [ ] Cross-widget queries

---

*This architecture extends the existing data/event patterns to give AI assistants contextual UI awareness without screenshots.*
