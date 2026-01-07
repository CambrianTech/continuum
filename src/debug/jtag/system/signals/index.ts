/**
 * Signals - Reactive state management for widgets
 *
 * React-like memoization without React:
 * - signal() - Observable value that triggers effects when changed
 * - computed()/memo() - Derived value that caches until deps change
 * - effect()/watchEffect() - Side effect that runs on dependency changes
 * - watch() - Watch specific signal with old/new values
 * - createWidgetSignals() - Create full widget state store
 *
 * @example
 * ```typescript
 * import { signal, effect, createWidgetSignals } from '@system/signals';
 *
 * // Simple signal
 * const count = signal(0);
 * effect(() => console.log(count.value)); // Logs on change
 * count.value = 1;
 *
 * // Widget state store
 * const store = createWidgetSignals({ roomId: null, messages: [] });
 * store.set('roomId', 'general');
 * ```
 */

export * from './SignalTypes';
export * from './WidgetSignals';
