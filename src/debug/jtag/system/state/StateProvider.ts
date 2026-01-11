/**
 * StateProvider - Hierarchical state management like React Context
 *
 * Pattern: AppState → PageState → WidgetState → ControlState
 * Each level can override the level above it.
 *
 * This provides useState-like simplicity:
 * - State changes trigger effects automatically
 * - Effects only run when their dependencies change
 * - No imperative method calls needed
 *
 * @example
 * ```typescript
 * // Create a state provider at widget level
 * const widgetState = createStateProvider<ChatWidgetState>({
 *   roomId: null,
 *   messages: [],
 *   isLoading: false
 * });
 *
 * // Subscribe to state changes (like useEffect)
 * widgetState.effect(
 *   (state) => state.roomId,  // Dependencies
 *   async (roomId) => {
 *     // Effect runs when roomId changes
 *     widgetState.set('isLoading', true);
 *     const messages = await loadMessages(roomId);
 *     widgetState.batch({ messages, isLoading: false });
 *   }
 * );
 *
 * // Update state (like setState)
 * widgetState.set('roomId', 'general');  // Triggers the effect above
 * ```
 */

import { signal, effect, batch, computed } from '@preact/signals-core';
import type { Signal, ReadonlySignal } from '@preact/signals-core';

/**
 * Cleanup function returned by effects
 */
export type Dispose = () => void;

/**
 * Selector function to pick dependencies from state
 */
export type Selector<T, S> = (state: T) => S;

/**
 * Effect handler that runs when dependencies change
 */
export type EffectHandler<S> = (deps: S, prevDeps: S | undefined) => void | Promise<void> | Dispose;

/**
 * State provider interface - hierarchical state with React-like effects
 */
export interface IStateProvider<T extends object> {
  /** Get current state snapshot */
  readonly state: T;

  /** Get signal for a specific key (for fine-grained subscriptions) */
  getSignal<K extends keyof T>(key: K): Signal<T[K]>;

  /** Get computed value (memoized derived state) */
  computed<R>(selector: Selector<T, R>): ReadonlySignal<R>;

  /** Set a single value (triggers effects if dependencies changed) */
  set<K extends keyof T>(key: K, value: T[K]): void;

  /** Batch multiple updates (single notification for all changes) */
  batch(updates: Partial<T>): void;

  /** Create an effect that runs when selected dependencies change */
  effect<S>(selector: Selector<T, S>, handler: EffectHandler<S>): Dispose;

  /** Create a child provider that inherits and can override this state */
  createChild<C extends object>(childDefaults: C): IStateProvider<T & C>;

  /** Dispose all effects and clean up */
  dispose(): void;
}

/**
 * Create a state provider with hierarchical inheritance
 */
