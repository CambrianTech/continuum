/**
 * ReactiveStore - Generic reactive store primitive
 *
 * Foundation for the Positronic Reactive State Architecture.
 * Provides a simple subscribe/notify pattern that works across all contexts.
 *
 * Features:
 * - Type-safe with full generics support
 * - Immediate notification on subscribe (current state)
 * - Functional updates (set with callback)
 * - Partial updates (update with Partial<T>)
 * - Error isolation (one bad listener doesn't break others)
 *
 * Usage:
 *   const store = new ReactiveStore<MyState>({ count: 0 });
 *   const unsub = store.subscribe(state => console.log(state.count));
 *   store.update({ count: 1 });  // Partial update
 *   store.set(prev => ({ ...prev, count: prev.count + 1 }));  // Functional update
 *   unsub();  // Cleanup
 */

/**
 * Listener callback type
 */
export type StoreListener<T> = (state: T) => void;

/**
 * Generic reactive store with subscribe/notify pattern
 */
export class ReactiveStore<T> {
  private _state: T;
  private _listeners = new Set<StoreListener<T>>();
  private _notifying = false;

  constructor(initialState: T) {
    this._state = initialState;
  }

  /**
   * Get current state (read-only snapshot)
   */
  get(): T {
    return this._state;
  }

  /**
   * Set new state (full replacement or functional update)
   *
   * @param newState - New state object or function that receives previous state
   */
  set(newState: T | ((prev: T) => T)): void {
    const prevState = this._state;

    if (typeof newState === 'function') {
      this._state = (newState as (prev: T) => T)(prevState);
    } else {
      this._state = newState;
    }

    // Only notify if state actually changed
    if (this._state !== prevState) {
      this.notify();
    }
  }

  /**
   * Update state with partial object (shallow merge)
   *
   * @param partial - Partial state to merge with current
   */
  update(partial: Partial<T>): void {
    this._state = { ...this._state, ...partial };
    this.notify();
  }

  /**
   * Subscribe to state changes
   *
   * Immediately calls callback with current state, then calls on every change.
   *
   * @param callback - Function to call with state
   * @returns Unsubscribe function - call this in cleanup/disconnectedCallback
   */
  subscribe(callback: StoreListener<T>): () => void {
    this._listeners.add(callback);

    // Immediately notify with current state
    try {
      callback(this._state);
    } catch (error) {
      console.error('ReactiveStore: Error in subscriber callback:', error);
    }

    // Return unsubscribe function
    return () => {
      this._listeners.delete(callback);
    };
  }

  /**
   * Get subscriber count (for debugging)
   */
  get subscriberCount(): number {
    return this._listeners.size;
  }

  /**
   * Check if store has any subscribers
   */
  get hasSubscribers(): boolean {
    return this._listeners.size > 0;
  }

  /**
   * Notify all listeners of state change
   * Protected against recursive notifications
   */
  private notify(): void {
    // Prevent recursive notifications
    if (this._notifying) {
      return;
    }

    this._notifying = true;

    try {
      for (const listener of this._listeners) {
        try {
          listener(this._state);
        } catch (error) {
          console.error('ReactiveStore: Error in subscriber callback:', error);
          // Continue notifying other listeners
        }
      }
    } finally {
      this._notifying = false;
    }
  }

  /**
   * Reset store to a new state without notifying subscribers
   * Useful for initialization or testing
   */
  reset(newState: T): void {
    this._state = newState;
  }

  /**
   * Clear all subscribers (for cleanup/testing)
   */
  clearSubscribers(): void {
    this._listeners.clear();
  }
}

/**
 * Create a derived store that transforms another store's state
 *
 * Usage:
 *   const countStore = new ReactiveStore({ count: 0 });
 *   const doubledStore = deriveStore(countStore, state => state.count * 2);
 */
export function deriveStore<T, U>(
  source: ReactiveStore<T>,
  transform: (state: T) => U
): ReactiveStore<U> {
  const derived = new ReactiveStore<U>(transform(source.get()));

  source.subscribe(state => {
    derived.reset(transform(state));
    // Manually trigger notification since reset doesn't notify
    (derived as any).notify();
  });

  return derived;
}
