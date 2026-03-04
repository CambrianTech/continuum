# Scoped State Architecture

## Problem

The current widget system has fragmented state management:
- Each widget independently queries database for UserState
- Multiple attribute names for entity ID (`data-entity-id`, `entity-id`, `room`)
- No shared page-level state
- Timing issues between URL parsing, state updates, and widget creation
- Widgets read URL directly instead of from state

## Solution: Cascading Scoped State

State flows down through scopes, each level inheriting from above with override capability:

```
┌─────────────────────────────────────────────────────────┐
│  SITE STATE (global)                                    │
│  - currentUser, theme, session                          │
│  - Source: UserStateEntity                              │
├─────────────────────────────────────────────────────────┤
│  PAGE STATE (current route)                    ↓ fills  │
│  - contentType, entityId, resolvedEntity                │
│  - Source: Router (MainWidget) parses URL               │
├─────────────────────────────────────────────────────────┤
│  WIDGET STATE (component-specific)             ↓ fills  │
│  - messages, scrollPosition, selectedItem               │
│  - Can OVERRIDE page state (e.g., pinned room)          │
├─────────────────────────────────────────────────────────┤
│  CONTROL STATE (UI element)                    ↓ fills  │
│  - inputValue, dropdownOpen, focused                    │
│  - Can OVERRIDE widget state                            │
└─────────────────────────────────────────────────────────┘
```

### Override Capability

Each level can override values from above:

```typescript
// Widget reads with fallback chain (most specific wins)
get room(): string {
  return this.localOverride     // widget-level override (pinned)
      ?? this.pageState.room    // page-level (from URL)
      ?? this.siteState.defaultRoom  // site default
}
```

Example: RightPanel chat is pinned to "help" room regardless of page state.

## Implementation

### PageStateService

Single source of truth for current page/route state:

```typescript
// system/state/PageStateService.ts

interface PageState {
  contentType: string;
  entityId?: string;
  resolved?: {
    id: UUID;
    uniqueId: string;
    displayName: string;
  };
}

class PageStateService {
  private state: PageState | null = null;
  private listeners: Set<(state: PageState) => void> = new Set();

  // Called by router (MainWidget) after URL parsing
  setContent(contentType: string, entityId?: string, resolved?: ResolvedEntity): void {
    this.state = { contentType, entityId, resolved };
    this.notify();
  }

  // Read current state
  getContent(): PageState | null {
    return this.state;
  }

  // Widgets subscribe for updates
  subscribe(callback: (state: PageState) => void): () => void {
    this.listeners.add(callback);
    // Immediately call with current state if exists
    if (this.state) callback(this.state);
    return () => this.listeners.delete(callback);
  }

  private notify(): void {
    for (const listener of this.listeners) {
      listener(this.state!);
    }
  }
}

export const pageState = new PageStateService();
```

### Router Flow (MainWidget)

URL is parsed ONCE by router, then state is updated:

```typescript
// MainWidget.setupUrlRouting()

// 1. Parse URL (only place that reads URL)
const { type, entityId } = parseContentPath(window.location.pathname);

// 2. Resolve entity if needed
const resolved = await RoutingService.resolveRoom(entityId);

// 3. Update page state (BEFORE creating widget)
pageState.setContent(type, entityId, resolved);

// 4. Create widget (reads from pageState, not URL)
this.switchContentView(type, entityId);
```

### Widget Consumption

Widgets read from pageState, not attributes or URL:

```typescript
// ChatWidget

protected async onWidgetInitialize(): Promise<void> {
  // Check for local override first (pinned widget)
  const pinnedRoom = this.getAttribute('room');
  if (pinnedRoom) {
    await this.switchToRoom(pinnedRoom);
    return;
  }

  // Otherwise, read from page state
  const state = pageState.getContent();
  if (state?.contentType === 'chat' && state.entityId) {
    await this.switchToRoom(state.entityId);
  }

  // Subscribe to future changes
  this.pageStateUnsubscribe = pageState.subscribe((state) => {
    if (state.contentType === 'chat' && state.entityId) {
      this.switchToRoom(state.entityId);
    }
  });
}
```

### BaseWidget Integration

Add pageState access to BaseWidget:

```typescript
// BaseWidget.ts

import { pageState, PageState } from '@system/state/PageStateService';

export abstract class BaseWidget extends HTMLElement {
  private pageStateUnsubscribe?: () => void;

  // Easy access to current page state
  protected get pageState(): PageState | null {
    return pageState.getContent();
  }

  // Subscribe to page state changes
  protected subscribeToPageState(callback: (state: PageState) => void): void {
    this.pageStateUnsubscribe = pageState.subscribe(callback);
  }

  disconnectedCallback(): void {
    this.pageStateUnsubscribe?.();
  }
}
```

## Benefits

1. **Single source of truth** - URL parsed once, state shared everywhere
2. **No timing issues** - State set before widget created
3. **Clean override pattern** - Widget can override page, page can override site
4. **No attribute mess** - Widgets read from service, not DOM attributes
5. **Testable** - Can set pageState directly in tests
6. **Reactive** - Widgets subscribe and update automatically

## Migration Path

1. Create `PageStateService`
2. Update MainWidget to use it (set state before creating widgets)
3. Update BaseWidget with pageState accessor
4. Migrate widgets one by one:
   - Remove `getAttribute('data-entity-id')` calls
   - Use `this.pageState.entityId` instead
   - Keep `room` attribute only for explicit overrides (pinned widgets)
5. Remove legacy attribute handling from BaseWidget

## State Scope Summary

| Scope | Source | Lifetime | Override By |
|-------|--------|----------|-------------|
| Site | UserStateEntity | Session | - |
| Page | Router/URL | Route change | - |
| Widget | Component | Mount/unmount | Local state |
| Control | UI element | Interaction | Local state |

Each level inherits from above and can override specific values.
