/**
 * Simple Reactive State Management System
 *
 * React-like state management where changing state automatically triggers events
 * for any subscribers watching that property.
 */

export type StateChangeHandler<T> = (newValue: T, oldValue: T) => void;

export class ReactiveState<T extends Record<string, any>> {
  private state: T;
  private watchers = new Map<keyof T, Set<StateChangeHandler<any>>>();

  // Async notification batching - prevents synchronous cascades
  private _pendingNotifications = new Map<keyof T, { newValue: T[keyof T], oldValue: T[keyof T] }>();
  private _notifyScheduled = false;

  constructor(initialState: T) {
    // Create proxy to intercept property changes
    this.state = new Proxy(initialState, {
      set: (target, property, value) => {
        const oldValue = target[property as keyof T];

        // Only trigger if value actually changed
        if (oldValue !== value) {
          target[property as keyof T] = value;
          this.notifyWatchers(property as keyof T, value, oldValue);
        }

        return true;
      }
    });
  }

  /**
   * Get current state (reactive)
   */
  get current(): T {
    return this.state;
  }

  /**
   * Watch for changes to a specific property
   */
  watch<K extends keyof T>(property: K, handler: StateChangeHandler<T[K]>): () => void {
    if (!this.watchers.has(property)) {
      this.watchers.set(property, new Set());
    }

    this.watchers.get(property)!.add(handler);

    // Return unsubscribe function
    return () => {
      this.watchers.get(property)?.delete(handler);
    };
  }

  /**
   * Update state (triggers watchers)
   */
  update(changes: Partial<T>): void {
    Object.assign(this.state, changes);
  }

  /**
   * Queue notification for batched async delivery.
   * Multiple changes within same microtask are coalesced.
   * Prevents synchronous cascades that can cause infinite loops.
   */
  private notifyWatchers<K extends keyof T>(property: K, newValue: T[K], oldValue: T[K]): void {
    // Queue notification (overwrites previous if same property changed multiple times)
    this._pendingNotifications.set(property, { newValue, oldValue } as { newValue: T[keyof T], oldValue: T[keyof T] });

    // Schedule single batch flush
    if (!this._notifyScheduled) {
      this._notifyScheduled = true;
      queueMicrotask(() => {
        this._notifyScheduled = false;
        this.flushNotifications();
      });
    }
  }

  /**
   * Flush all pending notifications in a single batch.
   */
  private flushNotifications(): void {
    const batch = new Map(this._pendingNotifications);
    this._pendingNotifications.clear();

    for (const [property, { newValue, oldValue }] of batch) {
      const handlers = this.watchers.get(property);
      if (handlers) {
        handlers.forEach(handler => handler(newValue, oldValue));
      }
    }
  }
}