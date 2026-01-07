# Reactive Widget Architecture

## Problem Statement

Tab switching takes 10-15 seconds because:
1. Most widgets use imperative `innerHTML = ''` on every state change
2. No component recycling - DOM elements destroyed and recreated
3. State changes don't drive rendering - manual imperative calls everywhere
4. EntityScroller.refresh() clears ALL DOM on every room switch

## Current Widget Inventory

### 35 Total Widgets

**Using OLD BaseWidget (imperative pattern):**
| Widget | Problem |
|--------|---------|
| MainWidget | Caches widgets but still slow |
| ChatWidget | 15s tab switch, scroller.refresh() clears DOM |
| RoomListWidget | EntityScroller clears DOM |
| UserListWidget | EntityScroller clears DOM |
| ContentTabsWidget | Manual innerHTML updates |
| HeaderControlsWidget | Manual innerHTML |
| SettingsWidget | Manual innerHTML |
| SettingsNavWidget | Manual innerHTML |
| SettingsAssistantWidget | Manual innerHTML |
| UserProfileWidget | Manual innerHTML |
| ThemeWidget | Manual innerHTML |
| HelpWidget | Manual innerHTML |
| LogsNavWidget | Manual innerHTML |
| ContinuumWidget | Container, minimal updates |
| SidebarWidget | Container |
| RightPanelWidget | Container |
| DiagnosticsWidget | Manual innerHTML |
| LogViewerWidget | Manual innerHTML |
| PersonaBrainWidget | Manual innerHTML |
| CognitionHistogramWidget | Canvas rendering |
| ContinuumEmoterWidget | Canvas/SVG |
| ContinuumMetricsWidget | Manual innerHTML |
| DrawingCanvasWidget | Canvas |
| PositronCursorWidget | Position only |
| PanelLayoutWidget | Container |

**Using NEW ReactiveWidget (Lit-based):**
| Widget | Status |
|--------|--------|
| WebViewWidget | Working, template diffing |

## Target Architecture

### 1. State Hierarchy (React-like)

```
AppState (global)
  ├── theme, currentUser, online
  │
  └── PageState (per-page)
        ├── contentType, entityId, resolved
        │
        └── WidgetState (per-widget)
              ├── local state signals
              │
              └── ControlState (per-control)
                    └── input values, focus, etc.
```

### 2. Hooks API (React-like)

Every widget gets these hooks via ReactiveWidget:

```typescript
class MyWidget extends ReactiveWidget {
  // Reactive state - changes trigger re-render
  @state() roomId: string | null = null;
  @state() messages: Message[] = [];
  @state() loading = false;

  // Effects run when dependencies change
  protected onFirstRender() {
    // Effect: load messages when roomId changes
    this.createEffect(() => {
      if (this.roomId) {
        this.loadMessages(this.roomId);
      }
    });
  }

  // Declarative rendering - Lit diffs and updates minimally
  protected renderContent() {
    return html`
      <div class="messages">
        ${this.messages.map(m => html`
          <message-row .message=${m}></message-row>
        `)}
      </div>
    `;
  }

  // To switch rooms - just change state
  switchRoom(roomId: string) {
    this.roomId = roomId;  // Effect runs automatically
  }
}
```

### 3. Component Recycling (EntityScroller)

Current (BAD):
```typescript
async load() {
  entityManager.clear();
  container.innerHTML = '';  // DESTROY ALL DOM
  // rebuild everything from scratch
}
```

Target (GOOD):
```typescript
async load() {
  const newItems = await fetchData();

  // Diff existing vs new
  const existingIds = new Set(this.items.map(i => i.id));
  const newIds = new Set(newItems.map(i => i.id));

  // Remove items no longer present
  for (const id of existingIds) {
    if (!newIds.has(id)) {
      this.recycleElement(id);  // Hide, don't destroy
    }
  }

  // Add or update items
  for (const item of newItems) {
    if (existingIds.has(item.id)) {
      this.updateElement(item.id, item);  // Update in place
    } else {
      this.addElement(item);  // Create or recycle from pool
    }
  }
}
```

### 4. Element Pool (Component Recycling)

