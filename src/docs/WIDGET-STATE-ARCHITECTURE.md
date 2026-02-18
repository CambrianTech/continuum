# Widget State Architecture - useState-like Reactivity

## Current Problem

Tab switching takes 15+ seconds because:

1. **MainWidget destroys widgets**: `contentView.innerHTML = widgetHtml` (line 359) destroys and recreates widgets entirely
2. **Attribute changes trigger full re-renders**: Setting `room` attribute → `attributeChangedCallback` → `switchToRoom` → `refresh()` → rebuild ALL DOM
3. **No DOM diffing**: EntityScroller.refresh() clears everything and reloads from scratch
4. **Multiple event triggers**: 5+ different paths all call switchToRoom, causing cascading re-renders

## Desired Behavior (React/useState Model)

```typescript
// React pattern we want to emulate
const [roomId, setRoomId] = useState('general');

useEffect(() => {
  loadMessages(roomId);
}, [roomId]); // Only runs when roomId changes

return <MessageList messages={messages} />; // React diffs, only updates changed DOM
```

Key principles:
1. **State changes drive updates** - not events, not attribute changes
2. **Minimal DOM updates** - only changed elements update
3. **Single source of truth** - one store, all widgets subscribe
4. **No widget destruction** - widgets persist, only internal state changes

## Architecture Design

### 1. Centralized AppState Store

```typescript
// system/state/AppState.ts
import { signal, computed, batch } from '@preact/signals-core';

export const AppState = {
  // Core navigation state
  currentContentType: signal<string>('chat'),
  currentEntityId: signal<string | null>('general'),

  // Open tabs (VS Code-style)
  openTabs: signal<ContentItem[]>([]),
  activeTabId: signal<string | null>(null),

  // Computed values
  currentTab: computed(() => {
    const tabs = AppState.openTabs.value;
    const activeId = AppState.activeTabId.value;
    return tabs.find(t => t.id === activeId) || null;
  }),

  // Actions (batch updates to prevent intermediate states)
  switchTab(tabId: string) {
    batch(() => {
      const tab = AppState.openTabs.value.find(t => t.id === tabId);
      if (tab) {
        AppState.activeTabId.value = tabId;
        AppState.currentContentType.value = tab.type;
        AppState.currentEntityId.value = tab.entityId || null;
      }
    });
  },

  openContent(type: string, entityId?: string) {
    batch(() => {
      // Add tab if not exists
      const existingTab = AppState.openTabs.value.find(t =>
        t.type === type && t.entityId === entityId
      );

      if (!existingTab) {
        const newTab = { id: crypto.randomUUID(), type, entityId };
        AppState.openTabs.value = [...AppState.openTabs.value, newTab];
        AppState.activeTabId.value = newTab.id;
      } else {
        AppState.activeTabId.value = existingTab.id;
      }

      AppState.currentContentType.value = type;
      AppState.currentEntityId.value = entityId || null;
    });
  }
};
```

### 2. MainWidget Uses Store (No innerHTML Destruction)

```typescript
// MainWidget.ts
class MainWidget extends BaseWidget {
  private widgetCache = new Map<string, HTMLElement>(); // Persist widgets!
  private disposeEffects: Dispose[] = [];

  protected async onWidgetInitialize() {
    // Watch store for content changes
    this.disposeEffects.push(
      effect(() => {
        const contentType = AppState.currentContentType.value;
        const entityId = AppState.currentEntityId.value;
        this.showWidget(contentType, entityId);
      })
    );
  }

  private showWidget(contentType: string, entityId?: string) {
    const contentView = this.shadowRoot?.querySelector('.content-view');
    if (!contentView) return;

    const widgetTag = getWidgetForType(contentType);
    const cacheKey = `${widgetTag}-${entityId || 'default'}`;

    // Hide all children
    for (const child of contentView.children) {
      (child as HTMLElement).style.display = 'none';
    }

    // Get or create widget
    let widget = this.widgetCache.get(cacheKey);

    if (!widget) {
      widget = document.createElement(widgetTag);
      if (entityId) {
        widget.setAttribute('entity-id', entityId);
      }
      contentView.appendChild(widget);
      this.widgetCache.set(cacheKey, widget);
    }

    // Show the widget
    widget.style.display = '';

    // Notify widget it's now active (optional)
    if ('onActivate' in widget && typeof widget.onActivate === 'function') {
      widget.onActivate(entityId);
    }
  }
}
```

### 3. ChatWidget Connects to Store (No Attribute Watching)

