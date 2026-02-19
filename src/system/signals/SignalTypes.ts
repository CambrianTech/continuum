/**
 * Signal Types - Type definitions for reactive state management
 *
 * Provides React-like memoization for widgets without React.
 * Based on @preact/signals-core for minimal overhead.
 */

import type { Signal, ReadonlySignal } from '@preact/signals-core';

/**
 * Cleanup function returned by effects and subscriptions
 */
export type Dispose = () => void;

/**
 * A reactive signal that can be read and written
 */
export type WritableSignal<T> = Signal<T>;

/**
 * A computed signal that derives from other signals (read-only)
 */
export type ComputedSignal<T> = ReadonlySignal<T>;

/**
 * Effect callback that runs when dependencies change
 */
export type EffectCallback = () => void | Dispose;

/**
 * Widget state slice - each widget gets a typed state object
 */
export interface WidgetSignalState<T extends object> {
  /** Get current snapshot of all state */
  readonly state: T;

  /** Get a specific signal for fine-grained reactivity */
  getSignal<K extends keyof T>(key: K): WritableSignal<T[K]>;

  /** Update a single property (triggers effects) */
  set<K extends keyof T>(key: K, value: T[K]): void;

  /** Batch multiple updates (single re-render) */
  batch(updates: Partial<T>): void;

  /** Subscribe to all changes (returns cleanup) */
  subscribe(callback: (state: T) => void): Dispose;

  /** Clean up all signals and effects */
  dispose(): void;
}

/**
 * Options for creating a widget signal store
 */
export interface WidgetSignalOptions {
  /** Enable debug logging */
  debug?: boolean;
  /** Widget name for logging */
  widgetName?: string;
}

/**
 * Comparison function for memoization
 */
export type EqualityFn<T> = (prev: T, next: T) => boolean;

/**
 * Memoized computation result
 */
export interface MemoizedValue<T> {
  readonly value: T;
  readonly isStale: boolean;
  invalidate(): void;
}
