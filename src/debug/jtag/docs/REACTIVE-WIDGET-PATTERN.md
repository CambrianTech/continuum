# Reactive Widget Pattern Design

**Status**: Design Phase
**Authors**: AI Team (Claude Assistant, DeepSeek, Groq Lightning, Together Assistant, Teacher AI)
**Branch**: feature/widget-state-and-routing

## Executive Summary

This document defines the architecture for the Reactive Widget Pattern - a pub/sub system enabling real-time communication between widgets without polling. The pattern has been proven with a ChatWidget → UserProfileWidget implementation showing 100ms latency.

## Problem Statement

Currently, widgets that need data from other widgets must either:
1. **Poll** - Inefficient, creates unnecessary load
2. **Shared global state** - Tight coupling, hard to test
3. **Parent-child props** - Limited to direct relationships

We need a pattern that enables:
- Real-time updates between sibling widgets
- Automatic cleanup (no memory leaks)
- Decoupled, testable code
- Built-in performance monitoring

## Architecture Decision: Base Class

**Decision**: Create `ReactiveWidget` base class (not mixin)

**Rationale**:
- Enforces cleanup patterns by default (prevents memory leaks)
- Built-in telemetry for monitoring
- Single source of truth for subscription lifecycle
- Cycle detection prevents event storms

## ReactiveWidget Base Class

```typescript
// widgets/shared/ReactiveWidget.ts

export class ReactiveWidget extends HTMLElement {
  private subscriptions = new Map<ReactiveWidget, Map<string, Set<Function>>>();
  private emitTimestamps = new Map<string, number>(); // For telemetry

  /**
   * Emit an event to all subscribers
   */
  protected emit(event: string, data: any): void {
    // Telemetry: track emit latency
    const start = performance.now();

    // Get all subscribers for this widget+event
    PositronWidgetState.emitReactiveEvent(this, event, data);

    // Log latency
    console.log(`[ReactiveWidget] ${this.constructor.name}.emit('${event}') - ${performance.now() - start}ms`);
  }

  /**
   * Subscribe to another widget's events
   * Returns unsubscribe function
   */
  protected subscribe(
    widget: ReactiveWidget,
    event: string,
    handler: (data: any) => void
  ): () => void {
    // Track subscription for auto-cleanup
    if (!this.subscriptions.has(widget)) {
      this.subscriptions.set(widget, new Map());
    }
    const widgetSubs = this.subscriptions.get(widget)!;
    if (!widgetSubs.has(event)) {
      widgetSubs.set(event, new Set());
    }
    widgetSubs.get(event)!.add(handler);

    // Register with central state manager
    const unsubscribe = PositronWidgetState.subscribeToWidget(widget, event, handler);

    // Return cleanup function
    return () => {
      widgetSubs.get(event)?.delete(handler);
      unsubscribe();
    };
  }

  /**
   * Auto-cleanup all subscriptions when widget disconnects
   */
  disconnectedCallback(): void {
    // Clean up ALL subscriptions this widget made
    for (const [widget, events] of this.subscriptions) {
      for (const [event, handlers] of events) {
        for (const handler of handlers) {
          PositronWidgetState.unsubscribeFromWidget(widget, event, handler);
        }
      }
    }
    this.subscriptions.clear();
    console.log(`[ReactiveWidget] ${this.constructor.name} cleaned up all subscriptions`);
  }
}
```

## Widget Pairs Priority

### Tier 1 - High Impact (This PR)

| Subscriber | Publisher | Events | Use Case |
|------------|-----------|--------|----------|
| ChatWidget | UserProfileWidget | `status:changed` | Real-time member status |
| ChatWidget | RoomWidget | `metadata:changed`, `members:changed` | Room info updates |
| NotificationWidget | ChatWidget | `message:received`, `mention:created` | Unread counts |

### Tier 2 - Next Phase

| Subscriber | Publisher | Events | Use Case |
|------------|-----------|--------|----------|
| MemberListWidget | RoomWidget | `member:joined`, `member:left` | Live member list |
| ChatWidget | TypingIndicatorWidget | `typing:started`, `typing:stopped` | Typing indicators |

### Pattern Exclusions

Do NOT use this pattern for:
- **Parent → Child data** - Use standard Web Components properties
- **Global application state** - Use shared services/stores
- **One-time initialization** - Use attributes

## PR Structure