```typescript
// ChatWidget.ts
class ChatWidget extends EntityScrollerWidget<ChatMessageEntity> {
  private roomSignals = createWidgetSignals<{
    roomId: string | null;
    messages: ChatMessageEntity[];
    isLoading: boolean;
  }>({ roomId: null, messages: [], isLoading: false });

  private disposeEffects: Dispose[] = [];

  protected async onWidgetInitialize() {
    // Connect to AppState - watch for room changes
    this.disposeEffects.push(
      effect(() => {
        const contentType = AppState.currentContentType.value;
        const entityId = AppState.currentEntityId.value;

        // Only respond if we're the active content type
        if (contentType === 'chat' && entityId !== this.roomSignals.state.roomId) {
          this.loadRoom(entityId);
        }
      })
    );

    // Watch local room state - update messages when room changes
    this.disposeEffects.push(
      watch(this.roomSignals.getSignal('roomId'), (newRoom, oldRoom) => {
        if (newRoom && newRoom !== oldRoom) {
          this.loadMessages(newRoom);
        }
      })
    );

    // Watch messages - update DOM efficiently
    this.disposeEffects.push(
      watch(this.roomSignals.getSignal('messages'), (newMessages, oldMessages) => {
        this.diffUpdateMessages(newMessages, oldMessages || []);
      })
    );
  }

  private async loadRoom(roomId: string | null) {
    if (!roomId) return;
    this.roomSignals.set('roomId', roomId);
  }

  private async loadMessages(roomId: string) {
    this.roomSignals.set('isLoading', true);

    // Check cache first
    const cached = ChatMessageCache.get(roomId);
    if (cached) {
      this.roomSignals.set('messages', cached.messages);
      this.roomSignals.set('isLoading', false);
      return;
    }

    // Fetch from server
    const result = await this.executeCommand('data/list', {
      collection: 'chat_messages',
      filter: { roomId },
      limit: 50,
      orderBy: [{ field: 'timestamp', direction: 'desc' }]
    });

    if (result.success) {
      ChatMessageCache.set(roomId, result.items, result.totalCount);
      this.roomSignals.set('messages', result.items);
    }

    this.roomSignals.set('isLoading', false);
  }

  // Key insight: DIFF the messages, don't rebuild
  private diffUpdateMessages(
    newMessages: ChatMessageEntity[],
    oldMessages: ChatMessageEntity[]
  ) {
    const container = this.shadowRoot?.querySelector('.messages');
    if (!container) return;

    const oldIds = new Set(oldMessages.map(m => m.id));
    const newIds = new Set(newMessages.map(m => m.id));

    // Remove deleted messages
    for (const msg of oldMessages) {
      if (!newIds.has(msg.id)) {
        const el = container.querySelector(`[data-entity-id="${msg.id}"]`);
        el?.remove();
      }
    }

    // Add new messages (keyed insertion)
    for (const msg of newMessages) {
      if (!oldIds.has(msg.id)) {
        const el = this.renderMessage(msg);
        el.setAttribute('data-entity-id', msg.id);
        // Insert at correct position based on timestamp
        this.insertAtCorrectPosition(container, el, msg);
      }
    }
  }
}
```

### 4. EntityScroller Gets Keyed Updates

```typescript
// EntityScroller.ts - add diff-based update
interface ScrollerAPI<T> {
  // ... existing methods ...

  /**
   * Efficiently update entities using key-based diffing
   * Only adds/removes elements that changed
   */
  diffUpdate(newEntities: T[], getKey?: (entity: T) => string): void;
}

function createEntityScroller<T>(...) {
  // ...

  return {
    // ...

    diffUpdate(newEntities: T[], getKey = (e: T) => (e as any).id) {
      const existingKeys = new Set(
        Array.from(container.querySelectorAll('[data-entity-id]'))
          .map(el => el.getAttribute('data-entity-id'))
      );

      const newKeys = new Set(newEntities.map(getKey));

      // Remove elements not in new set
      container.querySelectorAll('[data-entity-id]').forEach(el => {
        const key = el.getAttribute('data-entity-id');
        if (key && !newKeys.has(key)) {
          el.remove();
          entityManager.remove(key);
        }
      });

      // Add new elements
      const fragment = document.createDocumentFragment();
      for (const entity of newEntities) {
        const key = getKey(entity);
        if (!existingKeys.has(key)) {
          const element = render(entity, { index: 0, total: newEntities.length });
          element.setAttribute('data-entity-id', key);
          fragment.appendChild(element);
          entityManager.set(key, entity);
        }
      }

      container.appendChild(fragment);
    }
  };
}
```

## Migration Strategy

### Phase 1: Create AppState Store (Non-Breaking)
- Create `system/state/AppState.ts`
- Export signals for content navigation
- No existing code changes yet

### Phase 2: Update MainWidget
- Replace `innerHTML` with widget caching
- Connect to AppState for content switching
- Keep existing event emissions for backward compatibility

### Phase 3: Update ChatWidget
- Connect to AppState instead of attribute watching
- Remove `attributeChangedCallback` room handling
- Use `diffUpdate` instead of `refresh()`

### Phase 4: Update Other Widgets
- RoomListWidget, UserListWidget, etc.
- All connect to AppState

### Phase 5: Remove Legacy
- Remove redundant events (ROOM_SELECTED, content:opened, etc.)
- Remove attribute-based room switching
- Remove `refresh()` calls

## Expected Results

| Operation | Before | After |
|-----------|--------|-------|
| Tab switch (same widget type) | ~15s | <50ms |
| Tab switch (different widget) | ~15s | <100ms |
| New message arrives | <100ms | <50ms |
| Room list click | ~15s | <50ms |

## Key Insights

1. **Don't destroy widgets** - cache and hide/show
2. **Don't rebuild DOM** - diff and update only changes
3. **Single source of truth** - AppState signals drive everything
4. **No cascading events** - direct signal subscription, no event chains
5. **Keyed elements** - every entity element has `data-entity-id` for diffing
