# Data Event System - Unified CRUD Subscriptions

## Overview

The unified CRUD event subscription system provides a clean, powerful way for widgets to subscribe to database changes with real-time UI updates. Instead of managing multiple individual subscriptions, widgets can now use single subscription patterns to handle all Create, Read, Update, Delete operations.

## Core Concepts

### Event Naming Pattern
All CRUD events follow the pattern: `data:{Collection}:{Action}`

```
data:User:created     // User created
data:User:updated     // User updated
data:User:deleted     // User deleted
data:Room:created     // Room created
data:ChatMessage:updated  // Chat message updated
```

### Subscription Strategies

#### 1. **All CRUD Operations** (Recommended)
```typescript
import { createEntityCrudHandler } from '../../../commands/data/shared/DataEventUtils';

// Subscribe to ALL User operations (create, update, delete)
const unsubscribe = createEntityCrudHandler<UserEntity>('User', {
  add: (user) => scroller.add(user),
  update: (id, user) => scroller.update(id, user),
  remove: (id) => scroller.remove(id)
});
```

#### 2. **Selective CRUD Operations**
```typescript
import { subscribeToSelectedCrudEvents } from '../../../commands/data/shared/DataEventUtils';

// Subscribe only to updates and deletes (skip creates)
const unsubscribe = subscribeToSelectedCrudEvents<UserEntity>(
  'User',
  ['updated', 'deleted'],
  (user, action) => {
    if (action === 'updated') scroller.update(user.id, user);
    if (action === 'deleted') scroller.remove(user.id);
  }
);
```

#### 3. **Manual Individual Subscriptions**
```typescript
import { subscribeToSpecificCrudEvent } from '../../../commands/data/shared/DataEventUtils';

// Subscribe to specific operations manually
const unsubscribe = subscribeToSpecificCrudEvent<UserEntity>(
  'User',
  'updated',
  (user) => scroller.update(user.id, user)
);
```

## Widget Integration Examples

### UserListWidget (Complete Example)
```typescript
export class UserListWidget extends ChatWidgetBase {
  private unsubscribeUserEvents?: () => void;

  private async setupUserEventSubscriptions(): Promise<void> {
    // Single subscription for ALL User CRUD operations
    this.unsubscribeUserEvents = createEntityCrudHandler<UserEntity>(
      UserEntity.collection,
      {
        add: (user: UserEntity) => {
          this.userScroller?.add(user);
          this.updateUserCount();
        },
        update: (id: string, user: UserEntity) => {
          this.userScroller?.update(id, user);
          this.updateUserCount();
        },
        remove: (id: string) => {
          this.userScroller?.remove(id);
          this.updateUserCount();
        }
      }
    );
  }

  protected async onWidgetCleanup(): Promise<void> {
    this.unsubscribeUserEvents?.();
    this.unsubscribeUserEvents = undefined;
  }
}
```

### ChatWidget (Selective Example)
```typescript
export class ChatWidget extends ChatWidgetBase {
  private unsubscribeChatEvents?: () => void;

  private async setupMessageSubscriptions(): Promise<void> {
    // Only subscribe to message creation and updates (skip deletes)
    this.unsubscribeChatEvents = subscribeToSelectedCrudEvents<ChatMessageEntity>(
      'ChatMessage',
      ['created', 'updated'],
      (message, action) => {
        if (action === 'created') this.onMessageReceived(message);
        if (action === 'updated') this.onMessageUpdated(message);
      }
    );
  }
}
```

## API Reference

### `createEntityCrudHandler<T>(collection, scroller)`
Creates a unified subscription for all CRUD operations that automatically calls the appropriate EntityScroller methods.

**Parameters:**
- `collection: string` - Entity collection name (e.g., 'User', 'Room')
- `scroller: EntityScrollerInterface<T>` - Object with add/update/remove methods

**Returns:** `() => void` - Unsubscribe function

**Best for:** List widgets that need to stay in sync with database changes

---

### `subscribeToAllCrudEvents<T>(collection, handler)`
Creates subscriptions for all CRUD operations with a single handler function.

**Parameters:**
- `collection: string` - Entity collection name
- `handler: (entity: T, action: CrudAction) => void` - Unified handler function

**Returns:** `() => void` - Unsubscribe function

**Best for:** Custom logic that needs to handle all operations differently

---

### `subscribeToSelectedCrudEvents<T>(collection, actions, handler)`
Creates subscriptions for specific CRUD operations only.

**Parameters:**
- `collection: string` - Entity collection name
- `actions: CrudAction[]` - Array of operations to subscribe to
- `handler: (entity: T, action: CrudAction) => void` - Handler function

**Returns:** `() => void` - Unsubscribe function

**Best for:** Widgets that only care about specific operations (e.g., only updates)

---

### `subscribeToSpecificCrudEvent<T>(collection, action, handler)`
Creates a subscription for a single CRUD operation.

**Parameters:**
- `collection: string` - Entity collection name
- `action: CrudAction` - Specific operation ('created', 'updated', 'deleted')
- `handler: (entity: T) => void` - Handler function

**Returns:** `() => void` - Unsubscribe function

**Best for:** One-off subscriptions or legacy code migration

## Migration Guide

### Before (Old Approach)
```typescript
// Multiple individual subscriptions
Events.subscribe('data:User:created', handler1);
Events.subscribe('data:User:updated', handler2);
// TODO: Add delete events (often forgotten!)
```

### After (New Approach)
```typescript
// Single unified subscription
const unsubscribe = createEntityCrudHandler('User', scroller);

// Or selective approach
const unsubscribe = subscribeToSelectedCrudEvents(
  'User',
  ['created', 'updated', 'deleted'],
  handler
);
```

## Benefits

1. **Simplified Code**: One subscription instead of multiple
2. **Complete CRUD Coverage**: DELETE events are no longer forgotten
3. **Type Safety**: Full TypeScript support with entity types
4. **Automatic Cleanup**: Single unsubscribe function
5. **EntityScroller Integration**: Direct integration with scrolling lists
6. **Consistent Patterns**: Same approach across all widgets
7. **Extensible**: Easy to add new subscription patterns

## Future Enhancements

### Regex-Like Patterns (Planned)
```typescript
// Subscribe to multiple collections
Events.subscribe('data:User|Room:*', handler);

// Subscribe to specific operations across all entities
Events.subscribe('data:*:updated', handler);

// Complex patterns
Events.subscribe('data:ChatMessage:updated|deleted', handler);
```

### Cross-Entity Subscriptions
```typescript
// Subscribe to related entity changes
subscribeToRelatedEntities(['User', 'Room'], handler);
```

## Testing

The unified CRUD system includes comprehensive test coverage:

- **Unit tests**: Individual subscription functions
- **Integration tests**: Real-time widget updates
- **E2E tests**: Full CRUD cycle with UI verification

Run tests:
```bash
npm run test:crud-subscriptions
```

## Performance Considerations

- **Memory**: Each subscription uses minimal memory overhead
- **Event Matching**: Efficient pattern matching for large numbers of subscriptions
- **Cleanup**: Automatic cleanup prevents memory leaks
- **Batching**: Future enhancement will support event batching for high-frequency updates

---

*This system provides the foundation for comprehensive real-time UI updates across the entire application, ensuring that every database change is immediately reflected in the user interface.*