### PR 1: Core Infrastructure
- [ ] `ReactiveWidget` base class
- [ ] Extend `PositronWidgetState` with widget-to-widget pub/sub
- [ ] Telemetry integration (latency, frequency tracking)
- [ ] Unit tests for base class
- [ ] Pattern Guide documentation

### PR 2: Migrate Existing Implementation
- [ ] Refactor `ChatWidget` to extend `ReactiveWidget`
- [ ] Migrate current Positron subscription to use new API
- [ ] Validate no performance regression (100ms baseline)
- [ ] Integration tests

### PR 3: Tier 1 Expansion
- [ ] Add `ChatWidget ↔ RoomWidget` subscription
- [ ] Add `NotificationWidget ↔ ChatWidget` subscription
- [ ] Update documentation with new examples

### PR 4: Developer Tooling (Future)
- [ ] DevTools panel for subscription graph visualization
- [ ] Memory leak detection warnings
- [ ] Performance profiling dashboard

## Architectural Safeguards

### 1. Subscription Graph Complexity
**Risk**: Circular dependencies, event storms
**Mitigation**:
- Max subscription depth limit (3 levels)
- Cycle detection in `subscribe()`
- Event batching for cascading updates (16ms frame budget)

### 2. Memory Leaks
**Risk**: Forgotten unsubscribes
**Mitigation**:
- Base class auto-cleanup in `disconnectedCallback()`
- Console warnings for orphaned subscriptions in dev mode
- Unit test assertions for cleanup

### 3. Testing Strategy
**Risk**: Tests coupled to implementation
**Mitigation**:
- Mock `emit()` and `subscribe()` in unit tests
- Integration tests for widget pairs
- Performance regression tests against 100ms baseline

### 4. Backward Compatibility
**Risk**: Breaking existing widgets
**Mitigation**:
- Gradual migration (opt-in per widget)
- Existing PositronWidgetState API remains unchanged
- Feature flag during rollout (if needed)

## Decision Tree

```
Should I use Reactive Widget Pattern?
│
├─ Need real-time updates between widgets?
│   └─ YES → Consider this pattern
│
├─ Is it a parent-child relationship?
│   └─ NO → Use standard props/attributes
│
├─ Is it global app state?
│   └─ NO → Use shared service/store
│
├─ Multiple widgets need the same data?
│   └─ YES → Good candidate
│
└─ Updates triggered by user actions?
    └─ YES → This pattern fits well
```

## Performance Targets

| Metric | Target | Current |
|--------|--------|---------|
| Event propagation latency | < 100ms | 100ms (proven) |
| Memory per subscription | < 1KB | TBD |
| Max subscriptions per widget | 10 | TBD |
| Event batch window | 16ms | TBD |

## API Reference

### ReactiveWidget Methods

```typescript
// Publish event to subscribers
protected emit(event: string, data: any): void

// Subscribe to widget events (returns unsubscribe fn)
protected subscribe(widget: ReactiveWidget, event: string, handler: Function): () => void

// Auto-cleanup (called by browser)
disconnectedCallback(): void
```

### Event Naming Convention

```
domain:action
```

Examples:
- `status:changed`
- `member:joined`
- `typing:started`
- `message:received`

## Migration Guide

### Before (Direct Positron)
```typescript
class ChatWidget extends EntityScrollerWidget {
  private positronUnsubscribe?: () => void;

  async onWidgetInitialize() {
    this.positronUnsubscribe = PositronWidgetState.subscribe((ctx) => {
      if (ctx.widget.widgetType === 'profile') {
        this.handleProfileChange(ctx);
      }
    });
  }

  disconnectedCallback() {
    this.positronUnsubscribe?.();
  }
}
```

### After (ReactiveWidget)
```typescript
class ChatWidget extends ReactiveWidget {
  async onWidgetInitialize() {
    // Type-safe, auto-cleanup
    this.subscribe(userProfileWidget, 'status:changed', (data) => {
      this.updateMemberStatus(data.userId, data.status);
    });
  }
  // No manual cleanup needed!
}
```

## Open Questions

1. **Feature flag?** - Do we need gradual rollout or just ship?
2. **DevTools priority?** - Build subscription graph viz now or later?
3. **Batch window?** - 16ms (frame budget) or configurable?

## References

- Proof of concept: `widgets/chat/chat-widget/ChatWidget.ts` (lines 591-677)
- PositronWidgetState: `widgets/shared/services/state/PositronWidgetState.ts`
- AI team discussion: Chat room "general", 2025-12-27
