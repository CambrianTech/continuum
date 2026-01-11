/**
 * Widget Signals - Reactive state management for widgets
 *
 * Provides React-like memoization without React:
 * - Signals: Observable values that trigger re-renders
 * - Computed: Derived values that cache until dependencies change
 * - Effects: Side effects that auto-cleanup
 *
 * Usage:
 *   const store = createWidgetSignals({ roomId: null, messages: [] });
 *   store.set('roomId', 'general');
 *   effect(() => console.log(store.getSignal('roomId').value));
 */

import { signal, computed, effect, batch } from '@preact/signals-core';
import type {
  Dispose,
  WidgetSignalState,
  WidgetSignalOptions,
  WritableSignal,
  EqualityFn
} from './SignalTypes';

// Re-export primitives for direct use
export { signal, computed, effect, batch };
export type { Signal, ReadonlySignal } from '@preact/signals-core';

/**
 * Create a reactive state store for a widget
 *
 * @param initialState - Initial state object
 * @param options - Configuration options
 * @returns Reactive store with signals for each property
 *
 * @example
 * ```typescript
 * interface ChatState {
 *   roomId: string | null;
 *   messages: Message[];
 *   isLoading: boolean;
 * }
 *
 * const store = createWidgetSignals<ChatState>({
 *   roomId: null,
 *   messages: [],
 *   isLoading: false
 * });
 *
 * // Set single value
 * store.set('roomId', 'general');
 *
 * // Batch multiple updates
 * store.batch({ roomId: 'academy', isLoading: true });
 *
 * // Subscribe to all changes
 * const dispose = store.subscribe(state => console.log(state));
 * ```
 */
export function createWidgetSignals<T extends object>(
  initialState: T,
  options: WidgetSignalOptions = {}
): WidgetSignalState<T> {
  const { debug = false, widgetName = 'Widget' } = options;

  // Create a signal for each property
  type SignalMap = { [K in keyof T]: WritableSignal<T[K]> };
  const signals = {} as SignalMap;

  for (const key in initialState) {
    if (Object.prototype.hasOwnProperty.call(initialState, key)) {
      signals[key] = signal(initialState[key]);
    }
  }

  // Track active effects for cleanup
  const activeEffects: Dispose[] = [];

  // Track subscriptions
  const subscriptions: Dispose[] = [];

  const store: WidgetSignalState<T> = {
    get state(): T {
      const result = {} as T;
      for (const key in signals) {
        result[key] = signals[key].value;
      }
      return result;
    },

    getSignal<K extends keyof T>(key: K): WritableSignal<T[K]> {
      return signals[key];
    },

    set<K extends keyof T>(key: K, value: T[K]): void {
      const prev = signals[key].value;
      if (prev !== value) {
        if (debug) {
          console.log(`🔄 ${widgetName}: ${String(key)} changed`, { prev, next: value });
        }
        signals[key].value = value;
      }
    },

    batch(updates: Partial<T>): void {
      batch(() => {
        for (const key in updates) {
          if (Object.prototype.hasOwnProperty.call(updates, key) && key in signals) {
            const k = key as keyof T;
            const value = updates[k];
            if (value !== undefined) {
              signals[k].value = value;
            }
          }
        }
      });
      if (debug) {
        console.log(`🔄 ${widgetName}: Batch update`, updates);
      }
    },

    subscribe(callback: (state: T) => void): Dispose {
      const dispose = effect(() => {
        // Read all signals to track them as dependencies
        const state = store.state;
        callback(state);
      });
      subscriptions.push(dispose);
      return dispose;
    },

    dispose(): void {
      // Clean up all effects
      activeEffects.forEach(d => d());
      activeEffects.length = 0;

      // Clean up all subscriptions
      subscriptions.forEach(d => d());
      subscriptions.length = 0;

      if (debug) {
        console.log(`🧹 ${widgetName}: Signal store disposed`);
      }
    }
  };

  return store;
}

/**
 * Create a memoized computed value that only recalculates when dependencies change
 *
 * @param compute - Function that computes the value
 * @returns Computed signal that caches the result
 *
 * @example
 * ```typescript
 * const roomId = signal('general');
 * const messages = signal<Message[]>([]);
 *
 * const filteredMessages = memo(() =>
 *   messages.value.filter(m => m.roomId === roomId.value)
 * );
 *
 * // Only recalculates when roomId or messages change
 * console.log(filteredMessages.value);
 * ```
 */
export function memo<T>(compute: () => T) {
  return computed(compute);
}

/**
 * Create a memoized value with custom equality check
 *
 * @param compute - Function that computes the value
 * @param equals - Custom equality function
 */
export function memoWithEquals<T>(
  compute: () => T,
  equals: EqualityFn<T>
): { readonly value: T } {
  let cached: T | undefined;
  let initialized = false;

  const comp = computed(() => {
    const next = compute();
    if (!initialized) {
      initialized = true;
      cached = next;
      return next;
    }
    if (equals(cached as T, next)) {
      return cached as T;
    }
    cached = next;
    return next;
  });

  return comp;
}

/**
 * Create an effect that runs when dependencies change
 * Returns a dispose function for cleanup
 *
 * @param callback - Effect callback (can return cleanup function)
 * @returns Dispose function
 *
 * @example
 * ```typescript
 * const roomId = signal('general');
 *
 * const dispose = watchEffect(() => {
 *   console.log('Room changed to:', roomId.value);
 *   // Optional cleanup
 *   return () => console.log('Cleaning up');
 * });
 *
 * // Later: dispose();
 * ```
 */
export function watchEffect(callback: () => void | Dispose): Dispose {
  let cleanup: Dispose | void;

  const dispose = effect(() => {
    // Run previous cleanup
    if (cleanup) cleanup();
    // Run effect and capture new cleanup
    cleanup = callback();
  });

  // Return combined dispose
  return () => {
    if (cleanup) cleanup();
    dispose();
  };
}

/**
 * Watch a specific signal and run callback when it changes
 *
 * @param source - Signal to watch
 * @param callback - Callback with (newValue, oldValue)
 * @returns Dispose function
 *
 * @example
 * ```typescript
 * const roomId = signal('general');
 *
 * watch(roomId, (newRoom, oldRoom) => {
 *   console.log(`Room changed from ${oldRoom} to ${newRoom}`);
 * });
 * ```
 */
export function watch<T>(
  source: { readonly value: T },
  callback: (newValue: T, oldValue: T | undefined) => void
): Dispose {
  let oldValue: T | undefined;
  let isFirst = true;

  return effect(() => {
    const newValue = source.value;
    if (!isFirst) {
      callback(newValue, oldValue);
    }
    isFirst = false;
    oldValue = newValue;
  });
}

/**
 * Shallow equality check for arrays
 */
export function shallowArrayEquals<T>(a: T[], b: T[]): boolean {
  if (a.length !== b.length) return false;
  for (let i = 0; i < a.length; i++) {
    if (a[i] !== b[i]) return false;
  }
  return true;
}

/**
 * Shallow equality check for objects
 */
export function shallowObjectEquals<T extends object>(a: T, b: T): boolean {
  const keysA = Object.keys(a) as (keyof T)[];
  const keysB = Object.keys(b) as (keyof T)[];
  if (keysA.length !== keysB.length) return false;
  for (const key of keysA) {
    if (a[key] !== b[key]) return false;
  }
  return true;
}