```typescript
class ElementPool<T extends HTMLElement> {
  private pool: T[] = [];
  private active: Map<string, T> = new Map();

  acquire(id: string, factory: () => T): T {
    let element = this.pool.pop() ?? factory();
    element.style.display = '';
    this.active.set(id, element);
    return element;
  }

  release(id: string): void {
    const element = this.active.get(id);
    if (element) {
      element.style.display = 'none';
      this.active.delete(id);
      this.pool.push(element);  // Recycle, don't destroy
    }
  }
}
```

## Migration Plan

Each widget goes through TWO stages:
1. **Stage A**: Convert to ReactiveWidget (get within paradigm)
2. **Stage B**: Decompose into sub-components (modularize)

### Phase 1: ChatWidget (Highest Impact)

**Stage A - Convert to ReactiveWidget:**
- Replace BaseWidget inheritance
- Add @state() for roomId, messages, loading
- Replace switchToRoom imperative chain with effect
- Use repeat() with keys for message list

**Stage B - Decompose into components:**
```
ChatWidget
├── chat-header
├── ai-status-bar
├── message-list
│   └── message-row (recycled via repeat())
└── message-input
```

### Phase 2: Navigation Widgets

**ContentTabsWidget:**
- Stage A: Convert, @state() for tabs/activeTab
- Stage B: tab-bar + tab components

**RoomListWidget:**
- Stage A: Convert, fix EntityScroller
- Stage B: room-item components

**MainWidget:**
- Stage A: Already caches, add proper state flow
- Stage B: Minimal - it's mostly a container

### Phase 3: List Widgets

**UserListWidget, LogsNavWidget, SettingsNavWidget:**
- Stage A: Convert to ReactiveWidget
- Stage B: List item components with local hover/select state

### Phase 4: Content Widgets

**SettingsWidget:**
- Stage A: Convert
- Stage B: settings-section, setting-control components

**HelpWidget, DiagnosticsWidget, LogViewerWidget, PersonaBrainWidget, UserProfileWidget:**
- Stage A: Convert each
- Stage B: Section components per widget

### Phase 5: Status & Container Widgets

**HeaderControlsWidget, ContinuumMetricsWidget, CognitionHistogramWidget:**
- Stage A: Convert
- Stage B: Individual control/metric components

**ContinuumWidget, SidebarWidget, RightPanelWidget:**
- Mostly containers, minimal decomposition needed

## Migration Template

For each widget:

```typescript
// BEFORE: BaseWidget (imperative)
export class MyWidget extends BaseWidget {
  private data: Item[] = [];

  async loadData() {
    this.data = await fetch();
    this.render();  // Manual re-render
  }

  render() {
    this.shadowRoot.innerHTML = `...`;  // DESTROY + REBUILD
  }
}

// AFTER: ReactiveWidget (declarative)
export class MyWidget extends ReactiveWidget {
  @state() data: Item[] = [];

  protected onFirstRender() {
    this.loadData();
  }

  async loadData() {
    this.data = await fetch();  // Lit auto-diffs
  }

  protected renderContent() {
    return html`
      ${repeat(this.data, d => d.id, d => html`
        <item-row .item=${d}></item-row>
      `)}
    `;
  }
}
```

## Component Modularization

After migrating to ReactiveWidget, break each widget into focused sub-components.

### Example: ChatWidget Decomposition

**Before (monolithic):**
```
ChatWidget (1500 lines, redraws everything)
```

**After (modular):**
```
ChatWidget (orchestrator only)
├── ChatHeader         ← redraws on: roomName, memberCount
│   └── MemberChip     ← redraws on: member.status
├── AIStatusBar        ← redraws on: aiStatus changes
├── MessageList        ← container, no redraw
│   └── MessageRow     ← redraws on: message.content, reactions
│       ├── MessageBubble
│       ├── MessageMedia
│       └── ReactionBar
└── MessageInput       ← redraws on: inputValue, attachments
    └── AttachmentPreview
```

Each component:
- Has its own `@state()` props
- Only redraws when ITS data changes
- Receives data via properties, not global state

### Component Design Rules

1. **Single Responsibility**: One component = one concern
2. **Props Down, Events Up**: Parent passes data, child emits events
3. **Minimal State**: Only local UI state, not domain data
4. **No DOM Queries**: Use refs or bindings, not querySelector

### Widget Decomposition Plan

