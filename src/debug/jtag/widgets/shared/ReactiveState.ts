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

  private notifyWatchers<K extends keyof T>(property: K, newValue: T[K], oldValue: T[K]): void {
    const handlers = this.watchers.get(property);
    if (handlers) {
      handlers.forEach(handler => handler(newValue, oldValue));
    }
  }
}