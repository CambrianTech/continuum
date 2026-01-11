/**
 * WidgetState - Simple state management using @preact/signals-core
 *
 * This is the MINIMAL state management pattern for widgets.
 * Uses signals directly - no custom wrappers needed.
 *
 * Pattern:
 * 1. Create signals for state
 * 2. Create effects that run when signals change
 * 3. Effects handle side effects (loading data, updating DOM)
 * 4. That's it.
 *
 * @example
 * ```typescript
 * class MyChatWidget extends BaseWidget {
 *   // State (signals)
 *   private roomId = signal<string | null>(null);
 *   private messages = signal<Message[]>([]);
 *   private isLoading = signal(false);
 *
 *   async onWidgetInitialize() {
 *     // Effect: Load messages when roomId changes
 *     effect(() => {
 *       const room = this.roomId.value;
 *       if (room) {
 *         this.loadMessages(room);
 *       }
 *     });
 *
 *     // Effect: Update DOM when messages change
 *     effect(() => {
 *       this.renderMessages(this.messages.value);
 *     });
 *   }
 *
 *   // Called by parent when room changes
 *   setRoom(roomId: string) {
 *     this.roomId.value = roomId;  // Triggers effect above
 *   }
 *
 *   private async loadMessages(roomId: string) {
 *     this.isLoading.value = true;
 *     const msgs = await fetchMessages(roomId);
 *     batch(() => {
 *       this.messages.value = msgs;
 *       this.isLoading.value = false;
 *     });
 *   }
 * }
 * ```
 */

import { signal, effect, batch, computed } from '@preact/signals-core';
import type { Signal, ReadonlySignal } from '@preact/signals-core';

// Re-export everything from @preact/signals-core for convenience
export { signal, effect, batch, computed };
export type { Signal, ReadonlySignal };

/**
 * Cleanup function returned by effects
 */
export type Dispose = () => void;

/**
 * Create multiple signals at once from an initial state object
 *
 * @example
 * ```typescript
 * const { roomId, messages, isLoading } = createSignals({
 *   roomId: null as string | null,
 *   messages: [] as Message[],
 *   isLoading: false
 * });
 *
 * // Use like regular signals
 * roomId.value = 'general';
 * effect(() => console.log(messages.value.length));
 * ```
 */
export function createSignals<T extends Record<string, unknown>>(
  initialState: T
): { [K in keyof T]: Signal<T[K]> } {
  const signals = {} as { [K in keyof T]: Signal<T[K]> };

  for (const key of Object.keys(initialState) as (keyof T)[]) {
    signals[key] = signal(initialState[key]) as Signal<T[keyof T]>;
  }

  return signals;
}

/**
 * Create an effect that only runs when specific values change
 * (like React's useEffect with deps array)
 *
 * @example
 * ```typescript
 * // Only runs when roomId.value changes
 * watchEffect(
 *   () => roomId.value,
 *   (newRoomId, oldRoomId) => {
 *     console.log(`Room changed from ${oldRoomId} to ${newRoomId}`);
 *     loadMessages(newRoomId);
 *   }
 * );
 * ```
 */
export function watchEffect<T>(
  getDeps: () => T,
  handler: (newValue: T, oldValue: T | undefined) => void | Dispose
): Dispose {
  let oldValue: T | undefined;
  let cleanup: Dispose | void;

  return effect(() => {
    const newValue = getDeps();

    // Run cleanup from previous effect
    if (cleanup) {
      cleanup();
      cleanup = undefined;
    }

    // Only run if value actually changed
    if (oldValue === undefined || !Object.is(newValue, oldValue)) {
      cleanup = handler(newValue, oldValue);
      oldValue = newValue;
    }
  });
}

/**
 * Create a derived/computed signal (memoized)
 *
 * @example
 * ```typescript
 * const messages = signal<Message[]>([]);
 * const unreadCount = derived(() => messages.value.filter(m => !m.read).length);
 * ```
 */
export const derived = computed;

/**
 * Get all signal values as a plain object (for debugging/serialization)
 */
export function getState<T extends Record<string, Signal<unknown>>>(
  signals: T
): { [K in keyof T]: T[K] extends Signal<infer V> ? V : never } {
  const result = {} as { [K in keyof T]: T[K] extends Signal<infer V> ? V : never };

  for (const key of Object.keys(signals) as (keyof T)[]) {
    result[key] = signals[key].value as any;
  }

  return result;
}

/**
 * Batch update multiple signals at once
 * (single notification for all changes)
 *
 * @example
 * ```typescript
 * batchUpdate({ isLoading, messages }, { isLoading: false, messages: newMsgs });
 * ```
 */
export function batchUpdate<T extends Record<string, Signal<unknown>>>(
  signals: T,
  updates: Partial<{ [K in keyof T]: T[K] extends Signal<infer V> ? V : never }>
): void {
  batch(() => {
    for (const [key, value] of Object.entries(updates)) {
      if (key in signals && value !== undefined) {
        (signals[key as keyof T] as Signal<unknown>).value = value;
      }
    }
  });
}