| Widget | Sub-Components |
|--------|----------------|
| ChatWidget | ChatHeader, MessageList, MessageRow, MessageInput, AIStatusBar |
| RoomListWidget | RoomListHeader, RoomItem, RoomBadge |
| UserListWidget | UserListHeader, UserItem, StatusDot |
| SettingsWidget | SettingsNav, SettingsSection, SettingControl |
| DiagnosticsWidget | DiagSection, MetricCard, LogStream |
| PersonaBrainWidget | BrainHeader, CognitionGraph, MemoryList |
| ContentTabsWidget | TabBar, Tab, TabCloseButton |
| HeaderControlsWidget | VersionBadge, ThemeToggle, SettingsButton |

### Sub-Component Template

```typescript
// MessageRow - renders ONE message, redraws only when THIS message changes
@customElement('message-row')
export class MessageRow extends LitElement {
  // Props from parent
  @property({ type: Object }) message!: ChatMessage;
  @property({ type: Boolean }) isCurrentUser = false;

  // Local state only
  @state() private showActions = false;

  static styles = css`...`;

  render() {
    return html`
      <div class="message ${this.isCurrentUser ? 'own' : ''}"
           @mouseenter=${() => this.showActions = true}
           @mouseleave=${() => this.showActions = false}>
        <message-bubble .content=${this.message.content}></message-bubble>
        ${this.showActions ? html`<message-actions></message-actions>` : ''}
      </div>
    `;
  }
}
```

### Parent-Child Communication

```typescript
// Parent: MessageList
render() {
  return html`
    ${repeat(this.messages, m => m.id, m => html`
      <message-row
        .message=${m}
        .isCurrentUser=${m.senderId === this.userId}
        @delete=${(e) => this.handleDelete(e.detail.id)}
        @react=${(e) => this.handleReact(e.detail)}
      ></message-row>
    `)}
  `;
}

// Child: MessageRow
handleDeleteClick() {
  this.dispatchEvent(new CustomEvent('delete', {
    detail: { id: this.message.id },
    bubbles: true
  }));
}
```

## Key Patterns

### 1. State Changes Drive Behavior

```typescript
// BAD (imperative)
switchRoom(roomId) {
  this.currentRoomId = roomId;
  await this.loadRoomData(roomId);
  await this.scroller.refresh();
  this.updateHeader();
  // ... more imperative calls
}

// GOOD (declarative)
@state() roomId: string | null = null;

onFirstRender() {
  this.createEffect(() => {
    if (this.roomId) this.loadRoom(this.roomId);
  });
}

switchRoom(roomId) {
  this.roomId = roomId;  // Effect handles the rest
}
```

### 2. Keyed Lists for Efficient Updates

```typescript
// BAD (no keys, full rebuild)
render() {
  return html`${items.map(i => html`<div>${i.name}</div>`)}`;
}

// GOOD (keyed, minimal updates)
render() {
  return html`
    ${repeat(items, i => i.id, i => html`
      <div>${i.name}</div>
    `)}
  `;
}
```

### 3. Component Pool for Heavy Elements

```typescript
// Message rows are expensive - recycle them
const messagePool = new ElementPool<MessageRow>();

// When message leaves viewport
messagePool.release(message.id);

// When new message enters
const row = messagePool.acquire(message.id, () => new MessageRow());
row.message = message;
```

## Testing Each Migration

After each widget migration:

```bash
# 1. Build
npm run build:ts

# 2. Deploy
npm start

# 3. Test tab switching speed
time ./jtag screenshot  # Should be <1s

# 4. Verify functionality
./jtag collaboration/chat/send --room="general" --message="Testing"
./jtag collaboration/chat/export --room="general" --limit=5
```

## Success Metrics

| Metric | Before | Target |
|--------|--------|--------|
| Tab switch | 15s | <100ms |
| Room switch | 10s | <50ms |
| Initial load | 5s | <2s |
| Memory on refresh | +50MB | +0MB |

## Files to Modify

### Core Infrastructure
- `widgets/shared/ReactiveWidget.ts` - Add createEffect() method
- `widgets/shared/EntityScroller.ts` - Remove innerHTML clearing
- `widgets/shared/ElementPool.ts` - NEW: Component recycling

### Per Widget
- Replace `extends BaseWidget` with `extends ReactiveWidget`
- Replace `innerHTML =` with `renderContent()` returning html``
- Replace manual state with `@state()` decorators
- Replace imperative calls with effects