export function createStateProvider<T extends object>(
  initialState: T,
  parent?: IStateProvider<any>,
  options: { debug?: boolean; name?: string } = {}
): IStateProvider<T> {
  const { debug = false, name = 'StateProvider' } = options;

  // Create signals for each property
  const signals = new Map<keyof T, Signal<T[keyof T]>>();
  for (const key of Object.keys(initialState) as (keyof T)[]) {
    const initialValue = parent?.state[key] ?? initialState[key];
    signals.set(key, signal(initialValue));
  }

  // Track active effects for cleanup
  const activeEffects: Dispose[] = [];

  const provider: IStateProvider<T> = {
    get state(): T {
      const result = {} as T;
      for (const [key, sig] of signals) {
        result[key] = sig.value;
      }
      // Merge with parent state (parent values are defaults)
      if (parent) {
        return { ...parent.state, ...result };
      }
      return result;
    },

    getSignal<K extends keyof T>(key: K): Signal<T[K]> {
      let sig = signals.get(key) as Signal<T[K]> | undefined;
      if (!sig) {
        // Inherit from parent if exists
        const parentValue = parent?.state[key];
        sig = signal(parentValue ?? (undefined as T[K]));
        signals.set(key, sig as Signal<T[keyof T]>);
      }
      return sig;
    },

    computed<R>(selector: Selector<T, R>): ReadonlySignal<R> {
      return computed(() => selector(provider.state));
    },

    set<K extends keyof T>(key: K, value: T[K]): void {
      const sig = signals.get(key);
      if (sig) {
        const prev = sig.value;
        if (prev !== value) {
          debug && console.log(`🔄 ${name}.${String(key)}:`, prev, '→', value);
          sig.value = value;
        }
      } else {
        // Create new signal for this key
        debug && console.log(`🔄 ${name}.${String(key)}: (new)`, value);
        signals.set(key, signal(value) as Signal<T[keyof T]>);
      }
    },

    batch(updates: Partial<T>): void {
      batch(() => {
        for (const [key, value] of Object.entries(updates) as [keyof T, T[keyof T]][]) {
          if (value !== undefined) {
            provider.set(key, value);
          }
        }
      });
      debug && console.log(`🔄 ${name}.batch:`, Object.keys(updates));
    },

    effect<S>(selector: Selector<T, S>, handler: EffectHandler<S>): Dispose {
      let prevDeps: S | undefined;
      let cleanup: Dispose | void;

      const dispose = effect(() => {
        const deps = selector(provider.state);

        // Run cleanup from previous effect
        if (cleanup) {
          cleanup();
          cleanup = undefined;
        }

        // Only run handler if dependencies changed
        if (prevDeps === undefined || !shallowEqual(deps, prevDeps)) {
          debug && console.log(`⚡ ${name} effect triggered:`, { prev: prevDeps, next: deps });
          const result = handler(deps, prevDeps);

          // Handle async handlers and cleanup functions
          if (result instanceof Promise) {
            // Async effect - no cleanup
          } else if (typeof result === 'function') {
            cleanup = result;
          }

          prevDeps = deps;
        }
      });

      activeEffects.push(dispose);
      return dispose;
    },

    createChild<C extends object>(childDefaults: C): IStateProvider<T & C> {
      return createStateProvider<T & C>(
        { ...provider.state, ...childDefaults } as T & C,
        provider as IStateProvider<any>,
        { debug, name: `${name}.child` }
      );
    },

    dispose(): void {
      debug && console.log(`🧹 ${name}: Disposing ${activeEffects.length} effects`);
      activeEffects.forEach(d => d());
      activeEffects.length = 0;
    }
  };

  return provider;
}

/**
 * Shallow equality check for effect dependency comparison
 */
function shallowEqual<T>(a: T, b: T): boolean {
  if (a === b) return true;
  if (a === null || b === null) return false;
  if (typeof a !== 'object' || typeof b !== 'object') return false;

  if (Array.isArray(a) && Array.isArray(b)) {
    if (a.length !== b.length) return false;
    return a.every((v, i) => v === b[i]);
  }

  const keysA = Object.keys(a) as (keyof T)[];
  const keysB = Object.keys(b) as (keyof T)[];
  if (keysA.length !== keysB.length) return false;

  return keysA.every(key => a[key] === b[key]);
}

// ============================================================================
// CONVENIENCE HOOKS (React-like API)
// ============================================================================

/**
 * Create a simple signal (like useState)
 */
export function useState<T>(initialValue: T): [() => T, (value: T) => void] {
  const sig = signal(initialValue);
  return [
    () => sig.value,
    (value: T) => { sig.value = value; }
  ];
}

/**
 * Create an effect that runs when signals change (like useEffect)
 */
export function useEffect(callback: () => void | Dispose, deps?: ReadonlySignal<any>[]): Dispose {
  if (!deps || deps.length === 0) {
    // Run once on mount
    const cleanup = callback();
    return () => { if (cleanup) cleanup(); };
  }

  let prevValues: any[] | undefined;
  let cleanup: Dispose | void;

  return effect(() => {
    const currentValues = deps.map(d => d.value);

    // Run cleanup from previous
    if (cleanup) {
      cleanup();
      cleanup = undefined;
    }

    // Check if deps changed
    if (!prevValues || !currentValues.every((v, i) => v === prevValues![i])) {
      cleanup = callback();
      prevValues = currentValues;
    }
  });
}

/**
 * Create a memoized value (like useMemo)
 */
export function useMemo<T>(compute: () => T, deps: ReadonlySignal<any>[]): ReadonlySignal<T> {
  return computed(() => {
    // Touch all deps to track them
    deps.forEach(d => d.value);
    return compute();
  });
}
